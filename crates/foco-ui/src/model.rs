//! Derived-state helpers over the store (`Store.qml` derived properties and deadlines).

use crate::cx::StoreRead;
use foco_base::{start_of_day_epoch, Mood, Ms, Task};

/// Slack added to computed tick deadlines so the wake lands after the second flips.
const TICK_SLACK_MS: Ms = 2.0;

/// `Store.petMood`.
pub fn mood(store: &dyn StoreRead, epoch: f64) -> Mood {
    Mood::derive(&store.state().focus, epoch, store.celebrate_until())
}

/// `Store.today`: (completed today, ms focused today = spent of today's done + running focus).
pub fn today(store: &dyn StoreRead, epoch: f64) -> (u32, i64) {
    let st = store.state();
    let sod = start_of_day_epoch(epoch) as i64;
    let mut count = 0u32;
    let mut ms = 0i64;
    for t in &st.done {
        if t.done_at < sod {
            break;
        }
        count += 1;
        ms += t.spent_ms;
    }
    for t in &st.focus {
        ms += t.elapsed(epoch);
    }
    (count, ms)
}

/// Completed today.
pub fn done_today(store: &dyn StoreRead, epoch: f64) -> usize {
    let sod = start_of_day_epoch(epoch) as i64;
    store.state().done.iter().take_while(|t| t.done_at >= sod).count()
}

/// `Store.petLine` without the music branch. `quip` wins when set.
pub fn pet_line(mood: Mood, n_focus: usize, n_queue: usize, quip: Option<&str>) -> String {
    if let Some(q) = quip {
        if !q.is_empty() {
            return q.to_string();
        }
    }
    match mood {
        Mood::Sleep => {
            if n_queue > 0 {
                format!("Zzz… {n_queue} na fila. Não vão se fazer sozinhas.")
            } else {
                "Zzz… no future e nenhuma demanda.".to_string()
            }
        }
        Mood::Idle => "Pausado. Intervalo do show.".to_string(),
        Mood::Happy => "Mais uma pro setlist!".to_string(),
        Mood::Tired => "Música do Ramones dura 2 min. Você tá em 50.".to_string(),
        Mood::Focus => {
            if n_focus > 1 {
                format!("{n_focus} em foco. Isso é caos, não multitarefa.")
            } else {
                "Uma por vez. Três acordes bastam.".to_string()
            }
        }
    }
}

/// Monotonic time at which `floor(elapsed / period)` of a running `task` next changes
/// (`None` when the task is paused). `period` in ms.
pub fn next_tick(task: &Task, epoch: f64, now: Ms, period: Ms) -> Option<Ms> {
    if !task.running || period <= 0.0 {
        return None;
    }
    let elapsed = task.elapsed(epoch) as f64;
    let next = ((elapsed / period).floor() + 1.0) * period;
    Some(now + (next - elapsed) + TICK_SLACK_MS)
}

/// Monotonic time at which the minute shown by `fmt_short(today.ms)` next changes while
/// something is running (`None` when nothing runs: the value is static until a store change).
pub fn next_today_minute(store: &dyn StoreRead, epoch: f64, now: Ms) -> Option<Ms> {
    let running = store.state().focus.iter().filter(|t| t.running).count();
    if running == 0 {
        return None;
    }
    let (_, ms) = today(store, epoch);
    let ms = ms as f64;
    let next = ((ms / 60_000.0).floor() + 1.0) * 60_000.0;
    Some(now + (next - ms) / running as f64 + TICK_SLACK_MS)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::FakeStore;

    fn task(running: bool, acc: i64, started: i64) -> Task {
        Task { tid: "t".into(), title: "x".into(), acc_ms: acc, started_at: started, running, ..Task::default() }
    }

    #[test]
    fn pet_lines() {
        assert_eq!(pet_line(Mood::Sleep, 0, 0, None), "Zzz… no future e nenhuma demanda.");
        assert_eq!(pet_line(Mood::Sleep, 0, 3, None), "Zzz… 3 na fila. Não vão se fazer sozinhas.");
        assert_eq!(pet_line(Mood::Focus, 2, 0, None), "2 em foco. Isso é caos, não multitarefa.");
        assert_eq!(pet_line(Mood::Focus, 1, 0, None), "Uma por vez. Três acordes bastam.");
        assert_eq!(pet_line(Mood::Happy, 1, 0, Some("Oi")), "Oi");
        assert_eq!(pet_line(Mood::Happy, 1, 0, Some("")), "Mais uma pro setlist!");
    }

    #[test]
    fn next_tick_second_boundary() {
        let t = task(true, 0, 1_000_000);
        // elapsed 1250 ms -> next whole second in 750 ms
        let n = next_tick(&t, 1_001_250.0, 500.0, 1000.0).unwrap();
        assert!((n - (500.0 + 750.0 + TICK_SLACK_MS)).abs() < 1e-9);
        assert_eq!(next_tick(&task(false, 5, 0), 10.0, 0.0, 1000.0), None);
        // exactly on a boundary moves to the next one
        let n = next_tick(&t, 1_002_000.0, 0.0, 1000.0).unwrap();
        assert!((n - (1000.0 + TICK_SLACK_MS)).abs() < 1e-9);
    }

    #[test]
    fn today_and_minute() {
        let mut st = FakeStore::default();
        let now = foco_base::epoch_ms();
        st.state.focus.push(task(true, 30_000, now as i64 - 15_000));
        let mut d = task(false, 0, 0);
        d.done_at = now as i64 - 1000;
        d.spent_ms = 60_000;
        st.state.done.push(d.clone());
        d.done_at = 1;
        st.state.done.push(d); // ancient: stops the scan
        let (c, ms) = today(&st, now);
        assert_eq!(c, 1);
        assert!((ms - (60_000 + 45_000)).abs() < 50, "{ms}");
        assert_eq!(done_today(&st, now), 1);
        let n = next_today_minute(&st, now, 0.0).unwrap();
        // 105 s -> next minute boundary at 120 s: 15 s away
        assert!((n - (15_000.0 + TICK_SLACK_MS)).abs() < 100.0, "{n}");
        st.state.focus[0].running = false;
        assert_eq!(next_today_minute(&st, now, 0.0), None);
    }
}
