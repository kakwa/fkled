use crate::font::{Font, Glyph};
use crate::palette;
use anyhow::{anyhow, Context, Result};
use flate2::read::GzDecoder;
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;

/// Load a BDF font from disk. Transparently handles gzip-compressed files.
pub fn load_bdf(path: &str) -> Result<Font> {
    let mut file = File::open(path).with_context(|| format!("opening font {path}"))?;
    let mut data = Vec::new();
    file.read_to_end(&mut data)?;

    // Check if gzipped (.bdf.gz)
    if data.starts_with(&[0x1f, 0x8b]) {
        let mut decoder = GzDecoder::new(&data[..]);
        let mut decoded = Vec::new();
        decoder
            .read_to_end(&mut decoded)
            .with_context(|| format!("decompressing font {path}"))?;
        data = decoded;
    }

    parse_bdf(&data).with_context(|| format!("parsing BDF font {path}"))
}

pub fn parse_bdf(data: &[u8]) -> Result<Font> {
    let text = String::from_utf8_lossy(data);

    let mut glyphs: HashMap<char, Glyph> = HashMap::new();
    let mut default_char = None;
    let mut font_ascent = None;
    let mut font_descent = None;
    let mut font_bbox = None;
    let mut max_width = 0;
    let mut max_height = 0;

    let mut saw_startfont = false;
    let mut in_char = false;
    let mut in_bitmap = false;
    let mut bitmap_rows: Vec<String> = Vec::new();
    let mut encoding: Option<i64> = None;
    let mut dwidth: Option<i32> = None;
    let mut bbx: Option<(i32, i32, i32, i32)> = None;

    for raw_line in text.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }

        if in_bitmap {
            if line.eq_ignore_ascii_case("ENDCHAR") {
                if let Some(glyph) = finish_glyph(encoding, dwidth, bbx, &bitmap_rows) {
                    max_width = max_width.max(glyph.0.width as i32);
                    max_height = max_height.max(glyph.0.height as i32);
                    glyphs.insert(glyph.1, glyph.0);
                }
                in_bitmap = false;
                in_char = false;
                reset_char(&mut encoding, &mut dwidth, &mut bbx, &mut bitmap_rows);
            } else {
                bitmap_rows.push(line.to_string());
            }
            continue;
        }

        let mut parts = line.split_whitespace();
        let key = parts.next().unwrap_or("");

        match key {
            "STARTFONT" => saw_startfont = true,
            "FONTBOUNDINGBOX" => {
                let values = parse_ints(&mut parts, 4);
                if let [w, h, x, y] = values[..] {
                    font_bbox = Some((w, h, x, y));
                }
            }
            "FONT_ASCENT" => font_ascent = parts.next().and_then(|v| v.parse().ok()),
            "FONT_DESCENT" => font_descent = parts.next().and_then(|v| v.parse().ok()),
            "DEFAULT_CHAR" => {
                default_char = parts
                    .next()
                    .and_then(|v| v.parse::<i64>().ok())
                    .and_then(|v| u32::try_from(v).ok())
                    .and_then(char::from_u32);
            }
            "STARTCHAR" => {
                in_char = true;
                reset_char(&mut encoding, &mut dwidth, &mut bbx, &mut bitmap_rows);
            }
            "ENCODING" => {
                // Some fonts use "ENCODING <code> <non-standard code>".
                encoding = parts.next().and_then(|v| v.parse().ok());
            }
            "DWIDTH" => {
                dwidth = parts.next().and_then(|v| v.parse().ok());
            }
            "BBX" => {
                let values = parse_ints(&mut parts, 4);
                if let [w, h, x, y] = values[..] {
                    bbx = Some((w, h, x, y));
                }
            }
            "BITMAP" => {
                if in_char {
                    in_bitmap = true;
                    bitmap_rows.clear();
                }
            }
            "ENDCHAR" => {
                // Glyph with no BITMAP (e.g. zero-size glyph).
                if in_char {
                    if let Some(glyph) = finish_glyph(encoding, dwidth, bbx, &bitmap_rows) {
                        max_width = max_width.max(glyph.0.width as i32);
                        max_height = max_height.max(glyph.0.height as i32);
                        glyphs.insert(glyph.1, glyph.0);
                    }
                }
                in_char = false;
                reset_char(&mut encoding, &mut dwidth, &mut bbx, &mut bitmap_rows);
            }
            "ENDFONT" => break,
            _ => {}
        }
    }

    if !saw_startfont && glyphs.is_empty() {
        return Err(anyhow!("not a valid BDF font file"));
    }

    if max_width == 0 {
        max_width = font_bbox.map(|b| b.0).unwrap_or(0);
    }
    if max_height == 0 {
        max_height = font_bbox.map(|b| b.1).unwrap_or(0);
    }

    let ascent = font_ascent.unwrap_or_else(|| {
        glyphs
            .values()
            .map(|g| g.ascent)
            .max()
            .unwrap_or(max_height)
    });
    let descent = font_descent.unwrap_or_else(|| {
        glyphs
            .values()
            .map(|g| (-g.descent).max(0))
            .max()
            .unwrap_or(0)
    });

    Ok(Font {
        glyphs,
        default_char,
        max_width,
        max_height,
        ascent,
        descent,
    })
}

fn reset_char(
    encoding: &mut Option<i64>,
    dwidth: &mut Option<i32>,
    bbx: &mut Option<(i32, i32, i32, i32)>,
    bitmap_rows: &mut Vec<String>,
) {
    *encoding = None;
    *dwidth = None;
    *bbx = None;
    bitmap_rows.clear();
}

fn parse_ints<'a>(parts: &mut impl Iterator<Item = &'a str>, count: usize) -> Vec<i32> {
    let mut values = Vec::with_capacity(count);
    for _ in 0..count {
        match parts.next().and_then(|v| v.parse().ok()) {
            Some(value) => values.push(value),
            None => break,
        }
    }
    values
}

/// Convert a completed BDF character into a `(Glyph, char)` pair.
fn finish_glyph(
    encoding: Option<i64>,
    dwidth: Option<i32>,
    bbx: Option<(i32, i32, i32, i32)>,
    bitmap_rows: &[String],
) -> Option<(Glyph, char)> {
    // Negative encodings mark unencoded glyphs.
    let encoding = encoding?;
    let ch = u32::try_from(encoding).ok().and_then(char::from_u32)?;

    let (bbx_width, bbx_height, x_offset, y_offset) = bbx.unwrap_or((0, 0, 0, 0));
    let width = bbx_width.max(0) as u32;
    let height = bbx_height.max(0) as u32;
    let bytes_per_row = width.div_ceil(8) as usize;

    let mut bitmap = Vec::with_capacity(height as usize);
    for row in 0..height as usize {
        let mut bits = Vec::with_capacity(width as usize);
        let hex: Vec<u8> = bitmap_rows
            .get(row)
            .map(|r| r.bytes().filter(|b| b.is_ascii_hexdigit()).collect())
            .unwrap_or_default();

        for byte_index in 0..bytes_per_row {
            let hi = hex.get(byte_index * 2).copied().map(hex_value).unwrap_or(0);
            let lo = hex
                .get(byte_index * 2 + 1)
                .copied()
                .map(hex_value)
                .unwrap_or(0);
            let byte = (hi << 4) | lo;
            for bit in (0..8).rev() {
                bits.push((byte & (1 << bit)) != 0);
            }
        }
        bits.truncate(width as usize);
        bits.resize(width as usize, false);
        bitmap.push(bits);
    }

    let character_width = dwidth.unwrap_or(width as i32);

    Some((
        Glyph {
            width,
            height,
            bitmap,
            left_bearing: x_offset,
            right_bearing: x_offset + width as i32,
            // Top/bottom of the glyph relative to the baseline.
            ascent: y_offset + height as i32,
            descent: y_offset,
            character_width,
        },
        ch,
    ))
}

fn hex_value(byte: u8) -> u8 {
    match byte {
        b'0'..=b'9' => byte - b'0',
        b'a'..=b'f' => byte - b'a' + 10,
        b'A'..=b'F' => byte - b'A' + 10,
        _ => 0,
    }
}

pub fn render_text(
    font: &Font,
    text: &str,
    start_x: i32,
    start_y: i32,
    color: u8,
    pixels: &mut [[u8; 96]; 16],
) {
    let baseline = start_y + font.ascent;
    let mut x = start_x;

    for ch in text.chars() {
        let glyph = font.glyphs.get(&ch).or_else(|| {
            font.default_char
                .and_then(|default| font.glyphs.get(&default))
        });

        let Some(glyph) = glyph else {
            x += 4;
            continue;
        };

        let glyph_top = baseline - glyph.ascent;
        let glyph_left = x + glyph.left_bearing;

        for (row, bits) in glyph.bitmap.iter().enumerate() {
            let py = glyph_top + row as i32;
            if !(0..16).contains(&py) {
                continue;
            }
            for (col, &on) in bits.iter().enumerate() {
                if !on {
                    continue;
                }
                let px = glyph_left + col as i32;
                if (0..96).contains(&px) {
                    pixels[py as usize][px as usize] = palette::normalize(color);
                }
            }
        }

        x += glyph.character_width;
    }
}

pub fn get_text_width(font: &Font, text: &str) -> i32 {
    let mut width = 0;
    for ch in text.chars() {
        let glyph = font.glyphs.get(&ch).or_else(|| {
            font.default_char
                .and_then(|default| font.glyphs.get(&default))
        });
        width += glyph.map(|g| g.character_width).unwrap_or(4);
    }
    width
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_BDF: &str = "\
STARTFONT 2.1
FONT -misc-sample-medium-r-normal--4-40-72-72-C-80-iso10646-1
FONTBOUNDINGBOX 8 4 0 -1
STARTPROPERTIES 3
FONT_ASCENT 3
FONT_DESCENT 1
DEFAULT_CHAR 63
ENDPROPERTIES
CHARS 3
STARTCHAR A
ENCODING 65
DWIDTH 8 0
BBX 3 3 0 0
BITMAP
80
40
20
ENDCHAR
STARTCHAR question
ENCODING 63
DWIDTH 8 0
BBX 3 3 0 0
BITMAP
E0
40
40
ENDCHAR
STARTCHAR unencoded
ENCODING -1
DWIDTH 8 0
BBX 3 3 0 0
BITMAP
FF
FF
FF
ENDCHAR
ENDFONT
";

    #[test]
    fn parses_bitmap_bits_in_msb_first_order() {
        let font = parse_bdf(SAMPLE_BDF.as_bytes()).unwrap();
        let a = font.glyphs.get(&'A').unwrap();

        assert_eq!(a.width, 3);
        assert_eq!(a.height, 3);
        assert_eq!(a.character_width, 8);
        assert_eq!(a.ascent, 3);
        assert_eq!(a.descent, 0);
        assert_eq!(
            a.bitmap,
            vec![
                vec![true, false, false],
                vec![false, true, false],
                vec![false, false, true],
            ]
        );
    }

    #[test]
    fn parses_font_metadata_and_skips_unencoded_glyphs() {
        let font = parse_bdf(SAMPLE_BDF.as_bytes()).unwrap();

        assert_eq!(font.ascent, 3);
        assert_eq!(font.descent, 1);
        assert_eq!(font.default_char, Some('?'));
        assert_eq!(font.glyphs.len(), 2);
        assert!(font.glyphs.contains_key(&'A'));
        assert!(font.glyphs.contains_key(&'?'));
    }

    #[test]
    fn renders_with_baseline_and_default_glyph() {
        let font = parse_bdf(SAMPLE_BDF.as_bytes()).unwrap();
        let mut pixels = [[0u8; 96]; 16];

        render_text(&font, "A", 1, 2, 7, &mut pixels);

        // baseline = 2 + ascent(3) = 5, so the glyph occupies rows 2..5.
        assert_eq!(pixels[2][1], palette::normalize(7));
        assert_eq!(pixels[3][2], palette::normalize(7));
        assert_eq!(pixels[4][3], palette::normalize(7));

        // Unknown character falls back to DEFAULT_CHAR ('?') and has the same advance.
        let mut fallback = [[0u8; 96]; 16];
        render_text(&font, "Z", 0, 0, 7, &mut fallback);
        assert_eq!(fallback[0][0], palette::normalize(7));
        assert_eq!(get_text_width(&font, "ZZ"), 16);
    }

    #[test]
    fn rejects_non_bdf_input() {
        assert!(parse_bdf(b"not a font").is_err());
    }

    #[test]
    fn loads_gzip_compressed_bdf() {
        use flate2::write::GzEncoder;
        use flate2::Compression;
        use std::io::Write;

        let path = std::env::temp_dir().join(format!("fk-bdf-test-{}.bdf.gz", std::process::id()));
        {
            let file = File::create(&path).unwrap();
            let mut encoder = GzEncoder::new(file, Compression::default());
            encoder.write_all(SAMPLE_BDF.as_bytes()).unwrap();
            encoder.finish().unwrap();
        }

        let font = load_bdf(path.to_str().unwrap()).unwrap();
        let _ = std::fs::remove_file(&path);

        assert_eq!(font.ascent, 3);
        assert!(font.glyphs.contains_key(&'A'));
    }

    /// Parse every BDF in the bundled bitmap-fonts collection and render sample
    /// text with each of them. Skipped when the collection is not checked out.
    #[test]
    fn loads_entire_bitmap_fonts_collection() {
        use std::path::{Path, PathBuf};

        fn collect_bdf(dir: &Path, out: &mut Vec<PathBuf>) {
            let Ok(entries) = std::fs::read_dir(dir) else {
                return;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    collect_bdf(&path, out);
                } else if path
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("bdf"))
                {
                    out.push(path);
                }
            }
        }

        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("bitmap-fonts/bitmap");
        if !root.exists() {
            eprintln!("skipping: {} not found", root.display());
            return;
        }

        let mut fonts = Vec::new();
        collect_bdf(&root, &mut fonts);
        fonts.sort();
        assert!(
            !fonts.is_empty(),
            "no BDF files found in {}",
            root.display()
        );

        let mut glyph_count = 0usize;
        for path in &fonts {
            let font = load_bdf(path.to_str().unwrap())
                .unwrap_or_else(|err| panic!("{}: {err:#}", path.display()));

            assert!(
                !font.glyphs.is_empty(),
                "{}: parsed no glyphs",
                path.display()
            );
            assert!(
                font.max_width > 0 && font.max_height > 0,
                "{}: empty bounding box",
                path.display()
            );

            let mut has_lit_pixel = false;
            for glyph in font.glyphs.values() {
                assert_eq!(
                    glyph.bitmap.len(),
                    glyph.height as usize,
                    "{}",
                    path.display()
                );
                assert!(
                    glyph
                        .bitmap
                        .iter()
                        .all(|row| row.len() == glyph.width as usize),
                    "{}: malformed glyph bitmap",
                    path.display()
                );
                has_lit_pixel |= glyph.bitmap.iter().flatten().any(|&bit| bit);
            }
            assert!(has_lit_pixel, "{}: no glyph pixels", path.display());

            // Every glyph must render without panicking.
            let all_chars: String = font.glyphs.keys().copied().collect();
            let mut pixels = [[0u8; 96]; 16];
            render_text(&font, &all_chars, 0, 0, 7, &mut pixels);
            glyph_count += font.glyphs.len();
        }

        eprintln!(
            "validated {} BDF fonts containing {} glyphs",
            fonts.len(),
            glyph_count
        );
    }
}
