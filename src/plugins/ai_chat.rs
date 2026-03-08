use bevy::prelude::*;
use std::sync::{Mutex, mpsc};

use super::code_editor::ScadCode;
use super::compilation::PartLabel;

pub struct AiChatPlugin;

const DEFAULT_SYSTEM_PROMPT: &str = "\
You are an AI assistant for a 3D CAD application (SynapsCAD). \
The user is working with OpenSCAD code. Help them modify their 3D models.\n\
\n\
## Code Output\n\
\n\
For **targeted edits** (changing a parameter, adding/modifying a module, small fixes),\n\
use search-replace blocks — one per logical change:\n\
\n\
<<<REPLACE\n\
<exact text to find — must appear exactly once in the current code>\n\
===\n\
<replacement text>\n\
>>>\n\
\n\
Rules:\n\
- The search text must match the current code **exactly** (whitespace, newlines, indentation).\n\
- Use enough context (e.g. the full function body) to make the match unique.\n\
- You can include multiple <<<REPLACE blocks in one response.\n\
- Leave the replacement empty to delete the matched text.\n\
\n\
For **large rewrites** or when writing code **from scratch**, wrap the full code in a synapscad block:\n\
\\`\\`\\`synapscad\n\
<complete code here>\n\
\\`\\`\\`\n\
\n\
Always use the `$view` system: define your geometry in a module and select it with an \
`if ($view == \"name\")` conditional. Start with a single view called \"main\":\n\
```\n\
$view = \"main\";\n\
module view_main() { /* all geometry here */ }\n\
if ($view == \"main\") view_main();\n\
```\n\
Only add additional views (e.g. \"assembly\", \"part_a\") when the user explicitly asks for them. \
If you create multiple parts, create views for each part.\n\
\n\n\
## General Guidelines\n\
Be concise and helpful.\n\
Always verify your results after making changes with the given 3D context \
information (orthographic views, bounding boxes, part counts). \
If something is unclear, ask clarifying questions before making changes. \
If something looks wrong in the rendered views, suggest corrections.\n\
In verification rounds, carefully compare the rendered views against the user's request. \
\n\
## Part Colors\n\
Use `color()` to give each part a realistic, semantically meaningful color. \
For example: green for plants/leaves, brown for wood/soil, red for flowers, \
gray for metal/concrete, blue for water, white for snow, orange for flames. \
Always pick colors that match the real-world material or object being modeled. \
Example: `color(\"green\") cylinder(h = 20, r = 3);` for a plant stem.\n\
\n\
## Physical Realism\n\
When generating 3D models, consider real-world physics and functionality. \
Objects should be structurally plausible and functionally correct:\n\
- A pipe must be a hollow cylinder (`difference()` of two cylinders), not a solid rod.\n\
- A cup needs an interior cavity so it can hold liquid.\n\
- A wheel should have an axle hole.\n\
- Load-bearing structures (bridges, shelves) need appropriate thickness and supports.\n\
- Moving parts (hinges, gears) need clearance gaps between components.\n\
- Materials that are connected (e.g., a wooden tabletop on metal legs) do not \"flow\" together; \
ensure they have distinct boundaries and do not simply merge into a single shape.\n\
- For objects with multiple parts, ensure each individual part is physically sound and \
\"fits\" correctly with others (proper tolerances, no unintended intersections, alignment).\n\
Think about what the object does in the real world and ensure the geometry reflects that.";

/// Supported AI provider adapters.
pub const ADAPTER_NAMES: &[&str] = &[
    "Anthropic",
    "OpenAI",
    "Gemini",
    "Groq",
    "Ollama",
    "DeepSeek",
    "Cohere",
    "Fireworks",
    "Together",
    "Xai",
    "Zai",
];

/// Returns the environment variable name used for the API key of the given adapter.
/// Returns `None` for adapters that don't need an API key (e.g. Ollama).
pub fn env_var_for_adapter(adapter: &str) -> Option<&'static str> {
    match adapter {
        "Anthropic" => Some("ANTHROPIC_API_KEY"),
        "OpenAI" => Some("OPENAI_API_KEY"),
        "Gemini" => Some("GEMINI_API_KEY"),
        "Groq" => Some("GROQ_API_KEY"),
        "DeepSeek" => Some("DEEPSEEK_API_KEY"),
        "Cohere" => Some("COHERE_API_KEY"),
        "Fireworks" => Some("FIREWORKS_API_KEY"),
        "Together" => Some("TOGETHER_API_KEY"),
        "Xai" => Some("XAI_API_KEY"),
        "Zai" => Some("ZAI_API_KEY"),
        _ => None,
    }
}

#[derive(Resource)]
pub struct AiConfig {
    pub adapter_name: String,
    pub model_name: String,
    /// Per-provider API keys (`adapter_name` → key).
    pub api_keys: std::collections::HashMap<String, String>,
    /// Per-provider last-used model (`adapter_name` → `model_name`).
    pub model_per_provider: std::collections::HashMap<String, String>,
    /// Custom Ollama host (e.g. <http://192.168.1.10:11434>).
    pub ollama_host: String,
    /// Last committed Ollama host (used to trigger model re-fetch).
    pub last_ollama_host: String,
    pub system_prompt: String,
    pub temperature: f64,
    /// Maximum automatic verification rounds (`u32::MAX` = unlimited).
    pub max_verification_rounds: u32,
    pub extended_thinking: bool,
}

impl AiConfig {
    /// Get the API key for the currently selected adapter.
    pub fn api_key(&self) -> &str {
        self.api_keys
            .get(&self.adapter_name)
            .map_or("", String::as_str)
    }

    /// Get a mutable reference to the API key for the currently selected adapter.
    pub fn api_key_mut(&mut self) -> &mut String {
        self.api_keys
            .entry(self.adapter_name.clone())
            .or_default()
    }
}

impl Default for AiConfig {
    fn default() -> Self {
        let mut ollama_host: String = std::env::var("OLLAMA_HOST").unwrap_or_else(|_| "http://localhost:11434".into());
        if !ollama_host.is_empty() && !ollama_host.ends_with('/') {
            ollama_host.push('/');
        }
        Self {
            adapter_name: "Anthropic".into(),
            model_name: "claude-3-5-sonnet-latest".into(),
            api_keys: std::collections::HashMap::new(),
            model_per_provider: std::collections::HashMap::new(),
            last_ollama_host: ollama_host.clone(),
            ollama_host,
            system_prompt: DEFAULT_SYSTEM_PROMPT.into(),
            temperature: 0.7,
            max_verification_rounds: 2,
            extended_thinking: false,
        }
    }
}

/// Dynamically fetched model names for the selected adapter.
#[derive(Resource, Default)]
pub struct AvailableModels {
    pub models: Vec<String>,
    pub loading: bool,
    pub last_adapter: String,
    pub last_api_key: String,
    pub last_ollama_host: String,
    pub error: Option<String>,
    /// Set to true when the persisted model is no longer available.
    pub needs_configuration: bool,
    /// Saved model name to restore after model list is fetched.
    pub pending_model: Option<String>,
    #[allow(clippy::type_complexity)]
    pub receiver: Option<Mutex<mpsc::Receiver<Result<Vec<String>, String>>>>,
}

/// An image attached to a chat message, stored as base64 PNG/JPEG.
#[derive(Clone, Debug)]
pub struct ChatImage {
    pub filename: String,
    pub mime_type: String,
    pub base64_data: String,
}

#[derive(Clone, Debug)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
    /// Optional reasoning/thinking content from the model.
    pub thinking: Option<String>,
    /// Images attached to this message.
    pub images: Vec<ChatImage>,
    /// True for messages generated automatically (e.g. verification rounds), not typed by user.
    pub auto_generated: bool,
    /// True for error messages (compilation errors, internal errors).
    pub is_error: bool,
}

#[derive(Debug)]
pub enum AiStreamChunk {
    /// Incremental text content chunk.
    Chunk(String),
    /// Incremental reasoning/thinking chunk.
    ReasoningChunk(String),
    /// Stream finished — final content and reasoning are built from chunks.
    Done {
        content: String,
        reasoning: Option<String>,
    },
    Error(String),
}

/// Maximum number of automatic verify-and-fix rounds per user request.
/// Predefined choices for the UI dropdown.
pub const VERIFICATION_ROUND_CHOICES: &[u32] = &[1, 2, 5, 10, 15, 20, 50, 100, u32::MAX];

const VERIFICATION_PROMPT: &str = "\
These are the rendered orthographic views AFTER your code change was compiled. \
Compare them carefully against the user's original request. \
If the result does NOT match what was asked for, provide corrected code in a synapscad code block. \
If it looks correct, briefly confirm what you see — do NOT repeat the code.\n\
\n\
## Verification Checklist\n\
When verifying or reviewing a model, go through the following checklist:\n\
\n\
### Phase 1: Mechanical Connections (Fasteners & Tolerances)\n\
- [ ] Hole Alignment: Do center axes of all through-holes on overlapping parts perfectly align?\n\
- [ ] Thread & Clearance Tolerances: Is the clearance hole slightly larger than the bolt diameter (e.g. 3.2mm hole for M3 screw)? For tapped holes, does the diameter match the minor diameter?\n\
- [ ] Fastener Completeness: Does every bolt through an unthreaded hole have a nut? Are washers present where needed?\n\
- [ ] Length & Protrusion: Are bolts long enough to engage but short enough not to penetrate unintended components?\n\
- [ ] Interference Detection: Do any solid parts intersect each other (bolt heads clipping into walls, etc.)?\n\
\n\
### Phase 2: Fluid Dynamics & Containment\n\
- [ ] Watertight Integrity: Is the fluid chamber completely enclosed (no non-manifold edges, holes, or gaps)?\n\
- [ ] Gasket/Seal Verification: Where parts join to contain fluid, is there a seal or gasket face?\n\
- [ ] Internal Flow Paths: Is there an unobstructed path from inlet to outlet? Are screws or components blocking the channel?\n\
- [ ] Surface Normals: Are all normals on the fluid boundary facing the correct direction?\n\
\n\
### Phase 3: Physics & Simulation Constraints\n\
- [ ] Density & Mass: Is every part assigned a material density? Is the Center of Mass logical?\n\
- [ ] Buoyancy: If an object should float, is its overall density less than the fluid density?\n\
- [ ] Joint Constraints: Are rigidly connected parts grouped with fixed joints? Do moving parts have correct degrees of freedom and limits?\n\
- [ ] Gravity & Orientation: Is the gravity vector correct relative to fluid reservoirs?\n\
\n\
### Phase 4: Metadata & BOM\n\
- [ ] BOM Consistency: Does the number of generated screws, nuts, and parts match the intended BOM?\n\
- [ ] Naming Conventions: Are parts named logically (e.g. M3_Bolt_12mm, Fluid_Reservoir_Bottom) rather than generic defaults?";

#[derive(Resource)]
pub struct ChatState {
    pub messages: Vec<ChatMessage>,
    pub input_buffer: String,
    pub input_history: Vec<(String, Vec<ChatImage>)>,
    pub history_index: Option<usize>,
    /// Saved draft text when the user starts cycling through history.
    pub history_draft: Option<String>,
    pub is_streaming: bool,
    pub stream_receiver: Option<Mutex<mpsc::Receiver<AiStreamChunk>>>,
    /// Images queued to attach to the next sent message.
    pub pending_images: Vec<ChatImage>,
    /// When the AI produces code that triggers compilation, this is set to
    /// `WaitingForCompilation`. After compilation completes and views update,
    /// it transitions to `ReadyToVerify` and a verification round fires.
    pub verification: VerificationState,
    /// Index into `messages` where the current session starts.
    /// Messages before this index are displayed but not sent to the AI.
    pub session_start: usize,
    /// When streaming started, used to display elapsed time.
    pub streaming_start: Option<std::time::Instant>,
}

impl Default for ChatState {
    fn default() -> Self {
        Self {
            messages: Vec::new(),
            input_buffer: String::new(),
            input_history: Vec::new(),
            history_index: None,
            history_draft: None,
            is_streaming: false,
            stream_receiver: None,
            pending_images: Vec::new(),
            verification: VerificationState::Idle,
            session_start: 0,
            streaming_start: None,
        }
    }
}

/// Tracks the auto-verification loop state.
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub enum VerificationState {
    #[default]
    Idle,
    /// AI produced code; waiting for compilation to finish and views to update.
    WaitingForCompilation,
    /// Compilation done, new views available — trigger verification call.
    ReadyToVerify,
    /// Currently running a verification round (the Nth).
    Verifying(u32),
}

#[derive(Resource)]
pub struct TokioRuntime(pub tokio::runtime::Runtime);

impl Plugin for AiChatPlugin {
    fn build(&self, app: &mut App) {
        let tokio_rt = tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime");
        app.init_resource::<AiConfig>()
            .init_resource::<ChatState>()
            .init_resource::<AvailableModels>()
            .insert_resource(TokioRuntime(tokio_rt))
            .add_systems(
                Update,
                (
                    fetch_models_system,
                    ai_send_system,
                    ai_receive_system,
                    ai_verify_system,
                )
                    .chain(),
            );
    }
}

/// Fetch model names when adapter selection changes.
fn fetch_models_system(
    mut ai_config: ResMut<AiConfig>,
    mut available: ResMut<AvailableModels>,
    runtime: Res<TokioRuntime>,
) {
    // Poll for results from a pending fetch
    if let Some(ref rx_mutex) = available.receiver {
        let rx = rx_mutex.lock().unwrap();
        if let Ok(result) = rx.try_recv() {
            drop(rx);
            available.loading = false;
            available.receiver = None;
            match result {
                Ok(models) => {
                    available.error = None;
                    // Restore pending model if it's in the fetched list
                    if let Some(pending) = available.pending_model.take() {
                        if models.contains(&pending) {
                            ai_config.model_name = pending;
                            available.needs_configuration = false;
                        } else {
                            available.needs_configuration = true;
                        }
                    } else { available.needs_configuration = !ai_config.model_name.is_empty()
                        && !models.contains(&ai_config.model_name) && available.last_adapter != ai_config.adapter_name; }
                    available.models = models;
                }
                Err(e) => {
                    eprintln!("[SynapsCAD] Failed to fetch models: {e}");
                    available.models.clear();
                    available.error = Some(e);
                    available.needs_configuration = true;
                }
            }
            return;
        }
    }

    // Trigger a new fetch if adapter, API key, or Ollama host changed
    let current_key = ai_config.api_key().to_string();
    let key_changed = available.last_api_key != current_key;
    let host_changed = available.last_ollama_host != ai_config.last_ollama_host;
    if (available.last_adapter != ai_config.adapter_name || key_changed || (ai_config.adapter_name == "Ollama" && host_changed)) && !available.loading {
        // Clear stale models immediately so the UI doesn't show old data
        available.models.clear();
        // Save current model name to restore after fetch if it's still valid
        if !ai_config.model_name.is_empty() {
            available.pending_model = Some(ai_config.model_name.clone());
        }
        ai_config.model_name.clear();
        available.last_adapter.clone_from(&ai_config.adapter_name);
        available.last_api_key.clone_from(&current_key);
        available.last_ollama_host.clone_from(&ai_config.last_ollama_host);
        available.loading = true;

        let adapter_name = ai_config.adapter_name.clone();
        let api_key = if current_key.is_empty() {
            None
        } else {
            Some(current_key)
        };
        let ollama_host = ai_config.last_ollama_host.clone();
        let (tx, rx) = mpsc::channel();
        available.receiver = Some(Mutex::new(rx));

        runtime.0.spawn(async move {
            let result = fetch_model_names(&adapter_name, api_key.as_deref(), &ollama_host).await;
            let _ = tx.send(result);
        });
    }
}

async fn fetch_model_names(
    adapter_name: &str,
    api_key: Option<&str>,
    ollama_host: &str,
) -> Result<Vec<String>, String> {
    use genai::Client;
    use genai::adapter::AdapterKind;
    use genai::resolver::AuthData;

    let adapter_kind = match adapter_name {
        "OpenAI" => AdapterKind::OpenAI,
        "Anthropic" => AdapterKind::Anthropic,
        "Gemini" => AdapterKind::Gemini,
        "Groq" => AdapterKind::Groq,
        "Ollama" => AdapterKind::Ollama,
        "DeepSeek" => AdapterKind::DeepSeek,
        "Cohere" => AdapterKind::Cohere,
        "Fireworks" => AdapterKind::Fireworks,
        "Together" => AdapterKind::Together,
        "Xai" => AdapterKind::Xai,
        "Zai" => AdapterKind::Zai,
        other => return Err(format!("Unknown adapter: {other}")),
    };

    if adapter_kind == AdapterKind::Ollama {
        // For Ollama, genai 0.6.0-beta.3 doesn't easily support custom endpoints for all_model_names.
        // We fetch it manually. Host is normalized to end with a slash.
        let url = format!("{ollama_host}api/tags");
        let client = reqwest::Client::new();
        let mut req = client.get(&url);
        if let Some(key) = api_key
            && !key.is_empty()
        {
            req = req.header("Authorization", format!("Bearer {key}"));
        }
        let res = req.send().await.map_err(|e| format!("Failed to fetch Ollama models: {e}"))?;
        if res.status() == reqwest::StatusCode::UNAUTHORIZED {
            return Err("Unauthorized (401). Please check your API key for this Ollama host.".into());
        }
        let body: serde_json::Value = res.json().await.map_err(|e| format!("Failed to parse Ollama models: {e}"))?;
        
        let mut models = Vec::new();
        if let Some(models_value) = body.get("models").and_then(|m| m.as_array()) {
            for model in models_value {
                if let Some(name) = model.get("name").and_then(|n| n.as_str()) {
                    models.push(name.to_string());
                }
            }
        }
        return Ok(models);
    }

    let client = api_key.map_or_else(Client::default, |key| {
        let key = key.to_string();
        Client::builder()
            .with_auth_resolver_fn(move |_| Ok(Some(AuthData::Key(key.clone()))))
            .build()
    });

    match client.all_model_names(adapter_kind).await {
        Ok(models) if !models.is_empty() => Ok(models),
        Ok(_) => Err("No models returned. Check your API key or Ollama host.".into()),
        Err(e) => Err(format!("Failed to fetch models: {e}")),
    }
}


fn ai_send_system(
    mut chat_state: ResMut<ChatState>,
    runtime: Res<TokioRuntime>,
    scad_code: Res<ScadCode>,
    ai_config: Res<AiConfig>,
    model_views: Res<super::compilation::ModelViews>,
    part_query: Query<&PartLabel>,
) {
    if !chat_state.is_streaming || chat_state.stream_receiver.is_some() {
        return;
    }

    let messages: Vec<ChatMessage> = chat_state.messages[chat_state.session_start..].to_vec();
    let part_context = build_part_context(&part_query);

    let current_code = scad_code.text.clone();
    let (active_view_name, _) = super::code_editor::detect_views(&current_code);
    let model_name = ai_config.model_name.clone();
    let current_key = ai_config.api_key().to_string();
    let api_key = if current_key.is_empty() {
        None
    } else {
        Some(current_key)
    };
    let ollama_host = ai_config.last_ollama_host.clone();
    let system_prompt = ai_config.system_prompt.clone();
    let temperature = ai_config.temperature;
    let extended_thinking = ai_config.extended_thinking;
    let views = model_views.views.clone();
    let other_views = model_views.other_views.clone();

    let (tx, rx) = mpsc::channel();
    chat_state.stream_receiver = Some(Mutex::new(rx));

    if cfg!(debug_assertions) {
        eprintln!("[DEBUG] === AI Chat Request ===");
        eprintln!("[DEBUG] Provider: {}", ai_config.adapter_name);
        eprintln!("[DEBUG] Model: {model_name}");
        if ai_config.adapter_name == "Ollama" {
            eprintln!("[DEBUG] Ollama Host: {ollama_host}");
        }
        eprintln!("[DEBUG] Temperature: {temperature}");
        eprintln!("[DEBUG] Extended thinking: {extended_thinking}");
        eprintln!("[DEBUG] System prompt: {} chars", system_prompt.len());
        eprintln!("[DEBUG] Messages: {}", messages.len());
        eprintln!("[DEBUG] Views: {}", views.len());
    }

    runtime.0.spawn(async move {
        let result = run_ai_stream(
            messages,
            current_code,
            active_view_name,
            &model_name,
            api_key.as_deref(),
            &ollama_host,
            &system_prompt,
            temperature,
            extended_thinking,
            &views,
            &other_views,
            part_context,
            tx.clone(),
        )
        .await;
        if let Err(e) = result {
            if cfg!(debug_assertions) {
                eprintln!("[DEBUG] AI error: {e}");
            }
            let _ = tx.send(AiStreamChunk::Error(format!("AI error: {e}")));
        }
    });
}

#[allow(clippy::too_many_arguments)]
async fn run_ai_stream(
    messages: Vec<ChatMessage>,
    current_code: String,
    active_view_name: Option<String>,
    model_name: &str,
    api_key: Option<&str>,
    ollama_host: &str,
    base_system_prompt: &str,
    temperature: f64,
    extended_thinking: bool,
    views: &[(String, String)],
    other_views: &[(String, Vec<(String, String)>)],
    part_context: String,
    tx: mpsc::Sender<AiStreamChunk>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use bevy::tasks::futures_lite::StreamExt;
    use genai::adapter::AdapterKind;
    use genai::chat::{
        ChatMessage as GenaiMessage, ChatOptions, ChatRequest, ChatStreamEvent, ContentPart,
        MessageContent, ReasoningEffort,
    };
    use genai::resolver::{AuthData, Endpoint};
    use genai::{Client, ServiceTarget};

    let ollama_host = ollama_host.to_string();
    let api_key_str = api_key.map(String::from);

    let client = Client::builder()
        .with_service_target_resolver_fn(move |service_target: ServiceTarget| {
            let mut service_target = service_target;
            if service_target.model.adapter_kind == AdapterKind::Ollama {
                service_target.endpoint = Endpoint::from_owned(ollama_host.clone());
            }
            if let Some(ref key) = api_key_str {
                service_target.auth = AuthData::Key(key.clone());
            }
            Ok(service_target)
        })
        .build();

    let mut system_prompt =
        format!("{base_system_prompt}\n\nCurrent OpenSCAD code:\n```\n{current_code}\n```\n");
    if !part_context.is_empty() {
        system_prompt.push('\n');
        system_prompt.push_str(&part_context);
    }

    if cfg!(debug_assertions) {
        eprintln!("[DEBUG] --- Full system prompt ({} chars) ---", system_prompt.len());
        eprintln!("{system_prompt}");
        eprintln!("[DEBUG] --- Chat messages ({} total) ---", messages.len());
        for (i, msg) in messages.iter().enumerate() {
            let preview: String = msg.content.chars().take(200).collect();
            eprintln!("[DEBUG]   [{i}] {} (auto={}): {preview}", msg.role, msg.auto_generated);
        }
        eprintln!("[DEBUG] Views: {}", views.len());
        eprintln!("[DEBUG] ---");
    }

    let mut chat_req = ChatRequest::default().with_system(system_prompt);

    for msg in &messages {
        match msg.role.as_str() {
            "user" => {
                if msg.images.is_empty() {
                    chat_req = chat_req.append_message(GenaiMessage::user(&msg.content));
                } else {
                    let mut parts = vec![ContentPart::from_text(&msg.content)];
                    for img in &msg.images {
                        parts.push(ContentPart::from_text(format!("{}:", img.filename)));
                        parts.push(ContentPart::from_binary_base64(
                            &img.mime_type,
                            img.base64_data.as_str(),
                            Some(img.filename.clone()),
                        ));
                    }
                    chat_req = chat_req.append_message(GenaiMessage::user(
                        MessageContent::from_parts(parts),
                    ));
                }
            }
            "assistant" => {
                chat_req = chat_req.append_message(GenaiMessage::assistant(&msg.content));
            }
            _ => {}
        }
    }

    // Attach orthographic views to the last user message if available
    if !views.is_empty() {
        let view_intro = active_view_name.as_ref().map_or_else(
            || "Current 3D model (active view) rendered from five orthographic/isometric views:".to_string(),
            |name| format!("The user is CURRENTLY SEEING the following $view \"{name}\" in their viewport. Here are five orthographic/isometric views of it:"),
        );
        let mut parts = vec![ContentPart::from_text(view_intro)];
        for (label, base64_png) in views {
            if !base64_png.is_empty() {
                let descriptive_label = match label.as_str() {
                    "Front" => "Front view (Looking from +Y towards origin)",
                    "Right" => "Right view (Looking from +X towards origin)",
                    "Top" => "Top view (Looking from +Z towards origin)",
                    "Bottom" => "Bottom view (Looking from -Z towards origin)",
                    "Iso" => "Isometric view (3/4 perspective)",
                    _ => label,
                };
                parts.push(ContentPart::from_text(format!("{descriptive_label}:")));
                parts.push(ContentPart::from_binary_base64(
                    "image/png",
                    base64_png.as_str(),
                    Some(format!("{label}_view.png")),
                ));
            }
        }
        let view_msg = GenaiMessage::user(MessageContent::from_parts(parts));
        chat_req = chat_req.append_message(view_msg);

        // In debug mode, save orthographic views to var/tmp/ for inspection
        if cfg!(debug_assertions) {
            use base64::Engine;
            let tmp_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("var/tmp");
            if let Err(e) = std::fs::create_dir_all(&tmp_dir) {
                eprintln!("[DEBUG] Failed to create {}: {e}", tmp_dir.display());
            }
            for (label, base64_png) in views {
                if !base64_png.is_empty() {
                    match base64::engine::general_purpose::STANDARD.decode(base64_png) {
                        Ok(bytes) => {
                            let path = tmp_dir.join(format!("{label}_view.png"));
                            match std::fs::write(&path, &bytes) {
                                Ok(()) => eprintln!("[DEBUG] Saved view image: {}", path.display()),
                                Err(e) => eprintln!("[DEBUG] Failed to write {}: {e}", path.display()),
                            }
                        }
                        Err(e) => eprintln!("[DEBUG] Failed to decode {label} base64: {e}"),
                    }
                }
            }
        }
    }

    // Attach views from other $view branches (smaller resolution)
    if !other_views.is_empty() {
        for (view_name, view_images) in other_views {
            let mut parts = vec![ContentPart::from_text(
                format!("Rendered views for $view \"{view_name}\":"),
            )];
            for (label, base64_png) in view_images {
                if !base64_png.is_empty() {
                    parts.push(ContentPart::from_text(format!("{label}:")));
                    parts.push(ContentPart::from_binary_base64(
                        "image/png",
                        base64_png.as_str(),
                        Some(format!("{view_name}_{label}_view.png")),
                    ));
                }
            }
            let view_msg = GenaiMessage::user(MessageContent::from_parts(parts));
            chat_req = chat_req.append_message(view_msg);
        }
    }

    // Ensure the conversation ends with a user message (some APIs require this)
    let ends_with_user = !views.is_empty()
        || !other_views.is_empty()
        || messages.last().is_some_and(|m| m.role == "user");
    if !ends_with_user {
        chat_req = chat_req.append_message(GenaiMessage::user(
            "Please respond to the conversation above.",
        ));
    }

    // Claude's API requires temperature=1 when extended thinking is enabled.
    let is_claude = model_name.contains("claude");
    let effective_temperature = if extended_thinking && is_claude { 1.0 } else { temperature };

    let mut chat_options = ChatOptions::default()
        .with_temperature(effective_temperature)
        .with_capture_content(true)
        .with_capture_reasoning_content(true);

    if extended_thinking {
        chat_options = chat_options.with_reasoning_effort(ReasoningEffort::High);
    }

    // Workaround for Ollama adapter ignoring auth in genai 0.6.0-beta.3
    if let Some(key) = api_key
        && !key.is_empty()
    {
        let headers = genai::Headers::from(("Authorization", format!("Bearer {key}")));
        chat_options.extra_headers = Some(headers);
    }

    let stream_response = client
        .exec_chat_stream(model_name, chat_req, Some(&chat_options))
        .await?;

    let mut stream = std::pin::pin!(stream_response.stream);
    let mut full_content = String::new();
    let mut full_reasoning: Option<String> = None;

    while let Some(event) = stream.next().await {
        match event {
            Ok(ChatStreamEvent::Chunk(chunk)) => {
                full_content.push_str(&chunk.content);
                let _ = tx.send(AiStreamChunk::Chunk(chunk.content));
            }
            Ok(ChatStreamEvent::ReasoningChunk(chunk)) => {
                full_reasoning
                    .get_or_insert_with(String::new)
                    .push_str(&chunk.content);
                let _ = tx.send(AiStreamChunk::ReasoningChunk(chunk.content));
            }
            Ok(ChatStreamEvent::End(_)) => {
                break;
            }
            Ok(_) => {} // Start, ThoughtSignatureChunk, ToolCallChunk
            Err(e) => {
                let err_msg = format!("{e}");
                let _ = tx.send(AiStreamChunk::Error(err_msg));
                return Ok(());
            }
        }
    }

    if full_content.is_empty() {
        full_content = "(no response)".to_string();
    }

    if cfg!(debug_assertions) {
        let preview: String = full_content.chars().take(500).collect();
        eprintln!("[DEBUG] AI response ({} chars): {preview}", full_content.len());
        if let Some(ref r) = full_reasoning {
            eprintln!("[DEBUG] AI reasoning ({} chars)", r.len());
        }
    }

    let _ = tx.send(AiStreamChunk::Done {
        content: full_content,
        reasoning: full_reasoning,
    });

    Ok(())
}

fn ai_receive_system(
    mut chat_state: ResMut<ChatState>,
    mut scad_code: ResMut<ScadCode>,
    ai_config: Res<AiConfig>,
) {
    if !chat_state.is_streaming {
        return;
    }

    // Drain all available chunks from the channel
    let chunks: Vec<AiStreamChunk> = {
        let Some(ref rx_mutex) = chat_state.stream_receiver else {
            return;
        };
        let rx = rx_mutex.lock().unwrap();
        let mut chunks = Vec::new();
        loop {
            match rx.try_recv() {
                Ok(c) => chunks.push(c),
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    if chunks.is_empty() {
                        drop(rx);
                        chat_state.is_streaming = false;
                        chat_state.streaming_start = None;
                        chat_state.stream_receiver = None;
                        return;
                    }
                    break;
                }
            }
        }
        chunks
    };

    for chunk in chunks {
        match chunk {
            AiStreamChunk::Chunk(text) => {
                // Append to the live assistant message (create if needed)
                let append = chat_state
                    .messages
                    .last()
                    .is_some_and(|m| m.role == "assistant" && !m.is_error);
                if append {
                    chat_state.messages.last_mut().unwrap().content.push_str(&text);
                } else {
                    chat_state.messages.push(ChatMessage {
                        role: "assistant".into(),
                        content: text,
                        thinking: None,
                        images: Vec::new(),
                        auto_generated: false,
                        is_error: false,
                    });
                }
            }
            AiStreamChunk::ReasoningChunk(text) => {
                let append = chat_state
                    .messages
                    .last()
                    .is_some_and(|m| m.role == "assistant" && !m.is_error);
                if append {
                    chat_state
                        .messages
                        .last_mut()
                        .unwrap()
                        .thinking
                        .get_or_insert_with(String::new)
                        .push_str(&text);
                } else {
                    chat_state.messages.push(ChatMessage {
                        role: "assistant".into(),
                        content: String::new(),
                        thinking: Some(text),
                        images: Vec::new(),
                        auto_generated: false,
                        is_error: false,
                    });
                }
            }
            AiStreamChunk::Done { content, reasoning } => {
                // Replace the live message with the final version
                let replace = chat_state
                    .messages
                    .last()
                    .is_some_and(|m| m.role == "assistant" && !m.is_error);
                if replace {
                    let last = chat_state.messages.last_mut().unwrap();
                    last.content.clone_from(&content);
                    last.thinking = reasoning;
                }
                chat_state.is_streaming = false;
                chat_state.streaming_start = None;
                chat_state.stream_receiver = None;

                let code_changed = match extract_code_change(&content) {
                    Some(CodeChange::FullReplace(new_code)) => {
                        scad_code.text = new_code;
                        true
                    }
                    Some(CodeChange::SearchReplace(replacements)) => {
                        match apply_search_replace(&scad_code.text, &replacements) {
                            Ok(new_code) => {
                                scad_code.text = new_code;
                                true
                            }
                            Err(err) => {
                                eprintln!("[SynapsCAD] Search-and-replace failed: {err}");
                                // Try full replacement as fallback
                                if let Some(full) = extract_openscad_code(&content) {
                                    scad_code.text = full;
                                    true
                                } else {
                                    false
                                }
                            }
                        }
                    }
                    None => false,
                };

                if code_changed {
                    scad_code.dirty = true;

                    let round = match &chat_state.verification {
                        VerificationState::Verifying(n) => *n,
                        _ => 0,
                    };

                    if round < ai_config.max_verification_rounds {
                        chat_state.verification = VerificationState::WaitingForCompilation;
                    } else {
                        chat_state.verification = VerificationState::Idle;
                    }
                } else {
                    chat_state.verification = VerificationState::Idle;
                }
                return;
            }
            AiStreamChunk::Error(err) => {
                // Remove partial streaming message if present
                if chat_state
                    .messages
                    .last()
                    .is_some_and(|m| m.role == "assistant" && !m.is_error)
                {
                    chat_state.messages.pop();
                }
                // Restore the user's last message back to input so they can retry
                if let Some(last_user_msg) = chat_state
                    .messages
                    .iter()
                    .rposition(|m| m.role == "user" && !m.auto_generated)
                {
                    let msg = chat_state.messages.remove(last_user_msg);
                    chat_state.input_buffer = msg.content;
                    chat_state.pending_images = msg.images;
                }
                chat_state.messages.push(ChatMessage {
                    role: "assistant".into(),
                    content: err,
                    thinking: None,
                    images: Vec::new(),
                    auto_generated: false,
                    is_error: true,
                });
                chat_state.is_streaming = false;
                chat_state.streaming_start = None;
                chat_state.stream_receiver = None;
                chat_state.verification = VerificationState::Idle;
                return;
            }
        }
    }
}

/// Watches for compilation to finish after AI-produced code, then triggers verification.
fn ai_verify_system(
    mut chat_state: ResMut<ChatState>,
    compilation_state: Res<super::compilation::CompilationState>,
    ai_config: Res<AiConfig>,
) {
    match chat_state.verification {
        VerificationState::WaitingForCompilation => {
            // Wait until compilation finishes
            if !compilation_state.is_compiling {
                chat_state.verification = VerificationState::ReadyToVerify;
            }
        }
        VerificationState::ReadyToVerify => {
            // Determine which round this will be
            #[allow(clippy::cast_possible_truncation)]
            let round = chat_state
                .messages
                .iter()
                .rev()
                .take_while(|m| m.role != "user" || m.auto_generated)
                .filter(|m| m.role == "user" && m.auto_generated)
                .count() as u32
                + 1;

            let max_label = if ai_config.max_verification_rounds == u32::MAX {
                "∞".to_string()
            } else {
                ai_config.max_verification_rounds.to_string()
            };

            // Inject a verification user message
            chat_state.messages.push(ChatMessage {
                role: "user".into(),
                content: format!("[Verification round {round}/{max_label}] {VERIFICATION_PROMPT}"),
                thinking: None,
                images: Vec::new(),
                auto_generated: true,
                is_error: false,
            });

            // Trigger the AI send
            chat_state.is_streaming = true;
            chat_state.streaming_start = Some(std::time::Instant::now());
            chat_state.verification = VerificationState::Verifying(round);
        }
        _ => {}
    }
}

/// Build part context describing the compiled parts (@1, @2, ...) for the AI.
fn build_part_context(part_query: &Query<&PartLabel>) -> String {
    use std::fmt::Write;
    let mut parts: Vec<&PartLabel> = part_query.iter().collect();
    if parts.is_empty() {
        return String::new();
    }
    parts.sort_by_key(|p| p.index);

    let mut ctx = String::from("Compiled parts:\n");
    for part in &parts {
        let [r, g, b] = part.color;
        let _ = writeln!(
            ctx,
            "  {}: color=({:.2}, {:.2}, {:.2})",
            part.label, r, g, b
        );
    }
    ctx.push_str("When the user references @N, it refers to the part listed above.\n");
    ctx
}

/// Result of extracting code from an AI response.
enum CodeChange {
    /// Full replacement — the AI sent a complete `synapscad` code block.
    FullReplace(String),
    /// Search-and-replace pairs — the AI sent `<<<REPLACE` blocks.
    SearchReplace(Vec<(String, String)>),
}

/// Extracts code changes from AI response.
/// First tries `<<<REPLACE` search-and-replace blocks, then falls back to full `synapscad` block.
fn extract_code_change(text: &str) -> Option<CodeChange> {
    // Try search-and-replace first
    let replacements = parse_search_replace(text);
    if !replacements.is_empty() {
        return Some(CodeChange::SearchReplace(replacements));
    }

    // Fall back to full replacement
    extract_openscad_code(text).map(CodeChange::FullReplace)
}

/// Parses `<<<REPLACE` / `===` / `>>>` blocks from AI response.
fn parse_search_replace(text: &str) -> Vec<(String, String)> {
    let mut results = Vec::new();
    let mut remaining = text;

    while let Some(start) = remaining.find("<<<REPLACE") {
        let after_marker = &remaining[start + "<<<REPLACE".len()..];
        // Skip to newline after <<<REPLACE
        let after_newline = if let Some(nl) = after_marker.find('\n') {
            &after_marker[nl + 1..]
        } else {
            break;
        };

        // Find the separator ===
        let Some(separator) = after_newline.find("\n===\n") else { break; };

        let old_str = &after_newline[..separator];

        let after_sep = &after_newline[separator + "\n===\n".len()..];

        // Find closing >>>
        let Some(end) = after_sep.find("\n>>>") else { break; };

        let new_str = &after_sep[..end];

        if !old_str.is_empty() {
            results.push((old_str.to_string(), new_str.to_string()));
        }

        remaining = &after_sep[end + "\n>>>".len()..];
    }

    results
}

/// Applies search-and-replace pairs to the current code buffer.
/// Returns the modified code, or None if any replacement failed to find its target.
fn apply_search_replace(code: &str, replacements: &[(String, String)]) -> Result<String, String> {
    let mut result = code.to_string();
    for (i, (old, new)) in replacements.iter().enumerate() {
        let count = result.matches(old.as_str()).count();
        if count == 0 {
            return Err(format!(
                "Search-and-replace #{}: could not find the target text in the code",
                i + 1
            ));
        }
        if count > 1 {
            return Err(format!(
                "Search-and-replace #{}: target text appears {} times (must be unique)",
                i + 1,
                count
            ));
        }
        result = result.replacen(old.as_str(), new.as_str(), 1);
    }
    Ok(result)
}

/// Extracts `OpenSCAD` code from AI response.
/// Supports ` ```synapscad ` and ` ```openscad ` code blocks (ignores any `:suffix`).
fn extract_openscad_code(text: &str) -> Option<String> {
    // Try both markers
    let markers = ["```synapscad", "```openscad"];
    
    for marker in &markers {
        if let Some(start) = text.find(marker) {
            let rest = &text[start + marker.len()..];

            // Skip any :suffix and find the newline
            let newline = rest.find('\n').unwrap_or(0);
            let code_rest = &rest[newline..];
            let end = code_rest.find("```")?;
            let code = code_rest[..end].trim().to_string();
            if !code.is_empty() {
                return Some(code);
            }
        }
    }
    
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_search_replace_single() {
        let text = "Here's the change:\n\n<<<REPLACE\ncube(10);\n===\ncube(20);\n>>>\n\nDone!";
        let pairs = parse_search_replace(text);
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].0, "cube(10);");
        assert_eq!(pairs[0].1, "cube(20);");
    }

    #[test]
    fn test_parse_search_replace_multiple() {
        let text = "<<<REPLACE\ncube(10);\n===\ncube(20);\n>>>\n\n<<<REPLACE\nsphere(5);\n===\nsphere(10);\n>>>";
        let pairs = parse_search_replace(text);
        assert_eq!(pairs.len(), 2);
        assert_eq!(pairs[0].0, "cube(10);");
        assert_eq!(pairs[0].1, "cube(20);");
        assert_eq!(pairs[1].0, "sphere(5);");
        assert_eq!(pairs[1].1, "sphere(10);");
    }

    #[test]
    fn test_parse_search_replace_multiline() {
        let text = "<<<REPLACE\nmodule foo() {\n    cube(10);\n}\n===\nmodule foo() {\n    cube(20);\n    sphere(5);\n}\n>>>";
        let pairs = parse_search_replace(text);
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].0, "module foo() {\n    cube(10);\n}");
        assert_eq!(pairs[0].1, "module foo() {\n    cube(20);\n    sphere(5);\n}");
    }

    #[test]
    fn test_parse_search_replace_empty_new() {
        let text = "<<<REPLACE\ncube(10);\n===\n\n>>>";
        let pairs = parse_search_replace(text);
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].0, "cube(10);");
        assert_eq!(pairs[0].1, "");
    }

    #[test]
    fn test_parse_search_replace_none() {
        let text = "Just some text without any replace blocks.";
        let pairs = parse_search_replace(text);
        assert!(pairs.is_empty());
    }

    #[test]
    fn test_apply_search_replace_ok() {
        let code = "cube(10);\nsphere(5);";
        let replacements = vec![("cube(10);".into(), "cube(20);".into())];
        let result = apply_search_replace(code, &replacements).unwrap();
        assert_eq!(result, "cube(20);\nsphere(5);");
    }

    #[test]
    fn test_apply_search_replace_not_found() {
        let code = "cube(10);";
        let replacements = vec![("cylinder(5);".into(), "cylinder(10);".into())];
        assert!(apply_search_replace(code, &replacements).is_err());
    }

    #[test]
    fn test_apply_search_replace_ambiguous() {
        let code = "cube(10);\ncube(10);";
        let replacements = vec![("cube(10);".into(), "cube(20);".into())];
        assert!(apply_search_replace(code, &replacements).is_err());
    }

    #[test]
    fn test_extract_code_change_prefers_replace() {
        let text = "<<<REPLACE\ncube(10);\n===\ncube(20);\n>>>\n\n```synapscad\ncube(99);\n```";
        match extract_code_change(text) {
            Some(CodeChange::SearchReplace(pairs)) => {
                assert_eq!(pairs.len(), 1);
                assert_eq!(pairs[0].1, "cube(20);");
            }
            _ => panic!("Expected SearchReplace"),
        }
    }

    #[test]
    fn test_extract_code_change_full_replace() {
        let text = "Here's the code:\n\n```synapscad\ncube(10);\n```";
        match extract_code_change(text) {
            Some(CodeChange::FullReplace(code)) => {
                assert_eq!(code, "cube(10);");
            }
            _ => panic!("Expected FullReplace"),
        }
    }

    #[test]
    fn test_extract_code_change_full_replace_openscad() {
        let text = "Here's the code:\n\n```openscad\nsphere(5);\n```";
        match extract_code_change(text) {
            Some(CodeChange::FullReplace(code)) => {
                assert_eq!(code, "sphere(5);");
            }
            _ => panic!("Expected FullReplace"),
        }
    }

    #[test]
    fn test_extract_code_change_none() {
        let text = "No code here, just a description.";
        assert!(extract_code_change(text).is_none());
    }

    #[test]
    fn test_extract_openscad_code_synapscad() {
        let text = "Here's the code:\n\n```synapscad\ncube(10);\n```";
        let code = extract_openscad_code(text);
        assert_eq!(code, Some("cube(10);".into()));
    }

    #[test]
    fn test_extract_openscad_code_openscad() {
        let text = "Here's the code:\n\n```openscad\nsphere(5);\n```";
        let code = extract_openscad_code(text);
        assert_eq!(code, Some("sphere(5);".into()));
    }

    #[test]
    fn test_extract_openscad_code_with_suffix() {
        let text = "Code with suffix:\n\n```synapscad:example\ncylinder(r=5, h=10);\n```";
        let code = extract_openscad_code(text);
        assert_eq!(code, Some("cylinder(r=5, h=10);".into()));
    }

    #[test]
    fn test_extract_openscad_code_openscad_with_suffix() {
        let text = "Code with suffix:\n\n```openscad:test\ntranslate([10,0,0]) cube(5);\n```";
        let code = extract_openscad_code(text);
        assert_eq!(code, Some("translate([10,0,0]) cube(5);".into()));
    }

    #[test]
    fn test_extract_openscad_code_prefers_synapscad() {
        let text = "Two blocks:\n\n```openscad\nsphere(5);\n```\n\n```synapscad\ncube(10);\n```";
        let code = extract_openscad_code(text);
        assert_eq!(code, Some("cube(10);".into()));
    }

    #[test]
    fn test_extract_openscad_code_empty() {
        let text = "Empty code:\n\n```synapscad\n\n```";
        let code = extract_openscad_code(text);
        assert_eq!(code, None);
    }
}
