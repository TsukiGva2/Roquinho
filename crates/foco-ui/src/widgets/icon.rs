//! Vector icons of `Icon.qml` (24x24 grid, round caps/joins). Music icons are not ported.

use foco_base::{path_from_svg, Color, Fill, Point, Surface};
use std::sync::OnceLock;
use tiny_skia::{LineCap, LineJoin, Path, Stroke};

/// Default stroke in icon pixels (`Icon.stroke`).
pub const DEFAULT_STROKE: f32 = 1.8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    Plus,
    Check,
    X,
    Play,
    Pause,
    ChevronUp,
    ChevronDown,
    EyeOff,
    Target,
    Undo,
    Sliders,
    Grip,
    Trash,
    Power,
    Restore,
    Bolt,
    Display,
    Star,
    Sparkle,
}

impl Icon {
    pub const ALL: [Icon; 19] = [
        Icon::Plus,
        Icon::Check,
        Icon::X,
        Icon::Play,
        Icon::Pause,
        Icon::ChevronUp,
        Icon::ChevronDown,
        Icon::EyeOff,
        Icon::Target,
        Icon::Undo,
        Icon::Sliders,
        Icon::Grip,
        Icon::Trash,
        Icon::Power,
        Icon::Restore,
        Icon::Bolt,
        Icon::Display,
        Icon::Star,
        Icon::Sparkle,
    ];

    /// Icons drawn as a filled shape instead of a stroke.
    pub fn filled(self) -> bool {
        matches!(self, Icon::Play | Icon::Bolt | Icon::Sparkle)
    }

    fn svg(self) -> &'static str {
        match self {
            Icon::Plus => "M12 5v14M5 12h14",
            Icon::Check => "M5 12.5l4.5 4.5L19 7.5",
            Icon::X => "M6.5 6.5l11 11M17.5 6.5l-11 11",
            Icon::Play => "M8.5 5.8v12.4a.8.8 0 0 0 1.2.7l9.6-6.2a.8.8 0 0 0 0-1.4L9.7 5.1a.8.8 0 0 0-1.2.7z",
            Icon::Pause => "M9 6v12M15 6v12",
            Icon::ChevronUp => "M6 14.5l6-6 6 6",
            Icon::ChevronDown => "M6 9.5l6 6 6-6",
            Icon::EyeOff => "M3.5 3.5l17 17M10.6 10.6a2 2 0 0 0 2.8 2.8M9.9 5.3A9.6 9.6 0 0 1 12 5c5 0 8.6 4.4 9.5 7a12.4 12.4 0 0 1-2.6 3.8M6.6 6.6C4.6 8 3.2 10 2.5 12c.9 2.6 4.5 7 9.5 7 1.7 0 3.2-.5 4.5-1.2",
            Icon::Target => "M12 3.5a8.5 8.5 0 1 0 0 17a8.5 8.5 0 1 0 0-17M12 8a4 4 0 1 0 0 8a4 4 0 1 0 0-8M12 11.6a.4.4 0 1 0 0 .8a.4.4 0 1 0 0-.8",
            Icon::Undo => "M9 14.5L4.5 10 9 5.5M4.5 10h10a5 5 0 0 1 0 10H11",
            Icon::Sliders => "M4 7.5h9M17 7.5h3M4 16.5h3M11 16.5h9M15 5.5v4M9 14.5v4",
            Icon::Grip => "M9 6.5h.01M15 6.5h.01M9 12h.01M15 12h.01M9 17.5h.01M15 17.5h.01",
            Icon::Trash => "M4.5 7h15M10 11v6M14 11v6M6.5 7l.9 12.1a1 1 0 0 0 1 .9h7.2a1 1 0 0 0 1-.9L17.5 7M9.5 7V4.5h5V7",
            Icon::Power => "M12 3.5v8M6.6 6.6a7.6 7.6 0 1 0 10.8 0",
            Icon::Restore => "M4.5 12a7.5 7.5 0 1 0 2.2-5.3M4.5 4.5v4h4",
            Icon::Bolt => "M13 3L5.5 13.5H12L11 21l7.5-10.5H12z",
            Icon::Display => "M4.5 5h15a1.5 1.5 0 0 1 1.5 1.5v9a1.5 1.5 0 0 1-1.5 1.5h-15A1.5 1.5 0 0 1 3 15.5v-9A1.5 1.5 0 0 1 4.5 5zM9 20.5h6M12 17v3.5",
            Icon::Star => "M12 3.8l2.5 5.2 5.7.8-4.1 4 1 5.7L12 16.8l-5.1 2.7 1-5.7-4.1-4 5.7-.8z",
            Icon::Sparkle => "M12 3.5l1.9 5.6 5.6 1.9-5.6 1.9L12 18.5l-1.9-5.6L4.5 11l5.6-1.9z",
        }
    }

    /// The parsed path (cached; parsed once per process).
    pub fn path(self) -> Option<&'static Path> {
        static PATHS: OnceLock<Vec<Option<Path>>> = OnceLock::new();
        let v = PATHS.get_or_init(|| Icon::ALL.iter().map(|i| path_from_svg(i.svg())).collect());
        v.get(self as usize)?.as_ref()
    }
}

/// Draws `icon` centred on `c`, `size` px square, in `color`. `stroke` is in icon pixels
/// (`Icon.stroke`; the QML default is 1.8).
pub fn paint_icon(s: &mut dyn Surface, icon: Icon, c: Point, size: f32, color: Color, stroke: f32) {
    let Some(path) = icon.path() else { return };
    if color.a <= 0.0 || size <= 0.0 {
        return;
    }
    s.save();
    s.translate(c.x - size * 0.5, c.y - size * 0.5);
    s.scale(size / 24.0, size / 24.0);
    let fill = Fill::Solid(color);
    if icon.filled() {
        s.fill_path(path, &fill);
    } else {
        let w = if icon == Icon::Grip { 3.2 } else { stroke * 24.0 / size };
        let st = Stroke { width: w, line_cap: LineCap::Round, line_join: LineJoin::Round, ..Stroke::default() };
        s.stroke_path(path, &fill, &st);
    }
    s.restore();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_icon_parses() {
        for i in Icon::ALL {
            assert!(i.path().is_some(), "{i:?}");
        }
    }
}
