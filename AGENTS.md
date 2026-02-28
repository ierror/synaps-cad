# AGENTS.md — Architecture Overview for AI Agents

## Purpose

This document describes the SynapsCAD architecture for AI agents (LLMs, copilots, and automated tools) that work with this codebase. It covers the system design, plugin structure, and the **part labeling** concept that enables users to reference specific 3D geometry parts in conversations with AI.

## System Architecture

SynapsCAD is a single-binary Rust desktop app built on **Bevy 0.15** (ECS game engine) with **egui** for UI.

### Data Flow

```
SynapsCAD code (editor)
    ↓  trigger_compilation_system
Compiler thread (scad-rs parser → AST evaluator → csgrs CSG → boolmesh)
    ↓  mpsc channel
poll_compilation_system
    ↓  spawns Bevy entities
3D Viewport (Bevy renderer)
    ↓  egui overlay
Part Labels (@1, @2, ...)
    ↓  system prompt injection
AI Chat (genai → Anthropic/OpenAI/Gemini/...)
    ↓  code block extraction
Code Editor (auto-apply)
```

### Plugin Structure (`src/plugins/`)

| Plugin              | File             | Responsibility                                                           |
| ------------------- | ---------------- | ------------------------------------------------------------------------ |
| `ScenePlugin`       | `scene.rs`       | Camera, lights, axes, grid setup                                         |
| `CodeEditorPlugin`  | `code_editor.rs` | OpenSCAD text editor, undo/redo, view detection (`$view` variable)       |
| `CompilationPlugin` | `compilation.rs` | Triggers compilation, spawns mesh entities with `CadModel` + `PartLabel` |
| `CameraPlugin`      | `camera.rs`      | Orbit/pan/zoom controls, zoom-to-fit, keyboard toggles (G=gizmos, L=labels) |
| `UiPlugin`          | `ui.rs`          | egui side panel layout, viewport toolbar, label overlays                 |
| `AiChatPlugin`      | `ai_chat.rs`     | AI streaming chat with context injection                                 |
| `PersistencePlugin` | `persistence.rs` | Save/load settings and code                                              |

### Key Files

| File                | Purpose                                                                                                                     |
| ------------------- | --------------------------------------------------------------------------------------------------------------------------- |
| `src/compiler.rs`   | OpenSCAD → triangle mesh pipeline. Evaluates the AST, creates CSG primitives, performs boolean ops, outputs `Vec<MeshData>` |
| `src/main.rs`       | App entry point, registers all plugins                                                                                      |
| `src/app_config.rs` | Developer constants (not user-facing)                                                                                       |

## Labels (`@N`)

### Part Numbering

Parts use a `@N` numbering scheme:

- Parts are numbered `@1`, `@2`, ... (1-based, in order of top-level geometry)

Each part gets a **unique color** (from `PART_PALETTE` or from `color()` in code). The label is rendered as a billboard overlay at the part's AABB center.

### Why This Matters for Agents

When a user says _"make @2 taller"_ or _"change the color of @1"_, the AI agent knows exactly which geometric part is being referenced. The label system provides:

1. **Visual identification** — colored labels in the viewport
2. **AI context** — part index, color, and bounding box injected into the system prompt
3. **Stable references** — part numbers correspond to top-level geometry statements in order

### How Parts Map to Code

Parts are created by the compiler in the order of top-level geometry statements:

```openscad
cube(10);           // → @1
translate([20,0,0])
    sphere(5);      // → @2
```

## For Agent Developers

### Code Block Namespace

AI-generated code uses the **`synapscad`** namespace in fenced code blocks:

- ` ```synapscad ` — wraps the full script to replace in the editor

The parser in `extract_openscad_code()` (`ai_chat.rs`) extracts code from the block and replaces the entire editor buffer.

### View System (`$view` variable)

SynapsCAD uses a **single editor buffer** with a `$view` variable to switch between views/parts of a model. Views are defined as modules and selected via `if ($view == "name")` conditionals.

**Pattern:**

```openscad
$view = "main";

module view_main() { cube(10); }
module view_assembly() { view_main(); translate([20,0,0]) sphere(5); }

if ($view == "main") view_main();
if ($view == "assembly") view_assembly();
```

**How it works:**

1. The UI parses `$view == "xxx"` occurrences to discover available views
2. A view selector appears in the Code heading row when multiple views exist
3. Selecting a view text-replaces the `$view = "..."` assignment line
4. Only the matching `if` branch executes → only that view is compiled/rendered

**Implementation:**

- `detect_views(code)` in `code_editor.rs` — returns `(active_view, all_views)`
- `set_active_view(code, name)` in `code_editor.rs` — text-replaces the assignment
- View selector dropdown in `ui.rs` Code heading row
- AI writes full scripts with `$view`, all modules, and all `if` selectors

**Important:** There is no multi-buffer/tab system. All code lives in one editor. When the AI provides code, it replaces the entire buffer.

### Making Code Changes

1. The compiler (`src/compiler.rs`) is the core — it evaluates OpenSCAD AST and produces meshes
2. UI changes go in `src/plugins/ui.rs` (egui-based)
3. New Bevy components/systems go in the appropriate plugin file
4. Tests live at the bottom of `compiler.rs` (unit tests) and `tests/openscad_examples/` (integration)

### Error Handling

**Never silently discard errors.** All compiler, renderer, and mesh processing errors must be surfaced to the user — not swallowed with `eprintln!` and an empty fallback.

- **Evaluator warnings** are collected in `Evaluator.warnings` (e.g. unsupported modules, recursion limits, extrude issues) and shown to the user after compilation.
- **Mesh errors** (non-manifold, empty vertices) propagate as `Result::Err` and are shown per-part. Partial models still render — only the failing part is skipped.
- **Panic recovery** via `catch_unwind` catches crashes in dependencies (boolmesh, csgrs) and surfaces them as user-visible errors.
- **Bug-report hints**: Internal errors (panics, non-manifold mesh) include a message asking the user to report the bug with their code snippet.

When adding new features, use `self.warnings.push(...)` in the `Evaluator` for recoverable issues, and `return Err(...)` for fatal per-part failures.

### Running

```sh
cargo run          # launch the app
cargo test         # run all tests
cargo clippy       # lint
```

### Testing Philosophy

- **Reference comparison tests**: compare output bounding box and triangle count against OpenSCAD reference data
- **No-panic tests**: for features using dependencies with known issues (spade, csgrs), verify they don't crash
- **Unit tests**: for specific compiler features (cones, polyhedra, boolean ops)

## UI Conventions

- **Hand cursor on hover**: All clickable/interactive widgets display a pointing-hand cursor. This is set globally via `style.interaction.interact_cursor` in `setup_egui_theme` — do **not** set cursors per-widget.
- **AI Settings**: Opened via a ⚙ gear button in the "AI Assistant" header row; rendered as a floating `egui::Window`, not inline.
- **Compile button**: Right-aligned in the "Code" heading row.

## Part Colors

When generating models, always use `color()` to give each part a realistic, semantically meaningful color:

- **Green** for plants, leaves, grass
- **Brown** for wood, soil, tree trunks
- **Red** for flowers, berries, fire
- **Gray** for metal, concrete, stone
- **Blue** for water, sky, ice
- **White** for snow, clouds
- **Orange** for flames, autumn leaves

Example: `color("green") cylinder(h = 20, r = 3);` for a plant stem.

## Maintaining This Document

**Keep `AGENTS.md` up to date.** When making architecture-relevant changes — new plugins, new resources/components, changed data flow, new UI patterns, or new conventions — update this file so that AI agents always have accurate context about the codebase.

**Keep `README.md` keyboard shortcuts table up to date.** When adding or changing keyboard shortcuts (in `camera.rs` or elsewhere), update the "Keyboard Shortcuts" and "3D Viewport Navigation" sections in `README.md` to match.

**Keep the in-app keyboard cheatsheet up to date.** The `cheatsheet_system` in `ui.rs` contains a `shortcuts` array listing all keyboard shortcuts shown to the user. When adding or changing shortcuts, update that array alongside the `README.md` tables.
