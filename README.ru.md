<div align="center">

<img src="data/icons/hicolor/512x512/apps/io.github.pzwt4cc.Mithril.png" alt="Логотип Mithril Viewer" width="160">

# Mithril Viewer

**Быстрый минималистичный просмотрщик изображений для Linux с GPU-ускорением.**
Плавный зум, анимированные GIF/WebP/APNG, быстрая обрезка, поворот и отражение, аккуратный интерфейс.

![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)
![Rust](https://img.shields.io/badge/Rust-orange.svg)
![Platform](https://img.shields.io/badge/platform-Linux-lightgrey.svg)
![Wayland and X11](https://img.shields.io/badge/Wayland%20%7C%20X11-supported-success.svg)

[English](README.md) · Русский

</div>

---

## Возможности

- **Форматы:** PNG, APNG, JPEG, GIF, WebP, BMP, SVG, AVIF, ICO, TIFF, QOI, включая анимацию
- **Плавный зум и перемещение:** колесо мыши масштабирует под курсором, картинку можно таскать мышью, `F11` — полный экран
- **Редактирование:** поворот, отражение по горизонтали/вертикали, обрезка с визуальным выделением, копирование в буфер обмена
- **Просмотр папки:** переключение между изображениями рядом с открытым с фоновой предзагрузкой соседних кадров
- **Панель информации:** имя, папка, разрешение, формат, размер, даты создания и изменения
- **Безопасное редактирование:** оригинал никогда не перезаписывается. Изменения сохраняются копией, а при закрытии приложение спросит, что делать
- **Интеграция с рабочим столом:** установка как обоев (`swww`, `awww`, `hyprpaper`, `swaybg`, `feh`, Plasma, GNOME), печать через CUPS, удаление в корзину или навсегда (с подтверждением)
- **Drag & drop:** можно перетащить файл в окно
- **Настройки:** 6 языков (English, Русский, Українська, Deutsch, Français, Español), тёмная/светлая тема, прозрачность окна
- **Нативный вид:** окно без рамки со скруглёнными углами, GPU-рендеринг (OpenGL ES 3.0), работает в Wayland и X11

## Установка

### Arch Linux (AUR)

```sh
yay -S mithril-viewer-git
```

Рецепт сборки: [`packaging/arch/PKGBUILD`](packaging/arch/PKGBUILD).

### Flatpak

См. [`packaging/flatpak/README.md`](packaging/flatpak/README.md) для локальной сборки Flatpak и публикации на Flathub.

### Из исходников

Нужен свежий Rust (edition 2024, Rust 1.85 или новее).

```sh
git clone https://github.com/pzwt4cc/mithril-viewer.git
cd mithril-viewer
cargo build --release
./target/release/mithril-viewer путь/к/картинке.png
```

Библиотеки для запуска: `dav1d`, `libglvnd` (EGL), `libxkbcommon`, `wayland`, `libx11`, `fontconfig`.
Опционально: `cups` для печати, `xdg-utils` для «открыть папку с файлом», утилиты для обоев (`hyprpaper`, `swww`, `swaybg`, `feh`).

Чтобы программа появилась в меню приложений и в списке «Открыть с помощью», нужно установить desktop-файл и иконки:

```sh
sudo install -Dm755 target/release/mithril-viewer /usr/local/bin/mithril-viewer
sudo install -Dm644 data/io.github.pzwt4cc.Mithril.desktop /usr/local/share/applications/io.github.pzwt4cc.Mithril.desktop
sudo install -Dm644 data/io.github.pzwt4cc.Mithril.metainfo.xml /usr/local/share/metainfo/io.github.pzwt4cc.Mithril.metainfo.xml
sudo cp -r data/icons/hicolor /usr/local/share/icons/
sudo gtk-update-icon-cache -f /usr/local/share/icons/hicolor 2>/dev/null || true
```

## Использование

```sh
mithril-viewer image.png
```

Или выбери **Mithril Viewer** в «Открыть с помощью», либо перетащи картинку в окно.

### Горячие клавиши

| Клавиша                          | Действие                                  |
| -------------------------------- | ----------------------------------------- |
| `←` / `→`, `PageUp` / `PageDown` | Предыдущее / следующее изображение        |
| Колесо мыши                      | Зум под курсором                          |
| `0`                              | Сбросить зум                              |
| `F11`                            | Полный экран                              |
| `R` / `Shift+R`                  | Повернуть по часовой / против часовой     |
| `Ctrl+R` / `Ctrl+Shift+R`        | Отразить по горизонтали / вертикали       |
| `C`                              | Обрезка (`Enter` применить, `Esc` отмена) |
| `Ctrl+C`                         | Скопировать изображение в буфер обмена    |
| `Ctrl+P`                         | Печать                                    |
| `Delete`                         | Удалить изображение                       |
| `I`                              | Показать / скрыть панель информации       |
| `Esc`                            | Закрыть меню / отменить обрезку / выйти   |

Правый клик по картинке открывает контекстное меню, а кнопка `⋯` в шапке открывает настройки.

## Участие в разработке

Issues и pull request'ы приветствуются.

## Авторство

Автор и мейнтейнер — [pzwt4cc](https://github.com/pzwt4cc). Проект создан при помощи ИИ (Claude от Anthropic).

## Лицензия

Mithril Viewer распространяется по лицензии [MIT](LICENSE).

Встроенный шрифт [Inter](https://rsms.me/inter/) распространяется по лицензии SIL Open Font License 1.1, см. [`assets/OFL.txt`](assets/OFL.txt).
