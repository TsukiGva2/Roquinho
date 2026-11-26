//! Icons of `qml/Icon.qml`: 24x24 viewBox, stroked (round caps/joins) or filled paths.
//! SVG paths are parsed once, on first use, and kept per thread.

use std::cell::RefCell;

use foco_base::surface::{Fill, Surface};
use foco_base::{path_from_svg, Color, Rect};
use tiny_skia::{LineCap, LineJoin, Path, Stroke};

/// `(name, svg path, filled)` exactly as in Icon.qml (`filledNames` = play bolt sparkle next previous pauseFill).
const ICONS: &[(&str, &str, bool)] = &[
    ("plus", "M12 5v14M5 12h14", false),
    ("check", "M5 12.5l4.5 4.5L19 7.5", false),
    ("x", "M6.5 6.5l11 11M17.5 6.5l-11 11", false),
    ("play", "M8.5 5.8v12.4a.8.8 0 0 0 1.2.7l9.6-6.2a.8.8 0 0 0 0-1.4L9.7 5.1a.8.8 0 0 0-1.2.7z", true),
    ("pause", "M9 6v12M15 6v12", false),
    ("chevronUp", "M6 14.5l6-6 6 6", false),
    ("chevronDown", "M6 9.5l6 6 6-6", false),
    ("eyeoff", "M3.5 3.5l17 17M10.6 10.6a2 2 0 0 0 2.8 2.8M9.9 5.3A9.6 9.6 0 0 1 12 5c5 0 8.6 4.4 9.5 7a12.4 12.4 0 0 1-2.6 3.8M6.6 6.6C4.6 8 3.2 10 2.5 12c.9 2.6 4.5 7 9.5 7 1.7 0 3.2-.5 4.5-1.2", false),
    ("target", "M12 3.5a8.5 8.5 0 1 0 0 17a8.5 8.5 0 1 0 0-17M12 8a4 4 0 1 0 0 8a4 4 0 1 0 0-8M12 11.6a.4.4 0 1 0 0 .8a.4.4 0 1 0 0-.8", false),
    ("undo", "M9 14.5L4.5 10 9 5.5M4.5 10h10a5 5 0 0 1 0 10H11", false),
    ("sliders", "M4 7.5h9M17 7.5h3M4 16.5h3M11 16.5h9M15 5.5v4M9 14.5v4", false),
    ("grip", "M9 6.5h.01M15 6.5h.01M9 12h.01M15 12h.01M9 17.5h.01M15 17.5h.01", false),
    ("trash", "M4.5 7h15M10 11v6M14 11v6M6.5 7l.9 12.1a1 1 0 0 0 1 .9h7.2a1 1 0 0 0 1-.9L17.5 7M9.5 7V4.5h5V7", false),
    ("power", "M12 3.5v8M6.6 6.6a7.6 7.6 0 1 0 10.8 0", false),
    ("restore", "M4.5 12a7.5 7.5 0 1 0 2.2-5.3M4.5 4.5v4h4", false),
    ("bolt", "M13 3L5.5 13.5H12L11 21l7.5-10.5H12z", true),
    ("music", "M9 18V6.5l10-2V16M9 18a2.5 2.5 0 1 1-5 0a2.5 2.5 0 1 1 5 0M19 16a2.5 2.5 0 1 1-5 0a2.5 2.5 0 1 1 5 0", false),
    ("next", "M5.5 6.6v10.8a.8.8 0 0 0 1.2.7l8.3-5.4a.8.8 0 0 0 0-1.4L6.7 5.9a.8.8 0 0 0-1.2.7zM16.8 6h1.6a.6.6 0 0 1 .6.6v10.8a.6.6 0 0 1-.6.6h-1.6a.6.6 0 0 1-.6-.6V6.6a.6.6 0 0 1 .6-.6z", true),
    ("previous", "M18.5 6.6v10.8a.8.8 0 0 1-1.2.7L9 12.7a.8.8 0 0 1 0-1.4l8.3-5.4a.8.8 0 0 1 1.2.7zM7.2 6H5.6a.6.6 0 0 0-.6.6v10.8a.6.6 0 0 0 .6.6h1.6a.6.6 0 0 0 .6-.6V6.6a.6.6 0 0 0-.6-.6z", true),
    ("pauseFill", "M7.5 5.5h2.4a.9.9 0 0 1 .9.9v11.2a.9.9 0 0 1-.9.9H7.5a.9.9 0 0 1-.9-.9V6.4a.9.9 0 0 1 .9-.9zM14.1 5.5h2.4a.9.9 0 0 1 .9.9v11.2a.9.9 0 0 1-.9.9h-2.4a.9.9 0 0 1-.9-.9V6.4a.9.9 0 0 1 .9-.9z", true),
    ("display", "M4.5 5h15a1.5 1.5 0 0 1 1.5 1.5v9a1.5 1.5 0 0 1-1.5 1.5h-15A1.5 1.5 0 0 1 3 15.5v-9A1.5 1.5 0 0 1 4.5 5zM9 20.5h6M12 17v3.5", false),
    ("star", "M12 3.8l2.5 5.2 5.7.8-4.1 4 1 5.7L12 16.8l-5.1 2.7 1-5.7-4.1-4 5.7-.8z", false),
    ("sparkle", "M12 3.5l1.9 5.6 5.6 1.9-5.6 1.9L12 18.5l-1.9-5.6L4.5 11l5.6-1.9z", true),
];

thread_local! {
    #[expect(clippy::option_option, reason = "outer = parsed yet, inner = path data valid")]
    static PATHS: RefCell<Vec<Option<Option<Path>>>> = RefCell::new((0..ICONS.len()).map(|_| None).collect());
}

/// All icon names.
pub fn names() -> &'static [&'static str] {
    static NAMES: std::sync::OnceLock<Vec<&'static str>> = std::sync::OnceLock::new();
    NAMES.get_or_init(|| ICONS.iter().map(|(n, _, _)| *n).collect())
}

fn index(name: &str) -> Option<usize> {
    ICONS.iter().position(|(n, _, _)| *n == name)
}

/// Draws icon `name` into `r` (its width is the icon size, like `Icon.size`). `stroke` is the
/// stroke width in output px (`Icon.stroke`); `grip` always uses a 3.2-unit dot like the QML.
pub fn draw(s: &mut dyn Surface, name: &str, r: Rect, color: Color, stroke: f32) {
    let Some(i) = index(name) else { return };
    let size = r.w.min(if r.h > 0.0 { r.h } else { r.w });
    if size <= 0.0 || color.a <= 0.0 {
        return;
    }
    let path = PATHS.with(|c| {
        let mut c = c.borrow_mut();
        c[i].get_or_insert_with(|| path_from_svg(ICONS[i].1)).clone()
    });
    let Some(path) = path else { return };
    s.save();
    s.translate(r.x, r.y);
    s.scale(size / 24.0, size / 24.0);
    if ICONS[i].2 {
        s.fill_path(&path, &Fill::Solid(color));
    } else {
        let w = if name == "grip" { 3.2 } else { stroke * 24.0 / size };
        let st = Stroke { width: w, line_cap: LineCap::Round, line_join: LineJoin::Round, ..Stroke::default() };
        s.stroke_path(&path, &Fill::Solid(color), &st);
    }
    s.restore();
}
