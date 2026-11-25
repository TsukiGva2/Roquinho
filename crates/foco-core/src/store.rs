//! `qml/Store.qml` as a pure state machine: `apply(Action, now_epoch_ms) -> Vec<Effect>`.
//!
//! Nothing here reads the clock except [`Store::load`] / [`Store::save`] convenience wrappers
//! (their `_at` variants take the time), so everything is testable. Derived values (mood,
//! status line, today's totals) are computed from `now` on demand; [`Store::next_change`]
//! says when one of them changes by itself so the app never needs a periodic ticker.

use crate::fsutil;
use foco_base::model::{make_uid, CELEBRATE_MS, MAX_DONE, TIRED_MS};
use foco_base::{
    epoch_ms, next_midnight_epoch, pet_event, start_of_day_epoch, Action, Effect, Mood, Settings,
    StateFile, Task,
};
use std::fs;
use std::io;
use std::path::Path;

/// `quipTimer.interval`: how long a quip replaces the status line.
pub const QUIP_MS: f64 = 2600.0;

pub const QUIPS_FOCUS_NEW: [&str; 3] = ["Hey ho, let's go!", "1, 2, 3, 4! Foco!", "Pogo mental iniciado."];
pub const QUIPS_QUEUED: [&str; 3] = ["Pra fila do gargarejo.", "Anotado. Procrastine com estilo.", "Guardei. Ela não foge, infelizmente."];
pub const QUIPS_FOCUS_FROM_QUEUE: [&str; 3] = ["Hey ho, let's go!", "Próxima faixa do lado B!", "Sem ensaio, só barulho!"];
pub const QUIP_RESUME: &str = "O intervalo acabou. Volta pro palco.";
pub const QUIPS_COMPLETE: [&str; 4] =
    ["Mais uma pro setlist!", "Anarchy in the to-do!", "Uma a menos. No future pra ela.", "Destruiu. Pogo!"];
/// `ExpandedView.qml` `onPetted` for the cat.
pub const QUIPS_PETTED_CAT: [&str; 3] = ["Ronrom… mas sou punk, hein.", "Cuidado com o moicano!", "Isso não sai daqui."];
/// `ExpandedView.qml` `onPetted` for the other mascots.
pub const QUIPS_PETTED_OTHER: [&str; 3] = ["Ei, sem stage diving!", "Cuidado com o visual!", "Tá, tá… mais um pouco."];
/// `Main.qml answerTip(never)`.
pub const QUIP_TIPS_OFF: &str = "Beleza, calo a boca. Milagre.";

fn seed_from_time() -> u64 {
    let t = epoch_ms().to_bits();
    let a = &t as *const u64 as u64;
    let s = t ^ (std::process::id() as u64).rotate_left(32) ^ a.rotate_left(17) ^ 0x9E37_79B9_7F4A_7C15;
    if s == 0 {
        0x2545_F491_4F6C_DD1D
    } else {
        s
    }
}

#[derive(Clone, Copy, PartialEq)]
struct Stamp {
    mood: Mood,
    sod: i64,
    quip: bool,
}

pub struct Store {
    st: StateFile,
    first_run: bool,
    celebrate_until: f64,
    quip: String,
    quip_until: f64,
    revision: u64,
    rng: u64,
    stamp: Option<Stamp>,
}

impl Default for Store {
    fn default() -> Self {
        Store::new()
    }
}

impl Store {
    /// An empty store (as after `load()` with no file, minus the first-run flag).
    pub fn new() -> Store {
        Store {
            st: StateFile::default(),
            first_run: false,
            celebrate_until: 0.0,
            quip: String::new(),
            quip_until: 0.0,
            revision: 1,
            rng: seed_from_time(),
            stamp: None,
        }
    }

    // ------------------------------------------------------------ persistence

    /// `Store.qml load()` from `path` at the current time. Does not write anything: like the
    /// QML (`load()` ends with `save()`), call [`Store::save`] afterwards so the file exists.
    pub fn load(path: &Path) -> Store {
        Store::load_at(path, epoch_ms())
    }

    pub fn load_at(path: &Path, now: f64) -> Store {
        let raw = fs::read(path).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default();
        let (s, corrupt) = Store::parse_at(&raw, now);
        if corrupt {
            // keep the unreadable file instead of silently overwriting it on the next save
            let mut bak = path.as_os_str().to_owned();
            bak.push(".corrupt");
            let _ = fs::copy(path, bak);
        }
        s
    }

    /// `load()` on the raw file text. Empty text = first run; invalid JSON = start from zero.
    pub fn from_json_at(raw: &str, now: f64) -> Store {
        Store::parse_at(raw, now).0
    }

    fn parse_at(raw: &str, now: f64) -> (Store, bool) {
        let mut s = Store::new();
        let mut corrupt = false;
        if raw.is_empty() {
            s.first_run = true;
        } else {
            match StateFile::from_json(raw) {
                Ok(mut st) => {
                    let n = now as i64;
                    if st.settings.pet_kind.is_empty() {
                        st.settings.pet_kind = "cat".into();
                    }
                    if st.settings.pet_name.is_empty() {
                        st.settings.pet_name = "Foquinho".into();
                    }
                    // running timers count only up to the last time the app was alive
                    let last_seen = (if st.last_seen == 0 { n } else { st.last_seen }).min(n);
                    for t in &mut st.focus {
                        if t.running {
                            t.acc_ms += (last_seen - t.started_at).max(0);
                            t.started_at = n;
                        }
                    }
                    for t in &mut st.queue {
                        t.pause(now);
                    }
                    st.last_seen = n;
                    s.st = st;
                }
                Err(_) => corrupt = !raw.trim().is_empty(),
            }
        }
        s.revision += 1; // load() ends with touch()
        (s, corrupt)
    }

    /// The JSON text `save()` writes, with `lastSeen = now` (`JSON.stringify(obj, null, 1)`).
    pub fn to_json_at(&self, now: f64) -> String {
        let last_seen = (now as i64).max(self.st.last_seen);
        if last_seen == self.st.last_seen {
            return self.st.to_json();
        }
        let mut st = self.st.clone();
        st.last_seen = last_seen;
        st.to_json()
    }

    /// Atomically writes the state file (tmp file in the same directory + rename).
    pub fn save(&self, path: &Path) -> io::Result<()> {
        self.save_at(path, epoch_ms())
    }

    pub fn save_at(&self, path: &Path, now: f64) -> io::Result<()> {
        fsutil::atomic_write(path, self.to_json_at(now).as_bytes())
    }

    /// Cheap `lastSeen` refresh (no revision bump, no effects): call before a heartbeat save.
    /// `save` already uses the current time; this only matters when saving with `to_json_at`.
    pub fn touch_last_seen(&mut self, now: f64) {
        self.st.last_seen = self.st.last_seen.max(now as i64);
    }

    // ------------------------------------------------------------ getters

    pub fn state(&self) -> &StateFile {
        &self.st
    }
    pub fn focus(&self) -> &[Task] {
        &self.st.focus
    }
    pub fn queue(&self) -> &[Task] {
        &self.st.queue
    }
    pub fn done(&self) -> &[Task] {
        &self.st.done
    }
    pub fn settings(&self) -> &Settings {
        &self.st.settings
    }
    pub fn total_done(&self) -> u32 {
        self.st.total_done
    }
    /// `level = 1 + floor(totalDone / 5)`.
    pub fn level(&self) -> u32 {
        self.st.level()
    }
    /// XP fraction of the current level, 0..1.
    pub fn xp(&self) -> f32 {
        self.st.xp()
    }
    /// No state file existed (or it was empty) at load.
    pub fn first_run(&self) -> bool {
        self.first_run
    }
    /// Bumped on every model change (tasks, settings, quips).
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn celebrate_until(&self) -> f64 {
        self.celebrate_until
    }
    pub fn running_count(&self) -> usize {
        self.st.running_count()
    }
    pub fn has_running(&self) -> bool {
        self.st.focus.iter().any(|t| t.running)
    }

    /// `Store.petMood`.
    pub fn mood(&self, now: f64) -> Mood {
        Mood::derive(&self.st.focus, now, self.celebrate_until)
    }

    /// The raw quip text (may be stale until a `Tick`; prefer [`Store::pet_line`]).
    pub fn quip(&self) -> &str {
        &self.quip
    }
    /// Epoch ms at which the current quip stops showing (0 when none). Schedule a wake here and
    /// apply `Action::Tick` when it fires.
    pub fn quip_until(&self) -> f64 {
        if self.quip.is_empty() {
            0.0
        } else {
            self.quip_until
        }
    }
    pub fn quip_active(&self, now: f64) -> bool {
        !self.quip.is_empty() && now < self.quip_until
    }

    /// `Store.petLine` without the music branch.
    pub fn pet_line(&self, now: f64) -> String {
        if self.quip_active(now) {
            return self.quip.clone();
        }
        match self.mood(now) {
            Mood::Sleep => {
                if self.st.queue.is_empty() {
                    "Zzz… no future e nenhuma demanda.".to_string()
                } else {
                    format!("Zzz… {} na fila. Não vão se fazer sozinhas.", self.st.queue.len())
                }
            }
            Mood::Idle => "Pausado. Intervalo do show.".to_string(),
            Mood::Happy => "Mais uma pro setlist!".to_string(),
            Mood::Tired => "Música do Ramones dura 2 min. Você tá em 50.".to_string(),
            Mood::Focus => {
                if self.st.focus.len() > 1 {
                    format!("{} em foco. Isso é caos, não multitarefa.", self.st.focus.len())
                } else {
                    "Uma por vez. Três acordes bastam.".to_string()
                }
            }
        }
    }

    /// `Store.today`: `(tasks completed today, ms spent today)`; the ms include running focus.
    pub fn today(&self, now: f64) -> (usize, i64) {
        let sod = start_of_day_epoch(now) as i64;
        let mut count = 0;
        let mut ms = 0;
        for t in &self.st.done {
            if t.done_at < sod {
                break;
            }
            count += 1;
            ms += t.spent_ms;
        }
        ms += self.st.focus.iter().map(|t| t.elapsed(now)).sum::<i64>();
        (count, ms)
    }

    /// `elapsedOf(focus[i])` (0 when out of range).
    pub fn elapsed_of(&self, i: usize, now: f64) -> i64 {
        self.st.focus.get(i).map_or(0, |t| t.elapsed(now))
    }

    /// The earliest future epoch-ms instant at which a derived value changes on its own:
    /// the end of the celebration, a running task passing 50 min ("tired"), the quip expiring,
    /// and midnight while `today()` has completed tasks. `None` when nothing will change.
    /// (The per-second clock of running timers is the UI's business.)
    pub fn next_change(&self, now: f64) -> Option<f64> {
        let mut best: Option<f64> = None;
        let mut cand = |t: f64| {
            if t > now {
                best = Some(best.map_or(t, |b| b.min(t)));
            }
        };
        cand(self.celebrate_until);
        if !self.quip.is_empty() {
            cand(self.quip_until);
        }
        for t in self.st.focus.iter().filter(|t| t.running) {
            // elapsed(now) > TIRED_MS  <=>  floor(now) >= started + (TIRED - acc) + 1
            cand((t.started_at + (TIRED_MS - t.acc_ms) + 1) as f64);
        }
        let sod = start_of_day_epoch(now) as i64;
        if self.st.done.first().is_some_and(|t| t.done_at >= sod) {
            cand(next_midnight_epoch(now));
        }
        best
    }

    // ------------------------------------------------------------ quips

    fn rand(&mut self) -> u64 {
        // xorshift64*
        let mut x = self.rng;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.rng = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// Seeds the quip/uid RNG (tests).
    pub fn seed(&mut self, seed: u64) {
        self.rng = if seed == 0 { 1 } else { seed };
    }

    fn say_fx(&mut self, text: &str, now: f64, fx: &mut Vec<Effect>) {
        self.quip = text.to_string();
        self.quip_until = now + QUIP_MS;
        self.revision += 1;
        fx.push(Effect::Say(text.to_string()));
    }

    fn say_one_of_fx(&mut self, list: &[&str], now: f64, fx: &mut Vec<Effect>) {
        let i = (self.rand() % list.len() as u64) as usize;
        self.say_fx(list[i], now, fx);
    }

    /// `Store.say(text)`.
    pub fn say(&mut self, text: &str, now: f64) -> Vec<Effect> {
        let mut fx = Vec::new();
        self.say_fx(text, now, &mut fx);
        fx
    }

    /// `Store.sayOneOf(list)`.
    pub fn say_one_of(&mut self, list: &[&str], now: f64) -> Vec<Effect> {
        let mut fx = Vec::new();
        self.say_one_of_fx(list, now, &mut fx);
        fx
    }

    /// The mascot was petted (`ExpandedView.qml onPetted`): a quip that depends on the kind.
    pub fn petted(&mut self, now: f64) -> Vec<Effect> {
        let list: &[&str] =
            if self.st.settings.pet_kind == "cat" { &QUIPS_PETTED_CAT } else { &QUIPS_PETTED_OTHER };
        self.say_one_of(list, now)
    }

    /// `Main.qml answerTip(never)`: records today's tip, optionally turns tips off. `day_key` is
    /// `foco_base::day_key(now)`.
    pub fn answer_tip(&mut self, never: bool, day_key: &str, now: f64) -> Vec<Effect> {
        let mut fx = self.apply(Action::SetLastTipDay(day_key.to_string()), now);
        if never {
            fx.extend(self.apply(Action::SetTipsEnabled(false), now));
            self.say_fx(QUIP_TIPS_OFF, now, &mut fx);
        } else {
            fx.push(Effect::PetEvent(pet_event::LOVE.to_string()));
        }
        fx
    }

    // ------------------------------------------------------------ actions

    fn touch(&mut self, fx: &mut Vec<Effect>) {
        self.revision += 1;
        if !fx.contains(&Effect::ModelChanged) {
            fx.push(Effect::ModelChanged);
        }
    }

    fn settings_changed(&mut self, fx: &mut Vec<Effect>) {
        self.revision += 1;
        if !fx.contains(&Effect::SettingsChanged) {
            fx.push(Effect::SettingsChanged);
        }
    }

    fn new_task(&mut self, title: &str, now: f64) -> Task {
        let r = self.rand();
        Task::new(make_uid(now, r), title, now)
    }

    /// `focusFromQueue`; returns whether something moved.
    fn focus_from_queue(&mut self, i: usize, silent: bool, now: f64, fx: &mut Vec<Effect>) -> bool {
        if i >= self.st.queue.len() {
            return false;
        }
        let mut t = self.st.queue.remove(i);
        t.resume(now);
        self.st.focus.push(t);
        self.touch(fx);
        if !silent {
            fx.push(Effect::PetEvent(pet_event::READY.to_string()));
            self.say_one_of_fx(&QUIPS_FOCUS_FROM_QUEUE, now, fx);
        }
        true
    }

    fn toggle_pause(&mut self, i: usize, now: f64, fx: &mut Vec<Effect>) {
        let Some(t) = self.st.focus.get_mut(i) else { return };
        if t.running {
            t.pause(now);
        } else {
            t.resume(now);
            fx.push(Effect::PetEvent(pet_event::READY.to_string()));
            self.say_fx(QUIP_RESUME, now, fx);
        }
        self.touch(fx);
    }

    fn complete(&mut self, i: usize, now: f64, fx: &mut Vec<Effect>) {
        if i >= self.st.focus.len() {
            return;
        }
        let mut t = self.st.focus.remove(i);
        t.pause(now);
        t.done_at = now as i64;
        t.spent_ms = t.acc_ms;
        let (title, spent) = (t.title.clone(), t.spent_ms);
        self.st.done.insert(0, t);
        self.st.done.truncate(MAX_DONE);
        self.st.total_done += 1;
        self.celebrate_until = now + CELEBRATE_MS;
        fx.push(Effect::Celebrate { until: self.celebrate_until });
        if self.st.settings.auto_advance && !self.st.queue.is_empty() {
            self.focus_from_queue(0, true, now, fx);
        }
        self.touch(fx);
        fx.push(Effect::Completed { title, spent_ms: spent });
        fx.push(Effect::PetEvent(pet_event::CELEBRATE.to_string()));
        self.say_one_of_fx(&QUIPS_COMPLETE, now, fx);
    }

    /// Applies one action at `now` (epoch ms) and returns the effects, in the order the QML
    /// signals fire. `Effect::Save` (debounce 400 ms) is appended once when anything changed.
    pub fn apply(&mut self, action: Action, now: f64) -> Vec<Effect> {
        let mut fx: Vec<Effect> = Vec::new();
        match action {
            Action::AddTask { title, to_focus } => {
                let title = title.trim();
                if title.is_empty() {
                    return fx;
                }
                let mut t = self.new_task(title, now);
                let go_focus = to_focus || (self.st.settings.auto_advance && self.st.focus.is_empty());
                if go_focus {
                    t.resume(now);
                    self.st.focus.push(t);
                } else {
                    self.st.queue.push(t);
                }
                self.touch(&mut fx);
                fx.push(Effect::Added { title: title.to_string(), to_focus: go_focus });
                fx.push(Effect::PetEvent(
                    if go_focus { pet_event::READY } else { pet_event::SURPRISED }.to_string(),
                ));
                if go_focus {
                    self.say_one_of_fx(&QUIPS_FOCUS_NEW, now, &mut fx);
                } else {
                    self.say_one_of_fx(&QUIPS_QUEUED, now, &mut fx);
                }
            }
            Action::FocusFromQueue { index, silent } => {
                self.focus_from_queue(index, silent, now, &mut fx);
            }
            Action::TogglePause(i) => self.toggle_pause(i, now, &mut fx),
            Action::ToggleAll => {
                let any_running = self.has_running();
                for i in 0..self.st.focus.len() {
                    if self.st.focus[i].running == any_running {
                        self.toggle_pause(i, now, &mut fx);
                    }
                }
            }
            Action::Unfocus(i) => {
                if i < self.st.focus.len() {
                    let mut t = self.st.focus.remove(i);
                    t.pause(now);
                    self.st.queue.insert(0, t);
                    self.touch(&mut fx);
                }
            }
            Action::Complete(i) => self.complete(i, now, &mut fx),
            Action::CompleteFirst => {
                if !self.st.focus.is_empty() {
                    self.complete(0, now, &mut fx);
                }
            }
            Action::RemoveQueue(i) => {
                if i < self.st.queue.len() {
                    self.st.queue.remove(i);
                    self.touch(&mut fx);
                }
            }
            Action::RenameQueue { index, title } => {
                let title = title.trim();
                if !title.is_empty() && index < self.st.queue.len() {
                    self.st.queue[index].title = title.to_string();
                    self.touch(&mut fx);
                }
            }
            Action::MoveQueue { from, to } => {
                let n = self.st.queue.len();
                if from != to && from < n && to < n {
                    let t = self.st.queue.remove(from);
                    self.st.queue.insert(to, t);
                    self.touch(&mut fx);
                }
            }
            Action::RestoreDone(i) => {
                if i < self.st.done.len() {
                    let mut t = self.st.done.remove(i);
                    t.done_at = 0;
                    t.spent_ms = 0;
                    self.st.queue.insert(0, t);
                    self.st.total_done = self.st.total_done.saturating_sub(1);
                    self.touch(&mut fx);
                }
            }
            Action::SetPetKind(v) => {
                if self.st.settings.pet_kind != v {
                    self.st.settings.pet_kind = v;
                    self.settings_changed(&mut fx);
                }
            }
            Action::SetPetName(v) => {
                if self.st.settings.pet_name != v {
                    self.st.settings.pet_name = v;
                    self.settings_changed(&mut fx);
                }
            }
            Action::SetAutoAdvance(v) => {
                if self.st.settings.auto_advance != v {
                    self.st.settings.auto_advance = v;
                    self.settings_changed(&mut fx);
                }
            }
            Action::SetHidden(v) => {
                if self.st.settings.hidden != v {
                    self.st.settings.hidden = v;
                    self.settings_changed(&mut fx);
                }
            }
            Action::SetTipsEnabled(v) => {
                if self.st.settings.tips_enabled != v {
                    self.st.settings.tips_enabled = v;
                    self.settings_changed(&mut fx);
                }
            }
            Action::SetLastTipDay(v) => {
                if self.st.settings.last_tip_day != v {
                    self.st.settings.last_tip_day = v;
                    self.settings_changed(&mut fx);
                }
            }
            Action::SetMusicEnabled(v) => {
                if self.st.settings.music_enabled != v {
                    self.st.settings.music_enabled = v;
                    self.settings_changed(&mut fx);
                }
            }
            Action::Tick => {
                if !self.quip.is_empty() && now >= self.quip_until {
                    self.quip.clear();
                    self.quip_until = 0.0;
                }
                if self.celebrate_until != 0.0 && now >= self.celebrate_until {
                    self.celebrate_until = 0.0;
                }
                let stamp = Stamp {
                    mood: self.mood(now),
                    sod: start_of_day_epoch(now) as i64,
                    quip: !self.quip.is_empty(),
                };
                if self.stamp != Some(stamp) {
                    self.stamp = Some(stamp);
                    self.revision += 1;
                    fx.push(Effect::ModelChanged);
                }
                return fx; // derived refresh only: nothing to persist
            }
        }
        if !fx.is_empty() && fx.iter().any(|e| matches!(e, Effect::ModelChanged | Effect::SettingsChanged)) {
            fx.push(Effect::Save);
        }
        fx
    }
}

#[cfg(test)]
mod tests;
