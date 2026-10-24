use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Frame {
    #[serde(
        serialize_with = "serialize_pixels",
        deserialize_with = "deserialize_pixels"
    )]
    pub pixels: [[u8; 96]; 16], // [row][col] = palette index
    pub duration_ms: u32,
}

fn serialize_pixels<S>(
    pixels: &[[u8; 96]; 16],
    serializer: S,
) -> std::result::Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    let mut flat = Vec::with_capacity(16 * 96);
    for row in pixels.iter() {
        for &val in row.iter() {
            flat.push(val);
        }
    }
    flat.serialize(serializer)
}

fn deserialize_pixels<'de, D>(deserializer: D) -> std::result::Result<[[u8; 96]; 16], D::Error>
where
    D: serde::Deserializer<'de>,
{
    let flat: Vec<u8> = Vec::deserialize(deserializer)?;
    let mut pixels = [[0u8; 96]; 16];
    let mut idx = 0;
    for row in 0..16 {
        for col in 0..96 {
            if idx < flat.len() {
                pixels[row][col] = flat[idx] & 0x0F;
            }
            idx += 1;
        }
    }
    Ok(pixels)
}

impl Default for Frame {
    fn default() -> Self {
        Self {
            pixels: [[0u8; 96]; 16],
            duration_ms: 100,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Animation {
    pub frames: Vec<Frame>,
    pub name: String,
}

impl Default for Animation {
    fn default() -> Self {
        Self {
            frames: vec![Frame::default()],
            name: "Untitled".to_string(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Clipboard {
    pub pixels: [[u8; 96]; 16],
    pub has_data: bool,
}
