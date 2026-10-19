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
        Color::from_rgb(self.r, self.g, self.b)
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

#[cfg(test)]
mod tests {
    use super::*;

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
