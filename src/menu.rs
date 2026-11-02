use crate::{
    BORDER, DIM, Fade, HOVER, R_ITEM, R_MENU, SHADOW_PAD, TEXT,
    config::{Accent, DeleteMode, accent},
    draw::{Canvas, text_width},
    fonts::Fonts,
    i18n::{available, tr},
    surface,
};

const MENU_W: f32 = 250.0;
const ITEM_H: f32 = 34.0;
const SECTION_H: f32 = 30.0;
const SEP_H: f32 = 9.0;
const SWATCH_H: f32 = 40.0;
const SWATCH_R: f32 = 9.0;
const MENU_PAD: f32 = 6.0;

#[derive(Clone, Copy, PartialEq)]
pub enum Action {
    ResetZoom,
    RotateLeft,
    RotateRight,
    Crop,
    Print,
    Copy,
    Delete,
    SetLanguage(&'static str),
    SetDeleteMode(DeleteMode),
    SetOpacity(u8),
    SetAccent(Accent),
}

pub enum Entry {
    Item {
        action: Action,
        label: &'static str,
        hint: &'static str,
        checked: bool,
    },
    Section(&'static str),
    Separator,
    Swatches(Accent),
}

fn height(e: &Entry) -> f32 {
    match e {
        Entry::Item { .. } => ITEM_H,
        Entry::Section(_) => SECTION_H,
        Entry::Separator => SEP_H,
        Entry::Swatches(_) => SWATCH_H,
    }
}

fn item(action: Action, label: &'static str, hint: &'static str) -> Entry {
    Entry::Item {
        action,
        label,
        hint,
        checked: false,
    }
}

fn delete_label(mode: DeleteMode) -> &'static str {
    if mode == DeleteMode::Trash {
        tr("menu.trash")
    } else {
        tr("menu.delete_permanent")
    }
}

pub fn context_entries(mode: DeleteMode) -> Vec<Entry> {
    vec![
        item(Action::ResetZoom, tr("menu.reset_zoom"), "0"),
        item(Action::RotateLeft, tr("menu.rotate_left"), "Shift+R"),
        item(Action::RotateRight, tr("menu.rotate_right"), "R"),
        item(Action::Crop, tr("menu.crop"), "C"),
        Entry::Separator,
        item(Action::Copy, tr("menu.copy"), "Ctrl+C"),
        item(Action::Print, tr("menu.print"), "Ctrl+P"),
        Entry::Separator,
        item(Action::Delete, delete_label(mode), "Del"),
    ]
}

const OPACITIES: [(u8, &str); 4] = [(100, "100%"), (85, "85%"), (70, "70%"), (55, "55%")];

pub fn settings_entries(
    language: Option<&str>,
    mode: DeleteMode,
    opacity: u8,
    accent: Accent,
) -> Vec<Entry> {
    let mut v = vec![
        Entry::Section(tr("settings.language")),
        Entry::Item {
            action: Action::SetLanguage("auto"),
            label: tr("settings.auto"),
            hint: "",
            checked: language.is_none(),
        },
    ];
    for (code, name) in available() {
        v.push(Entry::Item {
            action: Action::SetLanguage(code),
            label: name,
            hint: "",
            checked: language == Some(code),
        });
    }
    v.push(Entry::Separator);
    v.push(Entry::Section(tr("settings.delete")));
    v.push(Entry::Item {
        action: Action::SetDeleteMode(DeleteMode::Trash),
        label: tr("menu.trash"),
        hint: "",
        checked: mode == DeleteMode::Trash,
    });
    v.push(Entry::Item {
        action: Action::SetDeleteMode(DeleteMode::Permanent),
        label: tr("menu.delete_permanent"),
        hint: "",
        checked: mode == DeleteMode::Permanent,
    });
    v.push(Entry::Separator);
    v.push(Entry::Section(tr("settings.opacity")));
    for (value, label) in OPACITIES {
        v.push(Entry::Item {
            action: Action::SetOpacity(value),
            label,
            hint: "",
            checked: opacity == value,
        });
    }
    v.push(Entry::Separator);
    v.push(Entry::Section(tr("settings.accent")));
    v.push(Entry::Swatches(accent));
    v
}

pub struct Menu {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub hover: Option<usize>,
    pub appear: Fade,
    entries: Vec<Entry>,
    tops: Vec<f32>,
    fades: Vec<Fade>,
    scale: f32,
}

impl Menu {
    pub fn new(
        entries: Vec<Entry>,
        anchor: (i32, i32),
        win: (u32, u32),
        scale: f32,
        align_right: bool,
    ) -> Self {
        let mut tops = Vec::with_capacity(entries.len());
        let mut y = MENU_PAD;
        for e in &entries {
            tops.push(y);
            y += height(e);
        }
        let w = (MENU_W * scale).round() as i32;
        let h = ((y + MENU_PAD) * scale).round() as i32;
        let ax = if align_right { anchor.0 - w } else { anchor.0 };
        let x = ax.min(win.0 as i32 - w).max(0);
        let y = anchor.1.min(win.1 as i32 - h).max(0);
        let fades = vec![Fade::new(0.0); entries.len()];
        Self {
            x,
            y,
            w,
            h,
            hover: None,
            appear: Fade { v: 0.0, to: 1.0 },
            entries,
            tops,
            fades,
            scale,
        }
    }

    pub fn pad(&self) -> i32 {
        (SHADOW_PAD * self.scale).round() as i32
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
        let rel = (py / self.scale as f64) as f32;
        for (i, e) in self.entries.iter().enumerate() {
            let top = self.tops[i];
            if rel >= top && rel < top + height(e) {
                return matches!(e, Entry::Item { .. } | Entry::Swatches(_)).then_some(i);
            }
        }
        None
    }

    pub fn action_under(&self, p: (f64, f64)) -> Option<Action> {
        let i = self.item_at(p)?;
        match self.entries.get(i)? {
            Entry::Item { action, .. } => Some(*action),
            Entry::Swatches(_) => {
                let s = self.scale;
                let iw = self.w as f32 - 2.0 * MENU_PAD * s;
                let r = SWATCH_R * s;
                let n = Accent::ALL.len();
                let step = (iw - 20.0 * s - 2.0 * r) / (n as f32 - 1.0);
                let rel = p.0 as f32 - (self.x as f32 + MENU_PAD * s + 10.0 * s + r);
                let k = (rel / step).round().clamp(0.0, (n - 1) as f32) as usize;
                Some(Action::SetAccent(Accent::ALL[k]))
            }
            _ => None,
        }
    }

    pub fn draw(&self, cv: &mut Canvas, font: &Fonts) {
        let s = self.scale;
        let (mx, my, mw, mh) = (self.x as f32, self.y as f32, self.w as f32, self.h as f32);
        surface(cv, mx, my, mw, mh, R_MENU * s, s);

        let ix = self.x as f32 + MENU_PAD * s;
        let iw = self.w as f32 - 2.0 * MENU_PAD * s;
        let px = 14.0 * s;
        let small = 12.0 * s;
        let (Some(lm), Some(sm)) = (
            font.horizontal_line_metrics(px),
            font.horizontal_line_metrics(small),
        ) else {
            return;
        };
        let indent = if self.entries.iter().any(|e| matches!(e, Entry::Section(_))) {
            22.0 * s
        } else {
            0.0
        };

        let acc = accent();
        for (i, e) in self.entries.iter().enumerate() {
            let iy = self.y as f32 + self.tops[i] * s;
            match e {
                Entry::Swatches(current) => {
                    let r = SWATCH_R * s;
                    let cy = iy + SWATCH_H * s / 2.0;
                    let n = Accent::ALL.len();
                    let step = (iw - 20.0 * s - 2.0 * r) / (n as f32 - 1.0);
                    for (k, a) in Accent::ALL.iter().enumerate() {
                        let cx = ix + 10.0 * s + r + k as f32 * step;
                        cv.circle(cx, cy, r, a.rgb());
                        if a == current {
                            cv.ring(cx, cy, r + 4.0 * s, 1.6 * s, TEXT);
                        }
                    }
                }
                Entry::Separator => {
                    cv.fill_rect(
                        (ix + 8.0 * s) as i32,
                        (iy + SEP_H * s / 2.0) as i32,
                        (iw - 16.0 * s) as i32,
                        1,
                        BORDER,
                    );
                }
                Entry::Section(text) => {
                    let baseline =
                        iy + SECTION_H * s / 2.0 + (sm.ascent + sm.descent) / 2.0 + 3.0 * s;
                    cv.text(font, text, ix + 10.0 * s, baseline, small, DIM);
                }
                Entry::Item {
                    label,
                    hint,
                    checked,
                    ..
                } => {
                    let ih = ITEM_H * s;
                    let f = self.fades[i].v;
                    if f > 0.01 {
                        cv.round_rect_alpha(ix, iy, iw, ih, R_ITEM * s, HOVER, f);
                    }
                    let baseline = iy + ih / 2.0 + (lm.ascent + lm.descent) / 2.0;
                    if *checked {
                        let (x0, y0) = (ix + 10.0 * s, iy + ih / 2.0);
                        cv.line(x0, y0, x0 + 3.5 * s, y0 + 3.5 * s, 1.8 * s, acc);
                        cv.line(
                            x0 + 3.5 * s,
                            y0 + 3.5 * s,
                            x0 + 10.0 * s,
                            y0 - 3.5 * s,
                            1.8 * s,
                            acc,
                        );
                    }
                    cv.text(font, label, ix + 10.0 * s + indent, baseline, px, TEXT);
                    if !hint.is_empty() {
                        let hw = text_width(font, hint, px);
                        cv.text(font, hint, ix + iw - 10.0 * s - hw, baseline, px, DIM);
                    }
                }
            }
        }
    }
}
