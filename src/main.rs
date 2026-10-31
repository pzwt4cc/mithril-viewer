mod draw;
mod gpu;
mod info;
mod menu;

use std::{
    f32::consts::FRAC_PI_2,
    path::{Path, PathBuf},
    rc::Rc,
    time::Instant,
};

use fontdue::{Font, FontSettings};
use image::{ImageFormat, RgbaImage, imageops};
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::{MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::{CursorIcon, ResizeDirection, Window, WindowId},
};

use draw::{Canvas, Rgb, ellipsize, lerp, text_width};
use gpu::Gpu;
use info::{Info, labels};
use menu::{Action, ITEMS, Menu};

const BG: Rgb = [0x1e, 0x1e, 0x1e];
const HEADER: Rgb = [0x2a, 0x2a, 0x2a];
const SIDEBAR: Rgb = [0x26, 0x26, 0x26];
const CARD: Rgb = [0x31, 0x31, 0x31];
const BORDER: Rgb = [0x3a, 0x3a, 0x3a];
const HOVER: Rgb = [0x3f, 0x3f, 0x3f];
const ACTIVE: Rgb = [0x4d, 0x4d, 0x4d];
const TEXT: Rgb = [0xee, 0xee, 0xee];
const DIM: Rgb = [0x9a, 0x9a, 0x9a];

const HEADER_H: f32 = 46.0;
const SIDEBAR_W: f32 = 300.0;
const BTN_R: f32 = 14.0;
const EDGE: f32 = 5.0;
const PAD: f32 = 12.0;
const GAP: f32 = 12.0;
const ROW_H: f32 = 54.0;
const CARD_ROWS: [usize; 4] = [1, 1, 3, 2];

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
}

impl Hit {
    fn index(self) -> usize {
        self as usize
    }
}

struct Layout {
    scale: f32,
    win: (u32, u32),
    header_h: i32,
    sidebar_w: i32,
    sidebar_full: i32,
}

impl Layout {
    fn new(win: (u32, u32), scale: f32, frac: f32) -> Self {
        let full = (SIDEBAR_W * scale).round() as i32;
        Self {
            scale,
            win,
            header_h: (HEADER_H * scale).round() as i32,
            sidebar_w: (full as f32 * frac).round() as i32,
            sidebar_full: full,
        }
    }

    fn viewport(&self) -> (i32, i32, u32, u32) {
        let w = (self.win.0 as i32 - self.sidebar_w).max(1);
        let h = (self.win.1 as i32 - self.header_h).max(1);
        (0, self.header_h, w as u32, h as u32)
    }

    fn close_center(&self) -> (f32, f32) {
        (
            self.win.0 as f32 - (10.0 + BTN_R) * self.scale,
            self.header_h as f32 / 2.0,
        )
    }

    fn info_center(&self) -> (f32, f32) {
        let (x, y) = self.close_center();
        (x - 36.0 * self.scale, y)
    }

    fn card_x(&self) -> f32 {
        (self.win.0 as i32 - self.sidebar_full) as f32 + PAD * self.scale
    }

    fn card_w(&self) -> f32 {
        self.sidebar_full as f32 - 2.0 * PAD * self.scale
    }

    fn card_y(&self, n: usize) -> f32 {
        let rows_before: usize = CARD_ROWS[..n].iter().sum();
        self.header_h as f32 + (PAD + n as f32 * GAP + rows_before as f32 * ROW_H) * self.scale
    }

    fn folder_btn_center(&self) -> (f32, f32) {
        (
            self.card_x() + self.card_w() - 26.0 * self.scale,
            self.card_y(1) + ROW_H * self.scale / 2.0,
        )
    }

    fn hit(&self, p: (f64, f64)) -> Option<Hit> {
        let r = (BTN_R * self.scale) as f64;
        let near =
            |c: (f32, f32)| ((p.0 - c.0 as f64).powi(2) + (p.1 - c.1 as f64).powi(2)).sqrt() <= r;
        if near(self.close_center()) {
            Some(Hit::Close)
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
    cv.text(font, label, x + 14.0 * s, y + 21.0 * s, 12.0 * s, DIM);
    let vpx = 14.0 * s;
    let max_w = (w - 28.0 * s - right_pad).max(0.0);
    let v = ellipsize(font, value, vpx, max_w);
    cv.text(font, &v, x + 14.0 * s, y + 41.0 * s, vpx, TEXT);
}

struct App {
    font: Font,
    window: Option<Rc<Window>>,
    gpu: Option<Gpu>,
    path: Option<PathBuf>,
    info: Option<Info>,
    image: Option<RgbaImage>,
    view: View,
    cursor: (f64, f64),
    dragging: bool,
    shift: bool,
    side: Fade,
    hover: Option<Hit>,
    fades: [Fade; 3],
    appear: Fade,
    angle: Fade,
    cursor_icon: CursorIcon,
    menu: Option<Menu>,
    header_dirty: bool,
    sidebar_dirty: bool,
    menu_dirty: bool,
    last_frame: Instant,
    last_vp: (u32, u32),
}

impl App {
    fn new(font: Font) -> Self {
        Self {
            font,
            window: None,
            gpu: None,
            path: None,
            info: None,
            image: None,
            view: View::default(),
            cursor: (0.0, 0.0),
            dragging: false,
            shift: false,
            side: Fade::new(0.0),
            hover: None,
            fades: [Fade::new(0.0); 3],
            appear: Fade { v: 0.0, to: 1.0 },
            angle: Fade::new(0.0),
            cursor_icon: CursorIcon::Default,
            menu: None,
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

    fn open_menu(&mut self) {
        let Some(lay) = self.layout() else { return };
        let mut menu = Menu::new(self.cursor, lay.win, lay.scale);
        menu.set_hover(menu.item_at(self.cursor));
        self.menu = Some(menu);
        self.menu_dirty = true;
        self.redraw();
    }

    fn close_menu(&mut self) {
        if self.menu.take().is_some() {
            self.redraw();
        }
    }

    fn apply(&mut self, action: Action) {
        match action {
            Action::ResetZoom => {
                self.view.reset();
                self.redraw();
            }
            Action::RotateLeft => self.rotate(false),
            Action::RotateRight => self.rotate(true),
        }
    }

    fn rotate(&mut self, clockwise: bool) {
        let Some(img) = self.image.take() else { return };
        let rotated = if clockwise {
            imageops::rotate90(&img)
        } else {
            imageops::rotate270(&img)
        };
        if let Some(gpu) = &mut self.gpu {
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
        let new = (old * 1.1f32.powf(steps)).clamp(min, 32.0);

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
        let (w, s) = (lay.win.0, lay.scale);
        let r = BTN_R * s;

        cv.fill_rect(0, 0, w as i32, lay.header_h, HEADER);
        cv.fill_rect(0, lay.header_h - 1, w as i32, 1, BORDER);

        let title = self
            .path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned());
        if let Some(name) = title {
            let px = 14.0 * s;
            let max_w = (w as f32 - 2.0 * 130.0 * s).max(0.0);
            let text = ellipsize(&self.font, &name, px, max_w);
            let tw = text_width(&self.font, &text, px);
            if let Some(lm) = self.font.horizontal_line_metrics(px) {
                let baseline = lay.header_h as f32 / 2.0 + (lm.ascent + lm.descent) / 2.0;
                cv.text(&self.font, &text, (w as f32 - tw) / 2.0, baseline, px, TEXT);
            }
        }

        let (ix, iy) = lay.info_center();
        let hv = self.fades[Hit::Info.index()].v;
        let bg = lerp(lerp(HEADER, HOVER, hv), ACTIVE, self.side.v);
        cv.circle(ix, iy, r, bg);
        cv.ring(ix, iy, 7.0 * s, 1.6 * s, TEXT);
        cv.circle(ix, iy - 3.2 * s, 1.0 * s, TEXT);
        cv.line(ix, iy - 0.6 * s, ix, iy + 3.4 * s, 1.6 * s, TEXT);

        let (cx, cy) = lay.close_center();
        let cv_fade = self.fades[Hit::Close.index()].v;
        cv.circle(cx, cy, r, lerp(HEADER, HOVER, cv_fade));
        let d = 4.5 * s;
        cv.line(cx - d, cy - d, cx + d, cy + d, 1.6 * s, TEXT);
        cv.line(cx - d, cy + d, cx + d, cy - d, 1.6 * s, TEXT);
    }

    fn draw_sidebar(&self, cv: &mut Canvas, lay: &Layout) {
        let (w, h) = lay.win;
        let s = lay.scale;
        let r = BTN_R * s;

        let x = w as i32 - lay.sidebar_full;
        let sh = h as i32 - lay.header_h;
        cv.fill_rect(x, lay.header_h, lay.sidebar_full, sh, SIDEBAR);
        cv.fill_rect(x, lay.header_h, 1, sh, BORDER);

        let Some(info) = &self.info else { return };
        let lb = labels();
        let cards: [Vec<(&str, &str)>; 4] = [
            vec![(lb.name, info.name.as_str())],
            vec![(lb.folder, info.folder.as_str())],
            vec![
                (lb.resolution, info.resolution.as_str()),
                (lb.format, info.format.as_str()),
                (lb.size, info.size.as_str()),
            ],
            vec![
                (lb.created, info.created.as_str()),
                (lb.modified, info.modified.as_str()),
            ],
        ];
        let (cx0, cw) = (lay.card_x(), lay.card_w());
        for (n, rows) in cards.iter().enumerate() {
            let y0 = lay.card_y(n);
            let ch = rows.len() as f32 * ROW_H * s;
            cv.round_rect(cx0, y0, cw, ch, 10.0 * s, CARD);
            for (i, (label, value)) in rows.iter().enumerate() {
                let ry = y0 + i as f32 * ROW_H * s;
                if i > 0 {
                    cv.fill_rect(
                        (cx0 + 14.0 * s) as i32,
                        ry as i32,
                        (cw - 28.0 * s) as i32,
                        1,
                        BORDER,
                    );
                }
                let right_pad = if n == 1 { 44.0 * s } else { 0.0 };
                draw_row(cv, &self.font, cx0, ry, cw, s, label, value, right_pad);
            }
        }

        let (fx, fy) = lay.folder_btn_center();
        let ff = self.fades[Hit::OpenFolder.index()].v;
        cv.circle(fx, fy, r, lerp(CARD, HOVER, ff));
        cv.round_rect(
            fx - 7.0 * s,
            fy - 3.5 * s,
            14.0 * s,
            10.0 * s,
            2.0 * s,
            TEXT,
        );
        cv.round_rect(fx - 7.0 * s, fy - 6.5 * s, 6.0 * s, 4.0 * s, 1.5 * s, TEXT);
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

        let Some(lay) = self.layout() else { return };
        let (w, h) = lay.win;
        if w == 0 || h == 0 {
            return;
        }
        let hh = lay.header_h as u32;
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
                let sh = h.saturating_sub(hh).max(1);
                let mut sb = vec![0u32; (sw * sh) as usize];
                self.draw_sidebar(
                    &mut Canvas {
                        px: &mut sb,
                        w: sw,
                        h: sh,
                        ox: w as i32 - lay.sidebar_full,
                        oy: lay.header_h,
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
                let (mw, mh) = (menu.w as u32, menu.h as u32);
                let mut mb = vec![0u32; (mw * mh) as usize];
                menu.draw(
                    &mut Canvas {
                        px: &mut mb,
                        w: mw,
                        h: mh,
                        ox: menu.x,
                        oy: menu.y,
                    },
                    &self.font,
                );
                menu_buf = Some((mb, mw, mh));
            }
        }

        let Some(gpu) = self.gpu.as_mut() else { return };
        if let Some(hb) = &header {
            gpu.set_header(hb, w, hh);
        }
        if let Some((sb, sw, sh)) = &sidebar {
            gpu.set_sidebar(sb, *sw, *sh);
        }
        if let Some((mb, mw, mh)) = &menu_buf {
            gpu.set_menu(mb, *mw, *mh);
        }

        gpu.begin(w, h, BG);

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
            gpu.draw_sidebar(w as i32 - lay.sidebar_w, lay.header_h);
        }
        gpu.draw_header();
        if let Some(menu) = &self.menu {
            let lift = ((1.0 - menu.appear.v) * 6.0 * lay.scale) as i32;
            gpu.draw_menu(menu.x, menu.y + lift, menu.appear.v);
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

    fn window_event(&mut self, el: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => el.exit(),
            WindowEvent::ModifiersChanged(m) => self.shift = m.state().shift_key(),
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
                    PhysicalKey::Code(KeyCode::KeyR) => {
                        self.close_menu();
                        self.rotate(!self.shift);
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
                    let item = self.menu.as_ref().and_then(|m| m.item_at(self.cursor));
                    self.close_menu();
                    if let Some(i) = item {
                        self.apply(ITEMS[i]);
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
                    Some(Hit::OpenFolder) => self.open_folder(),
                    None if self.cursor.1 < lay.header_h as f64 => {
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

fn load(path: &Path) -> Result<(RgbaImage, Option<ImageFormat>, bool), Box<dyn std::error::Error>> {
    let reader = image::ImageReader::open(path)?.with_guessed_format()?;
    let format = reader.format();
    let img = reader.decode()?;
    let has_alpha = img.color().has_alpha();
    Ok((img.to_rgba8(), format, has_alpha))
}

fn main() {
    let font = Font::from_bytes(
        &include_bytes!("../assets/InterVariable.ttf")[..],
        FontSettings::default(),
    )
    .expect("не удалось загрузить шрифт");

    let mut app = App::new(font);

    if let Some(path) = std::env::args_os().nth(1).map(PathBuf::from) {
        match load(&path) {
            Ok((img, format, has_alpha)) => {
                app.info = Some(Info::gather(&path, img.dimensions(), format, has_alpha));
                app.image = Some(img);
                app.path = Some(path);
            }
            Err(e) => eprintln!("mithril: {}: {e}", path.display()),
        }
    }

    let event_loop = EventLoop::new().unwrap();
    event_loop.run_app(&mut app).unwrap();
}
