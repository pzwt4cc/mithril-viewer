use std::time::{Duration, Instant};
use winit::event::{MouseScrollDelta, TouchPhase};

const IDLE_RESET: Duration = Duration::from_millis(180);
const AXIS_SLOP_PX: f32 = 8.0;
const AXIS_LOCK_RATIO: f32 = 1.25;
const SWIPE_THRESHOLD_PX: f32 = 44.0;
const TOUCHPAD_ZOOM_PX_PER_STEP: f32 = 72.0;
const LINE_TO_PX: f32 = 40.0;
const TOUCH_SWIPE_PX: f64 = 48.0;
const DOUBLE_CLICK_MS: u128 = 350;
const DOUBLE_CLICK_DIST_PX: f64 = 8.0;
const DRAG_SLOP_PX: f64 = 4.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GestureAxis {
    Undecided,
    Horizontal,
    Vertical,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ScrollAction {
    None,
    Zoom(f32),
    Pan(f32, f32),
    Navigate(isize),
}

pub struct ScrollTracker {
    axis: GestureAxis,
    acc_x: f32,
    acc_y: f32,
    nav_triggered: bool,
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
            axis: GestureAxis::Undecided,
            acc_x: 0.0,
            acc_y: 0.0,
            nav_triggered: false,
            last_event: None,
        }
    }

    pub fn reset(&mut self) {
        self.axis = GestureAxis::Undecided;
        self.acc_x = 0.0;
        self.acc_y = 0.0;
        self.nav_triggered = false;
        self.last_event = None;
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

        if phase == TouchPhase::Started
            || self
                .last_event
                .is_some_and(|t| now.duration_since(t) >= IDLE_RESET)
        {
            self.reset();
        }

        let in_smooth_gesture = self.axis != GestureAxis::Undecided
            || self.acc_x != 0.0
            || self.acc_y != 0.0
            || self.nav_triggered;

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
                ScrollAction::Pan(dx, dy)
            } else {
                ScrollAction::None
            };
            if ended {
                self.reset();
            }
            return action;
        }

        self.acc_x += dx;
        self.acc_y += dy;

        let mut just_locked_horizontal = false;
        if self.axis == GestureAxis::Undecided {
            if self.acc_x.hypot(self.acc_y) >= AXIS_SLOP_PX {
                if self.acc_x.abs() > self.acc_y.abs() * AXIS_LOCK_RATIO {
                    self.axis = GestureAxis::Horizontal;
                    just_locked_horizontal = true;
                } else {
                    self.axis = GestureAxis::Vertical;
                    let total_y = self.acc_y;
                    if ended {
                        self.reset();
                    }
                    return if total_y != 0.0 {
                        ScrollAction::Zoom(total_y / TOUCHPAD_ZOOM_PX_PER_STEP)
                    } else {
                        ScrollAction::None
                    };
                }
            } else {
                if ended {
                    self.reset();
                }
                return ScrollAction::None;
            }
        }

        let action = match self.axis {
            GestureAxis::Vertical => {
                if dy != 0.0 {
                    ScrollAction::Zoom(dy / TOUCHPAD_ZOOM_PX_PER_STEP)
                } else {
                    ScrollAction::None
                }
            }
            GestureAxis::Horizontal => {
                if is_zoomed {
                    if can_pan_x {
                        let pan_x = if just_locked_horizontal {
                            self.acc_x
                        } else {
                            dx
                        };
                        ScrollAction::Pan(pan_x, 0.0)
                    } else {
                        ScrollAction::None
                    }
                } else if allow_nav && !self.nav_triggered && self.acc_x.abs() >= SWIPE_THRESHOLD_PX
                {
                    self.nav_triggered = true;
                    ScrollAction::Navigate(if self.acc_x < 0.0 { 1 } else { -1 })
                } else {
                    ScrollAction::None
                }
            }
            GestureAxis::Undecided => ScrollAction::None,
        };

        if ended {
            self.reset();
        }
        action
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
}

pub struct TouchTracker {
    points: Vec<(u64, (f64, f64))>,
    start_pos: (f64, f64),
    nav_triggered: bool,
    pinched: bool,
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
        }
    }

    pub fn reset(&mut self) {
        self.points.clear();
        self.nav_triggered = false;
        self.pinched = false;
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
                } else if self.points.len() >= 2 {
                    self.pinched = true;
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
                true,
                t0 + Duration::from_millis(60),
            ),
            ScrollAction::Navigate(-1)
        );
    }

    #[test]
    fn touchpad_vertical_zoom_normalizes_for_hidpi() {
        let mut t = ScrollTracker::new();
        let t0 = Instant::now();

        let a = t.handle(
            MouseScrollDelta::PixelDelta(PhysicalPosition::new(2.0, 72.0)),
            TouchPhase::Started,
            2.0,
            false,
            false,
            false,
            false,
            true,
            t0,
        );
        match a {
            ScrollAction::Zoom(steps) => assert!((steps - 0.5).abs() < 1e-4),
            other => panic!("expected Zoom, got {other:?}"),
        }
    }

    #[test]
    fn zoomed_horizontal_swipe_pans_when_wider_than_viewport() {
        let mut t = ScrollTracker::new();
        let t0 = Instant::now();

        let a = t.handle(
            MouseScrollDelta::PixelDelta(PhysicalPosition::new(-30.0, 2.0)),
            TouchPhase::Started,
            1.0,
            false,
            false,
            true,
            true,
            true,
            t0,
        );
        assert_eq!(a, ScrollAction::Pan(-30.0, 0.0));
    }

    #[test]
    fn x11_smooth_scroll_resets_after_idle_gap() {
        let mut t = ScrollTracker::new();
        let t0 = Instant::now();

        assert_eq!(
            t.handle(
                MouseScrollDelta::LineDelta(-0.8, 0.0),
                TouchPhase::Moved,
                1.0,
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
                MouseScrollDelta::LineDelta(-0.5, 0.0),
                TouchPhase::Moved,
                1.0,
                false,
                false,
                false,
                false,
                true,
                t0 + Duration::from_millis(16),
            ),
            ScrollAction::Navigate(1)
        );
        assert_eq!(
            t.handle(
                MouseScrollDelta::LineDelta(1.2, 0.0),
                TouchPhase::Moved,
                1.0,
                false,
                false,
                false,
                false,
                true,
                t0 + Duration::from_millis(250),
            ),
            ScrollAction::Navigate(-1)
        );
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
                true,
                t0,
            ),
            ScrollAction::Zoom(1.0)
        );
    }

    #[test]
    fn touch_pinch_and_swipe() {
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
