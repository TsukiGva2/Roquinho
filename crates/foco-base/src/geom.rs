//! Basic geometry: float and integer points/sizes/rectangles (logical pixels).

/// A point in logical pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    pub const fn new(x: f32, y: f32) -> Self {
        Point { x, y }
    }
}

/// A size in logical pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Size {
    pub w: f32,
    pub h: f32,
}

impl Size {
    pub const fn new(w: f32, h: f32) -> Self {
        Size { w, h }
    }
}

/// A float rectangle (`x`,`y` = top-left).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Rect { x, y, w, h }
    }
    /// Right edge (`x + w`).
    pub fn right(&self) -> f32 {
        self.x + self.w
    }
    /// Bottom edge (`y + h`).
    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }
    /// True when width or height is not positive.
    pub fn is_empty(&self) -> bool {
        !(self.w > 0.0 && self.h > 0.0)
    }
    /// Centre point.
    pub fn center(&self) -> Point {
        Point::new(self.x + self.w * 0.5, self.y + self.h * 0.5)
    }
    pub fn size(&self) -> Size {
        Size::new(self.w, self.h)
    }
    /// Half-open containment: `x <= p.x < right`, same for y.
    pub fn contains(&self, p: Point) -> bool {
        p.x >= self.x && p.x < self.right() && p.y >= self.y && p.y < self.bottom()
    }
    /// True when the interiors overlap (touching edges do not count).
    pub fn intersects(&self, o: &Rect) -> bool {
        self.x < o.right() && o.x < self.right() && self.y < o.bottom() && o.y < self.bottom()
    }
    /// Overlap of the two rects, `None` when they do not overlap.
    pub fn intersection(&self, o: &Rect) -> Option<Rect> {
        let x = self.x.max(o.x);
        let y = self.y.max(o.y);
        let r = self.right().min(o.right());
        let b = self.bottom().min(o.bottom());
        if r > x && b > y {
            Some(Rect::new(x, y, r - x, b - y))
        } else {
            None
        }
    }
    /// Bounding box of both; empty rects are ignored.
    pub fn union(&self, o: &Rect) -> Rect {
        if self.is_empty() {
            return *o;
        }
        if o.is_empty() {
            return *self;
        }
        let x = self.x.min(o.x);
        let y = self.y.min(o.y);
        Rect::new(x, y, self.right().max(o.right()) - x, self.bottom().max(o.bottom()) - y)
    }
    /// Grows by `dx` on left and right and `dy` on top and bottom (negative shrinks).
    pub fn inflate(&self, dx: f32, dy: f32) -> Rect {
        Rect::new(self.x - dx, self.y - dy, self.w + 2.0 * dx, self.h + 2.0 * dy)
    }
    pub fn translate(&self, dx: f32, dy: f32) -> Rect {
        Rect::new(self.x + dx, self.y + dy, self.w, self.h)
    }
    /// Smallest integer rect containing this one (floor left/top, ceil right/bottom).
    pub fn round_out(&self) -> IRect {
        let x0 = self.x.floor() as i32;
        let y0 = self.y.floor() as i32;
        let x1 = self.right().ceil() as i32;
        let y1 = self.bottom().ceil() as i32;
        IRect::new(x0, y0, x1 - x0, y1 - y0)
    }
}

/// An integer rectangle in device pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct IRect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl IRect {
    pub const fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        IRect { x, y, w, h }
    }
    pub fn right(&self) -> i32 {
        self.x + self.w
    }
    pub fn bottom(&self) -> i32 {
        self.y + self.h
    }
    pub fn is_empty(&self) -> bool {
        self.w <= 0 || self.h <= 0
    }
    pub fn area(&self) -> i64 {
        if self.is_empty() {
            0
        } else {
            self.w as i64 * self.h as i64
        }
    }
    /// Half-open containment.
    pub fn contains_point(&self, x: i32, y: i32) -> bool {
        x >= self.x && x < self.right() && y >= self.y && y < self.bottom()
    }
    /// Bounding box; empty rects are ignored.
    pub fn union(&self, o: &IRect) -> IRect {
        if self.is_empty() {
            return *o;
        }
        if o.is_empty() {
            return *self;
        }
        let x = self.x.min(o.x);
        let y = self.y.min(o.y);
        IRect::new(x, y, self.right().max(o.right()) - x, self.bottom().max(o.bottom()) - y)
    }
    /// Overlap, `None` when empty.
    pub fn intersect(&self, o: &IRect) -> Option<IRect> {
        let x = self.x.max(o.x);
        let y = self.y.max(o.y);
        let r = self.right().min(o.right());
        let b = self.bottom().min(o.bottom());
        if r > x && b > y {
            Some(IRect::new(x, y, r - x, b - y))
        } else {
            None
        }
    }
    /// True when the two rects overlap or share an edge segment (used for damage merging).
    pub fn touches(&self, o: &IRect) -> bool {
        self.x <= o.right() && o.x <= self.right() && self.y <= o.bottom() && o.y <= self.bottom()
    }
    pub fn to_rect(&self) -> Rect {
        Rect::new(self.x as f32, self.y as f32, self.w as f32, self.h as f32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_ops() {
        let a = Rect::new(0.0, 0.0, 10.0, 10.0);
        let b = Rect::new(5.0, 5.0, 10.0, 10.0);
        assert!(a.intersects(&b));
        assert_eq!(a.intersection(&b), Some(Rect::new(5.0, 5.0, 5.0, 5.0)));
        assert_eq!(a.union(&b), Rect::new(0.0, 0.0, 15.0, 15.0));
        let c = Rect::new(10.0, 0.0, 5.0, 5.0);
        assert!(!a.intersects(&c));
        assert_eq!(a.intersection(&c), None);
        assert!(a.contains(Point::new(0.0, 0.0)));
        assert!(!a.contains(Point::new(10.0, 5.0)));
        assert_eq!(a.inflate(2.0, 1.0), Rect::new(-2.0, -1.0, 14.0, 12.0));
        assert_eq!(a.translate(1.0, 2.0), Rect::new(1.0, 2.0, 10.0, 10.0));
        assert_eq!(a.center(), Point::new(5.0, 5.0));
        assert_eq!(Rect::new(0.5, 0.5, 1.0, 1.0).round_out(), IRect::new(0, 0, 2, 2));
        assert_eq!(Rect::default().union(&a), a);
    }

    #[test]
    fn irect_ops() {
        let a = IRect::new(0, 0, 10, 10);
        let b = IRect::new(10, 0, 5, 5);
        assert_eq!(a.intersect(&b), None);
        assert!(a.touches(&b));
        assert_eq!(a.union(&b), IRect::new(0, 0, 15, 10));
        assert_eq!(a.area(), 100);
        assert!(IRect::new(0, 0, 0, 5).is_empty());
        assert_eq!(IRect::new(0, 0, 0, 5).area(), 0);
        assert!(a.contains_point(9, 9));
        assert!(!a.contains_point(10, 9));
    }
}
