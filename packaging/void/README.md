# Void Linux Packaging

Packaging template for `mithril-viewer` using Void Linux `xbps-src`.

## Option 1: Build and install via `void-packages` (`xbps-src`)

Works on both `glibc` and `musl` Void Linux installations:

```sh
git clone --depth 1 https://github.com/void-linux/void-packages.git
cd void-packages
./xbps-src binary-bootstrap

mkdir -p srcpkgs/mithril-viewer
cp /path/to/mithril-viewer/packaging/void/template srcpkgs/mithril-viewer/template

./xbps-src pkg mithril-viewer
sudo xbps-install -R hostdir/binpkgs mithril-viewer
```

To update the checksum after bumping `version` in `template`:

```sh
xgensum -i srcpkgs/mithril-viewer/template
```

## Option 2: Build directly from source

```sh
sudo xbps-install -S rust cargo pkg-config libdav1d-devel fontconfig libglvnd libX11 libxkbcommon wayland hicolor-icon-theme
git clone https://github.com/pzwt4cc/mithril-viewer.git
cd mithril-viewer
cargo build --release --locked
```

Optional runtime tools: `cups` (printing), `xdg-desktop-portal` (Open/Save dialogs & wallpaper portal), `xdg-utils` (open folder), `swww`/`swaybg`/`hyprpaper`/`feh` (wallpaper on standalone WMs).
