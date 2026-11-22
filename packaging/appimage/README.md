# AppImage Packaging

Portable AppImage bundle for **Mithril Viewer** (`x86_64` and `aarch64`), bundling `libdav1d` while using the host system's `glibc` and GPU/display stack (`libGL`/`libEGL`, `wayland`, `libX11`).

## Run prebuilt AppImage

Download the AppImage for your architecture from [GitHub Releases](https://github.com/pzwt4cc/mithril-viewer/releases):

```sh
chmod +x Mithril-Viewer-*.AppImage
./Mithril-Viewer-*.AppImage
```

## Build locally

```sh
./packaging/appimage/build-appimage.sh
```

The output file will be created at `target/appimage/Mithril-Viewer-<version>-<arch>.AppImage`.
