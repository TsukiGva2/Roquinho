//! Standard locations (`QStandardPaths` `AppDataLocation` / `GenericConfigLocation` on Linux).

use std::path::PathBuf;

fn env_path(name: &str) -> Option<PathBuf> {
    let v = std::env::var_os(name)?;
    let p = PathBuf::from(v);
    // XDG: empty or relative values are invalid and must be ignored.
    if p.is_absolute() {
        Some(p)
    } else {
        None
    }
}

fn home() -> PathBuf {
    env_path("HOME").unwrap_or_else(|| PathBuf::from("/"))
}

/// `$XDG_DATA_HOME` or `~/.local/share`, plus `/foco-max`.
pub fn data_dir() -> PathBuf {
    env_path("XDG_DATA_HOME").unwrap_or_else(|| home().join(".local/share")).join("foco-max")
}

/// `$XDG_CONFIG_HOME` or `~/.config` (the generic config location, no app suffix).
pub fn config_dir() -> PathBuf {
    env_path("XDG_CONFIG_HOME").unwrap_or_else(|| home().join(".config"))
}

/// `data_dir()/state.json`.
pub fn state_path() -> PathBuf {
    data_dir().join("state.json")
}

/// `config_dir()/foco-maxrc`.
pub fn rc_path() -> PathBuf {
    config_dir().join("foco-maxrc")
}
