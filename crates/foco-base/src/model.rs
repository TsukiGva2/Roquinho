//! Persisted data model, byte-compatible with the QML app's `state.json`
//! (`Store.qml` `save()`/`load()`/`copyTask()`), plus small pure helpers.
//!
//! Loading is lenient like `copyTask` (`Number(x) || 0`, `!!x`, `String(x)`); unknown keys are
//! preserved (`extra`) and written back. All times are epoch milliseconds.

use crate::time::Ms;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Map, Value};

fn num(v: &Value) -> i64 {
    match v {
        Value::Number(n) => n
            .as_i64()
            .or_else(|| n.as_f64().filter(|f| f.is_finite()).map(|f| f as i64))
            .unwrap_or(0),
        Value::String(s) => s.trim().parse::<f64>().ok().filter(|f| f.is_finite()).map_or(0, |f| f as i64),
        Value::Bool(b) => *b as i64,
        _ => 0,
    }
}

fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0 && !f.is_nan()),
        Value::String(s) => !s.is_empty(),
        _ => true,
    }
}

fn text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn l_i64<'de, D: Deserializer<'de>>(d: D) -> Result<i64, D::Error> {
    Ok(num(&Value::deserialize(d)?))
}
fn l_u32<'de, D: Deserializer<'de>>(d: D) -> Result<u32, D::Error> {
    Ok(num(&Value::deserialize(d)?).clamp(0, u32::MAX as i64) as u32)
}
fn l_bool<'de, D: Deserializer<'de>>(d: D) -> Result<bool, D::Error> {
    Ok(truthy(&Value::deserialize(d)?))
}
fn l_string<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    Ok(text(&Value::deserialize(d)?))
}
fn d_true() -> bool {
    true
}
fn d_one() -> u32 {
    1
}
fn d_cat() -> String {
    "cat".into()
}
fn d_name() -> String {
    "Foquinho".into()
}

/// One demand (task). JSON keys match what `Store.qml` writes (`tid`, not `id`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Task {
    #[serde(default, deserialize_with = "l_string")]
    pub tid: String,
    #[serde(default, deserialize_with = "l_string")]
    pub title: String,
    /// Accumulated focus time while paused, ms.
    #[serde(rename = "accMs", default, deserialize_with = "l_i64")]
    pub acc_ms: i64,
    /// Epoch ms when the current run began (0 when not running).
    #[serde(rename = "startedAt", default, deserialize_with = "l_i64")]
    pub started_at: i64,
    #[serde(default, deserialize_with = "l_bool")]
    pub running: bool,
    #[serde(rename = "createdAt", default, deserialize_with = "l_i64")]
    pub created_at: i64,
    /// Epoch ms when completed (0 = not done).
    #[serde(rename = "doneAt", default, deserialize_with = "l_i64")]
    pub done_at: i64,
    /// Time spent, set on completion.
    #[serde(rename = "spentMs", default, deserialize_with = "l_i64")]
    pub spent_ms: i64,
}

impl Task {
    /// `makeTask(title)`: paused, zero time. `uid` comes from [`make_uid`].
    pub fn new(uid: String, title: &str, now_epoch_ms: f64) -> Task {
        Task { tid: uid, title: title.to_string(), created_at: now_epoch_ms as i64, ..Task::default() }
    }
    /// `elapsedOf(t)`: accumulated plus the running segment.
    pub fn elapsed(&self, now_epoch_ms: f64) -> i64 {
        self.acc_ms + if self.running { (now_epoch_ms as i64 - self.started_at).max(0) } else { 0 }
    }
    /// `pauseObj(t)`: folds a running segment into `acc_ms`.
    pub fn pause(&mut self, now_epoch_ms: f64) {
        if self.running {
            self.acc_ms += (now_epoch_ms as i64 - self.started_at).max(0);
            self.running = false;
            self.started_at = 0;
        }
    }
    /// Starts running at `now`.
    pub fn resume(&mut self, now_epoch_ms: f64) {
        self.running = true;
        self.started_at = now_epoch_ms as i64;
    }
}

/// Persisted settings. Unknown keys round-trip through `extra`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    #[serde(rename = "petKind", default = "d_cat", deserialize_with = "l_string")]
    pub pet_kind: String,
    #[serde(rename = "petName", default = "d_name", deserialize_with = "l_string")]
    pub pet_name: String,
    #[serde(rename = "autoAdvance", default = "d_true", deserialize_with = "l_bool")]
    pub auto_advance: bool,
    #[serde(default, deserialize_with = "l_bool")]
    pub hidden: bool,
    #[serde(rename = "tipsEnabled", default = "d_true", deserialize_with = "l_bool")]
    pub tips_enabled: bool,
    /// `Store.dayKey()` of the last daily tip shown.
    #[serde(rename = "lastTipDay", default, deserialize_with = "l_string")]
    pub last_tip_day: String,
    /// Music is removed in this port; kept so the field round-trips untouched.
    #[serde(rename = "musicEnabled", default = "d_true", deserialize_with = "l_bool")]
    pub music_enabled: bool,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            pet_kind: d_cat(),
            pet_name: d_name(),
            auto_advance: true,
            hidden: false,
            tips_enabled: true,
            last_tip_day: String::new(),
            music_enabled: true,
            extra: Map::new(),
        }
    }
}

impl Settings {
    /// `PetKind` parsed from `pet_kind` (unknown names fall back to cat).
    pub fn kind(&self) -> PetKind {
        PetKind::parse(&self.pet_kind)
    }
}

/// The whole `state.json`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StateFile {
    #[serde(default = "d_one", deserialize_with = "l_u32")]
    pub version: u32,
    /// Epoch ms of the last save.
    #[serde(rename = "lastSeen", default, deserialize_with = "l_i64")]
    pub last_seen: i64,
    #[serde(default)]
    pub settings: Settings,
    #[serde(rename = "totalDone", default, deserialize_with = "l_u32")]
    pub total_done: u32,
    /// "EM FOCO" tasks.
    #[serde(default)]
    pub focus: Vec<Task>,
    /// "A SEGUIR" queue.
    #[serde(default)]
    pub queue: Vec<Task>,
    /// Completed, newest first, capped at [`MAX_DONE`].
    #[serde(default)]
    pub done: Vec<Task>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Default for StateFile {
    fn default() -> Self {
        StateFile {
            version: 1,
            last_seen: 0,
            settings: Settings::default(),
            total_done: 0,
            focus: Vec::new(),
            queue: Vec::new(),
            done: Vec::new(),
            extra: Map::new(),
        }
    }
}

/// Completed list cap (`while (doneModel.count > 300)`).
pub const MAX_DONE: usize = 300;
/// Quick-add / rename title limit of the input field.
pub const MAX_TITLE_LEN: usize = 160;
/// Duration of the "happy" mood after completing (`celebrateUntil = now + 3200`).
pub const CELEBRATE_MS: f64 = 3200.0;
/// Time after which a running task makes the pet "tired".
pub const TIRED_MS: i64 = 50 * 60_000;

impl StateFile {
    /// Parses leniently; `Err` only for invalid JSON.
    ///
    /// # Errors
    /// Returns the `serde_json` error when `s` is not valid JSON.
    pub fn from_json(s: &str) -> Result<StateFile, serde_json::Error> {
        serde_json::from_str(s)
    }
    /// Serialises like `JSON.stringify(obj, null, 1)` (1-space indent).
    pub fn to_json(&self) -> String {
        let mut buf = Vec::new();
        let fmt = serde_json::ser::PrettyFormatter::with_indent(b" ");
        let mut ser = serde_json::Serializer::with_formatter(&mut buf, fmt);
        if self.serialize(&mut ser).is_err() {
            return String::new();
        }
        String::from_utf8(buf).unwrap_or_default()
    }
    /// `level = 1 + floor(totalDone / 5)`.
    pub fn level(&self) -> u32 {
        level_of(self.total_done)
    }
    /// XP fraction within the level, 0..1 (`(totalDone % 5) / 5`).
    pub fn xp(&self) -> f32 {
        xp_of(self.total_done)
    }
    /// Number of running focus tasks.
    pub fn running_count(&self) -> usize {
        self.focus.iter().filter(|t| t.running).count()
    }
}

pub fn level_of(total_done: u32) -> u32 {
    1 + total_done / 5
}
pub fn xp_of(total_done: u32) -> f32 {
    (total_done % 5) as f32 / 5.0
}

/// Mascot kind (`settings.petKind`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum PetKind {
    #[default]
    Cat,
    Ghost,
    Slime,
}

impl PetKind {
    pub const ALL: [PetKind; 3] = [PetKind::Cat, PetKind::Ghost, PetKind::Slime];
    pub fn parse(s: &str) -> PetKind {
        match s {
            "ghost" => PetKind::Ghost,
            "slime" => PetKind::Slime,
            _ => PetKind::Cat,
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            PetKind::Cat => "cat",
            PetKind::Ghost => "ghost",
            PetKind::Slime => "slime",
        }
    }
}

/// Mascot mood (`Store.petMood`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum Mood {
    #[default]
    Sleep,
    Idle,
    Focus,
    Tired,
    Happy,
}

impl Mood {
    pub fn as_str(self) -> &'static str {
        match self {
            Mood::Sleep => "sleep",
            Mood::Idle => "idle",
            Mood::Focus => "focus",
            Mood::Tired => "tired",
            Mood::Happy => "happy",
        }
    }
    /// `Store.petMood`: celebrating -> happy; no focus -> sleep; nothing running -> idle;
    /// any run over 50 min -> tired; else focus. `now`/`celebrate_until` are epoch ms.
    pub fn derive(focus: &[Task], now: f64, celebrate_until: f64) -> Mood {
        if now < celebrate_until {
            return Mood::Happy;
        }
        if focus.is_empty() {
            return Mood::Sleep;
        }
        if !focus.iter().any(|t| t.running) {
            return Mood::Idle;
        }
        if focus.iter().any(|t| t.running && t.elapsed(now) > TIRED_MS) {
            return Mood::Tired;
        }
        Mood::Focus
    }
}

/// `Store.fmt`: `mm:ss`, or `h:mm:ss` from one hour.
pub fn fmt_clock(ms: i64) -> String {
    let s = ms.max(0) / 1000;
    let (h, m, sec) = (s / 3600, (s % 3600) / 60, s % 60);
    if h > 0 {
        format!("{h}:{m:02}:{sec:02}")
    } else {
        format!("{m:02}:{sec:02}")
    }
}

/// `Store.fmtShort`: `<1m`, `42m`, `1h 05m`.
pub fn fmt_short(ms: i64) -> String {
    let m = ms.max(0) / 60_000;
    if m < 1 {
        return "<1m".into();
    }
    let h = m / 60;
    if h > 0 {
        format!("{h}h {:02}m", m % 60)
    } else {
        format!("{m}m")
    }
}

/// Lowercase base-36 of `n` (JS `Number.prototype.toString(36)`).
pub fn to_base36(mut n: u64) -> String {
    const D: &[u8; 36] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    if n == 0 {
        return "0".into();
    }
    let mut v = Vec::new();
    while n > 0 {
        v.push(D[(n % 36) as usize]);
        n /= 36;
    }
    v.reverse();
    String::from_utf8(v).unwrap_or_default()
}

/// `uid()`: base36(epoch ms) + 5 base36 chars drawn from `random` (any u64 entropy).
pub fn make_uid(now_epoch_ms: Ms, random: u64) -> String {
    let mut r = random;
    let mut s = to_base36(now_epoch_ms as u64);
    for _ in 0..5 {
        s.push(to_base36(r % 36).chars().next().unwrap_or('0'));
        r /= 36;
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    const REAL: &str = r#"{"version":1,"lastSeen":1795550528319,"settings":{"petKind":"cat","petName":"Foquinho","autoAdvance":true,"hidden":false,"tipsEnabled":true,"lastTipDay":"2026-11-24","musicEnabled":true},"totalDone":0,"focus":[],"queue":[],"done":[]}"#;

    fn roundtrip(s: &str) {
        let a: Value = serde_json::from_str(s).unwrap();
        let st = StateFile::from_json(s).unwrap();
        let out = st.to_json();
        let b: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn real_file_roundtrip() {
        roundtrip(REAL);
        let st = StateFile::from_json(REAL).unwrap();
        assert_eq!(st.settings.last_tip_day, "2026-11-24");
        assert_eq!(st.last_seen, 1_795_550_528_319);
        assert!(st.settings.music_enabled);
    }

    #[test]
    fn full_roundtrip_with_tasks_and_unknown_keys() {
        let s = r#"{"version":1,"lastSeen":5,"settings":{"petKind":"slime","petName":"X","autoAdvance":false,"hidden":true,"tipsEnabled":false,"lastTipDay":"2026-1-5","musicEnabled":false,"future":{"a":1}},
        "totalDone":7,"newTop":[1,2],
        "focus":[{"tid":"abc","title":"Ação ç","accMs":1000,"startedAt":99,"running":true,"createdAt":50,"doneAt":0,"spentMs":0}],
        "queue":[{"tid":"q1","title":"q","accMs":0,"startedAt":0,"running":false,"createdAt":60,"doneAt":0,"spentMs":0}],
        "done":[{"tid":"d1","title":"d","accMs":5,"startedAt":0,"running":false,"createdAt":1,"doneAt":9,"spentMs":5}]}"#;
        roundtrip(s);
        let st = StateFile::from_json(s).unwrap();
        assert_eq!(st.settings.kind(), PetKind::Slime);
        assert_eq!(st.settings.extra["future"]["a"], 1);
        assert_eq!(st.extra["newTop"][1], 2);
        assert_eq!(st.focus[0].acc_ms, 1000);
        assert!(st.focus[0].running);
        assert_eq!(st.running_count(), 1);
    }

    #[test]
    fn lenient_loading() {
        let st = StateFile::from_json(
            r#"{"focus":[{"tid":7,"title":"t","accMs":"12","running":1,"startedAt":null,"spentMs":1.9}]}"#,
        )
        .unwrap();
        let t = &st.focus[0];
        assert_eq!((t.tid.as_str(), t.acc_ms, t.running, t.started_at, t.spent_ms), ("7", 12, true, 0, 1));
        assert_eq!(st.settings, Settings::default());
        assert_eq!(st.version, 1);
        assert!(StateFile::from_json("{bad").is_err());
        // integers are written without a decimal point
        assert!(!st.to_json().contains("1.0"));
    }

    #[test]
    fn json_style_one_space_indent() {
        let j = StateFile::default().to_json();
        assert!(j.starts_with("{\n \"version\": 1"), "{j}");
    }

    #[test]
    fn task_time_maths() {
        let mut t = Task::new("u".into(), "x", 1000.0);
        assert_eq!(t.created_at, 1000);
        t.resume(2000.0);
        assert_eq!(t.elapsed(5000.0), 3000);
        t.pause(5000.0);
        assert_eq!((t.acc_ms, t.running, t.started_at), (3000, false, 0));
        assert_eq!(t.elapsed(9000.0), 3000);
        // clock going backwards never yields negative time
        t.resume(9000.0);
        assert_eq!(t.elapsed(1.0), 3000);
    }

    #[test]
    fn formatting() {
        assert_eq!(fmt_clock(0), "00:00");
        assert_eq!(fmt_clock(65_000), "01:05");
        assert_eq!(fmt_clock(3_661_000), "1:01:01");
        assert_eq!(fmt_clock(-5), "00:00");
        assert_eq!(fmt_short(30_000), "<1m");
        assert_eq!(fmt_short(42 * 60_000), "42m");
        assert_eq!(fmt_short(65 * 60_000), "1h 05m");
        assert_eq!(level_of(0), 1);
        assert_eq!(level_of(12), 3);
        assert_eq!(xp_of(12), 0.4);
    }

    #[test]
    fn uid_and_base36() {
        assert_eq!(to_base36(0), "0");
        assert_eq!(to_base36(35), "z");
        assert_eq!(to_base36(36), "10");
        let u = make_uid(1_795_550_528_319.0, 123_456_789);
        assert_eq!(u.len(), to_base36(1_795_550_528_319).len() + 5);
        assert!(u.chars().all(|c| c.is_ascii_alphanumeric()));
    }

    #[test]
    fn mood_derivation() {
        let mut t = Task::new("a".into(), "a", 0.0);
        assert_eq!(Mood::derive(&[], 10.0, 0.0), Mood::Sleep);
        assert_eq!(Mood::derive(&[], 10.0, 100.0), Mood::Happy);
        assert_eq!(Mood::derive(&[t.clone()], 10.0, 0.0), Mood::Idle);
        t.resume(0.0);
        assert_eq!(Mood::derive(&[t.clone()], 1000.0, 0.0), Mood::Focus);
        assert_eq!(Mood::derive(&[t], 51.0 * 60_000.0, 0.0), Mood::Tired);
        assert_eq!(PetKind::parse("ghost"), PetKind::Ghost);
        assert_eq!(PetKind::parse("??"), PetKind::Cat);
        assert_eq!(Mood::Tired.as_str(), "tired");
    }
}
