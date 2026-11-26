//! `ColorAnimation`: a colour that eases (linear) to a new target.

use foco_base::{Anim, Color, Easing, Ms};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ColorAnim {
    from: Color,
    to: Color,
    t: Anim,
}

impl ColorAnim {
    pub fn new(c: Color) -> ColorAnim {
        ColorAnim { from: c, to: c, t: Anim::new(1.0) }
    }
    /// Jumps to `c`.
    pub fn set(&mut self, c: Color) {
        *self = ColorAnim::new(c);
    }
    /// Eases from the current value to `c` over `dur` ms. No-op when `c` is already the target.
    pub fn animate_to(&mut self, c: Color, dur: Ms, now: Ms) {
        if c == self.to {
            return;
        }
        self.from = self.value(now);
        self.to = c;
        self.t.set(0.0);
        self.t.animate_to(1.0, dur, Easing::Linear, now);
    }
    pub fn value(&self, now: Ms) -> Color {
        let p = self.t.value(now);
        if p >= 1.0 {
            self.to
        } else {
            self.from.lerp(self.to, p)
        }
    }
    pub fn target(&self) -> Color {
        self.to
    }
    pub fn is_animating(&self, now: Ms) -> bool {
        self.t.is_animating(now)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eases_and_settles() {
        let mut c = ColorAnim::new(Color::BLACK);
        c.animate_to(Color::WHITE, 100.0, 1000.0);
        assert!(c.is_animating(1050.0));
        let mid = c.value(1050.0);
        assert!((mid.r - 0.5).abs() < 1e-5);
        assert_eq!(c.value(1100.0), Color::WHITE);
        assert!(!c.is_animating(1100.0));
        // retarget mid-flight starts from the current value
        let mut c = ColorAnim::new(Color::BLACK);
        c.animate_to(Color::WHITE, 100.0, 0.0);
        c.animate_to(Color::BLACK, 100.0, 50.0);
        assert!((c.value(50.0).r - 0.5).abs() < 1e-5);
        c.animate_to(Color::BLACK, 100.0, 60.0); // same target: no restart
        assert!((c.value(100.0).r - 0.25).abs() < 1e-5);
    }
}
