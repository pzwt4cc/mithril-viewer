mod draw;
mod info;

use std::{
    num::NonZeroU32,
    path::{Path, PathBuf},
    rc::Rc,
};

use fontdue::{Font, FontSettings};
use image::{
    ImageFormat, RgbaImage,
    imageops::{self, FilterType},
};
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::{MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::{CursorIcon, ResizeDirection, Window, WindowId},
};

use draw::{Canvas, Rgb, ellipsize, text_width};
use info::{Info, labels};

type Surface = softbuffer::Surface<Rc<Window>, Rc<Window>>;

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

fn fit_scale((iw, ih): (u32, u32), (w, h): (u32, u32)) -> f32 {
    (w as f32 / iw as f32).min(h as f32 / ih as f32).min(1.0)
}

#[derive(Default)]
struct View {
    zoom: Option<f32>,
    pan: (f32, f32),
}

impl View {
    fn scale(&self, img: (u32, u32), vp: (u32, u32)) -> f32 {
        self.zoom.unwrap_or_else(|| fit_scale(img, vp))
    }

    fn clamp_pan(&mut self, img: (u32, u32), vp: (u32, u32)) {
        let s = self.scale(img, vp);
        let max_x = ((img.0 as f32 * s - vp.0 as f32) / 2.0).max(0.0);
        let max_y = ((img.1 as f32 * s - vp.1 as f32) / 2.0).max(0.0);
        self.pan.0 = self.pan.0.clamp(-max_x, max_x);
        self.pan.1 = self.pan.1.clamp(-max_y, max_y);
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Hit {
    Info,
    Close,
    OpenFolder,
}

struct Layout {
    scale: f32,
    win: (u32, u32),
    header_h: i32,
    sidebar_w: i32,
}

impl Layout {
    fn new(win: (u32, u32), scale: f32, sidebar: bool) -> Self {
        Self {
            scale,
            win,
            header_h: (HEADER_H * scale).round() as i32,
            sidebar_w: if sidebar {
                (SIDEBAR_W * scale).round() as i32
            } else {
                0
            },
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
        (self.win.0 as i32 - self.sidebar_w) as f32 + PAD * self.scale
    }

    fn card_w(&self) -> f32 {
        self.sidebar_w as f32 - 2.0 * PAD * self.scale
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
        } else if self.sidebar_w > 0 && near(self.folder_btn_center()) {
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

fn draw_image(
    cv: &mut Canvas,
    src: &RgbaImage,
    k: f32,
    center: (f32, f32),
    clip: (i32, i32, i32, i32),
) {
    let (sw, sh) = src.dimensions();
    let ox = center.0 - sw as f32 * k / 2.0;
    let oy = center.1 - sh as f32 * k / 2.0;
    let x_lo = (ox.floor() as i32).max(clip.0);
    let x_hi = ((ox + sw as f32 * k).ceil() as i32).min(clip.2);
    let y_lo = (oy.floor() as i32).max(clip.1);
    let y_hi = ((oy + sh as f32 * k).ceil() as i32).min(clip.3);
    for y in y_lo..y_hi {
        let sy = (((y as f32 + 0.5 - oy) / k).floor().max(0.0) as u32).min(sh - 1);
        for x in x_lo..x_hi {
            let sx = (((x as f32 + 0.5 - ox) / k).floor().max(0.0) as u32).min(sw - 1);
            let [r, g, b, a] = src.get_pixel(sx, sy).0;
            cv.blend(x, y, [r, g, b], a as f32 / 255.0);
        }
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
    context: Option<softbuffer::Context<Rc<Window>>>,
    surface: Option<Surface>,
    path: Option<PathBuf>,
    info: Option<Info>,
    image: Option<RgbaImage>,
    scaled: Option<RgbaImage>,
    view: View,
    cursor: (f64, f64),
    dragging: bool,
    sidebar_open: bool,
    hover: Option<Hit>,
    cursor_icon: CursorIcon,
}

impl App {
    fn new(font: Font) -> Self {
        Self {
            font,
            window: None,
            context: None,
            surface: None,
            path: None,
            info: None,
            image: None,
            scaled: None,
            view: View::default(),
            cursor: (0.0, 0.0),
            dragging: false,
            sidebar_open: false,
            hover: None,
            cursor_icon: CursorIcon::Default,
        }
    }

    fn redraw(&self) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    fn layout(&self) -> Option<Layout> {
        let w = self.window.as_ref()?;
        let size = w.inner_size();
        Some(Layout::new(
            (size.width, size.height),
            w.scale_factor() as f32,
            self.sidebar_open,
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

    fn render(&mut self) {
        let Some(lay) = self.layout() else { return };
        let (w, h) = lay.win;
        let (Some(nw), Some(nh)) = (NonZeroU32::new(w), NonZeroU32::new(h)) else {
            return;
        };
        let (vx, vy, vw, vh) = lay.viewport();
        let s = lay.scale;

        let mut plan = None;
        if let Some(img) = &self.image {
            let dims = img.dimensions();
            self.view.clamp_pan(dims, (vw, vh));
            let scale = self.view.scale(dims, (vw, vh));
            let dw = ((dims.0 as f32 * scale).round() as u32).max(1);
            let dh = ((dims.1 as f32 * scale).round() as u32).max(1);
            let use_scaled = scale <= 1.0 && (dw, dh) != dims;
            if use_scaled && self.scaled.as_ref().map(|c| c.dimensions()) != Some((dw, dh)) {
                self.scaled = Some(imageops::resize(img, dw, dh, FilterType::Triangle));
            }
            plan = Some((scale, use_scaled));
        }

        let title = self
            .path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned());

        let Some(surface) = self.surface.as_mut() else {
            return;
        };
        surface.resize(nw, nh).unwrap();
        let mut buffer = surface.buffer_mut().unwrap();
        let mut cv = Canvas {
            px: &mut buffer,
            w,
            h,
        };
        cv.clear(BG);

        if let (Some(img), Some((scale, use_scaled))) = (&self.image, plan) {
            let (src, k) = if use_scaled {
                (self.scaled.as_ref().unwrap(), 1.0)
            } else {
                (img, scale)
            };
            let center = (
                vx as f32 + vw as f32 / 2.0 + self.view.pan.0,
                vy as f32 + vh as f32 / 2.0 + self.view.pan.1,
            );
            draw_image(
                &mut cv,
                src,
                k,
                center,
                (vx, vy, vx + vw as i32, vy + vh as i32),
            );
        }

        let r = BTN_R * s;

        if lay.sidebar_w > 0 {
            let x = w as i32 - lay.sidebar_w;
            let sh = h as i32 - lay.header_h;
            cv.fill_rect(x, lay.header_h, lay.sidebar_w, sh, SIDEBAR);
            cv.fill_rect(x, lay.header_h, 1, sh, BORDER);

            if let Some(info) = &self.info {
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
                        draw_row(&mut cv, &self.font, cx0, ry, cw, s, label, value, right_pad);
                    }
                }

                let (fx, fy) = lay.folder_btn_center();
                if self.hover == Some(Hit::OpenFolder) {
                    cv.circle(fx, fy, r, HOVER);
                }
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
        }

        cv.fill_rect(0, 0, w as i32, lay.header_h, HEADER);
        cv.fill_rect(0, lay.header_h - 1, w as i32, 1, BORDER);

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
        if self.sidebar_open {
            cv.circle(ix, iy, r, ACTIVE);
        } else if self.hover == Some(Hit::Info) {
            cv.circle(ix, iy, r, HOVER);
        }
        cv.ring(ix, iy, 7.0 * s, 1.6 * s, TEXT);
        cv.circle(ix, iy - 3.2 * s, 1.0 * s, TEXT);
        cv.line(ix, iy - 0.6 * s, ix, iy + 3.4 * s, 1.6 * s, TEXT);

        let (cx, cy) = lay.close_center();
        if self.hover == Some(Hit::Close) {
            cv.circle(cx, cy, r, HOVER);
        }
        let d = 4.5 * s;
        cv.line(cx - d, cy - d, cx + d, cy + d, 1.6 * s, TEXT);
        cv.line(cx - d, cy + d, cx + d, cy - d, 1.6 * s, TEXT);

        buffer.present().unwrap();
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
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
        let window = Rc::new(el.create_window(attrs).unwrap());
        let context = softbuffer::Context::new(window.clone()).unwrap();
        let surface = softbuffer::Surface::new(&context, window.clone()).unwrap();
        window.request_redraw();
        self.window = Some(window);
        self.context = Some(context);
        self.surface = Some(surface);
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => el.exit(),
            WindowEvent::KeyboardInput { event, .. }
                if event.state.is_pressed() && !event.repeat =>
            {
                match event.physical_key {
                    PhysicalKey::Code(KeyCode::Escape) => el.exit(),
                    PhysicalKey::Code(KeyCode::Digit0) | PhysicalKey::Code(KeyCode::Numpad0) => {
                        self.view = View::default();
                        self.redraw();
                    }
                    PhysicalKey::Code(KeyCode::KeyI) => {
                        self.sidebar_open = !self.sidebar_open;
                        self.redraw();
                    }
                    _ => {}
                }
            }
            WindowEvent::Resized(_) => self.redraw(),
            WindowEvent::MouseWheel { delta, .. } => {
                let steps = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32 / 40.0,
                };
                self.zoom_at_cursor(steps);
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
                    Some(Hit::Info) => {
                        self.sidebar_open = !self.sidebar_open;
                        self.redraw();
                    }
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
                    self.view.pan.0 += (new.0 - self.cursor.0) as f32;
                    self.view.pan.1 += (new.1 - self.cursor.1) as f32;
                    self.redraw();
                }
                self.cursor = new;
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
