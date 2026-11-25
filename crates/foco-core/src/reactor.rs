//! Single-threaded event reactor: one epoll, one `timerfd` (CLOCK_MONOTONIC) armed to the
//! next foco-base deadline, and one `signalfd` (SIGTERM, SIGINT, SIGHUP).
//!
//! rustix has no `signalfd`, so it is declared directly against glibc (already linked by std,
//! like `foco_base::time` does for `localtime_r`). Everything else goes through rustix.

use foco_base::{mono_ms, Ms};
use rustix::event::epoll;
use rustix::fd::{AsFd, BorrowedFd, OwnedFd, RawFd};
use rustix::io::Errno;
use rustix::time::{
    timerfd_create, timerfd_settime, Itimerspec, TimerfdClockId, TimerfdFlags, TimerfdTimerFlags, Timespec,
};
use std::io;

const TIMER_TOKEN: u64 = u64::MAX;
const SIGNAL_TOKEN: u64 = u64::MAX - 1;

/// Signals the reactor turns into [`Ready::Signal`].
pub const SIGNALS: [i32; 3] = [1 /* HUP */, 2 /* INT */, 15 /* TERM */];

/// What `Reactor::wait` reports.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ready {
    /// A registered fd is readable (or hung up); carries the token given to `add_fd`.
    Fd(u64),
    /// The deadline passed to `wait` was reached.
    Timer,
    /// A blocked signal was delivered (`SIGHUP`=1, `SIGINT`=2, `SIGTERM`=15).
    Signal(i32),
}

#[repr(C)]
struct SigSet([u64; 16]); // glibc `sigset_t` is 1024 bits

extern "C" {
    fn sigemptyset(set: *mut SigSet) -> i32;
    fn sigaddset(set: *mut SigSet, signo: i32) -> i32;
    fn pthread_sigmask(how: i32, set: *const SigSet, old: *mut SigSet) -> i32;
    fn signalfd(fd: i32, mask: *const SigSet, flags: i32) -> i32;
}

const SIG_BLOCK: i32 = 0;
const SFD_NONBLOCK: i32 = 0o4000;
const SFD_CLOEXEC: i32 = 0o2000000;

/// Blocks the signals on the calling thread and returns a signalfd for them.
fn make_signalfd() -> io::Result<OwnedFd> {
    let mut set = SigSet([0; 16]);
    // SAFETY: `set` is a valid, writable sigset_t-sized buffer; the signal numbers are valid.
    unsafe {
        sigemptyset(&mut set);
        for s in SIGNALS {
            sigaddset(&mut set, s);
        }
        if pthread_sigmask(SIG_BLOCK, &set, std::ptr::null_mut()) != 0 {
            return Err(io::Error::last_os_error());
        }
        let fd = signalfd(-1, &set, SFD_NONBLOCK | SFD_CLOEXEC);
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(std::os::fd::FromRawFd::from_raw_fd(fd))
    }
}

pub struct Reactor {
    epfd: OwnedFd,
    timer: OwnedFd,
    sigfd: OwnedFd,
    /// Deadline the timerfd is currently armed to (avoids a syscall when unchanged).
    armed: Option<Ms>,
    events: Vec<epoll::Event>,
}

fn io_err(e: Errno) -> io::Error {
    io::Error::from_raw_os_error(e.raw_os_error())
}

impl Reactor {
    pub fn new() -> io::Result<Self> {
        let epfd = epoll::create(epoll::CreateFlags::CLOEXEC).map_err(io_err)?;
        let timer = timerfd_create(TimerfdClockId::Monotonic, TimerfdFlags::CLOEXEC | TimerfdFlags::NONBLOCK)
            .map_err(io_err)?;
        let sigfd = make_signalfd()?;
        epoll::add(&epfd, &timer, epoll::EventData::new_u64(TIMER_TOKEN), epoll::EventFlags::IN)
            .map_err(io_err)?;
        epoll::add(&epfd, &sigfd, epoll::EventData::new_u64(SIGNAL_TOKEN), epoll::EventFlags::IN)
            .map_err(io_err)?;
        Ok(Reactor { epfd, timer, sigfd, armed: None, events: Vec::with_capacity(16) })
    }

    /// Registers `fd` (level-triggered, readable). `token` is returned in `Ready::Fd`.
    pub fn add_fd(&mut self, fd: RawFd, token: u64) -> io::Result<()> {
        if token >= SIGNAL_TOKEN {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "reserved token"));
        }
        // SAFETY: the caller guarantees `fd` is open for the duration of this call.
        let b = unsafe { BorrowedFd::borrow_raw(fd) };
        epoll::add(&self.epfd, b, epoll::EventData::new_u64(token), epoll::EventFlags::IN).map_err(io_err)
    }

    pub fn remove_fd(&mut self, fd: RawFd) {
        // SAFETY: the fd is only used for this ctl call; EBADF/ENOENT are ignored.
        let b = unsafe { BorrowedFd::borrow_raw(fd) };
        let _ = epoll::delete(&self.epfd, b);
    }

    fn arm(&mut self, deadline: Option<Ms>) {
        if self.armed == deadline {
            return;
        }
        let value = match deadline {
            Some(d) => {
                let rel = (d - mono_ms()).max(0.0);
                let ns = ((rel * 1e6) as i64).max(1_000); // at least 1 µs; 0 would disarm
                Timespec { tv_sec: ns / 1_000_000_000, tv_nsec: ns % 1_000_000_000 }
            }
            None => Timespec { tv_sec: 0, tv_nsec: 0 },
        };
        let spec = Itimerspec { it_interval: Timespec { tv_sec: 0, tv_nsec: 0 }, it_value: value };
        if timerfd_settime(&self.timer, TimerfdTimerFlags::empty(), &spec).is_ok() {
            self.armed = deadline;
        } else {
            self.armed = None;
        }
    }

    /// Sleeps until an fd is readable, a signal arrives, or `deadline` (a `mono_ms()` instant)
    /// passes. `None` = no timer. A deadline in the past returns promptly with `Ready::Timer`.
    pub fn wait(&mut self, deadline: Option<Ms>) -> io::Result<Vec<Ready>> {
        self.arm(deadline);
        self.events.clear();
        match epoll::wait(&self.epfd, rustix::buffer::spare_capacity(&mut self.events), None) {
            Ok(_) => {}
            Err(Errno::INTR) => return Ok(Vec::new()),
            Err(e) => return Err(io_err(e)),
        }
        let mut out = Vec::with_capacity(self.events.len());
        for i in 0..self.events.len() {
            let token = self.events[i].data.u64();
            match token {
                TIMER_TOKEN => {
                    let mut buf = [0u8; 8];
                    let _ = rustix::io::read(self.timer.as_fd(), &mut buf);
                    self.armed = None;
                    out.push(Ready::Timer);
                }
                SIGNAL_TOKEN => {
                    let mut buf = [0u8; 128]; // struct signalfd_siginfo
                    while let Ok(n) = rustix::io::read(self.sigfd.as_fd(), &mut buf) {
                        if n < 4 {
                            break;
                        }
                        let signo = u32::from_ne_bytes([buf[0], buf[1], buf[2], buf[3]]);
                        out.push(Ready::Signal(signo as i32));
                    }
                }
                t => out.push(Ready::Fd(t)),
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::os::fd::AsRawFd;
    use std::os::unix::net::UnixStream;
    use std::time::{Duration, Instant};

    extern "C" {
        fn raise(sig: i32) -> i32;
    }

    #[test]
    fn timer_fires_at_deadline() {
        let mut r = Reactor::new().unwrap();
        let t0 = Instant::now();
        let ev = r.wait(Some(mono_ms() + 30.0)).unwrap();
        let dt = t0.elapsed();
        assert_eq!(ev, vec![Ready::Timer]);
        assert!(dt >= Duration::from_millis(28) && dt < Duration::from_millis(80), "{dt:?}");
    }

    #[test]
    fn past_deadline_returns_immediately() {
        let mut r = Reactor::new().unwrap();
        let t0 = Instant::now();
        assert_eq!(r.wait(Some(mono_ms() - 500.0)).unwrap(), vec![Ready::Timer]);
        assert!(t0.elapsed() < Duration::from_millis(20));
        // and again with the same (now stale) deadline value path
        assert_eq!(r.wait(Some(mono_ms())).unwrap(), vec![Ready::Timer]);
    }

    #[test]
    fn fd_readiness_and_removal() {
        let mut r = Reactor::new().unwrap();
        let (a, mut b) = UnixStream::pair().unwrap();
        r.add_fd(a.as_raw_fd(), 7).unwrap();
        assert!(r.add_fd(a.as_raw_fd(), u64::MAX).is_err());
        b.write_all(b"x").unwrap();
        let ev = r.wait(Some(mono_ms() + 1000.0)).unwrap();
        assert_eq!(ev, vec![Ready::Fd(7)]);
        r.remove_fd(a.as_raw_fd());
        let ev = r.wait(Some(mono_ms() + 20.0)).unwrap();
        assert_eq!(ev, vec![Ready::Timer]);
    }

    #[test]
    fn disarm_when_no_deadline() {
        let mut r = Reactor::new().unwrap();
        let (a, mut b) = UnixStream::pair().unwrap();
        r.add_fd(a.as_raw_fd(), 1).unwrap();
        // arm, then wait with None: the stale timer must not fire; the fd wakes us.
        r.arm(Some(mono_ms() + 10.0));
        let th = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(60));
            b.write_all(b"y").unwrap();
            b
        });
        let ev = r.wait(None).unwrap();
        assert_eq!(ev, vec![Ready::Fd(1)]);
        let _ = th.join();
    }

    #[test]
    fn signal_becomes_event() {
        let mut r = Reactor::new().unwrap();
        // SAFETY: raise() targets the calling thread, where the signal is blocked.
        unsafe { raise(1) };
        let ev = r.wait(Some(mono_ms() + 1000.0)).unwrap();
        assert_eq!(ev, vec![Ready::Signal(1)]);
    }
}
