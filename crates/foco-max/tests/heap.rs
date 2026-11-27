//! Heap budget of the real `App` (dhat). Must stay the only test in this file: dhat counts every allocation of the
//! process, so a second test running on another thread would be counted too.

mod common;

use common::{Driver, TASKS};
use foco_base::AppCmd;

#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

/// Open/close cycles between the idle stretches.
const CYCLES: u32 = 8;
/// Measured: 10.25 MB peak live heap (flat from the 6th to the 10th cycle), including the driver's 2.5 MB window
/// buffer (shared memory in the real app, whose heaptrack peak is 9.7 MB). The sprite (4 MB) and glyph (2 MB) caps are
/// guarded by the foco-gfx cache tests: a run this short does not reach them.
const PEAK_BYTES: usize = 13_000_000;
/// Measured: 5.4 MB live at the end, after the panel closed and the renderer scratch was released.
const END_BYTES: usize = 7_000_000;

#[test]
fn heap_stays_within_budget() {
    let _profiler = dhat::Profiler::builder().testing().build();
    let mut d = Driver::new(TASKS);
    d.run_for(20_000.0);
    for _ in 0..CYCLES {
        d.cmd(AppCmd::Open);
        d.run_for(2_000.0);
        d.cmd(AppCmd::Collapse);
        d.run_for(2_000.0);
    }
    d.run_for(20_000.0);
    let s = dhat::HeapStats::get();
    println!("heap: peak {} B, end {} B, {} allocations", s.max_bytes, s.curr_bytes, s.total_blocks);
    assert!(s.max_bytes <= PEAK_BYTES, "peak heap {} B", s.max_bytes);
    assert!(s.curr_bytes <= END_BYTES, "heap at the end {} B", s.curr_bytes);
}
