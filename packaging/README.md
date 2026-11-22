# Packaging

Distribution packaging recipes and manifests for **Mithril Viewer** (`io.github.pzwt4cc.mithril-viewer`).

## Structure

- [`appimage/`](appimage/) — Portable AppImage build script (`build-appimage.sh`) and `AppRun` entrypoint (automated via [`.github/workflows/release.yml`](../.github/workflows/release.yml))
- [`arch/`](arch/) — Arch Linux AUR package (`PKGBUILD` and `.SRCINFO`) for `mithril-viewer-git`
- [`flatpak/`](flatpak/) — Flatpak / Flathub manifest (`io.github.pzwt4cc.mithril-viewer.yml`) and offline Cargo dependency generator (`generate-cargo-sources.rs`)
- [`void/`](void/) — Void Linux `xbps-src` package [`template`](void/template) for `mithril-viewer`

## Upstream Desktop Metadata

Shared desktop integration assets live in [`../data/`](../data/):

- `data/io.github.pzwt4cc.mithril-viewer.desktop`
- `data/io.github.pzwt4cc.mithril-viewer.metainfo.xml`
- `data/icons/hicolor/*/apps/io.github.pzwt4cc.mithril-viewer.{png,svg}`
