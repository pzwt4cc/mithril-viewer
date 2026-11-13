# Packaging

Distribution packaging recipes and manifests for **Mithril Viewer** (`io.github.pzwt4cc.Mithril`).

## Structure

- [`arch/`](arch/) — Arch Linux AUR package (`PKGBUILD` and `.SRCINFO`) for `mithril-viewer-git`
- [`flatpak/`](flatpak/) — Flatpak / Flathub manifest (`io.github.pzwt4cc.Mithril.yml`), `flathub.json`, and offline Cargo dependency generator (`generate-cargo-sources.py`)

Future distribution formats (such as `appimage/`, `debian/`, or `rpm/`) belong in dedicated subdirectories alongside `arch/` and `flatpak/`.

## Upstream Desktop Metadata

Shared desktop integration assets live in [`../data/`](../data/):

- `data/io.github.pzwt4cc.Mithril.desktop`
- `data/io.github.pzwt4cc.Mithril.metainfo.xml`
- `data/icons/hicolor/*/apps/io.github.pzwt4cc.Mithril.{png,svg}`
