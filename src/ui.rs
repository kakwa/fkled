use crate::export;
use crate::model::{Animation, Frame};
use crate::palette::Palette;
use crate::pcf;
use crate::{LedFxApp, Tool};
use egui::{Color32, Pos2, Rect, Sense, Stroke, Vec2};
#[cfg(not(target_arch = "wasm32"))]
use rfd::FileDialog;
#[cfg(not(target_arch = "wasm32"))]
use rfd;
use std::time::Instant;

pub fn draw_ui(app: &mut LedFxApp, ctx: &egui::Context) {
    let palette = Palette::default();

    // Menu bar
    egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
        egui::menu::bar(ui, |ui| {
            ui.menu_button("File", |ui| {
                if ui.button("New").clicked() {
                    app.animation = Animation::default();
                    app.current_frame = 0;
                    ui.close_menu();
                }
                #[cfg(not(target_arch = "wasm32"))] {
                if ui.button("Open Project (.ledfx.json)").clicked() {
                    if let Some(path) = FileDialog::new()
                        .add_filter("LEDFX Project", &["json"])
                        .pick_file()
                    {
                        if let Ok(anim) = export::load_project(path.to_str().unwrap()) {
                            app.animation = anim;
                            app.current_frame = 0;
                        }
                    }
                    ui.close_menu();
                }
                if ui.button("Save Project (.ledfx.json)").clicked() {
                    if let Some(path) = FileDialog::new()
                        .add_filter("LEDFX Project", &["json"])
                        .set_file_name("animation.ledfx.json")
                        .save_file()
                    {
                        let _ = export::save_project(&app.animation, path.to_str().unwrap());
                    }
                    ui.close_menu();
                }
                ui.separator();
                if ui.button("Import USB.FXD").clicked() {
                    if let Some(path) = FileDialog::new()
                        .add_filter("USB.FXD", &["FXD"])
                        .pick_file()
                    {
                        match export::import_usb_fxd(path.to_str().unwrap()) {
                            Ok(anim) => {
                                app.animation = anim;
                                app.current_frame = 0;
                            }
                            Err(err) => {
                                rfd::MessageDialog::new()
                                    .set_level(rfd::MessageLevel::Error)
                                    .set_title("USB.FXD import failed")
                                    .set_description(err.to_string())
                                    .show();
                            }
                        }
                    }
                    ui.close_menu();
                }
                if ui.button("Export USB.FXD").clicked() {
                    if let Some(path) = FileDialog::new()
                        .add_filter("USB.FXD", &["FXD"])
                        .set_file_name("output.FXD")
                        .save_file()
                    {
                        if let Err(err) =
                            export::export_usb_fxd(&app.animation, path.to_str().unwrap())
                        {
                            rfd::MessageDialog::new()
                                .set_level(rfd::MessageLevel::Error)
                                .set_title("USB.FXD export failed")
                                .set_description(err.to_string())
                                .show();
                        }
                    }
                    ui.close_menu();
                }
                }
                #[cfg(target_arch = "wasm32")] {
                    ui.label("Use browser file operations");
                    ui.close_menu();
                }
                ui.separator();
                #[cfg(not(target_arch = "wasm32"))] {
                if ui.button("Import GIF").clicked() {
                    if let Some(path) = FileDialog::new().add_filter("GIF", &["gif"]).pick_file() {
                        if let Ok(anim) = export::import_gif(path.to_str().unwrap()) {
                            app.animation = anim;
                            app.current_frame = 0;
                        }
                    }
                    ui.close_menu();
                }
                if ui.button("Export GIF").clicked() {
                    if let Some(path) = FileDialog::new()
                        .add_filter("GIF", &["gif"])
                        .set_file_name("animation.gif")
                        .save_file()
                    {
                        let _ = export::export_gif(&app.animation, path.to_str().unwrap());
                    }
                    ui.close_menu();
                }
                }
                #[cfg(target_arch = "wasm32")] {
                    ui.label("GIF ops unavailable in browser");
                    ui.close_menu();
                }
            });
        });
    });

    // Side panels
    egui::SidePanel::left("tools").show(ctx, |ui| {
        ui.heading("Tools");
        ui.horizontal(|ui| {
            ui.selectable_value(&mut app.tool, Tool::Pencil, "Pencil");
            ui.selectable_value(&mut app.tool, Tool::Eraser, "Eraser");
            ui.selectable_value(&mut app.tool, Tool::Fill, "Fill");
        });
        ui.label("Brush Size:");
        ui.add(egui::Slider::new(&mut app.brush_size, 1..=3));

        ui.separator();
        ui.heading("Palette");
        let mut color_idx = 0;
        for row in 0..4 {
            ui.horizontal(|ui| {
                for col in 0..4 {
                    let idx = row * 4 + col;
                    let color = palette.to_egui_color(idx);
                    let is_selected = app.selected_color == idx;
                    let button = egui::Button::new(" ").fill(color).stroke(if is_selected {
                        Stroke::new(2.0, Color32::WHITE)
                    } else {
                        Stroke::new(1.0, Color32::GRAY)
                    });
                    if ui.add(button).clicked() {
                        app.selected_color = idx;
                    }
                    color_idx += 1;
                }
            });
        }
        ui.label(format!("Selected: {}", app.selected_color));

        ui.separator();
        ui.heading("Copy/Paste");
        if ui.button("Copy Frame").clicked() && !app.animation.frames.is_empty() {
            app.clipboard.pixels = app.animation.frames[app.current_frame].pixels;
            app.clipboard.has_data = true;
        }
        if ui.button("Paste Frame").clicked()
            && app.clipboard.has_data
            && !app.animation.frames.is_empty()
        {
            app.animation.frames[app.current_frame].pixels = app.clipboard.pixels;
        }
        ui.label("Shift on paste:");
        ui.horizontal(|ui| {
            ui.label("X:");
            ui.add(egui::DragValue::new(&mut app.copy_shift_x).speed(1));
            ui.label("Y:");
            ui.add(egui::DragValue::new(&mut app.copy_shift_y).speed(1));
        });
        if ui.button("Paste with Shift").clicked()
            && app.clipboard.has_data
            && !app.animation.frames.is_empty()
        {
            let mut new_pixels = [[0u8; 96]; 16];
            for row in 0..16 {
                for col in 0..96 {
                    let src_row = (row as i32 - app.copy_shift_y) as i32;
                    let src_col = (col as i32 - app.copy_shift_x) as i32;
                    if src_row >= 0 && src_row < 16 && src_col >= 0 && src_col < 96 {
                        new_pixels[row][col] =
                            app.clipboard.pixels[src_row as usize][src_col as usize];
                    }
                }
            }
            app.animation.frames[app.current_frame].pixels = new_pixels;
        }
    });

    egui::SidePanel::right("text").show(ctx, |ui| {
        ui.heading("Text & Fonts");
        #[cfg(not(target_arch = "wasm32"))] {
        if ui.button("Load PCF Font (.pcf/.pcf.gz)").clicked() {
            if let Some(path) = FileDialog::new()
                .add_filter("PCF Font", &["pcf", "gz"])
                .pick_file()
            {
                if let Ok(font) = pcf::load_pcf(path.to_str().unwrap()) {
                    app.font = Some(font);
                    app.font_path = Some(path);
                }
            }
        }
        if app.font.is_some() {
            ui.label("Font loaded ✓");
        }
        }
        #[cfg(target_arch = "wasm32")] {
            ui.label("Font loading unavailable in browser");
        }

        ui.separator();
        ui.heading("Insert Text");
        ui.text_edit_singleline(&mut app.text_input);
        ui.horizontal(|ui| {
            ui.label("X:");
            ui.add(egui::DragValue::new(&mut app.text_x));
            ui.label("Y:");
            ui.add(egui::DragValue::new(&mut app.text_y));
        });
        ui.label("Color:");
        ui.add(egui::Slider::new(&mut app.text_color, 0..=15));
        if ui.button("Render Text to Frame").clicked() && !app.animation.frames.is_empty() {
            if let Some(ref font) = app.font {
                pcf::render_text(
                    font,
                    &app.text_input,
                    app.text_x,
                    app.text_y,
                    app.text_color,
                    &mut app.animation.frames[app.current_frame].pixels,
                );
            }
        }

        ui.separator();
        ui.heading("Scrolling Marquee");
        ui.text_edit_singleline(&mut app.scroll_text);
        ui.horizontal(|ui| {
            ui.label("Speed:");
            ui.add(egui::DragValue::new(&mut app.scroll_speed));
            ui.label("Gap:");
            ui.add(egui::DragValue::new(&mut app.scroll_gap));
        });
        if ui.button("Generate Scrolling Frames").clicked() {
            if let Some(ref font) = app.font {
                let text = format!(
                    "{}{}{}",
                    app.scroll_text,
                    " ".repeat(app.scroll_gap as usize),
                    app.scroll_text
                );
                let text_width = pcf::get_text_width(font, &text);
                let mut frames = Vec::new();
                for offset in (-(text_width)..96).step_by(app.scroll_speed.max(1) as usize) {
                    let mut frame = Frame {
                        pixels: [[0u8; 96]; 16],
                        duration_ms: 50,
                    };
                    pcf::render_text(
                        font,
                        &text,
                        offset,
                        app.text_y,
                        app.text_color,
                        &mut frame.pixels,
                    );
                    frames.push(frame);
                }
                if !frames.is_empty() {
                    app.animation.frames = frames;
                    app.current_frame = 0;
                }
            }
        }

        ui.separator();
        ui.heading("Preview");
        ui.checkbox(&mut app.onion_skinning, "Onion Skinning");
        if app.onion_skinning {
            ui.add(egui::Slider::new(&mut app.onion_skin_alpha, 0.1..=0.5));
        }
    });

    // Bottom panel - timeline
    egui::TopBottomPanel::bottom("timeline").show(ctx, |ui| {
        ui.horizontal(|ui| {
            if ui.button("◀◀").clicked() && !app.animation.frames.is_empty() {
                app.current_frame = 0;
            }
            if ui.button("◀").clicked() && !app.animation.frames.is_empty() {
                app.current_frame = if app.current_frame == 0 {
                    app.animation.frames.len() - 1
                } else {
                    app.current_frame - 1
                };
            }
            if app.playing {
                if ui.button("⏸ Pause").clicked() {
                    app.playing = false;
                }
            } else {
                if ui.button("▶ Play").clicked() && !app.animation.frames.is_empty() {
                    app.playing = true;
                    app.last_frame_time = Instant::now();
                }
            }
            if ui.button("▶▶").clicked() && !app.animation.frames.is_empty() {
                app.current_frame = app.animation.frames.len() - 1;
            }
            ui.label(format!(
                "Frame {}/{}",
                app.current_frame + 1,
                app.animation.frames.len()
            ));
        });

        ui.horizontal(|ui| {
            if ui.button("+ Add Frame").clicked() {
                app.animation
                    .frames
                    .insert(app.current_frame + 1, Frame::default());
                app.current_frame += 1;
            }
            if ui.button("Duplicate Frame").clicked() && !app.animation.frames.is_empty() {
                let clone = app.animation.frames[app.current_frame].clone();
                app.animation.frames.insert(app.current_frame + 1, clone);
                app.current_frame += 1;
            }
            if ui.button("Delete Frame").clicked() && app.animation.frames.len() > 1 {
                app.animation.frames.remove(app.current_frame);
                if app.current_frame >= app.animation.frames.len() {
                    app.current_frame = app.animation.frames.len() - 1;
                }
            }
            if !app.animation.frames.is_empty() {
                ui.label("Duration (ms):");
                let mut dur = app.animation.frames[app.current_frame].duration_ms;
                if ui
                    .add(egui::DragValue::new(&mut dur).range(10..=1000))
                    .changed()
                {
                    app.animation.frames[app.current_frame].duration_ms = dur;
                }
            }
        });

        ui.horizontal(|ui| {
            for (i, _frame) in app.animation.frames.iter().enumerate() {
                let is_current = i == app.current_frame;
                let btn = egui::Button::new(format!("{}", i + 1)).fill(if is_current {
                    Color32::from_rgb(100, 150, 255)
                } else {
                    Color32::from_gray(60)
                });
                if ui.add(btn).clicked() {
                    app.current_frame = i;
                    app.playing = false;
                }
            }
        });
    });

    // Center panel - canvas
    egui::CentralPanel::default().show(ctx, |ui| {
        ui.heading("96 × 16 LED Panel");
        let pixel_size = 8.0;
        let canvas_width = 96.0 * pixel_size;
        let canvas_height = 16.0 * pixel_size;

        let (response, painter) = ui.allocate_painter(
            Vec2::new(canvas_width, canvas_height),
            Sense::click_and_drag(),
        );
        let canvas_rect = response.rect;

        // Draw onion skin
        if app.onion_skinning && !app.animation.frames.is_empty() {
            if app.current_frame > 0 {
                let prev_frame = &app.animation.frames[app.current_frame - 1];
                for row in 0..16 {
                    for col in 0..96 {
                        let idx = prev_frame.pixels[row][col] as usize;
                        if idx != 0 {
                            let color = palette.to_egui_color(idx);
                            let alpha = (app.onion_skin_alpha * 255.0) as u8;
                            let color = Color32::from_rgba_unmultiplied(
                                color.r(),
                                color.g(),
                                color.b(),
                                alpha,
                            );
                            let rect = Rect::from_min_size(
                                canvas_rect.min
                                    + Vec2::new(col as f32 * pixel_size, row as f32 * pixel_size),
                                Vec2::new(pixel_size - 1.0, pixel_size - 1.0),
                            );
                            painter.rect_filled(rect, 0.0, color);
                        }
                    }
                }
            }
        }

        // Draw current frame
        if !app.animation.frames.is_empty() {
            for row in 0..16 {
                for col in 0..96 {
                    let idx = app.animation.frames[app.current_frame].pixels[row][col] as usize;
                    let color = if idx == 0 {
                        Color32::from_rgb(20, 20, 20)
                    } else {
                        palette.to_egui_color(idx)
                    };
                    let rect = Rect::from_min_size(
                        canvas_rect.min
                            + Vec2::new(col as f32 * pixel_size, row as f32 * pixel_size),
                        Vec2::new(pixel_size - 1.0, pixel_size - 1.0),
                    );
                    painter.rect_filled(rect, 0.0, color);
                }
            }

            // Handle drawing
            if response.dragged() || response.clicked() {
                if let Some(pos) = response.interact_pointer_pos() {
                    let col = ((pos.x - canvas_rect.min.x) / pixel_size) as i32;
                    let row = ((pos.y - canvas_rect.min.y) / pixel_size) as i32;
                    if row >= 0 && row < 16 && col >= 0 && col < 96 {
                        let color_to_use = if app.tool == Tool::Eraser {
                            0u8
                        } else {
                            app.selected_color as u8
                        };
                        let brush = app.brush_size as i32;
                        for dr in -(brush / 2)..=(brush / 2) {
                            for dc in -(brush / 2)..=(brush / 2) {
                                let nr = row + dr;
                                let nc = col + dc;
                                if nr >= 0 && nr < 16 && nc >= 0 && nc < 96 {
                                    app.animation.frames[app.current_frame].pixels[nr as usize]
                                        [nc as usize] = color_to_use & 0x0F;
                                }
                            }
                        }
                    }
                }
            }
        }

        // Grid
        for i in 0..=96 {
            let x = canvas_rect.min.x + i as f32 * pixel_size;
            painter.line_segment(
                [
                    Pos2::new(x, canvas_rect.min.y),
                    Pos2::new(x, canvas_rect.max.y),
                ],
                Stroke::new(0.5, Color32::from_gray(40)),
            );
        }
        for i in 0..=16 {
            let y = canvas_rect.min.y + i as f32 * pixel_size;
            painter.line_segment(
                [
                    Pos2::new(canvas_rect.min.x, y),
                    Pos2::new(canvas_rect.max.x, y),
                ],
                Stroke::new(0.5, Color32::from_gray(40)),
            );
        }
    });
}
