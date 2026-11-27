//! Headless scenario driver shared by the budget tests and benches: the real `App` under a fake clock.
#![expect(dead_code, reason = "each test crate uses a different part of the driver")]

use foco_base::{AppCmd, IRect, Ms};
use foco_core::Rc;
use foco_core::Store;
use foco_max::app::{App, AppInit, PlatformReq};
use foco_ui::WIN_DEV;
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// Fixture without a running timer and without tips.
pub const TASKS: &str = include_str!("../data/tasks.json");
/// Same tasks, tips on (the tip bubble pops 3 s after start).
pub const TASKS_TIPS: &str = include_str!("../data/tasks_tips.json");
/// Wall clock at simulated `now = 0` (the fixtures' `lastSeen`), so `epoch = EPOCH0 + now`. 12:00 UTC, so no local
/// midnight timer fires within a scenario in zones from UTC-12 to UTC+11.
pub const EPOCH0: f64 = 1_795_521_600_000.0;

/// What the loop did since the last [`Driver::take_stats`].
#[derive(Default)]
pub struct Stats {
    /// Loop iterations that ran `tick` (timer wakes plus commands).
    pub wakes: u32,
    /// Wakes that rendered.
    pub frames: u32,
    /// Damaged device px of each rendered frame (sum of rect areas).
    pub px: Vec<i64>,
    /// Wall time spent in `tick` + `render`.
    pub busy: Duration,
    /// The slowest single wake.
    pub worst: Duration,
}

impl Stats {
    /// Largest damage of one frame.
    pub fn max_px(&self) -> i64 {
        self.px.iter().copied().max().unwrap_or(0)
    }
}

/// An `App` built like `--snapshot` does, run by a loop that mirrors `main.rs` under a fake clock.
pub struct Driver {
    app: App,
    buf: Vec<u8>,
    now: Ms,
    stats: Stats,
}

impl Driver {
    /// Starts the app at `now = 0` from a state JSON (see [`TASKS`]); nothing has run yet.
    pub fn new(state: &str) -> Driver {
        let app = App::new(
            AppInit {
                store: Store::from_json_at(state, EPOCH0),
                rc: Rc::empty("/nonexistent/foco-maxrc"),
                persist: false,
                state_path: PathBuf::from("/nonexistent/state.json"),
                exe: PathBuf::from("foco-max"),
                monitors: Vec::new(),
                autostart: false,
                initial_cmd: None,
            },
            0.0,
            EPOCH0,
        );
        Driver { app, buf: vec![0; (WIN_DEV * WIN_DEV * 4) as usize], now: 0.0, stats: Stats::default() }
    }

    /// Simulated monotonic time, ms.
    pub fn now(&self) -> Ms {
        self.now
    }

    /// Returns what happened since the last call and starts counting afresh.
    pub fn take_stats(&mut self) -> Stats {
        std::mem::take(&mut self.stats)
    }

    /// Delivers a command (an IPC line in the real loop) and runs its frame.
    pub fn cmd(&mut self, cmd: AppCmd) {
        self.app.on_cmd(cmd, self.now, EPOCH0 + self.now);
        self.step();
    }

    /// Runs the loop for `ms` of simulated time: sleeps to each deadline, an idle app jumps to the end.
    pub fn run_for(&mut self, ms: Ms) {
        let end = self.now + ms;
        while let Some(d) = self.app.next_deadline(self.now, EPOCH0 + self.now) {
            if d > end {
                break;
            }
            self.now = d;
            self.step();
        }
        self.now = end;
    }

    /// One woken iteration of `main.rs`'s loop: `tick`, platform requests, then render if damaged
    /// (each render counts as presented at once, so there is no present pacing).
    fn step(&mut self) {
        let (now, epoch) = (self.now, EPOCH0 + self.now);
        let t0 = Instant::now();
        self.app.tick(now, epoch);
        loop {
            let reqs = self.app.take_requests();
            if reqs.is_empty() {
                break;
            }
            if reqs.contains(&PlatformReq::RefreshMonitors) {
                self.app.set_monitors(Vec::new(), now);
                self.app.tick(now, epoch);
            }
        }
        self.stats.wakes += 1;
        if self.app.has_damage() {
            let rects = self.app.take_damage();
            self.app.render(&mut self.buf, &rects, now, epoch);
            self.stats.frames += 1;
            self.stats.px.push(rects.iter().map(IRect::area).sum());
        }
        let took = t0.elapsed();
        self.stats.busy += took;
        self.stats.worst = self.stats.worst.max(took);
    }
}
