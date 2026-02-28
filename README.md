# SynapsCAD

<p align="center">
  <img src="assets/splash@2x.png" alt="SynapsCAD" width="280" />
</p>

<p align="center">
  The AI-powered 3D CAD IDE — edit code, visualize in 3D, and reshape your designs with natural language.
</p>

<br>

> ⚠️ **Early Prototype** — Not all OpenSCAD code will compile correctly yet. Start with simple models and expect rough edges. Bug reports with code snippets that cause issues are very welcome!

<br>

## See SynapsCAD in Action

[![SynapsCAD Demo](https://img.youtube.com/vi/cN8a5UozS5Q/maxresdefault.jpg)](https://www.youtube.com/watch?v=cN8a5UozS5Q)

## Overview

A desktop 3D CAD application that combines an OpenSCAD code editor, a real-time 3D viewport, and an AI assistant. Write OpenSCAD code, compile it to 3D models, visualize them interactively, and use AI to modify your designs through natural language — including context from 3D click interactions.

## Download

Pre-built binaries for Linux, macOS (Apple Silicon & Intel), and Windows are available on the [Releases](https://github.com/ierror/synaps-cad/releases) page.

## Building from Source

### Prerequisites

- **Rust** (stable toolchain)
- An AI provider API key (e.g. `ANTHROPIC_API_KEY`) for the chat assistant

### Quick Start

```sh
cargo run
```

### AI Provider Setup

SynapsCAD uses the [genai](https://crates.io/crates/genai) crate to connect to AI providers — including **local models via [Ollama](https://ollama.com)** for fully offline, private usage (no API key needed). Set the API key for your chosen cloud provider as an environment variable:

| Provider  | Environment Variable |
| --------- | -------------------- |
| Anthropic | `ANTHROPIC_API_KEY`  |
| OpenAI    | `OPENAI_API_KEY`     |
| Gemini    | `GEMINI_API_KEY`     |
| Groq      | `GROQ_API_KEY`       |
| DeepSeek  | `DEEPSEEK_API_KEY`   |
| Cohere    | `COHERE_API_KEY`     |
| Fireworks | `FIREWORKS_API_KEY`  |
| Together  | `TOGETHER_API_KEY`   |
| xAI       | `XAI_API_KEY`        |
| ZAI       | `ZAI_API_KEY`        |
| Ollama    | _(no key needed)_    |

```sh
export ANTHROPIC_API_KEY="sk-..."
cargo run
```

When an env var is set, the UI shows it as active. You can also enter or override the key in **⚙ AI Settings** within the app.

This opens a window with a 3D viewport on the right and a side panel on the left containing the code editor and AI chat.

### Basic Workflow

1. Write or edit OpenSCAD code in the editor panel
2. Click **Compile** — SynapsCAD parses and evaluates the code using scad-rs and renders CSG geometry via csgrs
3. Ask the AI assistant to modify your model — it sees your current code and part labels, and can update the code automatically

## Architecture Overview

SynapsCAD is a single-binary Rust application built on three main pillars:

### Runtime Stack

| Layer            | Technology                                               | Role                                                               |
| ---------------- | -------------------------------------------------------- | ------------------------------------------------------------------ |
| Rendering & ECS  | **Bevy 0.15**                                            | 3D viewport, entity management, frame loop                         |
| UI               | **bevy_egui** (egui 0.31)                                | Side panel with code editor and chat interface                     |
| OpenSCAD parsing | [**openscad-rs**](https://github.com/ierror/openscad-rs) | Lossless, resilient OpenSCAD parser                                |
| CSG rendering    | **csgrs**                                                | Constructive solid geometry — boolean ops, primitives, mesh output |
| Export           | **lib3mf**                                               | 3MF export with per-part colors; STL and OBJ exported natively     |
| AI               | **genai**                                                | Unified client for OpenAI / Anthropic / Gemini APIs                |
| Async            | **Tokio**                                                | Background runtime for AI network calls                            |

### Key Design Decisions

- **Bevy owns the main thread.** The Bevy app loop drives rendering and ECS systems. A separate Tokio runtime is stored as a Bevy `Resource` and used only for spawning async AI tasks.

- **`std::sync::mpsc` bridges async to sync.** Background tasks (compilation, AI streaming) send results through channels. Bevy systems poll with non-blocking `try_recv()` each frame, keeping the viewport responsive.

- **Pure-Rust compilation pipeline.** OpenSCAD code is parsed by `scad-syntax`, evaluated by a built-in AST walker, and rendered to triangle meshes via `csgrs` — no external tools or WASM required.

- **Built-in mesh picking.** Bevy 0.15's `MeshPickingPlugin` provides ray-cast picking via the observer pattern — no external picking crate needed.

### System Pipeline

```
ui_layout_system          — render egui side panel (editor + chat)
    ↓
trigger_compilation_system — if code is dirty, spawn compilation in a thread
    ↓
poll_compilation_system    — check if compilation finished, load mesh
    ↓
ai_send_system             — if chat submitted, spawn AI request on tokio
    ↓
ai_receive_system          — poll AI response, update chat + code
    ↓
adjust_camera_viewport     — resize 3D viewport to account for side panel
    ↓
orbit_camera_system        — process mouse/keyboard input for 3D navigation
    ↓
zoom_to_fit_system         — auto-frame model after compilation
```

## 3D Viewport Navigation

SynapsCAD uses **Blender-style** camera controls:

| Action          | Control                                               |
| --------------- | ----------------------------------------------------- |
| **Orbit**       | Middle mouse button drag _or_ Right mouse button drag |
| **Pan**         | Shift + Middle mouse button drag                      |
| **Zoom**        | Scroll wheel, `+`/`-` keys                            |
| **Move focus**  | `W`/`A`/`S`/`D` or Arrow keys                         |
| **Front view**  | `1`                                                   |
| **Back view**   | `2`                                                   |
| **Right view**  | `3`                                                   |
| **Left view**   | `4`                                                   |
| **Top view**    | `5`                                                   |
| **Bottom view** | `6`                                                   |
| **Isometric**   | `7`                                                   |

## Contact

[@boerni@chaos.social](https://chaos.social/@boerni)

## License

GPL v3
