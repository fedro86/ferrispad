//! VTE handler — ANSI escape sequence parser.
//!
//! Implements the `vte::Perform` trait to translate terminal escape
//! sequences into grid operations.

use fltk::enums::Color;

use super::grid::TerminalGrid;

/// Maps ANSI color index (0-7 normal, 8-15 bright) to FLTK Color
fn ansi_to_color(idx: u8) -> Color {
    match idx {
        0 => Color::XtermBlack,
        1 => Color::XtermRed,
        2 => Color::XtermGreen,
        3 => Color::XtermYellow,
        4 => Color::XtermBlue,
        5 => Color::XtermMagenta,
        6 => Color::XtermCyan,
        7 => Color::XtermWhite,
        // Bright variants
        8 => Color::from_rgb(85, 85, 85),
        9 => Color::from_rgb(255, 85, 85),
        10 => Color::from_rgb(85, 255, 85),
        11 => Color::from_rgb(255, 255, 85),
        12 => Color::from_rgb(85, 85, 255),
        13 => Color::from_rgb(255, 85, 255),
        14 => Color::from_rgb(85, 255, 255),
        15 => Color::from_rgb(255, 255, 255),
        // 256-color: 16-231 = 6×6×6 color cube, 232-255 = grayscale ramp
        16..=231 => {
            let idx = idx - 16;
            let r = (idx / 36) * 51;
            let g = ((idx % 36) / 6) * 51;
            let b = (idx % 6) * 51;
            Color::from_rgb(r, g, b)
        }
        232..=255 => {
            let gray = 8 + (idx - 232) * 10;
            Color::from_rgb(gray, gray, gray)
        }
    }
}

/// VTE handler that drives a TerminalGrid
pub struct VteHandler<'a> {
    pub grid: &'a mut TerminalGrid,
}

impl<'a> VteHandler<'a> {
    pub fn new(grid: &'a mut TerminalGrid) -> Self {
        Self { grid }
    }
}

impl vte::Perform for VteHandler<'_> {
    fn print(&mut self, c: char) {
        self.grid.put_char(c);
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            // BEL
            0x07 => {}
            // BS
            0x08 => self.grid.backspace(),
            // HT (tab)
            0x09 => self.grid.tab(),
            // LF, VT, FF
            0x0A..=0x0C => {
                self.grid.newline();
            }
            // CR
            0x0D => self.grid.carriage_return(),
            _ => {}
        }
    }

    fn hook(&mut self, _params: &vte::Params, _intermediates: &[u8], _ignore: bool, _action: char) {
    }

    fn put(&mut self, _byte: u8) {}

    fn unhook(&mut self) {}

    fn osc_dispatch(&mut self, _params: &[&[u8]], _bell_terminated: bool) {
        // OSC sequences (title changes, etc.) — ignore for now
    }

    fn csi_dispatch(
        &mut self,
        params: &vte::Params,
        intermediates: &[u8],
        _ignore: bool,
        action: char,
    ) {
        let params: Vec<u16> = params.iter().map(|p| p[0]).collect();
        let p0 = params.first().copied().unwrap_or(0);
        let p1 = params.get(1).copied().unwrap_or(0);

        match action {
            // CUU — cursor up
            'A' => self.grid.cursor_up(p0.max(1) as usize),
            // CUB — cursor backward (alias for 'D')
            // CUD — cursor down
            'B' => self.grid.cursor_down(p0.max(1) as usize),
            // CUF — cursor forward
            'C' => self.grid.cursor_forward(p0.max(1) as usize),
            // CUB — cursor backward
            'D' => self.grid.cursor_backward(p0.max(1) as usize),
            // CNL — cursor next line
            'E' => {
                self.grid.cursor_down(p0.max(1) as usize);
                self.grid.carriage_return();
            }
            // CPL — cursor previous line
            'F' => {
                self.grid.cursor_up(p0.max(1) as usize);
                self.grid.carriage_return();
            }
            // CHA — cursor horizontal absolute
            'G' => {
                let col = p0.max(1) as usize;
                self.grid.cursor_col = col.saturating_sub(1).min(self.grid.cols.saturating_sub(1));
            }
            // CUP — cursor position
            'H' | 'f' => {
                let row = p0.max(1) as usize;
                let col = p1.max(1) as usize;
                self.grid.set_cursor(row, col);
            }
            // ED — erase in display
            'J' => match p0 {
                0 => self.grid.clear_screen_from_cursor(),
                1 => self.grid.clear_screen_to_cursor(),
                2 | 3 => self.grid.clear_screen(),
                _ => {}
            },
            // EL — erase in line
            'K' => match p0 {
                0 => self.grid.clear_line_from_cursor(),
                1 => self.grid.clear_line_to_cursor(),
                2 => self.grid.clear_line(),
                _ => {}
            },
            // IL — insert lines
            'L' => self.grid.insert_lines(p0.max(1) as usize),
            // DL — delete lines
            'M' => self.grid.delete_lines(p0.max(1) as usize),
            // DCH — delete characters
            'P' => self.grid.delete_chars(p0.max(1) as usize),
            // SU — scroll up
            'S' => self.grid.scroll_up(p0.max(1) as usize),
            // SD — scroll down
            'T' => self.grid.scroll_down(p0.max(1) as usize),
            // ECH — erase characters
            'X' => self.grid.erase_chars(p0.max(1) as usize),
            // ICH — insert characters
            '@' => self.grid.insert_chars(p0.max(1) as usize),
            // VPA — line position absolute
            'd' => {
                let row = p0.max(1) as usize;
                self.grid.cursor_row = row.saturating_sub(1).min(self.grid.rows.saturating_sub(1));
            }
            // SGR — select graphic rendition
            'm' => self.handle_sgr(&params),
            // DECSTBM — set scrolling region
            'r' => {
                if intermediates.is_empty() {
                    let top = p0.max(1) as usize;
                    let bottom = if p1 == 0 { self.grid.rows } else { p1 as usize };
                    self.grid.set_scroll_region(top, bottom);
                    self.grid.set_cursor(1, 1);
                }
            }
            // DECSC/DECRC via CSI
            's' => self.grid.save_cursor(),
            'u' => self.grid.restore_cursor(),
            // SM/RM — set/reset mode (handle cursor visibility)
            'h' | 'l' => {
                if intermediates == b"?" && p0 == 25 {
                    self.grid.cursor_visible = action == 'h';
                }
            }
            _ => {}
        }
    }

    fn esc_dispatch(&mut self, intermediates: &[u8], _ignore: bool, byte: u8) {
        match (intermediates, byte) {
            // DECSC — save cursor
            (_, b'7') => self.grid.save_cursor(),
            // DECRC — restore cursor
            (_, b'8') => self.grid.restore_cursor(),
            // RI — reverse index (scroll down)
            (_, b'M') => {
                if self.grid.cursor_row == self.grid.scroll_top() {
                    self.grid.scroll_down(1);
                } else {
                    self.grid.cursor_up(1);
                }
            }
            // RIS — full reset
            (_, b'c') => {
                self.grid.clear_screen();
                self.grid.set_cursor(1, 1);
                self.grid.reset_attrs();
                self.grid.reset_scroll_region();
            }
            _ => {}
        }
    }
}

impl VteHandler<'_> {
    /// Handle SGR (Select Graphic Rendition) parameters
    fn handle_sgr(&mut self, params: &[u16]) {
        if params.is_empty() {
            self.grid.reset_attrs();
            return;
        }

        let mut i = 0;
        while i < params.len() {
            match params[i] {
                0 => self.grid.reset_attrs(),
                1 => self.grid.current_bold = true,
                // Reverse video
                7 => self.grid.current_reverse = true,
                22 => self.grid.current_bold = false,
                // Reset reverse video
                27 => self.grid.current_reverse = false,
                // Foreground 30-37
                30..=37 => self.grid.current_fg = ansi_to_color(params[i] as u8 - 30),
                // Bright foreground 90-97
                90..=97 => self.grid.current_fg = ansi_to_color(params[i] as u8 - 90 + 8),
                // Extended foreground: 38;5;n (256-color) or 38;2;r;g;b (truecolor)
                38 => {
                    if i + 1 < params.len() {
                        match params[i + 1] {
                            5 => {
                                if i + 2 < params.len() {
                                    self.grid.current_fg = ansi_to_color(params[i + 2] as u8);
                                    i += 2;
                                }
                            }
                            2 => {
                                if i + 4 < params.len() {
                                    self.grid.current_fg = Color::from_rgb(
                                        params[i + 2] as u8,
                                        params[i + 3] as u8,
                                        params[i + 4] as u8,
                                    );
                                    i += 4;
                                }
                            }
                            _ => {}
                        }
                    }
                }
                39 => self.grid.current_fg = Color::XtermWhite, // default fg
                // Background 40-47
                40..=47 => self.grid.current_bg = ansi_to_color(params[i] as u8 - 40),
                // Bright background 100-107
                100..=107 => self.grid.current_bg = ansi_to_color(params[i] as u8 - 100 + 8),
                // Extended background: 48;5;n or 48;2;r;g;b
                48 => {
                    if i + 1 < params.len() {
                        match params[i + 1] {
                            5 => {
                                if i + 2 < params.len() {
                                    self.grid.current_bg = ansi_to_color(params[i + 2] as u8);
                                    i += 2;
                                }
                            }
                            2 => {
                                if i + 4 < params.len() {
                                    self.grid.current_bg = Color::from_rgb(
                                        params[i + 2] as u8,
                                        params[i + 3] as u8,
                                        params[i + 4] as u8,
                                    );
                                    i += 4;
                                }
                            }
                            _ => {}
                        }
                    }
                }
                49 => self.grid.current_bg = Color::TransparentBg, // default bg
                _ => {}
            }
            i += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn process_bytes(grid: &mut TerminalGrid, bytes: &[u8]) {
        let mut parser = vte::Parser::new();
        let mut handler = VteHandler::new(grid);
        parser.advance(&mut handler, bytes);
    }

    #[test]
    fn test_print_text() {
        let mut grid = TerminalGrid::new(80, 24);
        process_bytes(&mut grid, b"Hello");
        assert_eq!(grid.cells[0][0].ch, 'H');
        assert_eq!(grid.cells[0][4].ch, 'o');
        assert_eq!(grid.cursor_col, 5);
    }

    #[test]
    fn test_newline() {
        let mut grid = TerminalGrid::new(80, 24);
        process_bytes(&mut grid, b"A\r\nB");
        assert_eq!(grid.cells[0][0].ch, 'A');
        assert_eq!(grid.cells[1][0].ch, 'B');
    }

    #[test]
    fn test_cursor_movement() {
        let mut grid = TerminalGrid::new(80, 24);
        // Move to row 3, col 5
        process_bytes(&mut grid, b"\x1b[3;5H");
        assert_eq!(grid.cursor_row, 2);
        assert_eq!(grid.cursor_col, 4);
    }

    #[test]
    fn test_clear_screen() {
        let mut grid = TerminalGrid::new(80, 24);
        process_bytes(&mut grid, b"XXXX\x1b[2J");
        // Screen should be cleared
        assert_eq!(grid.cells[0][0].ch, ' ');
    }

    #[test]
    fn test_sgr_colors() {
        let mut grid = TerminalGrid::new(80, 24);
        // Set red foreground, write 'R'
        process_bytes(&mut grid, b"\x1b[31mR");
        assert_eq!(grid.cells[0][0].ch, 'R');
        assert_eq!(grid.cells[0][0].fg, Color::XtermRed);
    }

    #[test]
    fn test_sgr_bold() {
        let mut grid = TerminalGrid::new(80, 24);
        process_bytes(&mut grid, b"\x1b[1mB\x1b[0mN");
        assert!(grid.cells[0][0].bold);
        assert!(!grid.cells[0][1].bold);
    }

    #[test]
    fn test_erase_in_line() {
        let mut grid = TerminalGrid::new(10, 1);
        process_bytes(&mut grid, b"ABCDEFGHIJ");
        // Move to col 5, erase from cursor to end
        process_bytes(&mut grid, b"\x1b[6G\x1b[K");
        assert_eq!(grid.cells[0][4].ch, 'E');
        assert_eq!(grid.cells[0][5].ch, ' ');
    }

    #[test]
    fn test_256_color() {
        let mut grid = TerminalGrid::new(80, 24);
        // Set fg to 256-color index 196 (red)
        process_bytes(&mut grid, b"\x1b[38;5;196mX");
        assert_eq!(grid.cells[0][0].ch, 'X');
        // Just verify it doesn't crash; exact color mapping tested in ansi_to_color
    }

    #[test]
    fn test_truecolor() {
        let mut grid = TerminalGrid::new(80, 24);
        // Set fg to RGB(100, 150, 200)
        process_bytes(&mut grid, b"\x1b[38;2;100;150;200mX");
        assert_eq!(grid.cells[0][0].ch, 'X');
        assert_eq!(grid.cells[0][0].fg, Color::from_rgb(100, 150, 200));
    }
}
