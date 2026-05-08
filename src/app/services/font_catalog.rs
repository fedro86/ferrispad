//! System font catalog.
//!
//! `FLTK` exposes only 16 hardcoded fonts until `app::get_font_names()` is
//! called, which scans the platform font registry (fontconfig on Linux, GDI on
//! Windows, CoreText on macOS). That call costs ~50–500 ms on first use, so we
//! delay it until the user actually opens the font picker (or, on Windows,
//! when a non-legacy font is configured at startup).
//!
//! Monospace detection: FLTK historically prefixes proportional fonts with a
//! leading space (X11 XLFD convention). That marker is reliable on X11 but is
//! NOT set by the Wayland/Pango backend, where every font name comes back
//! without the prefix. Render-time width measurement was tried as a portable
//! fallback but cost 2–5 s for a 1350-font catalog on Wayland/Cairo, so we
//! settled on a name-based heuristic — see `is_likely_monospace`.
//!
//! Note: `app::set_fonts("*")` updates FLTK's C-level table but NOT fltk-rs's
//! cached `FONTS` Vec, so `app::fonts()` would still return only the 16
//! built-ins. We use `App::load_system_fonts()` which scans the registry AND
//! populates the Rust-side cache in one shot — without it, `Font::by_name`
//! would silently fall back to Helvetica for every non-builtin font name.
//!
//! All functions are safe to call multiple times — the heavy enumeration
//! happens exactly once and the parsed result is cached in a `OnceLock`.

use fltk::app;
use std::sync::OnceLock;

#[derive(Debug, Clone)]
pub struct FontEntry {
    /// Human-readable name (leading marker stripped).
    pub display_name: String,
    /// Raw FLTK name including the leading marker; pass this to `Font::by_name`.
    pub fltk_name: String,
    /// True when `i`, `M`, and `W` all render to the same width at 14pt.
    pub is_monospace: bool,
}

static CATALOG: OnceLock<Vec<FontEntry>> = OnceLock::new();

/// Name-based monospace heuristic. Render-time width measurement was tried
/// first but cost ~1–3 ms per font on Wayland/Cairo, totalling 2–5 s for a
/// 1350-font catalog. The name match below covers virtually every real
/// monospace family (their authors invariably tag the family name) at zero
/// runtime cost.
fn is_likely_monospace(name: &str) -> bool {
    let lower = name.to_lowercase();
    const KEYWORDS: &[&str] = &[
        "mono",
        "courier",
        "console", // also matches "Consolas"
        "terminal",
        "typewriter",
        "fixed",
        "menlo",
        "monaco",
        "hack",
        "inconsolata",
        "cascadia",
        "jetbrains",
        "iosevka",
    ];
    if KEYWORDS.iter().any(|k| lower.contains(k)) {
        return true;
    }
    // "Code" is too generic to match anywhere in the name (would catch
    // "Code Saver" but also random unrelated strings); require it as a
    // standalone token so families like "Source Code Pro", "Fira Code" hit.
    lower.starts_with("code ")
        || lower.ends_with(" code")
        || lower.contains(" code ")
        || lower == "code"
}

fn build_catalog() -> Vec<FontEntry> {
    // load_system_fonts() does two things at once: it scans the platform font
    // registry (set_fonts("*")) AND populates fltk-rs's internal `FONTS` cache.
    // The latter is what `Font::by_name` reads — without this call, looking up
    // any font outside the 16 builtins falls back to Helvetica.
    let _ = fltk::app::App::default().load_system_fonts();
    app::fonts()
        .into_iter()
        .map(|raw| {
            let display_name = raw.trim_start().to_string();
            let is_monospace = is_likely_monospace(&display_name);
            FontEntry {
                display_name,
                fltk_name: raw,
                is_monospace,
            }
        })
        .collect()
}

/// Idempotently load all system fonts into the FLTK fonts table.
/// Safe to call multiple times; only the first call hits the platform registry.
pub fn ensure_loaded() {
    CATALOG.get_or_init(build_catalog);
}

/// Return the parsed catalog. Triggers `ensure_loaded()` on first call.
pub fn list() -> Vec<FontEntry> {
    CATALOG.get_or_init(build_catalog).clone()
}
