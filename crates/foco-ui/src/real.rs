//! The real implementations behind the seams: `StoreRead` for `foco_core::Store` and `PetLike`
//! over `foco_pet::Pet`.

use crate::cx::StoreRead;
use crate::pet::PetLike;
use foco_base::{Color, Mood, Ms, PetKind, Rect, StateFile, Surface, Wake};
use foco_core::Store;
use foco_pet::Pet;
use std::cell::Cell;
use std::sync::atomic::{AtomicU64, Ordering};

impl StoreRead for Store {
    fn state(&self) -> &StateFile {
        Store::state(self)
    }
    fn celebrate_until(&self) -> f64 {
        Store::celebrate_until(self)
    }
    fn first_run(&self) -> bool {
        Store::first_run(self)
    }
}

/// `foco_pet::Pet` behind `PetLike`. The pet learns its pixel size from the first `paint` (the
/// views give the rect there) and passes it on at the next `update`.
pub struct RealPet {
    pet: Pet,
    size: Cell<f32>,
    applied: f32,
}

static SEED: AtomicU64 = AtomicU64::new(0x9E37_79B9);

impl RealPet {
    pub fn new() -> RealPet {
        let n = SEED.fetch_add(0x1234_5679, Ordering::Relaxed);
        let seed = n ^ foco_base::epoch_ms().to_bits() ^ (std::process::id() as u64) << 20;
        RealPet { pet: Pet::new(PetKind::Cat, seed), size: Cell::new(32.0), applied: 32.0 }
    }
}

impl Default for RealPet {
    fn default() -> Self {
        RealPet::new()
    }
}

impl PetLike for RealPet {
    fn update(&mut self, now: Ms) -> bool {
        let sz = self.size.get();
        if sz != self.applied {
            self.pet.set_size(sz);
            self.applied = sz;
        }
        self.pet.update(now)
    }
    fn next_wake(&self, now: Ms) -> Wake {
        self.pet.next_wake(now)
    }
    fn paint(&self, s: &mut dyn Surface, rect: Rect, now: Ms) {
        self.size.set(rect.w);
        match self.pet.pose_key() {
            Some(key) => {
                let od = self.pet.overdraw() * rect.w;
                s.cached(key, rect.inflate(od, od), &mut |s| self.pet.paint(s, rect, now));
            }
            None => self.pet.paint(s, rect, now),
        }
    }
    fn react(&mut self, name: &str, ms: Ms, now: Ms) {
        self.pet.react_for(name, ms, now);
    }
    fn set_mood(&mut self, mood: Mood, now: Ms) {
        self.pet.set_mood(mood, now);
    }
    fn set_kind(&mut self, kind: PetKind, now: Ms) {
        self.pet.set_kind(kind, now);
    }
    fn set_active(&mut self, active: bool, now: Ms) {
        self.pet.set_active(active, now);
    }
    fn set_look(&mut self, x: f32, y: f32, now: Ms) {
        self.pet.set_look(x, y, now);
    }
    fn set_talking(&mut self, talking: bool, now: Ms) {
        self.pet.set_talking(talking, now);
    }
    /// Pixels: the pet reports a fraction of its width.
    fn overdraw(&self) -> f32 {
        self.pet.overdraw() * self.size.get()
    }
    fn set_accent(&mut self, accent: Color) {
        self.pet.set_accent(accent);
    }
    fn set_hovered(&mut self, hovered: bool, now: Ms) {
        self.pet.set_hover(hovered, now);
    }
    fn set_size(&mut self, size: f32) {
        self.size.set(size);
    }
    fn tap(&mut self, now: Ms) {
        self.pet.tap(now);
    }
    fn pointer(&mut self, fx: f32, fy: f32, now: Ms) {
        self.pet.pointer(fx, fy, now);
    }
}
