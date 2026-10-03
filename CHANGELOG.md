# Changelog

All notable changes to Herbarium are documented here.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-10-03

### Added
- First release: a desktop reader for the HTML pages AI tools generate. Pages stay plain `.html` files plus a JSON sidecar in a folder you own; a SQLite index speeds up search and is rebuilt from the files at any time.
- Import by drag and drop, file picker, or pasting HTML. A dropped folder tree stages every HTML file with its destination folder, files that are not real HTML are rejected with the reason, and an import can override the vault's CDN default for that batch.
- Library with a searchable page list: grid or list layout, sorting by relevance, recency, title or next review, bookmarks-free folder and tag chips, and per-page checkboxes for bulk move, tagging, rescheduling and trashing with per-page errors.
- Full-text search over titles, tags, folders, notes and page text, ranked with BM25 (title strongest) and returned with the matched excerpt highlighted as text — never as raw HTML.
- Sidebar file tree with inline creation, keyboard navigation (one tab stop, arrows, `Home`/`End`, `Enter`, `F2`, `Delete`), drag a page onto a folder to move it, and folder filtering that includes subfolders.
- Trash: deleting a page moves it to `.herbarium/trash/<id>/` instead of removing it. Restore puts it back in its original folder (or the root when that folder is gone), purge deletes one entry, and emptying the trash asks first. Deleting in the library always shows one confirmation with an Undo action.
- HTML editor in the reader: source beside a live preview of the draft in the same sandboxed frame, CodeMirror editing with native undo and find, and `Ctrl/⌘+S` to save.
- Unsaved-work protection: navigating away, closing the window and the tray's Quit all ask first (Cancel keeps the draft); saving sends the timestamp the page was loaded with and a stale save offers Reload, Keep editing, or an explicitly confirmed Overwrite.
- External-edit awareness: pages changed on disk by another tool or an agent are picked up when the window regains focus, keeping any draft, and the reader offers the version on disk when a save conflicts.
- Review system: editable preset intervals in minutes, hours or days, a strategy for the next interval (next preset, multiply, or repeat), a longest-interval cap, automatic review of imported pages, and a queue cap. The Review view shows real totals (due, overdue, reviewed today, all-time), a 14-day forecast and each page's history, and a session walks the due queue with `1` Again, `2` Good, `S` Skip and `Esc` to leave.
- Desktop notification when a page becomes due while the app is running, with the permission state shown in Settings.
- Command palette (`Ctrl/⌘ K`) covering the whole app: create pages and folders, import, rescan, open Review, Trash or Settings, toggle the sidebar and details panel, switch language, theme or layout, and act on the open page, plus full-text page search.
- Settings for vaults (open, create, recent, remove, reveal, rescan, export a copy, close to tray), in-app updates, page isolation (network default), reader appearance and editor, review, startup screen, import destination, and a `?` shortcut reference.
- Page isolation: every page renders in an `<iframe sandbox="allow-scripts allow-popups">` on a custom `herbarium://` scheme with a backend-injected CSP, cannot navigate away or open remote documents, and its network access is a per-page switch (allowlisted CDNs, or nothing). Web links a page tries to follow are offered for confirmation instead.
- Light and dark themes that can follow the operating system, and an English / French interface that starts from the OS language.
- MCP server (`herbarium mcp`) exposing 29 operations to agents — pages (create, read, list, search, update, set HTML, duplicate, bulk update, bulk delete, delete, trash, restore), review (schedule, complete, clear, due, stats, settings), library (folders, tags), network and vault introspection — with structured results, tool annotations and an agent-specific result limit.
- Extension API: features register JSON operations and subscribe to vault events; the desktop UI and the MCP server both run through the same registry.
- In-app updates from signed GitHub releases (`tauri-plugin-updater`), with an explicit confirmation and the unsaved-work guards, refusing to check or install in builds compiled without a signing public key.
- Linux `.deb`, `.rpm` and AppImage bundles, a one-line installer (`install.sh`), GitHub Actions CI on every pull request, and a tag-driven release workflow that runs the checks, builds every platform, verifies the updater manifest and publishes `SHA256SUMS`.

### Changed
- Sidecar metadata carries `schemaVersion` plus the sidecar `ext` namespace, and review durations are minutes everywhere (`intervalMinutes`); sidecars written by earlier builds with `intervalDays` are upgraded automatically.
- Agent-visible operations are limited to the ones an agent can use safely: enabling a page's network access, importing, changing review or network settings, and purging the trash are UI-only, while listing and searching default to 50 results for agents.

### Fixed
- Pages that a rescan finds without a sidecar are treated as untrusted: their network access starts off.
- A page file moved or renamed on disk is re-indexed instead of keeping a stale path or losing the page, and a sidecar that cannot be parsed is left alone for repair rather than overwritten with blank metadata.
- Writes to page files, sidecars, review settings and the app config are atomic, so an interrupted save cannot leave a half-written file.
- Trash entry ids are validated as single path components before any path is joined, recursive scans and folder listings skip directory symlinks, and a page folder that resolves outside the vault cannot serve assets — a vault cannot be escaped through a crafted id or a symlink.
- Exporting a vault or page writes a temporary archive and replaces the chosen file only after it succeeds, so a failed export no longer truncates the previous archive.
- `updatedAt` never lags the HTML file it was written from, so saving twice with the value you were handed no longer conflicts with itself, and reading a page that another tool changed returns the timestamp the files actually have.
- `reviewedToday` stays exact beyond 100 reviews of the same page in one day; scheduling a page does not count as reviewing it, and clearing a schedule keeps the last review.
- Closing to the tray prompts for unsaved work instead of hiding the window first, and a slow search response can no longer replace newer results.

### Known limitations
- Desktop only: the release workflow builds Linux (`.deb`, `.rpm`, AppImage), macOS and Windows bundles, but there is no mobile or web build.
- macOS bundles are unsigned and unnotarized until Apple credentials (`APPLE_CERTIFICATE`, `APPLE_SIGNING_IDENTITY`, notarization secrets) are configured on the repository; macOS will warn about the first launch. Updater signatures are independent of Apple code signing.
- In-app updates require a signing key pair on the repository (`HERBARIUM_UPDATER_PUBLIC_KEY` variable and `TAURI_SIGNING_PRIVATE_KEY` secret); without them the check reports the missing key and installs must come from the releases page.
- No sync, mobile app, Markdown notes, page linking, or plugin loader yet — the extension API is in place, but the WASM runtime that would load third-party extensions is future work.

[Unreleased]: https://github.com/abdoufermat5/herbarium/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/abdoufermat5/herbarium/releases/tag/v0.1.0
