# Changelog

All notable changes to this project will be documented in this file.

## [0.1.0] - 2026-02-27

- Initial release

## [0.2.0] - 2026-06-27

- First binary release with pre-built executables for Linux, macOS (Apple Silicon & Intel), and Windows

## [0.2.1] - 2026-06-27

- Fix: Updated README with correct release version and download instructions

## [0.2.2] - 2026-06-27

- Fix: Changed build target macos-13 to macos-latest


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