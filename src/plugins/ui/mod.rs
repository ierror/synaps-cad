use bevy::prelude::*;
use bevy_egui::{EguiInputSet, EguiPreUpdateSet};

pub mod resources;
pub mod theme;
pub mod layout;
pub mod chat;
pub mod editor;
pub mod viewport;
pub mod utils;
pub mod systems;

pub use resources::{OccupiedScreenSpace, AppErrors};
pub use systems::*;

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<OccupiedScreenSpace>()
            .init_resource::<resources::FilePickerState>()
            .init_resource::<resources::ImagePreviewState>()
            .init_resource::<AppErrors>()
            .init_resource::<resources::PerformanceMonitor>()
            .init_resource::<resources::SettingsDialogOpen>()
            .init_resource::<resources::CheatsheetOpen>()
            .init_resource::<resources::ExportState>()
            .init_resource::<resources::SplashScreen>()
            .add_systems(Startup, theme::setup_egui_theme)
            .add_systems(
                PreUpdate,
                systems::fix_clipboard_paste_events
                    .after(EguiInputSet::WriteEguiEvents)
                    .before(EguiPreUpdateSet::BeginPass),
            )
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
                    draw_axis_indicator,
                    file_drop_system,
                    performance_monitor_system,
                ),
            );
    }
}
