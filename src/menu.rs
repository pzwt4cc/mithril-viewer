use fontdue::Font;

use crate::{
    BORDER, DIM, Fade, HOVER, TEXT,
    draw::{Canvas, Rgb, lerp, text_width},
    info::labels,
};

const MENU_BG: Rgb = [0x2e, 0x2e, 0x2e];

const MENU_W: f32 = 230.0;
const ITEM_H: f32 = 34.0;
const MENU_PAD: f32 = 6.0;

#[derive(Clone, Copy, PartialEq)]
pub enum Action {
    ResetZoom,
    RotateLeft,
    RotateRight,
}

pub const ITEMS: [Action; 3] = [Action::ResetZoom, Action::RotateLeft, Action::RotateRight];

fn label(a: Action) -> (&'static str, &'static str) {
    let ru = labels().ru;
    match a {
        Action::ResetZoom => (
            if ru {
                "Сбросить масштаб"
            } else {
                "Reset zoom"
            },
            "0",
        ),
        Action::RotateLeft => (
            if ru {
                "Повернуть влево"
            } else {
                "Rotate left"
            },
            "Shift+R",
        ),
        Action::RotateRight => (
            if ru {
                "Повернуть вправо"
            } else {
                "Rotate right"
            },
            "R",
        ),
    }
}

pub struct Menu {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub hover: Option<usize>,
    pub appear: Fade,
    fades: [Fade; 3],
    scale: f32,
}

impl Menu {
    pub fn new(cursor: (f64, f64), win: (u32, u32), scale: f32) -> Self {
        let w = (MENU_W * scale).round() as i32;
        let h = ((ITEMS.len() as f32 * ITEM_H + 2.0 * MENU_PAD) * scale).round() as i32;
        let x = (cursor.0 as i32).min(win.0 as i32 - w).max(0);
        let y = (cursor.1 as i32).min(win.1 as i32 - h).max(0);
        Self {
            x,
            y,
            w,
            h,
            hover: None,
            appear: Fade { v: 0.0, to: 1.0 },
            fades: [Fade::new(0.0); 3],
            scale,
        }
    }

    pub fn set_hover(&mut self, item: Option<usize>) {
        self.hover = item;
        for (i, f) in self.fades.iter_mut().enumerate() {
            f.to = if item == Some(i) { 1.0 } else { 0.0 };
        }
    }

    pub fn step(&mut self, dt: f32) -> bool {
        let mut moved = self.appear.step(dt, 20.0);
        for f in self.fades.iter_mut() {
            moved |= f.step(dt, 24.0);
        }
        moved
    }

    pub fn item_at(&self, p: (f64, f64)) -> Option<usize> {
        let (px, py) = (p.0 - self.x as f64, p.1 - self.y as f64);
        if px < 0.0 || px >= self.w as f64 || py < 0.0 || py >= self.h as f64 {
            return None;
        }
        let rel = py / self.scale as f64 - MENU_PAD as f64;
        if rel < 0.0 {
            return None;
        }
        let i = (rel / ITEM_H as f64) as usize;
        (i < ITEMS.len()).then_some(i)
    }

    pub fn draw(&self, cv: &mut Canvas, font: &Font) {
        let s = self.scale;
        cv.fill_rect(self.x, self.y, self.w, self.h, BORDER);
        cv.fill_rect(self.x + 1, self.y + 1, self.w - 2, self.h - 2, MENU_BG);

        let px = 14.0 * s;
        let Some(lm) = font.horizontal_line_metrics(px) else {
            return;
        };
        for (i, &a) in ITEMS.iter().enumerate() {
            let ix = self.x as f32 + MENU_PAD * s;
            let iy = self.y as f32 + (MENU_PAD + i as f32 * ITEM_H) * s;
            let iw = self.w as f32 - 2.0 * MENU_PAD * s;
            let ih = ITEM_H * s;
            let f = self.fades[i].v;
            if f > 0.01 {
                cv.round_rect(ix, iy, iw, ih, 6.0 * s, lerp(MENU_BG, HOVER, f));
            }
            let baseline = iy + ih / 2.0 + (lm.ascent + lm.descent) / 2.0;
            let (text, hint) = label(a);
            cv.text(font, text, ix + 10.0 * s, baseline, px, TEXT);
            let hw = text_width(font, hint, px);
            cv.text(font, hint, ix + iw - 10.0 * s - hw, baseline, px, DIM);
        }
    }
}
