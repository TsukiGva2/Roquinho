//! Single-instance IPC over an abstract unix socket `\0foco-max-$USER`.
//!
//! The client path ([`send_to_running`]) is a handful of raw syscalls (socket, connect, send,
//! close) with every call non-blocking, so it takes microseconds and can never hang.

use rustix::net::{self, AddressFamily, SendFlags, SocketAddrUnix, SocketFlags, SocketType};
use std::io::{self, Read};
use std::os::fd::{AsRawFd, RawFd};
use std::os::linux::net::SocketAddrExt;
use std::os::unix::net::{SocketAddr, UnixListener};
use std::time::{Duration, Instant};

/// Abstract socket name (without the leading NUL): `foco-max-$USER`.
pub fn socket_name() -> String {
    format!(
        "foco-max-{}",
        std::env::var("USER")
            .ok()
            .filter(|u| !u.is_empty())
            .unwrap_or_else(|| "user".into())
    )
}

/// Connects to the running instance, writes `line` + `'\n'` and closes. `false` if there is no
/// server (or it is not accepting right now).
pub fn send_to_running(line: &str) -> bool {
    send_to(&socket_name(), line)
}

/// Like [`send_to_running`] for an explicit abstract socket name.
pub fn send_to(name: &str, line: &str) -> bool {
    let Ok(addr) = SocketAddrUnix::new_abstract_name(name.as_bytes()) else { return false };
    let Ok(fd) =
        net::socket_with(AddressFamily::UNIX, SocketType::STREAM, SocketFlags::CLOEXEC | SocketFlags::NONBLOCK, None)
    else {
        return false;
    };
    let mut tries = 0;
    loop {
        match net::connect(&fd, &addr) {
            Ok(()) => break,
            // listener backlog momentarily full: retry a few times, at most ~5 ms in total
            Err(rustix::io::Errno::AGAIN) if tries < 5 => {
                tries += 1;
                std::thread::sleep(Duration::from_millis(1));
            }
            Err(_) => return false,
        }
    }
    let mut buf = Vec::with_capacity(line.len() + 1);
    buf.extend_from_slice(line.as_bytes());
    buf.push(b'\n');
    let mut sent = 0;
    while sent < buf.len() {
        match net::send(&fd, &buf[sent..], SendFlags::NOSIGNAL) {
            Ok(n) if n > 0 => sent += n,
            _ => return false,
        }
    }
    true
}

/// The listening side. Register [`Server::fd`] with the reactor and call [`Server::drain`]
/// when it becomes readable.
pub struct Server {
    listener: UnixListener,
}

impl Server {
    /// Binds the per-user socket. `AddrInUse` means another instance is running.
    ///
    /// # Errors
    /// Returns the socket error, including `AddrInUse` when the name is taken.
    pub fn bind() -> io::Result<Server> {
        Server::bind_named(&socket_name())
    }

    /// Binds an explicit abstract name (tests).
    ///
    /// # Errors
    /// Same as [`Server::bind`].
    pub fn bind_named(name: &str) -> io::Result<Server> {
        let addr = SocketAddr::from_abstract_name(name.as_bytes())?;
        let listener = UnixListener::bind_addr(&addr)?;
        listener.set_nonblocking(true)?;
        Ok(Server { listener })
    }

    pub fn fd(&self) -> RawFd {
        self.listener.as_raw_fd()
    }

    /// Accepts every pending connection and returns the trimmed, non-empty lines they sent.
    pub fn drain(&mut self) -> Vec<String> {
        let mut lines = Vec::new();
        loop {
            match self.listener.accept() {
                Ok((mut conn, _)) => read_conn(&mut conn, &mut lines),
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {} // retry
                Err(_) => break,
            }
        }
        lines
    }
}

/// Reads one connection until EOF. A client that connected but has not written yet is
/// waited for briefly (it writes right after connecting), never for long.
fn read_conn(conn: &mut std::os::unix::net::UnixStream, lines: &mut Vec<String>) {
    const MAX: usize = 64 * 1024;
    let start = Instant::now();
    let budget = Duration::from_millis(50);
    let _ = conn.set_read_timeout(Some(Duration::from_millis(20)));
    let mut data = Vec::new();
    let mut chunk = [0u8; 4096];
    while data.len() < MAX && start.elapsed() < budget {
        match conn.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => data.extend_from_slice(&chunk[..n]),
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {} // retry
            Err(_) => break,
        }
    }
    for l in String::from_utf8_lossy(&data).split('\n') {
        let l = l.trim();
        if !l.is_empty() {
            lines.push(l.to_string());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    static N: AtomicU32 = AtomicU32::new(0);

    fn uniq() -> String {
        format!("foco-core-test-{}-{}", std::process::id(), N.fetch_add(1, Ordering::Relaxed))
    }

    #[test]
    fn no_server_means_false() {
        assert!(!send_to(&uniq(), "toggle"));
    }

    #[test]
    fn lines_roundtrip() {
        let name = uniq();
        let mut s = Server::bind_named(&name).unwrap();
        assert!(s.drain().is_empty());
        assert!(send_to(&name, "add comprar pão"));
        assert!(send_to(&name, "  done  "));
        assert!(send_to(&name, ""));
        assert!(send_to(&name, "focus ação ç"));
        assert_eq!(s.drain(), vec!["add comprar pão", "done", "focus ação ç"]);
        assert!(s.drain().is_empty());
    }

    #[test]
    fn second_bind_fails_and_name_is_freed_on_drop() {
        let name = uniq();
        let s = Server::bind_named(&name).unwrap();
        assert_eq!(Server::bind_named(&name).err().map(|e| e.kind()), Some(io::ErrorKind::AddrInUse));
        drop(s);
        assert!(!send_to(&name, "x"));
        assert!(Server::bind_named(&name).is_ok());
    }

    #[test]
    fn multiple_lines_in_one_connection() {
        use std::io::Write;
        use std::os::unix::net::UnixStream;
        let name = uniq();
        let mut s = Server::bind_named(&name).unwrap();
        let addr = SocketAddr::from_abstract_name(name.as_bytes()).unwrap();
        let mut c = UnixStream::connect_addr(&addr).unwrap();
        c.write_all(b"a 1\nb 2\nc").unwrap();
        drop(c);
        assert_eq!(s.drain(), vec!["a 1", "b 2", "c"]);
    }

    #[test]
    fn works_with_the_reactor() {
        use crate::reactor::{Reactor, Ready};
        let name = uniq();
        let mut s = Server::bind_named(&name).unwrap();
        let mut r = Reactor::new().unwrap();
        r.add_fd(s.fd(), 3).unwrap();
        let n2 = name.clone();
        let t = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(20));
            assert!(send_to(&n2, "pause"));
        });
        let ev = r.wait(Some(foco_base::mono_ms() + 2000.0)).unwrap();
        assert_eq!(ev, vec![Ready::Fd(3)]);
        assert_eq!(s.drain(), vec!["pause"]);
        t.join().unwrap();
    }

    #[test]
    fn client_latency() {
        let name = uniq();
        let mut s = Server::bind_named(&name).unwrap();
        let rounds = 20;
        let per = 100;
        let mut all = Vec::new();
        for _ in 0..rounds {
            for _ in 0..per {
                let t0 = Instant::now();
                assert!(send_to(&name, "add x"));
                all.push(t0.elapsed().as_nanos() as f64 / 1000.0);
            }
            assert_eq!(s.drain().len(), per);
        }
        all.sort_by(f64::total_cmp);
        let avg = all.iter().sum::<f64>() / all.len() as f64;
        println!(
            "ipc client send latency over {} sends: min {:.1} us, median {:.1} us, avg {:.1} us, p99 {:.1} us, max {:.1} us",
            all.len(),
            all[0],
            all[all.len() / 2],
            avg,
            all[all.len() * 99 / 100],
            all[all.len() - 1]
        );
        assert!(all[all.len() / 2] < 1000.0);
    }
}
