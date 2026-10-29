use crate::model::{Animation, Frame};
use crate::palette::{self, Palette};
use anyhow::{bail, Context, Result};
use chrono::{Datelike, Timelike};
use std::fs::File;
use std::io::{Read, Write};

const WIDTH: usize = 96;
const HEIGHT: usize = 16;
const ROW_BYTES: usize = WIDTH / 2; // 48 bytes = 96 pixels at 4 bpp
const FRAME_BYTES: usize = ROW_BYTES * HEIGHT; // 768 = 0x300

/// The original USB.FXD files always reserved 32 descriptor/frame slots, even
/// when only a handful of them were used. Keep that minimum so files with few
/// frames retain the exact layout that the hardware is known to accept.
const MIN_SLOTS: usize = 32;
/// Both the animation-header frame count and the index-entry tags are 16-bit.
const MAX_SLOTS: usize = u16::MAX as usize;

const DATA_OFFSET_FIELD: u32 = 0x0A00; // observed header value; not the frame start
const INDEX_ARRAY_OFFSET: usize = 0x03A4;
const INDEX_ENTRY_SIZE: usize = 8;
const INDEX_MARKER: u32 = 0x0004_0100;
const DESCRIPTOR_SIZE: usize = 0x28;
const DESCRIPTOR_POINTER_BASE: usize = 0x0200;
const FOOTER_BYTES: usize = 0x6C;

// The per-program animation header sits 0x70 bytes before the index array in
// both the single-program 32-frame sample and the multi-program sample. Two
// fields inside it are useful when importing: the used frame count and delay.
const ANIMATION_HEADER_COUNT_FROM_INDEX: usize = 0x5A;
const ANIMATION_HEADER_DELAY_FROM_INDEX: usize = 0x52;

fn index_marker_offset(slots: usize) -> usize {
    INDEX_ARRAY_OFFSET + slots * INDEX_ENTRY_SIZE
}

fn descriptor_table_offset(slots: usize) -> usize {
    // The 4-byte marker is followed by 4 bytes of padding.
    index_marker_offset(slots) + 8
}

fn first_frame_offset(slots: usize) -> usize {
    // The last descriptor has no separate 8-byte trailer: its +0x20 area is
    // where frame 0 starts.
    descriptor_table_offset(slots) + slots * DESCRIPTOR_SIZE - 8
}

fn frame_data_bytes(slots: usize) -> usize {
    slots * FRAME_BYTES
}

fn last_frame_offset(slots: usize) -> usize {
    first_frame_offset(slots) + (slots - 1) * FRAME_BYTES
}

fn footer_offset(slots: usize) -> usize {
    first_frame_offset(slots) + frame_data_bytes(slots)
}

fn file_bytes(slots: usize) -> usize {
    footer_offset(slots) + FOOTER_BYTES
}

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
            // The panel displays the low nibble as the left pixel of each
            // pair, so pack the even column there and the odd column in the
            // high nibble.
            let left = palette::normalize(frame.pixels[row][byte_col * 2]);
            let right = palette::normalize(frame.pixels[row][byte_col * 2 + 1]);
            out[row * ROW_BYTES + byte_col] = (right << 4) | left;
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
            frame.pixels[row][byte_col * 2] = palette::normalize(packed & 0x0F);
            frame.pixels[row][byte_col * 2 + 1] = palette::normalize(packed >> 4);
        }
    }
}

/// True when the file ends with the UTF-16LE string "Animation0", which is the
/// last field of the 0x6C-byte footer block in the observed format.
#[cfg(test)]
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

fn descriptor_frame_offset(data: &[u8], descriptor_table: usize, index: usize) -> Option<usize> {
    let descriptor = descriptor_table.checked_add(index.checked_mul(DESCRIPTOR_SIZE)?)?;
    let stored = get_u32(data, descriptor + 0x0C).ok()? as usize;
    stored.checked_add(DESCRIPTOR_POINTER_BASE)
}

/// One animation program embedded in a USB.FXD file.
///
/// A file may contain a single program (the original 32-frame sample) or a
/// container with several programs/scenes (the 86-86-12-6 sample). The layout
/// of each program is the same; only its position in the file changes.
#[derive(Clone, Copy, Debug)]
struct ProgramInfo {
    descriptor_offset: usize,
    /// Number of descriptor/index slots. This can be larger than the used
    /// frame count because small animations keep the historical 32 slots.
    slots: usize,
    used_frames: usize,
    first_frame: usize,
    frame_bytes: usize,
    row_bytes: usize,
    width: usize,
    height: usize,
    delay_ms: u16,
}

/// Parse a program from the `0x00040100` marker that terminates its index
/// array. The descriptor table follows the marker, and descriptor 0 points at
/// the first frame, which lets us derive the slot count without trusting any
/// fixed offset.
fn parse_program(data: &[u8], marker_offset: usize) -> Option<ProgramInfo> {
    let descriptor_offset = marker_offset.checked_add(8)?;
    let width = get_u16(data, descriptor_offset).ok()? as usize;
    let height = get_u16(data, descriptor_offset + 0x02).ok()? as usize;
    let row_bytes = get_u16(data, descriptor_offset + 0x04).ok()? as usize;
    let frame_bytes = get_u32(data, descriptor_offset + 0x08).ok()? as usize;
    let stored = get_u32(data, descriptor_offset + 0x0C).ok()? as usize;

    if width == 0 || width > WIDTH || height == 0 || height > HEIGHT {
        return None;
    }
    if row_bytes == 0 || row_bytes > ROW_BYTES || frame_bytes == 0 || frame_bytes > 1 << 20 {
        return None;
    }
    // All observed programs store exactly row_bytes * height bytes.
    if frame_bytes != row_bytes.checked_mul(height)? {
        return None;
    }

    let first_frame = stored.checked_add(DESCRIPTOR_POINTER_BASE)?;
    if first_frame < descriptor_offset {
        return None;
    }
    let descriptor_bytes = first_frame
        .checked_sub(descriptor_offset)?
        .checked_add(8)?;
    if descriptor_bytes % DESCRIPTOR_SIZE != 0 {
        return None;
    }
    let slots = descriptor_bytes / DESCRIPTOR_SIZE;
    if slots == 0 || slots > MAX_SLOTS {
        return None;
    }

    let index_offset = marker_offset.checked_sub(slots.checked_mul(INDEX_ENTRY_SIZE)?)?;
    if index_offset < 0x360 {
        return None;
    }
    if get_u32(data, index_offset).ok()? != 0x0001_0000 {
        return None;
    }
    if slots >= 2 && get_u32(data, index_offset + INDEX_ENTRY_SIZE).ok()? != 0x0001_0001 {
        return None;
    }
    // Descriptor 0 and 1 are never overwritten by frame data. Checking their
    // tags rejects coincidental 0x00040100 markers in footer/random data.
    if get_u32(data, descriptor_offset + 0x20).ok()? != 0x0004_0101 {
        return None;
    }
    if slots >= 2
        && get_u32(data, descriptor_offset + DESCRIPTOR_SIZE + 0x20).ok()? != 0x0004_0102
    {
        return None;
    }

    let program_end = first_frame.checked_add(slots.checked_mul(frame_bytes)?)?;
    if program_end > data.len() {
        return None;
    }

    // The per-program animation header sits a fixed distance before the index
    // array. Its count is the number of frames that are actually used, while
    // the descriptor table may reserve more slots.
    let used_frames = get_u16(
        data,
        index_offset.checked_sub(ANIMATION_HEADER_COUNT_FROM_INDEX)?,
    )
    .ok()
    .map(|count| count as usize)
    .filter(|&count| count > 0 && count <= slots)
    .unwrap_or(slots);
    let delay_ms = get_u16(
        data,
        index_offset.checked_sub(ANIMATION_HEADER_DELAY_FROM_INDEX)?,
    )
    .unwrap_or(0);

    Some(ProgramInfo {
        descriptor_offset,
        slots,
        used_frames,
        first_frame,
        frame_bytes,
        row_bytes,
        width,
        height,
        delay_ms,
    })
}

fn find_programs(data: &[u8]) -> Vec<ProgramInfo> {
    let mut programs = Vec::new();
    if data.len() < 4 {
        return programs;
    }
    for marker_offset in 0..=data.len() - 4 {
        if get_u32(data, marker_offset).ok() != Some(INDEX_MARKER) {
            continue;
        }
        if let Some(program) = parse_program(data, marker_offset) {
            programs.push(program);
        }
    }
    programs
}

fn write_metadata(buf: &mut [u8], slots: usize) {
    let first_frame = first_frame_offset(slots);
    let last_frame = last_frame_offset(slots);

    // 0x200: "Affichage-1" display object header.
    buf[0x200..0x20C].copy_from_slice(&[
        0x55, 0x55, 0x01, 0x03, 0x01, 0x00, 0x01, 0x00, 0x01, 0x00, 0x01, 0x00,
    ]);
    write_utf16(buf, 0x20C, "Affichage-1");
    put_u16(buf, 0x222, 0);

    put_u32(buf, 0x224, 1);
    put_u32(buf, 0x228, (last_frame + 0x16C) as u32);
    put_u32(buf, 0x22C, 1);

    put_u16(buf, 0x230, WIDTH as u16);
    put_u16(buf, 0x232, HEIGHT as u16);
    put_u32(buf, 0x234, 0x1010_0002);
    put_u32(buf, 0x238, (last_frame + 0xAC) as u32);
    put_u32(buf, 0x23C, 0x0007_0001);
    put_u32(buf, 0x240, (last_frame + 0x100) as u32);
    put_u32(buf, 0x244, 0);
    put_u32(buf, 0x248, (last_frame + 0x12C) as u32);
    put_u32(buf, 0x24C, 0x54);
    put_u32(buf, 0x250, (last_frame + 0xAC) as u32);
    put_u32(buf, 0x254, (last_frame + 0xAC) as u32);

    put_u32(buf, 0x258, 0x0000_0101 | ((slots as u32) << 16));
    put_u32(buf, 0x25C, 0x01A4_0000 | slots as u32);
    put_u32(buf, 0x260, (0x1A4 + slots * INDEX_ENTRY_SIZE) as u32);
    put_u32(buf, 0x264, (first_frame - DESCRIPTOR_POINTER_BASE) as u32);
    put_u32(buf, 0x268, frame_data_bytes(slots) as u32);
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
    put_u32(buf, 0x2CC, (last_frame - 0x34) as u32);
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

fn write_footer(buf: &mut [u8], slots: usize) {
    let f = footer_offset(slots);
    let last_frame = last_frame_offset(slots);

    put_u32(buf, f, (last_frame + 0x104) as u32);
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

fn gcd_u32(mut a: u32, mut b: u32) -> u32 {
    while b != 0 {
        let remainder = a % b;
        a = b;
        b = remainder;
    }
    a.max(1)
}

/// USB.FXD only stores one delay for the whole animation, so per-frame
/// durations are represented by repeating frames in the available slots.
///
/// The returned indices select a source frame for every slot, and the delay is
/// the duration of one slot. The common divisor is used when the whole
/// animation fits; otherwise the delay that minimizes the total timing error
/// while fitting in `max_slots` slots is chosen.
fn build_fxd_timeline(frames: &[Frame], max_slots: usize) -> (Vec<usize>, u16) {
    debug_assert!(!frames.is_empty());
    debug_assert!(frames.len() <= max_slots);

    let durations: Vec<u32> = frames
        .iter()
        .map(|frame| frame.duration_ms.max(1))
        .collect();

    let gcd = durations.iter().copied().reduce(gcd_u32).unwrap_or(1);
    let exact_slots: u64 = durations
        .iter()
        .map(|&duration| (duration / gcd) as u64)
        .sum();
    if gcd <= u16::MAX as u32 && exact_slots <= max_slots as u64 {
        let mut timeline = Vec::with_capacity(exact_slots as usize);
        for (index, &duration) in durations.iter().enumerate() {
            for _ in 0..(duration / gcd) {
                timeline.push(index);
            }
        }
        return (timeline, gcd as u16);
    }

    // A delay equal to the longest frame gives one slot per source frame,
    // which is the fallback when no better expansion fits.
    let max_delay = durations
        .iter()
        .copied()
        .max()
        .unwrap_or(1)
        .min(u16::MAX as u32)
        .max(1);

    if frames.len() >= max_slots {
        // There is no room to repeat frames at all, so every frame gets exactly
        // one slot. Pick the delay closest to all frame durations (the median)
        // to minimise the total timing error.
        let mut sorted = durations.clone();
        sorted.sort_unstable();
        let mid = sorted.len() / 2;
        let mut candidates = vec![sorted[mid].min(u16::MAX as u32).max(1)];
        if mid > 0 {
            candidates.push(sorted[mid - 1].min(u16::MAX as u32).max(1));
        }
        let delay = candidates
            .into_iter()
            .min_by_key(|&delay| {
                durations
                    .iter()
                    .map(|&duration| (duration as i64 - delay as i64).unsigned_abs())
                    .sum::<u64>()
            })
            .unwrap_or(max_delay);
        return ((0..frames.len()).collect(), delay as u16);
    }

    // Search small delay ranges exhaustively, but sample larger ones so a
    // 16-bit frame count cannot make the search quadratic.
    let mut candidates = Vec::new();
    if max_delay <= 4096 {
        candidates.extend(1..=max_delay);
    } else {
        let step = (max_delay + 4095) / 4096;
        let mut delay = 1;
        while delay <= max_delay {
            candidates.push(delay);
            delay = delay.saturating_add(step);
        }
        candidates.push(max_delay);
    }
    let total: u64 = durations.iter().map(|&duration| duration as u64).sum();
    let ideal = (total / max_slots as u64).clamp(1, max_delay as u64) as u32;
    candidates.push(ideal);
    candidates.sort_unstable();
    candidates.dedup();

    let mut best: Option<(u64, usize, u16, Vec<usize>)> = None;

    for delay in candidates {
        let mut repeats = Vec::with_capacity(frames.len());
        let mut slots = 0u64;
        let mut error = 0u64;
        let mut fits = true;

        for &duration in &durations {
            // Round to the nearest number of slots, but never drop a frame.
            let count = ((duration as u64 + delay as u64 / 2) / delay as u64).max(1);
            slots += count;
            if slots > max_slots as u64 {
                fits = false;
                break;
            }
            error += (count * delay as u64).abs_diff(duration as u64);
            repeats.push(count as usize);
        }

        if !fits {
            continue;
        }

        let better = match &best {
            None => true,
            Some((best_error, best_slots, _, _)) => {
                error < *best_error || (error == *best_error && slots < *best_slots as u64)
            }
        };
        if better {
            let mut timeline = Vec::with_capacity(slots as usize);
            for (index, count) in repeats.into_iter().enumerate() {
                for _ in 0..count {
                    timeline.push(index);
                }
            }
            best = Some((error, slots as usize, delay as u16, timeline));
        }
    }

    best.map(|(_, _, delay, timeline)| (timeline, delay))
        .unwrap_or_else(|| ((0..frames.len()).collect(), max_delay as u16))
}

pub fn export_usb_fxd_bytes(anim: &Animation) -> Result<Vec<u8>> {
    if anim.frames.is_empty() {
        bail!("cannot export USB.FXD: animation has no frames");
    }
    if anim.frames.len() > MAX_SLOTS {
        bail!(
            "cannot export USB.FXD: format supports at most {MAX_SLOTS} frames, animation has {}",
            anim.frames.len()
        );
    }

    let (timeline, delay_ms) = build_fxd_timeline(&anim.frames, MAX_SLOTS);
    let used_frames = timeline.len();
    // Keep the historical 32-slot layout for small animations, but grow the
    // slot count when the animation or its timing expansion needs more.
    let slots = used_frames.max(MIN_SLOTS);
    let descriptor_table = descriptor_table_offset(slots);
    let first_frame = first_frame_offset(slots);
    let mut buf = vec![0u8; file_bytes(slots)];

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

    write_metadata(&mut buf, slots);
    put_u16(&mut buf, 0x34A, used_frames as u16);
    // The animation-header field at +0x1C (0x350) stores the last frame index,
    // but observed multi-program files cap it at 31 even when a program has far
    // more frames (e.g. 86 frames still write 31). Clamp to 31 to match.
    put_u16(&mut buf, 0x350, (used_frames - 1).min(31) as u16);
    put_u16(&mut buf, 0x352, delay_ms);

    // Frame index array, followed by its 0x40100 marker and 4 bytes padding.
    for index in 0..slots {
        let offset = INDEX_ARRAY_OFFSET + index * INDEX_ENTRY_SIZE;
        put_u32(&mut buf, offset, 0x0001_0000 + index as u32);
        put_u32(&mut buf, offset + 4, 0);
    }
    put_u32(&mut buf, index_marker_offset(slots), INDEX_MARKER);

    // Frame descriptors. The final descriptor has no separate 8-byte trailer:
    // frame data starts exactly where that trailer would be.
    for index in 0..slots {
        let offset = descriptor_table + index * DESCRIPTOR_SIZE;
        put_u16(&mut buf, offset, WIDTH as u16);
        put_u16(&mut buf, offset + 0x02, HEIGHT as u16);
        put_u16(&mut buf, offset + 0x04, ROW_BYTES as u16);
        put_u16(&mut buf, offset + 0x06, ROW_BYTES as u16);
        put_u32(&mut buf, offset + 0x08, FRAME_BYTES as u32);
        put_u32(
            &mut buf,
            offset + 0x0C,
            (first_frame + index * FRAME_BYTES - DESCRIPTOR_POINTER_BASE) as u32,
        );

        let tag = if index == slots - 1 {
            0
        } else {
            0x0004_0101 + index as u32
        };
        put_u32(&mut buf, offset + 0x20, tag);
        put_u32(&mut buf, offset + 0x24, 0);
    }

    // Frame data. Slots beyond the used timeline repeat the last frame so the
    // minimum 32-slot layout stays fully addressable.
    for index in 0..slots {
        let source = &anim.frames[timeline[index.min(used_frames - 1)]];
        let offset = first_frame + index * FRAME_BYTES;
        pack_frame(source, &mut buf[offset..offset + FRAME_BYTES]);
    }

    write_footer(&mut buf, slots);

    Ok(buf)
}

pub fn export_usb_fxd(anim: &Animation, path: &str) -> Result<()> {
    let buf = export_usb_fxd_bytes(anim)?;
    let mut file = File::create(path)?;
    file.write_all(&buf)?;
    Ok(())
}

pub fn import_usb_fxd_bytes(data: &[u8]) -> Result<Animation> {
    if data.len() < 0x360 {
        bail!("not a USB.FXD file: only {} bytes", data.len());
    }

    let width = get_u16(data, 0x08)? as usize;
    let height = get_u16(data, 0x0A)? as usize;
    if width != WIDTH || height != HEIGHT {
        bail!("unsupported USB.FXD dimensions: {width}x{height} (expected 96x16)");
    }

    // A file may contain several programs. Prefer the first full-size 96x16
    // program; the editor's canvas cannot represent the smaller 16x16/20x16
    // programs that also appear in some vendor containers.
    let programs = find_programs(data);
    let program = programs
        .iter()
        .find(|program| {
            program.width == WIDTH
                && program.height == HEIGHT
                && program.frame_bytes == FRAME_BYTES
        })
        .or_else(|| programs.first())
        .context("USB.FXD contains no readable animation program")?;

    let frame_count = program.used_frames;
    if frame_count == 0 {
        bail!("USB.FXD contains no frames");
    }

    let frame_bytes = program.frame_bytes;
    let row_bytes = program.row_bytes;
    let program_end = program
        .first_frame
        .saturating_add(program.slots.saturating_mul(frame_bytes));
    let delay_ms = program.delay_ms;
    let mut frames = Vec::with_capacity(frame_count);

    for index in 0..frame_count {
        let fallback_offset = program
            .first_frame
            .saturating_add(index.saturating_mul(frame_bytes));
        let offset = descriptor_frame_offset(data, program.descriptor_offset, index)
            .filter(|&offset| offset >= program.first_frame && offset < program_end)
            .unwrap_or(fallback_offset);

        // Malformed or truncated descriptors must not make us slice outside
        // the file. Decode whatever bytes are actually available instead.
        let start = offset.min(data.len());
        let available = frame_bytes
            .min(program_end.saturating_sub(start))
            .min(data.len().saturating_sub(start));

        let mut frame = Frame::default();
        frame.duration_ms = if delay_ms == 0 { 100 } else { delay_ms as u32 };
        decode_frame(&data[start..start + available], row_bytes, &mut frame);
        frames.push(frame);
    }

    let mut name = "Imported USB.FXD".to_string();
    if let Some(display_name) = read_utf16_z(data, 0x20C) {
        if !display_name.is_empty() {
            name = display_name;
        }
    }

    Ok(Animation { frames, name })
}

pub fn import_usb_fxd(path: &str) -> Result<Animation> {
    let mut file = File::open(path)?;
    let mut data = Vec::new();
    file.read_to_end(&mut data)?;
    import_usb_fxd_bytes(&data)
}

pub fn export_gif_bytes(anim: &Animation) -> Result<Vec<u8>> {
    if anim.frames.is_empty() {
        bail!("cannot export GIF: animation has no frames");
    }

    let mut output = Vec::new();
    {
        let mut encoder = gif::Encoder::new(&mut output, 96, 16, &[])?;
        encoder.set_repeat(gif::Repeat::Infinite)?;

        let palette = Palette::default();
        let palette_vec: Vec<u8> = palette
            .colors
            .iter()
            .flat_map(|&(r, g, b)| [r, g, b])
            .collect();

        for frame in &anim.frames {
            let mut frame_data: Vec<u8> = vec![0u8; 96 * 16];
            for row in 0..16 {
                for col in 0..96 {
                    frame_data[row * 96 + col] = palette::normalize(frame.pixels[row][col]);
                }
            }
            let mut gif_frame =
                gif::Frame::from_palette_pixels(96, 16, frame_data, palette_vec.clone(), None);
            gif_frame.delay = (frame.duration_ms / 10) as u16;
            encoder.write_frame(&gif_frame)?;
        }
    }

    Ok(output)
}

pub fn export_gif(anim: &Animation, path: &str) -> Result<()> {
    let data = export_gif_bytes(anim)?;
    let mut file = File::create(path)?;
    file.write_all(&data)?;
    Ok(())
}

pub fn import_gif_bytes(data: &[u8]) -> Result<Animation> {
    use crate::palette::Palette;
    let palette = Palette::default();

    let mut decoder = gif::DecodeOptions::new();
    decoder.set_color_output(gif::ColorOutput::Indexed);
    let mut reader = decoder.read_info(data)?;

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

pub fn import_gif(path: &str) -> Result<Animation> {
    let mut file = File::open(path)?;
    let mut data = Vec::new();
    file.read_to_end(&mut data)?;
    import_gif_bytes(&data)
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
        anim.frames[0].pixels[0][2] = 0x07;
        anim.frames[0].pixels[0][3] = 0x02;
        anim.frames[0].duration_ms = 250;

        let path = "/tmp/ledfx_editor_layout.fxd";
        export_usb_fxd(&anim, path).unwrap();
        let bytes = std::fs::read(path).unwrap();

        assert_eq!(bytes.len(), file_bytes(MIN_SLOTS));
        assert_eq!(get_u32(&bytes, 0x04).unwrap(), DATA_OFFSET_FIELD);
        assert_eq!(get_u16(&bytes, 0x08).unwrap(), WIDTH as u16);
        assert_eq!(get_u16(&bytes, 0x0A).unwrap(), HEIGHT as u16);
        assert_eq!(get_u16(&bytes, 0x34A).unwrap(), 1);
        assert_eq!(get_u16(&bytes, 0x350).unwrap(), 0);
        assert_eq!(get_u16(&bytes, 0x352).unwrap(), 250);

        let descriptor = descriptor_table_offset(MIN_SLOTS);
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
            first_frame_offset(MIN_SLOTS)
        );

        // 4bpp packing: left pixel in the low nibble, right in the high
        // nibble: columns 5,3 -> 0x35 and 7,2 -> 0x27.
        assert_eq!(bytes[first_frame_offset(MIN_SLOTS)], 0x35);
        assert_eq!(bytes[first_frame_offset(MIN_SLOTS) + 1], 0x27);

        // The footer must start exactly after 32 frame slots.
        assert_eq!(footer_offset(MIN_SLOTS), 0x69A4);
        assert!(has_footer(&bytes));
    }

    #[test]
    fn test_export_preserves_per_frame_durations_by_repeating_frames() {
        let mut anim = Animation::default();
        anim.frames = vec![Frame::default(), Frame::default(), Frame::default()];
        anim.frames[0].duration_ms = 500;
        anim.frames[0].pixels[0][0] = 1;
        anim.frames[1].duration_ms = 1000;
        anim.frames[1].pixels[0][0] = 2;
        anim.frames[2].duration_ms = 800;
        anim.frames[2].pixels[0][0] = 3;

        let path = "/tmp/ledfx_editor_variable_durations.fxd";
        export_usb_fxd(&anim, path).unwrap();
        let bytes = std::fs::read(path).unwrap();

        // 500, 1000 and 800 ms share a 100 ms divisor, i.e. 5 + 10 + 8 slots.
        assert_eq!(get_u16(&bytes, 0x34A).unwrap(), 23);
        assert_eq!(get_u16(&bytes, 0x350).unwrap(), 22);
        assert_eq!(get_u16(&bytes, 0x352).unwrap(), 100);

        let exported_colors: Vec<u8> = (0..23)
            .map(|index| bytes[first_frame_offset(MIN_SLOTS) + index * FRAME_BYTES] & 0x0F)
            .collect();
        let expected: Vec<u8> = (0..5)
            .map(|_| 1)
            .chain((0..10).map(|_| 2))
            .chain((0..8).map(|_| 3))
            .collect();
        assert_eq!(exported_colors, expected);
    }

    #[test]
    fn test_fxd_timeline_falls_back_when_exact_expansion_is_too_large() {
        let frames: Vec<Frame> = (0..MIN_SLOTS)
            .map(|index| {
                let mut frame = Frame::default();
                frame.duration_ms = 100 + index as u32 * 10;
                frame
            })
            .collect();

        let (timeline, delay) = build_fxd_timeline(&frames, MIN_SLOTS);

        // All 32 source frames must remain, each occupying one slot.
        assert_eq!(timeline.len(), MIN_SLOTS);
        assert_eq!(timeline, (0..MIN_SLOTS).collect::<Vec<_>>());
        assert!(delay >= 1);

        // The chosen delay should make the exported timeline as close as
        // possible to the requested total duration.
        let total: u64 = frames.iter().map(|frame| frame.duration_ms as u64).sum();
        let exported = timeline.len() as u64 * delay as u64;
        assert!(total.abs_diff(exported) < total / 10);
    }

    #[test]
    fn test_fxd_timeline_keeps_all_frames_when_durations_exceed_u16() {
        let frames: Vec<Frame> = (0..MIN_SLOTS)
            .map(|_| {
                let mut frame = Frame::default();
                frame.duration_ms = 100_000;
                frame
            })
            .collect();

        let (timeline, delay) = build_fxd_timeline(&frames, MIN_SLOTS);

        assert_eq!(timeline.len(), MIN_SLOTS);
        assert_eq!(delay, u16::MAX);
    }

    #[test]
    fn test_fxd_timeline_expands_into_more_than_32_slots() {
        let frames: Vec<Frame> = (0..64)
            .map(|_| {
                let mut frame = Frame::default();
                frame.duration_ms = 100;
                frame
            })
            .collect();

        let (timeline, delay) = build_fxd_timeline(&frames, MAX_SLOTS);

        // The exact expansion needs one slot per frame and fits in 64 slots.
        assert_eq!(timeline.len(), 64);
        assert_eq!(timeline, (0..64).collect::<Vec<_>>());
        assert_eq!(delay, 100);
    }

    #[test]
    fn test_import_observed_usb_fxd() {
        let path = Path::new("reverse/USB.FXD");
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
        let source = Path::new("reverse/USB.FXD");
        if !source.exists() {
            return;
        }

        let original = std::fs::read(source).unwrap();
        let anim = import_usb_fxd(source.to_str().unwrap()).unwrap();
        let path = "/tmp/ledfx_editor_roundtrip32.fxd";
        export_usb_fxd(&anim, path).unwrap();
        let exported = std::fs::read(path).unwrap();

        assert_eq!(exported.len(), original.len());
        assert_eq!(exported.len(), file_bytes(MIN_SLOTS));

        // Metadata, index array and descriptors must match the observed file.
        // The timestamp at 0x30..0x48 is intentionally regenerated.
        assert_eq!(&exported[0x00..0x30], &original[0x00..0x30]);
        assert_eq!(&exported[0x48..0x200], &original[0x48..0x200]);
        assert_eq!(&exported[0x200..0x9A4], &original[0x200..0x9A4]);

        // All 32 complete frames, including the final frame, must survive.
        assert_eq!(
            &exported[first_frame_offset(MIN_SLOTS)..footer_offset(MIN_SLOTS)],
            &original[first_frame_offset(MIN_SLOTS)..footer_offset(MIN_SLOTS)]
        );
        assert_eq!(
            &exported[footer_offset(MIN_SLOTS)..],
            &original[footer_offset(MIN_SLOTS)..]
        );
    }

    #[test]
    fn test_gif_roundtrip_preserves_per_frame_durations() {
        let mut anim = Animation::default();
        anim.frames = vec![Frame::default(), Frame::default(), Frame::default()];
        anim.frames[0].duration_ms = 100;
        anim.frames[1].duration_ms = 500;
        anim.frames[2].duration_ms = 1000;

        let path = "/tmp/ledfx_editor_durations.gif";
        export_gif(&anim, path).unwrap();
        let imported = import_gif(path).unwrap();
        let durations: Vec<u32> = imported
            .frames
            .iter()
            .map(|frame| frame.duration_ms)
            .collect();

        assert_eq!(durations, vec![100, 500, 1000]);
    }

    #[test]
    fn test_pack_places_left_pixel_in_low_nibble() {
        let mut frame = Frame::default();
        frame.pixels[0][0] = 1; // left pixel of the first pair
        frame.pixels[0][1] = 4; // right pixel of the first pair

        let mut packed = vec![0u8; FRAME_BYTES];
        pack_frame(&frame, &mut packed);
        assert_eq!(packed[0], 0x41); // right << 4 | left

        let mut decoded = Frame::default();
        decode_frame(&packed, ROW_BYTES, &mut decoded);
        assert_eq!(decoded.pixels[0][0], 1);
        assert_eq!(decoded.pixels[0][1], 4);
    }

    #[test]
    fn test_pack_swaps_adjacent_column_pairs() {
        let mut frame = Frame::default();
        for col in 0..8usize {
            frame.pixels[0][col] = (col as u8 + 1) % 8;
        }

        let mut packed = vec![0u8; FRAME_BYTES];
        pack_frame(&frame, &mut packed);

        // Left/even column first in the low nibble, right/odd column in the
        // high nibble. Reading the bytes high-first would show 2,1,4,3,...;
        // the panel reads low-first and therefore shows 1,2,3,4,...
        assert_eq!(packed[0], 0x21); // col 1=2 (high), col 0=1 (low)
        assert_eq!(packed[1], 0x43); // col 3=4 (high), col 2=3 (low)
        assert_eq!(packed[2], 0x65); // col 5=6 (high), col 4=5 (low)
        assert_eq!(packed[3], 0x07); // col 7=0 (high), col 6=7 (low)
    }

    #[test]
    fn test_export_normalizes_unsupported_colors_to_black() {
        let mut anim = Animation::default();
        anim.frames[0].pixels[0][0] = 8;
        anim.frames[0].pixels[0][1] = 15;

        let path = "/tmp/ledfx_editor_unsupported_colors.fxd";
        export_usb_fxd(&anim, path).unwrap();
        let bytes = std::fs::read(path).unwrap();

        // Both unsupported indices become black (index 0), packed as 0x00.
        assert_eq!(bytes[first_frame_offset(MIN_SLOTS)], 0x00);
    }

    #[test]
    fn test_export_import_more_than_32_frames() {
        let mut anim = Animation::default();
        anim.frames = (0..64)
            .map(|index| {
                let mut frame = Frame::default();
                frame.duration_ms = 100;
                frame.pixels[0][0] = (index % 8) as u8;
                frame
            })
            .collect();

        let path = "/tmp/ledfx_editor_64_frames.fxd";
        export_usb_fxd(&anim, path).unwrap();
        let bytes = std::fs::read(path).unwrap();

        // The layout must grow past the historical 32-slot minimum.
        assert_eq!(bytes.len(), file_bytes(64));
        assert_eq!(get_u16(&bytes, 0x34A).unwrap(), 64);
        // The animation-header last-index field at 0x350 is capped at 31.
        assert_eq!(get_u16(&bytes, 0x350).unwrap(), 31);

        let imported = import_usb_fxd(path).unwrap();
        assert_eq!(imported.frames.len(), 64);
        for (index, frame) in imported.frames.iter().enumerate() {
            assert_eq!(frame.pixels[0][0], (index % 8) as u8);
        }
    }

    #[test]
    fn test_import_multi_program_usb_fxd() {
        let path = Path::new("86-86-12-6.FXD");
        if !path.exists() {
            return;
        }

        let anim = import_usb_fxd(path.to_str().unwrap()).unwrap();

        // The first full-size program has 86 frames.
        assert_eq!(anim.frames.len(), 86);
        assert!(anim.frames.iter().all(|frame| frame.duration_ms > 0));
    }

    #[test]
    fn test_export_86_frames_round_trip() {
        let mut anim = Animation::default();
        anim.frames = (0..86)
            .map(|index| {
                let mut frame = Frame::default();
                frame.duration_ms = 250;
                frame.pixels[0][0] = (index % 8) as u8;
                frame.pixels[15][95] = (index % 8) as u8;
                frame
            })
            .collect();

        let path = "/tmp/ledfx_editor_86_frames.fxd";
        export_usb_fxd(&anim, path).unwrap();
        let bytes = std::fs::read(path).unwrap();

        // 86 used frames grow past the 32-slot minimum.
        let slots = 86;
        assert_eq!(bytes.len(), file_bytes(slots));
        assert_eq!(get_u16(&bytes, 0x34A).unwrap(), 86);
        assert_eq!(get_u16(&bytes, 0x350).unwrap(), 31); // capped at 31
        assert_eq!(get_u16(&bytes, 0x352).unwrap(), 250);

        // Verify dynamic offsets match the formulas.
        assert_eq!(index_marker_offset(slots), 0x3A4 + slots * 8);
        assert_eq!(descriptor_table_offset(slots), index_marker_offset(slots) + 8);
        assert_eq!(
            first_frame_offset(slots),
            descriptor_table_offset(slots) + slots * DESCRIPTOR_SIZE - 8
        );
        assert_eq!(footer_offset(slots), first_frame_offset(slots) + slots * FRAME_BYTES);

        // Marker at end of index array.
        assert_eq!(
            get_u32(&bytes, index_marker_offset(slots)).unwrap(),
            INDEX_MARKER
        );

        let imported = import_usb_fxd(path).unwrap();
        assert_eq!(imported.frames.len(), 86);
        for (index, frame) in imported.frames.iter().enumerate() {
            assert_eq!(frame.pixels[0][0], (index % 8) as u8);
            assert_eq!(frame.pixels[15][95], (index % 8) as u8);
            assert_eq!(frame.duration_ms, 250);
        }
    }

    #[test]
    fn test_export_dynamic_offsets_for_non_32_slots() {
        // Verify that non-32 slot counts produce the expected dynamic layout.
        for slots in [1usize, 33, 40, 86, 100] {
            let marker = index_marker_offset(slots);
            let descriptors = descriptor_table_offset(slots);
            let first = first_frame_offset(slots);
            let footer = footer_offset(slots);
            let total = file_bytes(slots);

            assert_eq!(marker, 0x3A4 + slots * 8);
            assert_eq!(descriptors, marker + 8);
            assert_eq!(first, descriptors + slots * DESCRIPTOR_SIZE - 8);
            assert_eq!(footer, first + slots * FRAME_BYTES);
            assert_eq!(total, footer + FOOTER_BYTES);

            // Last descriptor's tag at +0x20 is exactly where frame data starts
            // (the last descriptor has no separate 8-byte trailer).
            let last_tag_offset = descriptors + (slots - 1) * DESCRIPTOR_SIZE + 0x20;
            assert_eq!(last_tag_offset, first);
        }
    }

    #[test]
    fn test_export_rejects_more_than_u16_max_frames() {
        let mut anim = Animation::default();
        anim.frames = (0..=MAX_SLOTS).map(|_| Frame::default()).collect();
        let result = export_usb_fxd(&anim, "/tmp/ledfx_editor_too_many.fxd");
        assert!(result.is_err());
    }
}
