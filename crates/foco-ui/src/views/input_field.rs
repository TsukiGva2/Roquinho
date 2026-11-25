//! `InputField.qml`: the new-demand field. 44 px tall, Enter = queue, Ctrl+Enter = straight to
//! focus, Esc clears first and then collapses, a shake on an empty submit, key hints while
//! focused.

use crate::cx::{Cx, FocusId, Handled, Msg, PaintCx};
use crate::view::{Section, View};
use crate::widgets::{
    paint_chip, paint_icon, paint_text_centered, text_top_for_center, ColorAnim, Icon, InputOutcome, MouseArea,
    TextInput,
};
use crate::cx::CursorShape;
use foco_base::{
    theme, Action, AppCmd, Color, Easing, Event, Fill, Ms, Point, Rect, Seq, Surface, TextStyle, Wake, Weight,
};

/// Field height.
pub const FIELD_H: f32 = 44.0;
const RADIUS: f32 = 14.0;
const MAX_CHARS: usize = 160;
/// `input.anchors.leftMargin` from the plus button's right edge.
const INPUT_X: f32 = 9.0 + 26.0 + 10.0;
/// Hints are shown only above this width (`f.width > 300`).
const HINT_MIN_W: f32 = 300.0;
const SHAKE_PAD: f32 = 6.0;

fn input_style() -> TextStyle {
    TextStyle::new(13.0, Weight::Regular, theme::TEXT)
}
fn chip_style() -> TextStyle {
    TextStyle::new(10.0, Weight::Regular, theme::TEXT2)
}
fn hint_style() -> TextStyle {
    TextStyle::new(11.0, Weight::Regular, theme::TEXT3)
}

/// The two hint pairs (`[["⏎","fila"],["Ctrl ⏎","foco"]]`).
const HINTS: [(&str, &str); 2] = [("⏎", "fila"), ("Ctrl ⏎", "foco")];

/// Visual state that was last painted; the field is damaged only when it changes.
#[derive(Clone, Copy, PartialEq)]
struct Shown {
    bg: Color,
    border: Color,
    plus: Color,
    shake: f32,
    hint: bool,
}

pub struct InputField {
    frame: Rect,
    input: TextInput,
    plus_ma: MouseArea,
    bg: ColorAnim,
    border: ColorAnim,
    plus: ColorAnim,
    shake: Seq,
    active: bool,
    hint_vis: bool,
    /// Per hint: (chip width, label width); measured once.
    hint_m: Option<[(f32, f32); 2]>,
    hint_w: f32,
    lh: f32,
    shown: Shown,
}

impl Default for InputField {
    fn default() -> Self {
        Self::new()
    }
}

impl InputField {
    pub fn new() -> Self {
        let bg = theme::CARD;
        let border = theme::STROKE;
        let plus = theme::FILL;
        InputField {
            frame: Rect::default(),
            input: TextInput::new(input_style(), MAX_CHARS),
            plus_ma: MouseArea::new(),
            bg: ColorAnim::new(bg),
            border: ColorAnim::new(border),
            plus: ColorAnim::new(plus),
            shake: Seq::new(0.0)
                .to(-6.0, 50.0, Easing::Linear)
                .to(6.0, 70.0, Easing::Linear)
                .to(-3.0, 60.0, Easing::Linear)
                .to(0.0, 60.0, Easing::Linear),
            active: false,
            hint_vis: false,
            hint_m: None,
            hint_w: 0.0,
            lh: 0.0,
            shown: Shown { bg, border, plus, shake: 0.0, hint: false },
        }
    }

    /// `input.text.length === 0`.
    pub fn is_empty(&self) -> bool {
        self.input.is_empty()
    }

    /// The typed text (for tests and the quick-add idle check).
    pub fn text(&self) -> &str {
        &self.input.text
    }

    /// `forceFocus()`: the field owns the keyboard.
    pub fn focus(&mut self, cx: &mut Cx) {
        if *cx.focus != FocusId::NewTask {
            *cx.focus = FocusId::NewTask;
        }
    }

    fn shake_x(&self, now: Ms) -> f32 {
        if self.shake.is_running(now) {
            self.shake.value(now)
        } else {
            0.0
        }
    }

    fn plus_rect(&self) -> Rect {
        Rect::new(self.frame.x + 9.0, self.frame.y + (FIELD_H - 26.0) * 0.5, 26.0, 26.0)
    }

    fn measure_hints(&mut self, cx: &mut Cx) {
        if self.hint_m.is_some() {
            return;
        }
        let mut m = [(0.0, 0.0); 2];
        for (i, (k, l)) in HINTS.iter().enumerate() {
            m[i] = (cx.text.measure(k, &chip_style()) + 10.0, cx.text.measure(l, &hint_style()));
        }
        self.hint_m = Some(m);
        // two `chip 4 label` rows separated by the 10 px Row spacing
        self.hint_w = m.iter().map(|(c, l)| c + 4.0 + l).sum::<f32>() + 10.0;
        self.lh = cx.text.line_height(&input_style());
    }

    /// Places the text input between the plus button and the hint (or the right padding).
    fn relayout_input(&mut self, cx: &mut Cx) {
        if self.lh == 0.0 {
            self.lh = cx.text.line_height(&input_style());
        }
        let right = if self.hint_vis {
            self.frame.right() - 12.0 - self.hint_w - 12.0
        } else {
            self.frame.right() - 12.0
        };
        let x = self.frame.x + INPUT_X;
        let r = Rect::new(x, self.frame.y + (FIELD_H - self.lh) * 0.5, (right - x).max(0.0), self.lh);
        self.input.layout(cx, r);
    }

    fn damage_all(&self, cx: &mut Cx) {
        cx.damage.add(self.frame.inflate(SHAKE_PAD + 1.0, 1.0));
    }

    fn submit(&mut self, cx: &mut Cx, to_focus: bool) {
        let t = self.input.text.trim();
        if t.is_empty() {
            self.shake.start(cx.now);
            self.damage_all(cx);
            return;
        }
        cx.msgs.push(Msg::Act(Action::AddTask { title: t.to_string(), to_focus }));
        self.input.clear();
        self.damage_all(cx);
    }

    fn on_key(&mut self, cx: &mut Cx, k: &foco_base::KeyEvent) {
        match self.input.key(cx, k) {
            InputOutcome::Submit { ctrl } => self.submit(cx, ctrl),
            InputOutcome::Escape => {
                if !self.input.is_empty() {
                    self.input.clear();
                    cx.damage.add(self.input.rect());
                } else {
                    // `input.focus = false; f.escapePressed()`
                    *cx.focus = FocusId::None;
                    cx.msgs.push(Msg::Cmd(AppCmd::Collapse));
                }
            }
            InputOutcome::Paste => cx.msgs.push(Msg::RequestPaste),
            InputOutcome::Changed | InputOutcome::None => {}
        }
    }

    fn current(&self, now: Ms) -> Shown {
        Shown {
            bg: self.bg.value(now),
            border: self.border.value(now),
            plus: self.plus.value(now),
            shake: self.shake_x(now),
            hint: self.hint_vis,
        }
    }
}

impl View for InputField {
    fn update(&mut self, cx: &mut Cx) {
        let now = cx.now;
        self.measure_hints(cx);
        let active = cx.has_focus(&FocusId::NewTask);
        self.active = active;
        let hv = active && self.frame.w > HINT_MIN_W;
        if hv != self.hint_vis {
            self.hint_vis = hv;
            self.relayout_input(cx);
        }
        self.bg.animate_to(if active { theme::CARD_HOVER } else { theme::CARD }, 160.0, now);
        self.border.animate_to(if active { cx.env.accent.with_alpha(0.75) } else { theme::STROKE }, 160.0, now);
        let plus_on = active || !self.input.is_empty();
        self.plus.animate_to(if plus_on { cx.env.accent } else { theme::FILL }, 180.0, now);
        self.input.tick(cx, active);
        let cur = self.current(now);
        if cur != self.shown {
            self.shown = cur;
            self.damage_all(cx);
        }
    }

    fn layout(&mut self, cx: &mut Cx, frame: Rect) {
        let f = Rect::new(frame.x, frame.y, frame.w, FIELD_H);
        if f != self.frame {
            cx.damage.add_moved(self.frame.inflate(SHAKE_PAD, 0.0), f.inflate(SHAKE_PAD, 0.0));
            self.frame = f;
            self.plus_ma.rect = self.plus_rect();
        }
        let hv = self.active && f.w > HINT_MIN_W;
        self.hint_vis = hv;
        self.relayout_input(cx);
    }

    fn event(&mut self, cx: &mut Cx, ev: &Event) -> Handled {
        match ev {
            Event::Key(k) => {
                if *cx.focus == FocusId::NewTask {
                    self.on_key(cx, k);
                    Handled::Yes
                } else {
                    Handled::No
                }
            }
            Event::Paste(s) => {
                if *cx.focus == FocusId::NewTask {
                    self.input.paste(cx, s);
                    Handled::Yes
                } else {
                    Handled::No
                }
            }
            Event::PointerMove { .. }
            | Event::PointerLeave
            | Event::PointerDown { .. }
            | Event::PointerUp { .. } => {
                // the whole field moves with the shake
                let sx = self.shake_x(cx.now);
                let ev = shifted(ev, -sx);
                let ev = &ev;
                let mut out = Handled::No;
                // plus button
                let o = self.plus_ma.event(ev);
                if self.plus_ma.hovered() {
                    cx.cursor = CursorShape::Hand;
                }
                if o.clicked {
                    if self.input.text.trim().is_empty() {
                        self.focus(cx);
                    } else {
                        self.submit(cx, false);
                    }
                }
                out = merge(out, o.handled);
                // text input: a press focuses it (`focusOnPress`)
                if let Event::PointerDown { pos, .. } = ev {
                    if self.input.rect().contains(*pos) {
                        self.focus(cx);
                    }
                }
                out = merge(out, self.input.pointer(cx, ev));
                out
            }
            Event::FocusOut => {
                self.input.deselect();
                Handled::No
            }
            _ => Handled::No,
        }
    }

    fn paint(&mut self, s: &mut dyn Surface, pc: &PaintCx) {
        let now = pc.now;
        let active = pc.has_focus(&FocusId::NewTask);
        let sx = self.shake_x(now);
        s.save();
        if sx != 0.0 {
            s.translate(sx, 0.0);
        }
        let r = self.frame;
        s.fill_rrect(r, RADIUS, &Fill::Solid(self.bg.value(now)));
        s.stroke_rrect(r.inflate(-0.5, -0.5), RADIUS - 0.5, &Fill::Solid(self.border.value(now)), 1.0);

        // plus button
        let pr = self.plus_rect();
        s.fill_rrect(pr, 13.0, &Fill::Solid(self.plus.value(now)));
        paint_icon(s, Icon::Plus, pr.center(), 14.0, Color::WHITE, 2.2);

        // text, caret, placeholder
        self.input.paint(s, pc, active, pc.env.accent.with_alpha(0.6), Some(Color::WHITE));
        if self.input.is_empty() {
            let ir = self.input.rect();
            let st = TextStyle::new(13.0, Weight::Regular, theme::TEXT3);
            paint_text_centered(s, "Mais um problema…", ir.x, r.y + FIELD_H * 0.5, &st);
        }

        // key hints
        if active && r.w > HINT_MIN_W {
            if let Some(m) = self.hint_m {
                let cy = r.y + FIELD_H * 0.5;
                let mut x = r.right() - 12.0 - self.hint_w;
                for (i, (k, l)) in HINTS.iter().enumerate() {
                    let (cw, lw) = m[i];
                    paint_chip(s, Rect::new(x, cy - 8.5, cw, 17.0), 5.0, k, &chip_style(), false);
                    x += cw + 4.0;
                    let st = hint_style();
                    let y = text_top_for_center(s, cy, &st);
                    s.text(l, x, y, &st);
                    x += lw + 10.0;
                }
            }
        }
        s.restore();
    }

    fn next_wake(&self, now: Ms) -> Wake {
        let mut w = self.input.next_wake(now, self.active);
        if self.shake.is_running(now)
            || self.bg.is_animating(now)
            || self.border.is_animating(now)
            || self.plus.is_animating(now)
        {
            w = w.merge(Wake::Frame);
        }
        w
    }

    fn bounds(&self) -> Rect {
        self.frame.inflate(SHAKE_PAD, 0.0)
    }
}

impl Section for InputField {
    fn height(&self) -> f32 {
        FIELD_H
    }
}

fn merge(a: Handled, b: Handled) -> Handled {
    match (a, b) {
        (Handled::Grab, _) | (_, Handled::Grab) => Handled::Grab,
        (Handled::Yes, _) | (_, Handled::Yes) => Handled::Yes,
        _ => Handled::No,
    }
}

/// The event with its position moved by `dx` (the field's shake translate).
fn shifted(ev: &Event, dx: f32) -> Event {
    if dx == 0.0 {
        return ev.clone();
    }
    match ev {
        Event::PointerMove { pos } => Event::PointerMove { pos: Point::new(pos.x + dx, pos.y) },
        Event::PointerDown { pos, button, clicks } => {
            Event::PointerDown { pos: Point::new(pos.x + dx, pos.y), button: *button, clicks: *clicks }
        }
        Event::PointerUp { pos, button } => Event::PointerUp { pos: Point::new(pos.x + dx, pos.y), button: *button },
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::Harness;
    use foco_base::{Button, Key, KeyEvent, Mods};

    const FRAME: Rect = Rect { x: 50.0, y: 200.0, w: 484.0, h: 44.0 };

    fn step(h: &mut Harness, v: &mut InputField, now: Ms) {
        h.with_cx(now, |cx| {
            v.update(cx);
            v.layout(cx, FRAME);
        });
    }
    fn focused() -> (Harness, InputField) {
        let mut h = Harness::new();
        let mut v = InputField::new();
        step(&mut h, &mut v, 0.0);
        h.with_cx(0.0, |cx| v.focus(cx));
        step(&mut h, &mut v, 1.0);
        (h, v)
    }
    fn key(k: Key, mods: Mods, text: Option<&str>) -> Event {
        Event::Key(KeyEvent { key: k, mods, text: text.map(String::from) })
    }
    fn typ(h: &mut Harness, v: &mut InputField, now: Ms, s: &str) {
        for c in s.chars() {
            let t = c.to_string();
            h.with_cx(now, |cx| v.event(cx, &key(Key::Char(c), Mods::NONE, Some(&t))));
        }
    }
    fn press(h: &mut Harness, v: &mut InputField, now: Ms, k: Key, mods: Mods) -> Handled {
        h.with_cx(now, |cx| v.event(cx, &key(k, mods, None)))
    }

    #[test]
    fn height_and_focus() {
        let (h, v) = focused();
        assert_eq!(v.height(), 44.0);
        assert_eq!(h.focus, FocusId::NewTask);
        assert!(v.is_empty());
    }

    #[test]
    fn enter_queues_and_ctrl_enter_focuses() {
        let (mut h, mut v) = focused();
        typ(&mut h, &mut v, 2.0, "  comprar pão ");
        assert!(!v.is_empty());
        press(&mut h, &mut v, 3.0, Key::Enter, Mods::NONE);
        assert_eq!(h.take_msgs(), vec![Msg::Act(Action::AddTask { title: "comprar pão".into(), to_focus: false })]);
        assert!(v.is_empty());
        typ(&mut h, &mut v, 4.0, "x");
        press(&mut h, &mut v, 5.0, Key::Enter, Mods::CTRL);
        assert_eq!(h.take_msgs(), vec![Msg::Act(Action::AddTask { title: "x".into(), to_focus: true })]);
    }

    #[test]
    fn dead_key_composed_text_is_inserted() {
        let (mut h, mut v) = focused();
        // the shell resolves ´ + a into "á" and sends it as KeyEvent.text
        h.with_cx(2.0, |cx| v.event(cx, &key(Key::Char('a'), Mods::NONE, Some("á"))));
        h.with_cx(2.0, |cx| v.event(cx, &key(Key::Char('c'), Mods::NONE, Some("ç"))));
        assert_eq!(v.text(), "áç");
    }

    #[test]
    fn max_160_chars() {
        let (mut h, mut v) = focused();
        typ(&mut h, &mut v, 2.0, &"a".repeat(200));
        assert_eq!(v.text().chars().count(), 160);
    }

    #[test]
    fn empty_submit_shakes_for_240ms_then_idles() {
        let (mut h, mut v) = focused();
        typ(&mut h, &mut v, 100.0, "   ");
        h.damage.clear();
        press(&mut h, &mut v, 1000.0, Key::Enter, Mods::NONE);
        assert!(h.take_msgs().is_empty(), "blank text is not submitted");
        assert!(!h.damage.is_empty());
        assert_eq!(v.next_wake(1000.0), Wake::Frame);
        // -6 at 50 ms, +6 at 120, -3 at 180, 0 at 240
        assert!((v.shake_x(1050.0) + 6.0).abs() < 1e-4);
        assert!((v.shake_x(1120.0) - 6.0).abs() < 1e-4);
        assert!((v.shake_x(1180.0) + 3.0).abs() < 1e-4);
        assert_eq!(v.shake_x(1240.0), 0.0);
        step(&mut h, &mut v, 1100.0);
        h.damage.clear();
        step(&mut h, &mut v, 1300.0);
        // caret blink may still be the only wake while focused
        assert!(!matches!(v.next_wake(1300.0), Wake::Frame));
    }

    #[test]
    fn escape_clears_then_collapses() {
        let (mut h, mut v) = focused();
        typ(&mut h, &mut v, 2.0, "abc");
        press(&mut h, &mut v, 3.0, Key::Escape, Mods::NONE);
        assert!(v.is_empty());
        assert!(h.take_msgs().is_empty());
        assert_eq!(h.focus, FocusId::NewTask);
        press(&mut h, &mut v, 4.0, Key::Escape, Mods::NONE);
        assert_eq!(h.take_msgs(), vec![Msg::Cmd(AppCmd::Collapse)]);
        assert_eq!(h.focus, FocusId::None);
    }

    #[test]
    fn ctrl_v_requests_paste_then_paste_event_inserts() {
        let (mut h, mut v) = focused();
        h.with_cx(2.0, |cx| v.event(cx, &key(Key::Char('v'), Mods::CTRL, None)));
        assert_eq!(h.take_msgs(), vec![Msg::RequestPaste]);
        h.with_cx(3.0, |cx| v.event(cx, &Event::Paste("linha1\nlinha2".into())));
        assert_eq!(v.text(), "linha1 linha2");
    }

    #[test]
    fn keys_ignored_without_focus() {
        let mut h = Harness::new();
        let mut v = InputField::new();
        step(&mut h, &mut v, 0.0);
        typ(&mut h, &mut v, 1.0, "abc");
        assert!(v.is_empty());
        assert_eq!(press(&mut h, &mut v, 2.0, Key::Enter, Mods::NONE), Handled::No);
    }

    #[test]
    fn plus_button_focuses_or_submits() {
        let mut h = Harness::new();
        let mut v = InputField::new();
        step(&mut h, &mut v, 0.0);
        let p = Point::new(FRAME.x + 9.0 + 13.0, FRAME.y + 22.0);
        let click = |h: &mut Harness, v: &mut InputField, now: Ms| {
            h.with_cx(now, |cx| {
                v.event(cx, &Event::PointerMove { pos: p });
                v.event(cx, &Event::PointerDown { pos: p, button: Button::Left, clicks: 1 });
                v.event(cx, &Event::PointerUp { pos: p, button: Button::Left });
            })
        };
        click(&mut h, &mut v, 1.0);
        assert_eq!(h.focus, FocusId::NewTask);
        step(&mut h, &mut v, 2.0);
        typ(&mut h, &mut v, 3.0, "oi");
        click(&mut h, &mut v, 4.0);
        assert_eq!(h.take_msgs(), vec![Msg::Act(Action::AddTask { title: "oi".into(), to_focus: false })]);
    }

    #[test]
    fn hints_only_while_focused_and_blink_wake() {
        let mut h = Harness::new();
        let mut v = InputField::new();
        step(&mut h, &mut v, 0.0);
        assert!(!v.hint_vis);
        assert_eq!(v.next_wake(0.0), Wake::Idle);
        h.with_cx(1.0, |cx| v.focus(cx));
        step(&mut h, &mut v, 1.0);
        assert!(v.hint_vis);
        // the text input shrinks to leave room for the hints
        let w_hint = v.input.rect().w;
        assert!(w_hint < 484.0 - 35.0 - 12.0);
        // caret blink: exact 500 ms deadline, no frames
        assert_eq!(v.next_wake(2.0), Wake::Frame, "focus colours fade for 180 ms");
        step(&mut h, &mut v, 400.0);
        let w = v.next_wake(400.0);
        assert!(matches!(w, Wake::At(t) if (t - 501.0).abs() < 1.0), "{w:?}");
        h.window_focused = false;
        step(&mut h, &mut v, 600.0);
        assert!(!v.hint_vis);
        step(&mut h, &mut v, 900.0);
        assert_eq!(v.next_wake(900.0), Wake::Idle);
    }

    fn save(name: &str, s: &foco_base::PixSurface) {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/ui-test/wave3b");
        std::fs::create_dir_all(&dir).unwrap();
        s.save_png(dir.join(name)).unwrap();
    }

    #[test]
    fn snapshots() {
        let (mut h, mut v) = focused();
        let draw = |h: &Harness, v: &mut InputField, now: Ms, name: &str| {
            let mut s = Harness::surface();
            s.fill_rrect(Rect::new(0.0, 0.0, 760.0, 760.0), 0.0, &Fill::Solid(theme::NOTCH));
            h.with_pc(now, Rect::new(0.0, 0.0, 760.0, 760.0), |pc| v.paint(&mut s, pc));
            save(name, &s);
        };
        step(&mut h, &mut v, 200.0);
        draw(&h, &mut v, 200.0, "input-focused-empty.png");
        typ(&mut h, &mut v, 300.0, "Escrever o relatório");
        step(&mut h, &mut v, 300.0);
        step(&mut h, &mut v, 500.0);
        draw(&h, &mut v, 500.0, "input-focused-text.png");
        h.window_focused = false;
        step(&mut h, &mut v, 800.0);
        step(&mut h, &mut v, 1100.0);
        draw(&h, &mut v, 1100.0, "input-unfocused-text.png");
    }
}
