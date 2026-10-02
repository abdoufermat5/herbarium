# Herbarium

A desktop reader for the HTML pages that AI tools generate. Import a page, find it again later, open it with its interactivity intact, and mark it for review so it comes back when you need it.

Your pages stay plain `.html` files in a folder you own, in the spirit of an vault.

## Features

- **Import** by drag and drop, file picker, or pasting HTML. Pages are copied into the vault unmodified.
- **Search** across titles, tags, folders and page text, backed by a SQLite full-text index.
- **Organize** with folders, tags and a personal note per page. Titles are filled in from `<title>` (falling back to the first `<h1>`).
- **Read** each page in a sandboxed viewer with scripts running.
- **Review** pages at fixed intervals (1, 3, 7 or 30 days) and work through a "Review today" queue.
- **Command palette** (`Ctrl/⌘ K`) and keyboard shortcuts (`/` to search, `i` to import, `Esc` to go back).
- **Light and dark themes**, and an **English / French** interface (English by default).

## How it works

### The vault

A vault is an ordinary folder:

```
my-vault/
├── {id}.html            # the page, exactly as imported
├── {id}.json            # sidecar metadata: title, tags, folder, note, review dates
└── rust/                # folders are real directories
    ├── {id}.html
    └── {id}.json
```

Metadata lives in a small JSON file next to each page. The SQLite index only speeds up search and can be rebuilt from the files at any time, so it is never the source of truth. You can back up, sync or open the folder with any other tool.

### Page isolation

AI-generated HTML runs code, so each page is treated as untrusted:

- It is rendered in an `<iframe sandbox="allow-scripts">` without `allow-same-origin`, so it cannot reach your files or the app.
- It is served over a custom `herbarium://` scheme with a Content-Security-Policy injected by the backend.
- **Network access is a per-page switch.** By default a page may load scripts, styles and fonts from a short list of known CDNs (cdnjs, jsDelivr, unpkg, jQuery, Google Fonts). Turn it off and the page can load nothing from the network.

Pages that depend on Claude-specific APIs (`window.storage`, `window.claude`, …) won't work outside Claude. Herbarium detects this and shows a notice.

## Tech stack

| Layer    | Choice                                                  |
| -------- | ------------------------------------------------------- |
| Shell    | [Tauri 2](https://tauri.app) (Rust)                     |
| Backend  | `rusqlite` (bundled SQLite), `scraper` for HTML parsing |
| Frontend | Svelte 5, TypeScript, Vite                              |
| Styling  | Hand-written CSS with design tokens, no UI framework    |

## Getting started

### Prerequisites

- [Node.js](https://nodejs.org) and [pnpm](https://pnpm.io)
- [Rust](https://rustup.rs) (stable)
- Tauri's system dependencies for your platform. See the [Tauri prerequisites](https://tauri.app/start/prerequisites/) (on Linux this includes WebKitGTK).

### Run

```bash
pnpm install
pnpm tauri dev      # desktop app with hot reload
```

On first launch, create a new vault or point Herbarium at an existing folder. [`examples/semver.html`](examples/semver.html) is a sample page to try importing.

### Build

```bash
pnpm tauri build    # produces .deb and .AppImage bundles
```

### Checks

```bash
pnpm check          # svelte-check + TypeScript
```

## Project layout

```
src/                    Svelte frontend
├── components/         Sidebar, page list, reader, review queue, import dialog, palette…
└── lib/
    ├── state.svelte.ts   app state and data loading
    ├── api.ts            typed wrappers around Tauri commands
    ├── i18n.svelte.ts    translation runtime
    └── locales/          en.ts (source), fr.ts
src-tauri/src/          Rust backend
├── commands.rs         Tauri commands exposed to the UI
├── vault.rs            files on disk, sidecar JSON, re-indexing
├── store.rs            SQLite index and search
├── protocol.rs         herbarium:// scheme and CSP
├── content.rs          title and text extraction
└── models.rs           data models, mirrored in src/lib/types.ts
```

## Adding a language

1. Copy `src/lib/locales/en.ts` to `<code>.ts` and translate the values. Plural keys use `_one` / `_other` suffixes.
2. Register it in `LOCALES` and `CATALOGS` in `src/lib/i18n.svelte.ts` and extend the `Locale` type.

TypeScript fails if a locale is missing a key that English defines.

## Scope

Version 1 deliberately does one thing: save generated pages and bring them back for review. Not included: sync, mobile, Markdown notes, page linking, plugins, AI features. Desktop only (Linux bundles for now).

## License

No license has been chosen yet.
