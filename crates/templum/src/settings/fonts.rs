//! Which installed font families are monospaced, for the Font section's list.

use std::collections::HashSet;

use gpui_kit::{SharedString, Window, font, px};

use crate::grid;

/// Equal advances for "i" and "m": a monospaced font (GPUI doesn't say which fonts are).
pub(super) fn is_monospace(i: f32, m: f32) -> bool {
    (i - m).abs() <= 0.01
}

/// The installed monospaced families, sorted. Measured when the panel opens: one advance pair
/// per family.
pub(crate) fn monospace_families(window: &Window) -> Vec<SharedString> {
    let ts = window.text_system();
    let installed = ts.all_font_names();
    let candidates = if cfg!(any(target_os = "macos", target_os = "windows")) {
        installed
    } else {
        safe_to_load(installed, fc_list_with_i_and_m())
    };
    candidates
        .into_iter()
        .filter(|name| {
            let id = ts.resolve_font(&font(name.clone()));
            // A family that doesn't load resolves to a fallback font: not this family.
            let loaded = ts
                .get_font_for_id(id)
                .is_some_and(|f| f.family.as_ref() == name.as_str());
            let advance = |c| ts.advance(id, px(14.0), c).map(|s| f32::from(s.width)).ok();
            loaded
                && matches!((advance('i'), advance('m')), (Some(i), Some(m)) if m > 0.0 && is_monospace(i, m))
        })
        .map(SharedString::from)
        .collect()
}

/// The families that are safe to load on Linux. There GPUI's text system drops a loaded family's
/// faces that have no "m" from the font database for good, and those are the emoji, symbol and
/// script fonts other text falls back to. So only families fontconfig says have both "i" and "m"
/// are loaded; without fc-list, only the built-in list.
pub(super) fn safe_to_load(
    installed: Vec<String>,
    with_i_and_m: Option<HashSet<String>>,
) -> Vec<String> {
    installed
        .into_iter()
        .filter(|name| match &with_i_and_m {
            Some(families) => families.contains(name),
            None => grid::MONOSPACE.contains(&name.as_str()),
        })
        .collect()
}

/// Every family name of the fonts that have both "i" and "m", from fontconfig.
fn fc_list_with_i_and_m() -> Option<HashSet<String>> {
    let out = std::process::Command::new("fc-list")
        .args([":charset=69 6d", "--format", "%{family}\n"])
        .output()
        .ok()?;
    out.status.success().then(|| {
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .flat_map(|line| line.split(','))
            .map(str::to_owned)
            .collect()
    })
}
