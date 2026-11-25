//! Wheel scrolling of the expanded body (`Flickable`, `StopAtBounds`).

use foco_base::{Anim, Easing, Ms, Wake};

/// Scroll offset with 200 ms OutCubic wheel steps.
#[derive(Clone, Copy, Debug)]
pub struct Scroll {
    off: Anim,
    max: f32,
    view_h: f32,
    content_h: f32,
}

impl Default for Scroll {
    fn default() -> Self {
        Scroll::new()
    }
}

impl Scroll {
    pub fn new() -> Scroll {
        Scroll { off: Anim::new(0.0), max: 0.0, view_h: 0.0, content_h: 0.0 }
    }
    /// `interactive: contentHeight > height + 1`.
    pub fn scrollable(&self) -> bool {
        self.content_h > self.view_h + 1.0
    }
    /// Sets viewport and content heights; clamps instantly when the content shrank.
    pub fn set_sizes(&mut self, view_h: f32, content_h: f32, now: Ms) {
        self.view_h = view_h;
        self.content_h = content_h;
        self.max = (content_h - view_h).max(0.0);
        let cur = self.off.value(now);
        if self.off.target() > self.max || cur > self.max {
            self.off.set(cur.min(self.max));
        }
    }
    /// Wheel step (`dy` px, positive = content moves up). True when the target changed.
    pub fn wheel(&mut self, dy: f32, now: Ms) -> bool {
        if !self.scrollable() {
            return false;
        }
        let t = (self.off.target() + dy).clamp(0.0, self.max);
        if t == self.off.target() {
            return false;
        }
        self.off.animate_to(t, 200.0, Easing::OutCubic, now);
        true
    }
    /// Current offset in px (0 = top).
    pub fn offset(&self, now: Ms) -> f32 {
        self.off.value(now)
    }
    pub fn target(&self) -> f32 {
        self.off.target()
    }
    pub fn max(&self) -> f32 {
        self.max
    }
    /// Back to the top, instantly.
    pub fn reset(&mut self) {
        self.off.set(0.0);
    }
    pub fn is_animating(&self, now: Ms) -> bool {
        self.off.is_animating(now)
    }
    pub fn next_wake(&self, now: Ms) -> Wake {
        if self.is_animating(now) {
            Wake::Frame
        } else {
            Wake::Idle
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wheel_clamps_and_animates() {
        let mut s = Scroll::new();
        s.set_sizes(400.0, 1000.0, 0.0);
        assert!(s.scrollable());
        assert!(s.wheel(120.0, 0.0));
        assert!(s.is_animating(100.0));
        assert!(s.offset(100.0) > 0.0 && s.offset(100.0) < 120.0);
        assert_eq!(s.offset(200.0), 120.0);
        s.wheel(10_000.0, 300.0);
        assert_eq!(s.target(), 600.0);
        assert!(!s.wheel(10.0, 400.0) || s.target() <= 600.0);
        assert!(!s.wheel(-1.0e9, 1000.0) || s.target() == 0.0);
        // the content shrinks: clamp at once
        s.wheel(10_000.0, 2000.0);
        s.set_sizes(400.0, 700.0, 3000.0);
        assert_eq!(s.offset(3000.0), 300.0);
        assert_eq!(s.target(), 300.0);
        // not scrollable within 1 px
        s.set_sizes(400.0, 400.5, 3001.0);
        assert!(!s.scrollable());
        assert!(!s.wheel(10.0, 3001.0));
    }
}
