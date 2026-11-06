use winit::window::CursorIcon;

use crate::{
    ACCENT, Fade, HOVER, R_ITEM, R_PANEL, TEXT,
    draw::{Canvas, Rgb},
    fonts::Fonts,
    gpu::{self, Gpu},
    i18n::tr,
    modal::CropBar,
    surface,
};

pub const RATIO_W: f32 = 128.0;
pub const RATIO_ROW: f32 = 37.0;
pub const RATIO_GAP: f32 = 3.0;
pub const RATIO_PAD: f32 = 7.0;
pub const MIN_CROP: f32 = 8.0;

#[derive(Clone, Copy, PartialEq)]
pub enum Preset {
    Free,
    Original,
    Fixed(u32, u32),
}

pub const PRESETS: [Preset; 7] = [
    Preset::Free,
    Preset::Original,
    Preset::Fixed(1, 1),
    Preset::Fixed(4, 3),
    Preset::Fixed(3, 2),
    Preset::Fixed(5, 4),
    Preset::Fixed(16, 9),
];

#[derive(Clone, Copy, PartialEq)]
pub enum Handle {
    Move,
    N,
    S,
    W,
    E,
    NW,
    NE,
    SW,
    SE,
}

#[derive(Clone, Copy)]
pub struct Drag {
    pub handle: Handle,
    pub start: (f64, f64),
    pub rect: [f32; 4],
}

pub struct Crop {
    pub rect: [f32; 4],
    pub drag: Option<Drag>,
    pub preset: usize,
    pub flip: bool,
    pub bar: CropBar,
    pub bar_dirty: bool,
    pub appear: Fade,
}

pub fn ratio_rect(r: f32, c: (f32, f32), iw: f32, ih: f32) -> [f32; 4] {
    let w = iw.min(ih * r);
    let h = w / r;
    let x = (c.0 - w / 2.0).clamp(0.0, (iw - w).max(0.0));
    let y = (c.1 - h / 2.0).clamp(0.0, (ih - h).max(0.0));
    [x, y, x + w, y + h]
}

pub fn lock_ratio(
    h: Handle,
    start: [f32; 4],
    free: [f32; 4],
    r: f32,
    iw: f32,
    ih: f32,
) -> [f32; 4] {
    if h == Handle::Move {
        return free;
    }
    let [x0, y0, x1, y1] = start;
    let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
    let (fw, fh) = (free[2] - free[0], free[3] - free[1]);
    let min_w = MIN_CROP.min(iw).max(MIN_CROP.min(ih) * r);
    match h {
        Handle::NW | Handle::NE | Handle::SW | Handle::SE => {
            let right = matches!(h, Handle::NE | Handle::SE);
            let down = matches!(h, Handle::SW | Handle::SE);
            let ax = if right { x0 } else { x1 };
            let ay = if down { y0 } else { y1 };
            let max_w =
                (if right { iw - ax } else { ax }).min((if down { ih - ay } else { ay }) * r);
            let w = fw.max(fh * r).max(min_w).min(max_w);
            let hh = w / r;
            let (nx0, nx1) = if right { (ax, ax + w) } else { (ax - w, ax) };
            let (ny0, ny1) = if down { (ay, ay + hh) } else { (ay - hh, ay) };
            [nx0, ny0, nx1, ny1]
        }
        Handle::E | Handle::W => {
            let east = h == Handle::E;
            let ax = if east { x0 } else { x1 };
            let max_w = (if east { iw - ax } else { ax }).min(2.0 * cy.min(ih - cy) * r);
            let w = fw.max(min_w).min(max_w);
            let hh = w / r;
            let (nx0, nx1) = if east { (ax, ax + w) } else { (ax - w, ax) };
            [nx0, cy - hh / 2.0, nx1, cy + hh / 2.0]
        }
        _ => {
            let south = h == Handle::S;
            let ay = if south { y0 } else { y1 };
            let max_h = (if south { ih - ay } else { ay }).min(2.0 * cx.min(iw - cx) / r);
            let hh = fh.max(min_w / r).min(max_h);
            let w = hh * r;
            let (ny0, ny1) = if south { (ay, ay + hh) } else { (ay - hh, ay) };
            [cx - w / 2.0, ny0, cx + w / 2.0, ny1]
        }
    }
}

pub fn drag_rect(h: Handle, r: [f32; 4], dx: f32, dy: f32, iw: f32, ih: f32) -> [f32; 4] {
    if h == Handle::Move {
        let dx = dx.max(-r[0]).min(iw - r[2]);
        let dy = dy.max(-r[1]).min(ih - r[3]);
        return [r[0] + dx, r[1] + dy, r[2] + dx, r[3] + dy];
    }
    let (mw, mh) = (MIN_CROP.min(iw), MIN_CROP.min(ih));
    let mut n = r;
    if matches!(h, Handle::W | Handle::NW | Handle::SW) {
        n[0] = (r[0] + dx).min(r[2] - mw).max(0.0);
    }
    if matches!(h, Handle::E | Handle::NE | Handle::SE) {
        n[2] = (r[2] + dx).max(r[0] + mw).min(iw);
    }
    if matches!(h, Handle::N | Handle::NW | Handle::NE) {
        n[1] = (r[1] + dy).min(r[3] - mh).max(0.0);
    }
    if matches!(h, Handle::S | Handle::SW | Handle::SE) {
        n[3] = (r[3] + dy).max(r[1] + mh).min(ih);
    }
    n
}

pub fn handle_icon(h: Handle) -> CursorIcon {
    match h {
        Handle::Move => CursorIcon::Move,
        Handle::N | Handle::S => CursorIcon::NsResize,
        Handle::W | Handle::E => CursorIcon::EwResize,
        Handle::NW | Handle::SE => CursorIcon::NwseResize,
        Handle::NE | Handle::SW => CursorIcon::NeswResize,
    }
}

fn quad(gpu: &Gpu, slot: usize, x0: f32, y0: f32, x1: f32, y1: f32, a: f32) {
    let (x0, y0, x1, y1) = (x0.round(), y0.round(), x1.round(), y1.round());
    if x1 > x0 && y1 > y0 {
        gpu.draw_overlay_scaled(slot, (x0, y0, x1 - x0, y1 - y0), a);
    }
}

pub fn draw_crop(gpu: &Gpu, s: f32, c: [f32; 4], im: [f32; 4], vp: (i32, i32, u32, u32)) {
    let (vx, vy, vw, vh) = vp;
    let (ix0, iy0) = (im[0].max(vx as f32), im[1].max(vy as f32));
    let (ix1, iy1) = (
        im[2].min((vx + vw as i32) as f32),
        im[3].min((vy + vh as i32) as f32),
    );
    let (cx0, cy0) = (c[0].max(ix0).min(ix1), c[1].max(iy0).min(iy1));
    let (cx1, cy1) = (c[2].max(ix0).min(ix1), c[3].max(iy0).min(iy1));
    let dim = 0.55;
    quad(gpu, gpu::BLACK, ix0, iy0, ix1, cy0, dim);
    quad(gpu, gpu::BLACK, ix0, cy1, ix1, iy1, dim);
    quad(gpu, gpu::BLACK, ix0, cy0, cx0, cy1, dim);
    quad(gpu, gpu::BLACK, cx1, cy0, ix1, cy1, dim);

    for i in 1..3 {
        let f = i as f32 / 3.0;
        let x = c[0] + (c[2] - c[0]) * f;
        let y = c[1] + (c[3] - c[1]) * f;
        quad(gpu, gpu::WHITE, x, c[1], x + 1.0, c[3], 0.3);
        quad(gpu, gpu::WHITE, c[0], y, c[2], y + 1.0, 0.3);
    }

    let t = (1.5 * s).max(1.0);
    quad(gpu, gpu::WHITE, c[0], c[1], c[2], c[1] + t, 1.0);
    quad(gpu, gpu::WHITE, c[0], c[3] - t, c[2], c[3], 1.0);
    quad(gpu, gpu::WHITE, c[0], c[1], c[0] + t, c[3], 1.0);
    quad(gpu, gpu::WHITE, c[2] - t, c[1], c[2], c[3], 1.0);

    let (mx, my) = ((c[0] + c[2]) / 2.0, (c[1] + c[3]) / 2.0);
    let hs = 4.5 * s;
    for (px, py) in [
        (c[0], c[1]),
        (mx, c[1]),
        (c[2], c[1]),
        (c[0], my),
        (c[2], my),
        (c[0], c[3]),
        (mx, c[3]),
        (c[2], c[3]),
    ] {
        quad(
            gpu,
            gpu::BLACK,
            px - hs - 1.0,
            py - hs - 1.0,
            px + hs + 1.0,
            py + hs + 1.0,
            0.45,
        );
        quad(gpu, gpu::WHITE, px - hs, py - hs, px + hs, py + hs, 1.0);
    }
}

pub fn draw_ratios(
    cv: &mut Canvas,
    font: &Fonts,
    crop: &Crop,
    panel_rect: (f32, f32, f32, f32),
    rows: impl IntoIterator<Item = (usize, (f32, f32, f32, f32))>,
    ratio_hover: Option<usize>,
    s: f32,
) {
    let (x, y, w, h) = panel_rect;
    surface(cv, x, y, w, h, R_PANEL * s, s);
    let px = 14.0 * s;
    let dark: Rgb = [0x2b, 0x2d, 0x31];
    let Some(lm) = font.horizontal_line_metrics(px) else {
        return;
    };
    for (i, (rx, ry, rw, rh)) in rows {
        let preset = &PRESETS[i];
        let active = crop.preset == i;
        if active {
            cv.round_rect(rx, ry, rw, rh, R_ITEM * s, ACCENT);
        } else if ratio_hover == Some(i) {
            cv.round_rect(rx, ry, rw, rh, R_ITEM * s, HOVER);
        }
        let label = match *preset {
            Preset::Free => tr("crop.free").to_string(),
            Preset::Original => tr("crop.original").to_string(),
            Preset::Fixed(a, b) if crop.flip => format!("{b}:{a}"),
            Preset::Fixed(a, b) => format!("{a}:{b}"),
        };
        let baseline = ry + rh / 2.0 + (lm.ascent + lm.descent) / 2.0;
        let color = if active { dark } else { TEXT };
        cv.text(font, &label, rx + 12.0 * s, baseline, px, color);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ratio_rect_aspect() {
        let r = ratio_rect(1.0, (100.0, 100.0), 200.0, 100.0);
        let w = r[2] - r[0];
        let h = r[3] - r[1];
        assert_eq!(w, 100.0);
        assert_eq!(h, 100.0);
    }
}
