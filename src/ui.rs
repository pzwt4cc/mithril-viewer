use crate::{
    Fade, R_BTN, R_PANEL, R_PILL,
    draw::{Canvas, Rgb, ellipsize, lerp, text_width},
    fonts::Fonts,
    i18n::tr,
    info::Info,
    layout::{BTN_S, GROUP_GAP, Hit, Layout, MARGIN, PILL_H, ROW_H},
    surface, theme,
};

pub fn draw_header(
    cv: &mut Canvas,
    font: &Fonts,
    lay: &Layout,
    info: Option<&Info>,
    siblings_count: usize,
    index: usize,
    fades: &[Fade; 7],
    side_v: f32,
    settings_open: bool,
) {
    let s = lay.scale;
    let t = theme::current();

    if let Some(info) = info {
        let px = 14.0 * s;
        let (pad, icon_w, gap) = (14.0 * s, 16.0 * s, 9.0 * s);
        let counter = if siblings_count > 1 {
            format!("{} / {}", index + 1, siblings_count)
        } else {
            String::new()
        };
        let cw = if counter.is_empty() {
            0.0
        } else {
            gap + text_width(font, &counter, px)
        };
        let max_text = (lay.title_max_w() - 2.0 * pad - icon_w - gap - cw).max(0.0);
        let name = ellipsize(font, &info.name, px, max_text);
        let tw = text_width(font, &name, px);
        let (x, y) = (MARGIN * s, MARGIN * s);
        let (w, h) = (pad + icon_w + gap + tw + cw + pad, PILL_H * s);
        surface(cv, x, y, w, h, R_PILL * s, s);

        let (gx, gy) = (x + pad, y + h / 2.0);
        let dark = t.on_accent;
        cv.round_rect(gx, gy - 7.0 * s, icon_w, 14.0 * s, 3.5 * s, t.accent);
        cv.line(
            gx + 2.5 * s,
            gy + 4.2 * s,
            gx + 6.2 * s,
            gy - 0.6 * s,
            1.5 * s,
            dark,
        );
        cv.line(
            gx + 6.2 * s,
            gy - 0.6 * s,
            gx + 9.0 * s,
            gy + 2.6 * s,
            1.5 * s,
            dark,
        );
        cv.line(
            gx + 9.0 * s,
            gy + 2.6 * s,
            gx + 10.4 * s,
            gy + 1.2 * s,
            1.5 * s,
            dark,
        );
        cv.line(
            gx + 10.4 * s,
            gy + 1.2 * s,
            gx + 13.5 * s,
            gy + 4.2 * s,
            1.5 * s,
            dark,
        );
        cv.circle(gx + 11.3 * s, gy - 3.0 * s, 1.5 * s, dark);

        if let Some(lm) = font.horizontal_line_metrics(px) {
            let baseline = y + h / 2.0 + (lm.ascent + lm.descent) / 2.0;
            cv.text(font, &name, gx + icon_w + gap, baseline, px, t.text);
            if !counter.is_empty() {
                let cx = gx + icon_w + gap + tw + gap;
                cv.text(font, &counter, cx, baseline, px, t.dim);
            }
        }
    }

    let (x, y, w, h) = lay.capsule();
    surface(cv, x, y, w, h, R_PILL * s, s);

    let half = BTN_S * s / 2.0;
    let btn = |cv: &mut Canvas, c: (f32, f32), color: Rgb, amt: f32| {
        cv.round_rect_alpha(
            c.0 - half,
            c.1 - half,
            2.0 * half,
            2.0 * half,
            R_BTN * s,
            color,
            amt,
        )
    };

    let (ox, oy) = lay.open_center();
    let ov = fades[Hit::Open.index()].v;
    btn(&mut *cv, (ox, oy), t.hover, ov);
    cv.round_rect(
        ox - 7.0 * s,
        oy - 6.0 * s,
        6.0 * s,
        4.0 * s,
        1.5 * s,
        t.text,
    );
    cv.round_rect(
        ox - 7.0 * s,
        oy - 4.0 * s,
        14.0 * s,
        10.0 * s,
        2.2 * s,
        t.text,
    );

    let (ix, iy) = lay.info_center();
    let hv = fades[Hit::Info.index()].v;
    btn(
        &mut *cv,
        (ix, iy),
        lerp(t.hover, t.active, side_v),
        hv.max(side_v),
    );
    let ic = lerp(t.text, t.accent, side_v);
    cv.ring(ix, iy, 6.5 * s, 1.6 * s, ic);
    cv.circle(ix, iy - 3.0 * s, 1.0 * s, ic);
    cv.line(ix, iy - 0.6 * s, ix, iy + 3.2 * s, 1.6 * s, ic);

    let (mx, my) = lay.more_center();
    let mv = fades[Hit::More.index()].v;
    let active = if settings_open { 1.0 } else { 0.0 };
    btn(
        &mut *cv,
        (mx, my),
        lerp(t.hover, t.active, active),
        mv.max(active),
    );
    for k in [-1.0f32, 0.0, 1.0] {
        cv.circle(mx + k * 4.4 * s, my, 1.5 * s, t.text);
    }

    let (cx, cy) = lay.close_center();
    let div = ((mx + cx) / 2.0) as i32;
    cv.fill_rect(div, (cy - 8.0 * s) as i32, 1, (16.0 * s) as i32, t.border);
    let cf = fades[Hit::Close.index()].v;
    btn(&mut *cv, (cx, cy), t.danger, cf);
    let d = 4.3 * s;
    cv.line(cx - d, cy - d, cx + d, cy + d, 1.6 * s, t.text);
    cv.line(cx - d, cy + d, cx + d, cy - d, 1.6 * s, t.text);
}

pub fn draw_row(
    cv: &mut Canvas,
    font: &Fonts,
    x: f32,
    y: f32,
    w: f32,
    s: f32,
    label: &str,
    value: &str,
    right_pad: f32,
) {
    let t = theme::current();
    cv.text(font, label, x, y + 20.0 * s, 11.5 * s, t.dim);
    let vpx = 14.0 * s;
    let max_w = (w - right_pad).max(0.0);
    let v = ellipsize(font, value, vpx, max_w);
    cv.text(font, &v, x, y + 38.0 * s, vpx, t.text);
}

pub fn draw_sidebar(
    cv: &mut Canvas,
    font: &Fonts,
    lay: &Layout,
    info: Option<&Info>,
    fades: &[Fade; 7],
) {
    let s = lay.scale;
    let t = theme::current();

    let (px, py) = (lay.panel_x(), lay.top_h as f32);
    let (pw, ph) = (lay.panel_w(), lay.panel_h());
    surface(cv, px, py, pw, ph, R_PANEL * s, s);

    let Some(info) = info else { return };
    let groups: [Vec<(&str, &str)>; 4] = [
        vec![(tr("info.name"), info.name.as_str())],
        vec![(tr("info.folder"), info.folder.as_str())],
        vec![
            (tr("info.resolution"), info.resolution.as_str()),
            (tr("info.format"), info.format.as_str()),
            (tr("info.size"), info.size.as_str()),
        ],
        vec![
            (tr("info.created"), info.created.as_str()),
            (tr("info.modified"), info.modified.as_str()),
        ],
    ];
    let (cx0, cw) = (lay.card_x(), lay.card_w());
    for (n, rows) in groups.iter().enumerate() {
        let y0 = lay.group_y(n);
        if n > 0 {
            cv.fill_rect(
                cx0 as i32,
                (y0 - GROUP_GAP * s / 2.0) as i32,
                cw as i32,
                1,
                t.border,
            );
        }
        for (i, (label, value)) in rows.iter().enumerate() {
            let ry = y0 + i as f32 * ROW_H * s;
            let right_pad = if n == 1 { 44.0 * s } else { 0.0 };
            draw_row(cv, font, cx0, ry, cw, s, label, value, right_pad);
        }
    }

    let (fx, fy) = lay.folder_btn_center();
    let ff = fades[Hit::OpenFolder.index()].v;
    let half = BTN_S * s / 2.0;
    cv.round_rect_alpha(
        fx - half,
        fy - half,
        2.0 * half,
        2.0 * half,
        R_BTN * s,
        t.hover,
        ff,
    );
    let ic = lerp(t.dim, t.text, ff);
    cv.round_rect(fx - 7.0 * s, fy - 3.5 * s, 14.0 * s, 10.0 * s, 2.0 * s, ic);
    cv.round_rect(fx - 7.0 * s, fy - 6.5 * s, 6.0 * s, 4.0 * s, 1.5 * s, ic);
}

pub fn draw_start(
    cv: &mut Canvas,
    font: &Fonts,
    lay: &Layout,
    start_fade: f32,
    start_folder_fade: f32,
) {
    let s = lay.scale;
    let t = theme::current();
    let (cx, y0) = lay.start_origin();
    let line = |cv: &mut Canvas, text: &str, row_y: f32, row_h: f32, px: f32, color: Rgb| {
        let Some(lm) = font.horizontal_line_metrics(px) else {
            return;
        };
        let tw = text_width(font, text, px);
        let baseline = row_y + row_h / 2.0 + (lm.ascent + lm.descent) / 2.0;
        cv.text(font, text, cx - tw / 2.0, baseline, px, color);
    };

    line(
        &mut *cv,
        tr("empty.title"),
        y0 + 142.0 * s,
        26.0 * s,
        22.0 * s,
        t.text,
    );

    let (bx, by, bw, bh) = lay.start_btn();
    let highlight = if t.is_light {
        [0x55, 0x8e, 0xff]
    } else {
        [230, 230, 230]
    };
    let fill = lerp(t.accent, highlight, 0.25 * start_fade);
    cv.round_rect(bx, by, bw, bh, 12.0 * s, fill);
    line(&mut *cv, tr("empty.button"), by, bh, 15.0 * s, t.on_accent);

    let (fx, fy, fw, fh) = lay.start_btn2();
    cv.round_rect(
        fx,
        fy,
        fw,
        fh,
        11.0 * s,
        lerp(t.surf_top, t.hover, start_folder_fade),
    );
    line(&mut *cv, tr("empty.folder"), fy, fh, 14.0 * s, t.text);

    line(
        &mut *cv,
        tr("empty.hint"),
        y0 + 296.0 * s,
        18.0 * s,
        13.0 * s,
        t.dim,
    );
}
