use bevy::prelude::*;
use bevy_egui::egui;
use std::sync::{Mutex, mpsc};
use crate::export::ExportFormat;

#[derive(Resource, Default)]
pub struct OccupiedScreenSpace {
    pub left: f32,
}

/// Async file-picker result receiver (avoids blocking the main thread).
#[derive(Resource, Default)]
pub struct FilePickerState {
    pub(crate) receiver: Option<Mutex<mpsc::Receiver<Vec<std::path::PathBuf>>>>,
}

/// State for image hover preview in chat.
#[derive(Resource, Default)]
pub struct ImagePreviewState {
    /// (`base64_data` key, decoded texture handle)
    pub(crate) active: Option<(String, egui::TextureHandle)>,
}

/// Whether the AI settings dialog window is open.
#[derive(Resource)]
pub struct SettingsDialogOpen(pub bool);

impl Default for SettingsDialogOpen {
    fn default() -> Self {
        Self(false)
    }
}

#[derive(Resource, Default)]
pub struct CheatsheetOpen(pub bool);

/// Non-fatal errors shown in the UI instead of panicking.
#[derive(Resource, Default)]
pub struct AppErrors {
    pub(crate) errors: Vec<AppError>,
}

pub struct AppError {
    pub(crate) message: String,
    pub(crate) timestamp: std::time::Instant,
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
pub struct ExportState {
    /// Receives the chosen save path from the async file dialog.
    pub(crate) receiver: Option<Mutex<mpsc::Receiver<Option<std::path::PathBuf>>>>,
    /// The format selected for the pending export.
    pub(crate) pending_format: Option<ExportFormat>,
}

/// Splash screen state — shown on app startup, dismissed by click or timeout.
#[derive(Resource)]
pub struct SplashScreen {
    pub(crate) texture: Option<egui::TextureHandle>,
    pub(crate) timer: f32,
    pub(crate) dismissing: bool,
}

impl Default for SplashScreen {
    fn default() -> Self {
        Self {
            texture: None,
            timer: 1.5,
            dismissing: true,
        }
    }
}
