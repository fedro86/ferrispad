//! System font catalog.
//!
//! `FLTK` exposes only 16 hardcoded fonts until `app::set_fonts("*")` is called,
//! which scans the platform font registry (fontconfig on Linux, GDI on Windows,
//! CoreText on macOS). That call costs ~50–500 ms on first use, so we delay it
//! until the user actually opens the font picker.
//!
//! `FLTK` font names use a leading-space prefix to mark *proportional* fonts;
//! monospace fonts have no leading space. We expose that flag on each entry so
//! the picker can offer a "Monospace only" filter.
//!
//! All functions are safe to call multiple times — the heavy `set_fonts` call
//! is gated behind an atomic flag.

use fltk::app;
use std::sync::atomic::{AtomicBool, Ordering};

static LOADED: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone)]
pub struct FontEntry {
    /// Human-readable name (leading marker stripped).
    pub display_name: String,
    /// Raw FLTK name including the leading marker; pass this to `Font::by_name`.
    pub fltk_name: String,
    /// True when the font has no leading-space marker (FLTK monospace convention).
    pub is_monospace: bool,
}

/// Idempotently load all system fonts into the FLTK fonts table.
/// Safe to call multiple times; only the first call hits the platform registry.
pub fn ensure_loaded() {
    if LOADED.swap(true, Ordering::AcqRel) {
        return;
    }
    let _count = app::set_fonts("*");
}

/// Return the parsed catalog. Triggers `ensure_loaded()` on first call.
pub fn list() -> Vec<FontEntry> {
    ensure_loaded();
    app::fonts()
        .into_iter()
        .map(|raw| {
            let is_monospace = !raw.starts_with(' ');
            let display_name = raw.trim_start().to_string();
            FontEntry {
                display_name,
                fltk_name: raw,
                is_monospace,
            }
        })
        .collect()
}
