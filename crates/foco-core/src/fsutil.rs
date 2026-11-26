//! Small filesystem helpers shared by the store, rc and autostart code.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

static SEQ: AtomicU32 = AtomicU32::new(0);

/// Atomic write like `QSaveFile`: creates parent directories, follows a symlinked target,
/// writes a temp file next to it and renames it over the target (keeping the old permissions).
pub fn atomic_write(path: &Path, data: &[u8]) -> io::Result<()> {
    let target: PathBuf = match fs::canonicalize(path) {
        Ok(p) => p,
        Err(_) => path.to_path_buf(),
    };
    if let Some(dir) = target.parent() {
        if !dir.as_os_str().is_empty() {
            fs::create_dir_all(dir)?;
        }
    }
    let name = target.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let tmp =
        target.with_file_name(format!(".{}.{}.{}.tmp", name, std::process::id(), SEQ.fetch_add(1, Ordering::Relaxed)));
    let res = (|| {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(data)?;
        if let Ok(meta) = fs::metadata(&target) {
            let _ = fs::set_permissions(&tmp, meta.permissions());
        }
        drop(f);
        fs::rename(&tmp, &target)
    })();
    if res.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    res
}

#[cfg(test)]
pub(crate) mod testutil {
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU32, Ordering};

    static N: AtomicU32 = AtomicU32::new(0);

    /// A unique temp directory, removed on drop. Tests never touch the real home.
    pub struct TempDir(pub PathBuf);

    impl TempDir {
        pub fn new(tag: &str) -> TempDir {
            let p = std::env::temp_dir().join(format!(
                "foco-core-test-{}-{}-{}",
                tag,
                std::process::id(),
                N.fetch_add(1, Ordering::Relaxed)
            ));
            let _ = std::fs::remove_dir_all(&p);
            std::fs::create_dir_all(&p).unwrap();
            TempDir(p)
        }
        pub fn path(&self) -> &std::path::Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}
