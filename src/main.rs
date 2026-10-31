use std::{num::NonZeroU32, path::PathBuf, rc::Rc};

use image::{
    RgbaImage,
    imageops::{self, FilterType},
};
use winit::{
    application::ApplicationHandler,
    event::{MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::{Key, NamedKey},
    window::{Window, WindowId},
};

type Surface = softbuffer::Surface<Rc<Window>, Rc<Window>>;

const BG: [u8; 3] = [0x1e, 0x1e, 0x2e];

fn fit_scale((iw, ih): (u32, u32), (w, h): (u32, u32)) -> f32 {
    (w as f32 / iw as f32).min(h as f32 / ih as f32).min(1.0)
}

#[derive(Default)]
struct View {
    zoom: Option<f32>,
    pan: (f32, f32),
}

impl View {
    fn scale(&self, img: (u32, u32), win: (u32, u32)) -> f32 {
        self.zoom.unwrap_or_else(|| fit_scale(img, win))
    }

    fn clamp_pan(&mut self, img: (u32, u32), win: (u32, u32)) {
        let s = self.scale(img, win);
        let max_x = ((img.0 as f32 * s - win.0 as f32) / 2.0).max(0.0);
        let max_y = ((img.1 as f32 * s - win.1 as f32) / 2.0).max(0.0);
        self.pan.0 = self.pan.0.clamp(-max_x, max_x);
        self.pan.1 = self.pan.1.clamp(-max_y, max_y);
    }
}

#[derive(Default)]
struct App {
    window: Option<Rc<Window>>,
    context: Option<softbuffer::Context<Rc<Window>>>,
    surface: Option<Surface>,
    path: Option<PathBuf>,
    image: Option<RgbaImage>,
    scaled: Option<RgbaImage>,
    view: View,
    cursor: (f64, f64),
    dragging: bool,
}

impl App {
    fn redraw(&self) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    fn zoom_at_cursor(&mut self, steps: f32) {
        let (Some(window), Some(img)) = (&self.window, &self.image) else {
            return;
        };
        let size = window.inner_size();
        let win = (size.width, size.height);
        let imgd = img.dimensions();

        let old = self.view.scale(imgd, win);
        let min = fit_scale(imgd, win);
        let new = (old * 1.1f32.powf(steps)).clamp(min, 32.0);

        let cx = self.cursor.0 as f32 - win.0 as f32 / 2.0;
        let cy = self.cursor.1 as f32 - win.1 as f32 / 2.0;
        let rx = (cx - self.view.pan.0) / old;
        let ry = (cy - self.view.pan.1) / old;

        self.view.pan = (cx - rx * new, cy - ry * new);
        self.view.zoom = if (new - min).abs() < 1e-3 {
            None
        } else {
            Some(new)
        };
        self.view.clamp_pan(imgd, win);
        window.request_redraw();
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

        let attrs = Window::default_attributes().with_title(title);
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
            WindowEvent::KeyboardInput { event, .. } if event.state.is_pressed() => {
                match &event.logical_key {
                    Key::Named(NamedKey::Escape) => el.exit(),
                    Key::Character(c) if c.as_str() == "0" => {
                        self.view = View::default();
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
            } => self.dragging = state.is_pressed(),
            WindowEvent::CursorMoved { position, .. } => {
                let new = (position.x, position.y);
                if self.dragging && self.image.is_some() {
                    self.view.pan.0 += (new.0 - self.cursor.0) as f32;
                    self.view.pan.1 += (new.1 - self.cursor.1) as f32;
                    self.redraw();
                }
                self.cursor = new;
            }
            WindowEvent::RedrawRequested => {
                let (Some(window), Some(surface)) = (&self.window, &mut self.surface) else {
                    return;
                };
                let size = window.inner_size();
                let (Some(nw), Some(nh)) =
                    (NonZeroU32::new(size.width), NonZeroU32::new(size.height))
                else {
                    return;
                };
                let (w, h) = (size.width, size.height);

                surface.resize(nw, nh).unwrap();
                let mut buffer = surface.buffer_mut().unwrap();
                let pixels: &mut [u32] = &mut buffer;
                pixels.fill(u32::from_be_bytes([0, BG[0], BG[1], BG[2]]));

                if let Some(img) = &self.image {
                    let (iw, ih) = img.dimensions();
                    self.view.clamp_pan((iw, ih), (w, h));
                    let scale = self.view.scale((iw, ih), (w, h));

                    let dw = ((iw as f32 * scale).round() as u32).max(1);
                    let dh = ((ih as f32 * scale).round() as u32).max(1);

                    let src: &RgbaImage = if (dw, dh) == (iw, ih) || scale > 1.0 {
                        img
                    } else {
                        if self.scaled.as_ref().map(|s| s.dimensions()) != Some((dw, dh)) {
                            self.scaled = Some(imageops::resize(img, dw, dh, FilterType::Triangle));
                        }
                        self.scaled.as_ref().unwrap()
                    };
                    let k = if std::ptr::eq(src, img) { scale } else { 1.0 };

                    let (sw, sh) = src.dimensions();
                    let ox = w as f32 / 2.0 + self.view.pan.0 - sw as f32 * k / 2.0;
                    let oy = h as f32 / 2.0 + self.view.pan.1 - sh as f32 * k / 2.0;
                    let x_lo = ox.max(0.0).floor() as u32;
                    let x_hi = (ox + sw as f32 * k).ceil().min(w as f32) as u32;
                    let y_lo = oy.max(0.0).floor() as u32;
                    let y_hi = (oy + sh as f32 * k).ceil().min(h as f32) as u32;

                    for y in y_lo..y_hi {
                        let sy = (((y as f32 + 0.5 - oy) / k).floor().max(0.0) as u32).min(sh - 1);
                        for x in x_lo..x_hi {
                            let sx =
                                (((x as f32 + 0.5 - ox) / k).floor().max(0.0) as u32).min(sw - 1);
                            let [r, g, b, a] = src.get_pixel(sx, sy).0;
                            let a = a as u32;
                            let mix = |c: u8, bg: u8| (c as u32 * a + bg as u32 * (255 - a)) / 255;
                            pixels[(y * w + x) as usize] =
                                mix(r, BG[0]) << 16 | mix(g, BG[1]) << 8 | mix(b, BG[2]);
                        }
                    }
                }
                buffer.present().unwrap();
            }
            _ => {}
        }
    }
}

fn main() {
    let mut app = App::default();

    if let Some(path) = std::env::args_os().nth(1).map(PathBuf::from) {
        match image::open(&path) {
            Ok(img) => {
                app.image = Some(img.to_rgba8());
                app.path = Some(path);
            }
            Err(e) => eprintln!("mithril: {}: {e}", path.display()),
        }
    }

    let event_loop = EventLoop::new().unwrap();
    event_loop.run_app(&mut app).unwrap();
}
