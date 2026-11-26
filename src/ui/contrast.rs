//! FerrisPad's FLTK contrast function (T0078).
//!
//! FLTK picks the text color drawn on a colored background with
//! `fl_contrast(fg, bg)`: it keeps `fg` when its CIELAB lightness differs from
//! `bg`'s by at least the contrast level, and falls back to black or white
//! otherwise. FerrisPad lowers that level to [`FLTK_CONTRAST_LEVEL`] (10) so
//! the tree's git-status label colors survive on a selected row. At level 10,
//! though, text selected in the editor keeps its (syntax) color on the strong
//! blue selection and is barely readable (1.55:1 on Ocean Light).
//!
//! So FerrisPad installs its own contrast function ([`install`]):
//! - on the editor's selection backgrounds (focused, and FLTK's unfocused
//!   blend) the text keeps its color only if it is WCAG-readable (>= 4.5:1),
//!   otherwise it becomes black or white, whichever reads better;
//! - everywhere else it is an exact port of FLTK's own CIELAB rule at
//!   [`FLTK_CONTRAST_LEVEL`], so every other widget draws as before.

use std::ffi::{c_int, c_uint};
use std::sync::atomic::{AtomicU32, Ordering};

use fltk::app::{self, ContrastMode};
use fltk::enums::Color;

use super::color::wcag_contrast;
use super::theme::FLTK_CONTRAST_LEVEL;

/// WCAG AA contrast for normal text.
const READABLE_CONTRAST: f64 = 4.5;

/// FLTK's unfocused-selection blend weight: `Fl_Text_Display` draws the
/// selection of an unfocused editor as
/// `fl_color_average(color(), selection_color(), 0.4)`.
pub const UNFOCUSED_SELECTION_WEIGHT: f32 = 0.4;

/// FLTK's `fl_luminance`: sRGB channels raised to 2.4 (no linear segment).
fn fltk_luminance(color: Color) -> f64 {
    let (r, g, b) = color.to_rgb();
    let ch = |c: u8| (f64::from(c) / 255.0).powf(2.4);
    0.212_672_9 * ch(r) + 0.715_152_2 * ch(g) + 0.072_175 * ch(b)
}

/// FLTK's `fl_lightness`: CIELAB L*, 0 (black) to 100 (white).
pub fn fltk_lightness(color: Color) -> f64 {
    let y = fltk_luminance(color);
    if y <= 216.0 / 24389.0 {
        y * (24389.0 / 27.0)
    } else {
        y.powf(1.0 / 3.0) * 116.0 - 16.0
    }
}

/// Port of FLTK 1.4's `fl_contrast_cielab` (`fl_contrast.cxx`) at contrast
/// `level`: `fg` if the L* difference reaches `level`, else black on a light
/// background (L* > 50) and white on a dark one.
pub fn fltk_cielab_contrast(fg: Color, bg: Color, level: i32) -> Color {
    let tc = f64::from(level);
    let lbg = fltk_lightness(bg);
    let lc = fltk_lightness(fg) - lbg;
    if lc >= tc || lc <= -tc {
        fg
    } else if lbg > 50.0 {
        Color::Black
    } else {
        Color::White
    }
}

/// Port of FLTK's `fl_color_average(c1, c2, weight)` for RGB colors:
/// `weight` of `c1` plus `1 - weight` of `c2` per channel, in `float`, and
/// truncated by the `(uchar)` cast.
pub fn fltk_color_average(c1: (u8, u8, u8), c2: (u8, u8, u8), weight: f32) -> (u8, u8, u8) {
    let mix = |a: u8, b: u8| (f32::from(a) * weight + f32::from(b) * (1.0 - weight)) as u8;
    (mix(c1.0, c2.0), mix(c1.1, c2.1), mix(c1.2, c2.2))
}

/// The backgrounds the editor's selected text is drawn on.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EditorSelectionBgs {
    /// The selection color (focused editor).
    pub focused: Option<(u8, u8, u8)>,
    /// FLTK's blend of editor background and selection (unfocused editor,
    /// e.g. a Find match while the Find dialog has focus).
    pub unfocused: Option<(u8, u8, u8)>,
    /// The same blend on the current-line band (T0081): for a twin style
    /// char, FLTK blends the selection with the char's `BgColorExt` color
    /// (`clear_rect`'s `bgbasecolor`), not with the editor background.
    pub unfocused_on_line: Option<(u8, u8, u8)>,
    /// The selected-row color of dialog lists (T0097).
    pub list_selection: Option<(u8, u8, u8)>,
}

impl EditorSelectionBgs {
    pub fn new(editor_bg: (u8, u8, u8), selection: (u8, u8, u8)) -> Self {
        Self {
            focused: Some(selection),
            unfocused: Some(fltk_color_average(
                editor_bg,
                selection,
                UNFOCUSED_SELECTION_WEIGHT,
            )),
            unfocused_on_line: None,
            list_selection: None,
        }
    }

    /// Also treat the unfocused selection drawn on the current-line band
    /// `line_bg` as a selection background.
    pub fn with_current_line(mut self, line_bg: (u8, u8, u8)) -> Self {
        self.unfocused_on_line = self
            .focused
            .map(|selection| fltk_color_average(line_bg, selection, UNFOCUSED_SELECTION_WEIGHT));
        self
    }

    /// Also treat the selected-row color of dialog lists (T0097) as a
    /// selection background: FLTK draws a selected row's text with
    /// `fl_contrast(text, selection)` too.
    pub fn with_list_selection(mut self, list_selection: (u8, u8, u8)) -> Self {
        self.list_selection = Some(list_selection);
        self
    }

    fn contains(&self, rgb: (u8, u8, u8)) -> bool {
        self.focused == Some(rgb)
            || self.unfocused == Some(rgb)
            || self.unfocused_on_line == Some(rgb)
            || self.list_selection == Some(rgb)
    }
}

/// The black or white FLTK color with the higher WCAG contrast on `bg`.
fn best_black_or_white(bg: (u8, u8, u8)) -> Color {
    if wcag_contrast((0, 0, 0), bg) >= wcag_contrast((255, 255, 255), bg) {
        Color::Black
    } else {
        Color::White
    }
}

/// FerrisPad's contrast rule (see the module docs). Pure: `selection` is
/// passed in so tests need no global state.
pub fn ferrispad_contrast(fg: Color, bg: Color, selection: EditorSelectionBgs) -> Color {
    let bg_rgb = bg.to_rgb();
    if selection.contains(bg_rgb) {
        if wcag_contrast(fg.to_rgb(), bg_rgb) >= READABLE_CONTRAST {
            fg
        } else {
            best_black_or_white(bg_rgb)
        }
    } else {
        fltk_cielab_contrast(fg, bg, FLTK_CONTRAST_LEVEL)
    }
}

// The editor's selection backgrounds, for the contrast callback. FLTK takes a
// plain function pointer (no closure, no user data), so the callback can only
// read global state. Each color is packed as `PRESENT | 0xRRGGBB` (0 = none).
// Atomics keep this free of `unsafe` and of locks that could poison or panic
// inside a callback invoked from C during drawing.
static FOCUSED_BG: AtomicU32 = AtomicU32::new(0);
static UNFOCUSED_BG: AtomicU32 = AtomicU32::new(0);
static UNFOCUSED_LINE_BG: AtomicU32 = AtomicU32::new(0);
static LIST_SELECTION_BG: AtomicU32 = AtomicU32::new(0);
const PRESENT: u32 = 1 << 24;

fn pack(rgb: Option<(u8, u8, u8)>) -> u32 {
    rgb.map_or(0, |(r, g, b)| {
        PRESENT | u32::from(r) << 16 | u32::from(g) << 8 | u32::from(b)
    })
}

fn unpack(bits: u32) -> Option<(u8, u8, u8)> {
    (bits & PRESENT != 0).then(|| {
        let [_, r, g, b] = bits.to_be_bytes();
        (r, g, b)
    })
}

/// Record the main editor's background, selection and current-line colors,
/// so selected text is drawn readable on them (including on the current-line
/// band, T0081). Called wherever the editor selection is set.
pub fn register_editor_selection(
    editor_bg: (u8, u8, u8),
    selection: (u8, u8, u8),
    current_line: (u8, u8, u8),
) {
    let bgs = EditorSelectionBgs::new(editor_bg, selection).with_current_line(current_line);
    FOCUSED_BG.store(pack(bgs.focused), Ordering::Relaxed);
    UNFOCUSED_BG.store(pack(bgs.unfocused), Ordering::Relaxed);
    UNFOCUSED_LINE_BG.store(pack(bgs.unfocused_on_line), Ordering::Relaxed);
}

/// Record the selected-row color of the dialog lists (T0097), so the text of
/// a selected row is drawn readable on it. Called by
/// `DialogTheme::style_list`.
pub fn register_list_selection(selection: (u8, u8, u8)) {
    LIST_SELECTION_BG.store(pack(Some(selection)), Ordering::Relaxed);
}

/// The dialog list selection currently registered (for tests).
#[cfg(test)]
pub(crate) fn registered_list_selection() -> Option<(u8, u8, u8)> {
    unpack(LIST_SELECTION_BG.load(Ordering::Relaxed))
}

fn registered_editor_selection() -> EditorSelectionBgs {
    EditorSelectionBgs {
        focused: unpack(FOCUSED_BG.load(Ordering::Relaxed)),
        unfocused: unpack(UNFOCUSED_BG.load(Ordering::Relaxed)),
        unfocused_on_line: unpack(UNFOCUSED_LINE_BG.load(Ordering::Relaxed)),
        list_selection: unpack(LIST_SELECTION_BG.load(Ordering::Relaxed)),
    }
}

/// The function FLTK calls for every `fl_contrast`, with FLTK's C signature
/// `Fl_Color (*)(Fl_Color fg, Fl_Color bg, int context, int size)`. It runs
/// inside C drawing code: it only does arithmetic and atomic loads, and being
/// `extern "C"`, a panic here would abort rather than unwind into FLTK.
extern "C" fn contrast_callback(fg: c_uint, bg: c_uint, _context: c_int, _size: c_int) -> c_uint {
    // `from_rgbi` keeps the raw bits (indexed colors included); `to_rgb`
    // resolves an index through FLTK's colormap.
    ferrispad_contrast(
        Color::from_rgbi(fg),
        Color::from_rgbi(bg),
        registered_editor_selection(),
    )
    .bits()
}

/// Make FLTK use [`ferrispad_contrast`] for all its contrast decisions.
/// Call once at startup, before any widget is drawn.
pub fn install() {
    app::set_contrast_mode(ContrastMode::Custom);
    // Registered directly, not through `app::set_contrast_function`, which
    // transmutes a Rust-ABI fn pointer into the C one.
    // SAFETY: `contrast_callback` has exactly the C signature FLTK expects
    // (`Fl_Contrast_Function`), captures nothing, and does not unwind.
    unsafe { fltk_sys::fl::Fl_set_contrast_function(Some(contrast_callback)) };
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rgb((r, g, b): (u8, u8, u8)) -> Color {
        crate::ui::color::fltk_rgb(r, g, b)
    }

    /// Hand-checked reference pairs for the CIELAB port at level 10, from
    /// `fl_contrast_cielab`: keep fg if |L*fg - L*bg| >= 10, else black when
    /// L*bg > 50, else white. Guards the port against drift.
    #[test]
    fn contrast_matches_fltk_cielab_elsewhere() {
        // The tree's selected row in light mode (Ocean Light background
        // darkened to 0.90): L* 85.61.
        let tree_sel = (215, 216, 220);
        let cases = [
            // Git-status labels keep their color (the reason for level 10):
            ((190, 140, 40), tree_sel, None), // modified, L* 59.47
            ((60, 140, 50), tree_sel, None),  // added, L* 49.21
            ((190, 50, 50), tree_sel, None),  // conflict, L* 41.33
            // Ocean Light text on the T0078 blue: L* 34.21 vs 47.63 -> kept.
            ((79, 91, 102), (0, 120, 215), None),
            // L* 79.51 vs 83.31: too close, light bg -> black.
            ((200, 200, 200), (210, 210, 210), Some(Color::Black)),
            // L* 20.45 vs 15.51: too close, dark bg -> white.
            ((60, 60, 60), (50, 50, 50), Some(Color::White)),
            ((0, 0, 0), (255, 255, 255), None),
        ];
        for (fg, bg, expected) in cases {
            let got = fltk_cielab_contrast(rgb(fg), rgb(bg), FLTK_CONTRAST_LEVEL);
            let want = expected.unwrap_or(rgb(fg));
            assert_eq!(got, want, "fg {fg:?} on bg {bg:?}");
            // Off the editor selection, FerrisPad's rule is the port.
            let ours = ferrispad_contrast(rgb(fg), rgb(bg), EditorSelectionBgs::default());
            assert_eq!(ours, want, "ferrispad_contrast, fg {fg:?} on bg {bg:?}");
        }
    }

    /// The port agrees with FLTK's own `fl_contrast` in CIELAB mode at level
    /// 10 over a grid of colors, including near-threshold pairs.
    #[test]
    fn contrast_port_matches_fltk_itself() {
        app::set_contrast_mode(ContrastMode::Cielab);
        app::set_contrast_level(FLTK_CONTRAST_LEVEL);
        let steps = [0u8, 37, 64, 90, 128, 150, 191, 215, 240, 255];
        let mut mismatches = Vec::new();
        for &r in &steps {
            for &g in &[0u8, 120, 255] {
                for &b in &steps {
                    let fg = rgb((r, g, b));
                    for bg in [
                        (0, 120, 215),
                        (70, 140, 230),
                        (128, 128, 128),
                        (215, 216, 220),
                    ] {
                        let bg = rgb(bg);
                        let fltk = Color::contrast(fg, bg);
                        let port = fltk_cielab_contrast(fg, bg, FLTK_CONTRAST_LEVEL);
                        if fltk.to_rgb() != port.to_rgb() {
                            mismatches.push(format!("{:?} on {:?}", fg.to_rgb(), bg.to_rgb()));
                        }
                    }
                }
            }
        }
        assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
    }

    /// The registered unfocused background is exactly FLTK's
    /// `fl_color_average(editor_bg, selection, 0.4)`.
    #[test]
    fn unfocused_blend_matches_fltk() {
        // Hand-computed in float and truncated: 43*.4+70*.6 = 59.2 -> 59 (a
        // rounding port would give the same here), 23.6+138 = 161.6 -> 161
        // (rounding would give 162).
        assert_eq!(
            fltk_color_average((43, 48, 59), (70, 140, 230), 0.4),
            (59, 103, 161)
        );
        for (bg, sel) in [
            ((43, 48, 59), (70, 140, 230)),
            ((239, 241, 245), (0, 120, 215)),
            ((0, 43, 54), (70, 140, 230)),
            ((253, 246, 227), (0, 120, 215)),
            ((255, 255, 255), (0, 120, 215)),
        ] {
            let fltk = Color::color_average(rgb(bg), rgb(sel), UNFOCUSED_SELECTION_WEIGHT);
            assert_eq!(
                EditorSelectionBgs::new(bg, sel).unfocused,
                Some(fltk.to_rgb()),
                "bg {bg:?}, selection {sel:?}"
            );
        }
    }

    #[test]
    fn readable_fg_on_selection_is_kept_unreadable_becomes_black_or_white() {
        let sel = EditorSelectionBgs::new((239, 241, 245), (0, 120, 215));
        let blue = rgb((0, 120, 215));
        // RGB black is 4.67:1 on (0,120,215): readable, kept as is.
        let black = rgb((0, 0, 0));
        assert_eq!(ferrispad_contrast(black, blue, sel), black);
        // Ocean Light text is 1.55:1 there: replaced by the better of black
        // (4.67:1) and white (4.49:1).
        let replaced = ferrispad_contrast(rgb((79, 91, 102)), blue, sel);
        assert_eq!(replaced, Color::Black);
        // The same pair off the selection follows FLTK's rule (fg kept).
        let elsewhere = ferrispad_contrast(rgb((79, 91, 102)), blue, EditorSelectionBgs::default());
        assert_eq!(elsewhere, rgb((79, 91, 102)));
    }

    /// The C callback plumbs raw `Fl_Color` bits: with Ocean Light's
    /// selection registered, its text color comes back as FL_BLACK (index
    /// 56); off the selection, the CIELAB port keeps it.
    #[test]
    fn contrast_callback_plumbs_raw_color_bits() {
        register_editor_selection((239, 241, 245), (0, 120, 215), (225, 228, 235));
        let fg = rgb((79, 91, 102)).bits();
        assert_eq!(
            contrast_callback(fg, 0x0078_D700, 0, 14),
            Color::Black.bits()
        );
        assert_eq!(Color::Black.bits(), 56);
        // An indexed background (FL_WHITE, 255) resolves via the colormap.
        assert_eq!(contrast_callback(fg, Color::White.bits(), 0, 14), fg);
    }

    /// T0081 edge case 12: a Find match on the cursor line while the Find
    /// dialog has focus is drawn on `fl_color_average(line, selection, 0.4)`.
    /// That blend must get the readable-text rule like the plain unfocused
    /// blend: a fg that FLTK's level-10 rule keeps but that is not WCAG
    /// readable there becomes black or white.
    #[test]
    fn current_line_unfocused_selection_blend_registered() {
        use crate::app::domain::settings::SyntaxTheme;
        use crate::ui::theme::derived_editor_colors;

        let mut exercised = 0;
        for theme in SyntaxTheme::all() {
            let colors = derived_editor_colors(theme.background(), theme.foreground());
            let plain = EditorSelectionBgs::new(colors.background, colors.selection);
            let with_line = plain.with_current_line(colors.current_line);
            let blend = fltk_color_average(
                colors.current_line,
                colors.selection,
                UNFOCUSED_SELECTION_WEIGHT,
            );
            assert_eq!(with_line.unfocused_on_line, Some(blend), "{theme:?}");
            assert!(with_line.contains(blend), "{theme:?}");

            // A grey that the CIELAB rule keeps on the blend but that is
            // below 4.5:1 there: registered → replaced by black/white.
            let kept_but_unreadable = (0..=255u8).map(|g| (g, g, g)).find(|&fg| {
                fltk_cielab_contrast(rgb(fg), rgb(blend), FLTK_CONTRAST_LEVEL) == rgb(fg)
                    && wcag_contrast(fg, blend) < READABLE_CONTRAST
            });
            let Some(fg) = kept_but_unreadable else {
                continue;
            };
            let out = ferrispad_contrast(rgb(fg), rgb(blend), with_line);
            assert!(out == Color::Black || out == Color::White, "{theme:?}");
            assert!(
                wcag_contrast(out.to_rgb(), blend) >= READABLE_CONTRAST,
                "{theme:?}"
            );
            exercised += 1;
        }
        assert!(exercised > 0, "no theme exercised the readable-text rule");
    }

    #[test]
    fn registry_round_trips_through_the_atomics() {
        assert_eq!(unpack(pack(None)), None);
        for c in [(0, 0, 0), (255, 255, 255), (0, 120, 215)] {
            assert_eq!(unpack(pack(Some(c))), Some(c));
        }
    }
}
