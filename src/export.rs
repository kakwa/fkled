use crate::model::{Animation, Frame};
use anyhow::{bail, Context, Result};
use chrono::{Datelike, Timelike};
use std::fs::File;
use std::io::{Read, Write};

const WIDTH: usize = 96;
const HEIGHT: usize = 16;
const ROW_BYTES: usize = WIDTH / 2; // 48 bytes = 96 pixels at 4 bpp
const FRAME_BYTES: usize = ROW_BYTES * HEIGHT; // 768 = 0x300
const MAX_FRAMES: usize = 32;

const DATA_OFFSET_FIELD: u32 = 0x0A00; // observed header value; not the frame start
const FIRST_FRAME_OFFSET: usize = 0x09A4;
const DESCRIPTOR_TABLE_OFFSET: usize = 0x04AC;
const DESCRIPTOR_SIZE: usize = 0x28;
const DESCRIPTOR_POINTER_BASE: usize = 0x0200;
const FRAME_DATA_BYTES: usize = MAX_FRAMES * FRAME_BYTES; // 0x6000
const FOOTER_OFFSET: usize = FIRST_FRAME_OFFSET + FRAME_DATA_BYTES; // 0x69A4
const FOOTER_BYTES: usize = 0x6C;
const FILE_BYTES: usize = FOOTER_OFFSET + FOOTER_BYTES; // 0x6A10

fn put_u16(buf: &mut [u8], offset: usize, value: u16) {
    buf[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn put_u32(buf: &mut [u8], offset: usize, value: u32) {
    buf[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn get_u16(buf: &[u8], offset: usize) -> Result<u16> {
    let bytes = buf
        .get(offset..offset + 2)
        .with_context(|| format!("truncated u16 at 0x{offset:04X}"))?;
    Ok(u16::from_le_bytes(bytes.try_into().unwrap()))
}

fn get_u32(buf: &[u8], offset: usize) -> Result<u32> {
    let bytes = buf
        .get(offset..offset + 4)
        .with_context(|| format!("truncated u32 at 0x{offset:04X}"))?;
    Ok(u32::from_le_bytes(bytes.try_into().unwrap()))
}

fn write_utf16(buf: &mut [u8], offset: usize, text: &str) {
    for (i, unit) in text.encode_utf16().enumerate() {
        put_u16(buf, offset + i * 2, unit);
    }
}

fn read_utf16_z(buf: &[u8], offset: usize) -> Option<String> {
    let mut units = Vec::new();
    let mut pos = offset;
    loop {
        let unit = get_u16(buf, pos).ok()?;
        if unit == 0 {
            break;
        }
        units.push(unit);
        pos += 2;
        if units.len() > 256 {
            return None;
        }
    }
    String::from_utf16(&units).ok()
}

fn pack_frame(frame: &Frame, out: &mut [u8]) {
    debug_assert_eq!(out.len(), FRAME_BYTES);
    for row in 0..HEIGHT {
        for byte_col in 0..ROW_BYTES {
            let high = frame.pixels[row][byte_col * 2] & 0x0F;
            let low = frame.pixels[row][byte_col * 2 + 1] & 0x0F;
            out[row * ROW_BYTES + byte_col] = (high << 4) | low;
        }
    }
}

fn decode_frame(bytes: &[u8], row_bytes: usize, frame: &mut Frame) {
    let row_bytes = row_bytes.clamp(1, ROW_BYTES);
    for row in 0..HEIGHT {
        for byte_col in 0..row_bytes {
            let Some(&packed) = bytes.get(row * row_bytes + byte_col) else {
                continue;
            };
            frame.pixels[row][byte_col * 2] = packed >> 4;
            frame.pixels[row][byte_col * 2 + 1] = packed & 0x0F;
        }
    }
}

/// True when the file ends with the UTF-16LE string "Animation0", which is the
/// last field of the 0x6C-byte footer block in the observed format.
fn has_footer(data: &[u8]) -> bool {
    if data.len() < FOOTER_BYTES {
        return false;
    }
    let tail = &data[data.len() - 20..];
    let needle: Vec<u8> = "Animation0"
        .encode_utf16()
        .flat_map(|unit| unit.to_le_bytes())
        .collect();
    tail == needle.as_slice()
}

fn pixel_data_limit(data: &[u8]) -> usize {
    if has_footer(data) {
        data.len().saturating_sub(FOOTER_BYTES)
    } else {
        data.len()
    }
}

fn descriptor_frame_offset(data: &[u8], index: usize) -> Option<usize> {
    let descriptor = DESCRIPTOR_TABLE_OFFSET.checked_add(index.checked_mul(DESCRIPTOR_SIZE)?)?;
    let stored = get_u32(data, descriptor + 0x0C).ok()? as usize;
    stored.checked_add(DESCRIPTOR_POINTER_BASE)
}

fn descriptor_geometry(data: &[u8]) -> Option<(usize, usize)> {
    let frame_bytes = get_u32(data, DESCRIPTOR_TABLE_OFFSET + 0x08).ok()? as usize;
    let row_bytes = get_u16(data, DESCRIPTOR_TABLE_OFFSET + 0x04).ok()? as usize;
    if frame_bytes == 0 || frame_bytes > 1 << 20 {
        return None;
    }
    let row_bytes = if row_bytes == 0 || row_bytes > ROW_BYTES {
        ROW_BYTES
    } else {
        row_bytes
    };
    Some((frame_bytes, row_bytes))
}

fn detect_frame_count(data: &[u8]) -> Option<usize> {
    let count = get_u16(data, 0x34A).ok()? as usize;
    if count > 0 {
        return Some(count);
    }

    // Older/damaged files may have a zero count field. Fall back to the
    // descriptor tags, stopping at the first unused/terminator record.
    let mut count = 0;
    for index in 0..MAX_FRAMES {
        let descriptor = DESCRIPTOR_TABLE_OFFSET + index * DESCRIPTOR_SIZE;
        let tag = get_u32(data, descriptor + 0x20).ok()?;
        if tag == 0 {
            break;
        }
        count += 1;
    }
    Some(count)
}

fn write_metadata(buf: &mut [u8]) {
    // 0x200: "Affichage-1" display object header.
    buf[0x200..0x20C].copy_from_slice(&[
        0x55, 0x55, 0x01, 0x03, 0x01, 0x00, 0x01, 0x00, 0x01, 0x00, 0x01, 0x00,
    ]);
    write_utf16(buf, 0x20C, "Affichage-1");
    put_u16(buf, 0x222, 0);

    put_u32(buf, 0x224, 1);
    put_u32(buf, 0x228, 0x6810);
    put_u32(buf, 0x22C, 1);

    put_u16(buf, 0x230, WIDTH as u16);
    put_u16(buf, 0x232, HEIGHT as u16);
    put_u32(buf, 0x234, 0x1010_0002);
    put_u32(buf, 0x238, 0x6750);
    put_u32(buf, 0x23C, 0x0007_0001);
    put_u32(buf, 0x240, 0x67A4);
    put_u32(buf, 0x244, 0);
    put_u32(buf, 0x248, 0x67D0);
    put_u32(buf, 0x24C, 0x54);
    put_u32(buf, 0x250, 0x6750);
    put_u32(buf, 0x254, 0x6750);

    put_u32(buf, 0x258, 0x0020_0101);
    put_u32(buf, 0x25C, 0x01A4_0020);
    put_u32(buf, 0x260, 0x0000_02A4);
    put_u32(buf, 0x264, 0x0000_07A4);
    put_u32(buf, 0x268, FRAME_DATA_BYTES as u32);
    put_u32(buf, 0x26C, 1);
    put_u32(buf, 0x270, 0x0001_7F01);
    put_u32(buf, 0x274, 0x24C3_24C3);
    put_u32(buf, 0x278, 0);
    put_u32(buf, 0x27C, 0x0001_517F);

    // 0x2C0: second bitmap/display descriptor.
    put_u16(buf, 0x2C0, WIDTH as u16);
    put_u16(buf, 0x2C2, HEIGHT as u16);
    put_u32(buf, 0x2C4, 0x0E01_0000);
    put_u32(buf, 0x2C8, 0);
    put_u32(buf, 0x2CC, 0x6670);
    put_u32(buf, 0x2D0, 0x0000_03A0);

    // 0x334: animation header. 0x338..0x33B stay zero.
    put_u32(buf, 0x334, 14);
    put_u16(buf, 0x33C, WIDTH as u16);
    put_u16(buf, 0x33E, HEIGHT as u16);
    put_u32(buf, 0x340, DESCRIPTOR_SIZE as u32);
    put_u32(buf, 0x344, HEIGHT as u32);
    put_u16(buf, 0x348, 0);
    put_u16(buf, 0x34C, 12);
    put_u16(buf, 0x34E, 256);
    put_u32(buf, 0x354, 1);
    put_u32(buf, 0x358, 2);
    put_u32(buf, 0x35C, 3);
}

fn write_footer(buf: &mut [u8]) {
    let f = FOOTER_OFFSET;

    put_u32(buf, f, 0x67A8);
    buf[f + 0x04..f + 0x0C].copy_from_slice(&[0x7D, 0x79, 0x8E, 0x08, 0xFF, 0x31, 0xDF, 0x08]);
    put_u32(buf, f + 0x0C, 7);
    buf[f + 0x10..f + 0x14].copy_from_slice(&[0x80, 0x80, 0x80, 0xFF]);
    buf[f + 0x14..f + 0x18].copy_from_slice(&[0x00, 0x00, 0x00, 0xFF]);
    put_u16(buf, f + 0x1C, HEIGHT as u16);
    put_u16(buf, f + 0x1E, WIDTH as u16);
    put_u16(buf, f + 0x24, HEIGHT as u16);
    put_u16(buf, f + 0x26, WIDTH as u16);
    put_u32(buf, f + 0x28, 0x0000_0401);

    put_u16(buf, f + 0x36, 0x14);
    write_utf16(buf, f + 0x38, "Programme0");
    buf[f + 0x4C..f + 0x54].copy_from_slice(&[0x7C, 0x79, 0x8E, 0x08, 0xFF, 0x31, 0xDF, 0x08]);
    put_u16(buf, f + 0x56, 0x14);
    write_utf16(buf, f + 0x58, "Animation0");
}

pub fn export_usb_fxd(anim: &Animation, path: &str) -> Result<()> {
    if anim.frames.is_empty() {
        bail!("cannot export USB.FXD: animation has no frames");
    }
    if anim.frames.len() > MAX_FRAMES {
        bail!(
            "cannot export USB.FXD: format supports at most {MAX_FRAMES} frames, animation has {}",
            anim.frames.len()
        );
    }

    let frame_count = anim.frames.len();
    let delay_ms = anim.frames[0].duration_ms.clamp(1, u16::MAX as u32) as u16;
    let mut buf = vec![0u8; FILE_BYTES];

    // Main header.
    put_u32(&mut buf, 0x00, 0x00A8_0139);
    put_u32(&mut buf, 0x04, DATA_OFFSET_FIELD);
    put_u16(&mut buf, 0x08, WIDTH as u16);
    put_u16(&mut buf, 0x0A, HEIGHT as u16);
    put_u32(&mut buf, 0x0C, 0x0000_3201);
    put_u32(&mut buf, 0x10, 0x0101_1002);
    put_u32(&mut buf, 0x14, 0x0001_0000);
    put_u32(&mut buf, 0x18, 0x0055_0000);

    let now = chrono::Local::now();
    put_u32(&mut buf, 0x30, now.year().max(0) as u32);
    put_u32(&mut buf, 0x34, now.month());
    put_u32(&mut buf, 0x38, now.day());
    put_u32(&mut buf, 0x3C, now.hour());
    put_u32(&mut buf, 0x40, now.minute());
    put_u32(&mut buf, 0x44, now.second());

    write_metadata(&mut buf);
    put_u16(&mut buf, 0x34A, frame_count as u16);
    put_u16(&mut buf, 0x350, (frame_count - 1) as u16);
    put_u16(&mut buf, 0x352, delay_ms);

    // 0x3A4: 32-entry frame index array, followed by its 0x40100 marker.
    for index in 0..MAX_FRAMES {
        let offset = 0x3A4 + index * 8;
        put_u32(&mut buf, offset, 0x0001_0000 + index as u32);
        put_u32(&mut buf, offset + 4, 0);
    }
    put_u32(&mut buf, 0x4A4, 0x0004_0100);

    // 0x4AC: 32 frame descriptors. The final descriptor has no 8-byte
    // trailer: frame data starts at 0x9A4, exactly where that trailer would be.
    for index in 0..MAX_FRAMES {
        let offset = DESCRIPTOR_TABLE_OFFSET + index * DESCRIPTOR_SIZE;
        put_u16(&mut buf, offset, WIDTH as u16);
        put_u16(&mut buf, offset + 0x02, HEIGHT as u16);
        put_u16(&mut buf, offset + 0x04, ROW_BYTES as u16);
        put_u16(&mut buf, offset + 0x06, ROW_BYTES as u16);
        put_u32(&mut buf, offset + 0x08, FRAME_BYTES as u32);
        put_u32(
            &mut buf,
            offset + 0x0C,
            (FIRST_FRAME_OFFSET + index * FRAME_BYTES - DESCRIPTOR_POINTER_BASE) as u32,
        );

        let tag = if index == MAX_FRAMES - 1 {
            0
        } else {
            0x0004_0101 + index as u32
        };
        put_u32(&mut buf, offset + 0x20, tag);
        put_u32(&mut buf, offset + 0x24, 0);
    }

    // Frame data: 0x9A4 + i*0x300. Slots beyond the used frame count repeat
    // the last frame so the fixed 32-slot layout stays fully addressable.
    for index in 0..MAX_FRAMES {
        let source = &anim.frames[index.min(frame_count - 1)];
        let offset = FIRST_FRAME_OFFSET + index * FRAME_BYTES;
        pack_frame(source, &mut buf[offset..offset + FRAME_BYTES]);
    }

    write_footer(&mut buf);

    let mut file = File::create(path)?;
    file.write_all(&buf)?;
    Ok(())
}

pub fn import_usb_fxd(path: &str) -> Result<Animation> {
    let mut file = File::open(path)?;
    let mut data = Vec::new();
    file.read_to_end(&mut data)?;

    if data.len() < 0x360 {
        bail!("not a USB.FXD file: only {} bytes", data.len());
    }

    let width = get_u16(&data, 0x08)? as usize;
    let height = get_u16(&data, 0x0A)? as usize;
    if width != WIDTH || height != HEIGHT {
        bail!("unsupported USB.FXD dimensions: {width}x{height} (expected 96x16)");
    }

    let frame_count = detect_frame_count(&data).unwrap_or(0);
    if frame_count == 0 {
        bail!("USB.FXD contains no frames");
    }
    if frame_count > MAX_FRAMES {
        bail!(
            "USB.FXD declares {frame_count} frames, but this format supports at most {MAX_FRAMES}"
        );
    }

    let delay_ms = get_u16(&data, 0x352).unwrap_or(0);
    let (frame_bytes, row_bytes) = descriptor_geometry(&data).unwrap_or((FRAME_BYTES, ROW_BYTES));

    let pixel_limit = pixel_data_limit(&data);
    let mut frames = Vec::with_capacity(frame_count);

    for index in 0..frame_count {
        let fallback_offset = FIRST_FRAME_OFFSET + index * frame_bytes;
        let offset = descriptor_frame_offset(&data, index)
            .filter(|&offset| offset >= FIRST_FRAME_OFFSET && offset < pixel_limit)
            .unwrap_or(fallback_offset);

        let available = pixel_limit
            .saturating_sub(offset)
            .min(frame_bytes)
            .min(data.len().saturating_sub(offset));

        let mut frame = Frame::default();
        frame.duration_ms = if delay_ms == 0 { 100 } else { delay_ms as u32 };
        decode_frame(&data[offset..offset + available], row_bytes, &mut frame);
        frames.push(frame);
    }

    let mut name = "Imported USB.FXD".to_string();
    if let Some(display_name) = read_utf16_z(&data, 0x20C) {
        if !display_name.is_empty() {
            name = display_name;
        }
    }

    Ok(Animation { frames, name })
}

pub fn export_gif(anim: &Animation, path: &str) -> Result<()> {
    let file = File::create(path)?;
    let mut encoder = gif::Encoder::new(file, 96, 16, &[])?;
    encoder.set_repeat(gif::Repeat::Infinite)?;

    for frame in &anim.frames {
        let mut frame_data: Vec<u8> = vec![0u8; 96 * 16];
        for row in 0..16 {
            for col in 0..96 {
                frame_data[row * 96 + col] = frame.pixels[row][col] & 0x0F;
            }
        }
        let palette_vec: Vec<u8> = vec![
            0, 0, 0, 85, 85, 85, 170, 170, 170, 255, 255, 255, 170, 0, 0, 255, 85, 85, 170, 85, 0,
            255, 255, 85, 0, 170, 0, 85, 255, 85, 0, 170, 170, 85, 255, 255, 0, 0, 170, 85, 85,
            255, 170, 0, 170, 255, 85, 255,
        ];
        let mut gif_frame = gif::Frame::from_palette_pixels(96, 16, frame_data, palette_vec, None);
        gif_frame.delay = (frame.duration_ms / 10) as u16;
        encoder.write_frame(&gif_frame)?;
    }

    Ok(())
}

pub fn import_gif(path: &str) -> Result<Animation> {
    use crate::palette::Palette;
    let palette = Palette::default();

    let mut file = File::open(path)?;
    let mut data = Vec::new();
    file.read_to_end(&mut data)?;

    let mut decoder = gif::DecodeOptions::new();
    decoder.set_color_output(gif::ColorOutput::Indexed);
    let mut reader = decoder.read_info(&data[..])?;

    let mut frames = Vec::new();
    while let Some(frame) = reader.read_next_frame()? {
        let mut anim_frame = Frame::default();
        let width = frame.width as usize;
        let height = frame.height as usize;

        for row in 0..height.min(16) {
            for col in 0..width.min(96) {
                let idx = frame.buffer[row * width + col] as usize;
                if let Some(ref pal) = frame.palette {
                    if idx * 3 + 2 < pal.len() {
                        let gif_r = pal[idx * 3];
                        let gif_g = pal[idx * 3 + 1];
                        let gif_b = pal[idx * 3 + 2];

                        let mut best_idx = 0;
                        let mut best_dist = u32::MAX;
                        for (pidx, &(pr, pg, pb)) in palette.colors.iter().enumerate() {
                            let dr = (gif_r as i32) - (pr as i32);
                            let dg = (gif_g as i32) - (pg as i32);
                            let db = (gif_b as i32) - (pb as i32);
                            let dist = (dr * dr + dg * dg + db * db) as u32;
                            if dist < best_dist {
                                best_dist = dist;
                                best_idx = pidx;
                            }
                        }
                        anim_frame.pixels[row][col] = best_idx as u8;
                    }
                }
            }
        }
        anim_frame.duration_ms = (frame.delay as u32) * 10;
        frames.push(anim_frame);
    }

    if frames.is_empty() {
        frames.push(Frame::default());
    }

    Ok(Animation {
        frames,
        name: "Imported GIF".to_string(),
    })
}

pub fn save_project(anim: &Animation, path: &str) -> Result<()> {
    let json = serde_json::to_string_pretty(anim)?;
    let mut file = File::create(path)?;
    file.write_all(json.as_bytes())?;
    Ok(())
}

pub fn load_project(path: &str) -> Result<Animation> {
    let mut file = File::open(path)?;
    let mut data = String::new();
    file.read_to_string(&mut data)?;
    let anim: Animation = serde_json::from_str(&data)?;
    Ok(anim)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Animation;
    use std::path::Path;

    #[test]
    fn test_export_import_fxd() {
        let mut anim = Animation::default();
        anim.frames[0].pixels[0][0] = 5;
        anim.frames[0].pixels[5][10] = 3;
        let path = "/tmp/ledfx_editor_roundtrip.fxd";
        export_usb_fxd(&anim, path).unwrap();
        let imported = import_usb_fxd(path).unwrap();
        assert_eq!(imported.frames.len(), 1);
        assert_eq!(imported.frames[0].pixels[0][0], 5);
        assert_eq!(imported.frames[0].pixels[5][10], 3);
    }

    #[test]
    fn test_export_layout_matches_observed_format() {
        let mut anim = Animation::default();
        anim.frames[0].pixels[0][0] = 5;
        anim.frames[0].pixels[0][1] = 3;
        anim.frames[0].pixels[0][2] = 0x0F;
        anim.frames[0].pixels[0][3] = 0x02;
        anim.frames[0].duration_ms = 250;

        let path = "/tmp/ledfx_editor_layout.fxd";
        export_usb_fxd(&anim, path).unwrap();
        let bytes = std::fs::read(path).unwrap();

        assert_eq!(bytes.len(), FILE_BYTES);
        assert_eq!(get_u32(&bytes, 0x04).unwrap(), DATA_OFFSET_FIELD);
        assert_eq!(get_u16(&bytes, 0x08).unwrap(), WIDTH as u16);
        assert_eq!(get_u16(&bytes, 0x0A).unwrap(), HEIGHT as u16);
        assert_eq!(get_u16(&bytes, 0x34A).unwrap(), 1);
        assert_eq!(get_u16(&bytes, 0x350).unwrap(), 0);
        assert_eq!(get_u16(&bytes, 0x352).unwrap(), 250);

        let descriptor = DESCRIPTOR_TABLE_OFFSET;
        assert_eq!(get_u16(&bytes, descriptor).unwrap(), WIDTH as u16);
        assert_eq!(get_u16(&bytes, descriptor + 0x02).unwrap(), HEIGHT as u16);
        assert_eq!(
            get_u16(&bytes, descriptor + 0x04).unwrap(),
            ROW_BYTES as u16
        );
        assert_eq!(
            get_u32(&bytes, descriptor + 0x08).unwrap(),
            FRAME_BYTES as u32
        );
        assert_eq!(
            get_u32(&bytes, descriptor + 0x0C).unwrap() as usize + DESCRIPTOR_POINTER_BASE,
            FIRST_FRAME_OFFSET
        );

        // 4bpp packing: 5,3 -> 0x53 and 15,2 -> 0xF2.
        assert_eq!(bytes[FIRST_FRAME_OFFSET], 0x53);
        assert_eq!(bytes[FIRST_FRAME_OFFSET + 1], 0xF2);

        // The footer must start exactly after 32 frame slots.
        assert_eq!(FOOTER_OFFSET, 0x69A4);
        assert!(has_footer(&bytes));
    }

    #[test]
    fn test_import_observed_usb_fxd() {
        let path = Path::new("../USB.FXD");
        if !path.exists() {
            return;
        }

        let anim = import_usb_fxd(path.to_str().unwrap()).unwrap();
        assert_eq!(anim.frames.len(), 32);
        assert_eq!(anim.frames[0].duration_ms, 250);

        // Frame 0 starts at 0x9A4 with two black rows.
        assert!(anim.frames[0].pixels[0].iter().all(|&pixel| pixel == 0));
        assert!(anim.frames[0].pixels[1].iter().all(|&pixel| pixel == 0));
        assert!(anim.frames[0].pixels[2].iter().all(|&pixel| pixel == 7));
        assert!(anim.frames[0].pixels[3].iter().all(|&pixel| pixel == 7));
        assert!(anim.frames[0].pixels[4].iter().all(|&pixel| pixel == 1));

        // The final frame is complete (it ends exactly at the footer).
        let last = &anim.frames[31];
        assert_eq!(last.pixels[15][0], 5);
        assert_eq!(last.pixels[15][5], 5);
        assert_eq!(last.pixels[15][6], 0);
    }

    #[test]
    fn test_roundtrip_observed_file_preserves_layout() {
        let source = Path::new("../USB.FXD");
        if !source.exists() {
            return;
        }

        let original = std::fs::read(source).unwrap();
        let anim = import_usb_fxd(source.to_str().unwrap()).unwrap();
        let path = "/tmp/ledfx_editor_roundtrip32.fxd";
        export_usb_fxd(&anim, path).unwrap();
        let exported = std::fs::read(path).unwrap();

        assert_eq!(exported.len(), original.len());
        assert_eq!(exported.len(), FILE_BYTES);

        // Metadata, index array and descriptors must match the observed file.
        // The timestamp at 0x30..0x48 is intentionally regenerated.
        assert_eq!(&exported[0x00..0x30], &original[0x00..0x30]);
        assert_eq!(&exported[0x48..0x200], &original[0x48..0x200]);
        assert_eq!(&exported[0x200..0x9A4], &original[0x200..0x9A4]);

        // All 32 complete frames, including the final frame, must survive.
        assert_eq!(
            &exported[FIRST_FRAME_OFFSET..FOOTER_OFFSET],
            &original[FIRST_FRAME_OFFSET..FOOTER_OFFSET]
        );
        assert_eq!(&exported[FOOTER_OFFSET..], &original[FOOTER_OFFSET..]);
    }

    #[test]
    fn test_export_rejects_more_than_32_frames() {
        let mut anim = Animation::default();
        anim.frames = (0..33).map(|_| Frame::default()).collect();
        let result = export_usb_fxd(&anim, "/tmp/ledfx_editor_too_many.fxd");
        assert!(result.is_err());
    }
}
