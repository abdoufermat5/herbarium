# Changelog

All notable changes to Herbarium are documented here.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Ask AI: one button in the title bar (or `Ctrl/⌘ J`, or "Ask AI…" in the command palette) opens a single panel for everything AI, instead of a button per feature. It offers what fits what is on screen — the page you are reading first (simplify, explain deeper, add a quiz, cheat sheet, fix & modernize, translate with one-click common languages), then your library (organize, import an AI chat export). Type what you want and press Enter to remix the page with your own words, or pick an action and your words go along as a note. The panel shows which AI service and model it uses, and when none is set up, one button leads to the setup.
- Organize with AI (in the Ask AI panel, or the command palette): the AI reads the titles and the start of your pages and proposes where they belong — existing folders first, new ones where none fit, with a few tags. You choose the pages to look at (the new and unsorted ones, or the whole library), can say what you have in mind, review the plan grouped by folder and untick anything, and apply it; Undo puts everything back. Works with every AI service Remix works with.

### Fixed
- The diff in the remix and agent proposals and in History scrolls when it is long; before, it showed a fixed screen and clipped the rest.

### Changed
- The reader's toolbar keeps its labels on smaller windows: page history, network access and resetting page data moved to a "More" menu (⋯), and a page allowed to load from CDNs says so next to its tags.
- The app's folder is `desktop/` instead of `src-tauri/` (the Rust code, `tauri.conf.json`, icons and packaging files moved with it); the Tauri command line finds it by itself, and scripts, CI and docs follow.

## [0.3.1] - 2026-10-09

### Changed
- AI & sharing starts from three plain choices — your Claude subscription (Claude Code), free on this computer (Ollama), or an API key — and each shows whether it works right now and, when it does not, numbered steps with commands to copy, a button to the service's key page and "Check again". Choosing where Claude Code is by hand is possible, and the Remix dialog says when it is missing.
- Adding the browser extension is a guided checklist: one click prepares everything (helper registered with every browser, extension copied to a stable folder), a button opens the browser on its extensions page, the folder to load has a Copy button, and the screen says "connected" by itself once the extension reaches the app.

### Fixed
- Claude Code is found when Herbarium is started from the desktop menu: on Linux Mint and wherever node comes from nvm, `~/.local/bin` or npm's own folder, `claude` was reported as not installed because the app does not get the shell's PATH. Herbarium now asks the login shell, looks in the usual install folders, and runs Claude Code with that PATH.
- A Claude Code that is not signed in is explained ("run claude in a terminal and sign in") instead of showing its raw output.

## [0.3.0] - 2026-10-08

### Added
- Page sources: a page can record where it came from — the address it was saved from, the tool that generated it and the prompt that asked for it. `pages.create` and `pages.update` take `source`, the tool and prompt are searchable, the details panel shows them, `herbarium add` records a fetched URL and takes `--tool` and `--prompt`, and agents are asked to fill it in.
- MCP resources and prompts: every page is a resource (`herbarium://page/<id>` for its HTML, `herbarium://page/<id>/text` for its text), and the `save_page`, `ask_vault` and `review_session` prompts package common requests.
- Review agent edits: with Settings → Agents → "Review agent edits" on, an agent's `pages.set_html` or `history.restore` leaves a proposal instead of changing the page. The reader shows a banner and a side-by-side diff to accept or reject it, new proposals are announced, and accepting one for a page that changed since asks first. Agents get `agents.settings`, `proposals.list` and `proposals.get`; only the user can accept, reject or change the setting.
- Quiz mode: answers a page marks with `data-herbarium-recall` (optionally holding the question) are hidden behind a "show answer" button during review sessions, or on any page with "Quiz me" in the reader.
- Search filters: `tag:`, `folder:`, `tool:`, `is:due`/`scheduled`/`unscheduled`/`network`, `has:note`/`source` and `due:`/`updated:`/`created:` windows such as `7d`, each negatable with `-`, mixed into any search. Searches can be saved, are listed in the sidebar and are available to agents (`searches.*`).
- Links between pages: a link to `herbarium-app://open/<id>` inside a page opens that page in the app. The details panel lists a page's links, broken links and backlinks and copies its link; agents get `pages.links`.
- Review activity: a heatmap of the last year's reviews with the current and longest streak, and Settings → Review can skip folders or tags, whose pages keep their schedule but stay out of the queue, due counts and reminders.
- Reader View menu: the page's contents (including headings built by script), zoom from 50 to 200 % (`Ctrl/⌘ +`, `-`, `0`) and a focus mode (`F`) that hides the sidebar, details and review bar.
- Reading paths: ordered lists of pages read like a course, with previous/next in the reader, managed from the sidebar and the details panel, and open to agents (`paths.*`). A trashed page keeps its place in a path.
- Exports: save a page as one self-contained HTML file with its local images, scripts and styles inlined, or publish the vault or a folder as a static website (an index plus one file per page, links between them kept) into a new or empty folder.
- Import from Claude and ChatGPT: point Herbarium at a data export (`.zip` or `conversations.json`) and pick the artifacts to keep — Claude artifacts (every revision resolved), ChatGPT canvases and HTML code blocks — each saved with its conversation as the source and its original date, and never imported twice. `herbarium import <export>` does the same from the terminal (`--dry-run` lists them).
- Browser extension (Chrome, Edge, Brave, Firefox): finds the HTML artifacts in the Claude, ChatGPT or Gemini conversation you are reading and saves them in one click, saves any page or selection with its images and styles inlined, shows on the toolbar when a page is already in the vault, searches the vault from the address bar (`h` + space) and offers a Today new-tab page. It talks to the app through a native messaging host that Settings → Browser registers (`herbarium native-host install` from the terminal).
- Quick capture: a global shortcut (default `Ctrl/⌘+Alt+H`) and the tray save the clipboard as a page; new HTML files in Downloads, and optionally whole pages copied to the clipboard, are offered for saving.
- Sample pages: a short tour (welcome, why reviewing works, a flexbox playground) as a reading path, offered on first launch and from Settings → Vault.
- Today: a home screen with what is due, the reading path in progress, recent pages, a page to rediscover, what you saved on this day, and the review streak. It is the default start view.
- Icons and colours: give a page an emoji icon and a folder or tag a colour and icon; they show in the sidebar, lists and chips.
- Page previews: the library grid shows a miniature of each page's first screen, measured in the page and redrawn by the app.
- Graph: the links between pages as an interactive map. Each folder settles into its own named island; zoomed out on a busy vault, the links between folders become one ribbon per pair. Hover lights a page's neighbourhood, a click lists its links both ways, a double click opens it; pages can be dragged, the map zoomed and searched, and a folder shown on its own. Drawn on a canvas, it stays smooth with thousands of pages.
- Highlights and margin notes: select text in a page to highlight it in one of four colours and attach a note. Highlights survive edits elsewhere in the page, are searchable, are listed in the details panel and during review, and agents can read them (`highlights.list`).
- Page health: `health.check` and the details panel say why a page may not work — missing local files, remote resources with network access off, hosts outside the CDN allowlist, broken links, very large or untitled pages — and a page can be made self-contained by inlining its local files. Settings → Vault checks every page.
- Remix with AI: rework a page — simplify it, explain it deeper, add quiz questions, translate it, turn it into a cheat sheet, fix and modernize it, or follow your own instructions. Your highlights and notes go with it. The result waits as a proposal you compare with the page and accept or reject. Works with Anthropic's Claude (your API key, streamed, with server-side fallbacks enabled so a declined request is retried on Anthropic's recommended fallback model), the Claude Code command line, OpenAI, Google Gemini, DeepSeek, Mistral, OpenRouter, a local Ollama, or any OpenAI-compatible service at an address you give; each keeps its own key. Nobody has to type a model name: Settings → AI & sharing is pick a service, paste a key (checked on the spot: "Ready — connected, 23 models available"), done. The model stays on Automatic, which asks the service for its models and uses its best current one (the newest Opus, GPT or Gemini Pro, DeepSeek's chat model…); a dropdown with readable names lists the others, and any name can still be typed. `herbarium remix` does the same from the terminal (`--provider`, `--model`, `--base-url`).
- Publish and share: share a page as a secret GitHub gist or publish it on your GitHub Pages site (the repository is created on first use and keeps an index of what you published), update it in place or take it down, and copy a share card — the page's first screen, title, a line of text and its link — to paste into a note, chat or email. Settings → AI & sharing connects your GitHub account; `herbarium publish` works from the terminal.
- End-to-end tests (`pnpm test:e2e`) run the command line, the native messaging host, the browser extension in Chromium, publishing and remixing against fake GitHub and Anthropic APIs, and the desktop app itself through tauri-driver, in CI.

### Changed
- Settings is split into tabs — General, Review, Vault, Capture, AI & sharing, About — instead of one long page.
- A lighter interface: a borderless reader toolbar that shows icons only in narrow windows, one preview area on every library card, sentence-case chips, one heading size across views, a single review statistics strip, and a narrower sidebar in small windows.
- Large libraries stay fast: the library draws the cards near the viewport only, so showing all pages, searching, filtering and switching layouts take milliseconds with thousands of pages (measured on 3,000: All pages 4.2 s → 80 ms, grid/list 6.6 s → 5 ms). Typing a search fetches only the search, and page previews are made only while you are idle, never for pages over 1.5 MB.
- Pasting a very large page (over 100 KB) into the import dialog no longer freezes the window: it is held aside and shown as a summary.
- A much smaller download: the release binary is optimised for size (34 MB down to 14 MB on Linux), the source editor loads on first use and only the Latin font subsets ship.

### Fixed
- The library sort setting showed no value until changed.
- Pages in the reader follow the app's theme: with the app set to dark on a light desktop, they stayed light. System remains the default.
- Settings changed at the same moment (or while GitHub checked a token) no longer undo each other.
- Escape closes only the dialog it is pressed in, never the reader or Settings behind it, and single-key shortcuts do nothing behind an open dialog.

### Security
- API keys and the GitHub token are kept apart from the settings in `secrets.json` in the app's configuration folder, readable only by the user, and are never sent to the window: the interface only learns whether one is set.
- Publishing never writes to a repository Herbarium did not create (it looks for its `.herbarium` marker), and asks before the first publication of each page, saying who will be able to read it. Unpublishing only deletes a page file from such a repository, whatever the vault's records say.
- `secrets.json` is created readable only by the user (never world-readable, even for a moment), and a damaged one is reported instead of being replaced, which would erase the other keys.
- Remixing with Claude Code runs it without tools, in an empty folder, so instructions hidden in a page cannot make it read local files.
- Pages saved from the browser, including the artifacts of a conversation, never get network access, whatever the vault's default.

## [0.2.1] - 2026-10-04

### Added
- Snap Store channel: a strict `herbarium` snap (amd64) built from the release `.deb` and published by the release workflow when `SNAPCRAFT_STORE_CREDENTIALS` is set; `gh workflow run snap.yml -f tag=vX.Y.Z` builds or publishes an existing release.

### Changed
- Installed from the Snap Store, Herbarium leaves updates to the store: the in-app updater never checks or installs, and Settings says so.

### Removed
- Homebrew and AUR publishing jobs and their templates.

### Fixed
- Intervals set by adaptive review show in whole days ("2d", "2 days") on the grade buttons, in toasts and in review history, instead of raw minutes such as "3321m".
- The "unchanged lines" bar in the page-history diff follows the theme instead of staying white in dark mode.

## [0.2.0] - 2026-10-04

### Added
- Pages keep their state: `localStorage` and the `window.storage` API used by Claude artifacts persist per page in its sidecar (up to 1 MiB), so quizzes, trackers and checklists remember where you left off. `sessionStorage` lasts for the visit. The draft preview never saves state, and the reader can reset a page's data.
- Page history: every in-app or agent overwrite of a page's HTML first keeps the previous version in `.herbarium/history/` (newest 50 per page). The reader's History panel shows each version side by side with the current one and restores it. Agents get `history.list`, `history.get` and `history.restore`.
- `herbarium add <file|-|url>...` imports pages from the terminal into the last-opened vault (or `--vault`), with `--folder`, `--tag`, `--title` and `--network`/`--no-network`, and prints each page's link.
- `herbarium-app://open/<id>` and `herbarium-app://review` links open the running app on a page or on Review; a second launch hands its link to the open window instead of starting another instance.
- Adaptive review (FSRS-6): grade a review Again, Hard, Good or Easy (`1`–`4`), each button showing the interval it would set, and choose the retention you aim for (70–97 %). Agents get `review.preview`.
- Release workflow: optional Windows code signing, and Homebrew, WinGet and AUR publishing, each enabled by its secret; a Flatpak manifest and AppStream metadata for Flathub. See `docs/distribution.md`.

### Changed
- Adaptive scheduling is the default review strategy; the preset ladder, repeat and multiply strategies remain in Settings, where Hard repeats the interval and Easy skips ahead.
- Pages that use `window.storage` are no longer flagged as needing Claude; only `window.claude` is.

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

[Unreleased]: https://github.com/abdoufermat5/herbarium/compare/v0.3.1...HEAD
[0.3.1]: https://github.com/abdoufermat5/herbarium/compare/v0.3.0...v0.3.1
[0.3.0]: https://github.com/abdoufermat5/herbarium/compare/v0.2.1...v0.3.0
[0.2.1]: https://github.com/abdoufermat5/herbarium/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/abdoufermat5/herbarium/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/abdoufermat5/herbarium/releases/tag/v0.1.0
