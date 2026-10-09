# Distribution

How Herbarium reaches each channel, which repository secret turns it on, and how
to set that channel up for the first time.

Every channel is optional and independent. A tag push with none of the secrets
below set behaves exactly like the first release: it builds every platform,
publishes the GitHub release and `SHA256SUMS`, and skips the rest — each skipped
job leaves a `::notice::` in the run summary saying which secret is missing.
Nothing here requires a secret to exist, and no secret is ever echoed to the
log.

## Channels at a glance

| Channel | Job | Turns on with | Publishes |
|---|---|---|---|
| GitHub release | `build`, `publish` | `GITHUB_TOKEN` (built in) | Bundles + `SHA256SUMS` + `latest.json` when updater keys are set |
| In-app updater | `gate` | `TAURI_SIGNING_PRIVATE_KEY` secret **and** `HERBARIUM_UPDATER_PUBLIC_KEY` variable | Signed updater artifacts and `latest.json` |
| macOS signing/notarization | `build` | `APPLE_CERTIFICATE` (+ password, identity, notarization secrets) | Signed and notarized `.dmg`/`.app` |
| Windows signing | `build` | `WINDOWS_CERTIFICATE` + `WINDOWS_CERTIFICATE_PASSWORD` | Authenticode-signed `.msi`/`-setup.exe` |
| WinGet | `winget` | `WINGET_TOKEN` | An update PR to `microsoft/winget-pkgs` |
| Snap Store | `snap` | `SNAPCRAFT_STORE_CREDENTIALS` | An amd64 snap on the Snap Store (`stable`, or `candidate` for prereleases) |
| Flatpak / Flathub | — (manual) | none | A PR to Flathub from the manifest in the repo |

`winget` runs only for stable releases: a tag with a pre-release suffix
(`v0.2.0-rc.1`) skips it. Snap runs for every release and sends prereleases to
the `candidate` channel instead of `stable`.

## Windows Authenticode signing

Tauri signs with a certificate in the runner's `Cert:\CurrentUser\My` store; the
bundler is configured with `bundle.windows.certificateThumbprint`,
`digestAlgorithm: sha256` and DigiCert's timestamp service. The workflow base64
decodes the certificate, imports it for the current user (no administrator rights),
and merges the thumbprint into the same `--config` file the updater step wrote, so
updater keys are preserved.

One-time setup:

1. Buy a Windows code-signing certificate from a CA. An OV certificate is enough
   for Authenticode signing and a timestamp; note that SmartScreen reputation
   still builds up over the first downloads.
2. Export the certificate **with its private key** as a `.pfx`.
3. Base64-encode it and set:
   - `WINDOWS_CERTIFICATE` — the base64 PFX (`base64 -w0 herbarium-signing.pfx`).
   - `WINDOWS_CERTIFICATE_PASSWORD` — the PFX password.

Missing either secret: the step logs `Windows code signing skipped` and the
Windows bundles build unsigned, exactly as before. Windows will show a SmartScreen
warning on first launch.

## WinGet

The `winget` job runs `vedantmgoyal9/winget-releaser`, which opens a PR against
`microsoft/winget-pkgs` for the identifier `abdoufermat5.Herbarium`. The
`installers-regex` matches the Windows assets the bundler produces
(`*-setup.exe` and `*.msi`).

One-time setup:

1. Submit the **first** version from Linux with
   [`komac`](https://github.com/russellbanks/Komac). winget-releaser only
   *updates* a package that already exists, and fails early if it does not:

   ```bash
   komac new abdoufermat5.Herbarium --version 0.2.0 \
     --urls <herbarium_0.2.0_x64_en-US.msi-url> <herbarium_0.2.0_x64-setup.exe-url>
   ```

   `komac` forks `microsoft/winget-pkgs` under the `abdoufermat5` account on
   its own and opens the PR from that fork — there is no need to create the
   fork by hand.
2. Create a classic PAT with the **`public_repo`** scope and store it as
   `WINGET_TOKEN`; winget-releaser pushes updates to the existing fork.

Missing `WINGET_TOKEN`: the job is skipped; the version simply is not submitted
to WinGet.

## Snap Store

The `snap` job calls the reusable `.github/workflows/snap.yml`, which downloads
the amd64 `.deb` from the release for the tag being built, renders the
`__VERSION__` token in `desktop/packaging/snap/snapcraft.yaml` (a `dump` part
whose source is that `.deb`), builds the snap with `snapcore/action-build@v1`,
and keeps it as a workflow artifact. When `SNAPCRAFT_STORE_CREDENTIALS` is set
it then publishes with `snapcore/action-publish@v1` — plain versions to
`stable`, a tag with a `-` in the version to `candidate`. Without the secret the
build still runs, and a `::notice::` says publishing was skipped.

Only amd64 is built: the arm64 `.deb` is produced on ubuntu-24.04 and needs
glibc 2.39, while the `core22` snap base ships glibc 2.35, so the arm64 `.deb`
cannot run inside the snap. An arm64 snap has to wait for an arm64 `.deb` built
on a 22.04 runner.

One-time setup:

1. Create an account on [snapcraft.io](https://snapcraft.io) and register the
   name. Snap names are global, so this also reserves `herbarium`:

   ```bash
   sudo snap install snapcraft --classic   # if snapcraft is not installed yet
   snapcraft login
   snapcraft register herbarium
   ```

2. Export a login scoped to this snap and store it as the repository secret
   `SNAPCRAFT_STORE_CREDENTIALS`:

   ```bash
   snapcraft export-login --snaps=herbarium \
     --acls package_access,package_push,package_update,package_release - \
     | gh secret set SNAPCRAFT_STORE_CREDENTIALS
   ```

3. To publish an already-released version (v0.2.0 shipped before the snap
   channel existed), dispatch the workflow with its tag:

   ```bash
   gh workflow run snap.yml -f tag=v0.2.0
   ```

   This builds from the existing release assets and publishes them; the snap is
   also kept as a workflow artifact either way.

Users install with `sudo snap install herbarium`.

Confinement notes:

- Strict confinement remaps `$HOME`: the app's config (and the "vault last
  opened" that the MCP server reads) lives under `~/snap/herbarium/`. Vaults the
  user picks must be under a non-hidden directory of the real home (`home`
  plug) or on removable media (`removable-media` plug); a path inside a hidden
  directory such as `~/.local/share/...` is not reachable.
- The in-app updater is disabled. Herbarium detects `SNAP`/`SNAP_NAME` and
  reports that updates are installed by the Snap Store; update with
  `sudo snap refresh herbarium`.
- The desktop entry keeps `MimeType=x-scheme-handler/herbarium-app;`. snapd
  preserves `MimeType` when it rewrites `Exec`, so `herbarium-app://` links open
  the snap.
- No single instance: owning the `io.herbarium.desktop.SingleInstance` D-Bus
  name needs a `dbus` slot, which the store sends to manual review on every
  upload. The snap ships without it, so a second launch or a `herbarium-app://`
  link opens a new window rather than reusing the running one. Restoring it
  means asking the store for a snap declaration for that name on
  forum.snapcraft.io (category `store-requests`), then adding the slot back.
- The CLI works unchanged: `/snap/bin/herbarium mcp` and
  `/snap/bin/herbarium add <file>` dispatch before the GUI starts.

## Flatpak (Flathub)

Flatpak is the one channel with no automation secret: Flathub builds from a
manifest in its own repository, so each release is a pull request.

The manifest lives at
`desktop/packaging/flatpak/io.github.abdoufermat5.Herbarium.yml`, with the
AppStream metadata in `io.github.abdoufermat5.Herbarium.metainfo.xml`. It targets
`org.gnome.Platform`/`org.gnome.Sdk` 47 (which ships webkit2gtk-4.1) and installs
the release `.deb` for the architecture being built, the desktop entry from the
tagged source (renamed to the application id so the `herbarium-app://` scheme
handler is registered), and the Metainfo file.

Per release, in the manifest:

1. Point the two `.deb` URLs and the desktop-entry URL at the new tag, and
   update all three `sha256` placeholders:

   ```bash
   base=https://github.com/abdoufermat5/herbarium/releases/download/v0.2.0
   curl -fsSL "$base/herbarium_0.2.0_amd64.deb" | sha256sum
   curl -fsSL "$base/herbarium_0.2.0_arm64.deb" | sha256sum

   curl -fsSL \
     https://raw.githubusercontent.com/abdoufermat5/herbarium/v0.2.0/src-tauri/packaging/herbarium.desktop \
     | sha256sum
   ```

2. Bump the version in the manifest's URLs and commit the change, then open a PR
   against the Flathub app repository (`flathub/io.github.abdoufermat5.Herbarium`).

The first submission is a PR to `flathub/flathub` asking for a new application;
after that, releases are updates to the app repository. Copy both the manifest
and `io.github.abdoufermat5.Herbarium.metainfo.xml` into that repository — the
manifest references the metainfo with a local `path:` source. Test locally with
`flatpak-builder --force-clean --user --install build-dir <manifest>`.

Missing nothing: Flatpak publishing does not depend on a repository secret.

## Cutting a release

The main README describes the normal flow:

```bash
pnpm release patch   # or minor | major | x.y.z
git push --follow-tags
```

The tag runs the gate checks, builds every platform, publishes the GitHub
release, and then runs whichever optional channels are configured. The
`desktop/packaging/` manifests carry `__PLACEHOLDER__` tokens and are **not**
meant to be edited by hand: `snap/snapcraft.yaml`'s `__VERSION__` is rendered by
`.github/workflows/snap.yml`, and the Flatpak manifest's version-pinned URLs and
checksums are updated per release as described above.
