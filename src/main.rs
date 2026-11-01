mod anim;
mod config;
mod draw;
mod gpu;
mod i18n;
mod info;
mod loader;
mod menu;

use std::{
    borrow::Cow,
    f32::consts::FRAC_PI_2,
    path::PathBuf,
    rc::Rc,
    time::{Duration, Instant},
};

use fontdue::{Font, FontSettings};
use image::{ImageFormat, RgbaImage, imageops};
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::{MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::{CursorIcon, ResizeDirection, Window, WindowId},
};

use anim::Anim;
use config::{Config, DeleteMode};
use draw::{Canvas, Rgb, ellipsize, lerp, text_width};
use gpu::Gpu;
use i18n::tr;
use info::Info;
use menu::{Action, Menu};

const BG: Rgb = [0x2b, 0x2d, 0x31];
const SURF_TOP: Rgb = [0x3b, 0x3e, 0x44];
const SURF_BOT: Rgb = [0x36, 0x39, 0x3f];
const EDGE_TOP: Rgb = [0x4b, 0x4f, 0x57];
const EDGE_BOT: Rgb = [0x46, 0x4a, 0x51];
const BORDER: Rgb = [0x4b, 0x4f, 0x57];
const HOVER: Rgb = [0x4a, 0x4e, 0x57];
const ACTIVE: Rgb = [0x56, 0x5e, 0x73];
const TEXT: Rgb = [0xee, 0xef, 0xf2];
const DIM: Rgb = [0xa3, 0xa8, 0xb1];
const ACCENT: Rgb = [0x8a, 0xa8, 0xff];
const DANGER: Rgb = [0xd9, 0x4f, 0x4f];

// Скругления
const R_PILL: f32 = 9.0;
const R_PANEL: f32 = 10.0;
const R_MENU: f32 = 9.0;
const R_ITEM: f32 = 6.0;
const R_BTN: f32 = 6.0;

// Тени
const SHADOW_PAD: f32 = 10.0; // запас вокруг элемента в текстуре под тень
const SHADOW_BLUR: f32 = 8.0;

// Плавающие элементы поверх фото
const MARGIN: f32 = 12.0;
const PILL_H: f32 = 40.0;
const TOP_H: f32 = 76.0; // полоса сверху: за неё таскаем окно
const BTN_S: f32 = 30.0;
const BTN_GAP: f32 = 6.0;
const CAP_PAD: f32 = 5.0;

// Панель свойств
const SIDEBAR_W: f32 = 332.0; // место под панель (панель плавает внутри него)
const PANEL_L: f32 = 16.0;
const PANEL_R: f32 = 16.0;
const PANEL_B: f32 = 16.0;
const PAD: f32 = 14.0;
const GROUP_GAP: f32 = 10.0;
const ROW_H: f32 = 50.0;
const CARD_ROWS: [usize; 4] = [1, 1, 3, 2];
const EDGE: f32 = 5.0;

/// Плавающая поверхность: тень, светлая кромка сверху и лёгкий градиент заливки.
fn surface(cv: &mut Canvas, x: f32, y: f32, w: f32, h: f32, r: f32, s: f32) {
    cv.shadow(x, y + 2.0 * s, w, h, r, SHADOW_BLUR * s, 0.22);
    cv.round_rect_grad(x, y, w, h, r, EDGE_TOP, EDGE_BOT);
    cv.round_rect_grad(
        x + 1.0,
        y + 1.0,
        w - 2.0,
        h - 2.0,
        r - 1.0,
        SURF_TOP,
        SURF_BOT,
    );
}

/// Фон окна: радиальный градиент с виньеткой (маленькая текстура, растягивается на всё окно).
fn bg_texture() -> Vec<u32> {
    const N: usize = 64;
    let (center, edge): (Rgb, Rgb) = ([0x37, 0x3a, 0x40], [0x2a, 0x2c, 0x30]);
    let mut v = Vec::with_capacity(N * N);
    for y in 0..N {
        for x in 0..N {
            let nx = (x as f32 + 0.5) / N as f32 * 2.0 - 1.0;
            let ny = (y as f32 + 0.5) / N as f32 * 2.0 - 1.0;
            let t = ((nx * nx + ny * ny).sqrt() / 1.25).clamp(0.0, 1.0);
            let c = lerp(center, edge, t * t * (3.0 - 2.0 * t));
            v.push(0xff00_0000 | (c[0] as u32) << 16 | (c[1] as u32) << 8 | c[2] as u32);
        }
    }
    v
}

#[derive(Clone, Copy)]
struct Fade {
    v: f32,
    to: f32,
}

impl Fade {
    fn new(v: f32) -> Self {
        Self { v, to: v }
    }

    fn step(&mut self, dt: f32, speed: f32) -> bool {
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

#[derive(Clone, Copy)]
struct Source {
    dims: (u32, u32),
    format: Option<ImageFormat>,
    has_alpha: bool,
}

fn fit_scale((iw, ih): (u32, u32), (w, h): (u32, u32)) -> f32 {
    (w as f32 / iw as f32).min(h as f32 / ih as f32).min(1.0)
}

#[derive(Default)]
struct View {
    zoom: Option<f32>,
    pan: (f32, f32),
    cur_scale: f32,
    cur_pan: (f32, f32),
}

impl View {
    fn scale(&self, img: (u32, u32), vp: (u32, u32)) -> f32 {
        self.zoom.unwrap_or_else(|| fit_scale(img, vp))
    }

    fn reset(&mut self) {
        self.zoom = None;
        self.pan = (0.0, 0.0);
    }

    fn clamp_pan(&mut self, img: (u32, u32), vp: (u32, u32)) {
        let s = self.scale(img, vp);
        let max_x = ((img.0 as f32 * s - vp.0 as f32) / 2.0).max(0.0);
        let max_y = ((img.1 as f32 * s - vp.1 as f32) / 2.0).max(0.0);
        self.pan.0 = self.pan.0.clamp(-max_x, max_x);
        self.pan.1 = self.pan.1.clamp(-max_y, max_y);
    }

    fn step(&mut self, img: (u32, u32), vp: (u32, u32), dt: f32, snap: bool) -> bool {
        let ts = self.scale(img, vp);
        if snap || self.cur_scale <= 0.0 {
            let changed = self.cur_scale != ts || self.cur_pan != self.pan;
            self.cur_scale = ts;
            self.cur_pan = self.pan;
            return changed;
        }
        let a = 1.0 - (-dt * 18.0).exp();
        let mut moving = false;

        let ds = ts - self.cur_scale;
        if ds != 0.0 {
            moving = true;
            if ds.abs() < ts * 0.0005 {
                self.cur_scale = ts;
            } else {
                self.cur_scale += ds * a;
            }
        }
        let dx = self.pan.0 - self.cur_pan.0;
        if dx != 0.0 {
            moving = true;
            if dx.abs() < 0.25 {
                self.cur_pan.0 = self.pan.0;
            } else {
                self.cur_pan.0 += dx * a;
            }
        }
        let dy = self.pan.1 - self.cur_pan.1;
        if dy != 0.0 {
            moving = true;
            if dy.abs() < 0.25 {
                self.cur_pan.1 = self.pan.1;
            } else {
                self.cur_pan.1 += dy * a;
            }
        }
        moving
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Hit {
    Info,
    Close,
    OpenFolder,
    More,
}

impl Hit {
    fn index(self) -> usize {
        self as usize
    }
}

struct Layout {
    scale: f32,
    win: (u32, u32),
    top_h: i32,
    sidebar_w: i32,
    sidebar_full: i32,
}

impl Layout {
    fn new(win: (u32, u32), scale: f32, frac: f32) -> Self {
        let full = (SIDEBAR_W * scale).round() as i32;
        Self {
            scale,
            win,
            top_h: (TOP_H * scale).round() as i32,
            sidebar_w: (full as f32 * frac).round() as i32,
            sidebar_full: full,
        }
    }

    fn pad(&self) -> i32 {
        (SHADOW_PAD * self.scale).round() as i32
    }

    /// Верх текстуры сайдбара: на отступ под тень выше самой панели.
    fn side_oy(&self) -> i32 {
        self.top_h - self.pad()
    }

    /// Фото занимает всё окно (кроме места под панель): шапки-«таблетки» плавают поверх него.
    fn viewport(&self) -> (i32, i32, u32, u32) {
        let w = (self.win.0 as i32 - self.sidebar_w).max(1);
        (0, 0, w as u32, self.win.1.max(1))
    }

    /// Правая капсула с кнопками: (x, y, w, h).
    fn capsule(&self) -> (f32, f32, f32, f32) {
        let s = self.scale;
        let w = (BTN_S * 3.0 + BTN_GAP * 2.0 + CAP_PAD * 2.0) * s;
        (
            self.win.0 as f32 - MARGIN * s - w,
            MARGIN * s,
            w,
            PILL_H * s,
        )
    }

    fn btn_center(&self, i: usize) -> (f32, f32) {
        let (x, y, _, h) = self.capsule();
        let s = self.scale;
        (
            x + (CAP_PAD + BTN_S / 2.0 + i as f32 * (BTN_S + BTN_GAP)) * s,
            y + h / 2.0,
        )
    }

    fn info_center(&self) -> (f32, f32) {
        self.btn_center(0)
    }

    fn more_center(&self) -> (f32, f32) {
        self.btn_center(1)
    }

    fn close_center(&self) -> (f32, f32) {
        self.btn_center(2)
    }

    /// Максимальная ширина левой «таблетки» с именем файла.
    fn title_max_w(&self) -> f32 {
        let (cx, ..) = self.capsule();
        (cx - 2.0 * MARGIN * self.scale).max(0.0)
    }

    fn panel_x(&self) -> f32 {
        (self.win.0 as i32 - self.sidebar_full) as f32 + PANEL_L * self.scale
    }

    fn panel_w(&self) -> f32 {
        self.sidebar_full as f32 - (PANEL_L + PANEL_R) * self.scale
    }

    /// Панель по высоте повторяет содержимое, но не вылезает за окно.
    fn panel_h(&self) -> f32 {
        let rows = CARD_ROWS.iter().sum::<usize>() as f32;
        let groups = (CARD_ROWS.len() - 1) as f32;
        let content = (2.0 * PAD + groups * GROUP_GAP + rows * ROW_H) * self.scale;
        let avail = (self.win.1 as i32 - self.top_h) as f32 - PANEL_B * self.scale;
        content.min(avail).max(0.0)
    }

    fn card_x(&self) -> f32 {
        self.panel_x() + PAD * self.scale
    }

    fn card_w(&self) -> f32 {
        self.panel_w() - 2.0 * PAD * self.scale
    }

    fn group_y(&self, n: usize) -> f32 {
        let rows_before: usize = CARD_ROWS[..n].iter().sum();
        self.top_h as f32 + (PAD + n as f32 * GROUP_GAP + rows_before as f32 * ROW_H) * self.scale
    }

    fn folder_btn_center(&self) -> (f32, f32) {
        (
            self.card_x() + self.card_w() - (BTN_S / 2.0 + 2.0) * self.scale,
            self.group_y(1) + ROW_H * self.scale / 2.0,
        )
    }

    fn hit(&self, p: (f64, f64)) -> Option<Hit> {
        let half = (BTN_S / 2.0 * self.scale) as f64;
        let near =
            |c: (f32, f32)| (p.0 - c.0 as f64).abs() <= half && (p.1 - c.1 as f64).abs() <= half;
        if near(self.close_center()) {
            Some(Hit::Close)
        } else if near(self.more_center()) {
            Some(Hit::More)
        } else if near(self.info_center()) {
            Some(Hit::Info)
        } else if self.sidebar_w == self.sidebar_full && near(self.folder_btn_center()) {
            Some(Hit::OpenFolder)
        } else {
            None
        }
    }
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

#[allow(clippy::too_many_arguments)]
fn draw_row(
    cv: &mut Canvas,
    font: &Font,
    x: f32,
    y: f32,
    w: f32,
    s: f32,
    label: &str,
    value: &str,
    right_pad: f32,
) {
    cv.text(font, label, x, y + 20.0 * s, 11.5 * s, DIM);
    let vpx = 14.0 * s;
    let max_w = (w - right_pad).max(0.0);
    let v = ellipsize(font, value, vpx, max_w);
    cv.text(font, &v, x, y + 38.0 * s, vpx, TEXT);
}

struct App {
    font: Font,
    config: Config,
    window: Option<Rc<Window>>,
    gpu: Option<Gpu>,
    path: Option<PathBuf>,
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
    hover: Option<Hit>,
    fades: [Fade; 4],
    appear: Fade,
    angle: Fade,
    cursor_icon: CursorIcon,
    menu: Option<Menu>,
    settings_open: bool,
    toast: Option<Toast>,
    clipboard: Option<arboard::Clipboard>,
    pending_delete: Option<Instant>,
    exit_requested: bool,
    header_dirty: bool,
    sidebar_dirty: bool,
    menu_dirty: bool,
    last_frame: Instant,
    last_vp: (u32, u32),
}

impl App {
    fn new(font: Font, config: Config) -> Self {
        Self {
            font,
            config,
            window: None,
            gpu: None,
            path: None,
            src: None,
            info: None,
            image: None,
            anim: None,
            view: View::default(),
            cursor: (0.0, 0.0),
            dragging: false,
            shift: false,
            ctrl: false,
            side: Fade::new(0.0),
            hover: None,
            fades: [Fade::new(0.0); 4],
            appear: Fade { v: 0.0, to: 1.0 },
            angle: Fade::new(0.0),
            cursor_icon: CursorIcon::Default,
            menu: None,
            settings_open: false,
            toast: None,
            clipboard: None,
            pending_delete: None,
            exit_requested: false,
            header_dirty: true,
            sidebar_dirty: true,
            menu_dirty: false,
            last_frame: Instant::now(),
            last_vp: (0, 0),
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

    fn layout(&self) -> Option<Layout> {
        let w = self.window.as_ref()?;
        let size = w.inner_size();
        Some(Layout::new(
            (size.width, size.height),
            w.scale_factor() as f32,
            self.side.v,
        ))
    }

    fn refresh_info(&mut self) {
        if let (Some(path), Some(src)) = (&self.path, self.src) {
            self.info = Some(Info::gather(path, src.dims, src.format, src.has_alpha));
        }
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
        let entries = menu::context_entries(self.config.delete_mode);
        let anchor = (self.cursor.0 as i32, self.cursor.1 as i32);
        let mut menu = Menu::new(entries, anchor, lay.win, lay.scale, false);
        menu.set_hover(menu.item_at(self.cursor));
        self.menu = Some(menu);
        self.menu_dirty = true;
        self.redraw();
    }

    fn open_settings(&mut self) {
        let Some(lay) = self.layout() else { return };
        let entries =
            menu::settings_entries(self.config.language.as_deref(), self.config.delete_mode);
        let (cx, cy, cw, ch) = lay.capsule();
        let anchor = ((cx + cw) as i32, (cy + ch + 6.0 * lay.scale) as i32);
        let mut menu = Menu::new(entries, anchor, lay.win, lay.scale, true);
        menu.set_hover(menu.item_at(self.cursor));
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
            Action::Copy => self.copy_image(),
            Action::Delete => self.request_delete(),
            Action::SetLanguage(code) => self.set_language(code),
            Action::SetDeleteMode(mode) => {
                self.config.delete_mode = mode;
                self.pending_delete = None;
                config::save(&self.config);
            }
        }
    }

    fn rotate(&mut self, clockwise: bool) {
        let Some(img) = self.image.take() else { return };
        let rotated = if clockwise {
            imageops::rotate90(&img)
        } else {
            imageops::rotate270(&img)
        };
        if let Some(anim) = &mut self.anim {
            anim.rotate(clockwise);
        } else if let Some(gpu) = &mut self.gpu {
            gpu.set_image(&rotated);
        }
        self.image = Some(rotated);
        self.view.reset();
        self.angle = Fade {
            v: if clockwise { -FRAC_PI_2 } else { FRAC_PI_2 },
            to: 0.0,
        };
        self.redraw();
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
        if self.config.delete_mode == DeleteMode::Permanent {
            let confirmed = self
                .pending_delete
                .is_some_and(|t| t.elapsed() < Duration::from_secs(4));
            if !confirmed {
                self.pending_delete = Some(Instant::now());
                self.show_toast(tr("toast.confirm_delete").to_string());
                return;
            }
        }
        let result = match self.config.delete_mode {
            DeleteMode::Trash => trash::delete(&path).map_err(|e| e.to_string()),
            DeleteMode::Permanent => std::fs::remove_file(&path).map_err(|e| e.to_string()),
        };
        match result {
            Ok(()) => self.exit_requested = true,
            Err(e) => {
                self.pending_delete = None;
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
        let min = fit_scale(imgd, vp);
        let new = (old * 1.25f32.powf(steps)).clamp(min, 32.0);

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

        let hover = lay.hit(self.cursor);
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

        let icon = if window.is_maximized() {
            CursorIcon::Default
        } else {
            edge_dir(self.cursor, lay.win, (EDGE * lay.scale) as f64)
                .map(dir_icon)
                .unwrap_or(CursorIcon::Default)
        };
        if icon != self.cursor_icon {
            self.cursor_icon = icon;
            window.set_cursor(icon);
        }
    }

    fn draw_header(&self, cv: &mut Canvas, lay: &Layout) {
        let s = lay.scale;

        // Левая плашка: только имя файла с маленьким значком.
        if let Some(info) = &self.info {
            let px = 14.0 * s;
            let (pad, icon_w, gap) = (14.0 * s, 16.0 * s, 9.0 * s);
            let max_text = (lay.title_max_w() - 2.0 * pad - icon_w - gap).max(0.0);
            let name = ellipsize(&self.font, &info.name, px, max_text);
            let tw = text_width(&self.font, &name, px);
            let (x, y) = (MARGIN * s, MARGIN * s);
            let (w, h) = (pad + icon_w + gap + tw + pad, PILL_H * s);
            surface(cv, x, y, w, h, R_PILL * s, s);

            let (gx, gy) = (x + pad, y + h / 2.0);
            let dark: Rgb = [0x2b, 0x2d, 0x31];
            cv.round_rect(gx, gy - 7.0 * s, icon_w, 14.0 * s, 3.5 * s, ACCENT);
            cv.line(
                gx + 2.5 * s,
                gy + 4.2 * s,
                gx + 6.2 * s,
                gy - 0.6 * s,
                1.5 * s,
                dark,
            );
            cv.line(
                gx + 6.2 * s,
                gy - 0.6 * s,
                gx + 9.0 * s,
                gy + 2.6 * s,
                1.5 * s,
                dark,
            );
            cv.line(
                gx + 9.0 * s,
                gy + 2.6 * s,
                gx + 10.4 * s,
                gy + 1.2 * s,
                1.5 * s,
                dark,
            );
            cv.line(
                gx + 10.4 * s,
                gy + 1.2 * s,
                gx + 13.5 * s,
                gy + 4.2 * s,
                1.5 * s,
                dark,
            );
            cv.circle(gx + 11.3 * s, gy - 3.0 * s, 1.5 * s, dark);

            if let Some(lm) = self.font.horizontal_line_metrics(px) {
                let baseline = y + h / 2.0 + (lm.ascent + lm.descent) / 2.0;
                cv.text(&self.font, &name, gx + icon_w + gap, baseline, px, TEXT);
            }
        }

        // Правая капсула: свойства, настройки | закрыть.
        let (x, y, w, h) = lay.capsule();
        surface(cv, x, y, w, h, R_PILL * s, s);

        let half = BTN_S * s / 2.0;
        let btn = |cv: &mut Canvas, c: (f32, f32), color: Rgb, amt: f32| {
            cv.round_rect_alpha(
                c.0 - half,
                c.1 - half,
                2.0 * half,
                2.0 * half,
                R_BTN * s,
                color,
                amt,
            )
        };

        let (ix, iy) = lay.info_center();
        let hv = self.fades[Hit::Info.index()].v;
        btn(
            &mut *cv,
            (ix, iy),
            lerp(HOVER, ACTIVE, self.side.v),
            hv.max(self.side.v),
        );
        let ic = lerp(TEXT, ACCENT, self.side.v);
        cv.ring(ix, iy, 6.5 * s, 1.6 * s, ic);
        cv.circle(ix, iy - 3.0 * s, 1.0 * s, ic);
        cv.line(ix, iy - 0.6 * s, ix, iy + 3.2 * s, 1.6 * s, ic);

        let (mx, my) = lay.more_center();
        let mv = self.fades[Hit::More.index()].v;
        let active = if self.settings_open { 1.0 } else { 0.0 };
        btn(
            &mut *cv,
            (mx, my),
            lerp(HOVER, ACTIVE, active),
            mv.max(active),
        );
        for k in [-1.0f32, 0.0, 1.0] {
            cv.circle(mx + k * 4.4 * s, my, 1.5 * s, TEXT);
        }

        let (cx, cy) = lay.close_center();
        let div = ((mx + cx) / 2.0) as i32;
        cv.fill_rect(div, (cy - 8.0 * s) as i32, 1, (16.0 * s) as i32, BORDER);
        let cf = self.fades[Hit::Close.index()].v;
        btn(&mut *cv, (cx, cy), DANGER, cf);
        let d = 4.3 * s;
        cv.line(cx - d, cy - d, cx + d, cy + d, 1.6 * s, TEXT);
        cv.line(cx - d, cy + d, cx + d, cy - d, 1.6 * s, TEXT);
    }

    fn draw_sidebar(&self, cv: &mut Canvas, lay: &Layout) {
        let s = lay.scale;

        let (px, py) = (lay.panel_x(), lay.top_h as f32);
        let (pw, ph) = (lay.panel_w(), lay.panel_h());
        surface(cv, px, py, pw, ph, R_PANEL * s, s);

        let Some(info) = &self.info else { return };
        let groups: [Vec<(&str, &str)>; 4] = [
            vec![(tr("info.name"), info.name.as_str())],
            vec![(tr("info.folder"), info.folder.as_str())],
            vec![
                (tr("info.resolution"), info.resolution.as_str()),
                (tr("info.format"), info.format.as_str()),
                (tr("info.size"), info.size.as_str()),
            ],
            vec![
                (tr("info.created"), info.created.as_str()),
                (tr("info.modified"), info.modified.as_str()),
            ],
        ];
        let (cx0, cw) = (lay.card_x(), lay.card_w());
        for (n, rows) in groups.iter().enumerate() {
            let y0 = lay.group_y(n);
            if n > 0 {
                cv.fill_rect(
                    cx0 as i32,
                    (y0 - GROUP_GAP * s / 2.0) as i32,
                    cw as i32,
                    1,
                    BORDER,
                );
            }
            for (i, (label, value)) in rows.iter().enumerate() {
                let ry = y0 + i as f32 * ROW_H * s;
                let right_pad = if n == 1 { 44.0 * s } else { 0.0 };
                draw_row(cv, &self.font, cx0, ry, cw, s, label, value, right_pad);
            }
        }

        let (fx, fy) = lay.folder_btn_center();
        let ff = self.fades[Hit::OpenFolder.index()].v;
        let half = BTN_S * s / 2.0;
        cv.round_rect_alpha(
            fx - half,
            fy - half,
            2.0 * half,
            2.0 * half,
            R_BTN * s,
            HOVER,
            ff,
        );
        let ic = lerp(DIM, TEXT, ff);
        cv.round_rect(fx - 7.0 * s, fy - 3.5 * s, 14.0 * s, 10.0 * s, 2.0 * s, ic);
        cv.round_rect(fx - 7.0 * s, fy - 6.5 * s, 6.0 * s, 4.0 * s, 1.5 * s, ic);
    }

    fn render(&mut self) {
        let now = Instant::now();
        let dt = now.duration_since(self.last_frame).as_secs_f32().min(0.05);
        self.last_frame = now;
        let mut busy = false;

        if self.side.step(dt, 16.0) {
            busy = true;
            self.header_dirty = true;
        }
        for (i, f) in self.fades.iter_mut().enumerate() {
            if f.step(dt, 20.0) {
                busy = true;
                if i == Hit::OpenFolder.index() {
                    self.sidebar_dirty = true;
                } else {
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
        if let Some(menu) = &mut self.menu {
            if menu.step(dt) {
                busy = true;
                self.menu_dirty = true;
            }
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
            self.draw_header(
                &mut Canvas {
                    px: &mut hb,
                    w,
                    h: hh,
                    ox: 0,
                    oy: 0,
                },
                &lay,
            );
            header = Some(hb);
        }
        if self.sidebar_dirty {
            self.sidebar_dirty = false;
            if lay.sidebar_w > 0 || self.sidebar_open() {
                let sw = lay.sidebar_full as u32;
                let sh = (h as i32 - lay.side_oy()).max(1) as u32;
                let mut sb = vec![0u32; (sw * sh) as usize];
                self.draw_sidebar(
                    &mut Canvas {
                        px: &mut sb,
                        w: sw,
                        h: sh,
                        ox: w as i32 - lay.sidebar_full,
                        oy: lay.side_oy(),
                    },
                    &lay,
                );
                sidebar = Some((sb, sw, sh));
            }
        }

        let mut menu_buf = None;
        if self.menu_dirty {
            self.menu_dirty = false;
            if let Some(menu) = &self.menu {
                let pad = menu.pad();
                let (mw, mh) = ((menu.w + 2 * pad) as u32, (menu.h + 2 * pad) as u32);
                let mut mb = vec![0u32; (mw * mh) as usize];
                menu.draw(
                    &mut Canvas {
                        px: &mut mb,
                        w: mw,
                        h: mh,
                        ox: menu.x - pad,
                        oy: menu.y - pad,
                    },
                    &self.font,
                );
                menu_buf = Some((mb, mw, mh));
            }
        }

        let mut toast_buf = None;
        if let Some(t) = &mut self.toast {
            if t.dirty {
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
                    };
                    let (fw, fh) = (bw as f32 - 2.0 * pad, bh as f32 - 2.0 * pad);
                    surface(&mut cv, pad, pad, fw, fh, R_PILL * s, s);
                    if let Some(lm) = self.font.horizontal_line_metrics(px) {
                        let baseline = pad + fh / 2.0 + (lm.ascent + lm.descent) / 2.0;
                        cv.text(&self.font, &text, pad + (fw - tw) / 2.0, baseline, px, TEXT);
                    }
                }
                toast_buf = Some((tb, bw, bh));
            }
        }

        let Some(gpu) = self.gpu.as_mut() else { return };
        if let Some(anim) = &mut self.anim {
            if let Some(frame) = anim.take_dirty() {
                gpu.set_image(frame);
            }
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

        gpu.begin(w, h, BG);
        gpu.draw_overlay_scaled(gpu::BG, (0.0, 0.0, w as f32, h as f32), 1.0);

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
            );
        }

        if lay.sidebar_w > 0 {
            gpu.draw_overlay(gpu::SIDEBAR, w as i32 - lay.sidebar_w, lay.side_oy(), 1.0);
        }
        gpu.draw_overlay(gpu::HEADER, 0, 0, 1.0);
        if let Some(menu) = &self.menu {
            let lift = ((1.0 - menu.appear.v) * 6.0 * lay.scale) as i32;
            let mp = menu.pad();
            gpu.draw_overlay(gpu::MENU, menu.x - mp, menu.y - mp + lift, menu.appear.v);
        }
        if let Some(t) = &self.toast {
            if let Some((bw, bh)) = gpu.overlay_size(gpu::TOAST) {
                let x = vx + (vw as i32 - bw) / 2;
                let y = vy + vh as i32 - bh - (28.0 * lay.scale) as i32
                    + lay.pad()
                    + ((1.0 - t.appear.v) * 8.0 * lay.scale) as i32;
                gpu.draw_overlay(gpu::TOAST, x, y, t.appear.v);
            }
        }
        gpu.present();

        if busy {
            self.redraw();
        }
    }
}

impl ApplicationHandler for App {
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
            .with_inner_size(LogicalSize::new(1000.0, 720.0))
            .with_min_inner_size(LogicalSize::new(420.0, 280.0));

        match Gpu::new(el, attrs) {
            Ok((window, mut gpu)) => {
                gpu.set_overlay(gpu::BG, &bg_texture(), 64, 64);
                if let Some(img) = &self.image {
                    gpu.set_image(img);
                }
                window.request_redraw();
                self.window = Some(window);
                self.gpu = Some(gpu);
            }
            Err(e) => {
                eprintln!("mithril: не удалось запустить OpenGL: {e}");
                el.exit();
            }
        }
    }

    fn exiting(&mut self, _el: &ActiveEventLoop) {
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
            WindowEvent::CloseRequested => el.exit(),
            WindowEvent::ModifiersChanged(m) => {
                self.shift = m.state().shift_key();
                self.ctrl = m.state().control_key();
            }
            WindowEvent::KeyboardInput { event, .. }
                if event.state.is_pressed() && !event.repeat =>
            {
                match event.physical_key {
                    PhysicalKey::Code(KeyCode::Escape) => {
                        if self.menu.is_some() {
                            self.close_menu();
                        } else {
                            el.exit();
                        }
                    }
                    PhysicalKey::Code(KeyCode::Digit0) | PhysicalKey::Code(KeyCode::Numpad0) => {
                        self.close_menu();
                        self.apply(Action::ResetZoom);
                    }
                    PhysicalKey::Code(KeyCode::KeyR) if !self.ctrl => {
                        self.close_menu();
                        self.rotate(!self.shift);
                    }
                    PhysicalKey::Code(KeyCode::KeyC) if self.ctrl => {
                        self.close_menu();
                        self.copy_image();
                    }
                    PhysicalKey::Code(KeyCode::Delete) => {
                        self.close_menu();
                        self.request_delete();
                    }
                    PhysicalKey::Code(KeyCode::KeyI) => self.toggle_sidebar(),
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
                self.header_dirty = true;
                self.sidebar_dirty = true;
                self.redraw();
            }
            WindowEvent::MouseWheel { delta, .. } => {
                if self.menu.is_some() {
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
                    return;
                }
                if self.menu.is_some() {
                    let action = self
                        .menu
                        .as_ref()
                        .and_then(|m| m.item_at(self.cursor).and_then(|i| m.action_at(i)));
                    self.close_menu();
                    if let Some(a) = action {
                        self.apply(a);
                    }
                    return;
                }
                let (Some(window), Some(lay)) = (self.window.clone(), self.layout()) else {
                    return;
                };
                if !window.is_maximized() {
                    if let Some(dir) = edge_dir(self.cursor, lay.win, (EDGE * lay.scale) as f64) {
                        let _ = window.drag_resize_window(dir);
                        return;
                    }
                }
                match lay.hit(self.cursor) {
                    Some(Hit::Close) => el.exit(),
                    Some(Hit::Info) => self.toggle_sidebar(),
                    Some(Hit::More) => self.open_settings(),
                    Some(Hit::OpenFolder) => self.open_folder(),
                    None if self.cursor.1 < lay.top_h as f64 => {
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

                if let Some(menu) = &mut self.menu {
                    let item = menu.item_at(new);
                    if item != menu.hover {
                        menu.set_hover(item);
                        self.redraw();
                    }
                }

                self.update_pointer();
            }
            WindowEvent::RedrawRequested => self.render(),
            _ => {}
        }
    }
}

fn main() {
    let font = Font::from_bytes(
        &include_bytes!("../assets/InterVariable.ttf")[..],
        FontSettings::default(),
    )
    .expect("не удалось загрузить шрифт");

    let config = config::load();
    i18n::init(config.language.as_deref());

    let mut app = App::new(font, config);

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
            }
            Err(e) => eprintln!("mithril: {}: {e}", path.display()),
        }
    }

    let event_loop = EventLoop::new().unwrap();
    event_loop.run_app(&mut app).unwrap();
}
