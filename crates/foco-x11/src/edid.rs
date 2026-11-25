//! Minimal EDID parsing: the monitor name descriptor (what Qt's `QScreen::model()` reports).

/// Monitor name (descriptor tag 0xFC), trimmed; `None` when absent.
pub fn monitor_name(edid: &[u8]) -> Option<String> {
    if edid.len() < 128 {
        return None;
    }
    for off in [54usize, 72, 90, 108] {
        let d = &edid[off..off + 18];
        if d[0] == 0 && d[1] == 0 && d[2] == 0 && d[3] == 0xfc {
            let txt = &d[5..18];
            let end = txt
                .iter()
                .position(|&b| b == b'\n' || b == 0)
                .unwrap_or(txt.len());
            let s: String = txt[..end].iter().map(|&b| b as char).collect();
            let s = s.trim().to_string();
            if !s.is_empty() {
                return Some(s);
            }
        }
    }
    None
}

/// Label as `Backend::screens()` builds it: "Notebook" for built-in panels, else the model,
/// else the connector name.
pub fn label_for(connector: &str, model: Option<&str>) -> String {
    if connector.starts_with("eDP") || connector.starts_with("LVDS") {
        return "Notebook".to_string();
    }
    match model.map(str::trim) {
        Some(m) if !m.is_empty() => m.to_string(),
        _ => connector.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_and_label() {
        let mut e = vec![0u8; 128];
        e[72..77].copy_from_slice(&[0, 0, 0, 0xfc, 0]);
        e[77..77 + 7].copy_from_slice(b"VG2328\n");
        for b in &mut e[84..90] {
            *b = b' ';
        }
        assert_eq!(monitor_name(&e).as_deref(), Some("VG2328"));
        assert_eq!(label_for("HDMI-1", Some("VG2328")), "VG2328");
        assert_eq!(label_for("HDMI-1", None), "HDMI-1");
        assert_eq!(label_for("eDP-1", Some("x")), "Notebook");
        assert_eq!(label_for("DP-1", Some("  ")), "DP-1");
        assert!(monitor_name(&[0u8; 10]).is_none());
    }
}
