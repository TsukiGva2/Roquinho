//! Time helpers. `Ms` is f64 milliseconds. Monotonic time counts from process start;
//! epoch time is like JS `Date.now()`. Local-time maths (day boundaries) use glibc
//! `localtime_r`/`mktime`, which are already linked by std.

use std::sync::OnceLock;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

/// Milliseconds as float (monotonic since process start, or epoch ms, by context).
pub type Ms = f64;

static START: OnceLock<Instant> = OnceLock::new();

/// Monotonic milliseconds since the first call in this process.
pub fn mono_ms() -> Ms {
    START.get_or_init(Instant::now).elapsed().as_secs_f64() * 1000.0
}

/// Wall clock in ms since the Unix epoch (JS `Date.now()`), fractional.
pub fn epoch_ms() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64() * 1000.0)
        .unwrap_or(0.0)
}

#[repr(C)]
#[derive(Clone, Copy)]
struct Tm {
    tm_sec: i32,
    tm_min: i32,
    tm_hour: i32,
    tm_mday: i32,
    tm_mon: i32,
    tm_year: i32,
    tm_wday: i32,
    tm_yday: i32,
    tm_isdst: i32,
    tm_gmtoff: i64,
    tm_zone: *const u8,
}

extern "C" {
    fn localtime_r(t: *const i64, out: *mut Tm) -> *mut Tm;
    fn mktime(tm: *mut Tm) -> i64;
}

fn local_tm(epoch_ms: f64) -> Option<Tm> {
    let secs = (epoch_ms / 1000.0).floor() as i64;
    let mut tm = Tm {
        tm_sec: 0,
        tm_min: 0,
        tm_hour: 0,
        tm_mday: 0,
        tm_mon: 0,
        tm_year: 0,
        tm_wday: 0,
        tm_yday: 0,
        tm_isdst: 0,
        tm_gmtoff: 0,
        tm_zone: std::ptr::null(),
    };
    // SAFETY: `secs` and `tm` are valid for the call; localtime_r only writes into `tm`.
    let r = unsafe { localtime_r(&secs, &mut tm) };
    if r.is_null() {
        None
    } else {
        Some(tm)
    }
}

fn mk(mut tm: Tm) -> f64 {
    tm.tm_isdst = -1;
    // SAFETY: `tm` is a valid, initialised struct; mktime may normalise it.
    unsafe { mktime(&mut tm) as f64 * 1000.0 }
}

/// Local midnight at or before `epoch_ms` (JS `d.setHours(0,0,0,0)`), as epoch ms.
pub fn start_of_day_epoch(epoch_ms: f64) -> f64 {
    match local_tm(epoch_ms) {
        Some(mut tm) => {
            tm.tm_sec = 0;
            tm.tm_min = 0;
            tm.tm_hour = 0;
            mk(tm)
        }
        None => (epoch_ms / 86_400_000.0).floor() * 86_400_000.0,
    }
}

/// Next local midnight strictly after `epoch_ms`.
pub fn next_midnight_epoch(epoch_ms: f64) -> f64 {
    match local_tm(epoch_ms) {
        Some(mut tm) => {
            tm.tm_sec = 0;
            tm.tm_min = 0;
            tm.tm_hour = 0;
            tm.tm_mday += 1;
            mk(tm)
        }
        None => ((epoch_ms / 86_400_000.0).floor() + 1.0) * 86_400_000.0,
    }
}

/// `Store.dayKey()`: local `YYYY-M-D`, no zero padding (e.g. `2026-1-5`, `2026-11-24`).
/// This is what `settings.lastTipDay` stores.
pub fn day_key(epoch_ms: f64) -> String {
    match local_tm(epoch_ms) {
        Some(tm) => format!("{}-{}-{}", tm.tm_year + 1900, tm.tm_mon + 1, tm.tm_mday),
        None => String::from("1970-1-1"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mono_is_monotonic() {
        let a = mono_ms();
        let b = mono_ms();
        assert!(b >= a && a >= 0.0);
    }

    #[test]
    fn day_boundaries_are_consistent() {
        let now = epoch_ms();
        let sod = start_of_day_epoch(now);
        let nm = next_midnight_epoch(now);
        assert!(sod <= now && now < nm);
        // 23h..25h per local day (DST)
        let len = nm - sod;
        assert!((23.0 * 3.6e6..=25.0 * 3.6e6).contains(&len), "{len}");
        assert_eq!(day_key(sod), day_key(now));
        assert_ne!(day_key(nm), day_key(now));
        assert_eq!(start_of_day_epoch(sod), sod);
        assert_eq!(start_of_day_epoch(nm), nm);
    }

    #[test]
    fn day_key_format_unpadded() {
        let k = day_key(epoch_ms());
        let parts: Vec<_> = k.split('-').collect();
        assert_eq!(parts.len(), 3);
        assert!(parts.iter().all(|p| !p.starts_with('0') && p.parse::<u32>().is_ok()));
    }
}
