use fontdue::Font;

pub type Rgb = [u8; 3];

pub struct Canvas<'a> {
    pub px: &'a mut [u32],
    pub w: u32,
    pub h: u32,
    pub ox: i32,
    pub oy: i32,
}

fn pack(c: Rgb) -> u32 {
    0xff00_0000 | (c[0] as u32) << 16 | (c[1] as u32) << 8 | c[2] as u32
}

impl Canvas<'_> {
    pub fn fill_rect(&mut self, x: i32, y: i32, w: i32, h: i32, c: Rgb) {
        let (x, y) = (x - self.ox, y - self.oy);
        let x0 = x.max(0);
        let y0 = y.max(0);
        let x1 = (x + w).min(self.w as i32);
        let y1 = (y + h).min(self.h as i32);
        if x0 >= x1 || y0 >= y1 {
            return;
        }
        let v = pack(c);
        for yy in y0..y1 {
            let row = (yy as u32 * self.w) as usize;
            self.px[row + x0 as usize..row + x1 as usize].fill(v);
        }
    }

    pub fn blend(&mut self, x: i32, y: i32, c: Rgb, cov: f32) {
        let (x, y) = (x - self.ox, y - self.oy);
        if cov <= 0.0 || x < 0 || y < 0 || x >= self.w as i32 || y >= self.h as i32 {
            return;
        }
        let i = (y as u32 * self.w + x as u32) as usize;
        let dst = self.px[i];
        let da = (dst >> 24) as f32 / 255.0;
        let sa = cov.min(1.0);
        let oa = sa + da * (1.0 - sa);
        if oa <= 0.0 {
            return;
        }
        let ch = |s: u8, shift: u32| {
            let d = ((dst >> shift) & 0xff) as f32;
            ((s as f32 * sa + d * da * (1.0 - sa)) / oa)
                .round()
                .min(255.0) as u32
        };
        self.px[i] = ((oa * 255.0).round() as u32) << 24
            | ch(c[0], 16) << 16
            | ch(c[1], 8) << 8
            | ch(c[2], 0);
    }

    fn shape(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, c: Rgb, f: impl Fn(f32, f32) -> f32) {
        for y in (y0.floor() as i32)..(y1.ceil() as i32) {
            for x in (x0.floor() as i32)..(x1.ceil() as i32) {
                let cov = f(x as f32 + 0.5, y as f32 + 0.5);
                self.blend(x, y, c, cov);
            }
        }
    }

    pub fn circle(&mut self, cx: f32, cy: f32, r: f32, c: Rgb) {
        self.shape(
            cx - r - 1.0,
            cy - r - 1.0,
            cx + r + 1.0,
            cy + r + 1.0,
            c,
            |x, y| {
                let d = ((x - cx).powi(2) + (y - cy).powi(2)).sqrt();
                (r + 0.5 - d).clamp(0.0, 1.0)
            },
        );
    }

    pub fn ring(&mut self, cx: f32, cy: f32, r: f32, t: f32, c: Rgb) {
        let o = r + t + 1.0;
        self.shape(cx - o, cy - o, cx + o, cy + o, c, |x, y| {
            let d = ((x - cx).powi(2) + (y - cy).powi(2)).sqrt();
            (t / 2.0 + 0.5 - (d - r).abs()).clamp(0.0, 1.0)
        });
    }

    pub fn line(&mut self, ax: f32, ay: f32, bx: f32, by: f32, t: f32, c: Rgb) {
        let (dx, dy) = (bx - ax, by - ay);
        let len2 = (dx * dx + dy * dy).max(1e-6);
        let o = t + 1.0;
        self.shape(
            ax.min(bx) - o,
            ay.min(by) - o,
            ax.max(bx) + o,
            ay.max(by) + o,
            c,
            |x, y| {
                let u = (((x - ax) * dx + (y - ay) * dy) / len2).clamp(0.0, 1.0);
                let (px, py) = (ax + u * dx, ay + u * dy);
                let d = ((x - px).powi(2) + (y - py).powi(2)).sqrt();
                (t / 2.0 + 0.5 - d).clamp(0.0, 1.0)
            },
        );
    }

    pub fn round_rect_alpha(&mut self, x: f32, y: f32, w: f32, h: f32, r: f32, c: Rgb, a: f32) {
        let (cx, cy) = (x + w / 2.0, y + h / 2.0);
        let (hx, hy) = (w / 2.0 - r, h / 2.0 - r);
        self.shape(x - 1.0, y - 1.0, x + w + 1.0, y + h + 1.0, c, |px, py| {
            let qx = (px - cx).abs() - hx;
            let qy = (py - cy).abs() - hy;
            let d = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt() + qx.max(qy).min(0.0) - r;
            (0.5 - d).clamp(0.0, 1.0) * a
        });
    }

    pub fn round_rect(&mut self, x: f32, y: f32, w: f32, h: f32, r: f32, c: Rgb) {
        self.round_rect_alpha(x, y, w, h, r, c, 1.0);
    }

    #[allow(clippy::too_many_arguments)]
    pub fn round_rect_grad(&mut self, x: f32, y: f32, w: f32, h: f32, r: f32, top: Rgb, bot: Rgb) {
        let (cx, cy) = (x + w / 2.0, y + h / 2.0);
        let (hx, hy) = (w / 2.0 - r, h / 2.0 - r);
        for yy in (y.floor() as i32 - 1)..((y + h).ceil() as i32 + 1) {
            let py = yy as f32 + 0.5;
            let c = lerp(top, bot, (py - y) / h.max(1.0));
            for xx in (x.floor() as i32 - 1)..((x + w).ceil() as i32 + 1) {
                let px = xx as f32 + 0.5;
                let qx = (px - cx).abs() - hx;
                let qy = (py - cy).abs() - hy;
                let d =
                    (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt() + qx.max(qy).min(0.0) - r;
                self.blend(xx, yy, c, (0.5 - d).clamp(0.0, 1.0));
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn shadow(&mut self, x: f32, y: f32, w: f32, h: f32, r: f32, blur: f32, alpha: f32) {
        let (cx, cy) = (x + w / 2.0, y + h / 2.0);
        let (hx, hy) = (w / 2.0 - r, h / 2.0 - r);
        self.shape(
            x - blur - 1.0,
            y - blur - 1.0,
            x + w + blur + 1.0,
            y + h + blur + 1.0,
            [0, 0, 0],
            |px, py| {
                let qx = (px - cx).abs() - hx;
                let qy = (py - cy).abs() - hy;
                let d =
                    (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt() + qx.max(qy).min(0.0) - r;
                let t = (1.0 - d / blur).clamp(0.0, 1.0);
                alpha * t * t * (3.0 - 2.0 * t)
            },
        );
    }

    pub fn text(&mut self, font: &Font, s: &str, x: f32, baseline: f32, px: f32, c: Rgb) {
        let mut pen = x;
        for ch in s.chars() {
            let (m, bitmap) = font.rasterize(ch, px);
            let gx = (pen + m.xmin as f32).round() as i32;
            let gy = (baseline - m.height as f32 - m.ymin as f32).round() as i32;
            for row in 0..m.height {
                for col in 0..m.width {
                    let cov = bitmap[row * m.width + col] as f32 / 255.0;
                    self.blend(gx + col as i32, gy + row as i32, c, cov);
                }
            }
            pen += m.advance_width;
        }
    }
}

pub fn text_width(font: &Font, s: &str, px: f32) -> f32 {
    s.chars().map(|c| font.metrics(c, px).advance_width).sum()
}

pub fn ellipsize(font: &Font, s: &str, px: f32, max_w: f32) -> String {
    if text_width(font, s, px) <= max_w {
        return s.to_owned();
    }
    let mut out = String::new();
    let mut w = text_width(font, "…", px);
    for ch in s.chars() {
        let cw = font.metrics(ch, px).advance_width;
        if w + cw > max_w {
            break;
        }
        w += cw;
        out.push(ch);
    }
    out.push('…');
    out
}

pub fn lerp(a: Rgb, b: Rgb, t: f32) -> Rgb {
    let t = t.clamp(0.0, 1.0);
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    [m(a[0], b[0]), m(a[1], b[1]), m(a[2], b[2])]
}

pub fn wrap(font: &Font, s: &str, px: f32, max_w: f32) -> Vec<String> {
    let mut lines = Vec::new();
    let mut cur = String::new();
    for word in s.split_whitespace() {
        let cand = if cur.is_empty() {
            word.to_owned()
        } else {
            format!("{cur} {word}")
        };
        if cur.is_empty() || text_width(font, &cand, px) <= max_w {
            cur = cand;
        } else {
            lines.push(std::mem::take(&mut cur));
            cur = word.to_owned();
        }
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    lines
}
