//! Keyed list animator (ui-design.md §6): QML `ListView` `add` / `remove` / `displaced`
//! transitions without a scene graph.
//!
//! The owner computes each row's slot `y` (relative to the list top) and calls [`KeyedList::sync`]
//! whenever the model changed; it then draws [`KeyedList::iter`] entries at `slot + Fx`. As in
//! QML the list height is the *final* content height; removed rows keep playing their remove
//! transition after they left the model.

use foco_base::{Anim, Easing, Ms, Rect};

/// One animated property of a transition. `from` only matters for `add` (start value; `None` =
/// the neutral value); `to` is the end value (for `remove` the value the row leaves at).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Track {
    pub from: Option<f32>,
    pub to: f32,
    pub dur: Ms,
    pub easing: Easing,
}

impl Track {
    pub fn new(from: Option<f32>, to: f32, dur: Ms, easing: Easing) -> Track {
        Track { from, to, dur, easing }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Transition {
    pub opacity: Option<Track>,
    pub scale: Option<Track>,
    /// Horizontal offset.
    pub x: Option<Track>,
}

impl Transition {
    fn max_dur(&self) -> Ms {
        [self.opacity, self.scale, self.x]
            .iter()
            .flatten()
            .map(|t| t.dur)
            .fold(0.0, f64::max)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ListSpec {
    pub add: Transition,
    pub remove: Transition,
    /// `displaced`: y animation of rows moved by an add/remove.
    pub displaced: (Ms, Easing),
    /// `moveDisplaced`: y animation while a row is being dragged.
    pub move_displaced: (Ms, Easing),
}

impl ListSpec {
    /// `MainSections.qml:224-236` (focus cards).
    pub fn focus_list() -> ListSpec {
        ListSpec {
            add: Transition {
                opacity: Some(Track::new(Some(0.0), 1.0, 320.0, Easing::Linear)),
                scale: Some(Track::new(Some(0.9), 1.0, 460.0, Easing::OUT_BACK)),
                x: None,
            },
            remove: Transition {
                opacity: Some(Track::new(None, 0.0, 280.0, Easing::Linear)),
                scale: None,
                x: Some(Track::new(None, 80.0, 320.0, Easing::InCubic)),
            },
            displaced: (340.0, Easing::OutCubic),
            move_displaced: (340.0, Easing::OutCubic),
        }
    }
    /// `QueueList.qml:15-28` (queue rows).
    pub fn queue() -> ListSpec {
        ListSpec {
            add: Transition {
                opacity: Some(Track::new(Some(0.0), 1.0, 260.0, Easing::Linear)),
                scale: None,
                x: Some(Track::new(Some(-24.0), 0.0, 340.0, Easing::OutCubic)),
            },
            remove: Transition {
                opacity: Some(Track::new(None, 0.0, 200.0, Easing::Linear)),
                scale: Some(Track::new(None, 0.9, 200.0, Easing::Linear)),
                x: None,
            },
            displaced: (260.0, Easing::OutCubic),
            move_displaced: (220.0, Easing::OutCubic),
        }
    }
}

/// Visual offsets of a row at some time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fx {
    /// Horizontal offset.
    pub dx: f32,
    /// Animated slot y (relative to the list top).
    pub y: f32,
    pub opacity: f32,
    /// Scale about the row centre.
    pub scale: f32,
}

#[derive(Clone, Debug)]
pub struct Entry<T> {
    pub key: String,
    pub data: T,
    /// Model index; `None` while the row plays its remove transition.
    pub index: Option<usize>,
    y: Anim,
    dx: Anim,
    op: Anim,
    sc: Anim,
    gone_at: Ms,
    seen: bool,
}

impl<T> Entry<T> {
    fn fx(&self, now: Ms) -> Fx {
        Fx { dx: self.dx.value(now), y: self.y.value(now), opacity: self.op.value(now), scale: self.sc.value(now) }
    }
    fn alive(&self, now: Ms) -> bool {
        self.index.is_some() || now < self.gone_at
    }
    fn animating(&self, now: Ms) -> bool {
        self.y.is_animating(now)
            || self.dx.is_animating(now)
            || self.op.is_animating(now)
            || self.sc.is_animating(now)
            || (self.index.is_none() && now < self.gone_at)
    }
}

#[derive(Clone, Debug)]
pub struct KeyedList<T> {
    spec: ListSpec,
    entries: Vec<Entry<T>>,
    primed: bool,
}

fn apply(anim: &mut Anim, tr: Option<&Track>, now: Ms, add: bool) {
    if let Some(t) = tr {
        if add {
            if let Some(f) = t.from {
                anim.set(f);
            }
        }
        anim.animate_to(t.to, t.dur, t.easing, now);
    }
}

impl<T: Clone> KeyedList<T> {
    pub fn new(spec: ListSpec) -> KeyedList<T> {
        KeyedList { spec, entries: Vec::new(), primed: false }
    }

    /// Reconciles with `items` (key, data, slot y relative to the list top), in model order.
    /// The first call is instant. New keys play `add`; vanished keys are kept with their last data
    /// and play `remove`; live rows whose slot moved animate y (`displaced`, or `move_displaced`
    /// while `dragging.is_some()`). The dragged row's own y is not animated. Returns true when
    /// anything changed (structure or slots).
    pub fn sync<'k>(
        &mut self,
        items: impl Iterator<Item = (&'k str, T, f32)>,
        now: Ms,
        dragging: Option<&str>,
    ) -> bool {
        let first = !self.primed;
        self.primed = true;
        let mut changed = false;
        for e in &mut self.entries {
            e.seen = false;
        }
        for (i, (key, data, slot)) in items.enumerate() {
            if let Some(e) = self.entries.iter_mut().find(|e| e.index.is_some() && !e.seen && e.key == key) {
                e.data = data;
                e.seen = true;
                if e.index != Some(i) {
                    e.index = Some(i);
                    changed = true;
                }
                if dragging == Some(key) {
                    if e.y.target() != slot {
                        changed = true;
                    }
                    e.y.set(slot);
                } else if e.y.target() != slot {
                    let (dur, ease) = if dragging.is_some() { self.spec.move_displaced } else { self.spec.displaced };
                    e.y.animate_to(slot, dur, ease, now);
                    changed = true;
                }
            } else {
                let mut e = Entry {
                    key: key.to_string(),
                    data,
                    index: Some(i),
                    y: Anim::new(slot),
                    dx: Anim::new(0.0),
                    op: Anim::new(1.0),
                    sc: Anim::new(1.0),
                    gone_at: f64::INFINITY,
                    seen: true,
                };
                if !first {
                    apply(&mut e.op, self.spec.add.opacity.as_ref(), now, true);
                    apply(&mut e.sc, self.spec.add.scale.as_ref(), now, true);
                    apply(&mut e.dx, self.spec.add.x.as_ref(), now, true);
                }
                self.entries.push(e);
                changed = true;
            }
        }
        for e in &mut self.entries {
            if e.index.is_some() && !e.seen {
                e.index = None;
                e.gone_at = now + self.spec.remove.max_dur();
                apply(&mut e.op, self.spec.remove.opacity.as_ref(), now, false);
                apply(&mut e.sc, self.spec.remove.scale.as_ref(), now, false);
                apply(&mut e.dx, self.spec.remove.x.as_ref(), now, false);
                changed = true;
            }
        }
        if changed {
            // live rows in model order, then the removing ones (drawn on top)
            self.entries.sort_by_key(|e| match e.index {
                Some(i) => (0u8, i),
                None => (1u8, 0),
            });
        }
        changed
    }

    /// Rows to draw: live ones in model order, then removing ones (on top).
    pub fn iter(&self, now: Ms) -> impl Iterator<Item = (&Entry<T>, Fx)> {
        self.entries.iter().filter(move |e| e.alive(now)).map(move |e| (e, e.fx(now)))
    }

    /// Drops rows whose remove transition finished; true when something was dropped.
    pub fn prune(&mut self, now: Ms) -> bool {
        let n = self.entries.len();
        self.entries.retain(|e| e.alive(now));
        self.entries.len() != n
    }

    pub fn is_animating(&self, now: Ms) -> bool {
        self.entries.iter().any(|e| e.animating(now))
    }

    /// `Frame` while anything animates.
    pub fn next_wake(&self, now: Ms) -> foco_base::Wake {
        if self.is_animating(now) {
            foco_base::Wake::Frame
        } else {
            foco_base::Wake::Idle
        }
    }

    /// Live (non-removing) row count.
    pub fn live_len(&self) -> usize {
        self.entries.iter().filter(|e| e.index.is_some()).count()
    }

    pub fn get(&self, key: &str, now: Ms) -> Option<(&Entry<T>, Fx)> {
        self.entries
            .iter()
            .find(|e| e.index.is_some() && e.key == key)
            .map(|e| (e, e.fx(now)))
    }

    /// Bounding rect of every row now (rows `w` wide at `x`, `row_h(data)` tall; slot y relative to
    /// the list top at 0).
    pub fn bounds(&self, now: Ms, x: f32, w: f32, row_h: impl Fn(&T) -> f32) -> Rect {
        let mut r = Rect::default();
        for (e, fx) in self.iter(now) {
            let h = row_h(&e.data);
            let grow = (fx.scale - 1.0).max(0.0) * 0.5;
            let rr = Rect::new(x + fx.dx, fx.y, w, h).inflate(w * grow, h * grow);
            r = r.union(&rr);
        }
        r
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn items(keys: &[&'static str]) -> Vec<(&'static str, u32, f32)> {
        keys.iter().enumerate().map(|(i, k)| (*k, i as u32, i as f32 * 42.0)).collect()
    }

    #[test]
    fn first_sync_is_instant() {
        let mut l = KeyedList::new(ListSpec::focus_list());
        assert!(l.sync(items(&["a", "b"]).into_iter(), 0.0, None));
        assert!(!l.is_animating(0.0));
        let v: Vec<_> = l.iter(0.0).map(|(e, fx)| (e.key.clone(), fx.opacity, fx.scale, fx.y)).collect();
        assert_eq!(v, vec![("a".into(), 1.0, 1.0, 0.0), ("b".into(), 1.0, 1.0, 42.0)]);
        assert!(!l.sync(items(&["a", "b"]).into_iter(), 10.0, None), "no change, no damage");
    }

    #[test]
    fn add_plays_transition_and_displaces() {
        let mut l = KeyedList::new(ListSpec::queue());
        l.sync(items(&["a", "b"]).into_iter(), 0.0, None);
        // insert "n" at the front: a, b move down, n animates in
        assert!(l.sync(items(&["n", "a", "b"]).into_iter(), 100.0, None));
        assert!(l.is_animating(100.0));
        let a = l.get("a", 100.0).unwrap().1;
        assert_eq!(a.y, 0.0, "displaced row starts where it was");
        let a = l.get("a", 100.0 + 260.0).unwrap().1;
        assert_eq!(a.y, 42.0);
        let n = l.get("n", 100.0).unwrap().1;
        assert_eq!((n.opacity, n.dx), (0.0, -24.0));
        let n = l.get("n", 100.0 + 400.0).unwrap().1;
        assert_eq!((n.opacity, n.dx), (1.0, 0.0));
        assert!(!l.is_animating(100.0 + 400.0));
        // order follows the model
        let order: Vec<_> = l.iter(600.0).map(|(e, _)| e.key.clone()).collect();
        assert_eq!(order, vec!["n", "a", "b"]);
    }

    #[test]
    fn remove_keeps_row_until_done_and_draws_on_top() {
        let mut l = KeyedList::new(ListSpec::focus_list());
        l.sync(items(&["a", "b", "c"]).into_iter(), 0.0, None);
        l.sync(vec![("a", 0, 0.0), ("c", 2, 42.0)].into_iter(), 100.0, None);
        assert_eq!(l.live_len(), 2);
        let keys: Vec<_> = l
            .iter(150.0)
            .map(|(e, fx)| (e.key.clone(), e.index, fx.opacity < 1.0))
            .collect();
        assert_eq!(keys.len(), 3);
        assert_eq!(keys[2].0, "b");
        assert_eq!(keys[2].1, None);
        assert!(keys[2].2);
        // c slides up into b's slot while b fades
        assert_eq!(l.get("c", 100.0).unwrap().1.y, 84.0);
        assert!(l.is_animating(300.0));
        assert!(!l.prune(300.0));
        assert!(l.prune(100.0 + 321.0));
        assert_eq!(l.iter(500.0).count(), 2);
        assert!(!l.is_animating(500.0));
        // the removed row ends at x=80, opacity 0 (checked just before it is dropped)
    }

    #[test]
    fn dragged_row_is_not_animated_and_others_use_move_displaced() {
        let mut l = KeyedList::new(ListSpec::queue());
        l.sync(items(&["a", "b", "c"]).into_iter(), 0.0, None);
        // a dragged into b's slot: model order b, a, c
        l.sync(vec![("b", 1, 0.0), ("a", 0, 42.0), ("c", 2, 84.0)].into_iter(), 0.0, Some("a"));
        assert_eq!(l.get("a", 0.0).unwrap().1.y, 42.0, "dragged row jumps");
        assert_eq!(l.get("b", 0.0).unwrap().1.y, 42.0);
        assert!(l.get("b", 220.0).unwrap().1.y == 0.0, "220 ms moveDisplaced");
        assert!(l.get("b", 110.0).unwrap().1.y > 0.0);
    }

    #[test]
    fn bounds_cover_rows() {
        let mut l = KeyedList::new(ListSpec::queue());
        l.sync(items(&["a", "b"]).into_iter(), 0.0, None);
        let b = l.bounds(0.0, 10.0, 100.0, |_| 40.0);
        assert_eq!(b, Rect::new(10.0, 0.0, 100.0, 82.0));
    }
}
