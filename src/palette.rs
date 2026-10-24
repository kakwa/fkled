#[derive(Clone, Debug)]
pub struct Palette {
    pub colors: [(u8, u8, u8); 16],
}

impl Default for Palette {
    fn default() -> Self {
        // Palette from palette16_lines_96x16.gif
        Self {
            colors: [
                (0x00, 0x00, 0x00), // 0: black
                (0x55, 0x55, 0x55), // 1: dark gray
                (0xaa, 0xaa, 0xaa), // 2: light gray
                (0xff, 0xff, 0xff), // 3: white
                (0xaa, 0x00, 0x00), // 4: dark red
                (0xff, 0x55, 0x55), // 5: light red
                (0xaa, 0x55, 0x00), // 6: brown/orange
                (0xff, 0xff, 0x55), // 7: yellow
                (0x00, 0xaa, 0x00), // 8: dark green
                (0x55, 0xff, 0x55), // 9: light green
                (0x00, 0xaa, 0xaa), // 10: cyan
                (0x55, 0xff, 0xff), // 11: light cyan
                (0x00, 0x00, 0xaa), // 12: dark blue
                (0x55, 0x55, 0xff), // 13: light blue
                (0xaa, 0x00, 0xaa), // 14: magenta
                (0xff, 0x55, 0xff), // 15: light magenta
            ],
        }
    }
}

impl Palette {
    pub fn to_egui_color(&self, idx: usize) -> egui::Color32 {
        if idx >= 16 {
            return egui::Color32::BLACK;
        }
        let (r, g, b) = self.colors[idx];
        egui::Color32::from_rgb(r, g, b)
    }
}
