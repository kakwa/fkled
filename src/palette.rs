/// Number of colors the LED screen can display.
pub const COLOR_COUNT: usize = 8;

/// Index of black. This is also the editor's background/eraser color.
pub const BLACK: u8 = 0;

/// The eight colours of the hardware palette.
///
/// The panel displays only these colours. Index bits are BGR: bit 2 = blue,
/// bit 1 = green, bit 0 = red. Indices `8–15` are not displayable and are
/// normalized to black.
///
/// The front-end mirrors this table in `src/palette.ts`; keeping it here makes
/// the server-side GIF exporter and the editor render the same colours.
pub const COLORS: [(u8, u8, u8); COLOR_COUNT] = [
    (0x00, 0x00, 0x00), // 0: black
    (0xff, 0x00, 0x00), // 1: red
    (0x00, 0xff, 0x00), // 2: green
    (0xff, 0xff, 0x00), // 3: yellow
    (0x00, 0x00, 0xff), // 4: blue
    (0xff, 0x00, 0xff), // 5: magenta
    (0x00, 0xff, 0xff), // 6: cyan
    (0xff, 0xff, 0xff), // 7: white
];

#[derive(Clone, Debug)]
pub struct Palette {
    pub colors: [(u8, u8, u8); COLOR_COUNT],
}

impl Default for Palette {
    fn default() -> Self {
        // Identified from the horizontal color bars in USB.FXD. The bars are
        // ordered 0,7,1,3,2,6,4,5 from left to right and display as
        // black, white, red, yellow, green, cyan, blue, magenta.
        Self { colors: COLORS }
    }
}

impl Palette {
    pub fn name(&self, idx: usize) -> &'static str {
        match idx {
            0 => "Black",
            1 => "Red",
            2 => "Green",
            3 => "Yellow",
            4 => "Blue",
            5 => "Magenta",
            6 => "Cyan",
            7 => "White",
            _ => "Unsupported",
        }
    }
}

/// Clamp a stored palette index to the 8 colors the screen supports.
/// Unsupported values become black, which is the editor's background color.
pub fn normalize(idx: u8) -> u8 {
    if (idx as usize) < COLOR_COUNT {
        idx
    } else {
        BLACK
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hardware_palette_matches_observed_color_bars() {
        let palette = Palette::default();
        assert_eq!(palette.colors[0], (0x00, 0x00, 0x00)); // black
        assert_eq!(palette.colors[1], (0xff, 0x00, 0x00)); // red
        assert_eq!(palette.colors[2], (0x00, 0xff, 0x00)); // green
        assert_eq!(palette.colors[3], (0xff, 0xff, 0x00)); // yellow
        assert_eq!(palette.colors[4], (0x00, 0x00, 0xff)); // blue
        assert_eq!(palette.colors[5], (0xff, 0x00, 0xff)); // magenta
        assert_eq!(palette.colors[6], (0x00, 0xff, 0xff)); // cyan
        assert_eq!(palette.colors[7], (0xff, 0xff, 0xff)); // white
    }

    #[test]
    fn unsupported_indices_become_black() {
        assert_eq!(normalize(0), 0);
        assert_eq!(normalize(7), 7);
        assert_eq!(normalize(8), BLACK);
        assert_eq!(normalize(15), BLACK);
    }
}
