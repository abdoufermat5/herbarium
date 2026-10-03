# Herbarium

A desktop reader for the HTML pages that AI tools generate. Import a page, find it again later, open it with its interactivity intact, and mark it for review so it comes back when you need it.

Your pages stay plain `.html` files in a folder you own, in the spirit of an vault.

## Features

- **Import** by drag and drop, file picker, or pasting HTML. Pages are copied into the vault unmodified. A dropped folder tree stages every HTML file it finds, and the import dialog can override the vault's CDN default for that batch.
- **Search** across titles, tags, folders, notes and page text, backed by a SQLite full-text index. Hits keep the backend's ranking, and the matched term is highlighted in the excerpt (never rendered as HTML).
- **Organize** with folders, tags and a personal note per page. Titles are filled in from `<title>` (falling back to the first `<h1>`); renaming a page in the app keeps your title even when the file's `<title>` changes. Tag chips on each page and in the palette filter the library, and agents can rename (merging into an existing tag) or delete a tag through MCP; folders can be renamed, moved, or deleted with their pages.
- **Select** pages with checkboxes, `Ctrl/⌘`-click or `Shift`-click to move, tag, reschedule or trash them in one go, with per-page errors reported. Drag a page onto a folder (or the Files header) to move it.
- **Trash** keeps deleted pages restorable: the Trash view lists each entry with its original folder, restores it, purges it, or empties the trash behind a confirmation. Deleting shows one in-app confirmation and an Undo toast.
- **Edit** a page's HTML in the app, or open its file in an editor installed on your system (VS Code, Zed, Kate… detected automatically, or a custom command).
- **Read** each page in a sandboxed viewer with scripts running.
- **Review** pages on a schedule you configure: preset intervals in any mix of minutes, hours and days (1, 3, 7, 30 days by default), a "Done" button that picks the next date (step up the presets, multiply the interval, or repeat it), optional automatic scheduling of imports, and a cap on the "Review today" queue. A session walks the due queue with `1` Again, `2` Good, `S` Skip and `Esc` to leave, and the Review view shows real totals, the 14-day forecast and each page's review history. A desktop notification tells you when a page comes due while the app is running (switch it off in Settings → Review). Review settings live in the vault (`.herbarium/review.json`).
- **Command palette** (`Ctrl/⌘ K`) and keyboard shortcuts (`/` to search, `i` to import, `?` for the shortcut reference, `Esc` to go back) with a searchable command list for pages, folders, tags and review.
- **Settings** for vaults (open, create, recent, reveal, export a copy), updates, editor, network, startup screen, import destination, review and more.
- **Light and dark themes** following the operating system by default, and an **English / French** interface that starts from the OS language.


## How it works

### The vault

A vault is an ordinary folder:

```
my-vault/
├── {id}.html            # the page, exactly as imported
├── {id}.json            # sidecar metadata: title, tags, folder, note, review dates
├── rust/                # folders are real directories
│   ├── {id}.html
│   └── {id}.json
└── .herbarium/          # app state: the search index, review/network settings
    └── trash/{id}/      # deleted pages, restorable until you empty the trash
```

Metadata lives in a small JSON file next to each page. The SQLite index only speeds up search and can be rebuilt from the files at any time, so it is never the source of truth. You can back up, sync or open the folder with any other tool.

The file name is the page id: moving or renaming a page's `.html`/`.json` pair on disk moves or renames the page on the next rescan. If two folders hold a file with the same name, only one is indexed (a warning names the others). A sidecar that cannot be parsed is left untouched for you to repair. Folder names starting with a dot are reserved and skipped.

### Page isolation

AI-generated HTML runs code, so each page is treated as untrusted:

- It is rendered in an `<iframe sandbox="allow-scripts allow-popups">` without `allow-same-origin`, so it cannot reach your files or the app. Popups are requested, never opened.
- It is served over a custom `herbarium://` scheme with a Content-Security-Policy injected by the backend.
- It cannot navigate away: any frame navigation outside the app (a link, `location.href`, `<meta refresh>`, a `target="_blank"` link) is blocked, so a page cannot leave its CSP by loading a remote document. Web links are offered in a prompt instead and open in your browser only if you click **Open in browser**. The guard covers Linux and macOS through the webview's navigation hook and Windows through a WebView2 frame-navigation hook.
- The app window itself runs under a strict Content-Security-Policy (`script-src 'self'`, no remote connections besides the app's IPC).
- **Network access is a per-page switch.** By default a page may load scripts, styles and fonts from a short list of known CDNs (cdnjs, jsDelivr, unpkg, jQuery, Google Fonts), and images and media from any `https:` host. Turn it off and the page can load nothing from the network.

> **Know what network-on means.** Because images may come from any `https:` host, a page with network on can contact a server of its author's choosing: a tracking pixel can tell it when you open the page (and your IP address), and a script can encode what you type into the page in an image URL. Pages cannot read your files, other pages or the app, so that is the extent of it. Turn network off for pages you don't trust or that ask for input you consider private.

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

Tools: `pages_create`, `pages_get`, `pages_list`, `pages_search`, `pages_update`, `pages_set_html`, `pages_duplicate`, `pages_bulk_update`, `pages_bulk_delete`, `pages_delete`, `pages_trash`, `pages_restore`, `review_schedule`, `review_complete`, `review_clear`, `review_due`, `review_stats`, `review_settings`, `network_set`, `network_settings`, `tags_list`, `tags_rename`, `tags_delete`, `folders_list`, `folders_create`, `folders_rename`, `folders_delete`, `vault_info`, `vault_rescan`. The server also sends agents instructions for writing pages that work in Herbarium (self-contained HTML, allowed CDNs, sandbox limits).

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

### In-app updates

Settings → Updates checks GitHub for a newer signed release and installs it with `tauri-plugin-updater` after an explicit confirmation (it also runs the unsaved-work guards first). The plugin is only enabled when the minisign public key is present at compile time as `HERBARIUM_UPDATER_PUBLIC_KEY`; `src-tauri/src/updater.rs` bakes it in and refuses to check or install without it, with a message that names the missing variable — there is no unsigned or default-key path.

CI reads the public key from the repository variable `HERBARIUM_UPDATER_PUBLIC_KEY` and the private key from the secret `TAURI_SIGNING_PRIVATE_KEY` (plus `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`), and `release.yml` fails the release when only one of the two is configured. Until both are set, releases are published without updater artifacts and new versions are installed from the releases page.

Create the pair once, with `pnpm tauri signer generate`:

```bash
pnpm tauri signer generate -w ~/.tauri/herbarium-updater.key   # keep this file secret
gh variable set HERBARIUM_UPDATER_PUBLIC_KEY --body "$(cat ~/.tauri/herbarium-updater.key.pub)"
gh secret set TAURI_SIGNING_PRIVATE_KEY < ~/.tauri/herbarium-updater.key
```

The private key never belongs in the repository or in a build log; only the `.pub` content is shared.

To build signed updater artifacts locally (and reproduce what CI does), turn the setting on with a CLI config file — the bundler reads `bundle.createUpdaterArtifacts` and `plugins.updater` from the config it is passed, so `TAURI_CONFIG` alone is not enough:

```bash
PUB="$(tr -d '\n' < ~/.tauri/herbarium-updater.key.pub)"
printf '{"bundle":{"createUpdaterArtifacts":true},"plugins":{"updater":{"pubkey":"%s"}}}\n' "$PUB" > /tmp/updater.conf.json
HERBARIUM_UPDATER_PUBLIC_KEY="$PUB" TAURI_SIGNING_PRIVATE_KEY="$(tr -d '\n' < ~/.tauri/herbarium-updater.key)" \
  pnpm tauri build --bundles appimage --config /tmp/updater.conf.json
ls src-tauri/target/release/bundle/appimage/*.sig   # the signed updater artifact
```

Rotating the key means publishing a release signed with the new one, because a build only trusts the key it was compiled with.

Two consequences worth knowing: in-app updates only begin with the first release published *after* the keys are configured (that release also uploads `latest.json`, which earlier releases do not have), and a build compiled without a key cannot self-update at all — its `Check for updates` answer names the missing public key and points the user at the releases page.

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

Version 1 deliberately does one thing: save generated pages and bring them back for review. Not included yet: sync, mobile, Markdown notes, page linking, a plugin loader (the extension API is in place; a WASM runtime will come later). Desktop only: the release workflow builds Linux (`.deb`, `.rpm`, AppImage), macOS and Windows bundles. macOS bundles are unsigned and unnotarized until Apple credentials are set on the repository, so macOS warns on first launch; the in-app updater trust comes from its own signing key, not from Apple.

## License

Released under the [MIT License](LICENSE).
