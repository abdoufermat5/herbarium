# Changelog

All notable changes to Herbarium are documented here.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Tauri 2 + Svelte 5 desktop app for keeping generated HTML pages in a plain-file vault and reviewing them at the right time.
- Minimal editorial design system: warm monochrome palette with pastel status colors, bundled Geist / Geist Mono / Newsreader fonts, Phosphor icons, hairline borders, and quiet scroll-entry motion. Includes a command palette, toasts, an inspector panel and a review flow.
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
- MCP server (`herbarium mcp`) so AI agents can create, read, search, edit, rewrite, delete and schedule review of pages in the vault, with built-in guidance for writing Herbarium-compatible HTML.
- Extension API: features register named JSON operations and subscribe to vault events; the desktop UI and the MCP server both run through the same operation registry. Built-in features are implemented as extensions.
- Sidecar metadata gains `schemaVersion` and an `ext` namespace for extension data, preserved across edits and re-indexing.
- The app re-syncs with the vault folder when its window regains focus, picking up pages added by agents or other tools.

- Create folders and pages from the sidebar (header buttons, or the hover buttons on a folder), with an inline name field. New pages start from a blank HTML skeleton and open straight away. Empty folders now show in the tree, and the MCP server gains `folders_create`.

- HTML editor in the reader: an Edit button opens the page source beside a live preview. Save with the button or `Ctrl/⌘+S`; invalid HTML is rejected with the reason and your draft is kept.

### Changed
- Remove duplicate theme and language controls from the sidebar; preferences remain in Settings, while the footer keeps Settings and Search.
- Backend split into a Cargo workspace: `herbarium-core` (vault, index, extensions), `herbarium-mcp`, and the desktop shell.
- Folder paths that escape the vault are now rejected instead of silently falling back to the vault root.

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
