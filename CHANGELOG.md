# Changelog

All notable changes to Herbarium are documented here.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Tauri 2 + Svelte 5 desktop app for keeping generated HTML pages in a plain-file vault and reviewing them at the right time.
- Botanical design system with a command palette, toasts, an inspector panel and a review flow.
- Light and dark themes.
- English (default) and French interface.
- Settings page for theme, interface language, library layout and sorting, with automatic preference persistence and the current vault location. Open it from the sidebar, command palette, or `Ctrl+,` / `⌘,`.
- Import dialog with a staged file list (sizes, remove, duplicate detection), drag-and-drop feedback, and paste-HTML import.
- Content validation on import: files renamed to `.html` that are not real HTML are rejected, in both the picker and the backend.
- Linux `.deb` and `.AppImage` bundles; the desktop entry is installed as `herbarium.desktop` so the icon shows on Wayland (KDE Plasma).
- Themed, draggable title bar (minimize / maximize / close) replacing the default GTK header bar, following the light and dark themes.
- One-line Linux installer (`install.sh`) with `.deb` and AppImage support, `--version` and `--uninstall`.
- MIT license.
- README, CI workflow, and a tag-driven release workflow whose GitHub release description is taken from this changelog.

### Changed
- Remove duplicate theme and language controls from the sidebar; preferences remain in Settings, while the footer keeps Settings and Search.

### Fixed
- Preserve newer inspector edits when an in-flight metadata save completes.
- Close the command palette before activating commands, exit the reader for navigation commands, and move focus to the destination.
- Keep Tab, Shift+Tab, and Escape owned by the topmost dialog, with inactive content inert.
- Improve light and dark theme contrast for muted text, placeholders, primary actions, filter chips, badges, and delete confirmation.
- Apply shared text-input styling to onboarding, library search, and inspector fields.
- Show localized review-loading errors with a retry action instead of an incorrect caught-up state.
- Expose sidebar navigation and filter selection to assistive technology.
- Keep delete confirmation available until explicitly confirmed or cancelled, without a time limit.
- Use existing radius tokens for the brand mark and keyboard shortcuts.

[Unreleased]: https://github.com/abdoufermat5/herbarium/commits/
