//! Single-line text input (ui-design.md §5): caret, selection, Ctrl+Backspace, word jumps,
//! paste request, horizontal scroll. No IME, no undo, no Ctrl+C/X.

use crate::cx::{Cx, CursorShape, Handled, PaintCx, TextMeasure};
use foco_base::{Button, Color, Event, Fill, Key, KeyEvent, Ms, Rect, Surface, TextStyle, Wake};

/// Caret blink half-period.
const BLINK_MS: Ms = 500.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputOutcome {
    None,
    Changed,
    Submit { ctrl: bool },
    Escape,
    /// The owner should push `Msg::RequestPaste`.
    Paste,
}

#[derive(Clone, Debug)]
pub struct TextInput {
    pub text: String,
    /// Byte offsets on char boundaries.
    cursor: usize,
    anchor: usize,
    pub max_chars: usize,
    pub align_right: bool,
    scroll_x: f32,
    style: TextStyle,
    rect: Rect,
    /// Cached advance width of `text`; `None` after a change made without a measurer.
    text_w: Option<f32>,
    blink_t0: Ms,
    was_active: bool,
    last_caret_on: bool,
    pending_damage: bool,
    dragging: bool,
}

impl TextInput {
    pub fn new(style: TextStyle, max_chars: usize) -> TextInput {
        TextInput {
            text: String::new(),
            cursor: 0,
            anchor: 0,
            max_chars,
            align_right: false,
            scroll_x: 0.0,
            style,
            rect: Rect::default(),
            text_w: None,
            blink_t0: 0.0,
            was_active: false,
            last_caret_on: false,
            pending_damage: false,
            dragging: false,
        }
    }

    pub fn style(&self) -> &TextStyle {
        &self.style
    }
    pub fn set_style(&mut self, style: TextStyle) {
        if self.style != style {
            self.style = style;
            self.text_w = None;
            self.pending_damage = true;
        }
    }
    pub fn rect(&self) -> Rect {
        self.rect
    }
    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }
    pub fn cursor(&self) -> usize {
        self.cursor
    }
    /// The selected byte range, if any.
    pub fn selection(&self) -> Option<(usize, usize)> {
        (self.cursor != self.anchor).then(|| (self.cursor.min(self.anchor), self.cursor.max(self.anchor)))
    }
    pub fn selected_text(&self) -> &str {
        match self.selection() {
            Some((a, b)) => &self.text[a..b],
            None => "",
        }
    }

    /// Replaces the text; the cursor goes to the end, no selection.
    pub fn set_text(&mut self, s: &str) {
        if self.text != s {
            self.text.clear();
            self.text.extend(s.chars().take(self.max_chars));
            self.pending_damage = true;
        }
        self.cursor = self.text.len();
        self.anchor = self.cursor;
        self.scroll_x = 0.0;
        self.text_w = None;
    }
    pub fn clear(&mut self) {
        self.set_text("");
    }
    pub fn select_all(&mut self) {
        self.anchor = 0;
        self.cursor = self.text.len();
        self.pending_damage = true;
    }
    /// Drops the selection (focus lost).
    pub fn deselect(&mut self) {
        if self.cursor != self.anchor {
            self.anchor = self.cursor;
            self.pending_damage = true;
        }
    }

    fn char_count(&self) -> usize {
        self.text.chars().count()
    }

    fn prefix_w(&self, m: &mut dyn TextMeasure, upto: usize) -> f32 {
        if upto == 0 {
            0.0
        } else {
            m.measure(&self.text[..upto], &self.style)
        }
    }

    fn total_w(&mut self, m: &mut dyn TextMeasure) -> f32 {
        match self.text_w {
            Some(w) => w,
            None => {
                let w = if self.text.is_empty() { 0.0 } else { m.measure(&self.text, &self.style) };
                self.text_w = Some(w);
                w
            }
        }
    }

    /// X of the text origin in window coordinates.
    fn origin_x(&self) -> f32 {
        let tw = self.text_w.unwrap_or(0.0);
        if self.align_right && tw + 1.0 < self.rect.w {
            self.rect.right() - tw - 1.0
        } else {
            self.rect.x - self.scroll_x
        }
    }

    fn ensure_visible(&mut self, m: &mut dyn TextMeasure) {
        let total = self.total_w(m);
        let view = self.rect.w;
        if total + 1.0 <= view {
            self.scroll_x = 0.0;
            return;
        }
        let caret = self.prefix_w(m, self.cursor);
        if caret - self.scroll_x < 0.0 {
            self.scroll_x = caret;
        } else if caret - self.scroll_x > view - 1.0 {
            self.scroll_x = caret - (view - 1.0);
        }
        self.scroll_x = self.scroll_x.clamp(0.0, (total + 1.0 - view).max(0.0));
    }

    /// Byte index of the char boundary nearest to `x` (relative to the text origin).
    fn index_at(&self, m: &mut dyn TextMeasure, x: f32) -> usize {
        if x <= 0.0 || self.text.is_empty() {
            return 0;
        }
        let mut prev_w = 0.0;
        let mut prev_i = 0;
        for (i, ch) in self.text.char_indices() {
            let end = i + ch.len_utf8();
            let w = self.prefix_w(m, end);
            if w >= x {
                return if x - prev_w <= w - x { prev_i } else { end };
            }
            prev_w = w;
            prev_i = end;
        }
        self.text.len()
    }

    fn prev_boundary(&self, i: usize) -> usize {
        self.text[..i].char_indices().next_back().map_or(0, |(p, _)| p)
    }
    fn next_boundary(&self, i: usize) -> usize {
        self.text[i..].chars().next().map_or(i, |c| i + c.len_utf8())
    }
    /// Start of the word before `i` (skips spaces, then the word).
    fn prev_word(&self, i: usize) -> usize {
        let mut j = i;
        while j > 0 {
            let p = self.prev_boundary(j);
            if self.text[p..j].chars().next().is_some_and(|c| !c.is_whitespace()) {
                break;
            }
            j = p;
        }
        while j > 0 {
            let p = self.prev_boundary(j);
            if self.text[p..j].chars().next().is_some_and(|c| c.is_whitespace()) {
                break;
            }
            j = p;
        }
        j
    }
    /// Start of the next word after `i` (skips the current word, then spaces).
    fn next_word(&self, i: usize) -> usize {
        let mut j = i;
        let len = self.text.len();
        while j < len {
            let n = self.next_boundary(j);
            if self.text[j..n].chars().next().is_some_and(|c| c.is_whitespace()) {
                break;
            }
            j = n;
        }
        while j < len {
            let n = self.next_boundary(j);
            if self.text[j..n].chars().next().is_some_and(|c| !c.is_whitespace()) {
                break;
            }
            j = n;
        }
        j
    }
    /// Run of same-class (space / non-space) chars around `i`.
    fn word_range(&self, i: usize) -> (usize, usize) {
        let ws = |c: char| c.is_whitespace();
        let probe = self.text[i..].chars().next().or_else(|| self.text[..i].chars().next_back());
        let Some(c) = probe else { return (0, 0) };
        let class = ws(c);
        let mut a = i;
        while a > 0 {
            let p = self.prev_boundary(a);
            if self.text[p..a].chars().next().is_some_and(|c| ws(c) != class) {
                break;
            }
            a = p;
        }
        let mut b = i;
        while b < self.text.len() {
            let n = self.next_boundary(b);
            if self.text[b..n].chars().next().is_some_and(|c| ws(c) != class) {
                break;
            }
            b = n;
        }
        (a, b)
    }

    fn delete_selection(&mut self) -> bool {
        if let Some((a, b)) = self.selection() {
            self.text.replace_range(a..b, "");
            self.cursor = a;
            self.anchor = a;
            true
        } else {
            false
        }
    }

    fn insert(&mut self, s: &str) -> bool {
        let sel_chars = self.selected_text().chars().count();
        let room = self.max_chars.saturating_sub(self.char_count() - sel_chars);
        let ins: String = s.chars().filter(|c| !c.is_control()).take(room).collect();
        let had_sel = self.delete_selection();
        if ins.is_empty() {
            return had_sel;
        }
        self.text.insert_str(self.cursor, &ins);
        self.cursor += ins.len();
        self.anchor = self.cursor;
        true
    }

    fn changed(&mut self, cx: &mut Cx) -> InputOutcome {
        self.text_w = None;
        self.moved(cx);
        InputOutcome::Changed
    }

    /// Cursor/selection moved without a text change.
    fn moved(&mut self, cx: &mut Cx) {
        self.ensure_visible(cx.text);
        self.blink_t0 = cx.now;
        self.pending_damage = false;
        cx.damage.add(self.rect);
    }

    /// Handles a key press for the focused input.
    pub fn key(&mut self, cx: &mut Cx, k: &KeyEvent) -> InputOutcome {
        let shift = k.mods.shift;
        let ctrl = k.mods.ctrl;
        match k.key {
            Key::Enter => return InputOutcome::Submit { ctrl },
            Key::Escape => return InputOutcome::Escape,
            Key::Backspace => {
                if self.delete_selection() {
                    return self.changed(cx);
                }
                if self.cursor > 0 {
                    let to = if ctrl { self.prev_word(self.cursor) } else { self.prev_boundary(self.cursor) };
                    self.text.replace_range(to..self.cursor, "");
                    self.cursor = to;
                    self.anchor = to;
                    return self.changed(cx);
                }
            }
            Key::Delete => {
                if self.delete_selection() {
                    return self.changed(cx);
                }
                if self.cursor < self.text.len() {
                    let to = if ctrl { self.next_word(self.cursor) } else { self.next_boundary(self.cursor) };
                    self.text.replace_range(self.cursor..to, "");
                    return self.changed(cx);
                }
            }
            Key::Left | Key::Right | Key::Home | Key::End => {
                let sel = self.selection();
                let to = match k.key {
                    Key::Left => match sel {
                        Some((a, _)) if !shift => a,
                        _ if ctrl => self.prev_word(self.cursor),
                        _ => self.prev_boundary(self.cursor),
                    },
                    Key::Right => match sel {
                        Some((_, b)) if !shift => b,
                        _ if ctrl => self.next_word(self.cursor),
                        _ => self.next_boundary(self.cursor),
                    },
                    Key::Home => 0,
                    _ => self.text.len(),
                };
                let before = (self.cursor, self.anchor);
                self.cursor = to;
                if !shift {
                    self.anchor = to;
                }
                if before != (self.cursor, self.anchor) {
                    self.moved(cx);
                }
            }
            Key::Char('a') if ctrl && !k.mods.alt && !k.mods.meta => {
                self.select_all();
                self.moved(cx);
            }
            Key::Char('v') if ctrl && !k.mods.alt && !k.mods.meta => return InputOutcome::Paste,
            _ => {
                if let Some(t) = &k.text {
                    if self.insert(t) {
                        return self.changed(cx);
                    }
                }
            }
        }
        InputOutcome::None
    }

    /// Inserts clipboard text (newlines become spaces).
    pub fn paste(&mut self, cx: &mut Cx, s: &str) {
        let flat: String = s.chars().map(|c| if c == '\n' || c == '\r' || c == '\t' { ' ' } else { c }).collect();
        if self.insert(&flat) {
            self.changed(cx);
        }
    }

    /// Click = caret, drag = select, double click = word. Position in window coordinates.
    pub fn pointer(&mut self, cx: &mut Cx, ev: &Event) -> Handled {
        match ev {
            Event::PointerMove { pos } => {
                if self.rect.contains(*pos) || self.dragging {
                    cx.cursor = CursorShape::IBeam;
                }
                if self.dragging {
                    self.total_w(cx.text);
                    let i = self.index_at(cx.text, pos.x - self.origin_x());
                    if i != self.cursor {
                        self.cursor = i;
                        self.moved(cx);
                    }
                    return Handled::Yes;
                }
                Handled::No
            }
            Event::PointerDown { pos, button: Button::Left, clicks } if self.rect.contains(*pos) => {
                self.total_w(cx.text);
                let i = self.index_at(cx.text, pos.x - self.origin_x());
                if *clicks >= 2 {
                    let (a, b) = self.word_range(i);
                    self.anchor = a;
                    self.cursor = b;
                } else {
                    self.cursor = i;
                    self.anchor = i;
                }
                self.dragging = *clicks < 2;
                self.moved(cx);
                if *clicks < 2 {
                    Handled::Grab
                } else {
                    Handled::Yes
                }
            }
            Event::PointerUp { button: Button::Left, .. } if self.dragging => {
                self.dragging = false;
                Handled::Yes
            }
            _ => Handled::No,
        }
    }

    /// Places the input; re-measures and damages when the rect changed.
    pub fn layout(&mut self, cx: &mut Cx, rect: Rect) {
        if rect != self.rect {
            cx.damage.add_moved(self.rect, rect);
            self.rect = rect;
            self.ensure_visible(cx.text);
        } else if self.text_w.is_none() {
            self.ensure_visible(cx.text);
        }
    }

    fn caret_on(&self, now: Ms, active: bool) -> bool {
        active && (((now - self.blink_t0) / BLINK_MS).floor() as i64) % 2 == 0
    }

    /// Call from the owner's `update`: flushes pending damage (e.g. after `set_text`) and
    /// damages on caret blink flips and focus changes.
    pub fn tick(&mut self, cx: &mut Cx, active: bool) {
        if active && !self.was_active {
            self.blink_t0 = cx.now;
        }
        let on = self.caret_on(cx.now, active);
        if self.pending_damage || on != self.last_caret_on || active != self.was_active {
            if self.text_w.is_none() {
                self.ensure_visible(cx.text);
            }
            cx.damage.add(self.rect);
            self.pending_damage = false;
        }
        self.last_caret_on = on;
        self.was_active = active;
    }

    /// Caret blink wake (500 ms), only while `active`.
    pub fn next_wake(&self, now: Ms, active: bool) -> Wake {
        if !active {
            return Wake::Idle;
        }
        let n = ((now - self.blink_t0) / BLINK_MS).floor() + 1.0;
        Wake::at(now, self.blink_t0 + n * BLINK_MS)
    }

    /// Paints the text, selection and caret clipped to the rect. Selection and caret only show
    /// while `active`. `sel` is the selection fill, `sel_text` the selected text colour.
    pub fn paint(&self, s: &mut dyn Surface, pc: &PaintCx, active: bool, sel: Color, sel_text: Option<Color>) {
        if self.rect.is_empty() {
            return;
        }
        s.save();
        s.clip_rect(self.rect);
        let lh = s.line_height(&self.style);
        let y = self.rect.y + (self.rect.h - lh) * 0.5;
        let tw = if self.text.is_empty() { 0.0 } else { s.measure(&self.text, &self.style) };
        let ox = if self.align_right && tw + 1.0 < self.rect.w {
            self.rect.right() - tw - 1.0
        } else {
            self.rect.x - self.scroll_x
        };
        let selection = if active { self.selection() } else { None };
        match selection {
            Some((a, b)) => {
                let xa = ox + if a == 0 { 0.0 } else { s.measure(&self.text[..a], &self.style) };
                let xb = ox + s.measure(&self.text[..b], &self.style);
                s.fill_rrect(Rect::new(xa, y, xb - xa, lh), 0.0, &Fill::Solid(sel));
                s.text(&self.text[..a], ox, y, &self.style);
                let mut st = self.style.clone();
                if let Some(c) = sel_text {
                    st.color = c;
                }
                s.text(&self.text[a..b], xa, y, &st);
                s.text(&self.text[b..], xb, y, &self.style);
            }
            None => {
                if !self.text.is_empty() {
                    s.text(&self.text, ox, y, &self.style);
                }
            }
        }
        if self.caret_on(pc.now, active) {
            let cx_ = ox + if self.cursor == 0 { 0.0 } else { s.measure(&self.text[..self.cursor], &self.style) };
            s.fill_rrect(Rect::new(cx_, y, 1.0, lh), 0.0, &Fill::Solid(self.style.color));
        }
        s.restore();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::Harness;
    use foco_base::{theme, Mods, Point, Weight};

    fn input() -> TextInput {
        TextInput::new(TextStyle::new(10.0, Weight::Regular, theme::TEXT), 160)
    }
    fn key(k: Key, mods: Mods, text: Option<&str>) -> KeyEvent {
        KeyEvent { key: k, mods, text: text.map(String::from) }
    }
    fn typ(t: &mut TextInput, h: &mut Harness, s: &str) {
        for c in s.chars() {
            let st = c.to_string();
            h.with_cx(0.0, |cx| t.key(cx, &key(Key::Char(c), Mods::NONE, Some(&st))));
        }
    }
    fn press(t: &mut TextInput, h: &mut Harness, k: Key, mods: Mods) -> InputOutcome {
        h.with_cx(0.0, |cx| t.key(cx, &key(k, mods, None)))
    }
    const SHIFT: Mods = Mods { ctrl: false, shift: true, alt: false, meta: false };

    #[test]
    fn typing_editing_and_words() {
        let mut h = Harness::new();
        let mut t = input();
        h.with_cx(0.0, |cx| t.layout(cx, Rect::new(0.0, 0.0, 200.0, 20.0)));
        typ(&mut t, &mut h, "olá mundo cruel");
        assert_eq!(t.text, "olá mundo cruel");
        press(&mut t, &mut h, Key::Backspace, Mods::NONE);
        assert_eq!(t.text, "olá mundo crue");
        press(&mut t, &mut h, Key::Backspace, Mods::CTRL);
        assert_eq!(t.text, "olá mundo ");
        press(&mut t, &mut h, Key::Backspace, Mods::CTRL);
        assert_eq!(t.text, "olá ");
        press(&mut t, &mut h, Key::Left, Mods::CTRL);
        assert_eq!(t.cursor(), 0);
        press(&mut t, &mut h, Key::Right, Mods::CTRL);
        assert_eq!(t.cursor(), "olá ".len());
        // multi-byte backspace
        press(&mut t, &mut h, Key::Left, Mods::NONE);
        press(&mut t, &mut h, Key::Left, Mods::NONE);
        assert_eq!(t.cursor(), "ol".len());
        press(&mut t, &mut h, Key::Right, Mods::NONE);
        assert_eq!(t.cursor(), "olá".len());
        press(&mut t, &mut h, Key::Backspace, Mods::NONE);
        assert_eq!(t.text, "ol ");
        // select with shift, replace by typing
        press(&mut t, &mut h, Key::Home, SHIFT);
        assert_eq!(t.selected_text(), "ol");
        typ(&mut t, &mut h, "é");
        assert_eq!(t.text, "é ");
        press(&mut t, &mut h, Key::Char('a'), Mods::CTRL);
        assert_eq!(t.selected_text(), "é ");
        press(&mut t, &mut h, Key::Delete, Mods::NONE);
        assert!(t.is_empty());
    }

    #[test]
    fn outcomes_and_limits() {
        let mut h = Harness::new();
        let mut t = input();
        t.max_chars = 5;
        h.with_cx(0.0, |cx| t.layout(cx, Rect::new(0.0, 0.0, 200.0, 20.0)));
        typ(&mut t, &mut h, "abcdefgh");
        assert_eq!(t.text, "abcde");
        assert_eq!(press(&mut t, &mut h, Key::Enter, Mods::NONE), InputOutcome::Submit { ctrl: false });
        assert_eq!(press(&mut t, &mut h, Key::Enter, Mods::CTRL), InputOutcome::Submit { ctrl: true });
        assert_eq!(press(&mut t, &mut h, Key::Escape, Mods::NONE), InputOutcome::Escape);
        assert_eq!(press(&mut t, &mut h, Key::Char('v'), Mods::CTRL), InputOutcome::Paste);
        // paste with a selection counts the replaced text
        t.select_all();
        h.with_cx(0.0, |cx| t.paste(cx, "x\ny z12345"));
        assert_eq!(t.text, "x y z");
        t.set_text("ab");
        h.with_cx(0.0, |cx| t.paste(cx, "\u{7}q"));
        assert_eq!(t.text, "abq");
    }

    #[test]
    fn click_drag_and_double_click() {
        let mut h = Harness::new();
        let mut t = input();
        h.with_cx(0.0, |cx| t.layout(cx, Rect::new(10.0, 0.0, 200.0, 20.0)));
        t.set_text("hello world");
        h.with_cx(0.0, |cx| t.layout(cx, Rect::new(10.0, 0.0, 200.0, 20.0)));
        // 5.5 px per char: click at char 3 boundary (16.5) + 10 origin
        let down = |x: f32, clicks| Event::PointerDown { pos: Point::new(x, 5.0), button: Button::Left, clicks };
        let r = h.with_cx(0.0, |cx| t.pointer(cx, &down(10.0 + 16.5, 1)));
        assert_eq!(r, Handled::Grab);
        assert_eq!(t.cursor(), 3);
        h.with_cx(0.0, |cx| t.pointer(cx, &Event::PointerMove { pos: Point::new(10.0 + 5.5 * 8.0, 5.0) }));
        assert_eq!(t.selected_text(), "lo wo");
        h.with_cx(0.0, |cx| t.pointer(cx, &Event::PointerUp { pos: Point::new(0.0, 0.0), button: Button::Left }));
        h.with_cx(0.0, |cx| t.pointer(cx, &down(10.0 + 5.5 * 8.0, 2)));
        assert_eq!(t.selected_text(), "world");
    }

    #[test]
    fn scrolls_to_keep_caret_visible_and_blinks() {
        let mut h = Harness::new();
        let mut t = input();
        h.with_cx(0.0, |cx| t.layout(cx, Rect::new(0.0, 0.0, 50.0, 20.0)));
        typ(&mut t, &mut h, "abcdefghijklmnopqrstuvwxyz");
        let caret = 26.0 * 5.5;
        assert!((t.scroll_x - (caret - 49.0)).abs() < 0.01, "{}", t.scroll_x);
        press(&mut t, &mut h, Key::Home, Mods::NONE);
        assert_eq!(t.scroll_x, 0.0);
        // blink: on for 500 ms after an edit, then off
        t.blink_t0 = 1000.0;
        assert!(t.caret_on(1100.0, true) && !t.caret_on(1600.0, true) && t.caret_on(2100.0, true));
        assert!(!t.caret_on(1100.0, false));
        assert_eq!(t.next_wake(1100.0, true), Wake::At(1500.0));
        assert_eq!(t.next_wake(1100.0, false), Wake::Idle);
    }

    #[test]
    fn align_right_origin() {
        let mut h = Harness::new();
        let mut t = input();
        t.align_right = true;
        t.set_text("ab");
        h.with_cx(0.0, |cx| t.layout(cx, Rect::new(0.0, 0.0, 100.0, 20.0)));
        assert!((t.origin_x() - (100.0 - 11.0 - 1.0)).abs() < 0.01);
    }
}
