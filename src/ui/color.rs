//! The one color-math API for theme derivation.
//!
//! Every themed widget (tab bar, dialogs, menus, panels, title bars) derives
//! its shades from the syntax-theme background through [`ThemeRgb`]'s
//! `darken` / `lighten` / `blend`, so there is a single rounding rule and a
//! single place to change it.

use fltk::enums::Color;

/// An 8-bit-per-channel RGB color used for theme derivation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ThemeRgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl ThemeRgb {
    pub const WHITE: Self = Self::new(255, 255, 255);

    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    pub const fn from_tuple(rgb: (u8, u8, u8)) -> Self {
        Self::new(rgb.0, rgb.1, rgb.2)
    }

    pub const fn to_tuple(self) -> (u8, u8, u8) {
        (self.r, self.g, self.b)
    }

    pub fn from_fltk(color: Color) -> Self {
        Self::from_tuple(color.to_rgb())
    }

    pub fn to_fltk(self) -> Color {
        fltk_rgb(self.r, self.g, self.b)
    }

    /// Calculate brightness (0-255)
    pub fn brightness(&self) -> u8 {
        ((self.r as u32 + self.g as u32 + self.b as u32) / 3) as u8
    }

    /// Darken color by a factor (0.0 = black, 1.0 = unchanged)
    pub fn darken(&self, factor: f32) -> Self {
        Self {
            r: (self.r as f32 * factor) as u8,
            g: (self.g as f32 * factor) as u8,
            b: (self.b as f32 * factor) as u8,
        }
    }

    /// Lighten color towards white by a factor (0.0 = unchanged, 1.0 = white)
    pub fn lighten(&self, factor: f32) -> Self {
        let up = |c: u8| c.saturating_add(((255 - c) as f32 * factor) as u8);
        Self {
            r: up(self.r),
            g: up(self.g),
            b: up(self.b),
        }
    }

    /// Blend towards another color (0.0 = self, 1.0 = other)
    pub fn blend(&self, other: &Self, factor: f32) -> Self {
        let mix = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * factor) as u8;
        Self {
            r: mix(self.r, other.r),
            g: mix(self.g, other.g),
            b: mix(self.b, other.b),
        }
    }
}

/// The FLTK color for an sRGB triple. Use this instead of
/// `Color::from_rgb` everywhere (T0098).
///
/// FLTK packs an RGB color as `0xRRGGBB00` and reads `0x00000000` as palette
/// index 0, `FL_FOREGROUND_COLOR`, which FerrisPad sets light in dark mode:
/// `Color::from_rgb(0, 0, 0)` therefore drew pure black as light gray. Black
/// is returned as `Color::Black` (`FL_BLACK`, index 56) instead; every other
/// color keeps its RGB encoding.
pub const fn fltk_rgb(r: u8, g: u8, b: u8) -> Color {
    if r == 0 && g == 0 && b == 0 {
        Color::Black
    } else {
        Color::from_rgb(r, g, b)
    }
}

/// `base` stepped away from `bg` until it reaches `min` WCAG contrast on it:
/// lighter first when `lighter` (toward white), darker otherwise (scaled
/// toward black, which keeps the hue), and the other way when that direction
/// cannot get there. One rule for every "readable on this background" color
/// (list selection, input borders).
pub fn push_to_contrast(base: ThemeRgb, bg: ThemeRgb, min: f64, lighter: bool) -> ThemeRgb {
    let rgb = |c: ThemeRgb| (c.r, c.g, c.b);
    let reaches = |c: ThemeRgb| wcag_contrast(rgb(c), rgb(bg)) >= min;
    let search = |up: bool| {
        let mut c = base;
        for _ in 0..120 {
            if reaches(c) {
                return Some(c);
            }
            c = if up { c.lighten(0.06) } else { c.darken(0.94) };
        }
        reaches(c).then_some(c)
    };
    search(lighter)
        .or_else(|| search(!lighter))
        .unwrap_or(if lighter {
            ThemeRgb::WHITE
        } else {
            ThemeRgb::new(0, 0, 0)
        })
}

/// WCAG 2 relative luminance of an sRGB color, from 0.0 (black) to 1.0
/// (white).
pub fn wcag_luminance((r, g, b): (u8, u8, u8)) -> f64 {
    let ch = |c: u8| {
        let c = f64::from(c) / 255.0;
        if c <= 0.039_28 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * ch(r) + 0.7152 * ch(g) + 0.0722 * ch(b)
}

/// WCAG 2 contrast ratio between two colors, from 1.0 (same luminance) to
/// 21.0 (black on white). Symmetric in its arguments.
pub fn wcag_contrast(a: (u8, u8, u8), b: (u8, u8, u8)) -> f64 {
    let (la, lb) = (wcag_luminance(a), wcag_luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// T0098: FLTK packs RGB as 0xRRGGBB00 and reads 0x00000000 as palette
    /// index 0, `FL_FOREGROUND_COLOR`, which the dark theme sets light: pure
    /// black must never become that.
    #[test]
    fn fltk_rgb_black_is_not_the_foreground_index() {
        let black = fltk_rgb(0, 0, 0);
        assert_ne!(
            black.bits(),
            0,
            "black packed as palette index 0 (foreground)"
        );
        assert_eq!(black, Color::Black);
        assert_eq!(ThemeRgb::new(0, 0, 0).to_fltk(), Color::Black);
        // Every other color keeps its plain RGB encoding.
        assert_eq!(fltk_rgb(0, 0, 1), Color::from_rgb(0, 0, 1));
        assert_eq!(fltk_rgb(255, 255, 255), Color::from_rgb(255, 255, 255));
        assert_eq!(fltk_rgb(43, 48, 59).to_rgb(), (43, 48, 59));
    }

    /// Every RGB → FLTK color conversion goes through [`fltk_rgb`]: a direct
    /// `Color::from_rgb` anywhere else could encode black as the foreground.
    #[test]
    fn no_direct_color_from_rgb_outside_fltk_rgb() {
        fn walk(dir: &std::path::Path, hits: &mut Vec<String>) {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    walk(&path, hits);
                } else if path.extension().is_some_and(|e| e == "rs")
                    && !path.ends_with("ui/color.rs")
                {
                    let text = std::fs::read_to_string(&path).unwrap();
                    for (n, line) in text.lines().enumerate() {
                        if line.contains("Color::from_rgb(") {
                            hits.push(format!("{}:{}", path.display(), n + 1));
                        }
                    }
                }
            }
        }
        let mut hits = Vec::new();
        walk(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
            &mut hits,
        );
        assert!(
            hits.is_empty(),
            "use ui::color::fltk_rgb:\n{}",
            hits.join("\n")
        );
    }

    #[test]
    fn wcag_contrast_extremes_and_symmetry() {
        let (black, white) = ((0, 0, 0), (255, 255, 255));
        assert!((wcag_contrast(black, white) - 21.0).abs() < 1e-9);
        assert!((wcag_contrast(white, black) - 21.0).abs() < 1e-9);
        assert!((wcag_contrast((10, 20, 30), (10, 20, 30)) - 1.0).abs() < 1e-9);
        assert_eq!(wcag_luminance(black), 0.0);
        assert!((wcag_luminance(white) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn darken_scales_and_truncates_each_channel() {
        let c = ThemeRgb::new(200, 101, 0);
        assert_eq!(c.darken(0.5), ThemeRgb::new(100, 50, 0));
        assert_eq!(c.darken(1.0), c);
        assert_eq!(c.darken(0.0), ThemeRgb::new(0, 0, 0));
    }

    #[test]
    fn lighten_moves_toward_white_without_overflow() {
        let c = ThemeRgb::new(55, 155, 255);
        assert_eq!(c.lighten(0.5), ThemeRgb::new(155, 205, 255));
        assert_eq!(c.lighten(1.0), ThemeRgb::WHITE);
        assert_eq!(c.lighten(0.0), c);
        // Out-of-range factors must not panic on u8 overflow.
        assert_eq!(c.lighten(2.0), ThemeRgb::WHITE);
    }

    #[test]
    fn blend_interpolates_in_both_directions() {
        let a = ThemeRgb::new(0, 100, 200);
        let b = ThemeRgb::new(100, 100, 0);
        assert_eq!(a.blend(&b, 0.5), ThemeRgb::new(50, 100, 100));
        assert_eq!(a.blend(&b, 0.0), a);
        assert_eq!(a.blend(&b, 1.0), b);
    }

    #[test]
    fn fltk_round_trip_and_brightness() {
        let c = ThemeRgb::new(12, 34, 56);
        assert_eq!(ThemeRgb::from_fltk(c.to_fltk()), c);
        assert_eq!(ThemeRgb::from_tuple(c.to_tuple()), c);
        assert_eq!(c.brightness(), 34);
    }
}
