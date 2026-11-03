<div align="center">

<img src="data/icons/hicolor/512x512/apps/mithril-viewer.png" alt="Логотип Mithril Viewer" width="160">

# Mithril Viewer

**Быстрый минималистичный просмотрщик изображений для Linux с GPU-ускорением.**
Плавный зум, анимированные GIF/WebP, быстрая обрезка и поворот, аккуратный тёмный интерфейс.

![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)
![Rust](https://img.shields.io/badge/Rust-2024-orange.svg)
![Platform](https://img.shields.io/badge/platform-Linux-lightgrey.svg)
![Wayland and X11](https://img.shields.io/badge/Wayland%20%7C%20X11-supported-success.svg)

[English](README.md) · Русский

</div>

---

## Возможности

- **Форматы:** PNG, JPEG, GIF, WebP, BMP, включая анимацию
- **Плавный зум и перемещение:** колесо мыши масштабирует под курсором, картинку можно таскать мышью
- **Редактирование:** поворот, обрезка с визуальным выделением, копирование в буфер обмена
- **Просмотр папки:** переключение между изображениями рядом с открытым
- **Панель информации:** имя, папка, разрешение, формат, размер, даты создания и изменения
- **Безопасное редактирование:** оригинал никогда не перезаписывается. Изменения сохраняются копией, а при закрытии приложение спросит, что делать
- **Печать** через CUPS
- **Удаление** в корзину или навсегда (с подтверждением)
- **Drag & drop:** можно перетащить файл в окно
- **Настройки:** язык (English / Русский), цвет акцента, прозрачность окна
- **Нативный вид:** окно без рамки со скруглёнными углами, GPU-рендеринг (OpenGL ES 3.0), работает в Wayland и X11

## Установка

### Arch Linux (AUR)

```sh
yay -S mithril-viewer-git
```

### Из исходников

Нужен свежий Rust (edition 2024, Rust 1.85 или новее).

```sh
git clone https://github.com/pzwt4cc/mithril-viewer.git
cd mithril-viewer
cargo build --release
./target/release/mithril-viewer путь/к/картинке.png
```

Библиотеки для запуска: `libglvnd` (EGL), `libxkbcommon`, `wayland`, `libx11`, `fontconfig`.
Опционально: `cups` для печати, `xdg-utils` для «открыть папку с файлом».

Чтобы программа появилась в меню приложений и в списке «Открыть с помощью», установи desktop-файл и иконки:

```sh
sudo install -Dm755 target/release/mithril-viewer /usr/local/bin/mithril-viewer
sudo install -Dm644 data/mithril-viewer.desktop /usr/local/share/applications/mithril-viewer.desktop
sudo cp -r data/icons/hicolor /usr/local/share/icons/
sudo gtk-update-icon-cache -f /usr/local/share/icons/hicolor 2>/dev/null || true
```

## Использование

```sh
mithril-viewer image.png
```

Или выбери **Mithril Viewer** в «Открыть с помощью», либо перетащи картинку в окно.

### Горячие клавиши

| Клавиша | Действие |
| --- | --- |
| `←` / `→`, `PageUp` / `PageDown` | Предыдущее / следующее изображение |
| Колесо мыши | Зум под курсором |
| `0` | Сбросить зум |
| `R` / `Shift+R` | Повернуть по часовой / против часовой |
| `C` | Обрезка (`Enter` применить, `Esc` отмена) |
| `Ctrl+C` | Скопировать изображение в буфер обмена |
| `Ctrl+P` | Печать |
| `Delete` | Удалить изображение |
| `I` | Показать / скрыть панель информации |
| `Esc` | Закрыть меню / отменить обрезку / выйти |

Правый клик по картинке открывает контекстное меню, а кнопка `⋯` в шапке открывает настройки.

## Участие в разработке

Issues и pull request'ы приветствуются.

## Лицензия

Mithril Viewer распространяется по лицензии [MIT](LICENSE).

Встроенный шрифт [Inter](https://rsms.me/inter/) распространяется по лицензии SIL Open Font License 1.1, см. [`assets/OFL.txt`](assets/OFL.txt).
