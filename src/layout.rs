use crate::crop::{PRESETS, RATIO_GAP, RATIO_PAD, RATIO_ROW, RATIO_W};

pub const MARGIN: f32 = 12.0;
pub const PILL_H: f32 = 40.0;
pub const TOP_H: f32 = 76.0;
pub const BTN_S: f32 = 30.0;
pub const BTN_GAP: f32 = 6.0;
pub const CAP_PAD: f32 = 5.0;
pub const LOGO_S: f32 = 120.0;
pub const START_W: f32 = 340.0;
pub const START_H: f32 = 316.0;
pub const START_BTN2_H: f32 = 40.0;
pub const START_BTN_W: f32 = 240.0;
pub const START_BTN_H: f32 = 46.0;

pub const SIDEBAR_W: f32 = 332.0;
pub const PANEL_L: f32 = 16.0;
pub const PANEL_R: f32 = 16.0;
pub const PANEL_B: f32 = 16.0;
pub const PAD: f32 = 14.0;
pub const GROUP_GAP: f32 = 10.0;
pub const ROW_H: f32 = 50.0;
pub const CARD_ROWS: [usize; 4] = [1, 1, 3, 2];
pub const SHADOW_PAD: f32 = 10.0;

#[derive(Clone, Copy, PartialEq)]
pub enum Hit {
    Info,
    Close,
    OpenFolder,
    More,
    Open,
    Start,
    StartFolder,
    Fullscreen,
}

impl Hit {
    pub fn index(self) -> usize {
        self as usize
    }
}

pub struct Layout {
    pub scale: f32,
    pub win: (u32, u32),
    pub top_h: i32,
    pub sidebar_w: i32,
    pub sidebar_full: i32,
    pub empty: bool,
}

impl Layout {
    pub fn new(win: (u32, u32), scale: f32, frac: f32) -> Self {
        let full = (SIDEBAR_W * scale).round() as i32;
        Self {
            scale,
            win,
            top_h: (TOP_H * scale).round() as i32,
            sidebar_w: (full as f32 * frac).round() as i32,
            sidebar_full: full,
            empty: false,
        }
    }

    pub fn pad(&self) -> i32 {
        (SHADOW_PAD * self.scale).round() as i32
    }

    pub fn side_oy(&self) -> i32 {
        self.top_h - self.pad()
    }

    pub fn viewport(&self) -> (i32, i32, u32, u32) {
        let w = (self.win.0 as i32 - self.sidebar_w).max(1);
        (0, 0, w as u32, self.win.1.max(1))
    }

    pub fn capsule(&self) -> (f32, f32, f32, f32) {
        let s = self.scale;
        let count = if self.empty { 4.0 } else { 5.0 };
        let w = (BTN_S * count + BTN_GAP * (count - 1.0) + CAP_PAD * 2.0) * s;
        (
            self.win.0 as f32 - MARGIN * s - w,
            MARGIN * s,
            w,
            PILL_H * s,
        )
    }

    pub fn btn_center(&self, i: usize) -> (f32, f32) {
        let (x, y, _, h) = self.capsule();
        let s = self.scale;
        (
            x + (CAP_PAD + BTN_S / 2.0 + i as f32 * (BTN_S + BTN_GAP)) * s,
            y + h / 2.0,
        )
    }

    pub fn fullscreen_center(&self) -> (f32, f32) {
        self.btn_center(0)
    }

    pub fn open_center(&self) -> (f32, f32) {
        self.btn_center(if self.empty { 0 } else { 1 })
    }

    pub fn info_center(&self) -> (f32, f32) {
        self.btn_center(if self.empty { 1 } else { 2 })
    }

    pub fn more_center(&self) -> (f32, f32) {
        self.btn_center(if self.empty { 2 } else { 3 })
    }

    pub fn close_center(&self) -> (f32, f32) {
        self.btn_center(if self.empty { 3 } else { 4 })
    }

    pub fn title_max_w(&self) -> f32 {
        let (cx, ..) = self.capsule();
        (cx - 2.0 * MARGIN * self.scale).max(0.0)
    }

    pub fn panel_x(&self) -> f32 {
        (self.win.0 as i32 - self.sidebar_full) as f32 + PANEL_L * self.scale
    }

    pub fn panel_w(&self) -> f32 {
        self.sidebar_full as f32 - (PANEL_L + PANEL_R) * self.scale
    }

    pub fn panel_h(&self) -> f32 {
        let rows = CARD_ROWS.iter().sum::<usize>() as f32;
        let groups = (CARD_ROWS.len() - 1) as f32;
        let content = (2.0 * PAD + groups * GROUP_GAP + rows * ROW_H) * self.scale;
        let avail = (self.win.1 as i32 - self.top_h) as f32 - PANEL_B * self.scale;
        content.min(avail).max(0.0)
    }

    pub fn card_x(&self) -> f32 {
        self.panel_x() + PAD * self.scale
    }

    pub fn card_w(&self) -> f32 {
        self.panel_w() - 2.0 * PAD * self.scale
    }

    pub fn group_y(&self, n: usize) -> f32 {
        let rows_before: usize = CARD_ROWS[..n].iter().sum();
        self.top_h as f32 + (PAD + n as f32 * GROUP_GAP + rows_before as f32 * ROW_H) * self.scale
    }

    pub fn folder_btn_center(&self) -> (f32, f32) {
        (
            self.card_x() + self.card_w() - (BTN_S / 2.0 + 2.0) * self.scale,
            self.group_y(1) + ROW_H * self.scale / 2.0,
        )
    }

    pub fn ratio_panel(&self) -> (f32, f32, f32, f32, f32) {
        let s = self.scale;
        let n = PRESETS.len() as f32;
        let (pad, gap) = (RATIO_PAD * s, RATIO_GAP * s);
        let avail = self.win.1 as f32 - self.top_h as f32 - 2.0 * MARGIN * s;
        let row = (RATIO_ROW * s).min(((avail - 2.0 * pad - gap * (n - 1.0)) / n).max(16.0 * s));
        let h = 2.0 * pad + row * n + gap * (n - 1.0);
        let top = self.top_h as f32;
        let y = top + (self.win.1 as f32 - top - h) / 2.0;
        let w = RATIO_W * s;
        let right = (self.win.0 as i32 - self.sidebar_w) as f32;
        (right - MARGIN * s - w, y, w, h, row)
    }

    pub fn ratio_row(&self, i: usize) -> (f32, f32, f32, f32) {
        let s = self.scale;
        let (x, y, w, _, row) = self.ratio_panel();
        (
            x + RATIO_PAD * s,
            y + RATIO_PAD * s + i as f32 * (row + RATIO_GAP * s),
            w - 2.0 * RATIO_PAD * s,
            row,
        )
    }

    pub fn ratio_at(&self, p: (f64, f64)) -> Option<usize> {
        (0..PRESETS.len()).find(|&i| {
            let r = self.ratio_row(i);
            p.0 >= r.0 as f64
                && p.0 <= (r.0 + r.2) as f64
                && p.1 >= r.1 as f64
                && p.1 <= (r.1 + r.3) as f64
        })
    }

    pub fn on_ratio_panel(&self, p: (f64, f64)) -> bool {
        let (x, y, w, h, _) = self.ratio_panel();
        p.0 >= x as f64 && p.0 <= (x + w) as f64 && p.1 >= y as f64 && p.1 <= (y + h) as f64
    }

    pub fn start_origin(&self) -> (f32, f32) {
        let block_h = START_H * self.scale;
        let cx = (self.win.0 as i32 - self.sidebar_w) as f32 / 2.0;
        let top = self.top_h as f32;
        let cy = top + (self.win.1 as f32 - top) / 2.0;
        (cx, (cy - block_h / 2.0).max(top))
    }

    pub fn start_btn(&self) -> (f32, f32, f32, f32) {
        let s = self.scale;
        let (cx, y0) = self.start_origin();
        (
            cx - START_BTN_W * s / 2.0,
            y0 + 186.0 * s,
            START_BTN_W * s,
            START_BTN_H * s,
        )
    }

    pub fn start_btn2(&self) -> (f32, f32, f32, f32) {
        let s = self.scale;
        let (cx, y0) = self.start_origin();
        (
            cx - START_BTN_W * s / 2.0,
            y0 + 242.0 * s,
            START_BTN_W * s,
            START_BTN2_H * s,
        )
    }

    pub fn hit(&self, p: (f64, f64)) -> Option<Hit> {
        let half = (BTN_S / 2.0 * self.scale) as f64;
        let near =
            |c: (f32, f32)| (p.0 - c.0 as f64).abs() <= half && (p.1 - c.1 as f64).abs() <= half;
        let inside = |r: (f32, f32, f32, f32)| {
            p.0 >= r.0 as f64
                && p.0 <= (r.0 + r.2) as f64
                && p.1 >= r.1 as f64
                && p.1 <= (r.1 + r.3) as f64
        };
        if near(self.close_center()) {
            Some(Hit::Close)
        } else if near(self.more_center()) {
            Some(Hit::More)
        } else if near(self.info_center()) {
            Some(Hit::Info)
        } else if near(self.open_center()) {
            Some(Hit::Open)
        } else if !self.empty && near(self.fullscreen_center()) {
            Some(Hit::Fullscreen)
        } else if self.empty && inside(self.start_btn()) {
            Some(Hit::Start)
        } else if self.empty && inside(self.start_btn2()) {
            Some(Hit::StartFolder)
        } else if self.sidebar_w == self.sidebar_full && near(self.folder_btn_center()) {
            Some(Hit::OpenFolder)
        } else {
            None
        }
    }
}

pub const EDGE: f32 = 5.0;
