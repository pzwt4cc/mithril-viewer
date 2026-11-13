mod anim;
mod config;
mod crop;
mod draw;
mod fonts;
mod gpu;
mod i18n;
mod info;
mod layout;
mod loader;
mod menu;
mod modal;
mod nav;
mod prefetch;
mod theme;
mod ui;
mod view;
mod wallpaper;

use std::{
    borrow::Cow,
    f32::consts::FRAC_PI_2,
    path::PathBuf,
    rc::Rc,
    time::{Duration, Instant},
};

use fontdue::{Font, FontSettings};
use image::{DynamicImage, ImageFormat, RgbaImage, imageops};
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::{MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy},
    keyboard::{KeyCode, PhysicalKey},
    window::{CursorIcon, Fullscreen, Icon, ResizeDirection, Window, WindowId},
};

use anim::Anim;
use config::{Config, DeleteMode};
use crop::{
    Crop, Drag, Handle, PRESETS, Preset, drag_rect, draw_crop, draw_ratios, handle_icon,
    lock_ratio, ratio_rect,
};
use draw::{Canvas, ellipsize, text_width};
use fonts::Fonts;
use gpu::Gpu;
use i18n::tr;
use info::Info;
use layout::{EDGE, Hit, LOGO_S, Layout, START_H, START_W};
use menu::{Action, Menu};
use prefetch::Prefetcher;
use modal::{BarAction, CropBar, CropField, Dialog, DialogAction};
use nav::first_image;
use ui::{draw_header, draw_sidebar, draw_start};
use view::{View, fill_scale, fit_scale};

const APP_ID: &str = "mithril-viewer";

static ICON_PNG: &[u8] = include_bytes!("../data/icons/hicolor/256x256/apps/mithril-viewer.png");

fn window_icon() -> Option<Icon> {
    let img = image::load_from_memory_with_format(ICON_PNG, ImageFormat::Png)
        .ok()?
        .into_rgba8();
    let (w, h) = img.dimensions();
    Icon::from_rgba(img.into_raw(), w, h).ok()
}

const R_WINDOW: f32 = 14.0;
pub const R_PILL: f32 = 9.0;
pub const R_PANEL: f32 = 10.0;
const R_MENU: f32 = 9.0;
pub const R_ITEM: f32 = 6.0;
pub const R_BTN: f32 = 6.0;

const ZOOM_STEP: f32 = 1.4;

const SHADOW_PAD: f32 = 10.0;
const SHADOW_BLUR: f32 = 8.0;

const NAV_REPEAT: Duration = Duration::from_millis(60);

pub fn surface(cv: &mut Canvas, x: f32, y: f32, w: f32, h: f32, r: f32, s: f32) {
    let t = theme::current();
    let shadow_alpha = if t.is_light { 0.12 } else { 0.22 };
    cv.shadow(x, y + 2.0 * s, w, h, r, SHADOW_BLUR * s, shadow_alpha);
    cv.round_rect_grad(x, y, w, h, r, t.edge_top, t.edge_bot);
    cv.round_rect_grad(
        x + 1.0,
        y + 1.0,
        w - 2.0,
        h - 2.0,
        r - 1.0,
        t.surf_top,
        t.surf_bot,
    );
}

#[derive(Clone, Copy)]
pub struct Fade {
    pub v: f32,
    pub to: f32,
}

impl Fade {
    pub fn new(v: f32) -> Self {
        Self { v, to: v }
    }

    pub fn step(&mut self, dt: f32, speed: f32) -> bool {
        let d = self.to - self.v;
        if d == 0.0 {
            return false;
        }
        if d.abs() < 0.003 {
            self.v = self.to;
            return true;
        }
        self.v += d * (1.0 - (-dt * speed).exp());
        true
    }
}

struct Toast {
    text: String,
    until: Instant,
    appear: Fade,
    dirty: bool,
}

enum UserEvent {
    Printed(Result<bool, String>),
    Picked(Option<PathBuf>),
    PickedFolder(Option<PathBuf>),
    Prefetched(u64, PathBuf, Box<loader::Loaded>),
    Wallpaper(Result<(), String>),
}

enum After {
    Exit,
    Navigate(isize),
    Open(PathBuf),
}

#[derive(Clone, Copy)]
struct Source {
    dims: (u32, u32),
    format: Option<ImageFormat>,
    has_alpha: bool,
}

struct CropBackup {
    image: RgbaImage,
    src: Option<Source>,
    info: Option<Info>,
    modified: bool,
    rotation: u8,
    flipped: bool,
    cropped: bool,
}

#[allow(clippy::manual_checked_ops)]
fn logo_pixels(size: u32) -> Vec<u32> {
    let Ok(img) = image::load_from_memory_with_format(ICON_PNG, ImageFormat::Png) else {
        return Vec::new();
    };
    let mut rgba = img.into_rgba8();
    for p in rgba.pixels_mut() {
        let a = p[3] as u32;
        for c in 0..3 {
            p[c] = ((p[c] as u32 * a + 127) / 255) as u8;
        }
    }
    let mut out = imageops::resize(&rgba, size, size, imageops::FilterType::Lanczos3);
    for p in out.pixels_mut() {
        let a = p[3] as u32;
        if a > 0 {
            for c in 0..3 {
                p[c] = ((p[c] as u32 * 255 + a / 2) / a).min(255) as u8;
            }
        }
    }
    out.pixels()
        .map(|p| ((p[3] as u32) << 24) | ((p[0] as u32) << 16) | ((p[1] as u32) << 8) | p[2] as u32)
        .collect()
}

fn is_nav_key(k: PhysicalKey) -> bool {
    matches!(
        k,
        PhysicalKey::Code(
            KeyCode::ArrowLeft | KeyCode::ArrowRight | KeyCode::PageUp | KeyCode::PageDown
        )
    )
}

fn edge_dir(p: (f64, f64), win: (u32, u32), b: f64) -> Option<ResizeDirection> {
    use ResizeDirection::*;
    let (l, r) = (p.0 < b, p.0 >= win.0 as f64 - b);
    let (t, bt) = (p.1 < b, p.1 >= win.1 as f64 - b);
    match (l, r, t, bt) {
        (true, _, true, _) => Some(NorthWest),
        (_, true, true, _) => Some(NorthEast),
        (true, _, _, true) => Some(SouthWest),
        (_, true, _, true) => Some(SouthEast),
        (true, ..) => Some(West),
        (_, true, ..) => Some(East),
        (_, _, true, _) => Some(North),
        (_, _, _, true) => Some(South),
        _ => None,
    }
}

fn dir_icon(d: ResizeDirection) -> CursorIcon {
    match d {
        ResizeDirection::East => CursorIcon::EResize,
        ResizeDirection::North => CursorIcon::NResize,
        ResizeDirection::NorthEast => CursorIcon::NeResize,
        ResizeDirection::NorthWest => CursorIcon::NwResize,
        ResizeDirection::South => CursorIcon::SResize,
        ResizeDirection::SouthEast => CursorIcon::SeResize,
        ResizeDirection::SouthWest => CursorIcon::SwResize,
        ResizeDirection::West => CursorIcon::WResize,
    }
}

struct App {
    font: Fonts,
    config: Config,
    window: Option<Rc<Window>>,
    gpu: Option<Gpu>,
    path: Option<PathBuf>,
    siblings: Vec<PathBuf>,
    index: usize,
    src: Option<Source>,
    info: Option<Info>,
    image: Option<RgbaImage>,
    anim: Option<Anim>,
    view: View,
    cursor: (f64, f64),
    dragging: bool,
    shift: bool,
    ctrl: bool,
    side: Fade,
    header_hide: Fade,
    drag_hide: Fade,
    hover: Option<Hit>,
    fades: [Fade; 8],
    appear: Fade,
    angle: Fade,
    cursor_icon: CursorIcon,
    menu: Option<Menu>,
    settings_open: bool,
    toast: Option<Toast>,
    crop: Option<Crop>,
    crop_backup: Option<CropBackup>,
    rotation: u8,
    flipped: bool,
    cropped: bool,
    dialog: Option<Dialog>,
    dialog_dirty: bool,
    modified: bool,
    proxy: EventLoopProxy<UserEvent>,
    clipboard: Option<arboard::Clipboard>,
    exit_requested: bool,
    picking: bool,
    printing: bool,
    logo: Option<(u32, Vec<u32>)>,
    ratio_hover: Option<usize>,
    ratio_dirty: bool,
    empty_dirty: bool,
    last_nav: Option<Instant>,
    header_dirty: bool,
    sidebar_dirty: bool,
    menu_dirty: bool,
    last_frame: Instant,
    last_vp: (u32, u32),
    after: After,
    sidebar_ready: bool,
    anim_active: bool,
    prefetcher: Prefetcher,
    nav_dir: isize,
}

impl App {
    fn new(font: Fonts, config: Config, proxy: EventLoopProxy<UserEvent>) -> Self {
        let fit = config.fit_to_window;
        Self {
            font,
            config,
            window: None,
            gpu: None,
            path: None,
            siblings: Vec::new(),
            index: 0,
            src: None,
            info: None,
            image: None,
            anim: None,
            view: View::new(fit),
            cursor: (0.0, 0.0),
            dragging: false,
            shift: false,
            ctrl: false,
            side: Fade::new(0.0),
            header_hide: Fade::new(0.0),
            drag_hide: Fade::new(0.0),
            hover: None,
            fades: [Fade::new(0.0); 8],
            appear: Fade { v: 0.0, to: 1.0 },
            angle: Fade::new(0.0),
            cursor_icon: CursorIcon::Default,
            menu: None,
            settings_open: false,
            toast: None,
            crop: None,
            crop_backup: None,
            rotation: 0,
            flipped: false,
            cropped: false,
            dialog: None,
            dialog_dirty: false,
            modified: false,
            proxy: proxy.clone(),
            prefetcher: Prefetcher::new(proxy),
            nav_dir: 1,
            clipboard: None,
            exit_requested: false,
            picking: false,
            printing: false,
            logo: None,
            ratio_hover: None,
            ratio_dirty: true,
            empty_dirty: true,
            last_nav: None,
            header_dirty: true,
            sidebar_dirty: true,
            menu_dirty: false,
            last_frame: Instant::now(),
            last_vp: (0, 0),
            after: After::Exit,
            sidebar_ready: false,
            anim_active: false,
        }
    }

    fn redraw(&self) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    fn sidebar_open(&self) -> bool {
        self.side.to > 0.5
    }

    fn toggle_sidebar(&mut self) {
        self.side.to = if self.sidebar_open() { 0.0 } else { 1.0 };
        self.header_dirty = true;
        self.sidebar_dirty = true;
        self.redraw();
    }

    fn toggle_fullscreen(&mut self) {
        let Some(win) = &self.window else { return };
        if win.fullscreen().is_some() {
            win.set_fullscreen(None);
        } else {
            win.set_fullscreen(Some(Fullscreen::Borderless(None)));
        }
        self.header_dirty = true;
        self.sidebar_dirty = true;
        self.redraw();
    }

    fn toggle_fit_to_window(&mut self) {
        self.config.fit_to_window = !self.config.fit_to_window;
        config::save(&self.config);
        self.view.fit_to_window = self.config.fit_to_window;
        self.view.reset();
        self.redraw();
    }

    fn layout(&self) -> Option<Layout> {
        let w = self.window.as_ref()?;
        let size = w.inner_size();
        let mut lay = Layout::new(
            (size.width, size.height),
            w.scale_factor() as f32,
            self.side.v,
        );
        lay.empty = self.image.is_none();
        Some(lay)
    }

    fn refresh_info(&mut self) {
        if let (Some(path), Some(src)) = (&self.path, self.src) {
            self.info = Some(Info::gather(path, src.dims, src.format, src.has_alpha));
        }
    }

    fn rescan(&mut self) {
        let Some(path) = self.path.clone() else {
            return;
        };
        let (list, index) = loader::siblings(&path);
        self.siblings = list;
        self.index = index;
        self.prefetch_neighbors();
    }

    fn prefetch_neighbors(&mut self) {
        let n = self.siblings.len();
        if n < 2 {
            self.prefetcher.clear();
            return;
        }
        let next = (self.index + 1) % n;
        let prev = (self.index + n - 1) % n;
        let mut targets = Vec::with_capacity(2);
        if self.nav_dir < 0 {
            targets.push(self.siblings[prev].clone());
            if next != prev {
                targets.push(self.siblings[next].clone());
            }
        } else {
            targets.push(self.siblings[next].clone());
            if prev != next {
                targets.push(self.siblings[prev].clone());
            }
        }
        self.prefetcher.request(targets);
    }

    fn open_path(&mut self, path: PathBuf) -> bool {
        let loaded = self
            .prefetcher
            .take(&path)
            .map_or_else(|| loader::load(&path), Ok);
        match loaded {
            Ok(l) => {
                let dims = l.image.dimensions();
                if let Some(gpu) = &mut self.gpu {
                    gpu.set_image(&l.image);
                }
                self.src = Some(Source {
                    dims,
                    format: l.format,
                    has_alpha: l.has_alpha,
                });
                self.info = Some(Info::gather(&path, dims, l.format, l.has_alpha));
                let title = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "Mithril".into());
                if let Some(window) = &self.window {
                    window.set_title(&title);
                }
                self.image = Some(l.image);
                self.anim = l.anim;
                self.path = Some(path);
                self.modified = false;
                self.rotation = 0;
                self.flipped = false;
                self.cropped = false;
                self.header_hide = Fade::new(0.0);
                self.drag_hide = Fade::new(0.0);
                self.view = View::new(self.config.fit_to_window);
                self.angle = Fade::new(0.0);
                self.appear = Fade { v: 0.0, to: 1.0 };
                self.header_dirty = true;
                self.sidebar_dirty = true;
                self.redraw();
                true
            }
            Err(e) => {
                eprintln!("mithril: {}: {e}", path.display());
                self.show_toast(format!("{}: {e}", tr("toast.open_failed")));
                false
            }
        }
    }

    fn pick_file(&mut self) {
        if self.picking || self.dialog.is_some() || self.crop_active() {
            return;
        }
        self.close_menu();
        self.picking = true;
        let proxy = self.proxy.clone();
        let start = self
            .path
            .as_ref()
            .and_then(|p| p.parent())
            .map(|d| d.to_path_buf());
        std::thread::spawn(move || {
            let mut dlg = rfd::FileDialog::new()
                .add_filter("Images", &["png", "apng", "jpg", "jpeg", "gif", "webp", "bmp", "svg", "svgz", "avif", "ico", "tiff", "tif", "qoi"])
                .add_filter("All Files", &["*"]);
            if let Some(dir) = start {
                dlg = dlg.set_directory(dir);
            }
            let _ = proxy.send_event(UserEvent::Picked(dlg.pick_file()));
        });
    }

    fn pick_folder(&mut self) {
        if self.picking || self.dialog.is_some() || self.crop_active() {
            return;
        }
        self.close_menu();
        self.picking = true;
        let proxy = self.proxy.clone();
        let start = self
            .path
            .as_ref()
            .and_then(|p| p.parent())
            .map(|d| d.parent().unwrap_or(d).to_path_buf());
        std::thread::spawn(move || {
            let mut dlg = rfd::FileDialog::new();
            if let Some(dir) = start {
                dlg = dlg.set_directory(dir);
            }
            let _ = proxy.send_event(UserEvent::PickedFolder(dlg.pick_folder()));
        });
    }

    fn open_dir(&mut self, dir: PathBuf) {
        if self.dialog.is_some() || self.crop_active() {
            return;
        }
        let Some(first) = first_image(&dir) else {
            self.show_toast(tr("toast.no_images").to_string());
            return;
        };
        if self.modified {
            self.open_external(first);
            return;
        }
        self.close_menu();
        if self.open_path(first) {
            self.rescan();
            if self.index != 0
                && let Some(p) = self.siblings.first().cloned()
                    && self.open_path(p) {
                        self.index = 0;
                    }
        }
    }

    fn open_external(&mut self, path: PathBuf) {
        if self.dialog.is_some() || self.crop_active() {
            return;
        }
        if self.modified {
            self.after = After::Open(path);
            self.open_dialog();
            return;
        }
        self.close_menu();
        if self.open_path(path) {
            self.rescan();
        }
    }

    fn navigate(&mut self, delta: isize) {
        if self.crop_active() || self.dialog.is_some() || self.siblings.len() < 2 {
            return;
        }
        if self.modified {
            self.after = After::Navigate(delta);
            self.open_dialog();
            return;
        }
        self.close_menu();
        self.nav_dir = delta;
        let n = self.siblings.len() as isize;
        let mut i = self.index as isize;
        for _ in 1..n {
            i = (i + delta).rem_euclid(n);
            let path = self.siblings[i as usize].clone();
            if self.open_path(path) {
                self.index = i as usize;
                self.last_nav = Some(Instant::now());
                self.prefetch_neighbors();
                return;
            }
        }
    }

    fn nav_key(&mut self, delta: isize, repeat: bool) {
        if repeat && self.last_nav.is_some_and(|t| t.elapsed() < NAV_REPEAT) {
            return;
        }
        self.navigate(delta);
        if repeat {
            self.appear = Fade { v: 1.0, to: 1.0 };
        }
    }

    fn after_delete(&mut self) {
        self.modified = false;
        if self.index < self.siblings.len() {
            let removed = self.siblings.remove(self.index);
            self.prefetcher.invalidate(&removed);
        }
        while !self.siblings.is_empty() {
            let i = self.index.min(self.siblings.len() - 1);
            let path = self.siblings[i].clone();
            if self.open_path(path) {
                self.index = i;
                self.prefetch_neighbors();
                return;
            }
            self.siblings.remove(i);
        }
        self.prefetcher.clear();
        self.exit_requested = true;
    }

    fn open_folder(&self) {
        let Some(info) = &self.info else { return };
        if let Ok(mut child) = std::process::Command::new("xdg-open")
            .arg(&info.folder_path)
            .spawn()
        {
            std::thread::spawn(move || {
                let _ = child.wait();
            });
        }
    }

    fn show_toast(&mut self, text: String) {
        self.toast = Some(Toast {
            text,
            until: Instant::now() + Duration::from_millis(1800),
            appear: Fade { v: 0.0, to: 1.0 },
            dirty: true,
        });
        self.redraw();
    }

    fn open_menu(&mut self) {
        let Some(lay) = self.layout() else { return };
        if self.settings_open {
            self.settings_open = false;
            self.header_dirty = true;
        }
        let entries = menu::context_entries(self.config.delete_mode, self.config.fit_to_window);
        let anchor = (self.cursor.0 as i32, self.cursor.1 as i32);
        let mut menu = Menu::new(entries, anchor, lay.win, lay.scale, false);
        menu.update_cursor(self.cursor, &self.font);
        self.menu = Some(menu);
        self.menu_dirty = true;
        self.redraw();
    }

    fn open_settings(&mut self) {
        let Some(lay) = self.layout() else { return };
        let entries = menu::settings_entries(
            self.config.theme,
            self.config.language.as_deref(),
            self.config.delete_mode,
            self.config.opacity,
        );
        let (cx, cy, cw, ch) = lay.capsule();
        let anchor = ((cx + cw) as i32, (cy + ch + 6.0 * lay.scale) as i32);
        let mut menu = Menu::new(entries, anchor, lay.win, lay.scale, true);
        menu.update_cursor(self.cursor, &self.font);
        self.menu = Some(menu);
        self.settings_open = true;
        self.menu_dirty = true;
        self.header_dirty = true;
        self.redraw();
    }

    fn close_menu(&mut self) {
        if self.menu.take().is_some() {
            if self.settings_open {
                self.settings_open = false;
                self.header_dirty = true;
            }
            self.redraw();
        }
    }

    fn set_theme(&mut self, mode: theme::ThemeMode) {
        self.config.theme = mode;
        config::save(&self.config);
        theme::set(mode);
        if let Some(gpu) = &mut self.gpu {
            gpu.set_overlay(gpu::BG, &theme::bg_texture(), 1, 1);
        }
        self.header_dirty = true;
        self.sidebar_dirty = true;
        self.menu_dirty = true;
        self.dialog_dirty = true;
        self.empty_dirty = true;
        self.ratio_dirty = true;
        if let Some(c) = &mut self.crop {
            c.bar_dirty = true;
        }
        self.redraw();
    }

    fn set_language(&mut self, code: &'static str) {
        self.config.language = if code == "auto" {
            None
        } else {
            Some(code.to_string())
        };
        i18n::init(self.config.language.as_deref());
        config::save(&self.config);
        self.refresh_info();
        self.header_dirty = true;
        self.sidebar_dirty = true;
        self.redraw();
    }

    fn apply(&mut self, action: Action) {
        match action {
            Action::ResetZoom => {
                self.view.reset();
                self.redraw();
            }
            Action::RotateLeft => self.rotate(false),
            Action::RotateRight => self.rotate(true),
            Action::FlipHorizontal => self.flip(true),
            Action::FlipVertical => self.flip(false),
            Action::Crop => self.start_crop(),
            Action::Fullscreen => self.toggle_fullscreen(),
            Action::ToggleFitToWindow => self.toggle_fit_to_window(),
            Action::Print => self.print(),
            Action::SetWallpaper => self.set_wallpaper(),
            Action::Copy => self.copy_image(),
            Action::Delete => self.request_delete(),
            Action::SetTheme(mode) => self.set_theme(mode),
            Action::SetLanguage(code) => self.set_language(code),
            Action::SetOpacity(p) => {
                self.config.opacity = p;
                config::save(&self.config);
                self.redraw();
            }
            Action::SetDeleteMode(mode) => {
                self.config.delete_mode = mode;
                config::save(&self.config);
            }
        }
    }

    fn rotate(&mut self, clockwise: bool) {
        if self.crop_active() || self.dialog.is_some() {
            return;
        }
        let Some(img) = self.image.take() else { return };
        let rotated = if clockwise {
            imageops::rotate90(&img)
        } else {
            imageops::rotate270(&img)
        };
        self.rotation = if clockwise ^ self.flipped {
            (self.rotation + 1) % 4
        } else {
            (self.rotation + 3) % 4
        };
        if let Some(anim) = &mut self.anim {
            anim.rotate(clockwise);
        } else {
            if let Some(gpu) = &mut self.gpu {
                gpu.set_image(&rotated);
            }
            self.modified = self.cropped || self.rotation != 0 || self.flipped;
        }
        self.image = Some(rotated);
        self.view.reset();
        self.angle = Fade {
            v: if clockwise { -FRAC_PI_2 } else { FRAC_PI_2 },
            to: 0.0,
        };
        self.redraw();
    }

    fn flip(&mut self, horizontal: bool) {
        if self.crop_active() || self.dialog.is_some() {
            return;
        }
        let Some(mut img) = self.image.take() else { return };
        if horizontal {
            imageops::flip_horizontal_in_place(&mut img);
            self.flipped = !self.flipped;
        } else {
            imageops::flip_vertical_in_place(&mut img);
            self.flipped = !self.flipped;
            self.rotation = (self.rotation + 2) % 4;
        }
        if let Some(anim) = &mut self.anim {
            anim.flip(horizontal);
        } else {
            if let Some(gpu) = &mut self.gpu {
                gpu.set_image(&img);
            }
            self.modified = self.cropped || self.rotation != 0 || self.flipped;
        }
        self.image = Some(img);
        self.redraw();
    }

    fn set_wallpaper(&mut self) {
        let Some(path) = self.path.clone() else {
            return;
        };
        let img = self
            .anim
            .as_ref()
            .map(|a| a.current().clone())
            .or_else(|| self.image.clone());
        let format = self.src.and_then(|s| s.format);
        let modified = self.modified;
        let proxy = self.proxy.clone();
        std::thread::spawn(move || {
            let res = wallpaper::set(&path, img.as_ref(), format, modified);
            let _ = proxy.send_event(UserEvent::Wallpaper(res));
        });
    }

    fn copy_image(&mut self) {
        let msg = {
            let Some(img) = self
                .anim
                .as_ref()
                .map(|a| a.current())
                .or(self.image.as_ref())
            else {
                return;
            };
            let data = arboard::ImageData {
                width: img.width() as usize,
                height: img.height() as usize,
                bytes: Cow::Borrowed(img.as_raw()),
            };
            if self.clipboard.is_none() {
                self.clipboard = arboard::Clipboard::new().ok();
            }
            match self.clipboard.as_mut() {
                Some(cb) => match cb.set_image(data) {
                    Ok(()) => tr("toast.copied").to_string(),
                    Err(e) => format!("{}: {e}", tr("toast.copy_failed")),
                },
                None => tr("toast.copy_failed").to_string(),
            }
        };
        self.show_toast(msg);
    }

    fn request_delete(&mut self) {
        let Some(path) = self.path.clone() else {
            return;
        };
        let result = match self.config.delete_mode {
            DeleteMode::Trash => trash::delete(&path).map_err(|e| e.to_string()),
            DeleteMode::Permanent => std::fs::remove_file(&path).map_err(|e| e.to_string()),
        };
        match result {
            Ok(()) => self.after_delete(),
            Err(e) => {
                self.show_toast(format!("{}: {e}", tr("toast.delete_failed")));
            }
        }
    }

    fn zoom_at_cursor(&mut self, steps: f32) {
        let Some(lay) = self.layout() else { return };
        let Some(imgd) = self.image.as_ref().map(|i| i.dimensions()) else {
            return;
        };
        let (vx, vy, vw, vh) = lay.viewport();
        let (px, py) = self.cursor;
        if px < vx as f64
            || px >= (vx + vw as i32) as f64
            || py < vy as f64
            || py >= (vy + vh as i32) as f64
        {
            return;
        }
        let vp = (vw, vh);
        let old = self.view.scale(imgd, vp);
        let min = if self.view.fit_to_window {
            fill_scale(imgd, vp)
        } else {
            fit_scale(imgd, vp)
        };
        let new = (old * ZOOM_STEP.powf(steps)).clamp(min, 32.0);

        let cx = px as f32 - (vx as f32 + vw as f32 / 2.0);
        let cy = py as f32 - (vy as f32 + vh as f32 / 2.0);
        let rx = (cx - self.view.pan.0) / old;
        let ry = (cy - self.view.pan.1) / old;

        self.view.pan = (cx - rx * new, cy - ry * new);
        self.view.zoom = if (new - min).abs() < 1e-3 {
            None
        } else {
            Some(new)
        };
        self.view.clamp_pan(imgd, vp);
        self.redraw();
    }

    fn update_pointer(&mut self) {
        let Some(window) = self.window.clone() else {
            return;
        };
        let Some(lay) = self.layout() else { return };

        let hover = if self.dialog.is_some() {
            None
        } else {
            lay.hit(self.cursor)
        };
        if hover != self.hover {
            self.hover = hover;
            for (i, f) in self.fades.iter_mut().enumerate() {
                f.to = if hover.map(|h| h.index()) == Some(i) {
                    1.0
                } else {
                    0.0
                };
            }
            window.request_redraw();
        }

        let rh = if self.crop_active() && self.dialog.is_none() {
            lay.ratio_at(self.cursor)
        } else {
            None
        };
        if rh != self.ratio_hover {
            self.ratio_hover = rh;
            self.ratio_dirty = true;
            window.request_redraw();
        }

        let mut icon = if window.is_maximized() || window.fullscreen().is_some() || self.dialog.is_some() {
            CursorIcon::Default
        } else {
            edge_dir(self.cursor, lay.win, (EDGE * lay.scale) as f64)
                .map(dir_icon)
                .unwrap_or(CursorIcon::Default)
        };
        if icon == CursorIcon::Default && self.dialog.is_none() {
            let dragging = self.crop.as_ref().and_then(|c| c.drag.map(|d| d.handle));
            let h = dragging.or_else(|| self.crop_hit(&lay, self.cursor));
            if let Some(h) = h {
                icon = handle_icon(h);
            }
        }
        if icon != self.cursor_icon {
            self.cursor_icon = icon;
            window.set_cursor(icon);
        }
    }

    fn crop_screen(&self, lay: &Layout, rect: [f32; 4]) -> Option<([f32; 4], [f32; 4])> {
        let (iw, ih) = self.image.as_ref()?.dimensions();
        let k = self.view.cur_scale;
        let (vx, vy, vw, vh) = lay.viewport();
        let cx = vx as f32 + vw as f32 / 2.0 + self.view.cur_pan.0;
        let cy = vy as f32 + vh as f32 / 2.0 + self.view.cur_pan.1;
        let (half_w, half_h) = (iw as f32 / 2.0, ih as f32 / 2.0);
        let m = |x: f32, y: f32| (cx + (x - half_w) * k, cy + (y - half_h) * k);
        let (a, b) = (m(rect[0], rect[1]), m(rect[2], rect[3]));
        let (i0, i1) = (m(0.0, 0.0), m(iw as f32, ih as f32));
        Some(([a.0, a.1, b.0, b.1], [i0.0, i0.1, i1.0, i1.1]))
    }

    fn crop_hit(&self, lay: &Layout, p: (f64, f64)) -> Option<Handle> {
        let c = self.crop.as_ref()?;
        if lay.on_ratio_panel(p) {
            return None;
        }
        let ([x0, y0, x1, y1], _) = self.crop_screen(lay, c.rect)?;
        let t = (10.0 * lay.scale) as f64;
        let (x0, y0, x1, y1) = (x0 as f64, y0 as f64, x1 as f64, y1 as f64);
        let near = |a: f64, b: f64| (a - b).abs() <= t;
        let in_x = p.0 >= x0 - t && p.0 <= x1 + t;
        let in_y = p.1 >= y0 - t && p.1 <= y1 + t;
        let (l, r) = (near(p.0, x0) && in_y, near(p.0, x1) && in_y);
        let (tp, b) = (near(p.1, y0) && in_x, near(p.1, y1) && in_x);
        match (l, r, tp, b) {
            (true, _, true, _) => Some(Handle::NW),
            (_, true, true, _) => Some(Handle::NE),
            (true, _, _, true) => Some(Handle::SW),
            (_, true, _, true) => Some(Handle::SE),
            (true, ..) => Some(Handle::W),
            (_, true, ..) => Some(Handle::E),
            (_, _, true, _) => Some(Handle::N),
            (_, _, _, true) => Some(Handle::S),
            _ if p.0 > x0 && p.0 < x1 && p.1 > y0 && p.1 < y1 => Some(Handle::Move),
            _ => None,
        }
    }

    fn start_crop(&mut self) {
        if self.crop_active() || self.dialog.is_some() {
            return;
        }
        if self.anim.is_some() {
            self.show_toast(tr("toast.crop_gif").to_string());
            return;
        }
        let Some((iw, ih)) = self.image.as_ref().map(|i| i.dimensions()) else {
            return;
        };
        let Some(lay) = self.layout() else { return };
        self.close_menu();
        self.view.reset();
        let (mx, my) = (iw as f32 * 0.08, ih as f32 * 0.08);
        self.crop = Some(Crop {
            rect: [mx, my, iw as f32 - mx, ih as f32 - my],
            drag: None,
            preset: 0,
            flip: false,
            bar: CropBar::new(&self.font, lay.scale),
            bar_dirty: true,
            appear: Fade { v: 0.0, to: 1.0 },
            canceling: false,
        });
        self.ratio_hover = None;
        self.ratio_dirty = true;
        self.redraw();
    }

    fn crop_ratio(&self) -> Option<f32> {
        let c = self.crop.as_ref()?;
        let (iw, ih) = self.image.as_ref()?.dimensions();
        let r = match PRESETS[c.preset] {
            Preset::Free => return None,
            Preset::Original => iw as f32 / ih as f32,
            Preset::Fixed(a, b) => a as f32 / b as f32,
        };
        Some(if c.flip { 1.0 / r } else { r })
    }

    fn set_crop_preset(&mut self, i: usize) {
        let Some((iw, ih)) = self.image.as_ref().map(|im| im.dimensions()) else {
            return;
        };
        let center = {
            let Some(c) = &mut self.crop else { return };
            if c.preset == i {
                if matches!(
                    PRESETS[i],
                    Preset::Free | Preset::Original | Preset::Fixed(1, 1)
                ) {
                    return;
                }
                c.flip = !c.flip;
            } else {
                c.preset = i;
                c.flip = false;
            }
            c.drag = None;
            ((c.rect[0] + c.rect[2]) / 2.0, (c.rect[1] + c.rect[3]) / 2.0)
        };
        if let Some(r) = self.crop_ratio()
            && let Some(c) = &mut self.crop {
                c.rect = ratio_rect(r, center, iw as f32, ih as f32);
            }
        self.ratio_dirty = true;
        self.redraw();
    }

    fn crop_active(&self) -> bool {
        self.crop.as_ref().is_some_and(|c| !c.canceling)
    }

    fn cancel_crop(&mut self) {
        self.crop_backup = None;
        if let Some(c) = &mut self.crop {
            c.canceling = true;
            c.appear.to = 0.0;
            self.redraw();
        }
    }

    fn commit_crop_field(&mut self) {
        let Some((iw, ih)) = self.image.as_ref().map(|im| im.dimensions()) else {
            return;
        };
        let Some(c) = &mut self.crop else { return };
        let cur_w = (c.rect[2] - c.rect[0]).round().max(1.0) as u32;
        let cur_h = (c.rect[3] - c.rect[1]).round().max(1.0) as u32;
        let new_w = c.bar.buf_w.parse::<u32>().unwrap_or(cur_w).clamp(1, iw);
        let new_h = c.bar.buf_h.parse::<u32>().unwrap_or(cur_h).clamp(1, ih);
        let cx = (c.rect[0] + c.rect[2]) / 2.0;
        let cy = (c.rect[1] + c.rect[3]) / 2.0;
        let x0 = (cx - new_w as f32 / 2.0).clamp(0.0, (iw - new_w) as f32);
        let y0 = (cy - new_h as f32 / 2.0).clamp(0.0, (ih - new_h) as f32);
        c.rect = [x0, y0, x0 + new_w as f32, y0 + new_h as f32];
        c.preset = 0;
        c.bar.editing = None;
        c.bar_dirty = true;
        self.redraw();
    }

    fn perform_crop(&mut self) -> bool {
        let Some(c) = self.crop.take() else {
            return false;
        };
        let Some(img) = self.image.as_ref() else {
            return false;
        };
        self.crop_backup = Some(CropBackup {
            image: img.clone(),
            src: self.src,
            info: self.info.clone(),
            modified: self.modified,
            rotation: self.rotation,
            flipped: self.flipped,
            cropped: self.cropped,
        });
        let (iw, ih) = img.dimensions();
        let x = (c.rect[0].round().max(0.0) as u32).min(iw.saturating_sub(1));
        let y = (c.rect[1].round().max(0.0) as u32).min(ih.saturating_sub(1));
        let w = ((c.rect[2] - c.rect[0]).round().max(1.0) as u32).min(iw - x);
        let h = ((c.rect[3] - c.rect[1]).round().max(1.0) as u32).min(ih - y);
        if (x, y, w, h) == (0, 0, iw, ih) {
            self.redraw();
            return false;
        }
        let cropped = imageops::crop_imm(img, x, y, w, h).to_image();
        if let Some(gpu) = &mut self.gpu {
            gpu.set_image(&cropped);
        }
        if let Some(src) = &mut self.src {
            src.dims = cropped.dimensions();
        }
        self.image = Some(cropped);
        self.refresh_info();
        self.sidebar_dirty = true;
        self.view.reset();
        self.modified = true;
        self.cropped = true;
        self.rotation = 0;
        self.flipped = false;
        self.redraw();
        true
    }

    fn apply_crop(&mut self) {
        if !self.perform_crop() {
            return;
        }
        let Some(lay) = self.layout() else { return };
        self.close_menu();
        self.dialog = Some(Dialog::crop(&self.font, lay.scale));
        self.dialog_dirty = true;
        self.update_modal_hover();
        self.redraw();
    }

    fn crop_press(&mut self, lay: &Layout) -> bool {
        let Some(handle) = self.crop_hit(lay, self.cursor) else {
            return false;
        };
        let cursor = self.cursor;
        if let Some(c) = &mut self.crop {
            c.drag = Some(Drag {
                handle,
                start: cursor,
                rect: c.rect,
            });
        }
        true
    }

    fn update_crop_drag(&mut self) {
        let Some((iw, ih)) = self.image.as_ref().map(|i| i.dimensions()) else {
            return;
        };
        let k = self.view.cur_scale.max(1e-6);
        let cursor = self.cursor;
        let ratio = self.crop_ratio();
        let Some(c) = &mut self.crop else { return };
        let Some(d) = c.drag else { return };
        let dx = (cursor.0 - d.start.0) as f32 / k;
        let dy = (cursor.1 - d.start.1) as f32 / k;
        let free = drag_rect(d.handle, d.rect, dx, dy, iw as f32, ih as f32);
        c.rect = match ratio {
            Some(r) => lock_ratio(d.handle, d.rect, free, r, iw as f32, ih as f32),
            None => free,
        };
        self.redraw();
    }

    fn open_dialog(&mut self) {
        let Some(lay) = self.layout() else { return };
        self.close_menu();
        self.dialog = Some(Dialog::unsaved(&self.font, lay.scale));
        self.dialog_dirty = true;
        self.update_modal_hover();
        self.redraw();
    }

    fn close_dialog(&mut self) {
        if self.dialog.take().is_some() {
            self.after = After::Exit;
            self.redraw();
        }
    }

    fn request_exit(&mut self, el: &ActiveEventLoop) {
        self.crop = None;
        if !self.modified {
            el.exit();
        } else if self.dialog.is_none() {
            self.open_dialog();
        }
    }

    fn dialog_action(&mut self, a: DialogAction, el: &ActiveEventLoop) {
        match a {
            DialogAction::Cancel => {
                self.close_dialog();
                if let Some(b) = self.crop_backup.take() {
                    if let Some(gpu) = &mut self.gpu {
                        gpu.set_image(&b.image);
                    }
                    self.image = Some(b.image);
                    self.src = b.src;
                    self.info = b.info;
                    self.modified = b.modified;
                    self.rotation = b.rotation;
                    self.flipped = b.flipped;
                    self.cropped = b.cropped;
                    self.sidebar_dirty = true;
                    self.header_dirty = true;
                    self.view.reset();
                    self.redraw();
                }
            }
            DialogAction::Discard => {
                self.crop_backup = None;
                self.finish_dialog(el);
            }
            DialogAction::Save => match self.save_copy() {
                Ok(_) => {
                    self.crop_backup = None;
                    self.finish_dialog(el);
                }
                Err(e) => {
                    self.close_dialog();
                    self.show_toast(format!("{}: {e}", tr("toast.save_failed")));
                }
            },
            DialogAction::CropReplace => {
                self.crop_backup = None;
                self.close_dialog();
                if let Err(e) = self.overwrite_current() {
                    self.show_toast(format!("{}: {e}", tr("toast.save_failed")));
                }
            }
            DialogAction::CropCopy => {
                self.crop_backup = None;
                self.close_dialog();
                match self.save_copy() {
                    Ok(_) => {
                        self.modified = false;
                        self.rescan();
                    }
                    Err(e) => self.show_toast(format!("{}: {e}", tr("toast.save_failed"))),
                }
            }
        }
    }

    fn finish_dialog(&mut self, el: &ActiveEventLoop) {
        match std::mem::replace(&mut self.after, After::Exit) {
            After::Exit => el.exit(),
            After::Navigate(delta) => {
                self.close_dialog();
                self.modified = false;
                self.navigate(delta);
            }
            After::Open(path) => {
                self.close_dialog();
                self.modified = false;
                if self.open_path(path) {
                    self.rescan();
                }
            }
        }
    }

    fn dialog_local(&self, lay: &Layout) -> Option<(f32, f32)> {
        let d = self.dialog.as_ref()?;
        let o = d.origin(lay.win);
        Some((
            self.cursor.0 as f32 - o.0 as f32,
            self.cursor.1 as f32 - o.1 as f32,
        ))
    }

    fn bar_local(&self, lay: &Layout) -> Option<(f32, f32)> {
        let c = self.crop.as_ref()?;
        let o = c.bar.origin(lay.viewport());
        Some((
            self.cursor.0 as f32 - o.0 as f32,
            self.cursor.1 as f32 - o.1 as f32,
        ))
    }

    fn update_modal_hover(&mut self) {
        let Some(lay) = self.layout() else { return };
        if let Some(p) = self.dialog_local(&lay) {
            let changed = self.dialog.as_mut().is_some_and(|d| d.set_hover(p));
            if changed {
                self.dialog_dirty = true;
                self.redraw();
            }
        }
        if let Some(p) = self.bar_local(&lay) {
            let changed = self.crop.as_mut().is_some_and(|c| c.bar.set_hover(p));
            if changed {
                if let Some(c) = &mut self.crop {
                    c.bar_dirty = true;
                }
                self.redraw();
            }
        }
    }

    fn save_to_path(&self, target: &std::path::Path) -> Result<(), String> {
        use std::io::Write;
        let img = self.image.as_ref().ok_or("no image")?;
        let orig_ext = target
            .extension()
            .map(|e| e.to_string_lossy().into_owned())
            .unwrap_or_default();
        let fmt = match self.src.and_then(|s| s.format) {
            Some(f) if !orig_ext.is_empty() => f,
            _ => ImageFormat::Png,
        };
        let file = std::fs::File::create(target).map_err(|e| e.to_string())?;
        let mut w = std::io::BufWriter::new(file);
        let result = if fmt == ImageFormat::Jpeg {
            let rgb = DynamicImage::ImageRgba8(img.clone()).into_rgb8();
            let enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut w, 92);
            DynamicImage::ImageRgb8(rgb).write_with_encoder(enc)
        } else {
            DynamicImage::ImageRgba8(img.clone()).write_to(&mut w, fmt)
        };
        result.map_err(|e| e.to_string())?;
        w.flush().map_err(|e| e.to_string())?;
        Ok(())
    }

    fn overwrite_current(&mut self) -> Result<(), String> {
        let path = self.path.clone().ok_or("no path")?;
        self.save_to_path(&path)?;
        self.prefetcher.invalidate(&path);
        self.modified = false;
        self.rotation = 0;
        self.flipped = false;
        self.cropped = false;
        self.refresh_info();
        self.sidebar_dirty = true;
        self.header_dirty = true;
        self.redraw();
        Ok(())
    }

    fn save_copy(&self) -> Result<PathBuf, String> {
        use std::io::Write;

        let img = self.image.as_ref().ok_or("no image")?;
        let path = self.path.as_ref().ok_or("no path")?;
        let orig_ext = path
            .extension()
            .map(|e| e.to_string_lossy().into_owned())
            .unwrap_or_default();
        let (fmt, ext) = match self.src.and_then(|s| s.format) {
            Some(
                f @ (ImageFormat::Png
                | ImageFormat::Jpeg
                | ImageFormat::Gif
                | ImageFormat::WebP
                | ImageFormat::Bmp),
            ) if !orig_ext.is_empty() => (f, orig_ext),
            _ => (ImageFormat::Png, "png".to_string()),
        };

        let dir = match path.parent() {
            Some(d) if !d.as_os_str().is_empty() => d.to_path_buf(),
            _ => PathBuf::from("."),
        };
        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "image".into());
        let suffix = tr("save.suffix");
        let mut out = dir.join(format!("{stem} ({suffix}).{ext}"));
        let mut n = 2;
        while out.exists() {
            out = dir.join(format!("{stem} ({suffix} {n}).{ext}"));
            n += 1;
        }

        let file = std::fs::File::create(&out).map_err(|e| e.to_string())?;
        let mut w = std::io::BufWriter::new(file);
        let result = if fmt == ImageFormat::Jpeg {
            let rgb = DynamicImage::ImageRgba8(img.clone()).into_rgb8();
            let enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut w, 92);
            DynamicImage::ImageRgb8(rgb).write_with_encoder(enc)
        } else {
            DynamicImage::ImageRgba8(img.clone()).write_to(&mut w, fmt)
        };
        result.map_err(|e| e.to_string())?;
        w.flush().map_err(|e| e.to_string())?;
        Ok(out)
    }

    fn print(&mut self) {
        if self.printing {
            return;
        }
        let Some(img) = self
            .anim
            .as_ref()
            .map(|a| a.current().clone())
            .or_else(|| self.image.clone())
        else {
            return;
        };
        self.close_menu();
        self.printing = true;
        let proxy = self.proxy.clone();
        let title = self
            .path
            .as_ref()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("Mithril")
            .to_string();
        std::thread::spawn(move || {
            let res = (|| -> Result<bool, String> {
                use std::io::Write;
                let stamp = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_nanos())
                    .unwrap_or(0);
                let path = std::env::temp_dir().join(format!("mithril-print-{stamp}.png"));
                {
                    let file = std::fs::File::create(&path).map_err(|e| e.to_string())?;
                    let mut w = std::io::BufWriter::new(file);
                    let enc = image::codecs::png::PngEncoder::new_with_quality(
                        &mut w,
                        image::codecs::png::CompressionType::Fast,
                        image::codecs::png::FilterType::NoFilter,
                    );
                    DynamicImage::ImageRgba8(img)
                        .write_with_encoder(enc)
                        .map_err(|e| e.to_string())?;
                    w.flush().map_err(|e| e.to_string())?;
                }
                let print_res = run_system_print(&path, &title);
                let _ = std::fs::remove_file(&path);
                print_res
            })();
            let _ = proxy.send_event(UserEvent::Printed(res));
        });
    }

    fn draw_ratios(&self, cv: &mut Canvas, lay: &Layout) {
        let Some(c) = &self.crop else { return };
        let (x, y, w, h, _) = lay.ratio_panel();
        let rows = (0..PRESETS.len()).map(|i| (i, lay.ratio_row(i)));
        draw_ratios(
            cv,
            &self.font,
            c,
            (x, y, w, h),
            rows,
            self.ratio_hover,
            lay.scale,
        );
    }

    fn render(&mut self) {
        let now = Instant::now();
        let dt = if self.anim_active {
            now.duration_since(self.last_frame).as_secs_f32().min(0.05)
        } else {
            1.0 / 60.0
        };
        self.last_frame = now;
        let mut busy = false;

        if self.side.step(dt, 16.0) {
            busy = true;
            self.header_dirty = true;
        }
        self.header_hide.to = if self.crop_active() { 1.0 } else { 0.0 };
        if self.header_hide.step(dt, 20.0) {
            busy = true;
        }
        let is_crop_dragging = self.crop.as_ref().is_some_and(|c| c.drag.is_some());
        self.drag_hide.to = if is_crop_dragging { 1.0 } else { 0.0 };
        if self.drag_hide.step(dt, 24.0) {
            busy = true;
        }
        for (i, f) in self.fades.iter_mut().enumerate() {
            if f.step(dt, 20.0) {
                busy = true;
                if i == Hit::OpenFolder.index() {
                    self.sidebar_dirty = true;
                } else if i != Hit::Start.index() && i != Hit::StartFolder.index() {
                    self.header_dirty = true;
                }
            }
        }
        let mut image_moving = false;
        if self.appear.step(dt, 12.0) {
            image_moving = true;
        }
        if self.angle.step(dt, 14.0) {
            image_moving = true;
        }
        if let Some(c) = &mut self.crop
            && c.appear.step(dt, 20.0) {
                busy = true;
            }
        if self
            .crop
            .as_ref()
            .is_some_and(|c| c.canceling && c.appear.v <= 0.003)
        {
            self.crop = None;
            busy = true;
        }
        if let Some(menu) = &mut self.menu
            && menu.step(dt) {
                busy = true;
                self.menu_dirty = true;
            }
        if let Some(t) = &mut self.toast {
            if now >= t.until {
                t.appear.to = 0.0;
            }
            t.appear.step(dt, 16.0);
            busy = true;
        }
        if self
            .toast
            .as_ref()
            .is_some_and(|t| t.appear.to == 0.0 && t.appear.v == 0.0)
        {
            self.toast = None;
        }

        let Some(lay) = self.layout() else { return };
        let (w, h) = lay.win;
        if w == 0 || h == 0 {
            return;
        }
        let hh = lay.top_h as u32;
        let (vx, vy, vw, vh) = lay.viewport();

        let snap = (vw, vh) != self.last_vp;
        self.last_vp = (vw, vh);
        if let Some(dims) = self.image.as_ref().map(|i| i.dimensions()) {
            self.view.clamp_pan(dims, (vw, vh));
            if self.view.step(dims, (vw, vh), dt, snap) {
                image_moving = true;
            }
        }
        if image_moving {
            busy = true;
        }

        let mut header = None;
        let mut sidebar = None;
        if self.header_dirty {
            self.header_dirty = false;
            let mut hb = vec![0u32; (w * hh) as usize];
            let is_fullscreen = self.window.as_ref().is_some_and(|w| w.fullscreen().is_some());
            draw_header(
                &mut Canvas {
                    px: &mut hb,
                    w,
                    h: hh,
                    ox: 0,
                    oy: 0,
                    clip: None,
                },
                &self.font,
                &lay,
                self.info.as_ref(),
                self.siblings.len(),
                self.index,
                &self.fades,
                self.side.v,
                self.settings_open,
                is_fullscreen,
            );
            header = Some(hb);
        }
        if self.sidebar_dirty {
            self.sidebar_dirty = false;
            if lay.sidebar_w > 0 || self.sidebar_open() || !self.sidebar_ready {
                self.sidebar_ready = true;
                let sw = lay.sidebar_full as u32;
                let sh = (h as i32 - lay.side_oy()).max(1) as u32;
                let mut sb = vec![0u32; (sw * sh) as usize];
                draw_sidebar(
                    &mut Canvas {
                        px: &mut sb,
                        w: sw,
                        h: sh,
                        ox: w as i32 - lay.sidebar_full,
                        oy: lay.side_oy(),
                        clip: None,
                    },
                    &self.font,
                    &lay,
                    self.info.as_ref(),
                    &self.fades,
                );
                sidebar = Some((sb, sw, sh));
            }
        }

        let mut menu_buf = None;
        if self.menu_dirty {
            self.menu_dirty = false;
            if let Some(menu) = &self.menu {
                let (bx, by, bw, bh) = menu.bounds();
                let (mw, mh) = (bw as u32, bh as u32);
                let mut mb = vec![0u32; (mw * mh) as usize];
                menu.draw(
                    &mut Canvas {
                        px: &mut mb,
                        w: mw,
                        h: mh,
                        ox: bx,
                        oy: by,
                        clip: None,
                    },
                    &self.font,
                );
                menu_buf = Some((mb, mw, mh));
            }
        }

        let mut toast_buf = None;
        if let Some(t) = &mut self.toast
            && t.dirty {
                t.dirty = false;
                let s = lay.scale;
                let px = 14.0 * s;
                let max_w = (vw as f32 - 80.0 * s).max(40.0 * s);
                let text = ellipsize(&self.font, &t.text, px, max_w);
                let tw = text_width(&self.font, &text, px);
                let pad = (SHADOW_PAD * s).round();
                let (bw, bh) = (
                    (tw + 36.0 * s + 2.0 * pad).ceil() as u32,
                    (38.0 * s + 2.0 * pad).ceil() as u32,
                );
                let mut tb = vec![0u32; (bw * bh) as usize];
                {
                    let mut cv = Canvas {
                        px: &mut tb,
                        w: bw,
                        h: bh,
                        ox: 0,
                        oy: 0,
                        clip: None,
                    };
                    let (fw, fh) = (bw as f32 - 2.0 * pad, bh as f32 - 2.0 * pad);
                    surface(&mut cv, pad, pad, fw, fh, R_PILL * s, s);
                    if let Some(lm) = self.font.horizontal_line_metrics(px) {
                        let baseline = pad + fh / 2.0 + (lm.ascent + lm.descent) / 2.0;
                        cv.text(
                            &self.font,
                            &text,
                            pad + (fw - tw) / 2.0,
                            baseline,
                            px,
                            theme::current().text,
                        );
                    }
                }
                toast_buf = Some((tb, bw, bh));
            }

        let crop_scr = match &self.crop {
            Some(c) => self.crop_screen(&lay, c.rect),
            None => None,
        };
        if let Some(c) = &mut self.crop {
            let rw = (c.rect[2] - c.rect[0]).round().max(1.0) as u32;
            let rh = (c.rect[3] - c.rect[1]).round().max(1.0) as u32;
            let label = format!("{rw} × {rh}");
            if c.bar.label != label {
                c.bar.label = label;
                c.bar_dirty = true;
            }
            if c.bar.editing.is_none() {
                c.bar.buf_w = rw.to_string();
                c.bar.buf_h = rh.to_string();
            }
        }

        let mut bar_buf = None;
        if let Some(c) = &mut self.crop
            && c.bar_dirty {
                c.bar_dirty = false;
                let pad = c.bar.pad();
                let (bw, bh) = ((c.bar.w + 2 * pad) as u32, (c.bar.h + 2 * pad) as u32);
                let mut bb = vec![0u32; (bw * bh) as usize];
                c.bar.draw(
                    &mut Canvas {
                        px: &mut bb,
                        w: bw,
                        h: bh,
                        ox: -pad,
                        oy: -pad,
                        clip: None,
                    },
                    &self.font,
                );
                bar_buf = Some((bb, bw, bh));
            }

        let mut dialog_buf = None;
        if self.dialog_dirty {
            self.dialog_dirty = false;
            if let Some(d) = &self.dialog {
                let pad = d.pad();
                let (dw, dh) = ((d.w + 2 * pad) as u32, (d.h + 2 * pad) as u32);
                let mut db = vec![0u32; (dw * dh) as usize];
                d.draw(
                    &mut Canvas {
                        px: &mut db,
                        w: dw,
                        h: dh,
                        ox: -pad,
                        oy: -pad,
                        clip: None,
                    },
                    &self.font,
                );
                dialog_buf = Some((db, dw, dh));
            }
        }

        let mut ratio_upload = None;
        let mut ratio_rect = None;
        if self.crop.is_some() {
            let (px, py, pw, ph, _) = lay.ratio_panel();
            let pad = ((SHADOW_PAD + 2.0) * lay.scale).ceil();
            let (rw, rh) = (
                (pw + 2.0 * pad).ceil() as u32,
                (ph + 2.0 * pad).ceil() as u32,
            );
            let (rx, ry) = ((px - pad).floor() as i32, (py - pad).floor() as i32);
            ratio_rect = Some((rw, rh, rx, ry));
            if self.ratio_dirty {
                self.ratio_dirty = false;
                let mut rb = vec![0u32; (rw * rh) as usize];
                self.draw_ratios(
                    &mut Canvas {
                        px: &mut rb,
                        w: rw,
                        h: rh,
                        ox: rx,
                        oy: ry,
                        clip: None,
                    },
                    &lay,
                );
                ratio_upload = Some((rb, rw, rh));
            }
        }

        let mut empty_upload = None;
        let mut empty_pos = None;
        if self.image.is_none() {
            let sc = lay.scale;
            let (cx, y0) = lay.start_origin();
            let (ew, eh) = ((START_W * sc).round() as u32, (START_H * sc).round() as u32);
            let (ex, ey) = ((cx - ew as f32 / 2.0).round() as i32, y0.round() as i32);
            empty_pos = Some((ex, ey));
            if self.empty_dirty {
                self.empty_dirty = false;
                let lp = (LOGO_S * sc).round().max(1.0) as u32;
                if self.logo.as_ref().map(|l| l.0) != Some(lp) {
                    self.logo = Some((lp, logo_pixels(lp)));
                }
                let mut eb = vec![0u32; (ew * eh) as usize];
                if let Some((_, px)) = &self.logo {
                    let n = lp.min(ew) as usize;
                    let lx = (ew.saturating_sub(lp) / 2) as usize;
                    for row in 0..lp.min(eh) as usize {
                        let src = row * lp as usize;
                        let dst = row * ew as usize + lx;
                        if px.len() >= src + n {
                            eb[dst..dst + n].copy_from_slice(&px[src..src + n]);
                        }
                    }
                }
                draw_start(
                    &mut Canvas {
                        px: &mut eb,
                        w: ew,
                        h: eh,
                        ox: ex,
                        oy: ey,
                        clip: None,
                    },
                    &self.font,
                    &lay,
                    self.fades[Hit::Start.index()].v,
                    self.fades[Hit::StartFolder.index()].v,
                );
                empty_upload = Some((eb, ew, eh));
            }
        }

        let Some(gpu) = self.gpu.as_mut() else { return };
        if let Some(anim) = &mut self.anim
            && let Some(frame) = anim.take_dirty() {
                gpu.set_image(frame);
            }
        if let Some(hb) = &header {
            gpu.set_overlay(gpu::HEADER, hb, w, hh);
        }
        if let Some((sb, sw, sh)) = &sidebar {
            gpu.set_overlay(gpu::SIDEBAR, sb, *sw, *sh);
        }
        if let Some((mb, mw, mh)) = &menu_buf {
            gpu.set_overlay(gpu::MENU, mb, *mw, *mh);
        }
        if let Some((tb, bw, bh)) = &toast_buf {
            gpu.set_overlay(gpu::TOAST, tb, *bw, *bh);
        }
        if let Some((bb, bw, bh)) = &bar_buf {
            gpu.set_overlay(gpu::BAR, bb, *bw, *bh);
        }
        if let Some((db, dw, dh)) = &dialog_buf {
            gpu.set_overlay(gpu::DIALOG, db, *dw, *dh);
        }
        if let Some((eb, ew, eh)) = &empty_upload {
            gpu.set_overlay(gpu::EMPTY, eb, *ew, *eh);
        }
        if let Some((rb, rw, rh)) = &ratio_upload {
            gpu.set_overlay(gpu::RATIOS, rb, *rw, *rh);
        }

        let square = self
            .window
            .as_ref()
            .is_none_or(|win| win.is_maximized() || win.fullscreen().is_some());
        gpu.begin(w, h, if square { 0.0 } else { R_WINDOW * lay.scale });
        gpu.draw_overlay_scaled(
            gpu::BG,
            (0.0, 0.0, w as f32, h as f32),
            self.config.opacity as f32 / 100.0,
        );

        if let Some(img) = &self.image {
            let dims = img.dimensions();
            let k = self.view.cur_scale * (0.96 + 0.04 * self.appear.v);
            let (dw, dh) = (dims.0 as f32 * k, dims.1 as f32 * k);
            let cx = vx as f32 + vw as f32 / 2.0 + self.view.cur_pan.0;
            let cy = vy as f32 + vh as f32 / 2.0 + self.view.cur_pan.1;
            let (mut rx, mut ry, mut rw, mut rh) = (cx - dw / 2.0, cy - dh / 2.0, dw, dh);
            if !image_moving {
                rx = rx.round();
                ry = ry.round();
                rw = rw.round();
                rh = rh.round();
            }
            gpu.draw_image(
                (rx, ry, rw, rh),
                (vx, vy, vw, vh),
                self.appear.v,
                self.angle.v,
                self.src.map(|s| s.has_alpha).unwrap_or(false),
                theme::current().is_light,
            );
        }
        if let Some((c, im)) = crop_scr {
            let crop_alpha = self.crop.as_ref().map(|cr| cr.appear.v).unwrap_or(1.0);
            draw_crop(gpu, lay.scale, c, im, (vx, vy, vw, vh), crop_alpha);
        }

        if let Some((ex, ey)) = empty_pos {
            gpu.draw_overlay(gpu::EMPTY, ex, ey, 1.0);
        }
        let h_alpha = 1.0 - self.header_hide.v;
        if lay.sidebar_w > 0 && h_alpha > 0.001 {
            let s_shift = (self.header_hide.v * lay.sidebar_w as f32).round() as i32;
            gpu.draw_overlay(
                gpu::SIDEBAR,
                w as i32 - lay.sidebar_w + s_shift,
                lay.side_oy(),
                h_alpha,
            );
        }
        if h_alpha > 0.001 {
            let h_lift = -((self.header_hide.v * lay.top_h as f32).round() as i32);
            gpu.draw_overlay(gpu::HEADER, 0, h_lift, h_alpha);
        }
        let drag_alpha = 1.0 - self.drag_hide.v;
        if let (Some((rw, _, rx, ry)), Some(c)) = (ratio_rect, &self.crop) {
            let t = 1.0 - c.appear.v;
            let travel = (vx + vw as i32 - rx) as f32;
            let shift = (t * travel.max(rw as f32) + self.drag_hide.v * travel.max(rw as f32))
                .round() as i32;
            let a = c.appear.v * drag_alpha;
            if a > 0.001 {
                gpu.draw_overlay(gpu::RATIOS, rx + shift, ry, a);
            }
        }
        if let Some(c) = &self.crop {
            let (bx, by) = c.bar.origin((vx, vy, vw, vh));
            let pad = c.bar.pad();
            let lift =
                (((1.0 - c.appear.v) * 18.0 + self.drag_hide.v * 36.0) * lay.scale).round() as i32;
            let a = c.appear.v * drag_alpha;
            if a > 0.001 {
                gpu.draw_overlay(gpu::BAR, bx - pad, by - pad + lift, a);
            }
        }
        if let Some(menu) = &self.menu {
            let (bx, by, ..) = menu.bounds();
            let lift = ((1.0 - menu.appear.v) * 6.0 * lay.scale) as i32;
            gpu.draw_overlay(gpu::MENU, bx, by + lift, menu.appear.v);
        }
        if let Some(t) = &self.toast
            && let Some((bw, bh)) = gpu.overlay_size(gpu::TOAST) {
                let x = vx + (vw as i32 - bw) / 2;
                let lift = if self.crop.as_ref().is_some_and(|c| !c.canceling) {
                    (56.0 * lay.scale) as i32
                } else {
                    0
                };
                let y = vy + vh as i32 - bh - (28.0 * lay.scale) as i32 - lift
                    + lay.pad()
                    + ((1.0 - t.appear.v) * 8.0 * lay.scale) as i32;
                gpu.draw_overlay(gpu::TOAST, x, y, t.appear.v);
            }
        if let Some(d) = &self.dialog {
            gpu.draw_overlay_scaled(gpu::BLACK, (0.0, 0.0, w as f32, h as f32), 0.5);
            let (dx, dy) = d.origin(lay.win);
            let pad = d.pad();
            gpu.draw_overlay(gpu::DIALOG, dx - pad, dy - pad, 1.0);
        }
        gpu.present();

        self.anim_active = busy;
        if now.elapsed() > Duration::from_millis(30) {
            self.last_frame = Instant::now() - Duration::from_millis(16);
        }
        if busy {
            self.redraw();
        }
    }
}

impl ApplicationHandler<UserEvent> for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.gpu.is_some() {
            return;
        }
        let title = self
            .path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Mithril".into());

        let attrs = Window::default_attributes()
            .with_title(title)
            .with_decorations(false)
            .with_transparent(true)
            .with_inner_size(LogicalSize::new(1000.0, 720.0))
            .with_min_inner_size(LogicalSize::new(420.0, 280.0))
            .with_window_icon(window_icon());
        #[cfg(target_os = "linux")]
        let attrs = {
            use winit::platform::{
                wayland::WindowAttributesExtWayland, x11::WindowAttributesExtX11,
            };
            let attrs = WindowAttributesExtWayland::with_name(attrs, APP_ID, APP_ID);
            WindowAttributesExtX11::with_name(attrs, APP_ID, APP_ID)
        };

        match Gpu::new(el, attrs) {
            Ok((window, mut gpu)) => {
                gpu.set_overlay(gpu::BG, &theme::bg_texture(), 1, 1);
                gpu.set_overlay(gpu::BLACK, &[0xff00_0000], 1, 1);
                gpu.set_overlay(gpu::WHITE, &[0xffff_ffff], 1, 1);
                if let Some(img) = &self.image {
                    gpu.set_image(img);
                }
                window.request_redraw();
                self.window = Some(window);
                self.gpu = Some(gpu);
            }
            Err(e) => {
                eprintln!("mithril: failed to launch OpenGL: {e}");
                el.exit();
            }
        }
    }

    fn user_event(&mut self, _el: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::Picked(path) => {
                self.picking = false;
                if let Some(path) = path {
                    self.open_external(path);
                }
            }
            UserEvent::PickedFolder(dir) => {
                self.picking = false;
                if let Some(dir) = dir {
                    self.open_dir(dir);
                }
            }
            UserEvent::Printed(res) => {
                self.printing = false;
                match res {
                    Ok(true) => self.show_toast(tr("toast.print_sent").to_string()),
                    Ok(false) => {}
                    Err(e) => {
                        let short: String =
                            e.lines().next().unwrap_or("").chars().take(60).collect();
                        let msg = if short.is_empty() {
                            tr("toast.print_failed").to_string()
                        } else {
                            format!("{}: {short}", tr("toast.print_failed"))
                        };
                        self.show_toast(msg);
                    }
                }
            }
            UserEvent::Prefetched(job_gen, path, loaded) => {
                self.prefetcher.insert(job_gen, path, *loaded);
            }
            UserEvent::Wallpaper(res) => match res {
                Ok(()) => self.show_toast(tr("toast.wallpaper_set").to_string()),
                Err(e) => {
                    let msg = format!("{}: {e}", tr("toast.wallpaper_failed"));
                    self.show_toast(msg);
                }
            },
        }
    }

    fn exiting(&mut self, _el: &ActiveEventLoop) {
        self.prefetcher.shutdown();
        self.gpu = None;
        self.window = None;
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        if self.exit_requested {
            el.exit();
            return;
        }
        let Some(anim) = self.anim.as_mut() else {
            el.set_control_flow(ControlFlow::Wait);
            return;
        };
        let changed = anim.tick(Instant::now());
        let next = anim.next_at();
        if changed {
            self.redraw();
        }
        el.set_control_flow(ControlFlow::WaitUntil(next));
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => self.request_exit(el),
            WindowEvent::ModifiersChanged(m) => {
                self.shift = m.state().shift_key();
                self.ctrl = m.state().control_key();
            }
            WindowEvent::KeyboardInput { event, .. } if event.state.is_pressed() => {
                if let Some(c) = &mut self.crop
                    && let Some(field) = c.bar.editing
                        && let Some(txt) = event.text.as_ref() {
                            let digits: String =
                                txt.chars().filter(|ch| ch.is_ascii_digit()).collect();
                            if !digits.is_empty() {
                                let buf = match field {
                                    CropField::W => &mut c.bar.buf_w,
                                    CropField::H => &mut c.bar.buf_h,
                                };
                                if buf.len() < 5 {
                                    buf.push_str(&digits);
                                    c.bar_dirty = true;
                                    self.redraw();
                                    return;
                                }
                            }
                        }
                if event.repeat && !is_nav_key(event.physical_key) {
                    return;
                }
                if self.dialog.is_some() {
                    match event.physical_key {
                        PhysicalKey::Code(KeyCode::Escape) => self.close_dialog(),
                        PhysicalKey::Code(KeyCode::Enter | KeyCode::NumpadEnter) => {
                            self.dialog_action(DialogAction::Save, el)
                        }
                        _ => {}
                    }
                    return;
                }
                match event.physical_key {
                    PhysicalKey::Code(KeyCode::Escape) => {
                        if let Some(c) = &mut self.crop
                            && c.bar.editing.is_some() {
                                c.bar.editing = None;
                                c.bar_dirty = true;
                                self.redraw();
                                return;
                            }
                        if self.crop_active() {
                            self.cancel_crop();
                        } else if self.menu.is_some() {
                            self.close_menu();
                        } else if self.window.as_ref().is_some_and(|w| w.fullscreen().is_some()) {
                            self.toggle_fullscreen();
                        } else {
                            self.request_exit(el);
                        }
                    }
                    PhysicalKey::Code(KeyCode::Enter | KeyCode::NumpadEnter) => {
                        if let Some(c) = &mut self.crop
                            && c.bar.editing.is_some() {
                                self.commit_crop_field();
                                return;
                            }
                        if self.crop_active() {
                            self.apply_crop();
                        } else if self.image.is_none() {
                            self.pick_file();
                        }
                    }
                    PhysicalKey::Code(KeyCode::Tab) => {
                        if let Some(c) = &mut self.crop
                            && let Some(field) = c.bar.editing {
                                self.commit_crop_field();
                                if let Some(c) = &mut self.crop {
                                    c.bar.editing = Some(match field {
                                        CropField::W => CropField::H,
                                        CropField::H => CropField::W,
                                    });
                                    c.bar_dirty = true;
                                    self.redraw();
                                }
                            }
                    }
                    PhysicalKey::Code(KeyCode::Backspace) => {
                        if let Some(c) = &mut self.crop {
                            let buf = match c.bar.editing {
                                Some(CropField::W) => &mut c.bar.buf_w,
                                Some(CropField::H) => &mut c.bar.buf_h,
                                None => &mut String::new(),
                            };
                            if !buf.is_empty() {
                                buf.pop();
                                c.bar_dirty = true;
                                self.redraw();
                            }
                        }
                    }
                    PhysicalKey::Code(KeyCode::Digit0) | PhysicalKey::Code(KeyCode::Numpad0) => {
                        self.close_menu();
                        self.apply(Action::ResetZoom);
                    }
                    PhysicalKey::Code(KeyCode::KeyO) if self.ctrl && self.shift => {
                        self.pick_folder()
                    }
                    PhysicalKey::Code(KeyCode::KeyO) if self.ctrl => self.pick_file(),
                    PhysicalKey::Code(KeyCode::KeyR) if self.ctrl => {
                        self.close_menu();
                        self.flip(!self.shift);
                    }
                    PhysicalKey::Code(KeyCode::KeyR) if !self.ctrl => {
                        self.close_menu();
                        self.rotate(!self.shift);
                    }
                    PhysicalKey::Code(KeyCode::KeyC) if self.ctrl => {
                        self.close_menu();
                        self.copy_image();
                    }
                    PhysicalKey::Code(KeyCode::KeyC) => {
                        self.close_menu();
                        self.start_crop();
                    }
                    PhysicalKey::Code(KeyCode::KeyP) if self.ctrl => {
                        self.close_menu();
                        self.print();
                    }
                    PhysicalKey::Code(KeyCode::Delete) => {
                        self.close_menu();
                        self.request_delete();
                    }
                    PhysicalKey::Code(KeyCode::ArrowRight | KeyCode::PageDown) => {
                        self.nav_key(1, event.repeat);
                    }
                    PhysicalKey::Code(KeyCode::ArrowLeft | KeyCode::PageUp) => {
                        self.nav_key(-1, event.repeat);
                    }
                    PhysicalKey::Code(KeyCode::KeyI) => self.toggle_sidebar(),
                    PhysicalKey::Code(KeyCode::F11) => {
                        self.close_menu();
                        self.toggle_fullscreen();
                    }
                    _ => {}
                }
            }
            WindowEvent::Resized(size) => {
                if let Some(gpu) = &mut self.gpu {
                    gpu.resize(size.width, size.height);
                }
                self.header_dirty = true;
                self.sidebar_dirty = true;
                self.redraw();
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                if let Some(lay) = self.layout() {
                    if self.dialog.is_some() {
                        self.dialog = Some(Dialog::unsaved(&self.font, lay.scale));
                        self.dialog_dirty = true;
                    }
                    if let Some(c) = &mut self.crop {
                        c.bar = CropBar::new(&self.font, lay.scale);
                        c.bar_dirty = true;
                    }
                }
                self.header_dirty = true;
                self.sidebar_dirty = true;
                self.redraw();
            }
            WindowEvent::MouseWheel { delta, .. } => {
                if self.dialog.is_some() {
                    return;
                }
                if let Some(menu) = &mut self.menu {
                    let steps = match delta {
                        MouseScrollDelta::LineDelta(_, y) => y,
                        MouseScrollDelta::PixelDelta(p) => p.y as f32 / 40.0,
                    };
                    if menu.scroll(steps) {
                        menu.update_cursor(self.cursor, &self.font);
                        self.menu_dirty = true;
                        self.redraw();
                        return;
                    }
                    if menu.contains(self.cursor) {
                        return;
                    }
                    self.close_menu();
                    return;
                }
                let steps = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32 / 40.0,
                };
                self.zoom_at_cursor(steps);
            }
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Right,
                ..
            } if state.is_pressed() => {
                if self.dialog.is_some() || self.crop_active() {
                    return;
                }
                let Some(lay) = self.layout() else { return };
                let (vx, vy, vw, vh) = lay.viewport();
                let (px, py) = self.cursor;
                let inside = px >= vx as f64
                    && px < (vx + vw as i32) as f64
                    && py >= vy as f64
                    && py < (vy + vh as i32) as f64;
                if inside && self.image.is_some() {
                    self.open_menu();
                } else {
                    self.close_menu();
                }
            }
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } => {
                if !state.is_pressed() {
                    self.dragging = false;
                    if let Some(c) = &mut self.crop
                        && c.drag.is_some() {
                            c.drag = None;
                            self.redraw();
                        }
                    return;
                }
                if self.dialog.is_some() {
                    let action = self
                        .layout()
                        .and_then(|lay| self.dialog_local(&lay))
                        .and_then(|p| self.dialog.as_ref().and_then(|d| d.action_at(p)));
                    if let Some(a) = action {
                        self.dialog_action(a, el);
                    }
                    return;
                }
                if let Some(menu) = &mut self.menu {
                    if let Some(a) = menu.action_under(self.cursor) {
                        self.close_menu();
                        self.apply(a);
                        return;
                    }
                    if menu.handle_click(self.cursor, &self.font) {
                        self.menu_dirty = true;
                        self.redraw();
                        return;
                    }
                    if menu.contains(self.cursor) {
                        return;
                    }
                    self.close_menu();
                    return;
                }
                let (Some(window), Some(lay)) = (self.window.clone(), self.layout()) else {
                    return;
                };
                if !window.is_maximized()
                    && window.fullscreen().is_none()
                    && let Some(dir) = edge_dir(self.cursor, lay.win, (EDGE * lay.scale) as f64) {
                        let _ = window.drag_resize_window(dir);
                        return;
                    }
                if let Some(p) = self.bar_local(&lay) {
                    let action = self.crop.as_ref().and_then(|c| c.bar.action_at(p));
                    match action {
                        Some(BarAction::Cancel) => {
                            self.cancel_crop();
                            return;
                        }
                        Some(BarAction::Apply) => {
                            self.commit_crop_field();
                            self.apply_crop();
                            return;
                        }
                        Some(BarAction::EditW) => {
                            if let Some(c) = &mut self.crop {
                                let cur_w = (c.rect[2] - c.rect[0]).round() as u32;
                                c.bar.buf_w = cur_w.to_string();
                                let cur_h = (c.rect[3] - c.rect[1]).round() as u32;
                                c.bar.buf_h = cur_h.to_string();
                                c.bar.editing = Some(CropField::W);
                                c.bar_dirty = true;
                                self.redraw();
                            }
                            return;
                        }
                        Some(BarAction::EditH) => {
                            if let Some(c) = &mut self.crop {
                                let cur_w = (c.rect[2] - c.rect[0]).round() as u32;
                                c.bar.buf_w = cur_w.to_string();
                                let cur_h = (c.rect[3] - c.rect[1]).round() as u32;
                                c.bar.buf_h = cur_h.to_string();
                                c.bar.editing = Some(CropField::H);
                                c.bar_dirty = true;
                                self.redraw();
                            }
                            return;
                        }
                        None => {
                            if let Some(c) = &mut self.crop
                                && c.bar.editing.is_some() {
                                    self.commit_crop_field();
                                }
                        }
                    }
                    let on_bar = self.crop.as_ref().is_some_and(|c| {
                        p.0 >= 0.0 && p.1 >= 0.0 && p.0 < c.bar.w as f32 && p.1 < c.bar.h as f32
                    });
                    if on_bar {
                        return;
                    }
                }
                if self.crop_active() {
                    if let Some(i) = lay.ratio_at(self.cursor) {
                        self.set_crop_preset(i);
                        return;
                    }
                    if lay.on_ratio_panel(self.cursor) {
                        return;
                    }
                }
                if self.crop_active() && lay.hit(self.cursor).is_none() && self.crop_press(&lay) {
                    return;
                }
                match lay.hit(self.cursor) {
                    Some(Hit::Close) => self.request_exit(el),
                    Some(Hit::Fullscreen) => self.toggle_fullscreen(),
                    Some(Hit::Info) => self.toggle_sidebar(),
                    Some(Hit::More) => self.open_settings(),
                    Some(Hit::OpenFolder) => self.open_folder(),
                    Some(Hit::Open | Hit::StartFolder) => self.pick_folder(),
                    Some(Hit::Start) => self.pick_file(),
                    None if self.cursor.1 < lay.top_h as f64 && self.window.as_ref().is_some_and(|w| w.fullscreen().is_none()) => {
                        let _ = window.drag_window();
                    }
                    None if self.cursor.0 < (lay.win.0 as i32 - lay.sidebar_w) as f64 => {
                        self.dragging = true;
                    }
                    None => {}
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                let new = (position.x, position.y);
                if self.dragging && self.image.is_some() {
                    let dx = (new.0 - self.cursor.0) as f32;
                    let dy = (new.1 - self.cursor.1) as f32;
                    self.view.pan.0 += dx;
                    self.view.pan.1 += dy;
                    self.view.cur_pan.0 += dx;
                    self.view.cur_pan.1 += dy;
                    self.redraw();
                }
                self.cursor = new;
                self.update_crop_drag();
                self.update_modal_hover();

                if let Some(menu) = &mut self.menu
                    && menu.update_cursor(new, &self.font) {
                        self.menu_dirty = true;
                        self.redraw();
                    }

                self.update_pointer();
            }
            WindowEvent::CursorLeft { .. } | WindowEvent::Focused(false) => {
                self.dragging = false;
                if let Some(c) = &mut self.crop
                    && c.drag.is_some() {
                        c.drag = None;
                        self.redraw();
                    }
            }
            WindowEvent::DroppedFile(path) => self.open_external(path),
            WindowEvent::RedrawRequested => self.render(),
            _ => {}
        }
    }
}

fn main() {
    let font = Fonts::new(
        Font::from_bytes(
            &include_bytes!("../assets/InterVariable.ttf")[..],
            FontSettings::default(),
        )
        .expect("failed to load font"),
    );

    let config = config::load();
    theme::init(config.theme);
    i18n::init(config.language.as_deref());

    let event_loop = EventLoop::<UserEvent>::with_user_event().build().unwrap();
    let mut app = App::new(font, config, event_loop.create_proxy());

    if let Some(path) = std::env::args_os().nth(1).map(PathBuf::from) {
        match loader::load(&path) {
            Ok(l) => {
                let dims = l.image.dimensions();
                app.src = Some(Source {
                    dims,
                    format: l.format,
                    has_alpha: l.has_alpha,
                });
                app.info = Some(Info::gather(&path, dims, l.format, l.has_alpha));
                app.image = Some(l.image);
                app.anim = l.anim;
                app.path = Some(path);
                app.rescan();
            }
            Err(e) => eprintln!("mithril: {}: {e}", path.display()),
        }
    }

    event_loop.run_app(&mut app).unwrap();
}

fn run_system_print(path: &std::path::Path, title: &str) -> Result<bool, String> {
    const SCRIPT: &str = r#"
import sys
try:
    import gi
    gi.require_version('Gtk', '3.0')
    from gi.repository import Gtk, Gdk, GdkPixbuf
except Exception:
    sys.exit(10)

path = sys.argv[1]
title = sys.argv[2] if len(sys.argv) > 2 else 'Mithril'

try:
    pixbuf = GdkPixbuf.Pixbuf.new_from_file(path)
except Exception as e:
    sys.stderr.write(f"{e}\n")
    sys.exit(1)

img_w, img_h = pixbuf.get_width(), pixbuf.get_height()
op = Gtk.PrintOperation()
op.set_job_name(title)
op.set_n_pages(1)
op.set_unit(Gtk.Unit.POINTS)
op.set_embed_page_setup(True)

def on_draw_page(operation, context, page_nr):
    cr = context.get_cairo_context()
    pw = context.get_width()
    ph = context.get_height()
    scale = min(pw / img_w, ph / img_h)
    cr.translate((pw - img_w * scale) / 2.0, (ph - img_h * scale) / 2.0)
    cr.scale(scale, scale)
    Gdk.cairo_set_source_pixbuf(cr, pixbuf, 0, 0)
    cr.paint()

op.connect('draw_page', on_draw_page)
res = op.run(Gtk.PrintOperationAction.PRINT_DIALOG, None)

if res == Gtk.PrintOperationResult.APPLY:
    sys.exit(0)
elif res == Gtk.PrintOperationResult.CANCEL:
    sys.exit(2)
else:
    sys.exit(1)
"#;

    if let Ok(out) = std::process::Command::new("python3")
        .arg("-c")
        .arg(SCRIPT)
        .arg(path)
        .arg(title)
        .output()
    {
        match out.status.code() {
            Some(0) => return Ok(true),
            Some(2) => return Ok(false),
            Some(10) => {}
            _ => {
                let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
                if !err.is_empty() {
                    return Err(err);
                }
            }
        }
    }

    if let Ok(out) = std::process::Command::new("gtklp").arg(path).output() {
        if out.status.success() {
            return Ok(true);
        }
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        if !err.is_empty() {
            return Err(err);
        }
        return Ok(false);
    }

    let out = std::process::Command::new("lp")
        .arg("-o")
        .arg("fit-to-page")
        .arg(path)
        .output()
        .map_err(|e| e.to_string())?;

    if out.status.success() {
        Ok(true)
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}
