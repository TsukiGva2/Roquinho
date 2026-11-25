//! xkbcommon keymap + state + Compose (dead keys), driven by the `state` field of X key events.

use std::ffi::OsString;

use foco_base::event::{Key, KeyEvent, Mods};
use xkbcommon::xkb::{self, compose, Keycode, Keysym};

/// X core modifier bits.
const X_SHIFT: u32 = 1 << 0;
const X_CONTROL: u32 = 1 << 2;
const X_MOD1: u32 = 1 << 3;
const X_MOD4: u32 = 1 << 6;

/// RMLVO names, as published by `setxkbcmap` in the root window's `_XKB_RULES_NAMES`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuleNames {
    pub rules: String,
    pub model: String,
    pub layout: String,
    pub variant: String,
    pub options: String,
}

impl Default for RuleNames {
    fn default() -> Self {
        RuleNames {
            rules: "evdev".into(),
            model: "pc105".into(),
            layout: "br".into(),
            variant: String::new(),
            options: String::new(),
        }
    }
}

impl RuleNames {
    /// Parses the property value: five NUL-separated strings rules, model, layout, variant,
    /// options. Empty fields fall back to the defaults (evdev/pc105/br).
    pub fn parse(data: &[u8]) -> RuleNames {
        let mut it = data
            .split(|&b| b == 0)
            .map(|s| String::from_utf8_lossy(s).into_owned());
        let d = RuleNames::default();
        let mut next = |def: &str| match it.next() {
            Some(s) if !s.is_empty() => s,
            _ => def.to_string(),
        };
        let rules = next(&d.rules);
        let model = next(&d.model);
        let layout = next(&d.layout);
        let variant = it.next().unwrap_or_default();
        let options = it.next().unwrap_or_default();
        RuleNames {
            rules,
            model,
            layout,
            variant,
            options,
        }
    }
}

pub struct Keyboard {
    ctx: xkb::Context,
    keymap: xkb::Keymap,
    state: xkb::State,
    compose: Option<compose::State>,
    names: RuleNames,
}

fn locale() -> OsString {
    for var in ["LC_ALL", "LC_CTYPE", "LANG"] {
        if let Some(v) = std::env::var_os(var) {
            if !v.is_empty() {
                return v;
            }
        }
    }
    OsString::from("C")
}

impl Keyboard {
    pub fn new(names: RuleNames) -> Result<Keyboard, String> {
        let ctx = xkb::Context::new(xkb::CONTEXT_NO_FLAGS);
        let keymap = Self::compile(&ctx, &names)?;
        let state = xkb::State::new(&keymap);
        let compose = compose::Table::new_from_locale(&ctx, &locale(), compose::COMPILE_NO_FLAGS)
            .ok()
            .map(|t| compose::State::new(&t, compose::STATE_NO_FLAGS));
        Ok(Keyboard {
            ctx,
            keymap,
            state,
            compose,
            names,
        })
    }

    fn compile(ctx: &xkb::Context, n: &RuleNames) -> Result<xkb::Keymap, String> {
        let opts = if n.options.is_empty() {
            None
        } else {
            Some(n.options.clone())
        };
        xkb::Keymap::new_from_names(
            ctx,
            n.rules.as_str(),
            n.model.as_str(),
            n.layout.as_str(),
            n.variant.as_str(),
            opts,
            xkb::KEYMAP_COMPILE_NO_FLAGS,
        )
        .ok_or_else(|| format!("xkb: could not compile keymap for {n:?}"))
    }

    pub fn names(&self) -> &RuleNames {
        &self.names
    }

    /// Rebuilds the keymap/state (keyboard layout changed). Keeps the old one on failure.
    pub fn rebuild(&mut self, names: RuleNames) {
        if names == self.names {
            return;
        }
        if let Ok(km) = Self::compile(&self.ctx, &names) {
            self.state = xkb::State::new(&km);
            self.keymap = km;
            self.names = names;
            if let Some(c) = self.compose.as_mut() {
                c.reset();
            }
        }
    }

    /// Keycodes whose group-0 level 0 or 1 holds `sym` (what the C++ grab loop matched).
    pub fn keycodes_for(&self, sym: u32) -> Vec<u8> {
        let lo = self.keymap.min_keycode().raw();
        let hi = self.keymap.max_keycode().raw().min(255);
        let mut out = Vec::new();
        for code in lo..=hi {
            let kc = Keycode::new(code);
            let hit = (0..2).any(|lvl| {
                self.keymap
                    .key_get_syms_by_level(kc, 0, lvl)
                    .iter()
                    .any(|s| s.raw() == sym)
            });
            if hit {
                out.push(code as u8);
            }
        }
        out
    }

    /// Translates a key press. `state` is the X event state field. Returns `None` for
    /// modifier keys and for presses swallowed by a Compose sequence in progress.
    pub fn key_press(
        &mut self,
        keycode: u8,
        state: u16,
        fallback: &mut dyn FnMut(u8, bool) -> Option<u32>,
    ) -> Option<KeyEvent> {
        let st = state as u32;
        self.state
            .update_mask(st & 0xff, 0, 0, 0, 0, (st >> 13) & 3);
        let kc = Keycode::new(keycode as u32);
        let mut sym = self.state.key_get_one_sym(kc);
        if sym.raw() == 0 {
            // Keycode our keymap does not know (xdotool/xmodmap remapped it on the server):
            // use the server's core mapping.
            match fallback(keycode, st & X_SHIFT != 0) {
                Some(f) if f != 0 => sym = Keysym::new(f),
                _ => return None,
            }
        }
        if is_modifier(sym.raw()) {
            return None;
        }
        let mods = Mods {
            ctrl: st & X_CONTROL != 0,
            shift: st & X_SHIFT != 0,
            alt: st & X_MOD1 != 0,
            meta: st & X_MOD4 != 0,
        };
        let command = mods.ctrl || mods.alt || mods.meta;

        let mut text: Option<String> = None;
        let mut eff = sym;
        let mut composed = false;
        if !command {
            let mut raw = self.state.key_get_utf8(kc);
            if raw.is_empty() {
                raw = xkb::keysym_to_utf8(sym);
            }
            text = Some(raw);
            if let Some(c) = self.compose.as_mut() {
                use compose::{FeedResult, Status};
                if c.feed(sym) == FeedResult::Accepted {
                    match c.status() {
                        Status::Composing | Status::Cancelled => {
                            if c.status() == Status::Cancelled {
                                c.reset();
                            }
                            return None;
                        }
                        Status::Composed => {
                            text = c.utf8();
                            if let Some(s) = c.keysym() {
                                eff = s;
                            }
                            composed = true;
                            c.reset();
                        }
                        Status::Nothing => {}
                    }
                }
            }
        }
        let key = self.map_key(kc, eff, composed);
        let text = text.filter(|t| !t.is_empty() && !t.chars().any(char::is_control));
        Some(KeyEvent { key, mods, text })
    }

    /// Resets any half-typed dead-key sequence (focus lost).
    pub fn reset_compose(&mut self) {
        if let Some(c) = self.compose.as_mut() {
            c.reset();
        }
    }

    fn map_key(&self, kc: Keycode, sym: Keysym, composed: bool) -> Key {
        let raw = sym.raw();
        match raw {
            0xff0d | 0xff8d => return Key::Enter,
            0xff1b => return Key::Escape,
            0xff08 => return Key::Backspace,
            0xffff | 0xff9f => return Key::Delete,
            0xff51 | 0xff96 => return Key::Left,
            0xff53 | 0xff98 => return Key::Right,
            0xff52 | 0xff97 => return Key::Up,
            0xff54 | 0xff99 => return Key::Down,
            0xff50 | 0xff95 => return Key::Home,
            0xff57 | 0xff9c => return Key::End,
            0xff09 | 0xfe20 => return Key::Tab,
            0xff55 | 0xff9a => return Key::PageUp,
            0xff56 | 0xff9b => return Key::PageDown,
            0x20 | 0xff80 => return Key::Space,
            _ => {}
        }
        // Printable: report the key's unshifted (level 0) character, lowercase, so shortcuts
        // like Ctrl+V match regardless of Shift/CapsLock. Composed results (and keys whose
        // level 0 is not a character) use the effective symbol instead.
        let lvl0 = if composed {
            None
        } else {
            self.keymap
                .key_get_syms_by_level(kc, 0, 0)
                .first()
                .copied()
                .filter(|s| s.key_char().is_some() && !is_dead(s.raw()))
        };
        let pick = lvl0.or(Some(sym).filter(|s| !is_dead(s.raw())));
        if let Some(c) = pick.and_then(|s| s.key_char()) {
            if c == ' ' {
                return Key::Space;
            }
            let mut lc = c.to_lowercase();
            return Key::Char(lc.next().filter(|_| lc.next().is_none()).unwrap_or(c));
        }
        Key::Other(raw)
    }
}

fn is_dead(raw: u32) -> bool {
    (0xfe50..=0xfe93).contains(&raw)
}

fn is_modifier(raw: u32) -> bool {
    matches!(raw,
        0xffe1..=0xffee   // Shift_L .. Hyper_R
        | 0xfe01..=0xfe13 // ISO_Lock .. ISO_Level5_Lock (levels, groups)
        | 0xff7e          // Mode_switch
        | 0xff7f) // Num_Lock
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rule_names_parse() {
        let n = RuleNames::parse(b"evdev\0pc105\0br\0abnt2\0caps:escape\0");
        assert_eq!(n.layout, "br");
        assert_eq!(n.variant, "abnt2");
        assert_eq!(n.options, "caps:escape");
        let d = RuleNames::parse(b"");
        assert_eq!(d, RuleNames::default());
    }

    /// Builds the default (br) keyboard; `None` when the xkb data is not installed.
    fn test_kb() -> Option<Keyboard> {
        std::env::set_var("LANG", "pt_BR.UTF-8");
        std::env::remove_var("LC_ALL");
        std::env::remove_var("LC_CTYPE");
        Keyboard::new(RuleNames::default()).ok()
    }

    fn press(kb: &mut Keyboard, sym: u32, state: u16) -> Option<KeyEvent> {
        // keycode that produces `sym` at level 0/1 of group 0
        let code = *kb
            .keycodes_for(sym)
            .first()
            .unwrap_or_else(|| panic!("no key for {sym:#x}"));
        kb.key_press(code, state, &mut |_, _| None)
    }

    #[test]
    fn dead_keys_compose() {
        let Some(mut kb) = test_kb() else { return };
        let a = press(&mut kb, 'a' as u32, 0).unwrap();
        assert_eq!((a.key, a.text.as_deref()), (Key::Char('a'), Some("a")));
        // ABNT2: dead_acute, dead_tilde, dead_circumflex are keys of their own.
        assert!(
            press(&mut kb, 0xfe51, 0).is_none(),
            "dead key alone is swallowed"
        );
        let e = press(&mut kb, 'a' as u32, 0).unwrap();
        assert_eq!(e.text.as_deref(), Some("\u{e1}"), "acute + a");
        assert!(press(&mut kb, 0xfe53, 0).is_none());
        assert_eq!(
            press(&mut kb, 'a' as u32, 0).unwrap().text.as_deref(),
            Some("\u{e3}")
        );
        assert!(
            press(&mut kb, 0xfe52, X_SHIFT as u16).is_none(),
            "shift+~ is dead_circumflex"
        );
        assert_eq!(
            press(&mut kb, 'e' as u32, 0).unwrap().text.as_deref(),
            Some("\u{ea}")
        );
        // ccedilla is a direct key on ABNT2
        assert_eq!(
            press(&mut kb, 0xe7, 0).unwrap().text.as_deref(),
            Some("\u{e7}")
        );
    }

    #[test]
    fn ctrl_and_shift() {
        let Some(mut kb) = test_kb() else { return };
        let v = press(&mut kb, 'v' as u32, X_CONTROL as u16).unwrap();
        assert!(v.is_ctrl('v') && v.text.is_none());
        let b = press(&mut kb, 'b' as u32, X_SHIFT as u16).unwrap();
        assert_eq!((b.key, b.text.as_deref()), (Key::Char('b'), Some("B")));
        assert!(b.mods.shift);
        let sp = press(&mut kb, 0x20, 0).unwrap();
        assert_eq!((sp.key, sp.text.as_deref()), (Key::Space, Some(" ")));
        assert_eq!(press(&mut kb, 0xff0d, 0).unwrap().key, Key::Enter);
        assert!(
            press(&mut kb, 0xffe1, 0).is_none(),
            "modifiers are not reported"
        );
    }
}
