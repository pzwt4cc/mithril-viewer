#![allow(unsafe_op_in_unsafe_fn)]

use std::{num::NonZeroU32, rc::Rc};

use glow::HasContext;
use glutin::{
    config::ConfigTemplateBuilder,
    context::{ContextApi, ContextAttributesBuilder, PossiblyCurrentContext, Version},
    display::GetGlDisplay,
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

pub const HEADER: usize = 0;
pub const SIDEBAR: usize = 1;
pub const MENU: usize = 2;
pub const TOAST: usize = 3;
pub const BG: usize = 4;
pub const BLACK: usize = 5;
pub const WHITE: usize = 6;
pub const BAR: usize = 7;
pub const DIALOG: usize = 8;
pub const EMPTY: usize = 9;
pub const RATIOS: usize = 10;
const OVERLAYS: usize = 11;

const VERT: &str = r#"#version 300 es
uniform vec4 u_rect;
uniform vec2 u_res;
uniform float u_angle;
out vec2 v_uv;
void main() {
    vec2 c = vec2(float(gl_VertexID & 1), float((gl_VertexID >> 1) & 1));
    v_uv = c;
    vec2 local = (c - 0.5) * u_rect.zw;
    float cs = cos(u_angle);
    float sn = sin(u_angle);
    vec2 rot = vec2(local.x * cs - local.y * sn, local.x * sn + local.y * cs);
    vec2 p = u_rect.xy + 0.5 * u_rect.zw + rot;
    gl_Position = vec4(p.x / u_res.x * 2.0 - 1.0, 1.0 - p.y / u_res.y * 2.0, 0.0, 1.0);
}
"#;

const FRAG: &str = r#"#version 300 es
precision highp float;
uniform sampler2D u_tex;
uniform float u_alpha;
uniform vec2 u_res;
uniform float u_corner;
uniform float u_checker;
uniform vec3 u_chk0;
uniform vec3 u_chk1;
in vec2 v_uv;
out vec4 o_color;
void main() {
    vec4 c = texture(u_tex, v_uv);
    if (u_checker > 0.5) {
        uvec2 grid = uvec2(gl_FragCoord.xy) / 12u;
        float check = float((grid.x + grid.y) & 1u);
        vec3 chk = mix(u_chk0, u_chk1, check);
        c.rgb = mix(chk, c.rgb, c.a);
        c.a = 1.0;
    }
    float a = c.a * u_alpha;
    if (u_corner > 0.0) {
        vec2 p = vec2(gl_FragCoord.x, u_res.y - gl_FragCoord.y) - 0.5 * u_res;
        vec2 q = abs(p) - (0.5 * u_res - vec2(u_corner));
        float d = length(max(q, vec2(0.0))) + min(max(q.x, q.y), 0.0) - u_corner;
        a *= clamp(0.5 - d, 0.0, 1.0);
    }
    o_color = vec4(c.rgb, a);
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
    u_alpha: Option<glow::UniformLocation>,
    u_angle: Option<glow::UniformLocation>,
    u_corner: Option<glow::UniformLocation>,
    u_checker: Option<glow::UniformLocation>,
    u_chk0: Option<glow::UniformLocation>,
    u_chk1: Option<glow::UniformLocation>,
    max_tex: u32,
    win: (u32, u32),
    image: Option<Tex>,
    overlays: [Option<Tex>; OVERLAYS],
}

impl Gpu {
    pub fn new(
        el: &ActiveEventLoop,
        attrs: WindowAttributes,
    ) -> Result<(Rc<Window>, Self), String> {
        let (window, config) = DisplayBuilder::new()
            .with_window_attributes(Some(attrs.clone()))
            .with_preference(ApiPreference::PreferEgl)
            .build(
                el,
                ConfigTemplateBuilder::new().with_alpha_size(8),
                |configs| {
                    let mut first = None;
                    for c in configs {
                        if c.supports_transparency().unwrap_or(false) {
                            return c;
                        }
                        if first.is_none() {
                            first = Some(c);
                        }
                    }
                    first.expect("нет подходящей GL-конфигурации")
                },
            )
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
        let (program, vao, u_rect, u_res, u_alpha, u_angle, u_corner, u_checker, u_chk0, u_chk1, max_tex) = unsafe {
            let program = build_program(&gl)?;
            let vao = gl.create_vertex_array()?;
            let u_rect = gl.get_uniform_location(program, "u_rect");
            let u_res = gl.get_uniform_location(program, "u_res");
            let u_alpha = gl.get_uniform_location(program, "u_alpha");
            let u_angle = gl.get_uniform_location(program, "u_angle");
            let u_corner = gl.get_uniform_location(program, "u_corner");
            let u_checker = gl.get_uniform_location(program, "u_checker");
            let u_chk0 = gl.get_uniform_location(program, "u_chk0");
            let u_chk1 = gl.get_uniform_location(program, "u_chk1");
            let max_tex = gl.get_parameter_i32(glow::MAX_TEXTURE_SIZE).max(1024) as u32;
            (
                program, vao, u_rect, u_res, u_alpha, u_angle, u_corner, u_checker, u_chk0, u_chk1, max_tex,
            )
        };

        let gpu = Self {
            gl,
            ctx,
            surface,
            program,
            vao,
            u_rect,
            u_res,
            u_alpha,
            u_angle,
            u_corner,
            u_checker,
            u_chk0,
            u_chk1,
            max_tex,
            win: (1, 1),
            image: None,
            overlays: Default::default(),
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

    pub fn set_overlay(&mut self, slot: usize, px: &[u32], w: u32, h: u32) {
        unsafe {
            upload(
                &self.gl,
                &mut self.overlays[slot],
                w as i32,
                h as i32,
                &to_rgba(px),
                false,
            )
        }
    }

    pub fn overlay_size(&self, slot: usize) -> Option<(i32, i32)> {
        self.overlays[slot].as_ref().map(|t| (t.w, t.h))
    }

    pub fn begin(&mut self, w: u32, h: u32, corner: f32) {
        self.win = (w, h);
        unsafe {
            let gl = &self.gl;
            gl.viewport(0, 0, w as i32, h as i32);
            gl.disable(glow::SCISSOR_TEST);
            gl.clear_color(0.0, 0.0, 0.0, 0.0);
            gl.clear(glow::COLOR_BUFFER_BIT);
            gl.enable(glow::BLEND);
            gl.blend_func_separate(
                glow::SRC_ALPHA,
                glow::ONE_MINUS_SRC_ALPHA,
                glow::ONE,
                glow::ONE_MINUS_SRC_ALPHA,
            );
            gl.use_program(Some(self.program));
            gl.bind_vertex_array(Some(self.vao));
            gl.active_texture(glow::TEXTURE0);
            gl.uniform_2_f32(self.u_res.as_ref(), w as f32, h as f32);
            gl.uniform_1_f32(self.u_corner.as_ref(), corner);
        }
    }

    pub fn draw_image(
        &self,
        rect: (f32, f32, f32, f32),
        clip: (i32, i32, u32, u32),
        alpha: f32,
        angle: f32,
        checker: bool,
        is_light: bool,
    ) {
        let Some(tex) = &self.image else { return };
        unsafe {
            self.gl.enable(glow::SCISSOR_TEST);
            self.gl.scissor(
                clip.0,
                self.win.1 as i32 - (clip.1 + clip.3 as i32),
                clip.2 as i32,
                clip.3 as i32,
            );
            if checker {
                self.gl.uniform_1_f32(self.u_checker.as_ref(), 1.0);
                if is_light {
                    self.gl
                        .uniform_3_f32(self.u_chk0.as_ref(), 0.86, 0.88, 0.92);
                    self.gl
                        .uniform_3_f32(self.u_chk1.as_ref(), 0.94, 0.95, 0.98);
                } else {
                    self.gl
                        .uniform_3_f32(self.u_chk0.as_ref(), 0.18, 0.20, 0.24);
                    self.gl
                        .uniform_3_f32(self.u_chk1.as_ref(), 0.24, 0.26, 0.31);
                }
            } else {
                self.gl.uniform_1_f32(self.u_checker.as_ref(), 0.0);
            }
            self.gl.bind_texture(glow::TEXTURE_2D, Some(tex.id));
            self.gl
                .uniform_4_f32(self.u_rect.as_ref(), rect.0, rect.1, rect.2, rect.3);
            self.gl.uniform_1_f32(self.u_alpha.as_ref(), alpha);
            self.gl.uniform_1_f32(self.u_angle.as_ref(), angle);
            self.gl.uniform_1_f32(self.u_checker.as_ref(), 0.0);
            self.gl.draw_arrays(glow::TRIANGLE_STRIP, 0, 4);
            self.gl.uniform_1_f32(self.u_checker.as_ref(), 0.0);
            self.gl.disable(glow::SCISSOR_TEST);
        }
    }

    pub fn draw_overlay(&self, slot: usize, x: i32, y: i32, alpha: f32) {
        if let Some(tex) = &self.overlays[slot] {
            self.quad(
                tex,
                (x as f32, y as f32, tex.w as f32, tex.h as f32),
                alpha,
                0.0,
            );
        }
    }

    pub fn draw_overlay_scaled(&self, slot: usize, rect: (f32, f32, f32, f32), alpha: f32) {
        if let Some(tex) = &self.overlays[slot] {
            self.quad(tex, rect, alpha, 0.0);
        }
    }

    pub fn present(&self) {
        let _ = self.surface.swap_buffers(&self.ctx);
    }

    fn quad(&self, tex: &Tex, r: (f32, f32, f32, f32), alpha: f32, angle: f32) {
        unsafe {
            self.gl.bind_texture(glow::TEXTURE_2D, Some(tex.id));
            self.gl
                .uniform_4_f32(self.u_rect.as_ref(), r.0, r.1, r.2, r.3);
            self.gl.uniform_1_f32(self.u_alpha.as_ref(), alpha);
            self.gl.uniform_1_f32(self.u_angle.as_ref(), angle);
            self.gl.draw_arrays(glow::TRIANGLE_STRIP, 0, 4);
        }
    }
}

fn to_rgba(px: &[u32]) -> Vec<u8> {
    let mut v = Vec::with_capacity(px.len() * 4);
    for &p in px {
        v.extend_from_slice(&[(p >> 16) as u8, (p >> 8) as u8, p as u8, (p >> 24) as u8]);
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
