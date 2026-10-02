# Changelog

All notable changes to Herbarium are documented here.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed
- Linux: the app icon now shows on Wayland (KDE Plasma). The desktop entry is installed as `herbarium.desktop` so it matches the window's app id.
- Clippy warnings.

### Changed
- GitHub release descriptions are now taken from this changelog.

## [0.1.0] - 2026-10-02

First release.

### Added
- Tauri 2 + Svelte 5 desktop app for keeping generated HTML pages in a plain-file vault and reviewing them at the right time.
- Botanical design system with a command palette, toasts, an inspector panel and a review flow.
- Light and dark themes.
- English (default) and French interface.
- Import dialog with a staged file list (sizes, remove, duplicate detection), drag-and-drop feedback, and paste-HTML import.
- Content validation on import: files renamed to `.html` that are not real HTML are rejected, in both the picker and the backend.
- README, CI workflow, and a tag-driven release workflow producing `.deb` and `.AppImage` bundles.

[Unreleased]: https://github.com/abdoufermat5/herbarium/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/abdoufermat5/herbarium/releases/tag/v0.1.0
