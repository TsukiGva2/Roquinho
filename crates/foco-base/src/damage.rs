//! Damage tracking: up to 6 dirty rectangles, collapsing to their bounding box beyond that.

use crate::geom::IRect;

/// Max number of tracked rectangles.
pub const MAX_DAMAGE_RECTS: usize = 6;

/// A small set of dirty rectangles (device pixels).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Damage {
    rects: [IRect; MAX_DAMAGE_RECTS],
    len: usize,
}

impl Damage {
    pub fn new() -> Damage {
        Damage::default()
    }
    /// Adds a dirty rect. Empty rects are ignored; overlapping or edge-adjacent rects are
    /// merged (bounding box); if more than 6 remain they collapse into one bounding box.
    pub fn add(&mut self, r: IRect) {
        if r.is_empty() {
            return;
        }
        let mut r = r;
        let mut i = 0;
        while i < self.len {
            if self.rects[i].touches(&r) {
                r = r.union(&self.rects[i]);
                self.len -= 1;
                self.rects[i] = self.rects[self.len];
                i = 0; // the grown rect may now touch others already skipped
            } else {
                i += 1;
            }
        }
        if self.len == MAX_DAMAGE_RECTS {
            let mut b = r;
            for q in &self.rects[..self.len] {
                b = b.union(q);
            }
            self.rects[0] = b;
            self.len = 1;
        } else {
            self.rects[self.len] = r;
            self.len += 1;
        }
    }
    /// The current rectangles (order unspecified).
    pub fn rects(&self) -> &[IRect] {
        &self.rects[..self.len]
    }
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
    /// Takes the rectangles out and clears the set.
    pub fn take(&mut self) -> Vec<IRect> {
        let v = self.rects().to_vec();
        self.len = 0;
        v
    }
    pub fn clear(&mut self) {
        self.len = 0;
    }
    /// Bounding box of everything (empty rect when clean).
    pub fn bounds(&self) -> IRect {
        self.rects().iter().fold(IRect::default(), |a, b| a.union(b))
    }
    /// Clips every rect to `bounds`, dropping those that fall outside.
    pub fn clip_to(&mut self, bounds: IRect) {
        let old = self.take();
        for r in old {
            if let Some(c) = r.intersect(&bounds) {
                self.rects[self.len] = c;
                self.len += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merges_overlap_and_adjacent() {
        let mut d = Damage::new();
        d.add(IRect::new(0, 0, 10, 10));
        d.add(IRect::new(100, 100, 5, 5));
        assert_eq!(d.rects().len(), 2);
        d.add(IRect::new(5, 5, 10, 10)); // overlaps first
        assert_eq!(d.rects().len(), 2);
        d.add(IRect::new(15, 0, 5, 5)); // adjacent to merged (0,0,15,15)
        assert_eq!(d.rects().len(), 2);
        assert!(d.rects().contains(&IRect::new(0, 0, 20, 15)));
        d.add(IRect::new(0, 0, 0, 4)); // empty ignored
        assert_eq!(d.rects().len(), 2);
    }

    #[test]
    fn chain_merge() {
        let mut d = Damage::new();
        d.add(IRect::new(0, 0, 10, 10));
        d.add(IRect::new(30, 0, 10, 10));
        d.add(IRect::new(8, 0, 24, 10)); // bridges both
        assert_eq!(d.rects(), &[IRect::new(0, 0, 40, 10)]);
    }

    #[test]
    fn collapses_beyond_six() {
        let mut d = Damage::new();
        for i in 0..6 {
            d.add(IRect::new(i * 100, 0, 10, 10));
        }
        assert_eq!(d.rects().len(), 6);
        d.add(IRect::new(700, 50, 10, 10));
        assert_eq!(d.rects(), &[IRect::new(0, 0, 710, 60)]);
    }

    #[test]
    fn take_and_clip() {
        let mut d = Damage::new();
        d.add(IRect::new(-10, -10, 20, 20));
        d.add(IRect::new(500, 500, 10, 10));
        d.clip_to(IRect::new(0, 0, 100, 100));
        assert_eq!(d.rects(), &[IRect::new(0, 0, 10, 10)]);
        assert_eq!(d.bounds(), IRect::new(0, 0, 10, 10));
        let t = d.take();
        assert_eq!(t.len(), 1);
        assert!(d.is_empty());
    }
}
