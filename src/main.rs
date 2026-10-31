use std::{num::NonZeroU32, path::PathBuf, rc::Rc};

use image::{
    RgbaImage,
    imageops::{self, FilterType},
};
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::{Key, NamedKey},
    window::{Window, WindowId},
};

type Surface = softbuffer::Surface<Rc<Window>, Rc<Window>>;

const BG: [u8; 3] = [0x1e, 0x1e, 0x2e];

#[derive(Default)]
struct App {
    window: Option<Rc<Window>>,
    context: Option<softbuffer::Context<Rc<Window>>>,
    surface: Option<Surface>,
    path: Option<PathBuf>,
    image: Option<RgbaImage>,
    scaled: Option<RgbaImage>,
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
            WindowEvent::KeyboardInput { event, .. }
                if event.state.is_pressed()
                    && event.logical_key == Key::Named(NamedKey::Escape) =>
            {
                el.exit()
            }
            WindowEvent::Resized(_) => {
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
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

                if let Some(img) = &self.image {
                    let (iw, ih) = img.dimensions();
                    let scale = (w as f32 / iw as f32).min(h as f32 / ih as f32).min(1.0);
                    let dw = ((iw as f32 * scale).round() as u32).clamp(1, w);
                    let dh = ((ih as f32 * scale).round() as u32).clamp(1, h);
                    if self.scaled.as_ref().map(|s| s.dimensions()) != Some((dw, dh)) {
                        self.scaled = Some(if (dw, dh) == (iw, ih) {
                            img.clone()
                        } else {
                            imageops::resize(img, dw, dh, FilterType::Triangle)
                        });
                    }
                }

                surface.resize(nw, nh).unwrap();
                let mut buffer = surface.buffer_mut().unwrap();
                let pixels: &mut [u32] = &mut buffer;
                pixels.fill(u32::from_be_bytes([0, BG[0], BG[1], BG[2]]));

                if let Some(s) = &self.scaled {
                    let (dw, dh) = s.dimensions();
                    let (ox, oy) = ((w - dw) / 2, (h - dh) / 2);
                    for y in 0..dh {
                        let start = ((oy + y) * w + ox) as usize;
                        let row = &mut pixels[start..start + dw as usize];
                        for (x, px) in row.iter_mut().enumerate() {
                            let [r, g, b, a] = s.get_pixel(x as u32, y).0;
                            let a = a as u32;
                            let mix = |c: u8, bg: u8| (c as u32 * a + bg as u32 * (255 - a)) / 255;
                            *px = mix(r, BG[0]) << 16 | mix(g, BG[1]) << 8 | mix(b, BG[2]);
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
