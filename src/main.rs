mod anim;
mod config;
mod draw;
mod fonts;
mod gpu;
mod i18n;
mod info;
mod loader;
mod menu;
mod modal;

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
    window::{CursorIcon, Icon, ResizeDirection, Window, WindowId},
};

use anim::Anim;
use config::{Config, DeleteMode, accent};
use draw::{Canvas, Rgb, ellipsize, lerp, text_width};
use fonts::Fonts;
use gpu::Gpu;
use i18n::tr;
use info::Info;
use menu::{Action, Menu};
use modal::{BarAction, CropBar, Dialog, DialogAction};

const APP_ID: &str = "mithril-viewer";

static ICON_PNG: &[u8] = include_bytes!("../data/icons/hicolor/256x256/apps/mithril-viewer.png");

fn window_icon() -> Option<Icon> {
    let img = image::load_from_memory_with_format(ICON_PNG, ImageFormat::Png)
        .ok()?
        .into_rgba8();
    let (w, h) = img.dimensions();
    Icon::from_rgba(img.into_raw(), w, h).ok()
}

const SURF_TOP: Rgb = [0x3b, 0x3e, 0x44];
const SURF_BOT: Rgb = [0x36, 0x39, 0x3f];
const EDGE_TOP: Rgb = [0x4b, 0x4f, 0x57];
const EDGE_BOT: Rgb = [0x46, 0x4a, 0x51];
const BORDER: Rgb = [0x4b, 0x4f, 0x57];
const HOVER: Rgb = [0x4a, 0x4e, 0x57];
const ACTIVE: Rgb = [0x56, 0x5e, 0x73];
const TEXT: Rgb = [0xee, 0xef, 0xf2];
const DIM: Rgb = [0xa3, 0xa8, 0xb1];
const DANGER: Rgb = [0xd9, 0x4f, 0x4f];

const R_WINDOW: f32 = 14.0;
const R_PILL: f32 = 9.0;
const R_PANEL: f32 = 10.0;
const R_MENU: f32 = 9.0;
const R_ITEM: f32 = 6.0;
const R_BTN: f32 = 6.0;

const ZOOM_STEP: f32 = 1.4;

const SHADOW_PAD: f32 = 10.0;
const SHADOW_BLUR: f32 = 8.0;

const MARGIN: f32 = 12.0;
const PILL_H: f32 = 40.0;
const TOP_H: f32 = 76.0;
const BTN_S: f32 = 30.0;
const BTN_GAP: f32 = 6.0;
const CAP_PAD: f32 = 5.0;
const LOGO_S: f32 = 120.0;
const START_W: f32 = 340.0;
const START_H: f32 = 264.0;
const START_BTN_W: f32 = 240.0;
const START_BTN_H: f32 = 46.0;
const RATIO_W: f32 = 108.0;
const NAV_REPEAT: Duration = Duration::from_millis(60);
const RATIO_ROW: f32 = 32.0;
const RATIO_GAP: f32 = 2.0;
const RATIO_PAD: f32 = 6.0;

const SIDEBAR_W: f32 = 332.0;
const PANEL_L: f32 = 16.0;
const PANEL_R: f32 = 16.0;
const PANEL_B: f32 = 16.0;
const PAD: f32 = 14.0;
const GROUP_GAP: f32 = 10.0;
const ROW_H: f32 = 50.0;
const CARD_ROWS: [usize; 4] = [1, 1, 3, 2];
const EDGE: f32 = 5.0;

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

enum UserEvent {
    Printed(Result<(), String>),
    Picked(Option<PathBuf>),
}

enum After {
    Exit,
    Navigate(isize),
    Open(PathBuf),
}

#[derive(Clone, Copy, PartialEq)]
enum Preset {
    Free,
    Original,
    Fixed(u32, u32),
}

const PRESETS: [Preset; 7] = [
    Preset::Free,
    Preset::Original,
    Preset::Fixed(1, 1),
    Preset::Fixed(4, 3),
    Preset::Fixed(3, 2),
    Preset::Fixed(5, 4),
    Preset::Fixed(16, 9),
];

#[derive(Clone, Copy, PartialEq)]
enum Handle {
    Move,
    N,
    S,
    W,
    E,
    NW,
    NE,
    SW,
    SE,
}

#[derive(Clone, Copy)]
struct Drag {
    handle: Handle,
    start: (f64, f64),
    rect: [f32; 4],
}

struct Crop {
    rect: [f32; 4],
    drag: Option<Drag>,
    preset: usize,
    flip: bool,
    bar: CropBar,
    bar_dirty: bool,
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
    Open,
    Start,
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
    empty: bool,
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
            empty: false,
        }
    }

    fn pad(&self) -> i32 {
        (SHADOW_PAD * self.scale).round() as i32
    }

    fn side_oy(&self) -> i32 {
        self.top_h - self.pad()
    }

    fn viewport(&self) -> (i32, i32, u32, u32) {
        let w = (self.win.0 as i32 - self.sidebar_w).max(1);
        (0, 0, w as u32, self.win.1.max(1))
    }

    fn capsule(&self) -> (f32, f32, f32, f32) {
        let s = self.scale;
        let w = (BTN_S * 4.0 + BTN_GAP * 3.0 + CAP_PAD * 2.0) * s;
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

    fn open_center(&self) -> (f32, f32) {
        self.btn_center(0)
    }

    fn info_center(&self) -> (f32, f32) {
        self.btn_center(1)
    }

    fn more_center(&self) -> (f32, f32) {
        self.btn_center(2)
    }

    fn close_center(&self) -> (f32, f32) {
        self.btn_center(3)
    }

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

    fn ratio_panel(&self) -> (f32, f32, f32, f32, f32) {
        let s = self.scale;
        let n = PRESETS.len() as f32;
        let (pad, gap) = (RATIO_PAD * s, RATIO_GAP * s);
        let avail = self.win.1 as f32 - self.top_h as f32 - 2.0 * MARGIN * s;
        let row = (RATIO_ROW * s).min(((avail - 2.0 * pad - gap * (n - 1.0)) / n).max(16.0 * s));
        let h = 2.0 * pad + row * n + gap * (n - 1.0);
        let top = self.top_h as f32;
        let y = top + (self.win.1 as f32 - top - h) / 2.0;
        (MARGIN * s, y, RATIO_W * s, h, row)
    }

    fn ratio_row(&self, i: usize) -> (f32, f32, f32, f32) {
        let s = self.scale;
        let (x, y, w, _, row) = self.ratio_panel();
        (
            x + RATIO_PAD * s,
            y + RATIO_PAD * s + i as f32 * (row + RATIO_GAP * s),
            w - 2.0 * RATIO_PAD * s,
            row,
        )
    }

    fn ratio_at(&self, p: (f64, f64)) -> Option<usize> {
        (0..PRESETS.len()).find(|&i| {
            let r = self.ratio_row(i);
            p.0 >= r.0 as f64
                && p.0 <= (r.0 + r.2) as f64
                && p.1 >= r.1 as f64
                && p.1 <= (r.1 + r.3) as f64
        })
    }

    fn on_ratio_panel(&self, p: (f64, f64)) -> bool {
        let (x, y, w, h, _) = self.ratio_panel();
        p.0 >= x as f64 && p.0 <= (x + w) as f64 && p.1 >= y as f64 && p.1 <= (y + h) as f64
    }

    fn start_origin(&self) -> (f32, f32) {
        let block_h = START_H * self.scale;
        let cx = (self.win.0 as i32 - self.sidebar_w) as f32 / 2.0;
        let top = self.top_h as f32;
        let cy = top + (self.win.1 as f32 - top) / 2.0;
        (cx, (cy - block_h / 2.0).max(top))
    }

    fn start_btn(&self) -> (f32, f32, f32, f32) {
        let s = self.scale;
        let (cx, y0) = self.start_origin();
        (
            cx - START_BTN_W * s / 2.0,
            y0 + 186.0 * s,
            START_BTN_W * s,
            START_BTN_H * s,
        )
    }

    fn hit(&self, p: (f64, f64)) -> Option<Hit> {
        let half = (BTN_S / 2.0 * self.scale) as f64;
        let near =
            |c: (f32, f32)| (p.0 - c.0 as f64).abs() <= half && (p.1 - c.1 as f64).abs() <= half;
        let inside = |r: (f32, f32, f32, f32)| {
            p.0 >= r.0 as f64
                && p.0 <= (r.0 + r.2) as f64
                && p.1 >= r.1 as f64
                && p.1 <= (r.1 + r.3) as f64
        };
        if near(self.close_center()) {
            Some(Hit::Close)
        } else if near(self.more_center()) {
            Some(Hit::More)
        } else if near(self.info_center()) {
            Some(Hit::Info)
        } else if near(self.open_center()) {
            Some(Hit::Open)
        } else if self.empty && inside(self.start_btn()) {
            Some(Hit::Start)
        } else if self.sidebar_w == self.sidebar_full && near(self.folder_btn_center()) {
            Some(Hit::OpenFolder)
        } else {
            None
        }
    }
}

const MIN_CROP: f32 = 8.0;

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

fn ratio_rect(r: f32, c: (f32, f32), iw: f32, ih: f32) -> [f32; 4] {
    let w = iw.min(ih * r);
    let h = w / r;
    let x = (c.0 - w / 2.0).clamp(0.0, (iw - w).max(0.0));
    let y = (c.1 - h / 2.0).clamp(0.0, (ih - h).max(0.0));
    [x, y, x + w, y + h]
}

fn lock_ratio(h: Handle, start: [f32; 4], free: [f32; 4], r: f32, iw: f32, ih: f32) -> [f32; 4] {
    if h == Handle::Move {
        return free;
    }
    let [x0, y0, x1, y1] = start;
    let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
    let (fw, fh) = (free[2] - free[0], free[3] - free[1]);
    let min_w = MIN_CROP.min(iw).max(MIN_CROP.min(ih) * r);
    match h {
        Handle::NW | Handle::NE | Handle::SW | Handle::SE => {
            let right = matches!(h, Handle::NE | Handle::SE);
            let down = matches!(h, Handle::SW | Handle::SE);
            let ax = if right { x0 } else { x1 };
            let ay = if down { y0 } else { y1 };
            let max_w =
                (if right { iw - ax } else { ax }).min((if down { ih - ay } else { ay }) * r);
            let w = fw.max(fh * r).max(min_w).min(max_w);
            let hh = w / r;
            let (nx0, nx1) = if right { (ax, ax + w) } else { (ax - w, ax) };
            let (ny0, ny1) = if down { (ay, ay + hh) } else { (ay - hh, ay) };
            [nx0, ny0, nx1, ny1]
        }
        Handle::E | Handle::W => {
            let east = h == Handle::E;
            let ax = if east { x0 } else { x1 };
            let max_w = (if east { iw - ax } else { ax }).min(2.0 * cy.min(ih - cy) * r);
            let w = fw.max(min_w).min(max_w);
            let hh = w / r;
            let (nx0, nx1) = if east { (ax, ax + w) } else { (ax - w, ax) };
            [nx0, cy - hh / 2.0, nx1, cy + hh / 2.0]
        }
        _ => {
            let south = h == Handle::S;
            let ay = if south { y0 } else { y1 };
            let max_h = (if south { ih - ay } else { ay }).min(2.0 * cx.min(iw - cx) / r);
            let hh = fh.max(min_w / r).min(max_h);
            let w = hh * r;
            let (ny0, ny1) = if south { (ay, ay + hh) } else { (ay - hh, ay) };
            [cx - w / 2.0, ny0, cx + w / 2.0, ny1]
        }
    }
}

fn drag_rect(h: Handle, r: [f32; 4], dx: f32, dy: f32, iw: f32, ih: f32) -> [f32; 4] {
    if h == Handle::Move {
        let dx = dx.max(-r[0]).min(iw - r[2]);
        let dy = dy.max(-r[1]).min(ih - r[3]);
        return [r[0] + dx, r[1] + dy, r[2] + dx, r[3] + dy];
    }
    let (mw, mh) = (MIN_CROP.min(iw), MIN_CROP.min(ih));
    let mut n = r;
    if matches!(h, Handle::W | Handle::NW | Handle::SW) {
        n[0] = (r[0] + dx).min(r[2] - mw).max(0.0);
    }
    if matches!(h, Handle::E | Handle::NE | Handle::SE) {
        n[2] = (r[2] + dx).max(r[0] + mw).min(iw);
    }
    if matches!(h, Handle::N | Handle::NW | Handle::NE) {
        n[1] = (r[1] + dy).min(r[3] - mh).max(0.0);
    }
    if matches!(h, Handle::S | Handle::SW | Handle::SE) {
        n[3] = (r[3] + dy).max(r[1] + mh).min(ih);
    }
    n
}

fn handle_icon(h: Handle) -> CursorIcon {
    match h {
        Handle::Move => CursorIcon::Move,
        Handle::N | Handle::S => CursorIcon::NsResize,
        Handle::W | Handle::E => CursorIcon::EwResize,
        Handle::NW | Handle::SE => CursorIcon::NwseResize,
        Handle::NE | Handle::SW => CursorIcon::NeswResize,
    }
}

fn quad(gpu: &Gpu, slot: usize, x0: f32, y0: f32, x1: f32, y1: f32, a: f32) {
    let (x0, y0, x1, y1) = (x0.round(), y0.round(), x1.round(), y1.round());
    if x1 > x0 && y1 > y0 {
        gpu.draw_overlay_scaled(slot, (x0, y0, x1 - x0, y1 - y0), a);
    }
}

fn draw_crop(gpu: &Gpu, s: f32, c: [f32; 4], im: [f32; 4], vp: (i32, i32, u32, u32)) {
    let (vx, vy, vw, vh) = vp;
    let (ix0, iy0) = (im[0].max(vx as f32), im[1].max(vy as f32));
    let (ix1, iy1) = (
        im[2].min((vx + vw as i32) as f32),
        im[3].min((vy + vh as i32) as f32),
    );
    let (cx0, cy0) = (c[0].max(ix0).min(ix1), c[1].max(iy0).min(iy1));
    let (cx1, cy1) = (c[2].max(ix0).min(ix1), c[3].max(iy0).min(iy1));
    let dim = 0.55;
    quad(gpu, gpu::BLACK, ix0, iy0, ix1, cy0, dim);
    quad(gpu, gpu::BLACK, ix0, cy1, ix1, iy1, dim);
    quad(gpu, gpu::BLACK, ix0, cy0, cx0, cy1, dim);
    quad(gpu, gpu::BLACK, cx1, cy0, ix1, cy1, dim);

    for i in 1..3 {
        let f = i as f32 / 3.0;
        let x = c[0] + (c[2] - c[0]) * f;
        let y = c[1] + (c[3] - c[1]) * f;
        quad(gpu, gpu::WHITE, x, c[1], x + 1.0, c[3], 0.3);
        quad(gpu, gpu::WHITE, c[0], y, c[2], y + 1.0, 0.3);
    }

    let t = (1.5 * s).max(1.0);
    quad(gpu, gpu::WHITE, c[0], c[1], c[2], c[1] + t, 1.0);
    quad(gpu, gpu::WHITE, c[0], c[3] - t, c[2], c[3], 1.0);
    quad(gpu, gpu::WHITE, c[0], c[1], c[0] + t, c[3], 1.0);
    quad(gpu, gpu::WHITE, c[2] - t, c[1], c[2], c[3], 1.0);

    let (mx, my) = ((c[0] + c[2]) / 2.0, (c[1] + c[3]) / 2.0);
    let hs = 4.5 * s;
    for (px, py) in [
        (c[0], c[1]),
        (mx, c[1]),
        (c[2], c[1]),
        (c[0], my),
        (c[2], my),
        (c[0], c[3]),
        (mx, c[3]),
        (c[2], c[3]),
    ] {
        quad(
            gpu,
            gpu::BLACK,
            px - hs - 1.0,
            py - hs - 1.0,
            px + hs + 1.0,
            py + hs + 1.0,
            0.45,
        );
        quad(gpu, gpu::WHITE, px - hs, py - hs, px + hs, py + hs, 1.0);
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
    font: &Fonts,
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
    hover: Option<Hit>,
    fades: [Fade; 6],
    appear: Fade,
    angle: Fade,
    cursor_icon: CursorIcon,
    menu: Option<Menu>,
    settings_open: bool,
    toast: Option<Toast>,
    crop: Option<Crop>,
    dialog: Option<Dialog>,
    dialog_dirty: bool,
    modified: bool,
    proxy: EventLoopProxy<UserEvent>,
    clipboard: Option<arboard::Clipboard>,
    pending_delete: Option<Instant>,
    exit_requested: bool,
    picking: bool,
    logo: Option<(u32, Vec<u32>)>,
    ratio_hover: Option<usize>,
    last_nav: Option<Instant>,
    header_dirty: bool,
    sidebar_dirty: bool,
    menu_dirty: bool,
    last_frame: Instant,
    last_vp: (u32, u32),
    after: After,
    sidebar_ready: bool,
    anim_active: bool,
}

impl App {
    fn new(font: Fonts, config: Config, proxy: EventLoopProxy<UserEvent>) -> Self {
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
            view: View::default(),
            cursor: (0.0, 0.0),
            dragging: false,
            shift: false,
            ctrl: false,
            side: Fade::new(0.0),
            hover: None,
            fades: [Fade::new(0.0); 6],
            appear: Fade { v: 0.0, to: 1.0 },
            angle: Fade::new(0.0),
            cursor_icon: CursorIcon::Default,
            menu: None,
            settings_open: false,
            toast: None,
            crop: None,
            dialog: None,
            dialog_dirty: false,
            modified: false,
            proxy,
            clipboard: None,
            pending_delete: None,
            exit_requested: false,
            picking: false,
            logo: None,
            ratio_hover: None,
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
    }

    fn open_path(&mut self, path: PathBuf) -> bool {
        match loader::load(&path) {
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
                self.pending_delete = None;
                self.view = View::default();
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
        if self.picking || self.dialog.is_some() || self.crop.is_some() {
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
                .add_filter("Images", &["png", "jpg", "jpeg", "gif", "webp", "bmp"]);
            if let Some(dir) = start {
                dlg = dlg.set_directory(dir);
            }
            let _ = proxy.send_event(UserEvent::Picked(dlg.pick_file()));
        });
    }

    fn open_external(&mut self, path: PathBuf) {
        if self.dialog.is_some() || self.crop.is_some() {
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
        if self.crop.is_some() || self.dialog.is_some() || self.siblings.len() < 2 {
            return;
        }
        if self.modified {
            self.after = After::Navigate(delta);
            self.open_dialog();
            return;
        }
        self.close_menu();
        let n = self.siblings.len() as isize;
        let mut i = self.index as isize;
        for _ in 1..n {
            i = (i + delta).rem_euclid(n);
            let path = self.siblings[i as usize].clone();
            if self.open_path(path) {
                self.index = i as usize;
                self.last_nav = Some(Instant::now());
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
            self.siblings.remove(self.index);
        }
        while !self.siblings.is_empty() {
            let i = self.index.min(self.siblings.len() - 1);
            let path = self.siblings[i].clone();
            if self.open_path(path) {
                self.index = i;
                return;
            }
            self.siblings.remove(i);
        }
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
        let entries = menu::settings_entries(
            self.config.language.as_deref(),
            self.config.delete_mode,
            self.config.opacity,
            self.config.accent,
        );
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
            Action::Crop => self.start_crop(),
            Action::Print => self.print(),
            Action::Copy => self.copy_image(),
            Action::Delete => self.request_delete(),
            Action::SetLanguage(code) => self.set_language(code),
            Action::SetAccent(a) => {
                self.config.accent = a;
                config::set_accent(a);
                config::save(&self.config);
                self.header_dirty = true;
                self.sidebar_dirty = true;
                if let Some(c) = &mut self.crop {
                    c.bar_dirty = true;
                }
                self.redraw();
            }
            Action::SetOpacity(p) => {
                self.config.opacity = p;
                config::save(&self.config);
                self.redraw();
            }
            Action::SetDeleteMode(mode) => {
                self.config.delete_mode = mode;
                self.pending_delete = None;
                config::save(&self.config);
            }
        }
    }

    fn rotate(&mut self, clockwise: bool) {
        if self.crop.is_some() || self.dialog.is_some() {
            return;
        }
        let Some(img) = self.image.take() else { return };
        let rotated = if clockwise {
            imageops::rotate90(&img)
        } else {
            imageops::rotate270(&img)
        };
        if let Some(anim) = &mut self.anim {
            anim.rotate(clockwise);
        } else {
            if let Some(gpu) = &mut self.gpu {
                gpu.set_image(&rotated);
            }
            self.modified = true;
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
            Ok(()) => self.after_delete(),
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

        let rh = if self.crop.is_some() && self.dialog.is_none() {
            lay.ratio_at(self.cursor)
        } else {
            None
        };
        if rh != self.ratio_hover {
            self.ratio_hover = rh;
            window.request_redraw();
        }

        let mut icon = if window.is_maximized() || self.dialog.is_some() {
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
        if self.crop.is_some() || self.dialog.is_some() {
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
        });
        self.ratio_hover = None;
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
        if let Some(r) = self.crop_ratio() {
            if let Some(c) = &mut self.crop {
                c.rect = ratio_rect(r, center, iw as f32, ih as f32);
            }
        }
        self.redraw();
    }

    fn cancel_crop(&mut self) {
        if self.crop.take().is_some() {
            self.redraw();
        }
    }

    fn apply_crop(&mut self) {
        let Some(c) = self.crop.take() else { return };
        let Some(img) = self.image.as_ref() else {
            return;
        };
        let (iw, ih) = img.dimensions();
        let x = (c.rect[0].round().max(0.0) as u32).min(iw.saturating_sub(1));
        let y = (c.rect[1].round().max(0.0) as u32).min(ih.saturating_sub(1));
        let w = ((c.rect[2] - c.rect[0]).round().max(1.0) as u32).min(iw - x);
        let h = ((c.rect[3] - c.rect[1]).round().max(1.0) as u32).min(ih - y);
        if (x, y, w, h) == (0, 0, iw, ih) {
            self.redraw();
            return;
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
            DialogAction::Cancel => self.close_dialog(),
            DialogAction::Discard => self.finish_dialog(el),
            DialogAction::Save => match self.save_copy() {
                Ok(_) => self.finish_dialog(el),
                Err(e) => {
                    self.close_dialog();
                    self.show_toast(format!("{}: {e}", tr("toast.save_failed")));
                }
            },
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
        let Some(img) = self
            .anim
            .as_ref()
            .map(|a| a.current())
            .or(self.image.as_ref())
        else {
            return;
        };
        let img = img.clone();
        let proxy = self.proxy.clone();
        std::thread::spawn(move || {
            let res = (|| -> Result<(), String> {
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
                let out = std::process::Command::new("lp")
                    .arg("-o")
                    .arg("fit-to-page")
                    .arg(&path)
                    .output();
                let _ = std::fs::remove_file(&path);
                let out = out.map_err(|e| e.to_string())?;
                if out.status.success() {
                    Ok(())
                } else {
                    Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
                }
            })();
            let _ = proxy.send_event(UserEvent::Printed(res));
        });
    }

    fn draw_header(&self, cv: &mut Canvas, lay: &Layout) {
        let s = lay.scale;

        if let Some(info) = &self.info {
            let px = 14.0 * s;
            let (pad, icon_w, gap) = (14.0 * s, 16.0 * s, 9.0 * s);
            let counter = if self.siblings.len() > 1 {
                format!("{} / {}", self.index + 1, self.siblings.len())
            } else {
                String::new()
            };
            let cw = if counter.is_empty() {
                0.0
            } else {
                gap + text_width(&self.font, &counter, px)
            };
            let max_text = (lay.title_max_w() - 2.0 * pad - icon_w - gap - cw).max(0.0);
            let name = ellipsize(&self.font, &info.name, px, max_text);
            let tw = text_width(&self.font, &name, px);
            let (x, y) = (MARGIN * s, MARGIN * s);
            let (w, h) = (pad + icon_w + gap + tw + cw + pad, PILL_H * s);
            surface(cv, x, y, w, h, R_PILL * s, s);

            let (gx, gy) = (x + pad, y + h / 2.0);
            let dark: Rgb = [0x2b, 0x2d, 0x31];
            cv.round_rect(gx, gy - 7.0 * s, icon_w, 14.0 * s, 3.5 * s, accent());
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
                if !counter.is_empty() {
                    let cx = gx + icon_w + gap + tw + gap;
                    cv.text(&self.font, &counter, cx, baseline, px, DIM);
                }
            }
        }

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

        let (ox, oy) = lay.open_center();
        let ov = self.fades[Hit::Open.index()].v;
        btn(&mut *cv, (ox, oy), HOVER, ov);
        cv.round_rect(ox - 7.0 * s, oy - 6.0 * s, 6.0 * s, 4.0 * s, 1.5 * s, TEXT);
        cv.round_rect(
            ox - 7.0 * s,
            oy - 4.0 * s,
            14.0 * s,
            10.0 * s,
            2.2 * s,
            TEXT,
        );

        let (ix, iy) = lay.info_center();
        let hv = self.fades[Hit::Info.index()].v;
        btn(
            &mut *cv,
            (ix, iy),
            lerp(HOVER, ACTIVE, self.side.v),
            hv.max(self.side.v),
        );
        let ic = lerp(TEXT, accent(), self.side.v);
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

    fn draw_ratios(&self, cv: &mut Canvas, lay: &Layout) {
        let Some(c) = &self.crop else { return };
        let s = lay.scale;
        let (x, y, w, h, _) = lay.ratio_panel();
        surface(cv, x, y, w, h, R_PANEL * s, s);
        let px = 13.0 * s;
        let dark: Rgb = [0x2b, 0x2d, 0x31];
        let Some(lm) = self.font.horizontal_line_metrics(px) else {
            return;
        };
        for (i, preset) in PRESETS.iter().enumerate() {
            let (rx, ry, rw, rh) = lay.ratio_row(i);
            let active = c.preset == i;
            if active {
                cv.round_rect(rx, ry, rw, rh, R_ITEM * s, accent());
            } else if self.ratio_hover == Some(i) {
                cv.round_rect(rx, ry, rw, rh, R_ITEM * s, HOVER);
            }
            let label = match *preset {
                Preset::Free => tr("crop.free").to_string(),
                Preset::Original => tr("crop.original").to_string(),
                Preset::Fixed(a, b) if c.flip => format!("{b}:{a}"),
                Preset::Fixed(a, b) => format!("{a}:{b}"),
            };
            let baseline = ry + rh / 2.0 + (lm.ascent + lm.descent) / 2.0;
            let color = if active { dark } else { TEXT };
            cv.text(&self.font, &label, rx + 10.0 * s, baseline, px, color);
        }
    }

    fn draw_start(&self, cv: &mut Canvas, lay: &Layout) {
        let s = lay.scale;
        let (cx, y0) = lay.start_origin();
        let font = &self.font;
        let line = |cv: &mut Canvas, text: &str, row_y: f32, row_h: f32, px: f32, color: Rgb| {
            let Some(lm) = font.horizontal_line_metrics(px) else {
                return;
            };
            let tw = text_width(font, text, px);
            let baseline = row_y + row_h / 2.0 + (lm.ascent + lm.descent) / 2.0;
            cv.text(font, text, cx - tw / 2.0, baseline, px, color);
        };

        line(
            &mut *cv,
            tr("empty.title"),
            y0 + 142.0 * s,
            26.0 * s,
            22.0 * s,
            TEXT,
        );

        let (bx, by, bw, bh) = lay.start_btn();
        let hv = self.fades[Hit::Start.index()].v;
        let fill = lerp(accent(), [255, 255, 255], 0.18 * hv);
        cv.round_rect(bx, by, bw, bh, 12.0 * s, fill);
        let dark: Rgb = [0x2b, 0x2d, 0x31];
        line(&mut *cv, tr("empty.button"), by, bh, 15.0 * s, dark);

        line(
            &mut *cv,
            tr("empty.hint"),
            y0 + 246.0 * s,
            18.0 * s,
            13.0 * s,
            DIM,
        );
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
        for (i, f) in self.fades.iter_mut().enumerate() {
            if f.step(dt, 20.0) {
                busy = true;
                if i == Hit::OpenFolder.index() {
                    self.sidebar_dirty = true;
                } else if i != Hit::Start.index() {
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
            if lay.sidebar_w > 0 || self.sidebar_open() || !self.sidebar_ready {
                self.sidebar_ready = true;
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

        let crop_scr = match &self.crop {
            Some(c) => self.crop_screen(&lay, c.rect),
            None => None,
        };
        if let Some(c) = &mut self.crop {
            let label = format!(
                "{} × {}",
                (c.rect[2] - c.rect[0]).round() as u32,
                (c.rect[3] - c.rect[1]).round() as u32
            );
            if c.bar.label != label {
                c.bar.label = label;
                c.bar_dirty = true;
            }
        }

        let mut bar_buf = None;
        if let Some(c) = &mut self.crop {
            if c.bar_dirty {
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
                    },
                    &self.font,
                );
                bar_buf = Some((bb, bw, bh));
            }
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
                    },
                    &self.font,
                );
                dialog_buf = Some((db, dw, dh));
            }
        }

        let mut ratio_buf = None;
        if self.crop.is_some() {
            let (px, py, pw, ph, _) = lay.ratio_panel();
            let pad = ((SHADOW_PAD + 2.0) * lay.scale).ceil();
            let (rw, rh) = (
                (pw + 2.0 * pad).ceil() as u32,
                (ph + 2.0 * pad).ceil() as u32,
            );
            let (rx, ry) = ((px - pad).floor() as i32, (py - pad).floor() as i32);
            let mut rb = vec![0u32; (rw * rh) as usize];
            self.draw_ratios(
                &mut Canvas {
                    px: &mut rb,
                    w: rw,
                    h: rh,
                    ox: rx,
                    oy: ry,
                },
                &lay,
            );
            ratio_buf = Some((rb, rw, rh, rx, ry));
        }

        let mut empty_buf = None;
        if self.image.is_none() {
            let sc = lay.scale;
            let (cx, y0) = lay.start_origin();
            let (ew, eh) = ((START_W * sc).round() as u32, (START_H * sc).round() as u32);
            let (ex, ey) = ((cx - ew as f32 / 2.0).round() as i32, y0.round() as i32);
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
            self.draw_start(
                &mut Canvas {
                    px: &mut eb,
                    w: ew,
                    h: eh,
                    ox: ex,
                    oy: ey,
                },
                &lay,
            );
            empty_buf = Some((eb, ew, eh, ex, ey));
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
        if let Some((bb, bw, bh)) = &bar_buf {
            gpu.set_overlay(gpu::BAR, bb, *bw, *bh);
        }
        if let Some((db, dw, dh)) = &dialog_buf {
            gpu.set_overlay(gpu::DIALOG, db, *dw, *dh);
        }
        if let Some((eb, ew, eh, _, _)) = &empty_buf {
            gpu.set_overlay(gpu::EMPTY, eb, *ew, *eh);
        }
        if let Some((rb, rw, rh, _, _)) = &ratio_buf {
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
            );
        }
        if let Some((c, im)) = crop_scr {
            draw_crop(gpu, lay.scale, c, im, (vx, vy, vw, vh));
        }

        if let Some((_, _, _, ex, ey)) = &empty_buf {
            gpu.draw_overlay(gpu::EMPTY, *ex, *ey, 1.0);
        }
        if lay.sidebar_w > 0 {
            gpu.draw_overlay(gpu::SIDEBAR, w as i32 - lay.sidebar_w, lay.side_oy(), 1.0);
        }
        gpu.draw_overlay(gpu::HEADER, 0, 0, 1.0);
        if let Some((_, _, _, rx, ry)) = &ratio_buf {
            gpu.draw_overlay(gpu::RATIOS, *rx, *ry, 1.0);
        }
        if let Some(c) = &self.crop {
            let (bx, by) = c.bar.origin((vx, vy, vw, vh));
            let pad = c.bar.pad();
            gpu.draw_overlay(gpu::BAR, bx - pad, by - pad, 1.0);
        }
        if let Some(menu) = &self.menu {
            let lift = ((1.0 - menu.appear.v) * 6.0 * lay.scale) as i32;
            let mp = menu.pad();
            gpu.draw_overlay(gpu::MENU, menu.x - mp, menu.y - mp + lift, menu.appear.v);
        }
        if let Some(t) = &self.toast {
            if let Some((bw, bh)) = gpu.overlay_size(gpu::TOAST) {
                let x = vx + (vw as i32 - bw) / 2;
                let lift = if self.crop.is_some() {
                    (56.0 * lay.scale) as i32
                } else {
                    0
                };
                let y = vy + vh as i32 - bh - (28.0 * lay.scale) as i32 - lift
                    + lay.pad()
                    + ((1.0 - t.appear.v) * 8.0 * lay.scale) as i32;
                gpu.draw_overlay(gpu::TOAST, x, y, t.appear.v);
            }
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
                gpu.set_overlay(gpu::BG, &bg_texture(), 64, 64);
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
                eprintln!("mithril: не удалось запустить OpenGL: {e}");
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
            UserEvent::Printed(Ok(())) => self.show_toast(tr("toast.print_sent").to_string()),
            UserEvent::Printed(Err(e)) => {
                let short: String = e.lines().next().unwrap_or("").chars().take(60).collect();
                let msg = if short.is_empty() {
                    tr("toast.print_failed").to_string()
                } else {
                    format!("{}: {short}", tr("toast.print_failed"))
                };
                self.show_toast(msg);
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
            WindowEvent::CloseRequested => self.request_exit(el),
            WindowEvent::ModifiersChanged(m) => {
                self.shift = m.state().shift_key();
                self.ctrl = m.state().control_key();
            }
            WindowEvent::KeyboardInput { event, .. }
                if event.state.is_pressed()
                    && (!event.repeat || is_nav_key(event.physical_key)) =>
            {
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
                        if self.crop.is_some() {
                            self.cancel_crop();
                        } else if self.menu.is_some() {
                            self.close_menu();
                        } else {
                            self.request_exit(el);
                        }
                    }
                    PhysicalKey::Code(KeyCode::Enter | KeyCode::NumpadEnter) => {
                        if self.crop.is_some() {
                            self.apply_crop();
                        } else if self.image.is_none() {
                            self.pick_file();
                        }
                    }
                    PhysicalKey::Code(KeyCode::Digit0) | PhysicalKey::Code(KeyCode::Numpad0) => {
                        self.close_menu();
                        self.apply(Action::ResetZoom);
                    }
                    PhysicalKey::Code(KeyCode::KeyO) if self.ctrl => self.pick_file(),
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
                if self.dialog.is_some() || self.crop.is_some() {
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
                    if let Some(c) = &mut self.crop {
                        c.drag = None;
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
                if self.menu.is_some() {
                    let action = self.menu.as_ref().and_then(|m| m.action_under(self.cursor));
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
                if let Some(p) = self.bar_local(&lay) {
                    let action = self.crop.as_ref().and_then(|c| c.bar.action_at(p));
                    match action {
                        Some(BarAction::Cancel) => {
                            self.cancel_crop();
                            return;
                        }
                        Some(BarAction::Apply) => {
                            self.apply_crop();
                            return;
                        }
                        None => {}
                    }
                    let on_bar = self.crop.as_ref().is_some_and(|c| {
                        p.0 >= 0.0 && p.1 >= 0.0 && p.0 < c.bar.w as f32 && p.1 < c.bar.h as f32
                    });
                    if on_bar {
                        return;
                    }
                }
                if self.crop.is_some() {
                    if let Some(i) = lay.ratio_at(self.cursor) {
                        self.set_crop_preset(i);
                        return;
                    }
                    if lay.on_ratio_panel(self.cursor) {
                        return;
                    }
                }
                if self.crop.is_some() && lay.hit(self.cursor).is_none() && self.crop_press(&lay) {
                    return;
                }
                match lay.hit(self.cursor) {
                    Some(Hit::Close) => self.request_exit(el),
                    Some(Hit::Info) => self.toggle_sidebar(),
                    Some(Hit::More) => self.open_settings(),
                    Some(Hit::OpenFolder) => self.open_folder(),
                    Some(Hit::Open | Hit::Start) => self.pick_file(),
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
                self.update_crop_drag();
                self.update_modal_hover();

                if let Some(menu) = &mut self.menu {
                    let item = menu.item_at(new);
                    if item != menu.hover {
                        menu.set_hover(item);
                        self.redraw();
                    }
                }

                self.update_pointer();
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
        .expect("не удалось загрузить шрифт"),
    );

    let config = config::load();
    config::set_accent(config.accent);
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
