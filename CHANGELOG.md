# Changelog

All notable changes to this project will be documented in this file.


## [0.7.1] - 2026-03-04

### Changed

- **Splash Screen Timer** — adjusted default splash screen display duration.
- **Markdown Rendering** — improved header detection and checklist rendering in chat messages.

### Fixed

- **Side Panel Auto-Expansion** — fixed a bug where the left panel would auto-expand when adding attachments, submitting chat messages, or resizing content. The panel width now stays fixed unless manually resized by the user via the drag handle.
- **Attachment Filename Overflow** — long attachment filenames (e.g., screenshot names) no longer push the panel wider. Filenames are truncated to 20 characters with full name shown on hover tooltip. Multiple attachments wrap to new lines instead of extending horizontally.
- **Color Parsing** — fixed color parsing to support hex color strings in `parse_color_args`.
- **Zoom Limits** — extended from 0.5–1000.0 to 0.1–5000.0 for both mouse and keyboard, so you can zoom much further in/out before hitting limits


## [0.7.0] - 2026-03-03

### Added

- **Enhanced Chat UI** — improved message styling with distinct background colors for user, AI, and error messages.
- **Thinking Process Display** — collapsible "thinking" section in chat responses to show the model's reasoning process.
- **Streaming Indicator** — visual feedback while the AI is generating a response.
- **$fn dropdown** — quick selection of common $fn values in the code editor toolbar.
- **Chat History Draft** — if you start cycling through previous messages and then return to the draft, your unsent text is preserved.
- **Dynamic Grid** — the XYZ grid now grows automatically based on the model's bounding box (with margin), minimum 50 units.
- **Grid Toggle (`G` key)** — toggling grid visibility now correctly applies to dynamically resized grids.
- **Agent Timer** — elapsed time is shown next to the spinner while the AI is working.

### Changed

- **Part Label Contrast** — improved visibility of part labels against the background.
- **Auto-scroll Behavior** — chat now smarter about scrolling to new messages vs. preserving scroll position.
- **Refactored Codebase** — split large `ui.rs` and `compilation.rs` files into modular components for better maintainability.
- **AI Context Improvements** — added physical realism checks in AI instructions.

### Fixed

- **UI Overlap** — Part labels are now hidden when they would overlap with the top viewport toolbar or the left side panel, preventing visual clutter.
- **Markdown Rendering** — Fixed bold text rendering (`**text**`) in chat messages and thinking blocks, ensuring inline bolding works correctly and markers are hidden.
- **BMesh Transformations** — Refactor BMesh transformations to include fallback to CsgMesh on panic.


## [0.6.0] - 2026-03-02

### Added

- **Better AI context for views** — the AI now knows exactly which `$view` you are currently seeing in the viewport. Standard orthographic views (Front, Right, Top, Bottom, Iso) include descriptive orientation labels (e.g., "Looking from +Y towards origin") for better spatial grounding.
- **Physical Realism guidelines** — the AI's internal instructions now emphasize checking the physical "fit" and structural integrity of individual parts in multi-part assemblies, including proper tolerances and alignment.

### Changed

- **Improved chat auto-scrolling** — the chat now respects your manual scrolling. It will only "stick to the bottom" if you are already at the end of the conversation. If you scroll up to read previous messages, new incoming text won't force-scroll you back down.

### Fixed

- **UI Overflow** — the "View" selector and "Attached" image strip now use wrapped layouts. If you have many views or attachments, they will wrap to new lines instead of overflowing the right edge of the sidebar.
- **Compilation error in UI system** — fixed a Rust compile error where `CompilationState` was incorrectly accessed as an immutable resource during a zoom-to-fit request.


## [0.5.2] - 2026-03-01

### Fixed

- **Non-manifold mesh fallback** — parts that fail manifold creation (e.g. thin `linear_extrude`) now render via direct polygon conversion instead of being silently dropped
- **Removed unsafe code** — bumped `genai` to 0.6.0-beta.3 which threads auth resolver through `all_model_names()`, eliminating the `set_var` workaround; `unsafe_code` lint reverted to `forbid`
- **Verification state reset** — verification state now properly resets to Idle after AI streaming ends


## [0.5.1] - 2026-03-01

### Fixed

- **Per-provider API keys** — each AI provider (Anthropic, OpenAI, Gemini, etc.) now stores its own API key; switching providers no longer loses your key
- **Per-provider model memory** — switching between providers remembers your last-used model for each
- **Multi-view context for AI** — when using `$view` branches, all views are rendered and sent to the AI as context (non-active views at 128px for efficiency)
- **View image cycling with spinner** — while waiting for AI response, cycles through rendered model views with a spinner overlay
- **Stale view images after code clear** — clearing code now properly clears cached view textures instead of showing old images
- **View cycling images not displaying** — textures are now cached across frames for proper GPU upload instead of re-created each frame


## [0.5.0] - 2026-02-28

### Added

- **Search-and-replace diffs** — AI can now send targeted `<<<REPLACE` / `===` / `>>>` blocks instead of full code replacement, saving tokens on large scripts. Falls back to full replacement automatically.
- **Syntax-highlighted code blocks** in AI chat responses — OpenSCAD/synapscad code uses the same color scheme as the editor (keywords, builtins, strings, numbers, comments)
- **Bottom and Isometric views** — AI now receives 5 rendered views (Front, Right, Top, Bottom, Iso) for better spatial understanding
- **Chat input history preserves images** — pressing ↑/↓ in chat input restores both text and attached images
- **Session-aware chat** — after app restart, previous chat messages are displayed but not re-sent to the AI, preventing context pollution from old sessions
- **Code clear resets AI chat** — clearing the code editor also resets the AI chat for a fresh session

### Fixed

- **Error messages always expanded** — error responses in chat are forced open regardless of persisted collapse state
- **macOS .app bundle launch** — added `NSPrincipalClass` to Info.plist and ad-hoc code signing in release workflow to prevent Gatekeeper blocking
- **Verification prompt rendering** — backtick-fenced text in verification prompts no longer incorrectly rendered as code blocks

### Changed

- **Splash screen** duration reduced from 3s to 1.5s (fade from 0.5s to 0.3s)


## [0.4.0] - 2026-02-28

### Added

- AI response streaming — see model output as it's generated, including live thinking/reasoning display
- Multiline chat input (3 rows) with word wrap; Enter sends, Shift+Enter inserts newline
- Compilation errors and warnings highlighted in red (⚠) in chat
- Compact icon-only Send (⬆), Stop (⏹), and Attach (📎) buttons
- Chat auto-scrolls to latest streaming content
- Debug mode: orthographic view images saved to `var/tmp/` for inspection
- App icon for macOS (.app bundle with .icns) and Windows (embedded .ico)

### Fixed

- `intersection_for` now correctly intersects all iteration results (was incorrectly treated as `for`/union)
- Boolean operation panics no longer cascade — failed parts are skipped with a warning, other parts still render
- BSP-tree boolean fallback: when boolmesh panics, operations automatically retry using csgrs BSP booleans
- AI model selection restored correctly after app restart (was being cleared during model list fetch)
- User input and image attachments preserved on AI stream errors (no longer lost on retry)

### Changed

- Chat messages use `is_error` flag for reliable error styling (no string matching)
- Boolean operations refactored into `bool_op_with_fallback` for unified boolmesh → BSP fallback logic
- `Shape::Failed` variant prevents corrupted geometry from propagating through subsequent operations


## [0.3.0] - 2026-02-28

### Added

- Toggle part labels (@1, @2, ...) visibility with toolbar button or `L` key
- Keyboard shortcuts cheatsheet dialog — open via toolbar `⌨` button or `?` key, close with `Esc`
- UI settings persistence — label visibility is now remembered across sessions
- Save-on-exit and immediate save on UI setting changes (no longer relying solely on 30-second auto-save)

### Changed

- Persistence config now has a `ui` section for UI-related settings (backward compatible)
- AGENTS.md updated with reminders to maintain keyboard shortcuts in both README and in-app cheatsheet

## [0.2.3] - 2026-02-28

### Added

- API keys entered in ⚙ AI Settings are now persisted across sessions
- "Set API key first" hint shown in settings when no API key is configured
- Local model support via Ollama highlighted in README

### Changed

- Model list is now fetched live from the provider API — no more hardcoded fallback models
- Model list and selection are cleared immediately when the API key or provider changes, preventing stale models from being shown

### Fixed

- API keys entered via the UI were not used for fetching the model list (genai workaround)


## [0.2.2] - 2026-06-27

- Fix: Changed build target macos-13 to macos-latest


## [0.2.1] - 2026-06-27

- Fix: Updated README with correct release version and download instructions


## [0.2.0] - 2026-06-27

- First binary release with pre-built executables for Linux, macOS (Apple Silicon & Intel), and Windows


## [0.1.0] - 2026-02-27

- Initial release