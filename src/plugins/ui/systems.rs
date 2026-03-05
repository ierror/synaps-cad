use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};

use crate::plugins::ai_chat::ChatState;
use crate::plugins::compilation::LastCompiledParts;
use crate::plugins::ui::resources::{FilePickerState, AppErrors, ExportState, SplashScreen};
use crate::plugins::ui::utils::{load_image_as_chat_image, IMAGE_EXTENSIONS};
pub use crate::plugins::ui::theme::{set_window_icon, SPLASH_IMAGE_BYTES};

pub use crate::plugins::ui::layout::ui_layout_system;
pub use crate::plugins::ui::viewport::{viewport_toolbar_system, cheatsheet_system, draw_part_labels, draw_axis_indicator};

const SPLASH_DURATION: f32 = 0.75;
pub const SPLASH_FADE_DURATION: f32 = 0.3;

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub fn splash_screen_system(
    mut contexts: EguiContexts,
    mut splash: ResMut<SplashScreen>,
    time: Res<Time>,
    mouse_button: Res<ButtonInput<MouseButton>>,
    keyboard: Res<ButtonInput<KeyCode>>,
) {
    if splash.timer <= -SPLASH_FADE_DURATION { return; }

    if !splash.dismissing && (mouse_button.just_pressed(MouseButton::Left) || keyboard.get_just_pressed().len() > 0) && splash.timer < SPLASH_DURATION - 0.2 {
        splash.dismissing = true; splash.timer = 0.0;
    }

    splash.timer -= time.delta_secs();
    let Some(ctx) = contexts.try_ctx_mut() else { return; };

    if splash.texture.is_none() {
        let image = image::load_from_memory(SPLASH_IMAGE_BYTES).expect("Failed to decode splash image");
        let rgba = image.to_rgba8();
        let color_image = egui::ColorImage::from_rgba_unmultiplied([rgba.width() as usize, rgba.height() as usize], rgba.as_raw());
        splash.texture = Some(ctx.load_texture("splash", color_image, egui::TextureOptions::LINEAR));
    }

    let Some(ref texture) = splash.texture else { return; };
    let alpha = if splash.timer < 0.0 { ((splash.timer + SPLASH_FADE_DURATION) / SPLASH_FADE_DURATION).clamp(0.0, 1.0) } else { 1.0 };
    if alpha <= 0.0 { return; }

    let screen_rect = ctx.screen_rect();
    egui::Area::new(egui::Id::new("splash_screen")).fixed_pos(screen_rect.min).order(egui::Order::Tooltip).interactable(false).show(ctx, |ui| {
        ui.painter().rect_filled(screen_rect, 0.0, egui::Color32::from_rgba_premultiplied(24, 24, 36, (alpha * 240.0) as u8));
        let tex_size = texture.size_vec2();
        let max_dim = screen_rect.height().min(screen_rect.width()) * 0.5;
        let scale = max_dim / tex_size.x.max(tex_size.y);
        let img_size = tex_size * scale;
        let img_rect = egui::Rect::from_center_size(screen_rect.center(), img_size);
        ui.put(img_rect, egui::Image::new(egui::load::SizedTexture::new(texture.id(), img_size)).tint(egui::Color32::from_rgba_unmultiplied(255, 255, 255, (alpha * 255.0) as u8)).corner_radius(egui::CornerRadius::same(16)));
        ui.painter().text(egui::pos2(screen_rect.center().x, img_rect.max.y + 20.0), egui::Align2::CENTER_TOP, format!("SynapsCAD v{}", env!("CARGO_PKG_VERSION")), egui::FontId::proportional(20.0), egui::Color32::from_rgba_unmultiplied(220, 220, 230, (alpha * 255.0) as u8));
    });
}

pub fn poll_file_picker_system(mut file_picker: ResMut<FilePickerState>, mut chat_state: ResMut<ChatState>) {
    let paths = file_picker.receiver.as_ref().and_then(|rx_mutex| rx_mutex.lock().unwrap().try_recv().ok());

    if let Some(paths) = paths {
        file_picker.receiver = None;
        for path in paths { if let Some(img) = load_image_as_chat_image(&path) { chat_state.pending_images.push(img); } }
    }
}

pub fn poll_export_system(mut export_state: ResMut<ExportState>, last_parts: Res<LastCompiledParts>, mut app_errors: ResMut<AppErrors>) {
    let maybe_path = export_state.receiver.as_ref().and_then(|rx_mutex| rx_mutex.lock().unwrap().try_recv().ok());

    if let Some(maybe_path) = maybe_path {
        let format = export_state.pending_format.take(); export_state.receiver = None;
        if let (Some(path), Some(fmt)) = (maybe_path, format) {
            match crate::export::export_parts(&last_parts.parts, &path, fmt) {
                Ok(()) => println!("[SynapsCAD] Exported to {}", path.display()),
                Err(e) => { eprintln!("[SynapsCAD] Export error: {e}"); app_errors.push(format!("Export failed: {e}")); }
            }
        }
    }
}

pub fn file_drop_system(mut dnd_events: EventReader<bevy::window::FileDragAndDrop>, mut chat_state: ResMut<ChatState>) {
    for event in dnd_events.read() {
        if let bevy::window::FileDragAndDrop::DroppedFile { path_buf, .. } = event
            && path_buf.extension().and_then(|e| e.to_str()).is_some_and(|ext| IMAGE_EXTENSIONS.contains(&ext.to_lowercase().as_str()))
                && let Some(img) = load_image_as_chat_image(path_buf) { chat_state.pending_images.push(img); }
    }
}
