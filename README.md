<div align="center">

<img src="data/icons/hicolor/512x512/apps/mithril-viewer.png" alt="Mithril Viewer logo" width="160">

# Mithril Viewer

**A fast, minimal, GPU-accelerated image viewer for Linux.**
Smooth zoom, animated GIF/WebP, quick crop and rotate, and a clean dark UI.

![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)
![Rust](https://img.shields.io/badge/Rust-orange.svg)
![Platform](https://img.shields.io/badge/platform-Linux-lightgrey.svg)
![Wayland and X11](https://img.shields.io/badge/Wayland%20%7C%20X11-supported-success.svg)

English · [Русский](README.ru.md)

</div>

---

## Features

- **Formats:** PNG, JPEG, GIF, WebP, BMP, SVG, including animated images
- **Smooth zoom and pan:** mouse wheel zooms at the cursor, drag to move
- **Edit:** rotate, crop with a visual selection, copy to clipboard
- **Browse a folder:** switch between images next to the opened one
- **Info panel:** name, folder, resolution, format, size, created and modified dates
- **Safe editing:** your original is never overwritten. Changes are saved as a copy, and you are asked before closing
- **Print** through CUPS
- **Delete** to trash or permanently (with confirmation)
- **Drag & drop** a file into the window
- **Customizable:** language (English / Русский), window opacity
- **Native feel:** frameless rounded window with GPU rendering (OpenGL ES 3.0), works on Wayland and X11

## Installation

### Arch Linux (AUR)

```sh
yay -S mithril-viewer-git
```

### From source

You need a recent Rust toolchain (edition 2024, Rust 1.85 or newer).

```sh
git clone https://github.com/pzwt4cc/mithril-viewer.git
cd mithril-viewer
cargo build --release
./target/release/mithril-viewer path/to/image.png
```

Runtime libraries: `libglvnd` (EGL), `libxkbcommon`, `wayland`, `libx11`, `fontconfig`.
Optional: `cups` for printing, `xdg-utils` for "open containing folder".

To add it to your application menu and "Open with" lists, install the desktop files and icons:

```sh
sudo install -Dm755 target/release/mithril-viewer /usr/local/bin/mithril-viewer
sudo install -Dm644 data/mithril-viewer.desktop /usr/local/share/applications/mithril-viewer.desktop
sudo cp -r data/icons/hicolor /usr/local/share/icons/
sudo gtk-update-icon-cache -f /usr/local/share/icons/hicolor 2>/dev/null || true
```

## Usage

```sh
mithril-viewer image.png
```

Or just pick **Mithril Viewer** in "Open with", or drop an image onto the window.

### Keyboard shortcuts

| Key                              | Action                               |
| -------------------------------- | ------------------------------------ |
| `←` / `→`, `PageUp` / `PageDown` | Previous / next image                |
| Mouse wheel                      | Zoom at the cursor                   |
| `0`                              | Reset zoom                           |
| `R` / `Shift+R`                  | Rotate clockwise / counter-clockwise |
| `C`                              | Crop (`Enter` apply, `Esc` cancel)   |
| `Ctrl+C`                         | Copy image to clipboard              |
| `Ctrl+P`                         | Print                                |
| `Delete`                         | Delete image                         |
| `I`                              | Toggle the info panel                |
| `Esc`                            | Close menu / cancel crop / quit      |

Right-click the image for the context menu, and use the `⋯` button in the header for settings.

## Contributing

Issues and pull requests are welcome.

## Credits

Designed and maintained by [pzwt4cc](https://github.com/pzwt4cc). Built with the help of AI (Claude by Anthropic).

## License

Mithril Viewer is released under the [MIT License](LICENSE).

The bundled [Inter](https://rsms.me/inter/) font is licensed under the SIL Open Font License 1.1, see [`assets/OFL.txt`](assets/OFL.txt).
