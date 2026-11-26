//! `~/.config/foco-maxrc`: a reader/writer compatible with Qt's `QSettings::IniFormat`.
//!
//! Compatibility (verified against Qt 6 output):
//! * keys without a group live in `[General]`, which is written first; other sections follow in
//!   byte order, keys inside a section also in byte order (case-sensitive, `Z` before `a`);
//! * values containing `,` `;` `=` or leading/trailing spaces are double-quoted, `"` and `\` are
//!   backslash-escaped, control characters become `\n \t \xNN`, non-ASCII stays raw UTF-8, a
//!   leading `@` is doubled; keys/sections are percent-escaped (`%20`, `%E9`, `%U20AC`), `/` in
//!   a key is written as `\`;
//! * an unquoted `a,b` is a string *list* to Qt; here [`Rc::get`] returns it joined with `,`
//!   (which is what `platform_x11.cpp` does), so `Meta+Alt+,` and `"Meta+Alt+,"` read the same;
//! * unknown sections/keys survive a read-modify-write; comments are dropped, like QSettings.

use crate::fsutil;
use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const GENERAL: &str = "General";

pub struct Rc {
    path: PathBuf,
    sections: BTreeMap<String, BTreeMap<String, String>>,
    dirty: bool,
}

impl Rc {
    /// Reads `path` (a missing or unreadable file is an empty rc).
    pub fn load(path: impl AsRef<Path>) -> Rc {
        let path = path.as_ref().to_path_buf();
        let text = fs::read(&path)
            .map(|b| String::from_utf8_lossy(&b).into_owned())
            .unwrap_or_default();
        Rc { path, sections: parse(&text), dirty: false }
    }

    /// An rc backed by `path` without reading it.
    pub fn empty(path: impl AsRef<Path>) -> Rc {
        Rc { path: path.as_ref().to_path_buf(), sections: BTreeMap::new(), dirty: false }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn get(&self, section: &str, key: &str) -> Option<&str> {
        self.sections.get(section)?.get(key).map(String::as_str)
    }

    pub fn contains(&self, section: &str, key: &str) -> bool {
        self.get(section, key).is_some()
    }

    /// Sets a value; the rc only becomes dirty if the value actually changed.
    pub fn set(&mut self, section: &str, key: &str, value: &str) {
        let sec = self.sections.entry(section.to_string()).or_default();
        if sec.get(key).map(String::as_str) != Some(value) {
            sec.insert(key.to_string(), value.to_string());
            self.dirty = true;
        }
    }

    /// Removes a key (and its section when it becomes empty). Returns whether it existed.
    pub fn remove(&mut self, section: &str, key: &str) -> bool {
        let Some(sec) = self.sections.get_mut(section) else { return false };
        let had = sec.remove(key).is_some();
        if sec.is_empty() {
            self.sections.remove(section);
        }
        self.dirty |= had;
        had
    }

    /// `[General] screen` (`Backend::m_screenName`); `None` when unset or empty (= primary).
    pub fn screen(&self) -> Option<&str> {
        self.get(GENERAL, "screen").filter(|s| !s.is_empty())
    }

    /// `Backend::setScreenName`: empty string means "primary monitor".
    pub fn set_screen(&mut self, name: &str) {
        self.set(GENERAL, "screen", name);
    }

    /// `[General] accent` as `#rrggbb`; `None` when unset or empty (= default colour).
    pub fn accent(&self) -> Option<&str> {
        self.get(GENERAL, "accent").filter(|s| !s.is_empty())
    }

    /// `Backend::setAccentChoice`: `None` writes an empty value like Qt does.
    pub fn set_accent(&mut self, color: Option<&str>) {
        self.set(GENERAL, "accent", color.unwrap_or(""));
    }

    /// The file contents QSettings would write.
    pub fn render(&self) -> String {
        let mut out = String::new();
        let mut first = true;
        let order = self
            .sections
            .iter()
            .filter(|(n, _)| n.as_str() == GENERAL)
            .chain(self.sections.iter().filter(|(n, _)| n.as_str() != GENERAL));
        for (name, keys) in order {
            if keys.is_empty() {
                continue;
            }
            if !first {
                out.push('\n');
            }
            first = false;
            out.push('[');
            out.push_str(&escape_key(name, true));
            out.push_str("]\n");
            for (k, v) in keys {
                out.push_str(&escape_key(k, false));
                out.push('=');
                out.push_str(&escape_value(v));
                out.push('\n');
            }
        }
        out
    }

    /// Writes the file atomically, only if something changed. `Ok(true)` if it wrote.
    pub fn save(&mut self) -> io::Result<bool> {
        if !self.dirty {
            return Ok(false);
        }
        fsutil::atomic_write(&self.path, self.render().as_bytes())?;
        self.dirty = false;
        Ok(true)
    }
}

// ---------------------------------------------------------------- shortcuts

/// `(id, default sequence)` of the five global shortcuts (music ids are not registered).
pub const SHORTCUT_DEFAULTS: [(&str, &str); 5] = [
    ("quick-add", "Meta+Alt+N"),
    ("toggle-panel", "Meta+Alt+F"),
    ("toggle-hide", "Meta+Alt+H"),
    ("complete", "Meta+Alt+D"),
    ("next-screen", "Meta+Alt+M"),
];

/// `registerShortcuts`: reads `[shortcuts]` for the five ids. A missing id is written back with
/// its default (so people can see what to change) and the file is saved if that changed it.
/// The returned text is the raw value (possibly empty = disabled, possibly Openbox style
/// `W-A-n`); run it through [`normalize_sequence`] before parsing the key.
pub fn shortcut_list(rc: &mut Rc) -> Vec<(String, String)> {
    let mut out = Vec::with_capacity(SHORTCUT_DEFAULTS.len());
    for (id, def) in SHORTCUT_DEFAULTS {
        match rc.get("shortcuts", id) {
            Some(v) => out.push((id.to_string(), v.to_string())),
            None => {
                rc.set("shortcuts", id, def);
                out.push((id.to_string(), def.to_string()));
            }
        }
    }
    let _ = rc.save();
    out
}

/// The text part of `parseSequence`: Openbox `W-A-n` style and `Super+`/`Win+` become the
/// `Meta+Alt+N` portable form.
pub fn normalize_sequence(text: &str) -> String {
    let text = text.trim();
    if text.is_empty() {
        return String::new();
    }
    let mut s = text.to_string();
    if is_openbox_style(text) {
        let mut parts: Vec<&str> = text.split('-').collect();
        let mut key = parts.pop().unwrap_or("").to_string();
        let mut mods: Vec<&str> = parts
            .iter()
            .map(|m| match *m {
                "W" => "Meta",
                "A" => "Alt",
                "C" => "Ctrl",
                "S" => "Shift",
                _ => "Alt",
            })
            .collect();
        match key.as_str() {
            "space" => key = "Space".into(),
            "period" => key = ".".into(),
            "comma" => key = ",".into(),
            _ => {}
        }
        mods.push(&key);
        s = mods.join("+");
    }
    let s = replace_ci(&s, "Super+", "Meta+");
    replace_ci(&s, "Win+", "Meta+")
}

/// `^(?:[WACSM]-)+\S+$`
fn is_openbox_style(t: &str) -> bool {
    let b = t.as_bytes();
    let mut i = 0;
    while i + 1 < b.len() && b"WACSM".contains(&b[i]) && b[i + 1] == b'-' {
        i += 2;
    }
    i >= 2 && i < b.len() && !t[i..].chars().any(char::is_whitespace)
}

fn replace_ci(s: &str, from: &str, to: &str) -> String {
    let (ls, lf) = (s.to_ascii_lowercase(), from.to_ascii_lowercase());
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while let Some(p) = ls[i..].find(&lf) {
        out.push_str(&s[i..i + p]);
        out.push_str(to);
        i += p + from.len();
    }
    out.push_str(&s[i..]);
    out
}

/// `keyLabels`: the chips shown in the UI for a sequence, e.g. `["Meta", "Alt", "N"]`;
/// Space is `Espaço`, an empty sequence is `—`.
pub fn key_labels(seq: &str) -> Vec<String> {
    let seq = normalize_sequence(seq);
    if seq.is_empty() {
        return vec!["—".into()];
    }
    let (mut meta, mut ctrl, mut alt, mut shift) = (false, false, false, false);
    let key: String;
    // the key is the last token; a trailing '+' key ("Ctrl++") is the key itself
    let mut rest = seq.as_str();
    loop {
        let Some(p) = rest.find('+') else {
            key = rest.to_string();
            break;
        };
        let (head, tail) = (&rest[..p], &rest[p + 1..]);
        let m = match head.to_ascii_lowercase().as_str() {
            "meta" => &mut meta,
            "ctrl" | "control" => &mut ctrl,
            "alt" => &mut alt,
            "shift" => &mut shift,
            _ => {
                key = rest.to_string();
                break;
            }
        };
        *m = true;
        if tail.is_empty() {
            key = "+".into();
            break;
        }
        rest = tail;
    }
    let mut out = Vec::new();
    if meta {
        out.push("Meta".to_string());
    }
    if ctrl {
        out.push("Ctrl".to_string());
    }
    if alt {
        out.push("Alt".to_string());
    }
    if shift {
        out.push("Shift".to_string());
    }
    out.push(if key.eq_ignore_ascii_case("space") {
        "Espaço".to_string()
    } else if key.chars().count() == 1 {
        key.to_uppercase()
    } else {
        key
    });
    out
}

// ---------------------------------------------------------------- INI codec

fn hex_digit(c: char) -> bool {
    c.is_ascii_hexdigit()
}

/// `QSettingsPrivate::iniEscapedKey` (sections use the same rule; `/` becomes `\`).
fn escape_key(key: &str, section: bool) -> String {
    let mut out = String::with_capacity(key.len());
    for c in key.chars() {
        if c == '/' && !section {
            out.push('\\');
        } else if c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.') {
            out.push(c);
        } else if (c as u32) <= 0xFF {
            out.push_str(&format!("%{:02X}", c as u32));
        } else {
            for u in c.encode_utf16(&mut [0; 2]) {
                out.push_str(&format!("%U{:04X}", u));
            }
        }
    }
    out
}

fn unescape_key(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut units: Vec<u16> = Vec::new();
    let flush = |units: &mut Vec<u16>, out: &mut String| {
        if !units.is_empty() {
            out.push_str(&String::from_utf16_lossy(units));
            units.clear();
        }
    };
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '%' && i + 1 < chars.len() {
            let (n, skip) = if chars[i + 1] == 'U' { (4, 2) } else { (2, 1) };
            if i + skip + n <= chars.len() && chars[i + skip..i + skip + n].iter().all(|c| hex_digit(*c)) {
                let h: String = chars[i + skip..i + skip + n].iter().collect();
                if let Ok(v) = u16::from_str_radix(&h, 16) {
                    units.push(v);
                    i += skip + n;
                    continue;
                }
            }
        }
        flush(&mut units, &mut out);
        out.push(if c == '\\' { '/' } else { c });
        i += 1;
    }
    flush(&mut units, &mut out);
    out
}

/// `QSettingsPrivate::iniEscapedString` for a plain QString value.
fn escape_value(v: &str) -> String {
    let mut out = String::with_capacity(v.len() + 2);
    let mut quotes = false;
    let mut esc_digit = false;
    if v.starts_with('@') {
        out.push('@');
    }
    let start = out.len();
    for c in v.chars() {
        if matches!(c, ';' | ',' | '=') {
            quotes = true;
        }
        if esc_digit && hex_digit(c) {
            out.push_str(&format!("\\x{:x}", c as u32));
            continue;
        }
        esc_digit = false;
        match c {
            '\0' => {
                out.push_str("\\0");
                esc_digit = true;
            }
            '\u{7}' => out.push_str("\\a"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{b}' => out.push_str("\\v"),
            '"' | '\\' => {
                out.push('\\');
                out.push(c);
            }
            c if (c as u32) <= 0x1F => {
                out.push_str(&format!("\\x{:x}", c as u32));
                esc_digit = true;
            }
            c => out.push(c),
        }
    }
    let body = &out[start..];
    if body.starts_with(' ') || body.ends_with(' ') {
        quotes = true;
    }
    if quotes {
        out.insert(start, '"');
        out.push('"');
    }
    out
}

/// Value text to string; unquoted commas are kept (the joined list), with the whitespace
/// around them dropped like Qt does when splitting the list.
fn unescape_value(raw: &str) -> String {
    let raw = raw.trim();
    let chars: Vec<char> = raw.chars().collect();
    let mut out = String::with_capacity(raw.len());
    let mut protected = 0usize; // bytes of `out` that came from quotes/escapes (never trimmed)
    let mut in_quotes = false;
    let mut skip_ws = false;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        i += 1;
        if skip_ws && !in_quotes && c.is_whitespace() {
            continue;
        }
        skip_ws = false;
        match c {
            '"' => {
                in_quotes = !in_quotes;
                protected = out.len();
            }
            ',' if !in_quotes => {
                while out.len() > protected && out.ends_with(char::is_whitespace) {
                    out.pop();
                }
                out.push(',');
                protected = out.len();
                skip_ws = true;
            }
            '\\' if i < chars.len() => {
                let e = chars[i];
                i += 1;
                match e {
                    'a' => out.push('\u{7}'),
                    'b' => out.push('\u{8}'),
                    'f' => out.push('\u{c}'),
                    'n' => out.push('\n'),
                    'r' => out.push('\r'),
                    't' => out.push('\t'),
                    'v' => out.push('\u{b}'),
                    'x' => {
                        let mut v: u32 = 0;
                        let mut n = 0;
                        while i < chars.len() && n < 4 && hex_digit(chars[i]) {
                            v = v * 16 + chars[i].to_digit(16).unwrap_or(0);
                            i += 1;
                            n += 1;
                        }
                        out.push(char::from_u32(v).unwrap_or('\u{fffd}'));
                    }
                    '0'..='7' => {
                        let mut v = e.to_digit(8).unwrap_or(0);
                        let mut n = 1;
                        while i < chars.len() && n < 3 && chars[i].is_digit(8) {
                            v = v * 8 + chars[i].to_digit(8).unwrap_or(0);
                            i += 1;
                            n += 1;
                        }
                        out.push(char::from_u32(v).unwrap_or('\u{fffd}'));
                    }
                    other => out.push(other),
                }
                protected = out.len();
            }
            c => out.push(c),
        }
    }
    while out.len() > protected && out.ends_with(char::is_whitespace) {
        out.pop();
    }
    if let Some(rest) = out.strip_prefix("@@") {
        return format!("@{rest}");
    }
    out
}

fn parse(text: &str) -> BTreeMap<String, BTreeMap<String, String>> {
    let mut sections: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
    let mut cur = GENERAL.to_string();
    for line in text.trim_start_matches('\u{feff}').split('\n') {
        let line = line.trim();
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }
        if let Some(inner) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            let name = unescape_key(inner);
            cur = if name.is_empty() || name == "%General" { GENERAL.to_string() } else { name };
            continue;
        }
        let Some((k, v)) = line.split_once('=') else { continue };
        let key = unescape_key(k.trim());
        if key.is_empty() {
            continue;
        }
        sections.entry(cur.clone()).or_default().insert(key, unescape_value(v));
    }
    sections
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fsutil::testutil::TempDir;

    const REAL: &str = "[General]\nscreen=HDMI-2\n\n[shortcuts]\ncomplete=Meta+Alt+D\nmusic-next=Meta+Alt+.\nmusic-prev=\"Meta+Alt+,\"\nmusic-toggle=Meta+Alt+Space\nnext-screen=Meta+Alt+M\nquick-add=Meta+Alt+N\ntoggle-hide=Meta+Alt+H\ntoggle-panel=Meta+Alt+F\n";

    fn rc_from(text: &str) -> Rc {
        Rc { path: PathBuf::new(), sections: parse(text), dirty: false }
    }

    #[test]
    fn real_file_roundtrips_byte_identical() {
        let rc = rc_from(REAL);
        assert_eq!(rc.screen(), Some("HDMI-2"));
        assert_eq!(rc.get("shortcuts", "music-prev"), Some("Meta+Alt+,"));
        assert_eq!(rc.get("shortcuts", "music-next"), Some("Meta+Alt+."));
        assert_eq!(rc.render(), REAL);
    }

    #[test]
    fn real_file_on_this_machine_if_present() {
        // read-only compatibility check against the user's actual file
        let p = crate::paths::rc_path();
        if let Ok(text) = fs::read_to_string(&p) {
            let rc = rc_from(&text);
            // QSettings itself wrote it, so rendering must reproduce it exactly
            assert_eq!(rc.render(), text);
        }
    }

    #[test]
    fn unquoted_comma_reads_like_the_joined_list() {
        let rc = rc_from("[shortcuts]\na=Meta+Alt+,\nb=\"Meta+Alt+,\"\nc=x, y ,z\n");
        assert_eq!(rc.get("shortcuts", "a"), Some("Meta+Alt+,"));
        assert_eq!(rc.get("shortcuts", "b"), Some("Meta+Alt+,"));
        assert_eq!(rc.get("shortcuts", "c"), Some("x,y,z"));
    }

    /// Output captured from a real Qt 6 `QSettings` (see the C++ probe used while writing this).
    const QT_OUT: &str = "[General]\nBeta=b\nZeta=z\naccent=\nalpha=a\nscreen=HDMI-2\n\n[Alpha]\nx=1\n\n[Main]\nx=1\n\n[shortcuts]\nKey%20With%20Space\\%E9=1\nk1=\"Meta+Alt+,\"\nk10=ctl\\x1\\x31\nk11=a b\nk12=\nk13=[brackets]\nk14=\"; semi\"\nk2=has \\\"quote\\\" and \\\\ back\nk3=\" lead\"\nk4=\"trail \"\nk5=\"a;b=c\"\nk6=tab\\there\\nnl\nk7=ação ç é\nk8=@@foo\nk9=x#y\n\n[zed]\nx=1\n";

    #[test]
    fn matches_qsettings_output() {
        let mut rc = Rc::empty("/nonexistent/x");
        rc.set("General", "screen", "HDMI-2");
        rc.set_accent(None);
        rc.set("General", "Zeta", "z");
        rc.set("General", "alpha", "a");
        rc.set("General", "Beta", "b");
        for (k, v) in [
            ("k1", "Meta+Alt+,"),
            ("k2", "has \"quote\" and \\ back"),
            ("k3", " lead"),
            ("k4", "trail "),
            ("k5", "a;b=c"),
            ("k6", "tab\there\nnl"),
            ("k7", "ação ç é"),
            ("k8", "@foo"),
            ("k9", "x#y"),
            ("k10", "ctl\u{1}1"),
            ("k11", "a b"),
            ("Key With Space/é", "1"),
            ("k12", ""),
            ("k13", "[brackets]"),
            ("k14", "; semi"),
        ] {
            rc.set("shortcuts", k, v);
        }
        rc.set("Alpha", "x", "1");
        rc.set("zed", "x", "1");
        rc.set("Main", "x", "1");
        assert_eq!(rc.render(), QT_OUT);
        // and everything reads back to what was written (key with '/' reads back as '/')
        let back = rc_from(QT_OUT);
        for (sec, keys) in &rc.sections {
            for (k, v) in keys {
                assert_eq!(back.get(sec, k), Some(v.as_str()), "{sec}/{k}");
            }
        }
    }

    #[test]
    fn read_matches_qsettings_probe() {
        let rc = rc_from("[General]\nscreen=HDMI-2\nbs=a\\\\b\\x41\\n\nsp=  padded  \nDup=1\ndup=2\n; comment\ne=\n");
        assert_eq!(rc.get("General", "bs"), Some("a\\bA\n"));
        assert_eq!(rc.get("General", "sp"), Some("padded"));
        assert_eq!(rc.get("General", "Dup"), Some("1"));
        assert_eq!(rc.get("General", "dup"), Some("2"));
        assert_eq!(rc.get("General", "e"), Some(""));
        assert_eq!(rc.get("General", "nope"), None);
    }

    #[test]
    fn value_escape_roundtrip() {
        for v in
            ["", "a", "a,b", " x", "x ", "q\"q", "b\\s", "\n\t\r", "@x", "@@x", "é;ç=", "a\u{1}b", "\u{1}1f", "日本"]
        {
            let esc = escape_value(v);
            assert_eq!(unescape_value(&esc), v, "{v:?} -> {esc}");
        }
    }

    #[test]
    fn modify_preserves_unknown_and_saves_only_when_dirty() {
        let dir = TempDir::new("rc");
        let p = dir.path().join("sub/foco-maxrc");
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(&p, REAL.replace("[General]\n", "[General]\nweird key=1\n")).unwrap();
        let mut rc = Rc::load(&p);
        assert!(!rc.save().unwrap());
        rc.set_screen("HDMI-2"); // unchanged
        assert!(!rc.is_dirty());
        rc.set_screen("HDMI-1");
        rc.set_accent(Some("#315bef"));
        assert!(rc.save().unwrap());
        let text = fs::read_to_string(&p).unwrap();
        assert!(text.starts_with("[General]\naccent=#315bef\nscreen=HDMI-1\nweird%20key=1\n\n[shortcuts]\n"), "{text}");
        assert!(text.contains("music-prev=\"Meta+Alt+,\"\n"));
        assert!(text.contains("music-toggle=Meta+Alt+Space\n"));
        let again = Rc::load(&p);
        assert_eq!(again.get("General", "weird key"), Some("1"));
        assert_eq!(again.accent(), Some("#315bef"));
        // no stray temp files
        let names: Vec<_> = fs::read_dir(p.parent().unwrap())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names.len(), 1, "{names:?}");
    }

    #[test]
    fn remove_and_empty_section() {
        let mut rc = rc_from("[a]\nx=1\n");
        assert!(!rc.remove("a", "y"));
        assert!(rc.remove("a", "x"));
        assert!(rc.is_dirty());
        assert_eq!(rc.render(), "");
        assert!(!rc.remove("a", "x"));
    }

    #[test]
    fn shortcut_list_writes_missing_defaults_but_not_music() {
        let dir = TempDir::new("rcs");
        let p = dir.path().join("foco-maxrc");
        fs::write(
            &p,
            "[General]\nscreen=HDMI-2\n\n[shortcuts]\nquick-add=W-A-g\ncomplete=\nmusic-prev=\"Meta+Alt+,\"\n",
        )
        .unwrap();
        let mut rc = Rc::load(&p);
        let list = shortcut_list(&mut rc);
        assert_eq!(
            list,
            vec![
                ("quick-add".to_string(), "W-A-g".to_string()),
                ("toggle-panel".to_string(), "Meta+Alt+F".to_string()),
                ("toggle-hide".to_string(), "Meta+Alt+H".to_string()),
                ("complete".to_string(), String::new()),
                ("next-screen".to_string(), "Meta+Alt+M".to_string()),
            ]
        );
        let text = fs::read_to_string(&p).unwrap();
        assert!(text.contains("toggle-panel=Meta+Alt+F\n") && text.contains("next-screen=Meta+Alt+M\n"));
        assert!(text.contains("music-prev=\"Meta+Alt+,\"\n"));
        assert!(!text.contains("music-toggle") && !text.contains("music-next"));
        // second call changes nothing
        let before = fs::read(&p).unwrap();
        let mut rc2 = Rc::load(&p);
        assert_eq!(shortcut_list(&mut rc2), list);
        assert_eq!(fs::read(&p).unwrap(), before);
        // fresh install gets all five defaults
        let p2 = dir.path().join("new/foco-maxrc");
        let mut rc3 = Rc::load(&p2);
        assert_eq!(shortcut_list(&mut rc3).len(), 5);
        assert_eq!(
            fs::read_to_string(&p2).unwrap(),
            "[shortcuts]\ncomplete=Meta+Alt+D\nnext-screen=Meta+Alt+M\nquick-add=Meta+Alt+N\ntoggle-hide=Meta+Alt+H\ntoggle-panel=Meta+Alt+F\n"
        );
    }

    #[test]
    fn sequence_normalisation_and_labels() {
        assert_eq!(normalize_sequence("W-A-n"), "Meta+Alt+n");
        assert_eq!(normalize_sequence("W-A-space"), "Meta+Alt+Space");
        assert_eq!(normalize_sequence("C-S-period"), "Ctrl+Shift+.");
        assert_eq!(normalize_sequence("Super+Alt+N"), "Meta+Alt+N");
        assert_eq!(normalize_sequence("win+x"), "Meta+x");
        assert_eq!(normalize_sequence("  Meta+Alt+N "), "Meta+Alt+N");
        assert_eq!(normalize_sequence(""), "");
        assert_eq!(normalize_sequence("M-x"), "Alt+x");
        assert_eq!(key_labels("Meta+Alt+N"), ["Meta", "Alt", "N"]);
        assert_eq!(key_labels("W-A-space"), ["Meta", "Alt", "Espaço"]);
        assert_eq!(key_labels("Alt+Ctrl+Shift+Meta+F5"), ["Meta", "Ctrl", "Alt", "Shift", "F5"]);
        assert_eq!(key_labels("Meta+Alt+,"), ["Meta", "Alt", ","]);
        assert_eq!(key_labels(""), ["—"]);
    }
}
