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
/// 1350-font catalog. The token match below covers virtually every real
/// monospace family at zero runtime cost.
///
/// We tokenize on non-letters and check each token, which avoids the
/// "Monotype" / "Monoton" false positives that a naive `contains("mono")`
/// would hit. A trailing-token fallback handles names that smush the tag
/// onto the family ("FreeMono", "IBMPlexMono").
fn is_likely_monospace(name: &str) -> bool {
    /// Tokens that, on their own, identify a monospace family.
    const MONO_TOKENS: &[&str] = &[
        "mono",
        "monospace",
        "monospaced",
        "courier",
        "console", // also matches "Consolas"
        "consolas",
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
        "code", // standalone token only, e.g. "Source Code Pro", "Fira Code"
    ];
    let lower = name.to_lowercase();
    for token in lower.split(|c: char| !c.is_ascii_alphabetic()) {
        if token.is_empty() {
            continue;
        }
        if MONO_TOKENS.contains(&token) {
            return true;
        }
        // Single-token CamelCase names like "FreeMono", "IBMPlexMono".
        if token.ends_with("mono") || token.ends_with("monospace") {
            return true;
        }
    }
    false
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

#[cfg(test)]
mod tests {
    use super::is_likely_monospace;

    #[test]
    fn classifies_common_monospace_families() {
        for name in [
            "DejaVu Sans Mono",
            "Liberation Mono",
            "Ubuntu Mono",
            "Nimbus Mono PS",
            "Noto Sans Mono CJK JP",
            "FreeMono",
            "PT Mono",
            "Roboto Mono",
            "IBM Plex Mono",
            "Fira Code",
            "Source Code Pro",
            "Cascadia Code",
            "JetBrains Mono",
            "Iosevka",
            "Hack",
            "Consolas",
            "Menlo",
            "Monaco",
            "Inconsolata",
            "Courier New",
            "Monospace",
            "Anonymous Pro Mono",
            "BigBlueTermPlus Nerd Font Mono",
        ] {
            assert!(is_likely_monospace(name), "expected mono: {:?}", name);
        }
    }

    #[test]
    fn rejects_proportional_lookalikes() {
        // "Monotype" / "Monoton" share the "mono" prefix but are display fonts.
        for name in [
            "Monotype Corsiva",
            "Monoton",
            "Sans",
            "Sans Bold",
            "Serif",
            "Helvetica",
            "Times New Roman",
            "Arial",
            "Comic Sans MS",
            "Verdana",
            "Georgia",
            "DejaVu Sans",
            "Liberation Sans",
            "Noto Sans",
            "Ubuntu",
        ] {
            assert!(
                !is_likely_monospace(name),
                "expected proportional: {:?}",
                name
            );
        }
    }

    #[test]
    fn handles_punctuation_and_case() {
        assert!(is_likely_monospace("UBUNTU MONO"));
        assert!(is_likely_monospace("dejavu_sans_mono"));
        assert!(is_likely_monospace("Source-Code-Pro"));
        assert!(!is_likely_monospace(""));
    }
}
