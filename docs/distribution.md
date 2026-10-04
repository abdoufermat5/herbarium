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
| Homebrew | `homebrew` | `HOMEBREW_TAP_TOKEN` | `Casks/herbarium.rb` in `abdoufermat5/homebrew-tap` |
| WinGet | `winget` | `WINGET_TOKEN` | An update PR to `microsoft/winget-pkgs` |
| AUR | `aur` | `AUR_SSH_PRIVATE_KEY` | `herbarium-bin` on the AUR |
| Flatpak / Flathub | — (manual) | none | A PR to Flathub from the manifest in the repo |

`homebrew`, `winget` and `aur` run only for stable releases: a tag with a
pre-release suffix (`v0.2.0-rc.1`) skips them.

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

## Homebrew cask

The `homebrew` job downloads the two macOS `.dmg` assets, reads their SHA-256
from the release's `SHA256SUMS`, renders
`src-tauri/packaging/homebrew/herbarium.rb.tmpl`, and pushes
`Casks/herbarium.rb` to `abdoufermat5/homebrew-tap`. The cask installs the
`.app` from the `.dmg`; Homebrew never quarantines its downloads, so no `xattr`
step is needed. When the builds are unsigned, the job adds a `caveats` block
explaining the Gatekeeper bypass; signed builds get no caveats.

One-time setup:

1. Create the public repository `abdoufermat5/homebrew-tap`.
2. Create a token that can push to it — a fine-grained PAT with **Contents:
   Read and write** on that repository, or a classic token with the `repo`
   scope — and store it as `HOMEBREW_TAP_TOKEN`.
3. Optionally, add a `Casks/herbarium.rb` placeholder; the next release replaces
   it.

Missing `HOMEBREW_TAP_TOKEN`: the job is skipped and the tap is left untouched.
Users can still install from the releases page.

## WinGet

The `winget` job runs `vedantmgoyal9/winget-releaser`, which opens a PR against
`microsoft/winget-pkgs` for the identifier `abdoufermat5.Herbarium`. The
`installers-regex` matches the Windows assets the bundler produces
(`*-setup.exe` and `*.msi`).

One-time setup:

1. Submit the **first** version of `abdoufermat5.Herbarium` to
   `microsoft/winget-pkgs` manually (or with `wingetcreate`). winget-releaser
   only *updates* a package that already exists, and fails early if it does not.
2. Fork `microsoft/winget-pkgs` under the `abdoufermat5` account; the action
   syncs and pushes to that fork.
3. Create a PAT with the **`public_repo`** scope (classic) and store it as
   `WINGET_TOKEN`.

Missing `WINGET_TOKEN`: the job is skipped; the version simply is not submitted
to WinGet.

## AUR (`herbarium-bin`)

The `aur` job renders `src-tauri/packaging/aur/PKGBUILD.tmpl` for the x86_64 and
aarch64 `.deb` assets with their SHA-256s, generates `.SRCINFO`, and pushes both
to the AUR with `KSXGitHub/github-actions-deploy-aur`. The PKGBUILD unpacks the
release `.deb` payload into `$pkgdir`.

One-time setup:

1. Create an AUR account and add an SSH public key to it.
2. Register the `herbarium-bin` package by pushing an initial `PKGBUILD` +
   `.SRCINFO` once (the AUR has no package-creation API; the repository must
   exist before the action can clone it).
3. Create a dedicated, passphrase-less SSH key for this repository and store the
   **private** key (OpenSSH format, including the header and footer) as
   `AUR_SSH_PRIVATE_KEY`.

Missing `AUR_SSH_PRIVATE_KEY`: the job is skipped and the AUR package is left at
its current version.

## Flatpak (Flathub)

Flatpak is the one channel with no automation secret: Flathub builds from a
manifest in its own repository, so each release is a pull request.

The manifest lives at
`src-tauri/packaging/flatpak/io.github.abdoufermat5.Herbarium.yml`, with the
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

`docs/0.2-design.md` and the main README describe the normal flow:

```bash
pnpm release patch   # or minor | major | x.y.z
git push --follow-tags
```

The tag runs the gate checks, builds every platform, publishes the GitHub
release, and then runs whichever optional channels are configured. The template
files under `src-tauri/packaging/` carry `__PLACEHOLDER__` tokens and are **not**
meant to be edited by hand — except the Flatpak manifest, whose version-pinned
URLs and checksums are updated per release as described above.
