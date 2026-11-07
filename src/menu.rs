use crate::{
    Fade, R_ITEM, R_MENU, SHADOW_PAD,
    config::DeleteMode,
    draw::{Canvas, text_width},
    fonts::Fonts,
    i18n::{available, tr},
    surface,
    theme::{self, ThemeMode},
};

const CONTEXT_MENU_W: f32 = 240.0;
const SETTINGS_MENU_W: f32 = 370.0;
const ITEM_H: f32 = 34.0;
const SECTION_H: f32 = 30.0;
const SEP_H: f32 = 9.0;
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
    SetTheme(ThemeMode),
}

#[derive(Clone)]
pub enum Entry {
    Item {
        action: Action,
        label: &'static str,
        hint: &'static str,
        checked: bool,
    },
    Submenu {
        label: &'static str,
        value: String,
        items: Vec<Entry>,
    },
    #[allow(dead_code)]
    Section(&'static str),
    Separator,
}

fn height(entry: &Entry) -> f32 {
    match entry {
        Entry::Item { .. } => ITEM_H,
        Entry::Submenu { .. } => ITEM_H,
        Entry::Section(_) => SECTION_H,
        Entry::Separator => SEP_H,
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
    theme: ThemeMode,
    language: Option<&str>,
    mode: DeleteMode,
    opacity: u8,
) -> Vec<Entry> {
    let cur_theme = match theme {
        ThemeMode::System => tr("settings.theme_system"),
        ThemeMode::Neutral => tr("settings.theme_neutral"),
        ThemeMode::Dark => tr("settings.theme_dark"),
        ThemeMode::Light => tr("settings.theme_light"),
    };
    let theme_items = vec![
        Entry::Item {
            action: Action::SetTheme(ThemeMode::System),
            label: tr("settings.theme_system"),
            hint: "",
            checked: theme == ThemeMode::System,
        },
        Entry::Item {
            action: Action::SetTheme(ThemeMode::Neutral),
            label: tr("settings.theme_neutral"),
            hint: "",
            checked: theme == ThemeMode::Neutral,
        },
        Entry::Item {
            action: Action::SetTheme(ThemeMode::Dark),
            label: tr("settings.theme_dark"),
            hint: "",
            checked: theme == ThemeMode::Dark,
        },
        Entry::Item {
            action: Action::SetTheme(ThemeMode::Light),
            label: tr("settings.theme_light"),
            hint: "",
            checked: theme == ThemeMode::Light,
        },
    ];
    let mut lang_items = vec![Entry::Item {
        action: Action::SetLanguage("auto"),
        label: tr("settings.auto"),
        hint: "",
        checked: language.is_none(),
    }];
    let mut cur_lang = tr("settings.auto");
    for (code, name) in available() {
        let checked = language == Some(code);
        if checked {
            cur_lang = name;
        }
        lang_items.push(Entry::Item {
            action: Action::SetLanguage(code),
            label: name,
            hint: "",
            checked,
        });
    }

    let del_trash = tr("menu.trash");
    let del_perm = tr("menu.delete_permanent");
    let cur_del = if mode == DeleteMode::Trash {
        del_trash
    } else {
        del_perm
    };
    let del_items = vec![
        Entry::Item {
            action: Action::SetDeleteMode(DeleteMode::Trash),
            label: del_trash,
            hint: "",
            checked: mode == DeleteMode::Trash,
        },
        Entry::Item {
            action: Action::SetDeleteMode(DeleteMode::Permanent),
            label: del_perm,
            hint: "",
            checked: mode == DeleteMode::Permanent,
        },
    ];

    let mut op_items = Vec::new();
    for (val, label) in OPACITIES {
        op_items.push(Entry::Item {
            action: Action::SetOpacity(val),
            label,
            hint: "",
            checked: opacity == val,
        });
    }

    vec![
        Entry::Submenu {
            label: tr("settings.theme"),
            value: cur_theme.to_string(),
            items: theme_items,
        },
        Entry::Separator,
        Entry::Submenu {
            label: tr("settings.language"),
            value: cur_lang.to_string(),
            items: lang_items,
        },
        Entry::Separator,
        Entry::Submenu {
            label: tr("settings.delete"),
            value: cur_del.to_string(),
            items: del_items,
        },
        Entry::Separator,
        Entry::Submenu {
            label: tr("settings.opacity"),
            value: format!("{opacity}%"),
            items: op_items,
        },
    ]
}

#[derive(Clone)]
pub struct SubmenuState {
    pub parent_index: usize,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub entries: Vec<Entry>,
    tops: Vec<f32>,
    fades: Vec<Fade>,
    pub hover: Option<usize>,
}

impl SubmenuState {
    pub fn new_dropdown(
        parent_index: usize,
        entries: Vec<Entry>,
        parent_x: i32,
        item_y: i32,
        button_x: i32,
        button_w: i32,
        win: (u32, u32),
        scale: f32,
        font: &Fonts,
    ) -> Self {
        let mut tops = Vec::with_capacity(entries.len());
        let mut y = MENU_PAD;
        for e in &entries {
            tops.push(y);
            y += height(e);
        }
        let px = 14.0 * scale;
        let max_tw = entries
            .iter()
            .map(|e| match e {
                Entry::Item { label, .. } => text_width(font, label, px),
                _ => 0.0,
            })
            .fold(0.0, f32::max);
        let w = (max_tw + 52.0 * scale).max(button_w as f32).max(130.0 * scale).round() as i32;
        let h = ((y + MENU_PAD) * scale).round() as i32;
        let gap = (4.0 * scale).round() as i32;

        let right_x = button_x + button_w;
        let mut x = right_x - w;
        if x < parent_x {
            x = parent_x;
        }
        let x = x.clamp(0, (win.0 as i32 - w).max(0));

        let mut y = item_y + (ITEM_H * scale).round() as i32 + gap;
        if y + h > win.1 as i32 - 10 {
            y = item_y - h - gap;
        }
        let y = y.clamp(0, (win.1 as i32 - h).max(0));

        let fades = vec![Fade::new(0.0); entries.len()];
        Self {
            parent_index,
            x,
            y,
            w,
            h,
            entries,
            tops,
            fades,
            hover: None,
        }
    }

    pub fn item_at(&self, p: (f64, f64), scale: f32) -> Option<usize> {
        let (px, py) = (p.0 - self.x as f64, p.1 - self.y as f64);
        if px < 0.0 || px >= self.w as f64 || py < 0.0 || py >= self.h as f64 {
            return None;
        }
        let rel = (py / scale as f64) as f32;
        for (i, e) in self.entries.iter().enumerate() {
            let top = self.tops[i];
            if rel >= top && rel < top + height(e) {
                return matches!(e, Entry::Item { .. }).then_some(i);
            }
        }
        None
    }

    pub fn action_under(&self, p: (f64, f64), scale: f32) -> Option<Action> {
        let i = self.item_at(p, scale)?;
        match self.entries.get(i)? {
            Entry::Item { action, .. } => Some(*action),
            _ => None,
        }
    }
}

pub struct Menu {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub hover: Option<usize>,
    pub appear: Fade,
    pub entries: Vec<Entry>,
    tops: Vec<f32>,
    fades: Vec<Fade>,
    scale: f32,
    win: (u32, u32),
    pub submenu: Option<SubmenuState>,
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
        let is_settings = entries.iter().any(|e| matches!(e, Entry::Submenu { .. }));
        let base_w = if is_settings { SETTINGS_MENU_W } else { CONTEXT_MENU_W };
        let w = (base_w * scale).round() as i32;
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
            win,
            submenu: None,
        }
    }

    pub fn pad(&self) -> i32 {
        (SHADOW_PAD * self.scale).round() as i32
    }

    pub fn bounds(&self) -> (i32, i32, i32, i32) {
        let pad = self.pad();
        let mut min_x = self.x - pad;
        let mut min_y = self.y - pad;
        let mut max_x = self.x + self.w + pad;
        let mut max_y = self.y + self.h + pad;
        if let Some(sub) = &self.submenu {
            min_x = min_x.min(sub.x - pad);
            min_y = min_y.min(sub.y - pad);
            max_x = max_x.max(sub.x + sub.w + pad);
            max_y = max_y.max(sub.y + sub.h + pad);
        }
        (min_x, min_y, max_x - min_x, max_y - min_y)
    }

    pub fn contains(&self, p: (f64, f64)) -> bool {
        let inside_parent = p.0 >= self.x as f64
            && p.0 < (self.x + self.w) as f64
            && p.1 >= self.y as f64
            && p.1 < (self.y + self.h) as f64;
        if inside_parent {
            return true;
        }
        if let Some(sub) = &self.submenu {
            let inside_sub = p.0 >= sub.x as f64
                && p.0 < (sub.x + sub.w) as f64
                && p.1 >= sub.y as f64
                && p.1 < (sub.y + sub.h) as f64;
            if inside_sub {
                return true;
            }
        }
        false
    }

    pub fn set_hover(&mut self, item: Option<usize>) {
        self.hover = item;
        for (i, f) in self.fades.iter_mut().enumerate() {
            f.to = if item == Some(i) { 1.0 } else { 0.0 };
        }
    }

    pub fn dropdown_button_geometry(&self, idx: usize, font: &Fonts) -> Option<(i32, i32, i32, i32)> {
        let entry = self.entries.get(idx)?;
        if let Entry::Submenu { value, .. } = entry {
            let s = self.scale;
            let px = 14.0 * s;
            let vw = text_width(font, value, px);
            let btn_pad = 8.0 * s;
            let chev_w = 10.0 * s;
            let btn_w = (vw + chev_w + btn_pad * 2.0 + 4.0 * s).max(72.0 * s);
            let btn_h = 24.0 * s;
            let ix = self.x as f32 + MENU_PAD * s;
            let iw = self.w as f32 - 2.0 * MENU_PAD * s;
            let btn_x = ix + iw - btn_w - 4.0 * s;
            let iy = self.y as f32 + self.tops[idx] * s;
            let ih = ITEM_H * s;
            let btn_y = iy + (ih - btn_h) / 2.0;
            Some((
                btn_x.round() as i32,
                btn_y.round() as i32,
                btn_w.round() as i32,
                btn_h.round() as i32,
            ))
        } else {
            None
        }
    }

    pub fn handle_click(&mut self, p: (f64, f64), font: &Fonts) -> bool {
        // If click is inside open dropdown list, let item action handle it
        if let Some(sub) = &self.submenu {
            let inside_sub = p.0 >= sub.x as f64
                && p.0 < (sub.x + sub.w) as f64
                && p.1 >= sub.y as f64
                && p.1 < (sub.y + sub.h) as f64;
            if inside_sub {
                return false;
            }
        }

        // If clicked on an item in parent menu:
        if let Some(idx) = self.item_at(p) {
            if let Entry::Submenu { items, .. } = &self.entries[idx] {
                // If this dropdown is already open, toggle it closed
                if self.submenu.as_ref().is_some_and(|sub| sub.parent_index == idx) {
                    self.submenu = None;
                    return true;
                }
                // Otherwise open this dropdown
                let item_y = self.y + (self.tops[idx] * self.scale).round() as i32;
                let (btn_x, _btn_y, btn_w, _btn_h) = self
                    .dropdown_button_geometry(idx, font)
                    .unwrap_or((self.x + self.w - 80, item_y, 80, 24));
                self.submenu = Some(SubmenuState::new_dropdown(
                    idx,
                    items.clone(),
                    self.x,
                    item_y,
                    btn_x,
                    btn_w,
                    self.win,
                    self.scale,
                    font,
                ));
                return true;
            }
        } else if self.submenu.is_some() {
            // Clicked outside parent items, close dropdown
            self.submenu = None;
            return true;
        }
        false
    }

    pub fn update_cursor(&mut self, p: (f64, f64), _font: &Fonts) -> bool {
        let mut changed = false;

        // 1. If cursor is inside dropdown list:
        if let Some(sub) = &mut self.submenu {
            let inside_sub = p.0 >= sub.x as f64
                && p.0 < (sub.x + sub.w) as f64
                && p.1 >= sub.y as f64
                && p.1 < (sub.y + sub.h) as f64;
            if inside_sub {
                let new_sub_hover = sub.item_at(p, self.scale);
                if new_sub_hover != sub.hover {
                    sub.hover = new_sub_hover;
                    for (i, f) in sub.fades.iter_mut().enumerate() {
                        f.to = if new_sub_hover == Some(i) { 1.0 } else { 0.0 };
                    }
                    changed = true;
                }
                if self.hover.is_some() {
                    self.set_hover(None);
                    changed = true;
                }
                return changed;
            } else if sub.hover.is_some() {
                sub.hover = None;
                for f in sub.fades.iter_mut() {
                    f.to = 0.0;
                }
                changed = true;
            }
        }

        // 2. Cursor inside parent menu:
        let item = self.item_at(p);
        if item != self.hover {
            self.set_hover(item);
            changed = true;
        }

        changed
    }

    pub fn step(&mut self, dt: f32) -> bool {
        let mut moved = self.appear.step(dt, 20.0);
        for f in self.fades.iter_mut() {
            moved |= f.step(dt, 24.0);
        }
        if let Some(sub) = &mut self.submenu {
            for f in sub.fades.iter_mut() {
                moved |= f.step(dt, 24.0);
            }
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
                return match e {
                    Entry::Item { .. } | Entry::Submenu { .. } => Some(i),
                    _ => None,
                };
            }
        }
        None
    }

    pub fn action_under(&self, p: (f64, f64)) -> Option<Action> {
        if let Some(sub) = &self.submenu {
            if let Some(act) = sub.action_under(p, self.scale) {
                return Some(act);
            }
        }
        let i = self.item_at(p)?;
        match self.entries.get(i)? {
            Entry::Item { action, .. } => Some(*action),
            _ => None,
        }
    }

    pub fn draw(&self, cv: &mut Canvas, font: &Fonts) {
        let s = self.scale;

        // Draw parent menu surface
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

        let t = theme::current();
        let acc = t.accent;
        for (i, e) in self.entries.iter().enumerate() {
            let iy = self.y as f32 + self.tops[i] * s;
            match e {
                Entry::Separator => {
                    cv.fill_rect(
                        (ix + 8.0 * s) as i32,
                        (iy + SEP_H * s / 2.0) as i32,
                        (iw - 16.0 * s) as i32,
                        1,
                        t.border,
                    );
                }
                Entry::Section(text) => {
                    let baseline =
                        iy + SECTION_H * s / 2.0 + (sm.ascent + sm.descent) / 2.0 + 3.0 * s;
                    cv.text(font, text, ix + 10.0 * s, baseline, small, t.dim);
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
                        cv.round_rect_alpha(ix, iy, iw, ih, R_ITEM * s, t.hover, f);
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
                    cv.text(font, label, ix + 10.0 * s + indent, baseline, px, t.text);
                    if !hint.is_empty() {
                        let hw = text_width(font, hint, px);
                        cv.text(font, hint, ix + iw - 10.0 * s - hw, baseline, px, t.dim);
                    }
                }
                Entry::Submenu { label, value, .. } => {
                    let ih = ITEM_H * s;
                    let is_open = self
                        .submenu
                        .as_ref()
                        .is_some_and(|sub| sub.parent_index == i);
                    let f = self.fades[i].v.max(if is_open { 0.4 } else { 0.0 });
                    if f > 0.01 {
                        cv.round_rect_alpha(ix, iy, iw, ih, R_ITEM * s, t.hover, f);
                    }
                    let baseline = iy + ih / 2.0 + (lm.ascent + lm.descent) / 2.0;

                    // Label on the left
                    cv.text(font, label, ix + 12.0 * s, baseline, px, t.text);

                    // Dropdown button geometry on the right
                    if let Some((bx, by, bw, bh)) = self.dropdown_button_geometry(i, font) {
                        let (bx, by, bw, bh) = (bx as f32, by as f32, bw as f32, bh as f32);
                        let btn_alpha = if is_open { 0.18 } else if f > 0.01 { 0.12 } else { 0.06 };
                        cv.round_rect_alpha(bx, by, bw, bh, 5.0 * s, t.text, btn_alpha);
                        cv.round_rect_alpha(bx, by, bw, bh, 5.0 * s, t.border, 0.5);

                        // Value inside button
                        let val_x = bx + 8.0 * s;
                        cv.text(font, value, val_x, baseline, px, t.text);

                        // Chevron «▾» or «▴»
                        let ch_cx = bx + bw - 13.0 * s;
                        let ch_cy = iy + ih / 2.0;
                        let ch_c = if is_open { t.accent } else if f > 0.01 { t.text } else { t.dim };
                        if is_open {
                            // Pointing up «▴»
                            cv.line(ch_cx - 3.5 * s, ch_cy + 1.5 * s, ch_cx, ch_cy - 2.0 * s, 1.5 * s, ch_c);
                            cv.line(ch_cx + 3.5 * s, ch_cy + 1.5 * s, ch_cx, ch_cy - 2.0 * s, 1.5 * s, ch_c);
                        } else {
                            // Pointing down «▾»
                            cv.line(ch_cx - 3.5 * s, ch_cy - 1.5 * s, ch_cx, ch_cy + 2.0 * s, 1.5 * s, ch_c);
                            cv.line(ch_cx + 3.5 * s, ch_cy - 1.5 * s, ch_cx, ch_cy + 2.0 * s, 1.5 * s, ch_c);
                        }
                    }
                }
            }
        }

        // Draw floating dropdown list if open
        if let Some(sub) = &self.submenu {
            let (sx, sy, sw, sh) = (sub.x as f32, sub.y as f32, sub.w as f32, sub.h as f32);
            surface(cv, sx, sy, sw, sh, R_MENU * s, s);

            let six = sx + MENU_PAD * s;
            let siw = sw - 2.0 * MENU_PAD * s;
            for (i, e) in sub.entries.iter().enumerate() {
                let siy = sy + sub.tops[i] * s;
                if let Entry::Item { label, checked, .. } = e {
                    let ih = ITEM_H * s;
                    let f = sub.fades[i].v;
                    if f > 0.01 {
                        cv.round_rect_alpha(six, siy, siw, ih, R_ITEM * s, t.hover, f);
                    }
                    let baseline = siy + ih / 2.0 + (lm.ascent + lm.descent) / 2.0;
                    if *checked {
                        let (x0, y0) = (six + 10.0 * s, siy + ih / 2.0);
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
                    cv.text(font, label, six + 26.0 * s, baseline, px, t.text);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fontdue::{Font, FontSettings};

    fn test_fonts() -> Fonts {
        Fonts::new(
            Font::from_bytes(
                include_bytes!("../assets/InterVariable.ttf") as &[u8],
                FontSettings::default(),
            )
            .unwrap(),
        )
    }

    #[test]
    fn test_settings_dropdown_click() {
        let fonts = test_fonts();
        let entries = settings_entries(ThemeMode::System, None, DeleteMode::Trash, 100);
        assert_eq!(entries.len(), 7);

        let mut menu = Menu::new(entries, (400, 100), (800, 600), 1.0, true);
        let (_bx1, _by1, bw1, bh1) = menu.bounds();
        assert!(bw1 > 0 && bh1 > 0);
        assert!(menu.submenu.is_none());

        // Hover over first entry does NOT open dropdown
        let inside_first = (menu.x as f64 + 10.0, menu.y as f64 + 10.0);
        assert!(menu.update_cursor(inside_first, &fonts));
        assert!(menu.submenu.is_none(), "hover must not open dropdown");

        // Click on first entry opens dropdown
        assert!(menu.handle_click(inside_first, &fonts));
        assert!(menu.submenu.is_some(), "click must open dropdown");

        // Bounds should expand to include the dropdown list
        let (_bx2, _by2, _bw2, bh2) = menu.bounds();
        assert!(bh2 > bh1);

        // Point inside dropdown should return true for contains
        let sub = menu.submenu.as_ref().unwrap();
        let sub_point = (sub.x as f64 + 10.0, sub.y as f64 + 10.0);
        assert!(menu.contains(sub_point));

        // Submenu item action
        let act = menu.action_under(sub_point);
        assert!(matches!(act, Some(Action::SetTheme(_))));

        // Clicking first entry again toggles it closed
        assert!(menu.handle_click(inside_first, &fonts));
        assert!(menu.submenu.is_none(), "clicking again must close dropdown");
    }
}
