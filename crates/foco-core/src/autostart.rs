//! Autostart (`Backend::autostart/setAutostart` + `X11Platform::autostartExec/autostartChanged`):
//! `~/.config/autostart/foco-max.desktop` (without the KDE-only keys) and the line marked
//! `# foquinho` in `~/.config/openbox/autostart`.

use crate::{fsutil, paths};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const MARKER: &str = "# foquinho";

pub fn desktop_path() -> PathBuf {
    paths::config_dir().join("autostart/foco-max.desktop")
}

/// `Backend::autostart()`: the `.desktop` file exists.
pub fn enabled() -> bool {
    desktop_path().exists()
}

/// `Backend::setAutostart`: enables or disables both entries for the binary at `exe`.
///
/// Unlike the C++ (which returns early when the `.desktop` already exists), enabling is
/// idempotent and re-syncs both entries, so a stale path (e.g. `/usr/bin/foco-max`) gets fixed.
pub fn set(enabled: bool, exe: &Path) -> io::Result<()> {
    set_in(&paths::config_dir(), enabled, exe)
}

/// `autostartExec`: `<exe> start`, the binary quoted when it contains a space.
pub fn exec_line(exe: &Path) -> String {
    let b = exe.to_string_lossy();
    if b.contains(' ') {
        format!("\"{b}\" start")
    } else {
        format!("{b} start")
    }
}

fn desktop_content(exe: &Path) -> String {
    format!(
        "[Desktop Entry]\nType=Application\nName=Foquinho\nComment=Notch de foco\nExec={}\nIcon=foco-max\n",
        exec_line(exe)
    )
}

/// The Openbox autostart line: waits a bit so a running XDG autostart gets there first.
pub fn openbox_line(exe: &Path) -> String {
    format!("(sleep 3; \"{}\" start) &  {MARKER}", exe.to_string_lossy())
}

pub(crate) fn set_in(config: &Path, enabled: bool, exe: &Path) -> io::Result<()> {
    let desktop = config.join("autostart/foco-max.desktop");
    let mut first_err = None;
    if enabled {
        let want = desktop_content(exe);
        if fs::read_to_string(&desktop).ok().as_deref() != Some(want.as_str()) {
            if let Err(e) = fsutil::atomic_write(&desktop, want.as_bytes()) {
                first_err = Some(e);
            }
        }
    } else if let Err(e) = fs::remove_file(&desktop) {
        if e.kind() != io::ErrorKind::NotFound {
            first_err = Some(e);
        }
    }
    if let Err(e) = update_openbox(&config.join("openbox"), enabled, exe) {
        first_err.get_or_insert(e);
    }
    first_err.map_or(Ok(()), Err)
}

/// `autostartChanged`: only touches an existing `openbox` config directory.
fn update_openbox(dir: &Path, enabled: bool, exe: &Path) -> io::Result<()> {
    if !dir.is_dir() {
        return Ok(());
    }
    let path = dir.join("autostart");
    if !enabled && !path.exists() {
        return Ok(());
    }
    let old = fs::read(&path).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default();
    let mut lines: Vec<&str> = old.split('\n').collect();
    while lines.last().is_some_and(|l| l.is_empty()) {
        lines.pop();
    }
    lines.retain(|l| !l.contains(MARKER));
    let line = openbox_line(exe);
    if enabled {
        lines.push(&line);
    }
    let new = lines.join("\n") + "\n";
    if new == old {
        return Ok(());
    }
    fsutil::atomic_write(&path, new.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fsutil::testutil::TempDir;

    const EXE: &str = "/usr/bin/foco-max";

    #[test]
    fn formats() {
        assert_eq!(exec_line(Path::new(EXE)), "/usr/bin/foco-max start");
        assert_eq!(exec_line(Path::new("/my dir/foco-max")), "\"/my dir/foco-max\" start");
        assert_eq!(openbox_line(Path::new(EXE)), "(sleep 3; \"/usr/bin/foco-max\" start) &  # foquinho");
        let d = desktop_content(Path::new(EXE));
        assert_eq!(
            d,
            "[Desktop Entry]\nType=Application\nName=Foquinho\nComment=Notch de foco\nExec=/usr/bin/foco-max start\nIcon=foco-max\n"
        );
        assert!(!d.contains("X-KDE"));
    }

    #[test]
    fn enable_disable_with_openbox() {
        let t = TempDir::new("as");
        let cfg = t.path();
        fs::create_dir_all(cfg.join("openbox")).unwrap();
        fs::write(cfg.join("openbox/autostart"), "picom &\n(sleep 3; \"/old\" start) &  # foquinho\nnitrogen --restore &\n\n\n")
            .unwrap();
        set_in(cfg, true, Path::new(EXE)).unwrap();
        assert_eq!(
            fs::read_to_string(cfg.join("openbox/autostart")).unwrap(),
            "picom &\nnitrogen --restore &\n(sleep 3; \"/usr/bin/foco-max\" start) &  # foquinho\n"
        );
        let d = fs::read_to_string(cfg.join("autostart/foco-max.desktop")).unwrap();
        assert!(d.contains("Exec=/usr/bin/foco-max start\n"));
        // idempotent, and re-syncs a stale exe
        set_in(cfg, true, Path::new("/home/u/.local/bin/foco-max")).unwrap();
        let d = fs::read_to_string(cfg.join("autostart/foco-max.desktop")).unwrap();
        assert!(d.contains("Exec=/home/u/.local/bin/foco-max start\n"));
        let ob = fs::read_to_string(cfg.join("openbox/autostart")).unwrap();
        assert_eq!(ob.matches("# foquinho").count(), 1);
        assert!(ob.contains("\"/home/u/.local/bin/foco-max\" start"));
        set_in(cfg, false, Path::new(EXE)).unwrap();
        assert!(!cfg.join("autostart/foco-max.desktop").exists());
        assert_eq!(fs::read_to_string(cfg.join("openbox/autostart")).unwrap(), "picom &\nnitrogen --restore &\n");
        set_in(cfg, false, Path::new(EXE)).unwrap(); // already off: fine
    }

    #[test]
    fn no_openbox_dir_is_skipped_and_perms_kept() {
        use std::os::unix::fs::PermissionsExt;
        let t = TempDir::new("as2");
        set_in(t.path(), true, Path::new(EXE)).unwrap();
        assert!(t.path().join("autostart/foco-max.desktop").exists());
        assert!(!t.path().join("openbox").exists());
        fs::create_dir_all(t.path().join("openbox")).unwrap();
        let p = t.path().join("openbox/autostart");
        fs::write(&p, "x &\n").unwrap();
        fs::set_permissions(&p, fs::Permissions::from_mode(0o755)).unwrap();
        set_in(t.path(), true, Path::new(EXE)).unwrap();
        assert_eq!(fs::metadata(&p).unwrap().permissions().mode() & 0o777, 0o755);
    }

    #[test]
    fn symlinked_autostart_is_written_through() {
        let t = TempDir::new("as3");
        fs::create_dir_all(t.path().join("openbox")).unwrap();
        fs::write(t.path().join("real"), "a &\n").unwrap();
        std::os::unix::fs::symlink(t.path().join("real"), t.path().join("openbox/autostart")).unwrap();
        set_in(t.path(), true, Path::new(EXE)).unwrap();
        assert!(fs::symlink_metadata(t.path().join("openbox/autostart")).unwrap().file_type().is_symlink());
        assert!(fs::read_to_string(t.path().join("real")).unwrap().contains("# foquinho"));
    }
}
