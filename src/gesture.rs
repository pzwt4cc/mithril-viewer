use std::time::{Duration, Instant};
use winit::event::{MouseScrollDelta, TouchPhase};

const IDLE_RESET: Duration = Duration::from_millis(160);
const NAV_COOLDOWN: Duration = Duration::from_millis(240);
const SWIPE_RATIO: f32 = 1.25;
const SWIPE_THRESHOLD_PX: f32 = 48.0;
const TOUCHPAD_ZOOM_PX_PER_STEP: f32 = 72.0;
const LINE_TO_PX: f32 = 40.0;
const TOUCH_SWIPE_PX: f64 = 48.0;
const DOUBLE_CLICK_MS: u128 = 350;
const DOUBLE_CLICK_DIST_PX: f64 = 8.0;
const DRAG_SLOP_PX: f64 = 4.0;
const TWIST_ROT_RAD: f32 = 0.75;
const PINCH_ROT_LOCK_SCALE: f32 = 1.22;
const PINCH_ZOOM_LOCK_DEG: f32 = 18.0;
const PINCH_ROTATE_DEG: f32 = 42.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ScrollAction {
    None,
    Zoom(f32),
    Pan(f32, f32),
    Navigate(isize),
}

pub struct ScrollTracker {
    acc_x: f32,
    acc_y: f32,
    nav_triggered: bool,
    last_nav: Option<Instant>,
    last_event: Option<Instant>,
}

impl Default for ScrollTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl ScrollTracker {
    pub fn new() -> Self {
        Self {
            acc_x: 0.0,
            acc_y: 0.0,
            nav_triggered: false,
            last_nav: None,
            last_event: None,
        }
    }

    pub fn reset(&mut self) {
        self.acc_x = 0.0;
        self.acc_y = 0.0;
        self.nav_triggered = false;
        self.last_event = None;
    }

    pub fn reset_all(&mut self) {
        self.reset();
        self.last_nav = None;
    }

    #[allow(clippy::too_many_arguments)]
    pub fn handle(
        &mut self,
        delta: MouseScrollDelta,
        phase: TouchPhase,
        scale: f32,
        ctrl: bool,
        shift: bool,
        is_zoomed: bool,
        can_pan_x: bool,
        can_pan_y: bool,
        allow_nav: bool,
        now: Instant,
    ) -> ScrollAction {
        let s = scale.max(0.25);
        let (dx, dy, is_discrete_candidate) = match delta {
            MouseScrollDelta::PixelDelta(p) => (p.x as f32 / s, p.y as f32 / s, false),
            MouseScrollDelta::LineDelta(x, y) => {
                let discrete = (x == 0.0 || (x.fract() == 0.0 && x.abs() >= 1.0))
                    && (y == 0.0 || (y.fract() == 0.0 && y.abs() >= 1.0))
                    && (x != 0.0 || y != 0.0);
                (x * LINE_TO_PX, y * LINE_TO_PX, discrete)
            }
        };

        if phase == TouchPhase::Started {
            self.reset();
            self.last_nav = None;
        } else if self
            .last_event
            .is_some_and(|t| now.duration_since(t) >= IDLE_RESET)
        {
            self.reset();
        }

        let in_smooth_gesture = self.acc_x != 0.0 || self.acc_y != 0.0 || self.nav_triggered;

        if is_discrete_candidate && !in_smooth_gesture {
            if ctrl {
                return if dy != 0.0 {
                    ScrollAction::Zoom(dy / LINE_TO_PX)
                } else {
                    ScrollAction::None
                };
            }
            if shift {
                if !is_zoomed {
                    return ScrollAction::None;
                }
                return if dx != 0.0 {
                    ScrollAction::Pan(dx, dy)
                } else {
                    ScrollAction::Pan(dy, 0.0)
                };
            }
            if dy != 0.0 && dx.abs() <= dy.abs() {
                return ScrollAction::Zoom(dy / LINE_TO_PX);
            }
            if dx != 0.0 {
                if is_zoomed && can_pan_x {
                    return ScrollAction::Pan(dx, 0.0);
                }
                if !is_zoomed && allow_nav {
                    return ScrollAction::Navigate(if dx < 0.0 { 1 } else { -1 });
                }
            }
            return ScrollAction::None;
        }

        let ended = matches!(phase, TouchPhase::Ended | TouchPhase::Cancelled);
        if dx == 0.0 && dy == 0.0 {
            if ended {
                self.reset();
            }
            return ScrollAction::None;
        }

        self.last_event = Some(now);

        if ctrl {
            let action = if dy != 0.0 {
                ScrollAction::Zoom(dy / TOUCHPAD_ZOOM_PX_PER_STEP)
            } else {
                ScrollAction::None
            };
            if ended {
                self.reset();
            }
            return action;
        }

        if shift {
            let action = if is_zoomed {
                ScrollAction::Pan(
                    if can_pan_x { dx } else { 0.0 },
                    if can_pan_y { dy } else { 0.0 },
                )
            } else {
                ScrollAction::None
            };
            if ended {
                self.reset();
            }
            return if matches!(action, ScrollAction::Pan(0.0, 0.0)) {
                ScrollAction::None
            } else {
                action
            };
        }

        if is_zoomed {
            let px = if can_pan_x { dx } else { 0.0 };
            let py = if can_pan_y { dy } else { 0.0 };
            if ended {
                self.reset();
            }
            return if px != 0.0 || py != 0.0 {
                ScrollAction::Pan(px, py)
            } else {
                ScrollAction::None
            };
        }

        self.acc_x += dx;
        self.acc_y += dy;

        let cooldown_active = self
            .last_nav
            .is_some_and(|t| now.duration_since(t) < NAV_COOLDOWN);

        let action = if allow_nav
            && !self.nav_triggered
            && !cooldown_active
            && self.acc_x.abs() >= SWIPE_THRESHOLD_PX
            && self.acc_x.abs() > self.acc_y.abs() * SWIPE_RATIO
        {
            self.nav_triggered = true;
            self.last_nav = Some(now);
            ScrollAction::Navigate(if self.acc_x < 0.0 { 1 } else { -1 })
        } else {
            ScrollAction::None
        };

        if ended {
            self.reset();
        }
        action
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PinchOutput {
    pub zoom_steps: f32,
    pub pan: (f32, f32),
    pub rotate: Option<bool>,
}

pub struct PinchTracker {
    active: bool,
    prev_scale: f32,
    max_scale_ratio: f32,
    acc_rot: f32,
    rot_locked: bool,
    zoom_locked: bool,
}

impl Default for PinchTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl PinchTracker {
    pub fn new() -> Self {
        Self {
            active: false,
            prev_scale: 1.0,
            max_scale_ratio: 1.0,
            acc_rot: 0.0,
            rot_locked: false,
            zoom_locked: false,
        }
    }

    pub fn begin(&mut self) {
        self.active = true;
        self.prev_scale = 1.0;
        self.max_scale_ratio = 1.0;
        self.acc_rot = 0.0;
        self.rot_locked = false;
        self.zoom_locked = false;
    }

    pub fn end(&mut self) {
        self.active = false;
        self.prev_scale = 1.0;
        self.max_scale_ratio = 1.0;
        self.acc_rot = 0.0;
        self.rot_locked = false;
        self.zoom_locked = false;
    }

    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &mut self,
        dx: f32,
        dy: f32,
        scale: f32,
        rotation: f32,
        zoom_step: f32,
        scale_factor: f32,
        is_zoomed: bool,
        allow_rotate: bool,
    ) -> PinchOutput {
        if !self.active {
            self.begin();
        }
        if !scale.is_finite() || scale <= 0.05 {
            return PinchOutput::default();
        }
        let ratio = scale.max(1.0 / scale);
        if ratio > self.max_scale_ratio {
            self.max_scale_ratio = ratio;
        }
        if !self.zoom_locked
            && self.max_scale_ratio >= PINCH_ROT_LOCK_SCALE
            && self.acc_rot.abs() < PINCH_ZOOM_LOCK_DEG
        {
            self.rot_locked = true;
        }
        if allow_rotate && !self.rot_locked && rotation.is_finite() {
            self.acc_rot += rotation;
            if self.acc_rot.abs() >= PINCH_ZOOM_LOCK_DEG && self.max_scale_ratio < 1.14 {
                self.zoom_locked = true;
            }
            if self.acc_rot.abs() >= PINCH_ROTATE_DEG {
                let cw = self.acc_rot > 0.0;
                self.acc_rot = 0.0;
                self.prev_scale = scale;
                return PinchOutput {
                    zoom_steps: 0.0,
                    pan: (0.0, 0.0),
                    rotate: Some(cw),
                };
            }
        }
        let zoom_steps = if !self.zoom_locked && self.prev_scale > 0.0 {
            (scale / self.prev_scale).ln() / zoom_step.ln()
        } else {
            0.0
        };
        self.prev_scale = scale;
        let sf = scale_factor.max(0.25);
        let pan = if is_zoomed && !self.zoom_locked && dx.is_finite() && dy.is_finite() {
            (dx * sf, dy * sf)
        } else {
            (0.0, 0.0)
        };
        PinchOutput {
            zoom_steps,
            pan,
            rotate: None,
        }
    }
}

pub struct SwipeTracker {
    active: bool,
    acc_x: f32,
    acc_y: f32,
    triggered: bool,
}

impl Default for SwipeTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl SwipeTracker {
    pub fn new() -> Self {
        Self {
            active: false,
            acc_x: 0.0,
            acc_y: 0.0,
            triggered: false,
        }
    }

    pub fn begin(&mut self, fingers: u32) {
        self.active = fingers >= 2;
        self.acc_x = 0.0;
        self.acc_y = 0.0;
        self.triggered = false;
    }

    pub fn end(&mut self) {
        self.active = false;
        self.acc_x = 0.0;
        self.acc_y = 0.0;
        self.triggered = false;
    }

    pub fn update(&mut self, dx: f32, dy: f32, allow_nav: bool) -> Option<isize> {
        if !self.active || self.triggered || !allow_nav {
            return None;
        }
        if !dx.is_finite() || !dy.is_finite() {
            return None;
        }
        self.acc_x += dx;
        self.acc_y += dy;
        if self.acc_x.abs() >= SWIPE_THRESHOLD_PX && self.acc_x.abs() > self.acc_y.abs() * 1.2 {
            self.triggered = true;
            Some(if self.acc_x < 0.0 { 1 } else { -1 })
        } else {
            None
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TouchAction {
    None,
    Pan(f32, f32),
    PinchZoom {
        center: (f64, f64),
        steps: f32,
        pan: (f32, f32),
    },
    Navigate(isize),
    Rotate(bool),
}

pub struct TouchTracker {
    points: Vec<(u64, (f64, f64))>,
    start_pos: (f64, f64),
    nav_triggered: bool,
    pinched: bool,
    prev_angle: Option<f32>,
    acc_rotation: f32,
}

impl Default for TouchTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl TouchTracker {
    pub fn new() -> Self {
        Self {
            points: Vec::new(),
            start_pos: (0.0, 0.0),
            nav_triggered: false,
            pinched: false,
            prev_angle: None,
            acc_rotation: 0.0,
        }
    }

    pub fn reset(&mut self) {
        self.points.clear();
        self.nav_triggered = false;
        self.pinched = false;
        self.prev_angle = None;
        self.acc_rotation = 0.0;
    }

    #[allow(clippy::too_many_arguments)]
    pub fn handle(
        &mut self,
        id: u64,
        phase: TouchPhase,
        pos: (f64, f64),
        scale: f32,
        zoom_step: f32,
        is_zoomed: bool,
        allow_nav: bool,
    ) -> TouchAction {
        match phase {
            TouchPhase::Started => {
                if let Some(p) = self.points.iter_mut().find(|(pid, _)| *pid == id) {
                    p.1 = pos;
                } else {
                    self.points.push((id, pos));
                }
                if self.points.len() == 1 {
                    self.start_pos = pos;
                    self.nav_triggered = false;
                    self.pinched = false;
                    self.prev_angle = None;
                    self.acc_rotation = 0.0;
                } else if self.points.len() >= 2 {
                    self.pinched = true;
                    let a = self.points[0].1;
                    let b = self.points[1].1;
                    self.prev_angle = Some(((b.1 - a.1) as f32).atan2((b.0 - a.0) as f32));
                }
                TouchAction::None
            }
            TouchPhase::Moved => {
                let Some(idx) = self.points.iter().position(|(pid, _)| *pid == id) else {
                    self.points.push((id, pos));
                    return TouchAction::None;
                };
                if self.points.len() >= 2 && idx < 2 {
                    let prev_a = self.points[0].1;
                    let prev_b = self.points[1].1;
                    self.points[idx].1 = pos;
                    let new_a = self.points[0].1;
                    let new_b = self.points[1].1;

                    let prev_dist = (prev_b.0 - prev_a.0).hypot(prev_b.1 - prev_a.1) as f32;
                    let new_dist = (new_b.0 - new_a.0).hypot(new_b.1 - new_a.1) as f32;
                    let prev_mid = ((prev_a.0 + prev_b.0) * 0.5, (prev_a.1 + prev_b.1) * 0.5);
                    let new_mid = ((new_a.0 + new_b.0) * 0.5, (new_a.1 + new_b.1) * 0.5);
                    let pan = (
                        (new_mid.0 - prev_mid.0) as f32,
                        (new_mid.1 - prev_mid.1) as f32,
                    );

                    let curr_angle = ((new_b.1 - new_a.1) as f32).atan2((new_b.0 - new_a.0) as f32);
                    if let Some(prev) = self.prev_angle {
                        let mut diff = curr_angle - prev;
                        while diff > std::f32::consts::PI {
                            diff -= std::f32::consts::TAU;
                        }
                        while diff < -std::f32::consts::PI {
                            diff += std::f32::consts::TAU;
                        }
                        self.acc_rotation += diff;
                        if self.acc_rotation >= TWIST_ROT_RAD {
                            self.acc_rotation = 0.0;
                            self.prev_angle = Some(curr_angle);
                            return TouchAction::Rotate(true);
                        }
                        if self.acc_rotation <= -TWIST_ROT_RAD {
                            self.acc_rotation = 0.0;
                            self.prev_angle = Some(curr_angle);
                            return TouchAction::Rotate(false);
                        }
                    }
                    self.prev_angle = Some(curr_angle);

                    let steps = if prev_dist > 10.0 && new_dist > 10.0 {
                        (new_dist / prev_dist).ln() / zoom_step.ln()
                    } else {
                        0.0
                    };
                    return TouchAction::PinchZoom {
                        center: new_mid,
                        steps,
                        pan,
                    };
                }
                let prev = self.points[idx].1;
                self.points[idx].1 = pos;
                if self.points.len() == 1 && !self.pinched {
                    if is_zoomed {
                        return TouchAction::Pan((pos.0 - prev.0) as f32, (pos.1 - prev.1) as f32);
                    }
                    if allow_nav && !self.nav_triggered {
                        let tx = pos.0 - self.start_pos.0;
                        let ty = pos.1 - self.start_pos.1;
                        let threshold = TOUCH_SWIPE_PX * scale.max(0.5) as f64;
                        if tx.abs() >= threshold && tx.abs() > ty.abs() * 1.3 {
                            self.nav_triggered = true;
                            return TouchAction::Navigate(if tx < 0.0 { 1 } else { -1 });
                        }
                    }
                }
                TouchAction::None
            }
            TouchPhase::Ended | TouchPhase::Cancelled => {
                self.points.retain(|(pid, _)| *pid != id);
                if self.points.is_empty() {
                    self.pinched = false;
                    self.nav_triggered = false;
                    self.prev_angle = None;
                    self.acc_rotation = 0.0;
                } else if self.points.len() == 1 {
                    self.prev_angle = None;
                    self.acc_rotation = 0.0;
                }
                TouchAction::None
            }
        }
    }
}

pub struct ClickTracker {
    press_pos: (f64, f64),
    moved: bool,
    last_click: Option<(Instant, (f64, f64))>,
}

impl Default for ClickTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl ClickTracker {
    pub fn new() -> Self {
        Self {
            press_pos: (0.0, 0.0),
            moved: false,
            last_click: None,
        }
    }

    pub fn clear(&mut self) {
        self.last_click = None;
        self.moved = false;
    }

    pub fn on_press(&mut self, pos: (f64, f64), scale: f32, now: Instant) -> bool {
        let max_dist = DOUBLE_CLICK_DIST_PX * scale.max(0.5) as f64;
        if let Some((t, p)) = self.last_click
            && now.duration_since(t).as_millis() <= DOUBLE_CLICK_MS
            && (pos.0 - p.0).hypot(pos.1 - p.1) <= max_dist
        {
            self.last_click = None;
            self.moved = false;
            return true;
        }
        self.press_pos = pos;
        self.moved = false;
        false
    }

    pub fn on_move(&mut self, pos: (f64, f64), scale: f32) {
        let slop = DRAG_SLOP_PX * scale.max(0.5) as f64;
        if (pos.0 - self.press_pos.0).hypot(pos.1 - self.press_pos.1) > slop {
            self.moved = true;
        }
    }

    pub fn on_release(&mut self, pos: (f64, f64), now: Instant) {
        if !self.moved {
            self.last_click = Some((now, pos));
        } else {
            self.last_click = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use winit::dpi::PhysicalPosition;

    #[test]
    fn touchpad_horizontal_swipe_ignores_small_vertical_jitter_and_fires_once() {
        let mut t = ScrollTracker::new();
        let t0 = Instant::now();

        assert_eq!(
            t.handle(
                MouseScrollDelta::PixelDelta(PhysicalPosition::new(-12.0, 3.0)),
                TouchPhase::Started,
                2.0,
                false,
                false,
                false,
                false,
                false,
                true,
                t0,
            ),
            ScrollAction::None
        );

        assert_eq!(
            t.handle(
                MouseScrollDelta::PixelDelta(PhysicalPosition::new(-50.0, 2.0)),
                TouchPhase::Moved,
                2.0,
                false,
                false,
                false,
                false,
                false,
                true,
                t0 + Duration::from_millis(10),
            ),
            ScrollAction::None
        );

        assert_eq!(
            t.handle(
                MouseScrollDelta::PixelDelta(PhysicalPosition::new(-40.0, -2.0)),
                TouchPhase::Moved,
                2.0,
                false,
                false,
                false,
                false,
                false,
                true,
                t0 + Duration::from_millis(20),
            ),
            ScrollAction::Navigate(1)
        );

        assert_eq!(
            t.handle(
                MouseScrollDelta::PixelDelta(PhysicalPosition::new(-100.0, 0.0)),
                TouchPhase::Moved,
                2.0,
                false,
                false,
                false,
                false,
                false,
                true,
                t0 + Duration::from_millis(30),
            ),
            ScrollAction::None
        );

        assert_eq!(
            t.handle(
                MouseScrollDelta::PixelDelta(PhysicalPosition::new(0.0, 0.0)),
                TouchPhase::Ended,
                2.0,
                false,
                false,
                false,
                false,
                false,
                true,
                t0 + Duration::from_millis(40),
            ),
            ScrollAction::None
        );

        assert_eq!(
            t.handle(
                MouseScrollDelta::PixelDelta(PhysicalPosition::new(110.0, 0.0)),
                TouchPhase::Started,
                2.0,
                false,
                false,
                false,
                false,
                false,
                true,
                t0 + Duration::from_millis(60),
            ),
            ScrollAction::Navigate(-1)
        );
    }

    #[test]
    fn diagonal_start_followed_by_horizontal_swipe_still_navigates() {
        let mut t = ScrollTracker::new();
        let t0 = Instant::now();

        assert_eq!(
            t.handle(
                MouseScrollDelta::PixelDelta(PhysicalPosition::new(-6.0, 6.0)),
                TouchPhase::Started,
                1.0,
                false,
                false,
                false,
                false,
                false,
                true,
                t0,
            ),
            ScrollAction::None
        );

        assert_eq!(
            t.handle(
                MouseScrollDelta::PixelDelta(PhysicalPosition::new(-54.0, 2.0)),
                TouchPhase::Moved,
                1.0,
                false,
                false,
                false,
                false,
                false,
                true,
                t0 + Duration::from_millis(12),
            ),
            ScrollAction::Navigate(1)
        );
    }

    #[test]
    fn swipe_cooldown_blocks_inertial_double_skip_without_started() {
        let mut t = ScrollTracker::new();
        let t0 = Instant::now();

        assert_eq!(
            t.handle(
                MouseScrollDelta::PixelDelta(PhysicalPosition::new(-60.0, 0.0)),
                TouchPhase::Moved,
                1.0,
                false,
                false,
                false,
                false,
                false,
                true,
                t0,
            ),
            ScrollAction::Navigate(1)
        );

        assert_eq!(
            t.handle(
                MouseScrollDelta::PixelDelta(PhysicalPosition::new(-60.0, 0.0)),
                TouchPhase::Moved,
                1.0,
                false,
                false,
                false,
                false,
                false,
                true,
                t0 + Duration::from_millis(180),
            ),
            ScrollAction::None
        );

        assert_eq!(
            t.handle(
                MouseScrollDelta::PixelDelta(PhysicalPosition::new(-60.0, 0.0)),
                TouchPhase::Moved,
                1.0,
                false,
                false,
                false,
                false,
                false,
                true,
                t0 + Duration::from_millis(450),
            ),
            ScrollAction::Navigate(1)
        );
    }

    #[test]
    fn touchpad_vertical_scroll_does_not_zoom_unless_ctrl_is_held() {
        let mut t = ScrollTracker::new();
        let t0 = Instant::now();

        assert_eq!(
            t.handle(
                MouseScrollDelta::PixelDelta(PhysicalPosition::new(2.0, 72.0)),
                TouchPhase::Started,
                2.0,
                false,
                false,
                false,
                false,
                false,
                true,
                t0,
            ),
            ScrollAction::None
        );

        let a = t.handle(
            MouseScrollDelta::PixelDelta(PhysicalPosition::new(0.0, 72.0)),
            TouchPhase::Moved,
            2.0,
            true,
            false,
            false,
            false,
            false,
            true,
            t0 + Duration::from_millis(16),
        );
        match a {
            ScrollAction::Zoom(steps) => assert!((steps - 0.5).abs() < 1e-4),
            other => panic!("expected Zoom with Ctrl, got {other:?}"),
        }
    }

    #[test]
    fn zoomed_two_finger_scroll_pans_in_2d() {
        let mut t = ScrollTracker::new();
        let t0 = Instant::now();

        assert_eq!(
            t.handle(
                MouseScrollDelta::PixelDelta(PhysicalPosition::new(-30.0, 2.0)),
                TouchPhase::Started,
                1.0,
                false,
                false,
                true,
                true,
                false,
                true,
                t0,
            ),
            ScrollAction::Pan(-30.0, 0.0)
        );

        assert_eq!(
            t.handle(
                MouseScrollDelta::PixelDelta(PhysicalPosition::new(12.0, 40.0)),
                TouchPhase::Moved,
                1.0,
                false,
                false,
                true,
                true,
                true,
                true,
                t0 + Duration::from_millis(16),
            ),
            ScrollAction::Pan(12.0, 40.0)
        );
    }

    #[test]
    fn pinch_tracker_zooms_without_false_rotation_and_rotates_on_twist() {
        let mut pt = PinchTracker::new();
        pt.begin();
        let out = pt.update(0.0, 0.0, 1.4, 8.0, 1.4, 2.0, false, true);
        assert!((out.zoom_steps - 1.0).abs() < 1e-4);
        assert_eq!(out.rotate, None);
        let out2 = pt.update(0.0, 0.0, 1.96, 36.0, 1.4, 2.0, true, true);
        assert!((out2.zoom_steps - 1.0).abs() < 1e-4);
        assert_eq!(out2.rotate, None);
        pt.end();

        pt.begin();
        let r1 = pt.update(0.0, 0.0, 1.02, 22.0, 1.4, 1.0, false, true);
        assert_eq!(r1.rotate, None);
        assert_eq!(r1.zoom_steps, 0.0);
        let r2 = pt.update(0.0, 0.0, 1.03, 22.0, 1.4, 1.0, false, true);
        assert_eq!(r2.rotate, Some(true));
        pt.end();
    }

    #[test]
    fn discrete_mouse_wheel_zooms_immediately() {
        let mut t = ScrollTracker::new();
        let t0 = Instant::now();

        assert_eq!(
            t.handle(
                MouseScrollDelta::LineDelta(0.0, 1.0),
                TouchPhase::Moved,
                2.0,
                false,
                false,
                false,
                false,
                false,
                true,
                t0,
            ),
            ScrollAction::Zoom(1.0)
        );
    }

    #[test]
    fn touch_pinch_and_swipe_and_twist() {
        let mut tt = TouchTracker::new();
        assert_eq!(
            tt.handle(
                1,
                TouchPhase::Started,
                (100.0, 100.0),
                1.0,
                1.4,
                false,
                true
            ),
            TouchAction::None
        );
        assert_eq!(
            tt.handle(1, TouchPhase::Moved, (40.0, 102.0), 1.0, 1.4, false, true),
            TouchAction::Navigate(1)
        );
        tt.handle(1, TouchPhase::Ended, (40.0, 102.0), 1.0, 1.4, false, true);

        tt.handle(
            1,
            TouchPhase::Started,
            (100.0, 100.0),
            1.0,
            1.4,
            false,
            true,
        );
        tt.handle(
            2,
            TouchPhase::Started,
            (200.0, 100.0),
            1.0,
            1.4,
            false,
            true,
        );
        match tt.handle(2, TouchPhase::Moved, (300.0, 100.0), 1.0, 1.4, false, true) {
            TouchAction::PinchZoom { steps, .. } => assert!(steps > 1.5),
            other => panic!("expected PinchZoom, got {other:?}"),
        }

        tt.reset();
        tt.handle(
            1,
            TouchPhase::Started,
            (100.0, 100.0),
            1.0,
            1.4,
            false,
            true,
        );
        tt.handle(
            2,
            TouchPhase::Started,
            (200.0, 100.0),
            1.0,
            1.4,
            false,
            true,
        );
        let action = tt.handle(2, TouchPhase::Moved, (100.0, 200.0), 1.0, 1.4, false, true);
        assert_eq!(action, TouchAction::Rotate(true));
    }

    #[test]
    fn click_tracker_distinguishes_double_tap_from_drag() {
        let mut ct = ClickTracker::new();
        let t0 = Instant::now();

        assert!(!ct.on_press((100.0, 100.0), 1.0, t0));
        ct.on_move((101.0, 101.0), 1.0);
        ct.on_release((101.0, 101.0), t0 + Duration::from_millis(30));
        assert!(ct.on_press((102.0, 100.0), 1.0, t0 + Duration::from_millis(180)));

        assert!(!ct.on_press((100.0, 100.0), 1.0, t0 + Duration::from_millis(500)));
        ct.on_move((140.0, 100.0), 1.0);
        ct.on_release((140.0, 100.0), t0 + Duration::from_millis(560));
        assert!(!ct.on_press((140.0, 100.0), 1.0, t0 + Duration::from_millis(620)));
    }
}
