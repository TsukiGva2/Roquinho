//! Store actions/effects and shell-level commands.
//!
//! `Action`s mirror the mutating functions of `qml/Store.qml` one-to-one (indices as in QML),
//! `Effect`s mirror its signals and side effects, and `AppCmd` is what the CLI/IPC/global
//! shortcuts ask of the shell (`Main.qml` `onCommand` + `Backend::runCommand`).

use crate::color::Color;

/// A mutation of the store (apply with the current epoch ms; returns `Effect`s).
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    /// `addTask(title, toFocus)`; empty/blank titles are ignored. Goes to focus when
    /// `to_focus`, or when `autoAdvance` is on and nothing is in focus.
    AddTask { title: String, to_focus: bool },
    /// `focusFromQueue(i, silent)`: queue[i] starts running in focus.
    FocusFromQueue { index: usize, silent: bool },
    /// `togglePause(i)` on a focus task.
    TogglePause(usize),
    /// `toggleAll()`: pause everything if anything runs, else resume everything.
    ToggleAll,
    /// `unfocus(i)`: pause focus[i] and put it at the front of the queue.
    Unfocus(usize),
    /// `complete(i)` on a focus task (auto-advances the queue when enabled).
    Complete(usize),
    /// `completeFirst()`.
    CompleteFirst,
    /// `removeQueue(i)`.
    RemoveQueue(usize),
    /// `renameQueue(i, title)`.
    RenameQueue { index: usize, title: String },
    /// `moveQueue(from, to)`.
    MoveQueue { from: usize, to: usize },
    /// `restoreDone(i)`: done[i] goes back to the front of the queue; `totalDone` decrements.
    RestoreDone(usize),
    /// `petKind` ("cat" | "ghost" | "slime").
    SetPetKind(String),
    SetPetName(String),
    SetAutoAdvance(bool),
    SetHidden(bool),
    SetTipsEnabled(bool),
    /// `lastTipDay` (a `day_key`).
    SetLastTipDay(String),
    /// Kept for round-tripping only; has no behaviour.
    SetMusicEnabled(bool),
    /// Periodic housekeeping (clock tick): lets the store refresh derived state.
    Tick,
}

/// A side effect or notification produced by applying an `Action`.
#[derive(Clone, Debug, PartialEq)]
pub enum Effect {
    /// `completed(title, spentMs)` signal.
    Completed { title: String, spent_ms: i64 },
    /// `added(title, toFocus)` signal (`toFocus` = where it actually went).
    Added { title: String, to_focus: bool },
    /// `petEvent(name)`: one of the [`pet_event`] names.
    PetEvent(String),
    /// `say(text)`: a short mascot quip replacing the status line for 2600 ms.
    Say(String),
    /// `celebrateUntil = now + 3200`; carries the epoch ms deadline.
    Celebrate { until: f64 },
    /// Tasks changed (`touch()`): re-layout lists, recompute running count/mood.
    ModelChanged,
    /// A setting changed.
    SettingsChanged,
    /// Persist soon (debounced 400 ms).
    Save,
}

/// Pet event/reaction names used by `petEvent()` and `Pet.react()`.
pub mod pet_event {
    pub const CELEBRATE: &str = "celebrate";
    pub const WAVE: &str = "wave";
    pub const SURPRISED: &str = "surprised";
    pub const READY: &str = "ready";
    pub const LOVE: &str = "love";
    pub const HAPPY: &str = "happy";
    pub const WINK: &str = "wink";
    pub const YAWN: &str = "yawn";
    pub const CURIOUS: &str = "curious";
    pub const HUM: &str = "hum";
    pub const SIGH: &str = "sigh";
    /// Default reaction duration in ms for an event name (`Pet.qml onPetEvent`).
    pub fn duration_ms(name: &str) -> f64 {
        match name {
            CELEBRATE => 3000.0,
            WAVE => 1700.0,
            SURPRISED => 1300.0,
            READY => 1500.0,
            LOVE => 1800.0,
            _ => 1600.0,
        }
    }
}

/// Target of the `screen` command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScreenArg {
    /// Cycle to the next monitor (`screen` / `screen next`).
    Next,
    /// Primary monitor (`primary` / `principal`).
    Primary,
    /// A RandR output name such as `HDMI-2`.
    Named(String),
}

/// A shell-level request from the CLI, IPC or a global shortcut.
#[derive(Clone, Debug, PartialEq)]
pub enum AppCmd {
    /// Expand if collapsed, collapse if expanded.
    Toggle,
    Open,
    Collapse,
    /// `add [text]`: with text queue it, without text open the input field.
    QuickAdd(Option<String>),
    /// `focus <text>`: create directly in focus.
    FocusNew(String),
    /// `hide` (QML: if already hidden, show again; else collapse to the dot).
    Hide,
    Show,
    CompleteFirst,
    /// `pause`: `toggleAll`.
    PauseAll,
    Quit,
    Screen(ScreenArg),
    /// Accent colour choice (`None` = default).
    SetAccent(Option<Color>),
    SetAutostart(bool),
    OpenSettings,
    CloseSettings,
    /// Make the mascot react (a [`pet_event`] name).
    PetReact(String),
    /// Unknown, or accepted-and-ignored (`play`, `next`, `prev`, `start`).
    Ignored,
}

impl AppCmd {
    /// The store action this command maps to, when it is purely a store mutation.
    pub fn to_action(&self) -> Option<Action> {
        match self {
            AppCmd::CompleteFirst => Some(Action::CompleteFirst),
            AppCmd::PauseAll => Some(Action::ToggleAll),
            AppCmd::FocusNew(t) => Some(Action::AddTask { title: t.clone(), to_focus: true }),
            AppCmd::QuickAdd(Some(t)) => Some(Action::AddTask { title: t.clone(), to_focus: false }),
            _ => None,
        }
    }
}

/// Maps `name` + `arg` (as in `Backend::dispatch`: first word, rest trimmed) to an `AppCmd`.
///
/// `toggle open add focus hide show done pause quit screen` are live; `play next prev` (music)
/// and `start` (autostart: "run, but open nothing") are accepted and `Ignored`.
pub fn parse_command(name: &str, arg: &str) -> AppCmd {
    let arg = arg.trim();
    match name {
        "toggle" => AppCmd::Toggle,
        "open" => AppCmd::Open,
        "add" => AppCmd::QuickAdd(if arg.is_empty() { None } else { Some(arg.to_string()) }),
        "focus" if !arg.is_empty() => AppCmd::FocusNew(arg.to_string()),
        "hide" => AppCmd::Hide,
        "show" => AppCmd::Show,
        "done" => AppCmd::CompleteFirst,
        "pause" => AppCmd::PauseAll,
        "quit" => AppCmd::Quit,
        "screen" => AppCmd::Screen(match arg {
            "" | "next" => ScreenArg::Next,
            "primary" | "principal" => ScreenArg::Primary,
            other => ScreenArg::Named(other.to_string()),
        }),
        // music and autostart no-ops
        "play" | "next" | "prev" | "start" => AppCmd::Ignored,
        _ => AppCmd::Ignored,
    }
}

/// Splits an IPC line into `(name, arg)` like `Backend::dispatch` (first space, arg trimmed).
pub fn split_line(line: &str) -> Option<(&str, &str)> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }
    Some(match line.split_once(' ') {
        Some((n, a)) => (n, a.trim()),
        None => (line, ""),
    })
}

/// Maps a `foco-maxrc [shortcuts]` key name (`quick-add`, `toggle-panel`, `toggle-hide`,
/// `complete`, `next-screen`) to its command. `music-*` keys are not commands here.
pub fn shortcut_command(key: &str) -> Option<AppCmd> {
    Some(match key {
        "quick-add" => AppCmd::QuickAdd(None),
        "toggle-panel" => AppCmd::Toggle,
        "toggle-hide" => AppCmd::Hide,
        "complete" => AppCmd::CompleteFirst,
        "next-screen" => AppCmd::Screen(ScreenArg::Next),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_all_commands() {
        assert_eq!(parse_command("toggle", ""), AppCmd::Toggle);
        assert_eq!(parse_command("open", ""), AppCmd::Open);
        assert_eq!(parse_command("add", ""), AppCmd::QuickAdd(None));
        assert_eq!(parse_command("add", "  comprar pão "), AppCmd::QuickAdd(Some("comprar pão".into())));
        assert_eq!(parse_command("focus", "x y"), AppCmd::FocusNew("x y".into()));
        assert_eq!(parse_command("focus", ""), AppCmd::Ignored);
        assert_eq!(parse_command("hide", ""), AppCmd::Hide);
        assert_eq!(parse_command("show", ""), AppCmd::Show);
        assert_eq!(parse_command("done", ""), AppCmd::CompleteFirst);
        assert_eq!(parse_command("pause", ""), AppCmd::PauseAll);
        assert_eq!(parse_command("quit", ""), AppCmd::Quit);
        assert_eq!(parse_command("screen", ""), AppCmd::Screen(ScreenArg::Next));
        assert_eq!(parse_command("screen", "next"), AppCmd::Screen(ScreenArg::Next));
        assert_eq!(parse_command("screen", "primary"), AppCmd::Screen(ScreenArg::Primary));
        assert_eq!(parse_command("screen", "principal"), AppCmd::Screen(ScreenArg::Primary));
        assert_eq!(parse_command("screen", "HDMI-1"), AppCmd::Screen(ScreenArg::Named("HDMI-1".into())));
        for n in ["play", "next", "prev", "start", "bogus", ""] {
            assert_eq!(parse_command(n, ""), AppCmd::Ignored, "{n}");
        }
    }

    #[test]
    fn line_splitting_and_shortcuts() {
        assert_eq!(split_line("add  oi mundo \n"), Some(("add", "oi mundo")));
        assert_eq!(split_line("toggle"), Some(("toggle", "")));
        assert_eq!(split_line("   "), None);
        assert_eq!(shortcut_command("quick-add"), Some(AppCmd::QuickAdd(None)));
        assert_eq!(shortcut_command("next-screen"), Some(AppCmd::Screen(ScreenArg::Next)));
        assert_eq!(shortcut_command("music-toggle"), None);
    }

    #[test]
    fn cmd_to_action() {
        assert_eq!(AppCmd::PauseAll.to_action(), Some(Action::ToggleAll));
        assert_eq!(
            AppCmd::QuickAdd(Some("a".into())).to_action(),
            Some(Action::AddTask { title: "a".into(), to_focus: false })
        );
        assert_eq!(AppCmd::Toggle.to_action(), None);
        assert_eq!(pet_event::duration_ms("celebrate"), 3000.0);
        assert_eq!(pet_event::duration_ms("hum"), 1600.0);
    }
}
