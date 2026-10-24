use anyhow::{anyhow, Result};
use byteorder::{LittleEndian, ReadBytesExt};
use flate2::read::GzDecoder;
use std::collections::HashMap;
use std::fs::File;
use std::io::{Cursor, Read};

#[derive(Clone, Debug)]
pub struct PcfFont {
    pub glyphs: HashMap<char, Glyph>,
    pub default_char: Option<char>,
    pub max_width: i32,
    pub max_height: i32,
}

#[derive(Clone, Debug)]
pub struct Glyph {
    pub width: u32,
    pub height: u32,
    pub bitmap: Vec<Vec<bool>>, // height x width
    pub left_bearing: i32,
    pub right_bearing: i32,
    pub ascent: i32,
    pub descent: i32,
    pub character_width: i32,
}

#[derive(Clone, Debug)]
struct PcfTable {
    _type: u32,
    format: u32,
    size: u32,
    offset: u32,
}

// PCF constants
const PCF_PROPERTIES: u32 = (1 << 0);
const PCF_ACCELERATORS: u32 = (1 << 1);
const PCF_METRICS: u32 = (1 << 2);
const PCF_BITMAPS: u32 = (1 << 3);
const PCF_INK_METRICS: u32 = (1 << 4);
const PCF_BDF_ENCODINGS: u32 = (1 << 5);
const PCF_SWIDTHS: u32 = (1 << 6);
const PCF_GLYPH_NAMES: u32 = (1 << 7);
const PCF_BDF_ACCELERATORS: u32 = (1 << 8);

const PCF_DEFAULT_FORMAT: u32 = 0x00000000;
const PCF_INKBOUNDS: u32 = 0x00000200;
const PCF_ACCEL_W_INKBOUNDS: u32 = 0x00000100;
const PCF_COMPRESSED_METRICS: u32 = 0x00000100;
const PCF_BYTE_MASK: u32 = 0x00000001;
const PCF_BIT_MASK: u32 = 0x00000002;
const PCF_GLYPH_PAD_MASK: u32 = 0x00000003;

pub fn load_pcf(path: &str) -> Result<PcfFont> {
    let mut file = File::open(path)?;
    let mut data = Vec::new();
    file.read_to_end(&mut data)?;

    // Check if gzipped
    if data.starts_with(&[0x1f, 0x8b]) {
        let decoded_data = {
            let mut decoder = GzDecoder::new(&data[..]);
            let mut tmp = Vec::new();
            decoder.read_to_end(&mut tmp)?;
            tmp
        };
        data = decoded_data;
    }

    let mut cursor = Cursor::new(data);

    // Check header
    let magic = cursor.read_u32::<LittleEndian>()?;
    if magic != 0x70636601 {
        return Err(anyhow!("Not a valid PCF font file"));
    }

    let table_count = cursor.read_u32::<LittleEndian>()?;
    let mut tables: Vec<PcfTable> = Vec::new();

    for _ in 0..table_count {
        let ttype = cursor.read_u32::<LittleEndian>()?;
        let format = cursor.read_u32::<LittleEndian>()?;
        let size = cursor.read_u32::<LittleEndian>()?;
        let offset = cursor.read_u32::<LittleEndian>()?;
        tables.push(PcfTable {
            _type: ttype,
            format,
            size,
            offset,
        });
    }

    // Find required tables
    let bitmaps_table = tables
        .iter()
        .find(|t| t._type == PCF_BITMAPS)
        .ok_or_else(|| anyhow!("No bitmaps table"))?;
    let encodings_table = tables
        .iter()
        .find(|t| t._type == PCF_BDF_ENCODINGS)
        .ok_or_else(|| anyhow!("No encodings table"))?;
    let metrics_table = tables
        .iter()
        .find(|t| t._type == PCF_METRICS)
        .ok_or_else(|| anyhow!("No metrics table"))?;

    // Parse encodings
    let mut enc_cursor = Cursor::new(cursor.get_ref().clone());
    enc_cursor.set_position(encodings_table.offset as u64);
    let enc_format = enc_cursor.read_u32::<LittleEndian>()?;
    let first_col = enc_cursor.read_u16::<LittleEndian>()? as u16;
    let last_col = enc_cursor.read_u16::<LittleEndian>()? as u16;
    let first_row = enc_cursor.read_u16::<LittleEndian>()? as u16;
    let last_row = enc_cursor.read_u16::<LittleEndian>()? as u16;
    let _default_char = enc_cursor.read_u16::<LittleEndian>()?;

    let mut encoding_map: Vec<i16> = vec![
        -1;
        ((last_col as usize) - (first_col as usize) + 1)
            * ((last_row as usize) - (first_row as usize) + 1)
    ];
    for i in 0..encoding_map.len() {
        encoding_map[i] = enc_cursor.read_i16::<LittleEndian>()?;
    }

    // Parse metrics
    let mut met_cursor = Cursor::new(cursor.get_ref().clone());
    met_cursor.set_position(metrics_table.offset as u64);
    let met_format = met_cursor.read_u32::<LittleEndian>()?;
    let metrics_count = if met_format & PCF_COMPRESSED_METRICS != 0 {
        // compressed
        met_cursor.read_u16::<LittleEndian>()? as u32
    } else {
        met_cursor.read_u32::<LittleEndian>()?
    };

    #[derive(Clone)]
    struct Metric {
        left_bearing: i16,
        right_bearing: i16,
        character_width: i16,
        ascent: i16,
        descent: i16,
        attributes: u16,
    }

    let mut metrics: Vec<Metric> = Vec::new();
    if met_format & PCF_COMPRESSED_METRICS != 0 {
        for _ in 0..metrics_count {
            let left_bearing = (met_cursor.read_u8()? as i16) - 0x80;
            let right_bearing = (met_cursor.read_u8()? as i16) - 0x80;
            let character_width = (met_cursor.read_u8()? as i16) - 0x80;
            let ascent = (met_cursor.read_u8()? as i16) - 0x80;
            let descent = (met_cursor.read_u8()? as i16) - 0x80;
            metrics.push(Metric {
                left_bearing,
                right_bearing,
                character_width,
                ascent,
                descent,
                attributes: 0,
            });
        }
    } else {
        for _ in 0..metrics_count {
            let left_bearing = met_cursor.read_i16::<LittleEndian>()?;
            let right_bearing = met_cursor.read_i16::<LittleEndian>()?;
            let character_width = met_cursor.read_i16::<LittleEndian>()?;
            let ascent = met_cursor.read_i16::<LittleEndian>()?;
            let descent = met_cursor.read_i16::<LittleEndian>()?;
            let attributes = met_cursor.read_u16::<LittleEndian>()?;
            metrics.push(Metric {
                left_bearing,
                right_bearing,
                character_width,
                ascent,
                descent,
                attributes,
            });
        }
    }

    // Parse bitmaps
    let mut bmp_cursor = Cursor::new(cursor.get_ref().clone());
    bmp_cursor.set_position(bitmaps_table.offset as u64);
    let bmp_format = bmp_cursor.read_u32::<LittleEndian>()?;
    let _glyph_count = bmp_cursor.read_u32::<LittleEndian>()?;
    let mut offsets: Vec<u32> = Vec::new();
    for _ in 0..metrics_count {
        offsets.push(bmp_cursor.read_u32::<LittleEndian>()?);
    }
    let mut bitmap_sizes: Vec<u32> = Vec::new();
    for _ in 0..4 {
        bitmap_sizes.push(bmp_cursor.read_u32::<LittleEndian>()?);
    }

    let glyph_pad = (bmp_format & PCF_GLYPH_PAD_MASK) as usize;
    let byte_swap = (bmp_format & PCF_BYTE_MASK) != 0;
    let bit_swap = (bmp_format & PCF_BIT_MASK) != 0;

    let mut glyph_bitmaps: Vec<Vec<u8>> = Vec::new();
    for i in 0..metrics_count as usize {
        let metric = &metrics[i];
        let width = if metric.character_width > 0 {
            metric.character_width as u32
        } else {
            0
        };
        let height = (metric.ascent - metric.descent) as u32;
        let bytes_per_row = ((width as usize) + 7) / 8;
        let row_size = ((bytes_per_row + glyph_pad - 1) / glyph_pad) * glyph_pad;
        let mut bitmap_data = vec![0u8; row_size * height as usize];
        for b in 0..bitmap_data.len() {
            bitmap_data[b] = bmp_cursor.read_u8()?;
        }
        glyph_bitmaps.push(bitmap_data);
    }

    // Build font
    let mut glyphs = HashMap::new();
    let mut max_width = 0i32;
    let mut max_height = 0i32;

    // Map encodings to chars
    for row in 0..((last_row as usize) - (first_row as usize) + 1) {
        for col in 0..((last_col as usize) - (first_col as usize) + 1) {
            let idx = row * ((last_col as usize) - (first_col as usize) + 1) + col;
            let glyph_idx = encoding_map[idx];
            if glyph_idx >= 0 && (glyph_idx as usize) < metrics.len() {
                let ch =
                    ((first_row as u32 + row as u32) << 8 | (first_col as u32 + col as u32)) as u32;
                // Try to convert to char
                let c = std::char::from_u32(ch).unwrap_or_else(|| (ch as u8) as char);
                let metric = &metrics[glyph_idx as usize];
                let bitmap_data = &glyph_bitmaps[glyph_idx as usize];
                let height = ((metric.ascent - metric.descent) as u32).max(0);
                let width = if metric.character_width > 0 {
                    metric.character_width as u32
                } else {
                    0
                };

                let bytes_per_row = ((width as usize) + 7) / 8;
                let row_size = if glyph_pad == 0 {
                    bytes_per_row
                } else {
                    ((bytes_per_row + glyph_pad - 1) / glyph_pad) * glyph_pad
                };

                let mut bitmap = vec![vec![false; width as usize]; height as usize];
                for r in 0..height as usize {
                    for col in 0..width as usize {
                        let byte_idx = r * row_size + (col / 8);
                        if byte_idx < bitmap_data.len() {
                            let bit = if bit_swap { (col % 8) } else { 7 - (col % 8) };
                            if (bitmap_data[byte_idx] & (1 << bit)) != 0 {
                                bitmap[r][col] = true;
                            }
                        }
                    }
                }

                max_width = max_width.max(width as i32);
                max_height = max_height.max(height as i32);

                glyphs.insert(
                    c,
                    Glyph {
                        width,
                        height,
                        bitmap,
                        left_bearing: metric.left_bearing as i32,
                        right_bearing: metric.right_bearing as i32,
                        ascent: metric.ascent as i32,
                        descent: metric.descent as i32,
                        character_width: metric.character_width as i32,
                    },
                );
            }
        }
    }

    Ok(PcfFont {
        glyphs,
        default_char: None,
        max_width,
        max_height,
    })
}

pub fn render_text(
    font: &PcfFont,
    text: &str,
    start_x: i32,
    start_y: i32,
    color: u8,
    pixels: &mut [[u8; 96]; 16],
) {
    let mut x = start_x;
    for ch in text.chars() {
        if let Some(glyph) = font.glyphs.get(&ch) {
            let glyph_y =
                start_y + font.glyphs.values().next().map(|g| g.ascent).unwrap_or(0) - glyph.ascent;
            for r in 0..glyph.height as i32 {
                for c in 0..glyph.width as i32 {
                    let py = glyph_y + r;
                    let px = x + glyph.left_bearing + c;
                    if py >= 0 && py < 16 && px >= 0 && px < 96 {
                        if (r as usize) < glyph.bitmap.len()
                            && (c as usize) < glyph.bitmap[r as usize].len()
                        {
                            if glyph.bitmap[r as usize][c as usize] {
                                pixels[py as usize][px as usize] = color & 0x0F;
                            }
                        }
                    }
                }
            }
            x += glyph.character_width;
        } else if let Some(default) = font.default_char {
            if let Some(glyph) = font.glyphs.get(&default) {
                x += glyph.character_width;
            } else {
                x += 4;
            }
        } else {
            x += 4;
        }
    }
}

pub fn get_text_width(font: &PcfFont, text: &str) -> i32 {
    let mut width = 0;
    for ch in text.chars() {
        if let Some(glyph) = font.glyphs.get(&ch) {
            width += glyph.character_width;
        } else {
            width += 4;
        }
    }
    width
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_text_width() {
        let font = PcfFont {
            glyphs: HashMap::new(),
            default_char: None,
            max_width: 8,
            max_height: 16,
        };
        assert_eq!(get_text_width(&font, "ABC"), 12);
    }
}
