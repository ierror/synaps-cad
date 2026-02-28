use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_egui::{EguiContexts, egui};
use egui::text::LayoutJob;
use std::sync::{Mutex, mpsc};

use super::ai_chat::{
    ADAPTER_NAMES, AiConfig, AvailableModels, ChatState, TokioRuntime, VERIFICATION_ROUND_CHOICES,
    env_var_for_adapter,
};
use super::camera::OrbitCamera;
use super::code_editor::{ScadCode, UndoHistory, detect_views, set_active_view};
use super::compilation::{CompilationState, LastCompiledParts, PartLabel};
use super::scene::{LabelVisibility, MainCamera};
use crate::export::{self, ALL_FORMATS, ExportFormat};

pub struct UiPlugin;

#[derive(Resource, Default)]
pub struct OccupiedScreenSpace {
    pub left: f32,
}

/// Async file-picker result receiver (avoids blocking the main thread).
#[derive(Resource, Default)]
struct FilePickerState {
    receiver: Option<Mutex<mpsc::Receiver<Vec<std::path::PathBuf>>>>,
}

/// State for image hover preview in chat.
#[derive(Resource, Default)]
struct ImagePreviewState {
    /// (`base64_data` key, decoded texture handle)
    active: Option<(String, egui::TextureHandle)>,
}

/// Whether the AI settings dialog window is open.
#[derive(Resource)]
struct SettingsDialogOpen(bool);

impl Default for SettingsDialogOpen {
    fn default() -> Self {
        Self(false)
    }
}

#[derive(Resource, Default)]
struct CheatsheetOpen(bool);

/// Non-fatal errors shown in the UI instead of panicking.
#[derive(Resource, Default)]
pub struct AppErrors {
    errors: Vec<AppError>,
}

struct AppError {
    message: String,
    timestamp: std::time::Instant,
}

impl AppErrors {
    #[allow(dead_code)]
    pub fn push(&mut self, message: impl Into<String>) {
        self.errors.push(AppError {
            message: message.into(),
            timestamp: std::time::Instant::now(),
        });
    }
}

/// Async export save-dialog state.
#[derive(Resource, Default)]
struct ExportState {
    /// Receives the chosen save path from the async file dialog.
    receiver: Option<Mutex<mpsc::Receiver<Option<std::path::PathBuf>>>>,
    /// The format selected for the pending export.
    pending_format: Option<ExportFormat>,
}

/// Splash screen state — shown on app startup, dismissed by click or timeout.
#[derive(Resource)]
struct SplashScreen {
    texture: Option<egui::TextureHandle>,
    timer: f32,
    dismissing: bool,
}

impl Default for SplashScreen {
    fn default() -> Self {
        Self {
            texture: None,
            timer: 3.0,
            dismissing: false,
        }
    }
}

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<OccupiedScreenSpace>()
            .init_resource::<FilePickerState>()
            .init_resource::<ImagePreviewState>()
            .init_resource::<AppErrors>()
            .init_resource::<SettingsDialogOpen>()
            .init_resource::<CheatsheetOpen>()
            .init_resource::<ExportState>()
            .init_resource::<SplashScreen>()
            .add_systems(Startup, setup_egui_theme)
            .add_systems(
                Update,
                (
                    set_window_icon,
                    splash_screen_system,
                    ui_layout_system,
                    poll_file_picker_system,
                    poll_export_system,
                    viewport_toolbar_system,
                    cheatsheet_system,
                    draw_part_labels,
                    file_drop_system,
                ),
            );
    }
}

fn setup_egui_theme(mut contexts: EguiContexts) {
    let ctx = contexts.ctx_mut();

    let mut visuals = egui::Visuals::dark();

    // Panel & window backgrounds
    let bg = egui::Color32::from_rgb(24, 24, 36);
    let panel_bg = egui::Color32::from_rgb(30, 30, 46);
    let widget_bg = egui::Color32::from_rgb(40, 40, 58);
    let accent = egui::Color32::from_rgb(100, 160, 255);
    let text_color = egui::Color32::from_rgb(220, 220, 230);
    let dim_text = egui::Color32::from_rgb(140, 140, 160);
    let separator = egui::Color32::from_rgb(55, 55, 75);

    visuals.panel_fill = panel_bg;
    visuals.window_fill = panel_bg;
    visuals.extreme_bg_color = bg;
    visuals.faint_bg_color = widget_bg;

    // Widget styling
    let rounding = egui::CornerRadius::same(6);
    let small_rounding = egui::CornerRadius::same(4);

    visuals.widgets.noninteractive.bg_fill = widget_bg;
    visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, dim_text);
    visuals.widgets.noninteractive.corner_radius = small_rounding;

    visuals.widgets.inactive.bg_fill = widget_bg;
    visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, text_color);
    visuals.widgets.inactive.corner_radius = rounding;

    visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(55, 55, 80);
    visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.0, egui::Color32::WHITE);
    visuals.widgets.hovered.corner_radius = rounding;

    visuals.widgets.active.bg_fill = accent;
    visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0, egui::Color32::WHITE);
    visuals.widgets.active.corner_radius = rounding;

    visuals.widgets.open.bg_fill = egui::Color32::from_rgb(50, 50, 72);
    visuals.widgets.open.fg_stroke = egui::Stroke::new(1.0, text_color);
    visuals.widgets.open.corner_radius = rounding;

    visuals.selection.bg_fill = accent.linear_multiply(0.3);
    visuals.selection.stroke = egui::Stroke::new(1.0, accent);

    visuals.window_corner_radius = egui::CornerRadius::same(8);
    visuals.window_stroke = egui::Stroke::new(1.0, separator);

    // Separator
    visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, separator);

    visuals.interact_cursor = Some(egui::CursorIcon::PointingHand);

    ctx.set_visuals(visuals);

    // Spacing
    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = egui::vec2(8.0, 6.0);
    style.spacing.button_padding = egui::vec2(10.0, 4.0);
    style.spacing.window_margin = egui::Margin::same(12);
    ctx.set_style(style);
}

const SPLASH_IMAGE_BYTES: &[u8] = include_bytes!("../../assets/splash@2x.png");
const SPLASH_DURATION: f32 = 3.0;
const SPLASH_FADE_DURATION: f32 = 0.5;

fn set_window_icon(
    mut contexts: EguiContexts,
    primary: Query<Entity, With<PrimaryWindow>>,
    mut done: Local<bool>,
) {
    if *done {
        return;
    }
    let Ok(entity) = primary.get_single() else {
        return;
    };
    let Some(ctx) = contexts.try_ctx_for_entity_mut(entity) else {
        return;
    };
    let image = image::load_from_memory(SPLASH_IMAGE_BYTES).expect("Failed to decode icon image");
    let icon_img = image.resize(256, 256, image::imageops::FilterType::Lanczos3);
    let rgba = icon_img.to_rgba8();
    let (w, h) = (rgba.width(), rgba.height());
    let icon_data = egui::IconData {
        rgba: rgba.into_raw(),
        width: w,
        height: h,
    };
    ctx.send_viewport_cmd(egui::ViewportCommand::Icon(Some(std::sync::Arc::new(
        icon_data,
    ))));
    *done = true;
}

fn splash_screen_system(
    mut contexts: EguiContexts,
    mut splash: ResMut<SplashScreen>,
    time: Res<Time>,
    mouse_button: Res<ButtonInput<MouseButton>>,
    keyboard: Res<ButtonInput<KeyCode>>,
) {
    if splash.timer <= -SPLASH_FADE_DURATION {
        return;
    }

    // Dismiss on click or key press
    if !splash.dismissing
        && (mouse_button.just_pressed(MouseButton::Left) || keyboard.get_just_pressed().len() > 0)
        && splash.timer < SPLASH_DURATION - 0.2
    // ignore very early clicks
    {
        splash.dismissing = true;
        splash.timer = 0.0;
    }

    splash.timer -= time.delta_secs();

    let Some(ctx) = contexts.try_ctx_mut() else {
        return;
    };

    // Load texture on first frame
    if splash.texture.is_none() {
        let image =
            image::load_from_memory(SPLASH_IMAGE_BYTES).expect("Failed to decode splash image");
        let rgba = image.to_rgba8();
        let size = [rgba.width() as usize, rgba.height() as usize];
        let pixels = rgba.into_raw();
        let color_image = egui::ColorImage::from_rgba_unmultiplied(size, &pixels);
        splash.texture =
            Some(ctx.load_texture("splash", color_image, egui::TextureOptions::LINEAR));
    }

    let Some(ref texture) = splash.texture else {
        return;
    };

    // Fade out during the last SPLASH_FADE_DURATION seconds
    let alpha = if splash.timer < 0.0 {
        ((splash.timer + SPLASH_FADE_DURATION) / SPLASH_FADE_DURATION).clamp(0.0, 1.0)
    } else {
        1.0
    };

    if alpha <= 0.0 {
        return;
    }

    let screen_rect = ctx.screen_rect();
    let bg_alpha = (alpha * 240.0) as u8;

    // Full-screen overlay
    egui::Area::new(egui::Id::new("splash_screen"))
        .fixed_pos(screen_rect.min)
        .order(egui::Order::Tooltip)
        .interactable(false)
        .show(ctx, |ui| {
            ui.painter().rect_filled(
                screen_rect,
                0.0,
                egui::Color32::from_rgba_premultiplied(24, 24, 36, bg_alpha),
            );

            // Center the image, scaled to fit nicely
            let tex_size = texture.size_vec2();
            let max_dim = screen_rect.height().min(screen_rect.width()) * 0.5;
            let scale = max_dim / tex_size.x.max(tex_size.y);
            let img_size = tex_size * scale;
            let img_rect = egui::Rect::from_center_size(screen_rect.center(), img_size);

            let tint = egui::Color32::from_rgba_unmultiplied(255, 255, 255, (alpha * 255.0) as u8);
            let rounding = egui::CornerRadius::same(16);
            ui.put(
                img_rect,
                egui::Image::new(egui::load::SizedTexture::new(texture.id(), img_size))
                    .tint(tint)
                    .corner_radius(rounding),
            );

            // App name below image
            let text_pos = egui::pos2(screen_rect.center().x, img_rect.max.y + 20.0);
            let text_color =
                egui::Color32::from_rgba_unmultiplied(220, 220, 230, (alpha * 255.0) as u8);
            ui.painter().text(
                text_pos,
                egui::Align2::CENTER_TOP,
                format!("SynapsCAD v{}", env!("CARGO_PKG_VERSION")),
                egui::FontId::proportional(20.0),
                text_color,
            );
        });
}

// OpenSCAD builtin module/function names for highlighting
const OPENSCAD_BUILTINS: &[&str] = &[
    "cube",
    "sphere",
    "cylinder",
    "polyhedron",
    "circle",
    "square",
    "polygon",
    "text",
    "translate",
    "rotate",
    "scale",
    "mirror",
    "multmatrix",
    "color",
    "offset",
    "resize",
    "union",
    "difference",
    "intersection",
    "hull",
    "minkowski",
    "linear_extrude",
    "rotate_extrude",
    "surface",
    "projection",
    "import",
    "children",
    "parent_module",
    "is_undef",
    "is_list",
    "is_num",
    "is_string",
    "is_bool",
    "len",
    "str",
    "chr",
    "ord",
    "concat",
    "lookup",
    "search",
    "abs",
    "sign",
    "sin",
    "cos",
    "tan",
    "asin",
    "acos",
    "atan",
    "atan2",
    "floor",
    "ceil",
    "round",
    "ln",
    "log",
    "pow",
    "sqrt",
    "exp",
    "min",
    "max",
    "norm",
    "cross",
    "rands",
];

fn highlight_openscad(text: &str, font_id: egui::FontId) -> LayoutJob {
    use openscad_rs::token::Token;

    let keyword_color = egui::Color32::from_rgb(198, 120, 221); // purple
    let number_color = egui::Color32::from_rgb(209, 154, 102); // orange
    let string_color = egui::Color32::from_rgb(152, 195, 121); // green
    let builtin_color = egui::Color32::from_rgb(97, 175, 239); // blue
    let comment_color = egui::Color32::from_rgb(92, 99, 112); // dim gray
    let operator_color = egui::Color32::from_rgb(171, 178, 191); // light gray
    let default_color = egui::Color32::from_rgb(220, 220, 230); // text
    let bool_color = egui::Color32::from_rgb(209, 154, 102); // orange (like numbers)
    let special_var_color = egui::Color32::from_rgb(224, 108, 117); // red for $fn etc

    let mut job = LayoutJob::default();

    let format_for = |color: egui::Color32| -> egui::TextFormat {
        egui::TextFormat {
            font_id: font_id.clone(),
            color,
            ..Default::default()
        }
    };

    let tokens = openscad_rs::lexer::lex(text);
    let mut cursor = 0;

    for (token, span) in &tokens {
        // Handle gap between tokens (whitespace and comments)
        if span.start > cursor {
            let gap = &text[cursor..span.start];
            add_gap_sections(
                &mut job,
                gap,
                cursor,
                &font_id,
                comment_color,
                default_color,
            );
        }

        let slice = &text[span.start..span.end];
        let color = match token {
            Token::Module
            | Token::Function
            | Token::If
            | Token::Else
            | Token::For
            | Token::Let
            | Token::Assert
            | Token::Echo
            | Token::Each
            | Token::Undef => keyword_color,
            Token::True | Token::False => bool_color,
            Token::Number(_) => number_color,
            Token::String(_) => string_color,
            Token::Include | Token::Use => keyword_color,
            Token::Identifier => {
                if slice.starts_with('$') {
                    special_var_color
                } else if OPENSCAD_BUILTINS.contains(&slice) {
                    builtin_color
                } else {
                    default_color
                }
            }
            // Operators and delimiters
            _ => operator_color,
        };

        job.append(slice, 0.0, format_for(color));
        cursor = span.end;
    }

    // Handle trailing content (whitespace/comments after last token)
    if cursor < text.len() {
        let gap = &text[cursor..];
        add_gap_sections(
            &mut job,
            gap,
            cursor,
            &font_id,
            comment_color,
            default_color,
        );
    }

    job
}

/// Process gaps between tokens, identifying comments and whitespace.
fn add_gap_sections(
    job: &mut LayoutJob,
    gap: &str,
    _offset: usize,
    font_id: &egui::FontId,
    comment_color: egui::Color32,
    default_color: egui::Color32,
) {
    let format_for = |color: egui::Color32| -> egui::TextFormat {
        egui::TextFormat {
            font_id: font_id.clone(),
            color,
            ..Default::default()
        }
    };

    let mut remaining = gap;
    while !remaining.is_empty() {
        if let Some(pos) = remaining.find("//") {
            // Text before the comment
            if pos > 0 {
                job.append(&remaining[..pos], 0.0, format_for(default_color));
            }
            // Line comment extends to end of line
            let comment_end = remaining[pos..]
                .find('\n')
                .map_or(remaining.len(), |n| pos + n);
            job.append(&remaining[pos..comment_end], 0.0, format_for(comment_color));
            remaining = &remaining[comment_end..];
        } else if let Some(pos) = remaining.find("/*") {
            // Text before the comment
            if pos > 0 {
                job.append(&remaining[..pos], 0.0, format_for(default_color));
            }
            // Block comment extends to */
            let comment_end = remaining[pos + 2..]
                .find("*/")
                .map_or(remaining.len(), |n| pos + 2 + n + 2);
            job.append(&remaining[pos..comment_end], 0.0, format_for(comment_color));
            remaining = &remaining[comment_end..];
        } else {
            // Plain whitespace/text
            job.append(remaining, 0.0, format_for(default_color));
            break;
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn ui_layout_system(
    mut contexts: EguiContexts,
    mut scad_code: ResMut<ScadCode>,
    mut chat_state: ResMut<ChatState>,
    mut occupied: ResMut<OccupiedScreenSpace>,
    mut ai_config: ResMut<AiConfig>,
    mut available_models: ResMut<AvailableModels>,
    compilation_state: Res<CompilationState>,
    mut file_picker: ResMut<FilePickerState>,
    runtime: Res<TokioRuntime>,
    mut preview_state: ResMut<ImagePreviewState>,
    mut app_errors: ResMut<AppErrors>,
    mut settings_open: ResMut<SettingsDialogOpen>,
    last_parts: Res<LastCompiledParts>,
    mut export_state: ResMut<ExportState>,
) {
    let Some(ctx) = contexts.try_ctx_mut() else {
        return;
    };

    let panel = egui::SidePanel::left("side_panel")
        .default_width(400.0)
        .min_width(300.0)
        .max_width(600.0)
        .resizable(true);

    let response = panel.show(ctx, |ui| {
        // Constrain content to panel width — prevents content from pushing panel wider
        let max_w = ui.available_width();
        ui.set_min_width(max_w);
        ui.set_max_width(max_w);
        // --- Error banner (auto-dismiss after 30s) ---
        app_errors.errors.retain(|e| e.timestamp.elapsed().as_secs() < 30);
        if !app_errors.errors.is_empty() {
            for err in &app_errors.errors {
                egui::Frame::new()
                    .fill(egui::Color32::from_rgb(80, 20, 20))
                    .corner_radius(egui::CornerRadius::same(4))
                    .inner_margin(egui::Margin::same(6))
                    .show(ui, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(egui::RichText::new("⚠").color(egui::Color32::YELLOW));
                            ui.label(egui::RichText::new(&err.message).color(egui::Color32::WHITE).small());
                        });
                        if ui.small_button("🔗 Report issue on GitHub").clicked() {
                            let url = format!("{}/issues/new", crate::app_config::GITHUB_REPO_URL);
                            let _ = open::that(&url);
                        }
                    });
                ui.add_space(2.0);
            }
            ui.separator();
        }

        ui.add_space(4.0);
        ui.separator();

        // --- AI Assistant (top) ---
        ui.horizontal_wrapped(|ui| {
            ui.heading("AI Assistant");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let gear_label = if available_models.needs_configuration {
                    "⚙ ⚠"
                } else {
                    "⚙"
                };
                if ui.button(gear_label).on_hover_text("AI Settings").clicked() {
                    settings_open.0 = !settings_open.0;
                }
                if ui.button("🗑").on_hover_text("Clear chat").clicked() {
                    chat_state.messages.clear();
                    chat_state.input_history.clear();
                    chat_state.history_index = None;
                    chat_state.pending_images.clear();
                }
                // Verify rounds selector
                let selected_label = if ai_config.max_verification_rounds == u32::MAX {
                    "∞".to_string()
                } else {
                    ai_config.max_verification_rounds.to_string()
                };
                egui::ComboBox::from_id_salt("verify_rounds_main")
                    .selected_text(selected_label)
                    .width(36.0)
                    .show_ui(ui, |ui| {
                        for &n in VERIFICATION_ROUND_CHOICES {
                            let label = if n == u32::MAX {
                                "∞".to_string()
                            } else {
                                n.to_string()
                            };
                            ui.selectable_value(
                                &mut ai_config.max_verification_rounds,
                                n,
                                label,
                            );
                        }
                    })
                    .response
                    .on_hover_text("Verification rounds: after generating code, the AI reviews the rendered result and self-corrects if needed. Set how many automatic correction rounds are allowed (∞ = unlimited).");
                ui.label(egui::RichText::new("Verify").small());
            });
        });
        ui.separator();

        // Pending image attachments strip
        if !chat_state.pending_images.is_empty() {
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    egui::RichText::new("Attached:")
                        .small()
                        .color(egui::Color32::from_rgb(140, 140, 160)),
                );
                let mut to_remove = None;
                for (i, img) in chat_state.pending_images.iter().enumerate() {
                    let frame_resp = egui::Frame::new()
                        .fill(egui::Color32::from_rgb(40, 40, 58))
                        .corner_radius(egui::CornerRadius::same(4))
                        .inner_margin(egui::Margin::symmetric(4, 2))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                let label_resp = ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(&img.filename)
                                            .small()
                                            .color(egui::Color32::from_rgb(180, 180, 200)),
                                    )
                                    .sense(egui::Sense::hover()),
                                );
                                if ui.small_button("x").clicked() {
                                    to_remove = Some(i);
                                }
                                label_resp
                            })
                        });
                    if frame_resp.inner.inner.hovered() {
                        show_image_preview(ui, img, &mut preview_state);
                    }
                }
                if let Some(idx) = to_remove {
                    chat_state.pending_images.remove(idx);
                }
            });
            ui.add_space(2.0);
        }

        ui.horizontal_wrapped(|ui| {
            if ui
                .button("📎")
                .on_hover_text("Attach image")
                .clicked()
                && file_picker.receiver.is_none()
            {
                let (tx, rx) = mpsc::channel();
                file_picker.receiver = Some(Mutex::new(rx));
                runtime.0.spawn(async move {
                    let handles = rfd::AsyncFileDialog::new()
                        .add_filter("Images", &["png", "jpg", "jpeg", "gif", "webp", "bmp"])
                        .pick_files()
                        .await;
                    let paths: Vec<std::path::PathBuf> = handles
                        .unwrap_or_default()
                        .into_iter()
                        .map(|h| h.path().to_path_buf())
                        .collect();
                    let _ = tx.send(paths);
                });
            }

            let input_response = ui.add(
                egui::TextEdit::singleline(&mut chat_state.input_buffer)
                    .hint_text("Ask the AI assistant...")
                    .desired_width(ui.available_width() - 60.0),
            );

            // Arrow key history navigation when input is focused
            #[allow(clippy::assigning_clones)]
            if input_response.has_focus() {
                if ui.input(|i| i.key_pressed(egui::Key::ArrowUp)) {
                    if !chat_state.input_history.is_empty() {
                        let idx = chat_state.history_index.map_or_else(
                            || chat_state.input_history.len() - 1,
                            |i| i.saturating_sub(1),
                        );
                        chat_state.history_index = Some(idx);
                        chat_state.input_buffer =
                            chat_state.input_history[idx].clone();
                    }
                } else if ui.input(|i| i.key_pressed(egui::Key::ArrowDown))
                    && let Some(idx) = chat_state.history_index {
                        if idx + 1 < chat_state.input_history.len() {
                            let new_idx = idx + 1;
                            chat_state.history_index = Some(new_idx);
                            chat_state.input_buffer =
                                chat_state.input_history[new_idx].clone();
                        } else {
                            chat_state.history_index = None;
                            chat_state.input_buffer.clear();
                        }
                    }
            }

            // Show Stop button while streaming, Send button otherwise
            if chat_state.is_streaming {
                if ui.button("⏹ Stop").clicked() {
                    chat_state.is_streaming = false;
                    chat_state.stream_receiver = None;
                    chat_state.verification = super::ai_chat::VerificationState::Idle;
                }
            } else {
                let send_clicked = ui.button("Send").clicked();
                let enter_pressed = input_response.lost_focus()
                    && ui.input(|i| i.key_pressed(egui::Key::Enter));

                if (send_clicked || enter_pressed)
                    && !chat_state.input_buffer.trim().is_empty()
                {
                    let user_msg = chat_state.input_buffer.trim().to_string();
                    let images = std::mem::take(&mut chat_state.pending_images);
                    chat_state.input_history.push(user_msg.clone());
                    chat_state.history_index = None;
                    chat_state.messages.push(super::ai_chat::ChatMessage {
                        role: "user".into(),
                        content: user_msg,
                        thinking: None,
                        images,
                    });
                    chat_state.input_buffer.clear();
                    chat_state.is_streaming = true;
                }
            }
        });

        // Compute fixed split for chat/code panes
        let total_remaining = ui.available_height();
        let chat_height = (total_remaining * 0.45).max(50.0);

        egui::ScrollArea::vertical()
            .id_salt("chat_scroll")
            .max_height(chat_height)
            .show(ui, |ui| {
                // Status indicators at top (newest-first layout)
                if chat_state.is_streaming {
                    ui.spinner();
                }
                match &chat_state.verification {
                    super::ai_chat::VerificationState::WaitingForCompilation => {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(
                                egui::RichText::new("Compiling... will verify result")
                                    .small()
                                    .italics()
                                    .color(egui::Color32::from_rgb(140, 140, 160)),
                            );
                        });
                    }
                    super::ai_chat::VerificationState::Verifying(round) => {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(
                                egui::RichText::new(format!("Verifying (round {round})..."))
                                    .small()
                                    .italics()
                                    .color(egui::Color32::from_rgb(100, 160, 255)),
                            );
                        });
                    }
                    _ => {}
                }

                // Messages in reverse order (newest first)
                let msg_count = chat_state.messages.len();
                for (rev_i, msg) in chat_state.messages.iter().rev().enumerate() {
                    let msg_idx = msg_count - 1 - rev_i;
                    let is_user = msg.role == "user";
                    let (prefix, color) = if is_user {
                        ("You", egui::Color32::from_rgb(100, 160, 255))
                    } else {
                        ("AI", egui::Color32::from_rgb(130, 220, 130))
                    };

                    // Build header text: for user messages show truncated preview
                    let header_text = if is_user {
                        let preview: String = msg.content.chars().take(80).collect();
                        let ellipsis = if msg.content.len() > 80 { "…" } else { "" };
                        format!("{prefix}: {preview}{ellipsis}")
                    } else {
                        format!("{prefix}:")
                    };

                    let id = ui.make_persistent_id(format!("chat_msg_{msg_idx}"));
                    let default_open = !is_user;
                    let state = egui::collapsing_header::CollapsingState::load_with_default_open(
                        ui.ctx(),
                        id,
                        default_open,
                    );
                    state
                        .show_header(ui, |ui| {
                            ui.label(egui::RichText::new(&header_text).strong().color(color));
                        })
                        .body(|ui| {
                            // Show thinking/reasoning if available (collapsible)
                            if let Some(ref thinking) = msg.thinking {
                                ui.collapsing(
                                    egui::RichText::new("💭 Thinking…")
                                        .italics()
                                        .color(egui::Color32::from_rgb(180, 180, 180)),
                                    |ui| {
                                        ui.label(
                                            egui::RichText::new(thinking)
                                                .italics()
                                                .color(egui::Color32::from_rgb(150, 150, 150)),
                                        );
                                    },
                                );
                            }
                            ui.horizontal_wrapped(|ui| {
                                ui.label(&msg.content);
                            });
                            if !msg.images.is_empty() {
                                for img in &msg.images {
                                    let frame_resp = egui::Frame::new()
                                        .fill(egui::Color32::from_rgb(40, 40, 58))
                                        .corner_radius(egui::CornerRadius::same(3))
                                        .inner_margin(egui::Margin::symmetric(4, 2))
                                        .show(ui, |ui| {
                                            ui.add(
                                                egui::Label::new(
                                                    egui::RichText::new(format!("📷 {}", img.filename))
                                                        .small()
                                                        .color(egui::Color32::from_rgb(160, 160, 180)),
                                                )
                                                .sense(egui::Sense::hover()),
                                            )
                                        });
                                    if frame_resp.inner.hovered() {
                                        show_image_preview(ui, img, &mut preview_state);
                                    }
                                }
                            }
                        });
                    ui.add_space(2.0);
                }
            });

        ui.add_space(4.0);
        ui.separator();

        // --- Code Editor (below chat) ---
        ui.horizontal_wrapped(|ui| {
            ui.heading("Code");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let compile_label = if compilation_state.is_compiling {
                    "Compiling..."
                } else {
                    "Compile"
                };
                if ui
                    .add_enabled(!compilation_state.is_compiling, egui::Button::new(compile_label))
                    .clicked()
                {
                    scad_code.dirty = true;
                }
                if ui.button("🗑").on_hover_text("Clear code").clicked() {
                    scad_code.text.clear();
                    scad_code.dirty = true;
                }
                // Export menu
                let has_parts = !last_parts.parts.is_empty();
                let is_exporting = export_state.receiver.is_some();
                if is_exporting {
                    ui.spinner();
                } else {
                    let export_btn = ui.add_enabled(
                        has_parts,
                        egui::Button::new("💾"),
                    );
                    let export_btn = export_btn.on_hover_text(if has_parts {
                        "Export model (STL, OBJ, 3MF)"
                    } else {
                        "Compile a model first to export"
                    });
                    let popup_id = ui.make_persistent_id("export_popup");
                    if export_btn.clicked() && has_parts {
                        ui.memory_mut(|m| m.toggle_popup(popup_id));
                    }
                    if ui.memory(|m| m.is_popup_open(popup_id)) {
                        let area_resp = egui::Area::new(popup_id)
                            .order(egui::Order::Foreground)
                            .fixed_pos(export_btn.rect.left_bottom())
                            .show(ui.ctx(), |ui| {
                                egui::Frame::popup(ui.style()).show(ui, |ui| {
                                    ui.set_min_width(160.0);
                                    for &fmt in ALL_FORMATS {
                                        if ui.button(fmt.label()).clicked() {
                                            let ext = fmt.extension();
                                            let (tx, rx) = mpsc::channel();
                                            export_state.receiver = Some(Mutex::new(rx));
                                            export_state.pending_format = Some(fmt);
                                            runtime.0.spawn(async move {
                                                let handle = rfd::AsyncFileDialog::new()
                                                    .set_file_name(format!("model.{ext}"))
                                                    .add_filter(ext.to_uppercase(), &[ext])
                                                    .save_file()
                                                    .await;
                                                let _ = tx.send(handle.map(|h| h.path().to_path_buf()));
                                            });
                                            ui.memory_mut(|m| m.close_popup());
                                        }
                                    }
                                });
                            });
                        // Close if clicked elsewhere
                        if ui.input(|i| i.pointer.any_click()) && !area_resp.response.contains_pointer() && !export_btn.contains_pointer() {
                            ui.memory_mut(|m| m.close_popup());
                        }
                    }
                }
            });
        });

        // --- View selector (shown when code defines multiple views) ---
        let (active_view, all_views) = detect_views(&scad_code.text);
        if all_views.len() > 1 {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("View:").small().color(egui::Color32::from_rgb(160, 160, 180)));
                let current = active_view.unwrap_or_default();
                for view_name in &all_views {
                    let is_active = *view_name == current;
                    let label = egui::RichText::new(view_name.as_str()).small();
                    let label = if is_active {
                        label.strong().color(egui::Color32::from_rgb(100, 160, 255))
                    } else {
                        label.color(egui::Color32::from_rgb(160, 160, 180))
                    };
                    if ui.selectable_label(is_active, label).clicked() && !is_active {
                        set_active_view(&mut scad_code.text, view_name);
                        scad_code.dirty = true;
                    }
                }
            });
        }
        ui.separator();

        let editor_height = (total_remaining * 0.55).max(80.0);

        let mut highlighter = |ui: &egui::Ui, text: &str, wrap_width: f32| {
            let font_id = egui::TextStyle::Monospace.resolve(ui.style());
            let mut layout_job = highlight_openscad(text, font_id);
            layout_job.wrap.max_width = wrap_width;
            ui.fonts(|f| f.layout_job(layout_job))
        };
        egui::ScrollArea::both()
            .id_salt("code_editor")
            .max_height(editor_height)
            .show(ui, |ui| {
                let response = ui.add(
                    egui::TextEdit::multiline(&mut scad_code.text)
                        .code_editor()
                        .desired_width(f32::INFINITY)
                        .desired_rows(20)
                        .layouter(&mut highlighter),
                );
                scad_code.editor_focused = response.has_focus();
                if response.changed() {
                    scad_code.changed_while_focused = true;
                }
                if response.lost_focus() && scad_code.changed_while_focused {
                    scad_code.dirty = true;
                    scad_code.changed_while_focused = false;
                }
            });

    });

    let new_left = response.response.rect.width();
    if (occupied.left - new_left).abs() > 0.5 {
        occupied.left = new_left;
    }

    // Auto-open settings dialog when model needs reconfiguration
    if available_models.needs_configuration {
        settings_open.0 = true;
    }

    // --- AI Settings dialog window ---
    egui::Window::new("⚙ AI Settings")
        .open(&mut settings_open.0)
        .resizable(true)
        .default_width(360.0)
        .collapsible(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            if available_models.needs_configuration {
                ui.colored_label(
                    egui::Color32::from_rgb(255, 180, 50),
                    "⚠ Previously configured model is no longer available. Please select a new model.",
                );
                ui.add_space(4.0);
            }
            ui.horizontal(|ui| {
                ui.label("Provider:");
                egui::ComboBox::from_id_salt("ai_adapter_select")
                    .selected_text(&ai_config.adapter_name)
                    .show_ui(ui, |ui| {
                        for &adapter in ADAPTER_NAMES {
                            ui.selectable_value(
                                &mut ai_config.adapter_name,
                                adapter.to_string(),
                                adapter,
                            );
                        }
                    });
            });
            ui.horizontal(|ui| {
                ui.label("Model:");
                // Check if adapter requires an API key and none is set
                let needs_key = env_var_for_adapter(&ai_config.adapter_name).is_some();
                let env_key_set = env_var_for_adapter(&ai_config.adapter_name)
                    .and_then(|name| std::env::var(name).ok())
                    .is_some_and(|v| !v.is_empty());
                let has_key = !needs_key || env_key_set || !ai_config.api_key.is_empty();

                if !has_key {
                    ui.colored_label(
                        egui::Color32::from_rgb(255, 180, 50),
                        "⚠ Set API key first",
                    );
                } else {
                    let model_label = if available_models.loading {
                        "Loading...".to_string()
                    } else if available_models.models.is_empty() {
                        "No models available".to_string()
                    } else {
                        ai_config.model_name.clone()
                    };
                    let prev_model = ai_config.model_name.clone();
                    egui::ComboBox::from_id_salt("ai_model_select")
                        .selected_text(model_label)
                        .show_ui(ui, |ui| {
                            for model in &available_models.models {
                                ui.selectable_value(
                                    &mut ai_config.model_name,
                                    model.clone(),
                                    model.as_str(),
                                );
                            }
                        });
                    // Clear warning when user picks a valid model
                    if ai_config.model_name != prev_model
                        && available_models.models.contains(&ai_config.model_name)
                    {
                        available_models.needs_configuration = false;
                    }
                }
            });
            if let Some(ref err) = available_models.error {
                ui.colored_label(egui::Color32::from_rgb(255, 100, 100), format!("⚠ {err}"));
            }
            ui.horizontal(|ui| {
                ui.label("API Key:");
                let env_var_name = env_var_for_adapter(&ai_config.adapter_name);
                let env_var_set = env_var_name
                    .and_then(|name| std::env::var(name).ok())
                    .is_some_and(|v| !v.is_empty());

                if env_var_set && ai_config.api_key.is_empty() {
                    ui.add_enabled(
                        false,
                        egui::TextEdit::singleline(&mut String::new())
                            .hint_text(format!("Set via {}", env_var_name.unwrap_or(""))),
                    );
                } else {
                    ui.add(
                        egui::TextEdit::singleline(&mut ai_config.api_key)
                            .password(true)
                            .hint_text(if env_var_set { "Override env var" } else { "Enter API key" }),
                    );
                }
            });
            if let Some(env_name) = env_var_for_adapter(&ai_config.adapter_name) {
                let env_set = std::env::var(env_name).ok().is_some_and(|v| !v.is_empty());
                if env_set && !ai_config.api_key.is_empty() {
                    ui.label(
                        egui::RichText::new(format!("⚠ Overriding {env_name} for this session"))
                            .color(egui::Color32::from_rgb(255, 180, 50))
                            .small(),
                    );
                } else if env_set {
                    ui.label(
                        egui::RichText::new(format!("✓ Using API key from {env_name}"))
                            .color(egui::Color32::from_rgb(100, 200, 100))
                            .small(),
                    );
                }
            }
            ui.horizontal(|ui| {
                ui.label("Temperature:").on_hover_text(
                    "Controls randomness of AI responses.\n\
                     0 = focused and deterministic\n\
                     1 = balanced (default for most models)\n\
                     2 = more creative and varied"
                );
                ui.add(egui::Slider::new(&mut ai_config.temperature, 0.0..=2.0).step_by(0.1));
            });
            ui.label("System Prompt:");
            ui.add(
                egui::TextEdit::multiline(&mut ai_config.system_prompt)
                    .desired_width(ui.available_width())
                    .desired_rows(4),
            );
        });

    // --- New Part dialog ---
}

fn poll_file_picker_system(
    mut file_picker: ResMut<FilePickerState>,
    mut chat_state: ResMut<ChatState>,
) {
    let result = {
        let Some(ref rx_mutex) = file_picker.receiver else {
            return;
        };
        let rx = rx_mutex.lock().unwrap();
        match rx.try_recv() {
            Ok(paths) => Some(paths),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => {
                drop(rx);
                file_picker.receiver = None;
                return;
            }
        }
    };
    if let Some(paths) = result {
        file_picker.receiver = None;
        for path in paths {
            if let Some(img) = load_image_as_chat_image(&path) {
                chat_state.pending_images.push(img);
            }
        }
    }
}

fn poll_export_system(
    mut export_state: ResMut<ExportState>,
    last_parts: Res<LastCompiledParts>,
    mut app_errors: ResMut<AppErrors>,
) {
    let result = {
        let Some(ref rx_mutex) = export_state.receiver else {
            return;
        };
        let rx = rx_mutex.lock().unwrap();
        match rx.try_recv() {
            Ok(path) => Some(path),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => {
                drop(rx);
                export_state.receiver = None;
                export_state.pending_format = None;
                return;
            }
        }
    };
    if let Some(maybe_path) = result {
        let format = export_state.pending_format.take();
        export_state.receiver = None;
        if let (Some(path), Some(fmt)) = (maybe_path, format) {
            match export::export_parts(&last_parts.parts, &path, fmt) {
                Ok(()) => println!("[SynapsCAD] Exported to {}", path.display()),
                Err(e) => {
                    eprintln!("[SynapsCAD] Export error: {e}");
                    app_errors.push(format!("Export failed: {e}"));
                }
            }
        }
    }
}

fn viewport_toolbar_system(
    mut contexts: EguiContexts,
    occupied: Res<OccupiedScreenSpace>,
    mut scad_code: ResMut<ScadCode>,
    mut history: ResMut<UndoHistory>,
    mut orbit: ResMut<OrbitCamera>,
    mut ruler: ResMut<super::camera::RulerState>,
    mut label_vis: ResMut<LabelVisibility>,
    mut cheatsheet: ResMut<CheatsheetOpen>,
) {
    let Some(ctx) = contexts.try_ctx_mut() else {
        return;
    };

    let toolbar_x = occupied.left + 8.0;
    let toolbar_y = 8.0;

    egui::Area::new(egui::Id::new("viewport_toolbar"))
        .fixed_pos(egui::pos2(toolbar_x, toolbar_y))
        .interactable(true)
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(egui::Color32::from_rgba_premultiplied(30, 30, 46, 220))
                .corner_radius(egui::CornerRadius::same(6))
                .inner_margin(egui::Margin::symmetric(6, 4))
                .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(55, 55, 75)))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = egui::vec2(4.0, 0.0);

                        // --- Undo / Redo ---
                        let undo_enabled = history.can_undo();
                        let redo_enabled = history.can_redo();

                        if ui
                            .add_enabled(undo_enabled, egui::Button::new("↩ Undo"))
                            .on_hover_text("Undo last code change")
                            .clicked()
                            && let Some(prev) = history.undo_stack.pop()
                        {
                            let current = scad_code.text.clone();
                            history.redo_stack.push(current);
                            prev.clone_into(&mut history.last_snapshot);
                            scad_code.text = prev;
                            scad_code.dirty = true;
                        }

                        if ui
                            .add_enabled(redo_enabled, egui::Button::new("↪ Redo"))
                            .on_hover_text("Redo last undone change")
                            .clicked()
                            && let Some(next) = history.redo_stack.pop()
                        {
                            let current = scad_code.text.clone();
                            history.undo_stack.push(current);
                            next.clone_into(&mut history.last_snapshot);
                            scad_code.text = next;
                            scad_code.dirty = true;
                        }

                        ui.separator();

                        // --- Camera view buttons ---
                        let view_btn = |ui: &mut egui::Ui, label: &str, tooltip: &str| -> bool {
                            ui.small_button(label).on_hover_text(tooltip).clicked()
                        };

                        let pi = std::f32::consts::PI;
                        let half_pi = std::f32::consts::FRAC_PI_2;
                        let quarter_pi = std::f32::consts::FRAC_PI_4;

                        if view_btn(ui, "F", "Front view (1)") {
                            orbit.yaw = 0.0;
                            orbit.pitch = 0.0;
                        }
                        if view_btn(ui, "Bk", "Back view (2)") {
                            orbit.yaw = pi;
                            orbit.pitch = 0.0;
                        }
                        if view_btn(ui, "R", "Right view (3)") {
                            orbit.yaw = half_pi;
                            orbit.pitch = 0.0;
                        }
                        if view_btn(ui, "L", "Left view (4)") {
                            orbit.yaw = -half_pi;
                            orbit.pitch = 0.0;
                        }
                        if view_btn(ui, "T", "Top view (5)") {
                            orbit.yaw = 0.0;
                            orbit.pitch = half_pi - 0.01;
                        }
                        if view_btn(ui, "Bo", "Bottom view (6)") {
                            orbit.yaw = 0.0;
                            orbit.pitch = -(half_pi - 0.01);
                        }
                        if view_btn(ui, "Iso", "Isometric view (7)") {
                            orbit.yaw = quarter_pi;
                            orbit.pitch = quarter_pi;
                        }
                        if view_btn(ui, "⊞", "Zoom to fit") {
                            orbit.zoom_to_fit = true;
                        }

                        ui.separator();

                        // --- Ruler tool ---
                        let ruler_label = if ruler.active { "📏 ✓" } else { "📏" };
                        let ruler_tooltip = if ruler.active {
                            if ruler.point_a.is_some() && ruler.point_b.is_none() {
                                "Click second point to measure (Esc to cancel)"
                            } else {
                                "Ruler active — click to start new measurement (Esc to cancel)"
                            }
                        } else {
                            "Measure distance between two points"
                        };
                        if ui
                            .selectable_label(ruler.active, ruler_label)
                            .on_hover_text(ruler_tooltip)
                            .clicked()
                        {
                            ruler.active = !ruler.active;
                            if !ruler.active {
                                ruler.point_a = None;
                                ruler.point_b = None;
                            }
                        }

                        // --- Label visibility toggle ---
                        let label_btn = if label_vis.visible { "@" } else { "@ ✗" };
                        if ui
                            .selectable_label(label_vis.visible, label_btn)
                            .on_hover_text("Toggle part labels (L)")
                            .clicked()
                        {
                            label_vis.visible = !label_vis.visible;
                        }

                        // --- Keyboard cheatsheet ---
                        if ui
                            .selectable_label(cheatsheet.0, "⌨")
                            .on_hover_text("Keyboard shortcuts (?)")
                            .clicked()
                        {
                            cheatsheet.0 = !cheatsheet.0;
                        }
                    });
                });
        });
}

/// Renders an inline diff view with colored +/- lines.
fn show_image_preview(
    ui: &egui::Ui,
    img: &super::ai_chat::ChatImage,
    preview_state: &mut ImagePreviewState,
) {
    use base64::Engine;

    // Decode and cache texture if not already active for this image
    let key = format!("{}_{}", img.filename, img.base64_data.len());
    let texture = if preview_state
        .active
        .as_ref()
        .is_some_and(|(k, _)| k == &key)
    {
        preview_state.active.as_ref().unwrap().1.clone()
    } else {
        let Ok(raw) = base64::engine::general_purpose::STANDARD.decode(&img.base64_data) else {
            return;
        };
        let Ok(dyn_img) = image::load_from_memory(&raw) else {
            return;
        };

        // Downscale if exceeding GPU texture limits
        let max_side = crate::app_config::MAX_TEXTURE_SIDE;
        let dyn_img = if dyn_img.width() > max_side || dyn_img.height() > max_side {
            dyn_img.resize(max_side, max_side, image::imageops::FilterType::Lanczos3)
        } else {
            dyn_img
        };

        let rgba = dyn_img.to_rgba8();
        let size = [rgba.width() as usize, rgba.height() as usize];
        let pixels = rgba.into_raw();
        let color_image = egui::ColorImage::from_rgba_unmultiplied(size, &pixels);
        let tex = ui
            .ctx()
            .load_texture(&key, color_image, egui::TextureOptions::LINEAR);
        preview_state.active = Some((key, tex.clone()));
        tex
    };

    let max_side = 400.0_f32;
    let [tw, th] = texture.size();
    #[allow(clippy::cast_precision_loss)]
    let aspect = tw as f32 / th.max(1) as f32;
    let (w, h) = if tw >= th {
        (max_side, max_side / aspect)
    } else {
        (max_side * aspect, max_side)
    };

    // Show as a floating area near mouse position
    if let Some(pos) = ui.ctx().input(|i| i.pointer.hover_pos()) {
        egui::Area::new(egui::Id::new("img_preview_popup"))
            .fixed_pos(egui::pos2(pos.x + 16.0, pos.y + 16.0))
            .order(egui::Order::Tooltip)
            .interactable(false)
            .show(ui.ctx(), |ui| {
                egui::Frame::new()
                    .fill(egui::Color32::from_rgb(30, 30, 46))
                    .corner_radius(egui::CornerRadius::same(6))
                    .inner_margin(egui::Margin::same(4))
                    .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(80, 80, 100)))
                    .show(ui, |ui| {
                        ui.image(egui::load::SizedTexture::new(
                            texture.id(),
                            egui::vec2(w, h),
                        ));
                    });
            });
    }
}

fn load_image_as_chat_image(path: &std::path::Path) -> Option<super::ai_chat::ChatImage> {
    use base64::Engine;

    let data = std::fs::read(path).ok()?;
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    let mime_type = match ext.as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        _ => "image/png",
    };
    let filename = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("image")
        .to_string();

    // Downscale if the image exceeds the API size limit
    let max_bytes = crate::app_config::MAX_IMAGE_BYTES;
    #[allow(clippy::cast_precision_loss)]
    let data = if data.len() > max_bytes {
        let dyn_img = image::load_from_memory(&data).ok()?;
        let mut scale = (max_bytes as f64 / data.len() as f64).sqrt();
        loop {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let new_w = (f64::from(dyn_img.width()) * scale) as u32;
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let new_h = (f64::from(dyn_img.height()) * scale) as u32;
            let resized = dyn_img.resize(
                new_w.max(1),
                new_h.max(1),
                image::imageops::FilterType::Lanczos3,
            );
            let mut buf = std::io::Cursor::new(Vec::new());
            resized.write_to(&mut buf, image::ImageFormat::Jpeg).ok()?;
            let encoded = buf.into_inner();
            if encoded.len() <= max_bytes || scale < 0.1 {
                return Some(super::ai_chat::ChatImage {
                    filename,
                    mime_type: "image/jpeg".to_string(),
                    base64_data: base64::engine::general_purpose::STANDARD.encode(&encoded),
                });
            }
            scale *= 0.8;
        }
    } else {
        data
    };

    let base64_data = base64::engine::general_purpose::STANDARD.encode(&data);

    Some(super::ai_chat::ChatImage {
        filename,
        mime_type: mime_type.to_string(),
        base64_data,
    })
}

fn cheatsheet_system(
    mut contexts: EguiContexts,
    mut cheatsheet: ResMut<CheatsheetOpen>,
    keyboard: Res<ButtonInput<KeyCode>>,
) {
    let Some(ctx) = contexts.try_ctx_mut() else {
        return;
    };

    // Toggle with ? key (Slash + Shift) when not typing
    if !ctx.wants_keyboard_input()
        && keyboard.pressed(KeyCode::ShiftLeft)
        && keyboard.just_pressed(KeyCode::Slash)
    {
        cheatsheet.0 = !cheatsheet.0;
    }

    if !cheatsheet.0 {
        return;
    }

    // Close on Esc
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        cheatsheet.0 = false;
        return;
    }

    egui::Window::new("⌨ Keyboard Shortcuts")
        .open(&mut cheatsheet.0)
        .resizable(false)
        .collapsible(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            egui::Grid::new("cheatsheet_grid")
                .num_columns(2)
                .spacing([24.0, 6.0])
                .show(ui, |ui| {
                    let shortcuts: &[(&str, &str)] = &[
                        // Navigation
                        ("Orbit", "MMB drag / RMB drag"),
                        ("Pan", "Shift + MMB drag"),
                        ("Zoom", "Scroll / + / −"),
                        ("Move focus", "W A S D / Arrow keys"),
                        // Views
                        ("Front view", "1"),
                        ("Back view", "2"),
                        ("Right view", "3"),
                        ("Left view", "4"),
                        ("Top view", "5"),
                        ("Bottom view", "6"),
                        ("Isometric view", "7"),
                        // Toggles
                        ("Toggle gizmos", "G"),
                        ("Toggle labels", "L"),
                        ("Keyboard shortcuts", "?"),
                        // Tools
                        ("Cancel ruler", "Esc"),
                    ];

                    for (action, key) in shortcuts {
                        ui.label(*action);
                        ui.strong(*key);
                        ui.end_row();
                    }
                });
        });
}

fn draw_part_labels(
    mut contexts: EguiContexts,
    part_query: Query<(
        &PartLabel,
        &GlobalTransform,
        &bevy::render::primitives::Aabb,
    )>,
    camera_query: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
    occupied: Res<OccupiedScreenSpace>,
    splash: Res<SplashScreen>,
    label_vis: Res<LabelVisibility>,
) {
    if !label_vis.visible {
        return;
    }
    if splash.timer > -SPLASH_FADE_DURATION {
        return;
    }
    let Some(ctx) = contexts.try_ctx_mut() else {
        return;
    };
    let Ok((camera, camera_transform)) = camera_query.get_single() else {
        return;
    };

    let bg = egui::Color32::from_rgba_premultiplied(30, 30, 30, 200);

    for (part_label, global_transform, aabb) in &part_query {
        // Compute world-space center of the AABB
        let center = global_transform.transform_point(aabb.center.into());

        let Ok(screen_pos) = camera.world_to_viewport(camera_transform, center) else {
            continue;
        };

        let [r, g, b] = part_label.color;
        let label_color =
            egui::Color32::from_rgb((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8);

        let label_text = &part_label.label;
        let char_width = 8.0;
        #[allow(clippy::cast_precision_loss)]
        let label_w = (label_text.len() as f32).mul_add(char_width, 8.0);
        let label_h = 18.0;

        let wx = screen_pos.x + occupied.left;
        let wy = screen_pos.y;

        egui::Area::new(egui::Id::new(format!("part_label_{}", part_label.index)))
            .fixed_pos(egui::pos2(wx - label_w / 2.0, wy - label_h / 2.0))
            .interactable(false)
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(bg)
                    .corner_radius(egui::CornerRadius::same(3))
                    .inner_margin(egui::Margin::symmetric(4, 2))
                    .show(ui, |ui| {
                        ui.label(
                            egui::RichText::new(label_text)
                                .color(label_color)
                                .strong()
                                .small(),
                        );
                    });
            });
    }
}

const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "gif", "webp", "bmp"];

fn file_drop_system(
    mut dnd_events: EventReader<bevy::window::FileDragAndDrop>,
    mut chat_state: ResMut<ChatState>,
) {
    for event in dnd_events.read() {
        if let bevy::window::FileDragAndDrop::DroppedFile { path_buf, .. } = event {
            let is_image = path_buf
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|ext| IMAGE_EXTENSIONS.contains(&ext.to_lowercase().as_str()));
            if is_image && let Some(img) = load_image_as_chat_image(path_buf) {
                chat_state.pending_images.push(img);
            }
        }
    }
}
