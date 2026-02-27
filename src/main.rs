// Bevy systems require owned parameters (Query, Res, ResMut, etc.)
#![allow(clippy::needless_pass_by_value)]

use bevy::prelude::*;
use bevy_egui::EguiPlugin;

mod app_config;
mod compiler;
mod export;
mod plugins;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: format!("SynapsCAD v{}", env!("CARGO_PKG_VERSION")),
                resolution: (1600.0, 900.0).into(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(EguiPlugin)
        .add_plugins(plugins::SynapScadPlugins)
        .run();
}
