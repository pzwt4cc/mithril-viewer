use std::time::{Duration, Instant};

use image::{RgbaImage, imageops};

pub struct Anim {
    frames: Vec<RgbaImage>,
    delays: Vec<Duration>,
    index: usize,
    next_at: Instant,
    dirty: bool,
}

impl Anim {
    pub fn new(frames: Vec<RgbaImage>, delays: Vec<Duration>) -> Self {
        let next_at = Instant::now() + delays[0];
        Self {
            frames,
            delays,
            index: 0,
            next_at,
            dirty: false,
        }
    }

    pub fn tick(&mut self, now: Instant) -> bool {
        if now < self.next_at {
            return false;
        }
        self.index = (self.index + 1) % self.frames.len();
        let base = if now > self.next_at + Duration::from_millis(200) {
            now
        } else {
            self.next_at
        };
        self.next_at = base + self.delays[self.index];
        self.dirty = true;
        true
    }

    pub fn next_at(&self) -> Instant {
        self.next_at
    }

    pub fn take_dirty(&mut self) -> Option<&RgbaImage> {
        if self.dirty {
            self.dirty = false;
            Some(&self.frames[self.index])
        } else {
            None
        }
    }

    pub fn rotate(&mut self, clockwise: bool) {
        for f in self.frames.iter_mut() {
            *f = if clockwise {
                imageops::rotate90(f)
            } else {
                imageops::rotate270(f)
            };
        }
        self.dirty = true;
    }

    pub fn flip(&mut self, horizontal: bool) {
        for f in self.frames.iter_mut() {
            if horizontal {
                imageops::flip_horizontal_in_place(f);
            } else {
                imageops::flip_vertical_in_place(f);
            }
        }
        self.dirty = true;
    }

    pub fn current(&self) -> &RgbaImage {
        &self.frames[self.index]
    }
}
