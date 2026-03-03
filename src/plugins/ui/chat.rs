use bevy_egui::egui;
use crate::plugins::ui::utils::highlight_openscad;

/// Render chat message content with code blocks highlighted.
pub fn render_chat_content(ui: &mut egui::Ui, content: &str, is_error: bool) -> egui::Response {
    let code_bg = egui::Color32::from_rgb(30, 30, 46);
    let code_color = egui::Color32::from_rgb(220, 220, 170);
    let lang_color = egui::Color32::from_rgb(100, 100, 130);
    let use_highlighting = |lang: &str| -> bool {
        matches!(lang.to_lowercase().as_str(), "synapscad" | "openscad" | "scad")
    };

    let mut last_resp: Option<egui::Response> = None;
    let mut remaining = content;

    while !remaining.is_empty() {
        if let Some(fence_start) = remaining.find("```") {
            let before = &remaining[..fence_start];
            if !before.is_empty() {
                render_markdown_text(ui, before, is_error);
            }

            let after_fence = &remaining[fence_start + 3..];
            if let Some(close_pos) = after_fence.find("```") {
                let block = &after_fence[..close_pos];
                let (lang, code) = if let Some(newline) = block.find('\n') {
                    let lang_tag = block[..newline].trim();
                    (lang_tag, &block[newline + 1..])
                } else {
                    ("", block)
                };

                let r = egui::Frame::new()
                    .fill(code_bg)
                    .corner_radius(egui::CornerRadius::same(4))
                    .inner_margin(egui::Margin::same(6))
                    .show(ui, |ui| {
                        if !lang.is_empty() {
                            ui.label(egui::RichText::new(lang).small().color(lang_color));
                        }
                        let trimmed = code.trim_end();
                        if use_highlighting(lang) {
                            let font_id = egui::FontId::monospace(12.0);
                            let job = highlight_openscad(trimmed, font_id);
                            ui.label(job);
                        } else {
                            ui.label(egui::RichText::new(trimmed).monospace().color(code_color));
                        }
                    });
                last_resp = Some(r.response);
                remaining = &after_fence[close_pos + 3..];
            } else {
                let block = after_fence;
                let (lang, code) = if let Some(newline) = block.find('\n') {
                    let lang_tag = block[..newline].trim();
                    (lang_tag, &block[newline + 1..])
                } else {
                    ("", block)
                };
                let r = egui::Frame::new()
                    .fill(code_bg)
                    .corner_radius(egui::CornerRadius::same(4))
                    .inner_margin(egui::Margin::same(6))
                    .show(ui, |ui| {
                        if !lang.is_empty() {
                            ui.label(egui::RichText::new(lang).small().color(lang_color));
                        }
                        let trimmed = code.trim_end();
                        if use_highlighting(lang) {
                            let font_id = egui::FontId::monospace(12.0);
                            let job = highlight_openscad(trimmed, font_id);
                            ui.label(job);
                        } else {
                            ui.label(egui::RichText::new(trimmed).monospace().color(code_color));
                        }
                    });
                last_resp = Some(r.response);
                remaining = "";
            }
        } else {
            render_markdown_text(ui, remaining, is_error);
            remaining = "";
        }
    }

    last_resp.unwrap_or_else(|| ui.label(""))
}

pub fn render_markdown_text(ui: &mut egui::Ui, text: &str, is_error: bool) {
    let error_color = egui::Color32::from_rgb(255, 120, 120);
    let header_bg = egui::Color32::from_rgb(60, 60, 80);

    for line in text.split('\n') {
        let line: &str = line;
        let trimmed = line.trim();
        if trimmed.starts_with("**") && trimmed.ends_with("**") {
            let header_text = trimmed.trim_matches('*');
            egui::Frame::new()
                .fill(header_bg)
                .corner_radius(egui::CornerRadius::same(3))
                .inner_margin(egui::Margin::symmetric(6, 2))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(egui::RichText::new(header_text).strong().color(egui::Color32::WHITE));
                });
        } else if !line.is_empty() {
            if is_error {
                ui.label(egui::RichText::new(line).color(error_color));
            } else {
                ui.label(line);
            }
        } else {
            ui.add_space(4.0);
        }
    }
}
