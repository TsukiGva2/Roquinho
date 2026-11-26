//! Shortcut sequence text parsing (port of `parseSequence()` / `keysymFor()` from
//! `platform_x11.cpp`).
//!
//! Accepts Qt portable text (`Meta+Alt+N`, `Super+Alt+N`, `Ctrl+Shift+F1`, `Meta+Alt+,`) and
//! the Openbox `rc.xml` style (`W-A-n`, `W-A-space`, `W-A-period`).

pub const MOD_SHIFT: u16 = 1 << 0;
pub const MOD_CONTROL: u16 = 1 << 2;
pub const MOD_1: u16 = 1 << 3; // Alt
pub const MOD_4: u16 = 1 << 6; // Super / Windows

/// A parsed shortcut: X modifier mask, X keysym and the display parts (`["Meta","Alt","N"]`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Seq {
    pub mods: u16,
    pub sym: u32,
    pub display: Vec<String>,
}

/// Normalises the Openbox style (`W-A-n`) to Qt portable text.
fn openbox_to_qt(text: &str) -> Option<String> {
    if text.contains(char::is_whitespace) {
        return None;
    }
    let mut parts: Vec<&str> = text.split('-').collect();
    if parts.len() < 2 || parts.iter().any(|p| p.is_empty()) {
        return None;
    }
    let key = parts.pop()?;
    let mut out: Vec<String> = Vec::new();
    for m in parts {
        out.push(
            match m {
                "W" => "Meta",
                "A" | "M" => "Alt",
                "C" => "Ctrl",
                "S" => "Shift",
                _ => return None,
            }
            .to_string(),
        );
    }
    out.push(
        match key {
            "space" => "Space",
            "period" => ".",
            "comma" => ",",
            k => k,
        }
        .to_string(),
    );
    Some(out.join("+"))
}

fn ci_replace(text: &str, from: &str, to: &str) -> String {
    let lower = text.to_ascii_lowercase();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while let Some(p) = lower[i..].find(from) {
        out.push_str(&text[i..i + p]);
        out.push_str(to);
        i += p + from.len();
    }
    out.push_str(&text[i..]);
    out
}

/// Parses a sequence; `None` when empty or not understood.
pub fn parse_sequence(text: &str) -> Option<Seq> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    let mut text = openbox_to_qt(text).unwrap_or_else(|| text.to_string());
    text = ci_replace(&text, "super+", "Meta+");
    text = ci_replace(&text, "win+", "Meta+");

    let (mods_part, key_tok) = if let Some(head) = text.strip_suffix("++") {
        (head.to_string(), "+".to_string())
    } else if let Some(p) = text.rfind('+') {
        if p + 1 >= text.len() {
            return None;
        }
        (text[..p].to_string(), text[p + 1..].to_string())
    } else {
        (String::new(), text.clone())
    };

    let (mut meta, mut ctrl, mut alt, mut shift) = (false, false, false, false);
    for m in mods_part.split('+').filter(|s| !s.is_empty()) {
        match m.to_ascii_lowercase().as_str() {
            "meta" => meta = true,
            "ctrl" | "control" => ctrl = true,
            "alt" => alt = true,
            "shift" => shift = true,
            _ => return None,
        }
    }
    let (sym, key_name) = keysym_for_name(&key_tok)?;
    let mut mods = 0;
    let mut display = Vec::new();
    // Qt's QKeySequence::toString order.
    if meta {
        mods |= MOD_4;
        display.push("Meta".to_string());
    }
    if ctrl {
        mods |= MOD_CONTROL;
        display.push("Ctrl".to_string());
    }
    if alt {
        mods |= MOD_1;
        display.push("Alt".to_string());
    }
    if shift {
        mods |= MOD_SHIFT;
        display.push("Shift".to_string());
    }
    display.push(key_name);
    Some(Seq { mods, sym, display })
}

/// Key token (`N`, `Space`, `F12`, `.`, `ç`) to (X keysym, display name).
fn keysym_for_name(tok: &str) -> Option<(u32, String)> {
    let mut chars = tok.chars();
    if let (Some(c), None) = (chars.next(), chars.next()) {
        return keysym_for_char(c);
    }
    let lower = tok.to_ascii_lowercase();
    if let Some(n) = lower.strip_prefix('f').and_then(|n| n.parse::<u32>().ok()) {
        if (1..=35).contains(&n) {
            return Some((0xffbe + (n - 1), format!("F{n}")));
        }
    }
    let (sym, name) = match lower.as_str() {
        "space" => (0x20, "Space"),
        "esc" | "escape" => (0xff1b, "Esc"),
        "tab" => (0xff09, "Tab"),
        "backspace" => (0xff08, "Backspace"),
        "return" => (0xff0d, "Return"),
        "enter" => (0xff8d, "Enter"),
        "ins" | "insert" => (0xff63, "Ins"),
        "del" | "delete" => (0xffff, "Del"),
        "pause" => (0xff13, "Pause"),
        "print" => (0xff61, "Print"),
        "home" => (0xff50, "Home"),
        "end" => (0xff57, "End"),
        "left" => (0xff51, "Left"),
        "up" => (0xff52, "Up"),
        "right" => (0xff53, "Right"),
        "down" => (0xff54, "Down"),
        "pgup" | "pageup" => (0xff55, "PgUp"),
        "pgdown" | "pagedown" => (0xff56, "PgDown"),
        _ => return None,
    };
    Some((sym, name.to_string()))
}

fn keysym_for_char(c: char) -> Option<(u32, String)> {
    let cp = c as u32;
    // Latin-1 letters: the keysym is the lowercase form.
    let is_upper_l1 = (0xc0..=0xde).contains(&cp) && cp != 0xd7;
    let is_lower_l1 = (0xdf..=0xff).contains(&cp) && cp != 0xf7;
    if c.is_ascii_alphabetic() {
        return Some((c.to_ascii_lowercase() as u32, c.to_ascii_uppercase().to_string()));
    }
    if is_upper_l1 {
        return Some((cp + 0x20, c.to_string()));
    }
    if is_lower_l1 {
        let up = if cp == 0xdf || cp == 0xff { c } else { char::from_u32(cp - 0x20)? };
        return Some((cp, up.to_string()));
    }
    if (0x20..=0x7e).contains(&cp) || (0xa0..=0xff).contains(&cp) {
        return Some((cp, c.to_string()));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> Seq {
        parse_sequence(s).unwrap_or_else(|| panic!("{s} should parse"))
    }

    #[test]
    fn qt_style() {
        let s = p("Meta+Alt+N");
        assert_eq!(s.mods, MOD_4 | MOD_1);
        assert_eq!(s.sym, 'n' as u32);
        assert_eq!(s.display, ["Meta", "Alt", "N"]);
        assert_eq!(p("Super+Alt+N"), s);
        assert_eq!(p("meta+alt+n"), s);
        assert_eq!(p("Alt+Meta+n"), s);
    }

    #[test]
    fn punctuation_and_names() {
        assert_eq!(p("Meta+Alt+.").sym, '.' as u32);
        assert_eq!(p("Meta+Alt+,").sym, ',' as u32);
        assert_eq!(p("Meta+Alt++").sym, '+' as u32);
        assert_eq!(p("Meta+Alt+Space").sym, 0x20);
        assert_eq!(p("Meta+Alt+Space").display, ["Meta", "Alt", "Space"]);
        assert_eq!(p("Meta+Alt+F12").sym, 0xffbe + 11);
        assert_eq!(p("Ctrl+Shift+Esc").mods, MOD_CONTROL | MOD_SHIFT);
        assert_eq!(p("Ctrl+ç").sym, 0xe7);
        assert_eq!(p("Ctrl+Ç").sym, 0xe7);
    }

    #[test]
    fn openbox_style() {
        assert_eq!(p("W-A-n"), p("Meta+Alt+N"));
        assert_eq!(p("W-A-space"), p("Meta+Alt+Space"));
        assert_eq!(p("W-A-period"), p("Meta+Alt+."));
        assert_eq!(p("W-A-comma"), p("Meta+Alt+,"));
        assert_eq!(p("C-S-F1").mods, MOD_CONTROL | MOD_SHIFT);
    }

    #[test]
    fn rejects() {
        assert!(parse_sequence("").is_none());
        assert!(parse_sequence("   ").is_none());
        assert!(parse_sequence("Meta+Alt+Banana").is_none());
        assert!(parse_sequence("Foo+N").is_none());
        assert!(parse_sequence("Meta+").is_none());
    }
}
