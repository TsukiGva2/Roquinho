//! The seam between views and the mascot.
//!
//! Views hold a `Box<dyn PetLike>` created by [`new_pet`], which returns `real::RealPet` (over
//! `foco_pet::Pet`); [`NullPet`] is a flat placeholder for tests. Dynamic dispatch
//! costs nothing next to a pet paint, and it keeps every view constructor argument-free.

use foco_base::{path_circle, Color, Fill, Ms, Mood, PetKind, Rect, Surface, Wake};

/// What a view needs from a mascot. `now` is monotonic ms.
pub trait PetLike {
    /// Advances ambient animation to `now`; returns true when the picture changed (the view then
    /// damages `rect.inflate(overdraw)`).
    fn update(&mut self, now: Ms) -> bool;
    /// `Ambient` while breathing (30 Hz), `At(..)` for blink/fidget, `Idle` when inactive.
    fn next_wake(&self, now: Ms) -> Wake;
    /// Paints the pet into `rect` (its `size` box) in the current transform.
    fn paint(&self, s: &mut dyn Surface, rect: Rect, now: Ms);
    /// `Pet.react(name, ms)`; ignored when the pet is not active.
    fn react(&mut self, name: &str, ms: Ms, now: Ms);
    fn set_mood(&mut self, mood: Mood, now: Ms);
    fn set_kind(&mut self, kind: PetKind, now: Ms);
    /// `animated && visible`: inactive pets neither animate nor wake.
    fn set_active(&mut self, active: bool, now: Ms);
    /// Pupil look target (`lookX` in -1.8..1.8, `lookY` in -1.2..1.2); `(0,0)` resets.
    fn set_look(&mut self, x: f32, y: f32, now: Ms);
    /// Talking mouth (tip bubble typing).
    fn set_talking(&mut self, talking: bool, now: Ms);
    /// Extra pixels around the size box that the pet may paint on (hearts, "!", arms).
    fn overdraw(&self) -> f32;
    /// Accent colour (the slime body follows it). Default: ignored.
    fn set_accent(&mut self, _accent: Color) {}
    /// Pointer is over an interactive pet (expression "content"). Default: ignored.
    fn set_hovered(&mut self, _hovered: bool, _now: Ms) {}
    /// Size (px) the pet is painted at; only used to quantize repaint decisions. Optional: the
    /// real pet also learns it from `paint`.
    fn set_size(&mut self, _size: f32) {}
    /// A tap on an interactive pet: `react("love", 1900)`.
    fn tap(&mut self, now: Ms) {
        self.react("love", 1900.0, now);
    }
    /// Pointer over an interactive pet at fraction `(fx, fy)` (0..1) of its box
    /// (`HoverHandler.onPointChanged`): hover + look target.
    fn pointer(&mut self, fx: f32, fy: f32, now: Ms) {
        self.set_hovered(true, now);
        self.set_look(((fx - 0.5) * 4.0).clamp(-1.8, 1.8), ((fy - 0.5) * 3.0).clamp(-1.2, 1.2), now);
    }
}

/// Creates the pet used by a view. Single wiring point for foco-pet.
pub fn new_pet() -> Box<dyn PetLike> {
    // unit tests of the views assert exact wake behaviour: they get the silent placeholder
    #[cfg(test)]
    {
        Box::new(NullPet::default())
    }
    #[cfg(not(test))]
    {
        Box::new(crate::real::RealPet::new())
    }
}

/// Placeholder pet: a flat coloured disc, never animates, never wakes.
#[derive(Debug, Default)]
pub struct NullPet {
    kind: PetKind,
    active: bool,
}

impl PetLike for NullPet {
    fn update(&mut self, _now: Ms) -> bool {
        false
    }
    fn next_wake(&self, _now: Ms) -> Wake {
        Wake::Idle
    }
    fn paint(&self, s: &mut dyn Surface, rect: Rect, _now: Ms) {
        let body = match self.kind {
            PetKind::Cat => Color::rgb8(0x23, 0x20, 0x28),
            PetKind::Ghost => Color::rgb8(0xEE, 0xEE, 0xF4),
            PetKind::Slime => Color::rgb8(0x6f, 0x8c, 0xf5),
        };
        let c = rect.center();
        if let Some(p) = path_circle(c.x, c.y, rect.w.min(rect.h) * 0.42) {
            s.fill_path(&p, &Fill::Solid(body));
        }
        let ink = Fill::Solid(Color::rgb8(0x1d, 0x1d, 0x1f));
        let e = rect.w * 0.12;
        for dx in [-e, e] {
            if let Some(p) = path_circle(c.x + dx, c.y - rect.h * 0.04, rect.w * 0.05) {
                s.fill_path(&p, &ink);
            }
        }
    }
    fn react(&mut self, _name: &str, _ms: Ms, _now: Ms) {}
    fn set_mood(&mut self, _mood: Mood, _now: Ms) {}
    fn set_kind(&mut self, kind: PetKind, _now: Ms) {
        self.kind = kind;
    }
    fn set_active(&mut self, active: bool, _now: Ms) {
        self.active = active;
    }
    fn set_look(&mut self, _x: f32, _y: f32, _now: Ms) {}
    fn set_talking(&mut self, _talking: bool, _now: Ms) {}
    fn overdraw(&self) -> f32 {
        0.0
    }
}
