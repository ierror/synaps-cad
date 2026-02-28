# Changelog

All notable changes to this project will be documented in this file.

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