//! Command-line handling (`src/main.cpp`): args are joined with spaces; a running instance
//! just receives the line (empty = `toggle`) over the IPC socket.

use crate::ipc;

/// `args` excludes `argv[0]`. Joined with `' '` and trimmed (`args.join(" ").trimmed()`).
pub fn command_line(args: &[String]) -> String {
    args.join(" ").trim().to_string()
}

/// `foco-max start`: run, but open nothing (autostart). First word only.
pub fn is_start(line: &str) -> bool {
    line.split_whitespace().next() == Some("start")
}

/// What is sent to an already-running instance: the line, or `toggle` when empty.
pub fn line_for_running(line: &str) -> &str {
    if line.is_empty() {
        "toggle"
    } else {
        line
    }
}

/// Client path: forwards the command line to a running instance. `true` if one took it.
pub fn forward(line: &str) -> bool {
    ipc::send_to_running(line_for_running(line))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn joins_and_trims() {
        assert_eq!(command_line(&v(&[])), "");
        assert_eq!(command_line(&v(&["add", "comprar", "pão"])), "add comprar pão");
        assert_eq!(command_line(&v(&["  add ", "x  "])), "add  x");
        assert_eq!(command_line(&v(&["", ""])), "");
    }

    #[test]
    fn start_detection() {
        assert!(is_start("start"));
        assert!(is_start("  start  "));
        assert!(!is_start("starting"));
        assert!(!is_start("add start"));
        assert!(!is_start(""));
        assert_eq!(line_for_running(""), "toggle");
        assert_eq!(line_for_running("done"), "done");
    }
}
