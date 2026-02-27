pub mod ai_chat;
pub mod camera;
pub mod code_editor;
pub mod compilation;
pub mod persistence;
pub mod scene;
pub mod ui;

use bevy::app::PluginGroupBuilder;
use bevy::prelude::*;

/// Plugin group that registers all `SynapsCAD` plugins.
pub struct SynapScadPlugins;

impl PluginGroup for SynapScadPlugins {
    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::start::<Self>()
            .add(scene::ScenePlugin)
            .add(code_editor::CodeEditorPlugin)
            .add(ui::UiPlugin)
            .add(compilation::CompilationPlugin)
            .add(camera::CameraPlugin)
            .add(ai_chat::AiChatPlugin)
            .add(persistence::PersistencePlugin)
    }
}
