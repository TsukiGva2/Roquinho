//! Window-local input events in logical pixels. Produced by foco-x11, consumed by views.

use crate::geom::Point;

/// Mouse button.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Button {
    Left,
    Middle,
    Right,
}

/// Modifier state (Lock/NumLock are not reported).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Mods {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub meta: bool,
}

impl Mods {
    pub const NONE: Mods = Mods { ctrl: false, shift: false, alt: false, meta: false };
    pub const CTRL: Mods = Mods { ctrl: true, shift: false, alt: false, meta: false };
    pub fn any(&self) -> bool {
        self.ctrl || self.shift || self.alt || self.meta
    }
}

/// A logical key. Letters/digits/punctuation arrive as `Char` holding the key's unshifted,
/// lowercase character (use `KeyEvent::text` for what to insert); `Ctrl+V` is
/// `Char('v')` with `mods.ctrl`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Key {
    Enter,
    Escape,
    Backspace,
    Delete,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    Tab,
    PageUp,
    PageDown,
    Space,
    Char(char),
    /// Any other key, by raw X keysym.
    Other(u32),
}

/// A key press (the shell sends presses and auto-repeats; releases are not delivered).
#[derive(Clone, Debug, PartialEq)]
pub struct KeyEvent {
    pub key: Key,
    pub mods: Mods,
    /// Composed text to insert (dead keys already resolved); `None` for non-printing keys
    /// and for Ctrl/Alt/Meta combinations.
    pub text: Option<String>,
}

impl KeyEvent {
    /// True for `Ctrl+<c>` (c lowercase) with no other modifiers except shift.
    pub fn is_ctrl(&self, c: char) -> bool {
        self.mods.ctrl && !self.mods.alt && !self.mods.meta && self.key == Key::Char(c)
    }
}

/// An input event.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    PointerMove { pos: Point },
    PointerLeave,
    /// `clicks` is 1 for a single press, 2 for a double-click.
    PointerDown { pos: Point, button: Button, clicks: u8 },
    PointerUp { pos: Point, button: Button },
    /// `dy` in pixels, positive = scroll content down.
    Wheel { pos: Point, dy: f32 },
    Key(KeyEvent),
    FocusIn,
    FocusOut,
    /// Clipboard text (result of a paste request).
    Paste(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ctrl_helper() {
        let k = KeyEvent { key: Key::Char('v'), mods: Mods::CTRL, text: None };
        assert!(k.is_ctrl('v') && !k.is_ctrl('c'));
        assert!(Mods::CTRL.any() && !Mods::NONE.any());
    }
}
