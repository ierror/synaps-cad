// Bevy systems require owned parameters (Query, Res, ResMut, etc.)
#![allow(clippy::needless_pass_by_value)]

use bevy::prelude::*;
use bevy::render::settings::{PowerPreference, RenderCreation, WgpuSettings};
use bevy::render::RenderPlugin;
use bevy::window::PresentMode;
use bevy_egui::EguiPlugin;

mod app_config;
pub mod compiler;
mod export;
mod plugins;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: format!("SynapsCAD v{}", env!("CARGO_PKG_VERSION")),
                    resolution: (1600.0, 900.0).into(),
                    // Use VSync to limit frame rate and reduce CPU usage when idle
                    present_mode: PresentMode::Fifo,
                    ..default()
                }),
                ..default()
            })
            .set(RenderPlugin {
                render_creation: RenderCreation::Automatic(
                    WgpuSettings {
                        // Prefer integrated GPU for power efficiency if available
                        power_preference: PowerPreference::LowPower,
                        ..default()
                    }
                ),
                synchronous_pipeline_compilation: false,
            })
        )
        .add_plugins(EguiPlugin)
        .add_plugins(plugins::SynapScadPlugins)
        .run();
}
