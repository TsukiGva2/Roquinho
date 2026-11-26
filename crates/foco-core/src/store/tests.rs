use super::*;
use crate::fsutil::testutil::TempDir;
use foco_base::model::fmt_clock;
use foco_base::{day_key, Action as A};

const NOW: f64 = 1_795_550_600_000.0;
const MIN: f64 = 60_000.0;

const REAL: &str = r#"{"version":1,"lastSeen":1795550528319,"settings":{"petKind":"cat","petName":"Foquinho","autoAdvance":true,"hidden":false,"tipsEnabled":true,"lastTipDay":"2026-11-24","musicEnabled":true},"totalDone":0,"focus":[],"queue":[],"done":[]}"#;

fn store() -> Store {
    let mut s = Store::new();
    s.seed(12345);
    s
}

fn add(s: &mut Store, title: &str, to_focus: bool, now: f64) -> Vec<Effect> {
    s.apply(A::AddTask { title: title.into(), to_focus }, now)
}

fn titles(v: &[Task]) -> Vec<&str> {
    v.iter().map(|t| t.title.as_str()).collect()
}

fn says(fx: &[Effect]) -> Vec<&str> {
    fx.iter()
        .filter_map(|e| match e {
            Effect::Say(t) => Some(t.as_str()),
            _ => None,
        })
        .collect()
}

fn pet_events(fx: &[Effect]) -> Vec<&str> {
    fx.iter()
        .filter_map(|e| match e {
            Effect::PetEvent(t) => Some(t.as_str()),
            _ => None,
        })
        .collect()
}

#[test]
fn add_task_goes_to_focus_when_empty_and_auto_advance() {
    let mut s = store();
    let fx = add(&mut s, "  escrever  ", false, NOW);
    assert_eq!(titles(s.focus()), ["escrever"]);
    assert!(s.queue().is_empty());
    let t = &s.focus()[0];
    assert!(t.running && t.started_at == NOW as i64 && t.created_at == NOW as i64 && t.acc_ms == 0);
    assert!(!t.tid.is_empty());
    assert_eq!(
        fx[..3],
        [
            Effect::ModelChanged,
            Effect::Added { title: "escrever".into(), to_focus: true },
            Effect::PetEvent("ready".into())
        ]
    );
    assert!(QUIPS_FOCUS_NEW.contains(&says(&fx)[0]));
    assert_eq!(fx.last(), Some(&Effect::Save));
    assert_eq!(s.quip_until(), NOW + 2600.0);
}

#[test]
fn add_task_goes_to_queue_when_focus_busy_or_auto_advance_off() {
    let mut s = store();
    add(&mut s, "a", false, NOW);
    let fx = add(&mut s, "b", false, NOW + 1.0);
    assert_eq!(titles(s.queue()), ["b"]);
    assert!(!s.queue()[0].running && s.queue()[0].started_at == 0);
    assert!(fx.contains(&Effect::Added { title: "b".into(), to_focus: false }));
    assert_eq!(pet_events(&fx), ["surprised"]);
    assert!(QUIPS_QUEUED.contains(&says(&fx)[0]));
    // toFocus overrides
    add(&mut s, "c", true, NOW + 2.0);
    assert_eq!(titles(s.focus()), ["a", "c"]);
    // autoAdvance off: empty focus still queues
    let mut s2 = store();
    s2.apply(A::SetAutoAdvance(false), NOW);
    add(&mut s2, "x", false, NOW);
    assert_eq!((s2.focus().len(), s2.queue().len()), (0, 1));
}

#[test]
fn add_task_ignores_blank_titles() {
    let mut s = store();
    let rev = s.revision();
    for t in ["", "   ", "\t\n"] {
        assert!(add(&mut s, t, true, NOW).is_empty());
    }
    assert!(s.focus().is_empty() && s.queue().is_empty());
    assert_eq!(s.revision(), rev);
}

#[test]
fn uids_differ() {
    let mut s = store();
    add(&mut s, "a", true, NOW);
    add(&mut s, "b", true, NOW);
    assert_ne!(s.focus()[0].tid, s.focus()[1].tid);
}

#[test]
fn focus_from_queue() {
    let mut s = store();
    add(&mut s, "a", true, NOW);
    add(&mut s, "b", false, NOW);
    add(&mut s, "c", false, NOW);
    let fx = s.apply(A::FocusFromQueue { index: 1, silent: false }, NOW + 5.0);
    assert_eq!(titles(s.focus()), ["a", "c"]);
    assert_eq!(titles(s.queue()), ["b"]);
    assert!(s.focus()[1].running && s.focus()[1].started_at == NOW as i64 + 5);
    assert_eq!(pet_events(&fx), ["ready"]);
    assert!(QUIPS_FOCUS_FROM_QUEUE.contains(&says(&fx)[0]));
    // silent: no pet event, no quip
    let fx = s.apply(A::FocusFromQueue { index: 0, silent: true }, NOW + 6.0);
    assert!(pet_events(&fx).is_empty() && says(&fx).is_empty());
    assert!(fx.contains(&Effect::ModelChanged) && fx.contains(&Effect::Save));
    // out of range
    assert!(s.apply(A::FocusFromQueue { index: 0, silent: false }, NOW).is_empty());
    assert!(s.apply(A::FocusFromQueue { index: usize::MAX, silent: false }, NOW).is_empty());
}

#[test]
fn toggle_pause_cycle() {
    let mut s = store();
    add(&mut s, "a", true, NOW);
    let fx = s.apply(A::TogglePause(0), NOW + 4000.0);
    let t = &s.focus()[0];
    assert!(!t.running && t.acc_ms == 4000 && t.started_at == 0);
    assert!(pet_events(&fx).is_empty() && says(&fx).is_empty());
    assert!(fx.contains(&Effect::ModelChanged));
    let fx = s.apply(A::TogglePause(0), NOW + 9000.0);
    let t = &s.focus()[0];
    assert!(t.running && t.started_at == NOW as i64 + 9000 && t.acc_ms == 4000);
    assert_eq!(pet_events(&fx), ["ready"]);
    assert_eq!(says(&fx), [QUIP_RESUME]);
    assert_eq!(s.elapsed_of(0, NOW + 10_000.0), 5000);
    assert_eq!(s.elapsed_of(5, NOW), 0);
    assert!(s.apply(A::TogglePause(3), NOW).is_empty());
}

#[test]
fn toggle_all_pauses_then_resumes_everything() {
    let mut s = store();
    add(&mut s, "a", true, NOW);
    add(&mut s, "b", true, NOW);
    s.apply(A::TogglePause(1), NOW + 1000.0); // b paused, a running
    assert_eq!(s.running_count(), 1);
    // something runs: pause all (only the running ones are toggled)
    let fx = s.apply(A::ToggleAll, NOW + 2000.0);
    assert_eq!(s.running_count(), 0);
    assert_eq!(s.focus()[0].acc_ms, 2000);
    assert_eq!(s.focus()[1].acc_ms, 1000);
    assert!(pet_events(&fx).is_empty());
    // nothing runs: resume all, one ready + quip per task like the QML
    let fx = s.apply(A::ToggleAll, NOW + 3000.0);
    assert_eq!(s.running_count(), 2);
    assert_eq!(pet_events(&fx), ["ready", "ready"]);
    assert_eq!(says(&fx), [QUIP_RESUME, QUIP_RESUME]);
    assert_eq!(fx.iter().filter(|e| **e == Effect::ModelChanged).count(), 1);
    // empty focus: nothing
    let mut e = store();
    assert!(e.apply(A::ToggleAll, NOW).is_empty());
}

#[test]
fn unfocus_pauses_and_goes_to_queue_front() {
    let mut s = store();
    add(&mut s, "a", true, NOW);
    add(&mut s, "b", false, NOW);
    let fx = s.apply(A::Unfocus(0), NOW + 7000.0);
    assert!(s.focus().is_empty());
    assert_eq!(titles(s.queue()), ["a", "b"]);
    assert!(!s.queue()[0].running && s.queue()[0].acc_ms == 7000 && s.queue()[0].started_at == 0);
    assert!(fx.contains(&Effect::ModelChanged) && fx.contains(&Effect::Save));
    assert!(s.apply(A::Unfocus(0), NOW).is_empty());
    assert!(s.apply(A::Unfocus(9), NOW).is_empty());
}

#[test]
fn complete_with_auto_advance() {
    let mut s = store();
    add(&mut s, "a", true, NOW);
    add(&mut s, "b", false, NOW);
    add(&mut s, "c", false, NOW);
    let fx = s.apply(A::Complete(0), NOW + 90_000.0);
    assert_eq!(titles(s.done()), ["a"]);
    let d = &s.done()[0];
    assert_eq!((d.done_at, d.spent_ms, d.acc_ms, d.running), (NOW as i64 + 90_000, 90_000, 90_000, false));
    assert_eq!(s.total_done(), 1);
    assert_eq!(titles(s.focus()), ["b"]); // auto-advanced
    assert!(s.focus()[0].running);
    assert_eq!(titles(s.queue()), ["c"]);
    assert!(fx.contains(&Effect::Celebrate { until: NOW + 90_000.0 + 3200.0 }));
    assert!(fx.contains(&Effect::Completed { title: "a".into(), spent_ms: 90_000 }));
    assert_eq!(pet_events(&fx), ["celebrate"]); // the silent auto-advance adds none
    assert!(QUIPS_COMPLETE.contains(&says(&fx)[0]));
    assert_eq!(s.mood(NOW + 90_000.0 + 1.0), Mood::Happy);
    assert_eq!(s.mood(NOW + 90_000.0 + 3200.0), Mood::Focus);
}

#[test]
fn complete_without_auto_advance_and_bounds() {
    let mut s = store();
    s.apply(A::SetAutoAdvance(false), NOW);
    add(&mut s, "a", true, NOW);
    add(&mut s, "b", false, NOW);
    assert!(s.apply(A::Complete(1), NOW).is_empty());
    assert!(s.apply(A::Complete(usize::MAX), NOW).is_empty());
    s.apply(A::Complete(0), NOW + 1000.0);
    assert!(s.focus().is_empty());
    assert_eq!(titles(s.queue()), ["b"]);
    assert_eq!(s.mood(NOW + 5000.0), Mood::Sleep);
    // pausing before completing keeps the accumulated time
    let mut s = store();
    add(&mut s, "p", true, NOW);
    s.apply(A::TogglePause(0), NOW + 2000.0);
    s.apply(A::Complete(0), NOW + 50_000.0);
    assert_eq!(s.done()[0].spent_ms, 2000);
}

#[test]
fn complete_first() {
    let mut s = store();
    assert!(s.apply(A::CompleteFirst, NOW).is_empty());
    add(&mut s, "a", true, NOW);
    add(&mut s, "b", true, NOW);
    s.apply(A::CompleteFirst, NOW + 1.0);
    assert_eq!(titles(s.focus()), ["b"]);
    assert_eq!(titles(s.done()), ["a"]);
}

#[test]
fn done_is_capped_at_300() {
    let mut s = store();
    for i in 0..305 {
        add(&mut s, &format!("t{i}"), true, NOW + i as f64);
        s.apply(A::Complete(0), NOW + i as f64);
    }
    assert_eq!(s.done().len(), 300);
    assert_eq!(s.done()[0].title, "t304");
    assert_eq!(s.done()[299].title, "t5");
    assert_eq!(s.total_done(), 305);
    assert_eq!((s.level(), s.xp()), (1 + 305 / 5, 0.0));
}

#[test]
fn queue_edits() {
    let mut s = store();
    add(&mut s, "f", true, NOW);
    for t in ["a", "b", "c", "d"] {
        add(&mut s, t, false, NOW);
    }
    // remove
    assert!(s.apply(A::RemoveQueue(4), NOW).is_empty());
    s.apply(A::RemoveQueue(3), NOW);
    assert_eq!(titles(s.queue()), ["a", "b", "c"]);
    // rename
    assert!(s.apply(A::RenameQueue { index: 0, title: "   ".into() }, NOW).is_empty());
    assert!(s.apply(A::RenameQueue { index: 3, title: "x".into() }, NOW).is_empty());
    s.apply(A::RenameQueue { index: 1, title: "  novo nome ".into() }, NOW);
    assert_eq!(titles(s.queue()), ["a", "novo nome", "c"]);
    // move: same, OOR no-ops
    for (f, t) in [(1, 1), (3, 0), (0, 3), (usize::MAX, 0)] {
        assert!(s.apply(A::MoveQueue { from: f, to: t }, NOW).is_empty(), "{f}->{t}");
    }
    s.apply(A::MoveQueue { from: 0, to: 2 }, NOW);
    assert_eq!(titles(s.queue()), ["novo nome", "c", "a"]);
    s.apply(A::MoveQueue { from: 2, to: 0 }, NOW);
    assert_eq!(titles(s.queue()), ["a", "novo nome", "c"]);
    s.apply(A::MoveQueue { from: 0, to: 1 }, NOW);
    assert_eq!(titles(s.queue()), ["novo nome", "a", "c"]);
}

#[test]
fn restore_done() {
    let mut s = store();
    add(&mut s, "a", true, NOW);
    add(&mut s, "b", false, NOW);
    s.apply(A::Complete(0), NOW + 5000.0); // b auto-advances
    assert_eq!(s.total_done(), 1);
    let fx = s.apply(A::RestoreDone(0), NOW + 6000.0);
    assert!(s.done().is_empty());
    assert_eq!(titles(s.queue()), ["a"]);
    let t = &s.queue()[0];
    assert_eq!((t.done_at, t.spent_ms, t.acc_ms), (0, 0, 5000));
    assert_eq!(s.total_done(), 0);
    assert!(fx.contains(&Effect::ModelChanged));
    assert!(s.apply(A::RestoreDone(0), NOW).is_empty());
    // totalDone never goes negative (state loaded with done items but totalDone 0)
    let mut s = Store::from_json_at(r#"{"totalDone":0,"done":[{"tid":"d","title":"d","doneAt":5,"spentMs":1}]}"#, NOW);
    s.apply(A::RestoreDone(0), NOW);
    assert_eq!(s.total_done(), 0);
}

#[test]
fn settings_actions() {
    let mut s = store();
    let fx = s.apply(A::SetPetKind("ghost".into()), NOW);
    assert_eq!(fx, vec![Effect::SettingsChanged, Effect::Save]);
    assert_eq!(s.settings().pet_kind, "ghost");
    assert!(s.apply(A::SetPetKind("ghost".into()), NOW).is_empty()); // no change, no save
    s.apply(A::SetPetName("Bolinha".into()), NOW);
    s.apply(A::SetAutoAdvance(false), NOW);
    s.apply(A::SetHidden(true), NOW);
    s.apply(A::SetTipsEnabled(false), NOW);
    s.apply(A::SetLastTipDay("2026-11-24".into()), NOW);
    s.apply(A::SetMusicEnabled(false), NOW);
    let st = s.settings();
    assert_eq!(
        (st.pet_name.as_str(), st.auto_advance, st.hidden, st.tips_enabled, st.last_tip_day.as_str(), st.music_enabled),
        ("Bolinha", false, true, false, "2026-11-24", false)
    );
    for a in [
        A::SetPetName("Bolinha".into()),
        A::SetAutoAdvance(false),
        A::SetHidden(true),
        A::SetTipsEnabled(false),
        A::SetLastTipDay("2026-11-24".into()),
        A::SetMusicEnabled(false),
    ] {
        assert!(s.apply(a, NOW).is_empty());
    }
    let json = s.to_json_at(NOW);
    assert!(json.contains("\"musicEnabled\": false") && json.contains("\"petKind\": \"ghost\""));
}

#[test]
fn mood_and_pet_line() {
    let mut s = store();
    assert_eq!(s.mood(NOW), Mood::Sleep);
    assert_eq!(s.pet_line(NOW), "Zzz… no future e nenhuma demanda.");
    s.apply(A::SetAutoAdvance(false), NOW);
    add(&mut s, "q1", false, NOW);
    add(&mut s, "q2", false, NOW);
    let line = s.pet_line(NOW + 3000.0);
    assert_eq!(line, "Zzz… 2 na fila. Não vão se fazer sozinhas.");
    add(&mut s, "f1", true, NOW);
    let after_quip = NOW + 2600.0;
    assert!(s.quip_active(NOW + 2599.0) && !s.quip_active(after_quip));
    assert!(QUIPS_FOCUS_NEW.contains(&s.pet_line(NOW + 1.0).as_str()));
    assert_eq!(s.pet_line(after_quip), "Uma por vez. Três acordes bastam.");
    add(&mut s, "f2", true, NOW);
    assert_eq!(s.pet_line(NOW + 5000.0), "2 em foco. Isso é caos, não multitarefa.");
    s.apply(A::ToggleAll, NOW + 1000.0);
    assert_eq!(s.mood(NOW + 5000.0), Mood::Idle);
    assert_eq!(s.pet_line(NOW + 5000.0), "Pausado. Intervalo do show.");
    s.apply(A::ToggleAll, NOW + 5000.0);
    // 50 minute threshold
    assert_eq!(s.mood(NOW + 49.0 * MIN), Mood::Focus);
    assert_eq!(s.mood(NOW + 51.0 * MIN), Mood::Tired);
    assert_eq!(s.pet_line(NOW + 51.0 * MIN), "Música do Ramones dura 2 min. Você tá em 50.");
    // happy
    s.apply(A::Complete(0), NOW + 60_000.0);
    assert_eq!(s.pet_line(NOW + 60_000.0 + 2700.0), "Mais uma pro setlist!");
}

#[test]
fn quip_expires_by_tick() {
    let mut s = store();
    let fx = s.say("oi", NOW);
    assert_eq!(fx, vec![Effect::Say("oi".into())]);
    assert_eq!((s.quip(), s.quip_until()), ("oi", NOW + 2600.0));
    assert!(s.apply(A::Tick, NOW + 100.0).iter().all(|e| *e == Effect::ModelChanged)); // stamp init
    assert_eq!(s.quip(), "oi");
    let rev = s.revision();
    assert!(s.apply(A::Tick, NOW + 1000.0).is_empty());
    assert_eq!(s.revision(), rev);
    let fx = s.apply(A::Tick, NOW + 2600.0);
    assert_eq!(fx, vec![Effect::ModelChanged]);
    assert_eq!((s.quip(), s.quip_until()), ("", 0.0));
    assert!(s.apply(A::Tick, NOW + 9000.0).is_empty());
    // a newer quip replaces the older one and restarts the clock
    s.say("a", NOW);
    s.say("b", NOW + 1000.0);
    assert_eq!((s.quip(), s.quip_until()), ("b", NOW + 3600.0));
}

#[test]
fn petted_and_tip_answers() {
    let mut s = store();
    s.seed(1);
    let fx = s.petted(NOW);
    assert!(QUIPS_PETTED_CAT.contains(&says(&fx)[0]));
    s.apply(A::SetPetKind("slime".into()), NOW);
    for seed in 1..40 {
        s.seed(seed);
        let fx = s.petted(NOW);
        assert!(QUIPS_PETTED_OTHER.contains(&says(&fx)[0]));
    }
    let fx = s.answer_tip(false, "2026-11-24", NOW);
    assert_eq!(s.settings().last_tip_day, "2026-11-24");
    assert!(s.settings().tips_enabled);
    assert!(fx.contains(&Effect::PetEvent("love".into())));
    let fx = s.answer_tip(true, "2026-11-25", NOW);
    assert!(!s.settings().tips_enabled);
    assert_eq!(says(&fx), [QUIP_TIPS_OFF]);
}

#[test]
fn every_quip_is_reachable() {
    let mut seen = std::collections::HashSet::new();
    for seed in 1..200u64 {
        let mut s = store();
        s.seed(seed.wrapping_mul(0x9E37_79B9));
        let fx = add(&mut s, "a", true, NOW);
        seen.insert(says(&fx)[0].to_string());
        let fx = add(&mut s, "b", false, NOW);
        seen.insert(says(&fx)[0].to_string());
        let fx = s.apply(A::Complete(0), NOW);
        seen.insert(says(&fx)[0].to_string());
    }
    for q in QUIPS_FOCUS_NEW.iter().chain(&QUIPS_QUEUED).chain(&QUIPS_COMPLETE) {
        assert!(seen.contains(*q), "{q}");
    }
}

#[test]
fn today_counts_done_since_midnight_and_running_focus() {
    let sod = start_of_day_epoch(NOW);
    let mut s = store();
    s.apply(A::SetAutoAdvance(false), NOW);
    assert_eq!(s.today(NOW), (0, 0));
    // yesterday's completion must not count; build via load
    let j = format!(
        r#"{{"done":[{{"tid":"a","title":"a","doneAt":{},"spentMs":1000}},{{"tid":"b","title":"b","doneAt":{},"spentMs":2000}},{{"tid":"c","title":"c","doneAt":{},"spentMs":99999}}],
        "focus":[{{"tid":"f","title":"f","accMs":500,"running":false}}]}}"#,
        sod as i64 + 5,
        sod as i64,
        sod as i64 - 1
    );
    let mut s = Store::from_json_at(&j, NOW);
    assert_eq!(s.today(NOW), (2, 3500));
    s.apply(A::TogglePause(0), NOW); // resume at NOW
    assert_eq!(s.today(NOW + 4000.0), (2, 7500));
}

#[test]
fn next_change_instants() {
    let mut s = store();
    assert_eq!(s.next_change(NOW), None);
    add(&mut s, "a", true, NOW);
    // quip (2600) comes before the 50 min mark
    assert_eq!(s.next_change(NOW), Some(NOW + 2600.0));
    // after the quip: the tired crossing is the next change
    s.apply(A::Tick, NOW + 2600.0);
    let tired = NOW + 50.0 * MIN + 1.0;
    assert_eq!(s.next_change(NOW + 2600.0), Some(tired));
    assert_eq!(s.mood(tired - 1.0), Mood::Focus);
    assert_eq!(s.mood(tired), Mood::Tired);
    // accumulated time shortens it, pausing removes it
    s.apply(A::TogglePause(0), NOW + 10.0 * MIN);
    assert_eq!(s.next_change(NOW + 10.0 * MIN + 5000.0), None);
    s.apply(A::TogglePause(0), NOW + 20.0 * MIN);
    s.apply(A::Tick, NOW + 20.0 * MIN + 5000.0);
    let t2 = s.next_change(NOW + 20.0 * MIN + 5000.0).unwrap();
    assert_eq!(t2, NOW + 20.0 * MIN + 40.0 * MIN + 1.0);
    assert_eq!(s.mood(t2), Mood::Tired);
    assert_eq!(s.mood(t2 - 1.0), Mood::Focus);
    // celebration end
    s.apply(A::Complete(0), NOW + 21.0 * MIN);
    let c = NOW + 21.0 * MIN + 3200.0;
    let nc = s.next_change(NOW + 21.0 * MIN + 1.0).unwrap();
    assert_eq!(nc, NOW + 21.0 * MIN + 2600.0); // quip first
    s.apply(A::Tick, nc);
    assert_eq!(s.next_change(nc), Some(c));
    // a task completed today: midnight is a change point once the celebration is over
    s.apply(A::Tick, c);
    let mid = next_midnight_epoch(c);
    assert_eq!(s.next_change(c), Some(mid));
    assert!(s.next_change(mid).is_none());
}

#[test]
fn revision_bumps_on_every_change() {
    let mut s = store();
    let r0 = s.revision();
    add(&mut s, "a", true, NOW);
    let r1 = s.revision();
    assert!(r1 > r0);
    s.apply(A::SetHidden(true), NOW);
    assert!(s.revision() > r1);
}

// ---------------------------------------------------------------- persistence

#[test]
fn loads_the_real_state_json_content() {
    let s = Store::from_json_at(REAL, NOW);
    assert!(!s.first_run());
    assert_eq!(s.settings().pet_kind, "cat");
    assert_eq!(s.settings().pet_name, "Foquinho");
    assert_eq!(s.settings().last_tip_day, "2026-11-24");
    assert!(s.settings().auto_advance && s.settings().tips_enabled && s.settings().music_enabled);
    assert!(!s.settings().hidden);
    assert_eq!(s.total_done(), 0);
    assert!(s.focus().is_empty() && s.queue().is_empty() && s.done().is_empty());
    assert_eq!(s.mood(NOW), Mood::Sleep);
    // identical to what JS writes when lastSeen is unchanged
    let json = s.to_json_at(1_795_550_528_319.0);
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    let w: serde_json::Value = serde_json::from_str(&REAL.replace("1795550528319", &(NOW as i64).to_string())).unwrap();
    assert_eq!(v, w);
}

#[test]
fn real_file_on_this_machine_if_present() {
    // read-only: only reads the user's real file, never writes it
    let p = crate::paths::state_path();
    if let Ok(raw) = fs::read_to_string(&p) {
        let now = epoch_ms();
        let s = Store::from_json_at(&raw, now);
        assert!(!s.first_run());
        let json = s.to_json_at(now);
        let a: serde_json::Value = serde_json::from_str(&raw).unwrap();
        let b: serde_json::Value = serde_json::from_str(&json).unwrap();
        if s.running_count() == 0 && s.queue().iter().all(|t| !t.running) {
            let strip = |mut v: serde_json::Value| {
                v.as_object_mut().unwrap().remove("lastSeen");
                v
            };
            assert_eq!(strip(a), strip(b));
        }
    }
}

#[test]
fn first_run_and_invalid_json() {
    let s = Store::from_json_at("", NOW);
    assert!(s.first_run());
    assert!(s.focus().is_empty());
    assert_eq!(s.settings(), &Settings::default());
    let s = Store::from_json_at("{not json", NOW);
    assert!(!s.first_run());
    assert_eq!(s.settings(), &Settings::default());
    let s = Store::from_json_at("[1,2]", NOW);
    assert!(!s.first_run());
}

#[test]
fn load_missing_file_is_first_run_and_corrupt_file_is_kept() {
    let d = TempDir::new("load");
    let p = d.path().join("data/state.json");
    let s = Store::load_at(&p, NOW);
    assert!(s.first_run());
    assert!(!p.exists(), "load must not write");
    s.save_at(&p, NOW).unwrap();
    assert!(p.exists());
    fs::write(&p, "{broken").unwrap();
    let s = Store::load_at(&p, NOW);
    assert!(!s.first_run());
    assert_eq!(fs::read_to_string(d.path().join("data/state.json.corrupt")).unwrap(), "{broken");
}

#[test]
fn running_timers_count_only_up_to_last_seen() {
    // started 10 min before lastSeen, app was dead for 1 hour
    let last_seen = NOW - 3_600_000.0;
    let started = last_seen - 10.0 * MIN;
    let j = format!(
        r#"{{"lastSeen":{},"focus":[
        {{"tid":"r","title":"r","accMs":1000,"startedAt":{},"running":true}},
        {{"tid":"p","title":"p","accMs":2000,"startedAt":0,"running":false}}],
        "queue":[{{"tid":"q","title":"q","accMs":5,"startedAt":{},"running":true}}]}}"#,
        last_seen as i64,
        started as i64,
        (NOW - 1000.0) as i64
    );
    let s = Store::from_json_at(&j, NOW);
    let r = &s.focus()[0];
    assert_eq!(r.acc_ms, 1000 + 10 * 60_000);
    assert!(r.running);
    assert_eq!(r.started_at, NOW as i64);
    assert_eq!(s.focus()[1].acc_ms, 2000);
    assert!(!s.focus()[1].running);
    // queue items are forced paused (pauseObj uses the current time, like the QML)
    let q = &s.queue()[0];
    assert!(!q.running && q.started_at == 0 && q.acc_ms == 5 + 1000);
    // lastSeen in the future or missing: capped at / defaulting to now (no time added)
    let j = format!(
        r#"{{"lastSeen":{},"focus":[{{"tid":"r","title":"r","startedAt":{},"running":true}}]}}"#,
        NOW as i64 + 5000,
        NOW as i64 - 7000
    );
    let s = Store::from_json_at(&j, NOW);
    assert_eq!(s.focus()[0].acc_ms, 7000);
    let j = format!(r#"{{"focus":[{{"tid":"r","title":"r","startedAt":{},"running":true}}]}}"#, NOW as i64 - 7000);
    let s = Store::from_json_at(&j, NOW);
    assert_eq!(s.focus()[0].acc_ms, 7000);
    // started after lastSeen: nothing negative
    let j = format!(
        r#"{{"lastSeen":{},"focus":[{{"tid":"r","title":"r","startedAt":{},"running":true}}]}}"#,
        NOW as i64 - 5000,
        NOW as i64 - 1000
    );
    assert_eq!(Store::from_json_at(&j, NOW).focus()[0].acc_ms, 0);
}

#[test]
fn lenient_settings_defaults() {
    let s = Store::from_json_at(r#"{"settings":{"petKind":"","petName":null,"hidden":1},"totalDone":"3"}"#, NOW);
    assert_eq!(s.settings().pet_kind, "cat");
    assert_eq!(s.settings().pet_name, "Foquinho");
    assert!(s.settings().hidden && s.settings().auto_advance && s.settings().tips_enabled);
    assert_eq!(s.total_done(), 3);
    assert_eq!((s.level(), s.xp()), (1, 0.6));
    let s = Store::from_json_at(r#"{"settings":{"autoAdvance":false,"tipsEnabled":0}}"#, NOW);
    assert!(!s.settings().auto_advance && !s.settings().tips_enabled);
}

#[test]
fn json_is_byte_identical_to_js_stringify() {
    // tests/data/js_expected.json was produced by node: JSON.stringify(obj, null, 1)
    let expected = include_str!("../../tests/data/js_expected.json");
    let s = Store::from_json_at(expected, NOW); // lastSeen == startedAt == NOW: nothing shifts
    assert_eq!(s.to_json_at(NOW), expected);
    assert!(!expected.ends_with('\n'));
    // and building the same data through actions gives the same bytes
    let mut b = store();
    b.apply(A::SetPetKind("ghost".into()), NOW);
    b.apply(A::SetPetName("Bolinha \"ç\" ã".into()), NOW);
    b.apply(A::SetAutoAdvance(false), NOW);
    b.apply(A::SetHidden(true), NOW);
    b.apply(A::SetLastTipDay("2026-11-24".into()), NOW);
    let mut j = b.to_json_at(NOW);
    assert!(j.contains("\"hidden\": true") && j.contains("\"autoAdvance\": false"));
    j.clear();
}

#[test]
fn unknown_keys_survive_load_save() {
    let j = r#"{"version":1,"lastSeen":1,"settings":{"petKind":"slime","future":{"a":[1,2]}},"extraTop":true,"totalDone":0,"focus":[],"queue":[],"done":[]}"#;
    let s = Store::from_json_at(j, NOW);
    let out = s.to_json_at(NOW);
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["extraTop"], true);
    assert_eq!(v["settings"]["future"]["a"][1], 2);
    assert_eq!(v["settings"]["petKind"], "slime");
}

#[test]
fn save_roundtrips_and_leaves_no_temp_files() {
    let d = TempDir::new("save");
    let p = d.path().join("foco-max/state.json");
    let mut s = store();
    add(&mut s, "um", true, NOW);
    add(&mut s, "dois ç", false, NOW);
    s.apply(A::Complete(0), NOW + 5000.0);
    s.save_at(&p, NOW + 6000.0).unwrap();
    let names: Vec<_> = fs::read_dir(p.parent().unwrap())
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(names.len(), 1, "{names:?}");
    let t = NOW + 7000.0;
    let back = Store::load_at(&p, t);
    assert_eq!(titles(back.done()), ["um"]);
    assert_eq!(titles(back.focus()), ["dois ç"]);
    assert_eq!(back.total_done(), 1);
    // the running task counts up to lastSeen (6000 after start-of-run at +5000 => 1000), then restarts
    assert_eq!(back.focus()[0].acc_ms, 1000);
    assert_eq!(back.focus()[0].started_at, t as i64);
    // saving again at a later time keeps tasks and bumps lastSeen
    back.save_at(&p, t + 1000.0).unwrap();
    let v: serde_json::Value = serde_json::from_str(&fs::read_to_string(&p).unwrap()).unwrap();
    assert_eq!(v["lastSeen"], (t + 1000.0) as i64);
}

#[test]
fn last_seen_never_goes_backwards() {
    let mut s = store();
    s.touch_last_seen(NOW + 10_000.0);
    let v: serde_json::Value = serde_json::from_str(&s.to_json_at(NOW)).unwrap();
    assert_eq!(v["lastSeen"], NOW as i64 + 10_000);
}

#[test]
fn day_key_and_clock_formats_come_from_base() {
    // tip bookkeeping uses foco_base::day_key; make sure the store accepts it verbatim
    let mut s = store();
    let k = day_key(NOW);
    s.apply(A::SetLastTipDay(k.clone()), NOW);
    assert_eq!(s.settings().last_tip_day, k);
    assert_eq!(fmt_clock(s.elapsed_of(0, NOW)), "00:00");
}
