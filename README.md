# Herbarium

A desktop reader for the HTML pages that AI tools generate. Import a page, find it again later, open it with its interactivity intact, and mark it for review so it comes back when you need it.

Your pages stay plain `.html` files in a folder you own, in the spirit of an vault.

## Features

- **Import** by drag and drop, file picker, or pasting HTML. Pages are copied into the vault unmodified.
- **Search** across titles, tags, folders and page text, backed by a SQLite full-text index.
- **Organize** with folders, tags and a personal note per page. Titles are filled in from `<title>` (falling back to the first `<h1>`).
- **Read** each page in a sandboxed viewer with scripts running.
- **Review** pages on a schedule you configure: preset intervals in any mix of minutes, hours and days (1, 3, 7, 30 days by default), a "Done" button that picks the next date (step up the presets, multiply the interval, or repeat it), optional automatic scheduling of imports, and a cap on the "Review today" queue. Settings live in the vault (`.herbarium/review.json`).
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

## Install

Linux (x86_64):

```bash
curl -fsSL https://raw.githubusercontent.com/abdoufermat5/herbarium/main/install.sh | sh
```

On Debian/Ubuntu this installs the `.deb` (needs `sudo`); elsewhere it installs the AppImage under `~/.local`. Options go after `sh -s --`, e.g. `... | sh -s -- --version 0.1.0`, `--appimage` to force the AppImage, or `--uninstall`. Prefer to inspect first? Download `install.sh` or grab the `.deb`/`.AppImage` from the [releases page](https://github.com/abdoufermat5/herbarium/releases/latest).

## Use with AI agents (MCP)

The `herbarium` binary also runs as a [Model Context Protocol](https://modelcontextprotocol.io) server over stdio, so agents can save, search, organize and schedule pages in your vault. For example, in Claude Code:

```bash
claude mcp add herbarium -- herbarium mcp
```

Then ask things like *"write a short HTML explainer of Cargo workspaces and save it to Herbarium under `rust`"*. The server uses `--vault <path>` if given, otherwise `HERBARIUM_VAULT`, otherwise the vault last opened in the app. The app picks up agent changes when its window regains focus.

Tools: `pages_create`, `pages_get`, `pages_list`, `pages_search`, `pages_update`, `pages_set_html`, `pages_delete`, `review_schedule`, `review_complete`, `review_clear`, `review_due`, `review_settings`, `network_set`, `tags_list`, `folders_list`, `folders_create`, `vault_info`, `vault_rescan`. The server also sends agents instructions for writing pages that work in Herbarium (self-contained HTML, allowed CDNs, sandbox limits).

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
pnpm check                                         # svelte-check + TypeScript
cargo test --manifest-path src-tauri/Cargo.toml --workspace
make ci                                            # everything CI runs; `make help` lists targets
```

### Releasing

Herbarium follows [Semantic Versioning](https://semver.org) (`0.y.z` until the first stable release: bump minor for features, patch for fixes).

```bash
pnpm release patch   # or minor | major | 1.2.3 | 1.0.0-beta.1
git push --follow-tags
```

Document changes under `[Unreleased]` in `CHANGELOG.md` first. The script bumps `package.json` and `src-tauri/Cargo.toml`, moves `[Unreleased]` into the new version, then commits and tags `vX.Y.Z`. Pushing the tag triggers `.github/workflows/release.yml`, which builds the `.deb` and `.AppImage` and publishes a GitHub release whose description is the matching `CHANGELOG.md` section (versions with a `-suffix` are marked as pre-releases). Pull requests and `main` run `.github/workflows/ci.yml`.

## Architecture

Every feature is an **extension** that registers **operations** (`area.verb`, JSON in / JSON out, with a JSON Schema) and may subscribe to **events** (`page.created`, `page.updated`, `page.deleted`, `vault.indexed`). Front ends never call features directly: the desktop UI goes through one `invoke_op` command and the MCP server turns each agent-visible operation into a tool. A new operation therefore appears in both without extra wiring. The built-in features (pages, library, review, network) use the same API a plugin would. That JSON-only boundary is meant to be implemented later by a sandboxed WASM plugin runtime.

Extensions keep per-page data under `ext.<extension-id>` in the sidecar JSON. The core preserves it through every edit and re-index. Sidecars carry a `schemaVersion`.

```rust
struct Stars;
impl Extension for Stars {
    fn id(&self) -> &str { "stars" }
    fn register(&self, r: &mut Registry) -> OpResult<()> {
        r.add(Operation::new("stars.add", "Star a page.", schema, |ctx: &mut Ctx, a: IdArgs| {
            let mut meta = ctx.page(&a.id)?;
            meta.ext.insert("stars".into(), json!({ "starred": true }));
            ctx.save(&meta)?;
            Ok(meta)
        }))?;
        r.on(events::PAGE_CREATED, |_store, event| { /* react */ });
        Ok(())
    }
}
// Host::with_extensions(vec![Box::new(Stars)])
```

## Project layout

```
src/                         Svelte frontend
├── components/              Sidebar, page list, reader, review queue, import dialog, palette…
└── lib/
    ├── state.svelte.ts      app state, data loading, resync on focus
    ├── api.ts               typed wrappers around operations (invoke_op)
    ├── i18n.svelte.ts       translation runtime
    └── locales/             en.ts (source), fr.ts
src-tauri/                   Cargo workspace
├── src/                     desktop shell: window, herbarium:// scheme + CSP, `herbarium mcp`
└── crates/
    ├── herbarium-core/      vault kernel, extension API, built-in extensions
    │   └── src/
    │       ├── extension.rs Operation, Registry, Extension, Ctx, events
    │       ├── host.rs      Host: extensions + open vault, single call entry point
    │       ├── builtin/     pages, library, review, network
    │       ├── vault.rs     files on disk, sidecar JSON, re-indexing
    │       ├── store.rs     SQLite index and search
    │       ├── content.rs   title and text extraction
    │       └── models.rs    data models, mirrored in src/lib/types.ts
    └── herbarium-mcp/       MCP stdio server over the operation registry
```

## Adding a language

1. Copy `src/lib/locales/en.ts` to `<code>.ts` and translate the values. Plural keys use `_one` / `_other` suffixes.
2. Register it in `LOCALES` and `CATALOGS` in `src/lib/i18n.svelte.ts` and extend the `Locale` type.

TypeScript fails if a locale is missing a key that English defines.

## Scope

Version 1 deliberately does one thing: save generated pages and bring them back for review. Not included yet: sync, mobile, Markdown notes, page linking, a plugin loader (the extension API is in place; a WASM runtime will come later). Desktop only (Linux bundles for now).

## License

Released under the [MIT License](LICENSE).
