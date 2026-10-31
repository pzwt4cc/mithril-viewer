#![allow(unsafe_op_in_unsafe_fn)]

use glutin::display::GetGlDisplay;
use std::{num::NonZeroU32, rc::Rc};

use glow::HasContext;
use glutin::{
    config::ConfigTemplateBuilder,
    context::{ContextApi, ContextAttributesBuilder, PossiblyCurrentContext, Version},
    prelude::*,
    surface::{Surface, SwapInterval, WindowSurface},
};
use glutin_winit::{ApiPreference, DisplayBuilder, GlWindow, finalize_window};
use image::{
    RgbaImage,
    imageops::{self, FilterType},
};
use winit::{
    event_loop::ActiveEventLoop,
    raw_window_handle::HasWindowHandle,
    window::{Window, WindowAttributes},
};

use crate::draw::Rgb;

const VERT: &str = r#"#version 300 es
uniform vec4 u_rect;
uniform vec2 u_res;
out vec2 v_uv;
void main() {
    vec2 c = vec2(float(gl_VertexID & 1), float((gl_VertexID >> 1) & 1));
    v_uv = c;
    vec2 p = u_rect.xy + c * u_rect.zw;
    gl_Position = vec4(p.x / u_res.x * 2.0 - 1.0, 1.0 - p.y / u_res.y * 2.0, 0.0, 1.0);
}
"#;

const FRAG: &str = r#"#version 300 es
precision mediump float;
uniform sampler2D u_tex;
in vec2 v_uv;
out vec4 o_color;
void main() {
    o_color = texture(u_tex, v_uv);
}
"#;

struct Tex {
    id: glow::Texture,
    w: i32,
    h: i32,
}

pub struct Gpu {
    gl: glow::Context,
    ctx: PossiblyCurrentContext,
    surface: Surface<WindowSurface>,
    program: glow::Program,
    vao: glow::VertexArray,
    u_rect: Option<glow::UniformLocation>,
    u_res: Option<glow::UniformLocation>,
    max_tex: u32,
    win: (u32, u32),
    image: Option<Tex>,
    header: Option<Tex>,
    sidebar: Option<Tex>,
}

impl Gpu {
    pub fn new(
        el: &ActiveEventLoop,
        attrs: WindowAttributes,
    ) -> Result<(Rc<Window>, Self), String> {
        let (window, config) = DisplayBuilder::new()
            .with_window_attributes(Some(attrs.clone()))
            .with_preference(ApiPreference::PreferEgl)
            .build(el, ConfigTemplateBuilder::new(), |mut configs| {
                configs.next().expect("нет подходящей GL-конфигурации")
            })
            .map_err(|e| e.to_string())?;
        let window = match window {
            Some(w) => w,
            None => finalize_window(el, attrs, &config).map_err(|e| e.to_string())?,
        };

        let display = config.display();
        let raw = window.window_handle().ok().map(|h| h.as_raw());
        let ctx_attrs = ContextAttributesBuilder::new()
            .with_context_api(ContextApi::Gles(Some(Version::new(3, 0))))
            .build(raw);
        let surf_attrs = window
            .build_surface_attributes(Default::default())
            .map_err(|e| e.to_string())?;

        let (surface, ctx) = unsafe {
            let not_current = display
                .create_context(&config, &ctx_attrs)
                .map_err(|e| format!("контекст OpenGL ES 3.0: {e}"))?;
            let surface = display
                .create_window_surface(&config, &surf_attrs)
                .map_err(|e| e.to_string())?;
            let ctx = not_current
                .make_current(&surface)
                .map_err(|e| e.to_string())?;
            (surface, ctx)
        };

        let _ = surface.set_swap_interval(&ctx, SwapInterval::Wait(NonZeroU32::new(1).unwrap()));

        let gl =
            unsafe { glow::Context::from_loader_function_cstr(|s| display.get_proc_address(s)) };
        let (program, vao, u_rect, u_res, max_tex) = unsafe {
            let program = build_program(&gl)?;
            let vao = gl.create_vertex_array()?;
            let u_rect = gl.get_uniform_location(program, "u_rect");
            let u_res = gl.get_uniform_location(program, "u_res");
            let max_tex = gl.get_parameter_i32(glow::MAX_TEXTURE_SIZE).max(1024) as u32;
            (program, vao, u_rect, u_res, max_tex)
        };

        let gpu = Self {
            gl,
            ctx,
            surface,
            program,
            vao,
            u_rect,
            u_res,
            max_tex,
            win: (1, 1),
            image: None,
            header: None,
            sidebar: None,
        };
        Ok((Rc::new(window), gpu))
    }

    pub fn resize(&mut self, w: u32, h: u32) {
        if let (Some(w), Some(h)) = (NonZeroU32::new(w), NonZeroU32::new(h)) {
            self.surface.resize(&self.ctx, w, h);
        }
    }

    pub fn set_image(&mut self, img: &RgbaImage) {
        let (w, h) = img.dimensions();
        let tmp;
        let img = if w.max(h) > self.max_tex {
            let k = self.max_tex as f32 / w.max(h) as f32;
            tmp = imageops::resize(
                img,
                ((w as f32 * k) as u32).max(1),
                ((h as f32 * k) as u32).max(1),
                FilterType::Triangle,
            );
            &tmp
        } else {
            img
        };
        unsafe {
            upload(
                &self.gl,
                &mut self.image,
                img.width() as i32,
                img.height() as i32,
                img.as_raw(),
                true,
            );
        }
    }

    pub fn set_header(&mut self, px: &[u32], w: u32, h: u32) {
        unsafe {
            upload(
                &self.gl,
                &mut self.header,
                w as i32,
                h as i32,
                &to_rgba(px),
                false,
            )
        }
    }

    pub fn set_sidebar(&mut self, px: &[u32], w: u32, h: u32) {
        unsafe {
            upload(
                &self.gl,
                &mut self.sidebar,
                w as i32,
                h as i32,
                &to_rgba(px),
                false,
            )
        }
    }

    pub fn begin(&mut self, w: u32, h: u32, bg: Rgb) {
        self.win = (w, h);
        unsafe {
            let gl = &self.gl;
            gl.viewport(0, 0, w as i32, h as i32);
            gl.disable(glow::SCISSOR_TEST);
            gl.clear_color(
                bg[0] as f32 / 255.0,
                bg[1] as f32 / 255.0,
                bg[2] as f32 / 255.0,
                1.0,
            );
            gl.clear(glow::COLOR_BUFFER_BIT);
            gl.enable(glow::BLEND);
            gl.blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);
            gl.use_program(Some(self.program));
            gl.bind_vertex_array(Some(self.vao));
            gl.active_texture(glow::TEXTURE0);
            gl.uniform_2_f32(self.u_res.as_ref(), w as f32, h as f32);
        }
    }

    pub fn draw_image(&self, rect: (f32, f32, f32, f32), clip: (i32, i32, u32, u32)) {
        let Some(tex) = &self.image else { return };
        unsafe {
            self.gl.enable(glow::SCISSOR_TEST);
            self.gl.scissor(
                clip.0,
                self.win.1 as i32 - (clip.1 + clip.3 as i32),
                clip.2 as i32,
                clip.3 as i32,
            );
        }
        self.quad(tex, rect);
        unsafe { self.gl.disable(glow::SCISSOR_TEST) };
    }

    pub fn draw_header(&self) {
        if let Some(tex) = &self.header {
            self.quad(tex, (0.0, 0.0, tex.w as f32, tex.h as f32));
        }
    }

    pub fn draw_sidebar(&self, x: i32, y: i32) {
        if let Some(tex) = &self.sidebar {
            self.quad(tex, (x as f32, y as f32, tex.w as f32, tex.h as f32));
        }
    }

    pub fn present(&self) {
        let _ = self.surface.swap_buffers(&self.ctx);
    }

    fn quad(&self, tex: &Tex, r: (f32, f32, f32, f32)) {
        unsafe {
            self.gl.bind_texture(glow::TEXTURE_2D, Some(tex.id));
            self.gl
                .uniform_4_f32(self.u_rect.as_ref(), r.0, r.1, r.2, r.3);
            self.gl.draw_arrays(glow::TRIANGLE_STRIP, 0, 4);
        }
    }
}

fn to_rgba(px: &[u32]) -> Vec<u8> {
    let mut v = Vec::with_capacity(px.len() * 4);
    for &p in px {
        v.extend_from_slice(&[(p >> 16) as u8, (p >> 8) as u8, p as u8, 255]);
    }
    v
}

unsafe fn upload(
    gl: &glow::Context,
    slot: &mut Option<Tex>,
    w: i32,
    h: i32,
    rgba: &[u8],
    mip: bool,
) {
    let reuse = matches!(slot, Some(t) if t.w == w && t.h == h);
    if reuse {
        let t = slot.as_ref().unwrap();
        gl.bind_texture(glow::TEXTURE_2D, Some(t.id));
        gl.tex_sub_image_2d(
            glow::TEXTURE_2D,
            0,
            0,
            0,
            w,
            h,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
            glow::PixelUnpackData::Slice(Some(rgba)),
        );
    } else {
        if let Some(old) = slot.take() {
            gl.delete_texture(old.id);
        }
        let id = gl.create_texture().expect("не удалось создать текстуру");
        gl.bind_texture(glow::TEXTURE_2D, Some(id));
        let min = if mip {
            glow::LINEAR_MIPMAP_LINEAR
        } else {
            glow::LINEAR
        };
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, min as i32);
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_MAG_FILTER,
            glow::LINEAR as i32,
        );
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_WRAP_S,
            glow::CLAMP_TO_EDGE as i32,
        );
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_WRAP_T,
            glow::CLAMP_TO_EDGE as i32,
        );
        gl.tex_image_2d(
            glow::TEXTURE_2D,
            0,
            glow::RGBA8 as i32,
            w,
            h,
            0,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
            glow::PixelUnpackData::Slice(Some(rgba)),
        );
        *slot = Some(Tex { id, w, h });
    }
    if mip {
        gl.generate_mipmap(glow::TEXTURE_2D);
    }
}

unsafe fn build_program(gl: &glow::Context) -> Result<glow::Program, String> {
    let program = gl.create_program()?;
    let mut shaders = Vec::new();
    for (kind, src) in [(glow::VERTEX_SHADER, VERT), (glow::FRAGMENT_SHADER, FRAG)] {
        let sh = gl.create_shader(kind)?;
        gl.shader_source(sh, src);
        gl.compile_shader(sh);
        if !gl.get_shader_compile_status(sh) {
            return Err(gl.get_shader_info_log(sh));
        }
        gl.attach_shader(program, sh);
        shaders.push(sh);
    }
    gl.link_program(program);
    if !gl.get_program_link_status(program) {
        return Err(gl.get_program_info_log(program));
    }
    for sh in shaders {
        gl.detach_shader(program, sh);
        gl.delete_shader(sh);
    }
    Ok(program)
}
