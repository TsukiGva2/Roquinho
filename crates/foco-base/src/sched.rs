//! One-shot deadline scheduler (one timerfd serves all of them).

use crate::time::Ms;
use std::cmp::Ordering;
use std::collections::{BTreeSet, HashMap};

/// Identifies a timer: a static name plus an instance number (e.g. `TimerKey("quip", 0)`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TimerKey(pub &'static str, pub u32);

#[derive(Clone, Copy, Debug, PartialEq)]
struct OrdMs(Ms);
impl Eq for OrdMs {}
impl PartialOrd for OrdMs {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}
impl Ord for OrdMs {
    fn cmp(&self, o: &Self) -> Ordering {
        self.0.total_cmp(&o.0)
    }
}

/// Keyed one-shot timers. Scheduling an existing key replaces its deadline.
/// Ties fire in scheduling order.
#[derive(Debug, Default)]
pub struct Scheduler {
    order: BTreeSet<(OrdMs, u64, TimerKey)>,
    by_key: HashMap<TimerKey, (OrdMs, u64)>,
    seq: u64,
}

impl Scheduler {
    pub fn new() -> Scheduler {
        Scheduler::default()
    }
    /// Sets (or replaces) the deadline for `key`.
    pub fn schedule(&mut self, key: TimerKey, at: Ms) {
        self.cancel(key);
        self.seq += 1;
        let at = OrdMs(at);
        self.order.insert((at, self.seq, key));
        self.by_key.insert(key, (at, self.seq));
    }
    /// Removes `key`; returns whether it was scheduled.
    pub fn cancel(&mut self, key: TimerKey) -> bool {
        match self.by_key.remove(&key) {
            Some((at, seq)) => {
                self.order.remove(&(at, seq, key));
                true
            }
            None => false,
        }
    }
    pub fn is_scheduled(&self, key: TimerKey) -> bool {
        self.by_key.contains_key(&key)
    }
    /// The deadline of `key`, if scheduled.
    pub fn deadline_of(&self, key: TimerKey) -> Option<Ms> {
        self.by_key.get(&key).map(|(a, _)| a.0)
    }
    /// Earliest pending deadline.
    pub fn next_deadline(&self) -> Option<Ms> {
        self.order.first().map(|(a, _, _)| a.0)
    }
    /// Removes and returns every key with deadline `<= now`, earliest first.
    pub fn pop_due(&mut self, now: Ms) -> Vec<TimerKey> {
        let mut out = Vec::new();
        while let Some(&(at, seq, key)) = self.order.first() {
            if at.0 > now {
                break;
            }
            self.order.remove(&(at, seq, key));
            self.by_key.remove(&key);
            out.push(key);
        }
        out
    }
    pub fn is_empty(&self) -> bool {
        self.by_key.is_empty()
    }
    pub fn len(&self) -> usize {
        self.by_key.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: TimerKey = TimerKey("a", 0);
    const B: TimerKey = TimerKey("b", 0);
    const C: TimerKey = TimerKey("c", 1);

    #[test]
    fn ordering_and_pop() {
        let mut s = Scheduler::new();
        assert_eq!(s.next_deadline(), None);
        s.schedule(B, 20.0);
        s.schedule(A, 10.0);
        s.schedule(C, 30.0);
        assert_eq!(s.next_deadline(), Some(10.0));
        assert_eq!(s.pop_due(5.0), vec![]);
        assert_eq!(s.pop_due(25.0), vec![A, B]);
        assert!(!s.is_scheduled(A));
        assert!(s.is_scheduled(C));
        assert_eq!(s.pop_due(100.0), vec![C]);
        assert!(s.is_empty());
    }

    #[test]
    fn replace_and_cancel() {
        let mut s = Scheduler::new();
        s.schedule(A, 10.0);
        s.schedule(A, 50.0);
        assert_eq!(s.len(), 1);
        assert_eq!(s.next_deadline(), Some(50.0));
        assert_eq!(s.deadline_of(A), Some(50.0));
        assert!(s.pop_due(20.0).is_empty());
        assert!(s.cancel(A));
        assert!(!s.cancel(A));
        assert_eq!(s.next_deadline(), None);
        assert!(s.pop_due(1e9).is_empty());
    }

    #[test]
    fn ties_in_schedule_order() {
        let mut s = Scheduler::new();
        s.schedule(C, 10.0);
        s.schedule(A, 10.0);
        assert_eq!(s.pop_due(10.0), vec![C, A]);
    }

    #[test]
    fn same_name_different_instance() {
        let mut s = Scheduler::new();
        s.schedule(TimerKey("t", 0), 1.0);
        s.schedule(TimerKey("t", 1), 2.0);
        assert_eq!(s.len(), 2);
    }
}
