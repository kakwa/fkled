mod export;
mod model;
mod palette;
mod pcf;
mod ui;
#[cfg(target_arch = "wasm32")]
mod web;

use eframe::egui;
use model::{Animation, Clipboard};
use pcf::PcfFont;
#[cfg(not(target_arch = "wasm32"))]
use std::path::PathBuf;
use std::time::Instant;

#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1400.0, 900.0])
            .with_title("LEDFX Editor"),
        ..Default::default()
    };
    eframe::run_native(
        "LEDFX Editor",
        options,
        Box::new(|_cc| Ok(Box::new(LedFxApp::default()))),
    )
}

#[cfg(target_arch = "wasm32")]
fn main() {}

pub struct LedFxApp {
    pub animation: Animation,
    pub current_frame: usize,
    pub selected_color: usize,
    pub playing: bool,
    pub last_frame_time: Instant,
    pub brush_size: usize,
    pub tool: Tool,
    pub clipboard: Clipboard,
    pub onion_skinning: bool,
    pub onion_skin_alpha: f32,
    pub font: Option<PcfFont>,
    #[cfg(not(target_arch = "wasm32"))]
    pub font_path: Option<PathBuf>,
    #[cfg(target_arch = "wasm32")]
    pub font_path: Option<String>,
    pub text_input: String,
    pub text_x: i32,
    pub text_y: i32,
    pub text_color: u8,
    pub scroll_text: String,
    pub scroll_speed: i32,
    pub scroll_gap: i32,
    pub copy_shift_x: i32,
    pub copy_shift_y: i32,
}

impl Default for LedFxApp {
    fn default() -> Self {
        Self {
            animation: Animation::default(),
            current_frame: 0,
            selected_color: 3,
            playing: false,
            last_frame_time: Instant::now(),
            brush_size: 1,
            tool: Tool::default(),
            clipboard: Clipboard::default(),
            onion_skinning: false,
            onion_skin_alpha: 0.3,
            font: None,
            font_path: None,
            text_input: String::new(),
            text_x: 0,
            text_y: 0,
            text_color: 3,
            scroll_text: String::new(),
            scroll_speed: 1,
            scroll_gap: 2,
            copy_shift_x: 0,
            copy_shift_y: 0,
        }
    }
}

impl Default for Clipboard {
    fn default() -> Self {
        Self {
            pixels: [[0u8; 96]; 16],
            has_data: false,
        }
    }
}

#[derive(Default, PartialEq)]
pub enum Tool {
    #[default]
    Pencil,
    Eraser,
    Fill,
}

impl eframe::App for LedFxApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Handle animation playback
        if self.playing && !self.animation.frames.is_empty() {
            let now = Instant::now();
            let duration = std::time::Duration::from_millis(
                self.animation.frames[self.current_frame].duration_ms as u64,
            );
            if now.duration_since(self.last_frame_time) >= duration {
                self.current_frame = (self.current_frame + 1) % self.animation.frames.len();
                self.last_frame_time = now;
                ctx.request_repaint();
            }
        }

        ui::draw_ui(self, ctx);
    }
}
