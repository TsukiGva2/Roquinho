//! `DoneSection.qml`: "DESTRUÍDAS HOJE", collapsible, with a restore button per row.
//!
//! Header 26 px (label + green count + chevron rotating 220 ms `OutCubic`), rows 34 px with 2 px
//! spacing (strikethrough title, spent time, restore button shown on hover). Only today's done
//! items are listed. The rows are a plain `Repeater` in the QML: no add/remove transitions.
//!
//! Damage: header on hover/rotation; a row on hover; the whole section when its height changes.
//! Wake: `Frame` only while the chevron rotates (or the restore button animates), else `Idle`.

use crate::cx::{CursorShape, Cx, Handled, Msg, PaintCx};
use crate::model::done_today;
use crate::view::{Section, View};
use crate::widgets::{paint_icon, paint_text_centered, Icon, IconButton, MouseArea};
use foco_base::{
    model::fmt_short, theme, Action, Anim, Easing, Event, Fill, Ms, Point, Rect, Surface, TextStyle, Wake, Weight,
};

const HEADER_H: f32 = 26.0;
const ROW_H: f32 = 34.0;
const SPACING: f32 = 2.0;
const DEFAULT_W: f32 = 512.0;

fn title_style() -> TextStyle {
    TextStyle::new(13.0, Weight::Regular, theme::TEXT2).strike()
}
fn spent_style() -> TextStyle {
    TextStyle::new(11.0, Weight::Regular, theme::TEXT3).tabular()
}

#[derive(Clone, Debug)]
struct Row {
    /// Index in `state.done`.
    index: usize,
    title: String,
    spent: String,
    spent_w: f32,
}

pub struct DoneSection {
    frame: Rect,
    w: f32,
    open: bool,
    count: usize,
    rows: Vec<Row>,
    rev: u64,
    dirty: bool,
    head: MouseArea,
    chevron: Anim,
    shown_chev: f32,
    hover_row: Option<usize>,
    restore: IconButton,
    /// Row the restore button is currently placed on.
    restore_row: Option<usize>,
    /// Restore press in progress (keeps the row hover fixed until release).
    pressed: bool,
    last_h: f32,
}

impl Default for DoneSection {
    fn default() -> Self {
        Self::new()
    }
}

impl DoneSection {
    pub fn new() -> Self {
        DoneSection {
            frame: Rect::default(),
            w: DEFAULT_W,
            open: false,
            count: 0,
            rows: Vec::new(),
            rev: u64::MAX,
            dirty: true,
            head: MouseArea::new(),
            chevron: Anim::new(0.0),
            shown_chev: 0.0,
            hover_row: None,
            restore: IconButton::new(Icon::Restore, 26.0, 13.0),
            restore_row: None,
            pressed: false,
            last_h: 0.0,
        }
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Number of today's completed items.
    pub fn count(&self) -> usize {
        self.count
    }

    fn row_rect(&self, i: usize) -> Rect {
        Rect::new(self.frame.x, self.frame.y + HEADER_H + SPACING + i as f32 * (ROW_H + SPACING), self.w, ROW_H)
    }

    fn compute_height(&self) -> f32 {
        if self.count == 0 {
            0.0
        } else if self.open {
            HEADER_H + self.count as f32 * (ROW_H + SPACING)
        } else {
            HEADER_H
        }
    }

    fn rebuild(&mut self, cx: &mut Cx) {
        self.dirty = false;
        self.rows.clear();
        if !self.open {
            return;
        }
        let ts = title_style();
        let ss = spent_style();
        let st = cx.store.state();
        let n = self.count;
        for (index, t) in st.done.iter().take(n).enumerate() {
            let spent = fmt_short(t.spent_ms);
            let spent_w = cx.text.measure(&spent, &ss);
            // title: left of the spent text (8 px) after the check circle (8 + 18 + 10)
            let max_w = (self.w - 36.0 - 8.0 - spent_w - 36.0).max(0.0);
            let title = cx.text.elide(&t.title, &ts, max_w);
            self.rows.push(Row { index, title, spent, spent_w });
        }
    }

    fn set_hover_row(&mut self, cx: &mut Cx, new: Option<usize>) {
        if new == self.hover_row {
            return;
        }
        if let Some(o) = self.hover_row {
            cx.damage.add(self.row_rect(o));
        }
        if let Some(n) = new {
            cx.damage.add(self.row_rect(n));
        }
        if new.is_none() {
            self.restore.reset(cx);
        }
        self.hover_row = new;
        self.place_restore(cx);
    }

    fn place_restore(&mut self, cx: &mut Cx) {
        match self.hover_row.filter(|&i| i < self.rows.len()) {
            Some(i) => {
                let r = self.row_rect(i);
                self.restore
                    .layout(cx, Rect::new(r.right() - 4.0 - 26.0, r.y + (ROW_H - 26.0) * 0.5, 26.0, 26.0));
                self.restore_row = Some(i);
            }
            None => {
                if self.restore_row.take().is_some() {
                    self.restore.layout(cx, Rect::default());
                }
            }
        }
    }

    fn row_at(&self, pos: Point) -> Option<usize> {
        if !self.open || self.rows.is_empty() {
            return None;
        }
        let top = self.frame.y + HEADER_H + SPACING;
        if pos.x < self.frame.x || pos.x >= self.frame.x + self.w || pos.y < top {
            return None;
        }
        let k = ((pos.y - top) / (ROW_H + SPACING)).floor() as usize;
        (k < self.rows.len() && pos.y - top - k as f32 * (ROW_H + SPACING) < ROW_H).then_some(k)
    }
}

impl View for DoneSection {
    fn update(&mut self, cx: &mut Cx) {
        let n = done_today(cx.store, cx.epoch);
        let old_h = self.compute_height();
        if n != self.count {
            self.count = n;
            self.dirty = true;
            if n == 0 {
                self.open = false;
                self.chevron.set(0.0);
            }
        }
        if cx.rev != self.rev {
            self.rev = cx.rev;
            self.dirty = true;
        }
        if self.dirty {
            self.rebuild(cx);
            if self.hover_row.is_some_and(|i| i >= self.rows.len()) {
                self.hover_row = None;
                self.pressed = false;
            }
            self.place_restore(cx);
            cx.damage.add(self.frame.inflate(0.0, 2.0));
        }
        let h = self.compute_height();
        if h != old_h || h != self.last_h {
            self.last_h = h;
            cx.damage.add(Rect::new(self.frame.x, self.frame.y, self.w, old_h.max(h) + 2.0));
        }
        let c = self.chevron.value(cx.now);
        if (c - self.shown_chev).abs() >= 0.5 {
            self.shown_chev = c;
            cx.damage
                .add(Rect::new(self.frame.x, self.frame.y, self.w.min(200.0), HEADER_H));
        }
        self.restore.tick(cx);
    }

    fn layout(&mut self, cx: &mut Cx, frame: Rect) {
        if frame != self.frame {
            let old = self.frame;
            if (frame.w - self.w).abs() > f32::EPSILON {
                self.w = frame.w;
                self.dirty = true;
                self.rev = u64::MAX;
            }
            self.frame = frame;
            cx.damage.add_moved(old, frame);
        }
        let head = Rect::new(frame.x, frame.y, frame.w, if self.count > 0 { HEADER_H } else { 0.0 });
        self.head.rect = head;
        self.place_restore(cx);
    }

    fn event(&mut self, cx: &mut Cx, ev: &Event) -> Handled {
        if self.count == 0 {
            return Handled::No;
        }
        // restore button press in progress: it owns the pointer until release
        if self.pressed {
            let out = self.restore.pointer(cx, ev);
            if matches!(ev, Event::PointerUp { .. }) {
                self.pressed = false;
                if out.clicked {
                    if let Some(i) = self.restore_row.and_then(|i| self.rows.get(i)) {
                        cx.msgs.push(Msg::Act(Action::RestoreDone(i.index)));
                    }
                }
                if let Event::PointerUp { pos, .. } = ev {
                    let r = self.row_at(*pos);
                    self.set_hover_row(cx, r);
                }
            }
            return Handled::Yes;
        }
        // header: hover colour, click toggles
        let was = self.head.hovered();
        let out = self.head.event(ev);
        if self.head.hovered() {
            cx.cursor = CursorShape::Hand;
        }
        if out.changed && was != self.head.hovered() {
            cx.damage
                .add(Rect::new(self.frame.x, self.frame.y, self.w.min(200.0), HEADER_H));
        }
        if out.clicked {
            self.open = !self.open;
            self.chevron
                .animate_to(if self.open { 180.0 } else { 0.0 }, 220.0, Easing::OutCubic, cx.now);
            self.dirty = true;
            if !self.open {
                self.set_hover_row(cx, None);
            }
            return Handled::Yes;
        }
        if out.handled == Handled::Grab {
            return Handled::Grab;
        }
        match ev {
            Event::PointerLeave => {
                self.set_hover_row(cx, None);
                Handled::No
            }
            Event::PointerMove { pos } => {
                let r = self.row_at(*pos);
                self.set_hover_row(cx, r);
                if self.hover_row.is_some() {
                    self.restore.pointer(cx, ev);
                }
                Handled::No
            }
            Event::PointerDown { pos, .. } => {
                if let Some(r) = self.row_at(*pos) {
                    self.set_hover_row(cx, Some(r));
                    let o = self.restore.pointer(cx, ev);
                    if o.handled == Handled::Grab {
                        self.pressed = true;
                        return Handled::Grab;
                    }
                    return Handled::Yes;
                }
                Handled::No
            }
            _ => Handled::No,
        }
    }

    fn paint(&mut self, s: &mut dyn Surface, pc: &PaintCx) {
        if self.count == 0 || !self.frame.inflate(0.0, 2.0).intersects(&pc.clip) {
            return;
        }
        let now = pc.now;
        let f = self.frame;
        // header
        let hov = self.head.hovered();
        let head_col = if hov { theme::TEXT2 } else { theme::TEXT3 };
        let cy = f.y + HEADER_H * 0.5;
        let label = TextStyle::new(10.0, Weight::DemiBold, head_col).with_spacing(1.0);
        let mut x = f.x + 4.0;
        x += paint_text_centered(s, "DESTRUÍDAS HOJE", x, cy, &label);
        x += 6.0;
        let cnt = self.count.to_string();
        let cst = TextStyle::new(10.0, Weight::DemiBold, theme::GREEN);
        x += paint_text_centered(s, &cnt, x, cy, &cst);
        x += 6.0;
        s.save();
        let c = Point::new(x + 6.0, cy);
        s.translate(c.x, c.y);
        s.rotate_deg(self.chevron.value(now));
        s.translate(-c.x, -c.y);
        paint_icon(s, Icon::ChevronDown, c, 12.0, theme::TEXT3, crate::widgets::DEFAULT_STROKE);
        s.restore();
        if !self.open {
            return;
        }
        let ts = title_style();
        let ss = spent_style();
        for (i, r) in self.rows.iter().enumerate() {
            let rr = self.row_rect(i);
            if !rr.intersects(&pc.clip) {
                continue;
            }
            let hovered = self.hover_row == Some(i);
            if hovered {
                s.fill_rrect(rr, 10.0, &Fill::Solid(theme::HOVER));
            }
            let mid = rr.y + ROW_H * 0.5;
            // check circle
            s.fill_rrect(Rect::new(rr.x + 8.0, mid - 9.0, 18.0, 18.0), 9.0, &Fill::Solid(theme::GREEN.with_alpha(0.2)));
            paint_icon(s, Icon::Check, Point::new(rr.x + 17.0, mid), 12.0, theme::GREEN, 2.2);
            paint_text_centered(s, &r.title, rr.x + 36.0, mid, &ts);
            // spent ends 6 px before the restore button (4 + 26 from the right edge)
            paint_text_centered(s, &r.spent, rr.right() - 36.0 - r.spent_w, mid, &ss);
            if hovered {
                self.restore.paint(s, now);
            }
        }
    }

    fn next_wake(&self, now: Ms) -> Wake {
        if self.chevron.is_animating(now) {
            return Wake::Frame;
        }
        if self.hover_row.is_some() {
            return self.restore.next_wake(now);
        }
        Wake::Idle
    }

    fn bounds(&self) -> Rect {
        Rect::new(self.frame.x, self.frame.y, self.w, self.compute_height()).inflate(0.0, 2.0)
    }
}

impl Section for DoneSection {
    fn height(&self) -> f32 {
        self.compute_height()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::Harness;
    use foco_base::{Button, Task};
    use std::path::PathBuf;

    const FRAME: Rect = Rect::new(18.0, 300.0, 512.0, 26.0);

    fn harness(n_today: usize, old: usize) -> Harness {
        let mut h = Harness::new();
        for i in 0..n_today {
            let mut t = Task::new(format!("d{i}"), &format!("Tarefa concluída número {}", i + 1), 0.0);
            t.done_at = h.epoch as i64 + 1000 - i as i64;
            t.spent_ms = (i as i64 + 1) * 25 * 60_000;
            h.store.state.done.push(t);
        }
        for i in 0..old {
            let mut t = Task::new(format!("o{i}"), "ontem", 0.0);
            t.done_at = 1000;
            t.spent_ms = 1;
            h.store.state.done.push(t);
        }
        // `epoch` is "now" at `now = 0`; keep done_at relative to the real day start
        let now_e = h.epoch;
        for t in h.store.state.done.iter_mut().filter(|t| t.done_at > 1000) {
            t.done_at = now_e as i64;
        }
        h
    }

    fn step(d: &mut DoneSection, h: &mut Harness, now: Ms) {
        h.with_cx(now, |cx| {
            d.update(cx);
            d.layout(cx, Rect::new(FRAME.x, FRAME.y, FRAME.w, d.height()));
        });
    }
    fn mv(x: f32, y: f32) -> Event {
        Event::PointerMove { pos: Point::new(x, y) }
    }
    fn click(d: &mut DoneSection, h: &mut Harness, now: Ms, x: f32, y: f32) {
        let p = Point::new(x, y);
        h.with_cx(now, |cx| d.event(cx, &mv(x, y)));
        h.with_cx(now, |cx| d.event(cx, &Event::PointerDown { pos: p, button: Button::Left, clicks: 1 }));
        h.with_cx(now, |cx| d.event(cx, &Event::PointerUp { pos: p, button: Button::Left }));
    }

    #[test]
    fn only_todays_items_and_heights() {
        let mut h = harness(0, 2);
        let mut d = DoneSection::new();
        step(&mut d, &mut h, 0.0);
        assert_eq!(d.height(), 0.0);
        assert!(!d.shown());
        let mut h = harness(3, 2);
        let mut d = DoneSection::new();
        step(&mut d, &mut h, 0.0);
        assert_eq!(d.count(), 3);
        assert_eq!(d.height(), 26.0, "collapsed: header only");
        click(&mut d, &mut h, 10.0, FRAME.x + 30.0, FRAME.y + 13.0);
        assert!(d.is_open());
        step(&mut d, &mut h, 10.0);
        assert_eq!(d.height(), 26.0 + 3.0 * 36.0);
        assert_eq!(d.rows.len(), 3);
        assert_eq!(d.rows[0].spent, "25m");
        assert_eq!(d.rows[2].spent, "1h 15m");
    }

    #[test]
    fn chevron_rotates_220ms_then_idle() {
        let mut h = harness(2, 0);
        let mut d = DoneSection::new();
        step(&mut d, &mut h, 0.0);
        assert_eq!(d.next_wake(0.0), Wake::Idle);
        click(&mut d, &mut h, 100.0, FRAME.x + 30.0, FRAME.y + 13.0);
        step(&mut d, &mut h, 100.0);
        assert_eq!(d.next_wake(100.0), Wake::Frame);
        assert!(d.chevron.value(210.0) > 90.0 && d.chevron.value(210.0) < 180.0);
        assert_eq!(d.chevron.value(320.0), 180.0);
        step(&mut d, &mut h, 330.0);
        assert_eq!(d.next_wake(330.0), Wake::Idle);
        // closing rotates back
        click(&mut d, &mut h, 1000.0, FRAME.x + 30.0, FRAME.y + 13.0);
        assert!(!d.is_open());
        assert_eq!(d.next_wake(1000.0), Wake::Frame);
        assert_eq!(d.height(), 26.0);
    }

    #[test]
    fn restore_button_emits_done_index() {
        let mut h = harness(3, 0);
        let mut d = DoneSection::new();
        step(&mut d, &mut h, 0.0);
        click(&mut d, &mut h, 10.0, FRAME.x + 30.0, FRAME.y + 13.0);
        step(&mut d, &mut h, 10.0);
        // hover row 1 (y = 26 + 2 + 36 .. +34)
        let ry = FRAME.y + 28.0 + 36.0 + 17.0;
        h.with_cx(20.0, |cx| d.event(cx, &mv(FRAME.x + 100.0, ry)));
        assert_eq!(d.hover_row, Some(1));
        let bx = FRAME.x + 512.0 - 4.0 - 13.0;
        click(&mut d, &mut h, 30.0, bx, ry);
        let a: Vec<_> = h.take_msgs();
        assert_eq!(a, vec![Msg::Act(Action::RestoreDone(1))]);
        // not hovering: clicking where the button would be does nothing
        h.with_cx(400.0, |cx| d.event(cx, &Event::PointerLeave));
        assert_eq!(d.hover_row, None);
        assert_eq!(d.next_wake(400.0), Wake::Idle);
    }

    #[test]
    fn no_damage_when_static() {
        let mut h = harness(2, 0);
        let mut d = DoneSection::new();
        step(&mut d, &mut h, 0.0);
        step(&mut d, &mut h, 16.0);
        h.damage.clear();
        step(&mut d, &mut h, 32.0);
        assert!(h.damage.is_empty());
    }

    #[test]
    fn png_snapshots() {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/ui-test/wave3c");
        std::fs::create_dir_all(&dir).unwrap();
        let mut h = harness(3, 2);
        let mut d = DoneSection::new();
        for (open, name) in [(false, "done-collapsed.png"), (true, "done-expanded.png")] {
            if open {
                click(&mut d, &mut h, 10.0, FRAME.x + 30.0, FRAME.y + 13.0);
                h.with_cx(20.0, |cx| d.event(cx, &mv(FRAME.x + 100.0, FRAME.y + 28.0 + 36.0 + 17.0)));
            }
            step(&mut d, &mut h, 20.0);
            step(&mut d, &mut h, 400.0);
            let mut s = Harness::surface();
            s.fill_rrect(Rect::new(0.0, 280.0, 548.0, 200.0), 24.0, &Fill::Solid(theme::NOTCH));
            h.with_pc(400.0, Rect::new(0.0, 0.0, 760.0, 760.0), |pc| d.paint(&mut s, pc));
            s.save_png(dir.join(name)).unwrap();
        }
    }
}
