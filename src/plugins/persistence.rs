use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use super::ai_chat::{AiConfig, ChatImage, ChatMessage, ChatState};
use super::code_editor::ScadCode;

pub struct PersistencePlugin;

const APP_DIR_NAME: &str = "synaps-cad";
const SESSION_FILE: &str = "session.json";
const MAX_PERSISTED_MESSAGES: usize = 50;

#[derive(Serialize, Deserialize)]
struct SerializableImage {
    filename: String,
    mime_type: String,
    base64_data: String,
}

/// Serializable chat message including attached images.
#[derive(Serialize, Deserialize)]
struct SerializableChatMessage {
    role: String,
    content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    thinking: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    images: Vec<SerializableImage>,
}

#[derive(Serialize, Deserialize)]
struct PersistentData {
    chat_messages: Vec<SerializableChatMessage>,
    adapter_name: String,
    model_name: String,
    system_prompt: String,
    temperature: f64,
    editor_code: String,
    #[serde(default = "default_verification_rounds")]
    max_verification_rounds: u32,
    /// Legacy: old multi-part data. Merged into `editor_code` on load.
    #[serde(default)]
    parts: std::collections::HashMap<String, String>,
}

fn default_verification_rounds() -> u32 {
    2
}

fn config_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join(APP_DIR_NAME))
}

fn session_path() -> Option<PathBuf> {
    config_dir().map(|d| d.join(SESSION_FILE))
}

impl Plugin for PersistencePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load_session_system)
            .add_systems(Update, auto_save_system);
    }
}

fn load_session_system(
    mut ai_config: ResMut<AiConfig>,
    mut chat_state: ResMut<ChatState>,
    mut scad_code: ResMut<ScadCode>,
) {
    let Some(path) = session_path() else {
        return;
    };
    let Ok(data) = std::fs::read_to_string(&path) else {
        return;
    };
    let Ok(saved) = serde_json::from_str::<PersistentData>(&data) else {
        eprintln!("[SynapsCAD] Failed to parse session file, starting fresh");
        return;
    };

    ai_config.adapter_name = saved.adapter_name;
    ai_config.model_name = saved.model_name;
    // Reset system prompt to default if it contains stale instructions
    if saved.system_prompt.contains("```openscad") || saved.system_prompt.contains("synapscad:") {
        eprintln!("[SynapsCAD] Resetting stale system prompt to default");
    } else {
        ai_config.system_prompt = saved.system_prompt;
    }
    ai_config.temperature = saved.temperature;
    ai_config.max_verification_rounds = saved.max_verification_rounds;

    chat_state.messages = saved
        .chat_messages
        .into_iter()
        .map(|m| ChatMessage {
            role: m.role,
            content: m.content,
            thinking: m.thinking,
            images: m
                .images
                .into_iter()
                .map(|i| ChatImage {
                    filename: i.filename,
                    mime_type: i.mime_type,
                    base64_data: i.base64_data,
                })
                .collect(),
        })
        .collect();

    // Backward compat: if old multi-part data exists, merge into single buffer
    if !saved.parts.is_empty() {
        let mut merged = String::new();
        for (name, code) in &saved.parts {
            if !merged.is_empty() {
                merged.push_str("\n\n");
            }
            merged.push_str(&format!("// --- {name} ---\n{code}"));
        }
        scad_code.text = merged;
    } else if !saved.editor_code.is_empty() {
        scad_code.text = saved.editor_code;
    }
    scad_code.dirty = true;

    eprintln!("[SynapsCAD] Session restored from {}", path.display());
}

/// Timer resource to throttle auto-save.
#[derive(Resource)]
struct AutoSaveTimer(Timer);

impl Default for AutoSaveTimer {
    fn default() -> Self {
        Self(Timer::from_seconds(30.0, TimerMode::Repeating))
    }
}

fn auto_save_system(
    time: Res<Time>,
    mut timer: Local<AutoSaveTimer>,
    ai_config: Res<AiConfig>,
    chat_state: Res<ChatState>,
    scad_code: Res<ScadCode>,
) {
    timer.0.tick(time.delta());
    if !timer.0.just_finished() {
        return;
    }

    save_session(&ai_config, &chat_state, &scad_code);
}

fn save_session(ai_config: &AiConfig, chat_state: &ChatState, scad_code: &ScadCode) {
    let Some(dir) = config_dir() else {
        return;
    };
    let Some(path) = session_path() else {
        return;
    };

    if std::fs::create_dir_all(&dir).is_err() {
        eprintln!("[SynapsCAD] Failed to create config directory");
        return;
    }

    let data = PersistentData {
        chat_messages: chat_state
            .messages
            .iter()
            .rev()
            .take(MAX_PERSISTED_MESSAGES)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .map(|m| SerializableChatMessage {
                role: m.role.clone(),
                content: m.content.clone(),
                thinking: m.thinking.clone(),
                images: m
                    .images
                    .iter()
                    .map(|i| SerializableImage {
                        filename: i.filename.clone(),
                        mime_type: i.mime_type.clone(),
                        base64_data: i.base64_data.clone(),
                    })
                    .collect(),
            })
            .collect(),
        adapter_name: ai_config.adapter_name.clone(),
        model_name: ai_config.model_name.clone(),
        system_prompt: ai_config.system_prompt.clone(),
        temperature: ai_config.temperature,
        editor_code: scad_code.text.clone(),
        max_verification_rounds: ai_config.max_verification_rounds,
        parts: std::collections::HashMap::new(),
    };

    match serde_json::to_string_pretty(&data) {
        Ok(json) => {
            if let Err(e) = std::fs::write(&path, json) {
                eprintln!("[SynapsCAD] Failed to save session: {e}");
            }
        }
        Err(e) => eprintln!("[SynapsCAD] Failed to serialize session: {e}"),
    }
}
