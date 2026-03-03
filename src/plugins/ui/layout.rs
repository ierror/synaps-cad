use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use std::sync::{Mutex, mpsc};

use crate::plugins::code_editor::{ScadCode, detect_views, set_active_view};
use crate::plugins::compilation::{CompilationState, LastCompiledParts, ModelViews};
use crate::plugins::ai_chat::{AiConfig, AvailableModels, ChatState, TokioRuntime, env_var_for_adapter, ADAPTER_NAMES, VERIFICATION_ROUND_CHOICES};
use crate::plugins::ui::resources::{OccupiedScreenSpace, ImagePreviewState, AppErrors, SettingsDialogOpen, ExportState};
use crate::plugins::ui::chat::render_chat_content;
use crate::plugins::ui::editor::render_code_editor;
use crate::plugins::ui::utils::{show_image_preview, clipboard_image_as_chat_image, copy_chat_image_to_clipboard};
use crate::export::ALL_FORMATS;

#[allow(clippy::too_many_arguments)]
pub fn ui_layout_system(
    mut contexts: EguiContexts,
    mut scad_code: ResMut<ScadCode>,
    mut chat_state: ResMut<ChatState>,
    mut occupied: ResMut<OccupiedScreenSpace>,
    mut ai_config: ResMut<AiConfig>,
    mut available_models: ResMut<AvailableModels>,
    mut compilation_state: ResMut<CompilationState>,
    mut file_picker: ResMut<crate::plugins::ui::resources::FilePickerState>,
    runtime: Res<TokioRuntime>,
    mut preview_state: ResMut<ImagePreviewState>,
    mut app_errors: ResMut<AppErrors>,
    mut settings_open: ResMut<SettingsDialogOpen>,
    last_parts: Res<LastCompiledParts>,
    mut export_state: ResMut<ExportState>,
    model_views: Res<ModelViews>,
    mut cached_view_textures: Local<Vec<(String, egui::TextureHandle)>>,
) {
    let Some(ctx) = contexts.try_ctx_mut() else { return; };

    // Cache view textures
    if model_views.is_changed() {
        use base64::Engine;
        let mut new_textures = Vec::new();
        for (label, base64_png) in &model_views.views {
            if base64_png.is_empty() { continue; }
            if let Ok(png_bytes) = base64::engine::general_purpose::STANDARD.decode(base64_png) {
                if let Ok(dyn_img) = image::load_from_memory(&png_bytes) {
                    let rgba = dyn_img.to_rgba8();
                    let color_image = egui::ColorImage::from_rgba_unmultiplied([rgba.width() as usize, rgba.height() as usize], rgba.as_raw());
                    new_textures.push((label.clone(), ctx.load_texture(format!("view_cycle_{label}"), color_image, egui::TextureOptions::LINEAR)));
                }
            }
        }
        *cached_view_textures = new_textures;
    }

    let response = egui::SidePanel::left("side_panel").default_width(400.0).min_width(300.0).max_width(600.0).resizable(true).show(ctx, |ui| {
        let max_w = ui.available_width();
        ui.set_min_width(max_w); ui.set_max_width(max_w);

        render_error_banner(ui, &mut app_errors);
        ui.add_space(4.0); ui.separator();

        render_ai_assistant_header(ui, &mut chat_state, &mut ai_config, &mut available_models, &mut settings_open);
        ui.separator();

        render_pending_attachments(ui, &mut chat_state, &mut preview_state);
        render_chat_input(ui, &mut chat_state, &mut file_picker, &runtime);

        let total_remaining = ui.available_height();
        let chat_height = (total_remaining * 0.45).max(50.0);
        render_chat_messages(ui, &mut chat_state, chat_height, &cached_view_textures, &mut preview_state);

        ui.add_space(4.0); ui.separator();

        render_code_header(ui, &mut scad_code, &mut chat_state, &mut compilation_state, &last_parts, &mut export_state, &runtime);
        render_view_selector(ui, &mut scad_code);
        ui.separator();

        let editor_height = (total_remaining * 0.55).max(80.0);
        render_code_editor(ui, &mut scad_code, editor_height);
    });

    occupied.left = response.response.rect.width();
    if available_models.needs_configuration { settings_open.0 = true; }
    if settings_open.0 && ctx.input(|i| i.key_pressed(egui::Key::Escape)) { settings_open.0 = false; }

    render_settings_dialog(ctx, &mut settings_open, &mut ai_config, &mut available_models);
}

fn render_error_banner(ui: &mut egui::Ui, app_errors: &mut AppErrors) {
    app_errors.errors.retain(|e| e.timestamp.elapsed().as_secs() < 30);
    for err in &app_errors.errors {
        egui::Frame::new().fill(egui::Color32::from_rgb(80, 20, 20)).corner_radius(egui::CornerRadius::same(4)).inner_margin(egui::Margin::same(6)).show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(egui::RichText::new("⚠").color(egui::Color32::YELLOW));
                ui.label(egui::RichText::new(&err.message).color(egui::Color32::WHITE).small());
            });
            if ui.small_button("🔗 Report issue on GitHub").clicked() {
                let _ = open::that(format!("{}/issues/new", crate::app_config::GITHUB_REPO_URL));
            }
        });
        ui.add_space(2.0);
    }
    if !app_errors.errors.is_empty() { ui.separator(); }
}

fn render_ai_assistant_header(ui: &mut egui::Ui, chat_state: &mut ChatState, ai_config: &mut AiConfig, available_models: &mut AvailableModels, settings_open: &mut SettingsDialogOpen) {
    ui.horizontal_wrapped(|ui| {
        ui.heading("AI Assistant");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button(if available_models.needs_configuration { "⚙ ⚠" } else { "⚙" }).clicked() { settings_open.0 = !settings_open.0; }
            if ui.button("🗑").clicked() {
                chat_state.session_start = chat_state.messages.len();
                chat_state.input_history.clear(); chat_state.history_index = None; chat_state.pending_images.clear();
            }

            let selected_label = if ai_config.max_verification_rounds == u32::MAX { "∞".into() } else { ai_config.max_verification_rounds.to_string() };
            egui::ComboBox::from_id_salt("verify_rounds_main").selected_text(selected_label).width(32.0).show_ui(ui, |ui| {
                for &n in VERIFICATION_ROUND_CHOICES {
                    let label = if n == u32::MAX { "∞".into() } else { n.to_string() };
                    ui.selectable_value(&mut ai_config.max_verification_rounds, n, label);
                }
            });
            ui.label(egui::RichText::new("Verify").small());
            ui.checkbox(&mut ai_config.extended_thinking, "").on_hover_text("Extended Thinking");
            ui.label(egui::RichText::new("Think").small());

            let mut current_adapter = ai_config.adapter_name.clone();
            if egui::ComboBox::from_id_salt("provider_select_main").selected_text(&current_adapter).width(80.0).show_ui(ui, |ui| {
                let mut changed = false;
                for &adapter in ADAPTER_NAMES {
                    let configured = !env_var_for_adapter(adapter).is_some() || env_var_for_adapter(adapter).and_then(|n| std::env::var(n).ok()).is_some_and(|v| !v.is_empty()) || ai_config.api_keys.get(adapter).is_some_and(|k| !k.is_empty());
                    ui.add_enabled_ui(configured, |ui| { if ui.selectable_value(&mut current_adapter, adapter.to_string(), adapter).clicked() { changed = true; } });
                }
                changed
            }).inner.unwrap_or(false) && current_adapter != ai_config.adapter_name {
                if !ai_config.model_name.is_empty() { ai_config.model_per_provider.insert(ai_config.adapter_name.clone(), ai_config.model_name.clone()); }
                ai_config.adapter_name = current_adapter;
                ai_config.model_name = ai_config.model_per_provider.get(&ai_config.adapter_name).cloned().unwrap_or_default();
                available_models.models.clear(); available_models.error = None;
            }
        });
    });
}

fn render_pending_attachments(ui: &mut egui::Ui, chat_state: &mut ChatState, preview_state: &mut ImagePreviewState) {
    if chat_state.pending_images.is_empty() { return; }
    ui.horizontal_wrapped(|ui| {
        ui.label(egui::RichText::new("Attached:").small().color(egui::Color32::from_rgb(140, 140, 160)));
        let mut to_remove = None;
        for (i, img) in chat_state.pending_images.iter().enumerate() {
            let frame = egui::Frame::new().fill(egui::Color32::from_rgb(40, 40, 58)).corner_radius(egui::CornerRadius::same(4)).inner_margin(egui::Margin::symmetric(4, 2)).show(ui, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.add(egui::Label::new(egui::RichText::new(&img.filename).small().color(egui::Color32::from_rgb(180, 180, 200))).sense(egui::Sense::click()));
                    if ui.small_button("x").clicked() { to_remove = Some(i); }
                    label
                })
            });
            if frame.inner.inner.hovered() { show_image_preview(ui, img, preview_state); }
            if frame.inner.inner.clicked() { copy_chat_image_to_clipboard(img); }
        }
        if let Some(idx) = to_remove { chat_state.pending_images.remove(idx); }
    });
    ui.add_space(2.0);
}

fn render_chat_input(ui: &mut egui::Ui, chat_state: &mut ChatState, file_picker: &mut crate::plugins::ui::resources::FilePickerState, runtime: &TokioRuntime) {
    ui.horizontal_wrapped(|ui| {
        let mut send_clicked = false;
        let mut enter_pressed = false;
        let mut attach_clicked = false;

        let input_resp = ui.horizontal_top(|ui| {
            let resp = ui.add(egui::TextEdit::multiline(&mut chat_state.input_buffer).hint_text("Ask the AI assistant...").desired_width(ui.available_width() - 68.0).desired_rows(3).lock_focus(true));
            ui.vertical(|ui| {
                if chat_state.is_streaming {
                    if ui.button("⏹").clicked() { chat_state.is_streaming = false; chat_state.stream_receiver = None; chat_state.verification = crate::plugins::ai_chat::VerificationState::Idle; }
                } else {
                    send_clicked = ui.button("⬆").clicked();
                    enter_pressed = resp.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter) && i.modifiers.shift);
                }
                attach_clicked = ui.button("📎").clicked();
            });
            resp
        }).inner;

        if attach_clicked && file_picker.receiver.is_none() {
            let (tx, rx) = mpsc::channel();
            file_picker.receiver = Some(Mutex::new(rx));
            runtime.0.spawn(async move {
                let handles = rfd::AsyncFileDialog::new().add_filter("Images", &["png", "jpg", "jpeg", "gif", "webp", "bmp"]).pick_files().await;
                let _ = tx.send(handles.unwrap_or_default().into_iter().map(|h| h.path().to_path_buf()).collect());
            });
        }

        if input_resp.has_focus() {
            if ui.input(|i| i.key_pressed(egui::Key::ArrowUp)) && !chat_state.input_history.is_empty() {
                let idx = chat_state.history_index.map_or_else(|| chat_state.input_history.len() - 1, |i| i.saturating_sub(1));
                chat_state.history_index = Some(idx);
                let (text, images) = chat_state.input_history[idx].clone();
                chat_state.input_buffer = text; chat_state.pending_images = images;
            } else if ui.input(|i| i.key_pressed(egui::Key::ArrowDown)) && let Some(idx) = chat_state.history_index {
                if idx + 1 < chat_state.input_history.len() {
                    let new_idx = idx + 1; chat_state.history_index = Some(new_idx);
                    let (text, images) = chat_state.input_history[new_idx].clone();
                    chat_state.input_buffer = text; chat_state.pending_images = images;
                } else { chat_state.history_index = None; chat_state.input_buffer.clear(); chat_state.pending_images.clear(); }
            }
        }

        if ui.input(|i| i.key_pressed(egui::Key::V) && i.modifiers.command) && let Some(img) = clipboard_image_as_chat_image() { chat_state.pending_images.push(img); }
        if enter_pressed { chat_state.input_buffer = chat_state.input_buffer.trim_end_matches('\n').to_string(); }

        if (send_clicked || enter_pressed) && !chat_state.input_buffer.trim().is_empty() {
            let user_msg = chat_state.input_buffer.trim().to_string();
            let images = std::mem::take(&mut chat_state.pending_images);
            chat_state.input_history.push((user_msg.clone(), images.clone()));
            chat_state.history_index = None;
            chat_state.messages.push(crate::plugins::ai_chat::ChatMessage { role: "user".into(), content: user_msg, thinking: None, images, auto_generated: false, is_error: false });
            chat_state.input_buffer.clear(); chat_state.is_streaming = true; chat_state.stick_to_bottom = true;
        }
    });
}

fn render_chat_messages(ui: &mut egui::Ui, chat_state: &mut ChatState, chat_height: f32, view_textures: &[(String, egui::TextureHandle)], preview_state: &mut ImagePreviewState) {
    egui::ScrollArea::vertical().id_salt("chat_scroll").max_height(chat_height).stick_to_bottom(chat_state.stick_to_bottom).show(ui, |ui| {
        if chat_state.is_streaming {
            let no_resp = !chat_state.messages.last().is_some_and(|m| m.role == "assistant" && !m.content.is_empty());
            if no_resp && !view_textures.is_empty() {
                let view_idx = (ui.input(|i| i.time) / 1.5) as usize % view_textures.len();
                let (label, texture) = &view_textures[view_idx];
                ui.vertical_centered(|ui| {
                    egui::Frame::new().stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(80, 80, 100))).corner_radius(egui::CornerRadius::same(4)).inner_margin(egui::Margin::same(4)).show(ui, |ui| {
                        let img_resp = ui.image(egui::load::SizedTexture::new(texture.id(), egui::vec2(180.0, 180.0)));
                        ui.put(egui::Rect::from_center_size(img_resp.rect.center(), egui::vec2(24.0, 24.0)), egui::Spinner::new());
                    });
                    ui.label(egui::RichText::new(format!("📷 {label}")).small().color(egui::Color32::from_rgb(140, 140, 160)));
                });
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
            } else { ui.spinner(); }
        }
        match &chat_state.verification {
            crate::plugins::ai_chat::VerificationState::WaitingForCompilation => { ui.horizontal(|ui| { ui.spinner(); ui.label(egui::RichText::new("Compiling... will verify result").small().italics().color(egui::Color32::from_rgb(140, 140, 160))); }); }
            crate::plugins::ai_chat::VerificationState::Verifying(round) => { ui.horizontal(|ui| { ui.spinner(); ui.label(egui::RichText::new(format!("Verifying (round {round})...")).small().italics().color(egui::Color32::from_rgb(100, 160, 255))); }); }
            _ => {}
        }

        let visible_messages = &chat_state.messages[chat_state.session_start..];
        let msg_count = visible_messages.len();
        let mut img_to_remove: Option<(usize, usize)> = None;
        for (rev_i, msg) in visible_messages.iter().rev().enumerate() {
            let msg_idx = chat_state.session_start + msg_count - 1 - rev_i;
            let is_user = msg.role == "user";
            let (prefix, color, header_bg) = if is_user { ("You", egui::Color32::from_rgb(140, 180, 255), egui::Color32::from_rgb(50, 70, 120)) }
                else if msg.is_error { ("⚠", egui::Color32::from_rgb(255, 140, 140), egui::Color32::from_rgb(120, 50, 50)) }
                else { ("AI", egui::Color32::from_rgb(160, 255, 160), egui::Color32::from_rgb(45, 80, 45)) };

            let header_text = if is_user { let preview: String = msg.content.chars().take(80).collect(); format!("{prefix}: {preview}{}", if msg.content.len() > 80 { "…" } else { "" }) } else { format!("{prefix}:") };
            let id = ui.make_persistent_id(format!("chat_msg_{msg_idx}"));
            let mut state = egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, if chat_state.is_streaming { rev_i == 0 } else { rev_i == 0 || !is_user || msg.is_error });
            if msg.is_error { state.set_open(true); }
            state.show_header(ui, |ui| {
                let w = ui.available_width();
                egui::Frame::new().fill(header_bg).corner_radius(egui::CornerRadius::same(3)).inner_margin(egui::Margin::symmetric(4, 2)).show(ui, |ui| {
                    ui.set_width(w); ui.label(egui::RichText::new(&header_text).strong().color(color));
                });
            }).body(|ui| {
                if let Some(ref thinking) = msg.thinking {
                    egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id.with("thinking"), true).show_header(ui, |ui| {
                        ui.label(egui::RichText::new("💭 Thinking…").italics().color(egui::Color32::from_rgb(180, 180, 180)));
                    }).body(|ui| { crate::plugins::ui::chat::render_thinking_content(ui, thinking); });
                }
                if !is_user || msg.content.chars().count() > 80 { let scroll = render_chat_content(ui, &msg.content, msg.is_error); if chat_state.is_streaming && rev_i == 0 { scroll.scroll_to_me(Some(egui::Align::BOTTOM)); } }
                if !msg.images.is_empty() {
                    ui.horizontal_wrapped(|ui| {
                        for (img_i, img) in msg.images.iter().enumerate() {
                            let frame = egui::Frame::new().fill(egui::Color32::from_rgb(40, 40, 58)).corner_radius(egui::CornerRadius::same(3)).inner_margin(egui::Margin::symmetric(4, 2)).show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    let label = ui.add(egui::Label::new(egui::RichText::new("📷").small().color(egui::Color32::from_rgb(160, 160, 180))).sense(egui::Sense::click()));
                                    if is_user && ui.small_button("x").clicked() { img_to_remove = Some((msg_idx, img_i)); }
                                    label
                                })
                            });
                            if frame.inner.inner.hovered() { show_image_preview(ui, img, preview_state); }
                            if frame.inner.inner.clicked() { copy_chat_image_to_clipboard(img); }
                        }
                    });
                }
            });
            ui.add_space(2.0);
        }
        if let Some((m_idx, i_idx)) = img_to_remove { chat_state.messages[m_idx].images.remove(i_idx); }
    });
    if ui.input(|i| i.smooth_scroll_delta.y > 0.0) { chat_state.stick_to_bottom = false; }
}

fn render_code_header(ui: &mut egui::Ui, scad_code: &mut ScadCode, chat_state: &mut ChatState, compilation_state: &mut CompilationState, last_parts: &LastCompiledParts, export_state: &mut ExportState, runtime: &TokioRuntime) {
    ui.horizontal_wrapped(|ui| {
        ui.heading("Code");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.add_enabled(!compilation_state.is_compiling, egui::Button::new(if compilation_state.is_compiling { "Compiling..." } else { "Compile" })).clicked() { scad_code.dirty = true; }
            if ui.button("🗑").clicked() {
                scad_code.text.clear(); scad_code.dirty = true; compilation_state.should_zoom = true;
                chat_state.session_start = chat_state.messages.len(); chat_state.input_history.clear(); chat_state.history_index = None; chat_state.pending_images.clear(); chat_state.verification = crate::plugins::ai_chat::VerificationState::Idle;
            }
            if export_state.receiver.is_some() { ui.spinner(); } else {
                let has_parts = !last_parts.parts.is_empty();
                let export_btn = ui.add_enabled(has_parts, egui::Button::new("💾"));
                let popup_id = ui.make_persistent_id("export_popup");
                if export_btn.clicked() && has_parts { ui.memory_mut(|m| m.toggle_popup(popup_id)); }
                if ui.memory(|m| m.is_popup_open(popup_id)) {
                    let area = egui::Area::new(popup_id).order(egui::Order::Foreground).fixed_pos(export_btn.rect.left_bottom()).show(ui.ctx(), |ui| {
                        egui::Frame::popup(ui.style()).show(ui, |ui| {
                            ui.set_min_width(160.0);
                            for &fmt in ALL_FORMATS {
                                if ui.button(fmt.label()).clicked() {
                                    let ext = fmt.extension(); let (tx, rx) = mpsc::channel();
                                    export_state.receiver = Some(Mutex::new(rx)); export_state.pending_format = Some(fmt);
                                    runtime.0.spawn(async move {
                                        let handle = rfd::AsyncFileDialog::new().set_file_name(format!("model.{ext}")).add_filter(ext.to_uppercase(), &[ext]).save_file().await;
                                        let _ = tx.send(handle.map(|h| h.path().to_path_buf()));
                                    });
                                    ui.memory_mut(|m| m.close_popup());
                                }
                            }
                        });
                    });
                    if ui.input(|i| i.pointer.any_click()) && !area.response.contains_pointer() && !export_btn.contains_pointer() { ui.memory_mut(|m| m.close_popup()); }
                }
            }
        });
    });
}

fn render_view_selector(ui: &mut egui::Ui, scad_code: &mut ScadCode) {
    let (active_view, all_views) = detect_views(&scad_code.text);
    if all_views.len() > 1 {
        ui.horizontal_wrapped(|ui| {
            ui.label(egui::RichText::new("View:").small().color(egui::Color32::from_rgb(160, 160, 180)));
            let current = active_view.unwrap_or_default();
            for view_name in &all_views {
                let is_active = *view_name == current;
                let label = egui::RichText::new(view_name.as_str()).small();
                let label = if is_active { label.strong().color(egui::Color32::from_rgb(100, 160, 255)) } else { label.color(egui::Color32::from_rgb(160, 160, 180)) };
                if ui.selectable_label(is_active, label).clicked() && !is_active { set_active_view(&mut scad_code.text, view_name); scad_code.dirty = true; }
            }
        });
    }
}

fn render_settings_dialog(ctx: &egui::Context, settings_open: &mut SettingsDialogOpen, ai_config: &mut AiConfig, available_models: &mut AvailableModels) {
    egui::Window::new("⚙ AI Settings").open(&mut settings_open.0).resizable(true).default_width(360.0).collapsible(false).anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO).order(egui::Order::Foreground).show(ctx, |ui| {
        if available_models.needs_configuration { ui.colored_label(egui::Color32::from_rgb(255, 180, 50), "⚠ Previously configured model is no longer available."); ui.add_space(4.0); }
        ui.horizontal(|ui| {
            ui.label("Provider:"); let prev = ai_config.adapter_name.clone();
            egui::ComboBox::from_id_salt("ai_adapter_select").selected_text(&ai_config.adapter_name).show_ui(ui, |ui| {
                for &adapter in ADAPTER_NAMES {
                    let configured = !env_var_for_adapter(adapter).is_some() || env_var_for_adapter(adapter).and_then(|n| std::env::var(n).ok()).is_some_and(|v| !v.is_empty()) || ai_config.api_keys.get(adapter).is_some_and(|k| !k.is_empty());
                    ui.add_enabled_ui(configured, |ui| { ui.selectable_value(&mut ai_config.adapter_name, adapter.to_string(), adapter); });
                }
            });
            if ai_config.adapter_name != prev {
                if !ai_config.model_name.is_empty() { ai_config.model_per_provider.insert(prev, ai_config.model_name.clone()); }
                ai_config.model_name = ai_config.model_per_provider.get(&ai_config.adapter_name).cloned().unwrap_or_default();
            }
        });
        ui.horizontal(|ui| {
            ui.label("Model:");
            let needs_key = env_var_for_adapter(&ai_config.adapter_name).is_some();
            let has_key = !needs_key || env_var_for_adapter(&ai_config.adapter_name).and_then(|n| std::env::var(n).ok()).is_some_and(|v| !v.is_empty()) || !ai_config.api_key().is_empty();
            if !has_key { ui.colored_label(egui::Color32::from_rgb(255, 180, 50), "⚠ Set API key first"); } else {
                let model_label = if available_models.loading { "Loading...".into() } else if available_models.models.is_empty() { "No models available".into() } else { ai_config.model_name.clone() };
                let prev_model = ai_config.model_name.clone();
                egui::ComboBox::from_id_salt("ai_model_select").selected_text(model_label).show_ui(ui, |ui| {
                    for model in &available_models.models { ui.selectable_value(&mut ai_config.model_name, model.clone(), model.as_str()); }
                });
                if ai_config.model_name != prev_model && available_models.models.contains(&ai_config.model_name) { available_models.needs_configuration = false; }
            }
        });
        if let Some(ref err) = available_models.error { ui.colored_label(egui::Color32::from_rgb(255, 100, 100), format!("⚠ {err}")); }
        ui.horizontal(|ui| {
            ui.label("API Key:"); let env_var = env_var_for_adapter(&ai_config.adapter_name);
            let env_set = env_var.and_then(|n| std::env::var(n).ok()).is_some_and(|v| !v.is_empty());
            if env_set && ai_config.api_key().is_empty() { ui.add_enabled(false, egui::TextEdit::singleline(&mut String::new()).hint_text(format!("Set via {}", env_var.unwrap_or("")))); }
            else { let key = ai_config.api_key_mut(); ui.add(egui::TextEdit::singleline(key).password(true).hint_text(if env_set { "Override env var" } else { "Enter API key" })); }
        });
        ui.horizontal(|ui| { ui.label("Temperature:"); ui.add(egui::Slider::new(&mut ai_config.temperature, 0.0..=2.0).step_by(0.1)); });
        ui.label("System Prompt:"); ui.add(egui::TextEdit::multiline(&mut ai_config.system_prompt).desired_width(ui.available_width()).desired_rows(4));
    });
}
