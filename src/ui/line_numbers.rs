//! Width of the line-number gutter (T0107): wide enough for the largest line
//! number of the document, in the font FLTK draws the numbers with.

use fltk::enums::Font;

/// FLTK draws a line number inside the gutter minus this many pixels on each
/// side (`Fl_Text_Display::draw_line_numbers`).
pub const FLTK_INSET: i32 = 3;

/// Extra room between the number and the text.
pub const GAP: i32 = 6;

/// The narrowest gutter (also the startup width).
pub const MIN_WIDTH: i32 = 40;

/// Digits of the largest line number shown for a document of `line_count`
/// lines. FLTK also numbers the empty line after a final newline, so the
/// largest number is `line_count + 1`; a count from `str::lines()` (no such
/// line) only ever over-estimates by one.
pub fn digits(line_count: usize) -> u32 {
    line_count.saturating_add(1).checked_ilog10().unwrap_or(0) + 1
}

/// Gutter width for `digits`-digit numbers whose widest digit is
/// `digit_width` px: the number, FLTK's insets and a gap, at least
/// [`MIN_WIDTH`].
pub fn width_for(digits: u32, digit_width: i32) -> i32 {
    let digits = i32::try_from(digits).unwrap_or(i32::MAX);
    digits
        .saturating_mul(digit_width)
        .saturating_add(2 * FLTK_INSET + GAP)
        .max(MIN_WIDTH)
}

/// Width in px (rounded up) of the widest decimal digit in `font` at
/// `size`. Measuring needs the display; FLTK's current drawing font is
/// restored afterwards.
pub fn widest_digit(font: Font, size: i32) -> i32 {
    let (old_font, old_size) = (fltk::draw::font(), fltk::draw::size());
    fltk::draw::set_font(font, size);
    let widest = (0..10)
        .map(|d| fltk::draw::width(&d.to_string()).ceil() as i32)
        .max()
        .unwrap_or(size);
    fltk::draw::set_font(old_font, old_size);
    widest
}

/// The gutter width for a document of `line_count` lines, drawn in `font`
/// at `size` (measures, so it needs the display).
pub fn gutter_width(line_count: usize, font: Font, size: i32) -> i32 {
    width_for(digits(line_count), widest_digit(font, size))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_numbers_digits_at_the_boundaries() {
        // The largest number shown is line_count + 1.
        assert_eq!(digits(0), 1);
        assert_eq!(digits(8), 1);
        assert_eq!(digits(9), 2);
        assert_eq!(digits(9_998), 4);
        assert_eq!(digits(9_999), 5);
        assert_eq!(digits(99_998), 5);
        assert_eq!(digits(99_999), 6);
        assert_eq!(digits(999_999), 7);
        assert_eq!(digits(usize::MAX), digits(usize::MAX - 1));
    }

    #[test]
    fn line_numbers_width_fits_the_number() {
        for d in 1..=9 {
            for digit_w in 4..=30 {
                let w = width_for(d, digit_w);
                assert!(
                    w >= d as i32 * digit_w + 2 * FLTK_INSET,
                    "{d} digits of {digit_w} px"
                );
                assert!(w >= MIN_WIDTH);
            }
        }
        assert_eq!(width_for(6, 9), 6 * 9 + 2 * FLTK_INSET + GAP);
        assert_eq!(width_for(1, 9), MIN_WIDTH);
    }
}
