use crate::{
    ACCENT, DIM, R_BTN, R_MENU, R_PILL, SHADOW_PAD, TEXT,
    draw::{Canvas, Rgb, lerp, text_width, wrap},
    fonts::Fonts,
    i18n::tr,
    surface,
};

const BTN_BG: Rgb = [0x45, 0x49, 0x51];
const BTN_BG_HOVER: Rgb = [0x55, 0x5a, 0x64];
const ON_ACCENT: Rgb = [0x1c, 0x1f, 0x26];

type Rect = (f32, f32, f32, f32);

fn contains(r: Rect, p: (f32, f32)) -> bool {
    p.0 >= r.0 && p.0 < r.0 + r.2 && p.1 >= r.1 && p.1 < r.1 + r.3
}

fn button_width(font: &Fonts, label: &str, s: f32) -> f32 {
    text_width(font, label, 13.5 * s) + 32.0 * s
}

fn draw_button(
    cv: &mut Canvas,
    font: &Fonts,
    r: Rect,
    label: &str,
    primary: bool,
    hover: bool,
    s: f32,
) {
    let acc = ACCENT;
    let fill = match (primary, hover) {
        (true, false) => acc,
        (true, true) => lerp(acc, [255, 255, 255], 0.28),
        (false, false) => BTN_BG,
        (false, true) => BTN_BG_HOVER,
    };
    cv.round_rect(r.0, r.1, r.2, r.3, R_BTN * s, fill);
    let px = 13.5 * s;
    let tw = text_width(font, label, px);
    if let Some(lm) = font.horizontal_line_metrics(px) {
        let baseline = r.1 + r.3 / 2.0 + (lm.ascent + lm.descent) / 2.0;
        let color = if primary { ON_ACCENT } else { TEXT };
        cv.text(font, label, r.0 + (r.2 - tw) / 2.0, baseline, px, color);
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum DialogAction {
    Cancel,
    Discard,
    Save,
}

pub struct Dialog {
    title: &'static str,
    lines: Vec<String>,
    buttons: Vec<(&'static str, DialogAction, Rect)>,
    pub w: i32,
    pub h: i32,
    scale: f32,
    hover: Option<usize>,
}

impl Dialog {
    pub fn unsaved(font: &Fonts, scale: f32) -> Self {
        let s = scale;
        let w = 400.0 * s;
        let pad = 22.0 * s;
        let (title_h, line_h, btn_h, gap) = (24.0 * s, 19.0 * s, 34.0 * s, 8.0 * s);
        let lines = wrap(font, tr("dialog.text"), 13.5 * s, w - 2.0 * pad);
        let h = pad + title_h + lines.len() as f32 * line_h + 20.0 * s + btn_h + pad;

        let specs = [
            (tr("dialog.cancel"), DialogAction::Cancel),
            (tr("dialog.discard"), DialogAction::Discard),
            (tr("dialog.save"), DialogAction::Save),
        ];
        let mut buttons = Vec::new();
        let mut x = w - pad;
        for (label, action) in specs.iter().rev() {
            let bw = button_width(font, label, s);
            x -= bw;
            buttons.push((*label, *action, (x, h - pad - btn_h, bw, btn_h)));
            x -= gap;
        }
        buttons.reverse();

        Self {
            title: tr("dialog.title"),
            lines,
            buttons,
            w: w.round() as i32,
            h: h.round() as i32,
            scale,
            hover: None,
        }
    }

    pub fn pad(&self) -> i32 {
        (SHADOW_PAD * self.scale).round() as i32
    }

    pub fn origin(&self, win: (u32, u32)) -> (i32, i32) {
        ((win.0 as i32 - self.w) / 2, (win.1 as i32 - self.h) / 2)
    }

    pub fn set_hover(&mut self, p: (f32, f32)) -> bool {
        let new = self.buttons.iter().position(|(_, _, r)| contains(*r, p));
        let changed = new != self.hover;
        self.hover = new;
        changed
    }

    pub fn action_at(&self, p: (f32, f32)) -> Option<DialogAction> {
        self.buttons
            .iter()
            .find(|(_, _, r)| contains(*r, p))
            .map(|(_, a, _)| *a)
    }

    pub fn draw(&self, cv: &mut Canvas, font: &Fonts) {
        let s = self.scale;
        surface(cv, 0.0, 0.0, self.w as f32, self.h as f32, R_MENU * s, s);

        let pad = 22.0 * s;
        let (title_h, line_h) = (24.0 * s, 19.0 * s);

        let tpx = 16.0 * s;
        if let Some(lm) = font.horizontal_line_metrics(tpx) {
            let baseline = pad + title_h / 2.0 + (lm.ascent + lm.descent) / 2.0;
            cv.text(font, self.title, pad, baseline, tpx, TEXT);
        }
        let px = 13.5 * s;
        if let Some(lm) = font.horizontal_line_metrics(px) {
            for (i, line) in self.lines.iter().enumerate() {
                let top = pad + title_h + i as f32 * line_h;
                let baseline = top + line_h / 2.0 + (lm.ascent + lm.descent) / 2.0;
                cv.text(font, line, pad, baseline, px, DIM);
            }
        }
        for (i, (label, action, r)) in self.buttons.iter().enumerate() {
            let primary = *action == DialogAction::Save;
            draw_button(cv, font, *r, label, primary, self.hover == Some(i), s);
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum BarAction {
    Cancel,
    Apply,
}

pub struct CropBar {
    buttons: Vec<(&'static str, BarAction, Rect)>,
    pub label: String,
    pub w: i32,
    pub h: i32,
    scale: f32,
    hover: Option<usize>,
}

impl CropBar {
    pub fn new(font: &Fonts, scale: f32) -> Self {
        let s = scale;
        let (pad, h, btn_h, gap) = (16.0 * s, 46.0 * s, 30.0 * s, 8.0 * s);
        let label_w = text_width(font, "00000 × 00000", 13.5 * s);
        let cancel = tr("dialog.cancel");
        let apply = tr("crop.apply");
        let (cw, aw) = (button_width(font, cancel, s), button_width(font, apply, s));
        let x_cancel = pad + label_w + 18.0 * s;
        let x_apply = x_cancel + cw + gap;
        let side = (h - btn_h) / 2.0;
        let w = x_apply + aw + side;
        let y = side;
        Self {
            buttons: vec![
                (cancel, BarAction::Cancel, (x_cancel, y, cw, btn_h)),
                (apply, BarAction::Apply, (x_apply, y, aw, btn_h)),
            ],
            label: String::new(),
            w: w.round() as i32,
            h: h.round() as i32,
            scale,
            hover: None,
        }
    }

    pub fn pad(&self) -> i32 {
        (SHADOW_PAD * self.scale).round() as i32
    }

    pub fn origin(&self, vp: (i32, i32, u32, u32)) -> (i32, i32) {
        (
            vp.0 + (vp.2 as i32 - self.w) / 2,
            vp.1 + vp.3 as i32 - self.h - (20.0 * self.scale) as i32,
        )
    }

    pub fn set_hover(&mut self, p: (f32, f32)) -> bool {
        let new = self.buttons.iter().position(|(_, _, r)| contains(*r, p));
        let changed = new != self.hover;
        self.hover = new;
        changed
    }

    pub fn action_at(&self, p: (f32, f32)) -> Option<BarAction> {
        self.buttons
            .iter()
            .find(|(_, _, r)| contains(*r, p))
            .map(|(_, a, _)| *a)
    }

    pub fn draw(&self, cv: &mut Canvas, font: &Fonts) {
        let s = self.scale;
        surface(cv, 0.0, 0.0, self.w as f32, self.h as f32, R_PILL * s, s);

        let px = 13.5 * s;
        if let Some(lm) = font.horizontal_line_metrics(px) {
            let baseline = self.h as f32 / 2.0 + (lm.ascent + lm.descent) / 2.0;
            cv.text(font, &self.label, 16.0 * s, baseline, px, TEXT);
        }
        for (i, (label, action, r)) in self.buttons.iter().enumerate() {
            let primary = *action == BarAction::Apply;
            draw_button(cv, font, *r, label, primary, self.hover == Some(i), s);
        }
    }
}
