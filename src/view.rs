pub fn fit_scale((iw, ih): (u32, u32), (w, h): (u32, u32)) -> f32 {
    (w as f32 / iw as f32).min(h as f32 / ih as f32).min(1.0)
}

pub fn fill_scale((iw, ih): (u32, u32), (w, h): (u32, u32)) -> f32 {
    (w as f32 / iw as f32).min(h as f32 / ih as f32)
}

#[derive(Default)]
pub struct View {
    pub zoom: Option<f32>,
    pub pan: (f32, f32),
    pub cur_scale: f32,
    pub cur_pan: (f32, f32),
    pub fit_to_window: bool,
}

impl View {
    pub fn new(fit_to_window: bool) -> Self {
        Self {
            fit_to_window,
            ..Default::default()
        }
    }

    pub fn scale(&self, img: (u32, u32), vp: (u32, u32)) -> f32 {
        self.zoom.unwrap_or_else(|| {
            if self.fit_to_window {
                fill_scale(img, vp)
            } else {
                fit_scale(img, vp)
            }
        })
    }

    pub fn reset(&mut self) {
        self.zoom = None;
        self.pan = (0.0, 0.0);
    }

    pub fn clamp_pan(&mut self, img: (u32, u32), vp: (u32, u32)) {
        let s = self.scale(img, vp);
        let max_x = ((img.0 as f32 * s - vp.0 as f32) / 2.0).max(0.0);
        let max_y = ((img.1 as f32 * s - vp.1 as f32) / 2.0).max(0.0);
        self.pan.0 = self.pan.0.clamp(-max_x, max_x);
        self.pan.1 = self.pan.1.clamp(-max_y, max_y);
    }

    pub fn step(&mut self, img: (u32, u32), vp: (u32, u32), dt: f32, snap: bool) -> bool {
        let ts = self.scale(img, vp);
        if snap || self.cur_scale <= 0.0 {
            let changed = self.cur_scale != ts || self.cur_pan != self.pan;
            self.cur_scale = ts;
            self.cur_pan = self.pan;
            return changed;
        }
        let a = 1.0 - (-dt * 18.0).exp();
        let mut moving = false;

        let ds = ts - self.cur_scale;
        if ds != 0.0 {
            moving = true;
            if ds.abs() < ts * 0.0005 {
                self.cur_scale = ts;
            } else {
                self.cur_scale += ds * a;
            }
        }
        let dx = self.pan.0 - self.cur_pan.0;
        if dx != 0.0 {
            moving = true;
            if dx.abs() < 0.25 {
                self.cur_pan.0 = self.pan.0;
            } else {
                self.cur_pan.0 += dx * a;
            }
        }
        let dy = self.pan.1 - self.cur_pan.1;
        if dy != 0.0 {
            moving = true;
            if dy.abs() < 0.25 {
                self.cur_pan.1 = self.pan.1;
            } else {
                self.cur_pan.1 += dy * a;
            }
        }
        moving
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fit_scale_modes() {
        let small = (100, 100);
        let vp = (800, 600);
        assert_eq!(fit_scale(small, vp), 1.0);
        assert_eq!(fill_scale(small, vp), 6.0);

        let large = (1600, 1200);
        assert_eq!(fit_scale(large, vp), 0.5);
        assert_eq!(fill_scale(large, vp), 0.5);
    }
}
