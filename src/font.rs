//! Shared font data model and BDF parsing.
//!
//! The editor accepts BDF fonts (`.bdf`, optionally gzip-compressed). Parsing
//! produces a JSON-friendly [`Font`] type shared by the server and browser.

use anyhow::{bail, Context, Result};
use flate2::read::GzDecoder;
use serde::Serialize;
use std::collections::HashMap;
use std::io::Read;

#[derive(Clone, Debug, Serialize)]
pub struct Font {
    pub glyphs: HashMap<char, Glyph>,
    pub default_char: Option<char>,
    pub max_width: i32,
    pub max_height: i32,
    /// Distance from the top of the line to the baseline.
    pub ascent: i32,
    /// Distance from the baseline to the bottom of the line.
    pub descent: i32,
}

#[derive(Clone, Debug, Serialize)]
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

/// Parse a BDF font from raw bytes.
///
/// Gzip-compressed input is decompressed transparently, so `.bdf.gz` works as
/// well as its uncompressed counterpart.
pub fn parse_font_bytes(data: &[u8]) -> Result<Font> {
    let data = decompress(data)?;

    if looks_like_bdf(&data) {
        return crate::bdf::parse_bdf(&data);
    }

    bail!("not a recognised font file (expected a BDF font)")
}

fn decompress(data: &[u8]) -> Result<Vec<u8>> {
    if !data.starts_with(&[0x1f, 0x8b]) {
        return Ok(data.to_vec());
    }

    let mut decoder = GzDecoder::new(data);
    let mut decoded = Vec::new();
    decoder
        .read_to_end(&mut decoded)
        .context("decompressing font")?;
    Ok(decoded)
}

fn looks_like_bdf(data: &[u8]) -> bool {
    let head = &data[..data.len().min(64 * 1024)];
    head.windows(b"STARTFONT".len())
        .any(|window| window == b"STARTFONT")
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_BDF: &str = "\
STARTFONT 2.1
FONT -misc-sample-medium-r-normal--4-40-72-72-C-80-iso10646-1
FONTBOUNDINGBOX 8 4 0 -1
FONT_ASCENT 3
FONT_DESCENT 1
CHARS 1
STARTCHAR A
ENCODING 65
DWIDTH 8 0
BBX 3 3 0 0
BITMAP
80
40
20
ENDCHAR
ENDFONT
";

    #[test]
    fn detects_bdf() {
        let font = parse_font_bytes(SAMPLE_BDF.as_bytes()).unwrap();
        assert!(font.glyphs.contains_key(&'A'));
        assert_eq!(font.ascent, 3);
    }

    #[test]
    fn rejects_unknown_input() {
        assert!(parse_font_bytes(b"not a font").is_err());
    }
}
