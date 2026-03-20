//! Terminal screen buffer (grid of cells).
//!
//! Stores the visible terminal content as a 2D grid of cells,
//! each with a character and color attributes. Includes a scrollback
//! ring buffer for history.

use fltk::enums::Color;

/// Maximum scrollback lines
const MAX_SCROLLBACK: usize = 10_000;

/// A single cell in the terminal grid
#[derive(Debug, Clone, Copy)]
pub struct Cell {
    pub ch: char,
    pub fg: Color,
    pub bg: Color,
    pub bold: bool,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            ch: ' ',
            fg: Color::XtermWhite,
            bg: Color::TransparentBg,
            bold: false,
        }
    }
}

/// Terminal screen buffer
pub struct TerminalGrid {
    /// Visible grid: rows × cols
    pub cells: Vec<Vec<Cell>>,
    /// Current cursor row (0-indexed)
    pub cursor_row: usize,
    /// Current cursor col (0-indexed)
    pub cursor_col: usize,
    /// Number of visible columns
    pub cols: usize,
    /// Number of visible rows
    pub rows: usize,
    /// Current default foreground color for new chars
    pub current_fg: Color,
    /// Current default background color for new chars
    pub current_bg: Color,
    /// Current bold state
    pub current_bold: bool,
    /// Scrollback buffer (oldest first)
    scrollback: Vec<Vec<Cell>>,
    /// Scroll offset from bottom (0 = at bottom, >0 = scrolled up)
    pub scroll_offset: usize,
    /// Scroll region top (inclusive, 0-indexed)
    scroll_top: usize,
    /// Scroll region bottom (inclusive, 0-indexed)
    scroll_bottom: usize,
    /// Whether cursor is visible (tracked from CSI ?25h/l)
    pub cursor_visible: bool,
    /// Current reverse video state (SGR 7)
    pub current_reverse: bool,
    /// Saved cursor position (for DECSC/DECRC)
    saved_cursor: Option<(usize, usize)>,
}

impl TerminalGrid {
    /// Create a new terminal grid with the given dimensions
    pub fn new(cols: usize, rows: usize) -> Self {
        let cells = vec![vec![Cell::default(); cols]; rows];
        Self {
            cells,
            cursor_row: 0,
            cursor_col: 0,
            cols,
            rows,
            current_fg: Color::XtermWhite,
            current_bg: Color::TransparentBg,
            current_bold: false,
            scrollback: Vec::new(),
            scroll_offset: 0,
            scroll_top: 0,
            scroll_bottom: rows.saturating_sub(1),
            cursor_visible: true,
            current_reverse: false,
            saved_cursor: None,
        }
    }

    /// Put a character at the current cursor position and advance
    pub fn put_char(&mut self, ch: char) {
        if self.cursor_col >= self.cols {
            // Auto-wrap
            self.cursor_col = 0;
            self.newline();
        }
        if self.cursor_row < self.rows && self.cursor_col < self.cols {
            let (fg, bg) = if self.current_reverse {
                // Reverse video: swap fg and bg
                let visual_bg = if self.current_bg == Color::TransparentBg {
                    Color::XtermBlack
                } else {
                    self.current_bg
                };
                (visual_bg, self.current_fg)
            } else {
                (self.current_fg, self.current_bg)
            };
            self.cells[self.cursor_row][self.cursor_col] = Cell {
                ch,
                fg,
                bg,
                bold: self.current_bold,
            };
            self.cursor_col += 1;
        }
    }

    /// Move to the next line, scrolling if necessary
    pub fn newline(&mut self) {
        if self.cursor_row == self.scroll_bottom {
            self.scroll_up(1);
        } else if self.cursor_row < self.rows - 1 {
            self.cursor_row += 1;
        }
    }

    /// Carriage return — move cursor to column 0
    pub fn carriage_return(&mut self) {
        self.cursor_col = 0;
    }

    /// Scroll the screen up by n lines within the scroll region
    pub fn scroll_up(&mut self, n: usize) {
        for _ in 0..n {
            // Move top line to scrollback
            if self.scroll_top == 0 {
                let line = self.cells[0].clone();
                self.scrollback.push(line);
                if self.scrollback.len() > MAX_SCROLLBACK {
                    self.scrollback.remove(0);
                }
            }
            // Shift lines up within scroll region
            for r in self.scroll_top..self.scroll_bottom {
                self.cells[r] = self.cells[r + 1].clone();
            }
            // Clear bottom line of scroll region
            self.cells[self.scroll_bottom] = vec![Cell::default(); self.cols];
        }
    }

    /// Scroll the screen down by n lines within the scroll region
    pub fn scroll_down(&mut self, n: usize) {
        for _ in 0..n {
            for r in (self.scroll_top + 1..=self.scroll_bottom).rev() {
                self.cells[r] = self.cells[r - 1].clone();
            }
            self.cells[self.scroll_top] = vec![Cell::default(); self.cols];
        }
    }

    /// Clear from cursor to end of line
    pub fn clear_line_from_cursor(&mut self) {
        if self.cursor_row < self.rows {
            for c in self.cursor_col..self.cols {
                self.cells[self.cursor_row][c] = Cell::default();
            }
        }
    }

    /// Clear from start of line to cursor
    pub fn clear_line_to_cursor(&mut self) {
        if self.cursor_row < self.rows {
            for c in 0..=self.cursor_col.min(self.cols - 1) {
                self.cells[self.cursor_row][c] = Cell::default();
            }
        }
    }

    /// Clear entire current line
    pub fn clear_line(&mut self) {
        if self.cursor_row < self.rows {
            self.cells[self.cursor_row] = vec![Cell::default(); self.cols];
        }
    }

    /// Clear from cursor to end of screen
    pub fn clear_screen_from_cursor(&mut self) {
        self.clear_line_from_cursor();
        for r in (self.cursor_row + 1)..self.rows {
            self.cells[r] = vec![Cell::default(); self.cols];
        }
    }

    /// Clear from start of screen to cursor
    pub fn clear_screen_to_cursor(&mut self) {
        self.clear_line_to_cursor();
        for r in 0..self.cursor_row {
            self.cells[r] = vec![Cell::default(); self.cols];
        }
    }

    /// Clear entire screen
    pub fn clear_screen(&mut self) {
        for r in 0..self.rows {
            self.cells[r] = vec![Cell::default(); self.cols];
        }
    }

    /// Set cursor position (1-indexed input, stored 0-indexed)
    pub fn set_cursor(&mut self, row: usize, col: usize) {
        self.cursor_row = row.saturating_sub(1).min(self.rows.saturating_sub(1));
        self.cursor_col = col.saturating_sub(1).min(self.cols.saturating_sub(1));
    }

    /// Move cursor up by n rows
    pub fn cursor_up(&mut self, n: usize) {
        self.cursor_row = self.cursor_row.saturating_sub(n);
    }

    /// Move cursor down by n rows
    pub fn cursor_down(&mut self, n: usize) {
        self.cursor_row = (self.cursor_row + n).min(self.rows.saturating_sub(1));
    }

    /// Move cursor forward by n columns
    pub fn cursor_forward(&mut self, n: usize) {
        self.cursor_col = (self.cursor_col + n).min(self.cols.saturating_sub(1));
    }

    /// Move cursor backward by n columns
    pub fn cursor_backward(&mut self, n: usize) {
        self.cursor_col = self.cursor_col.saturating_sub(n);
    }

    /// Set scroll region (1-indexed input)
    pub fn set_scroll_region(&mut self, top: usize, bottom: usize) {
        let t = top.saturating_sub(1).min(self.rows.saturating_sub(1));
        let b = bottom.saturating_sub(1).min(self.rows.saturating_sub(1));
        if t < b {
            self.scroll_top = t;
            self.scroll_bottom = b;
        }
    }

    /// Reset scroll region to full screen
    pub fn reset_scroll_region(&mut self) {
        self.scroll_top = 0;
        self.scroll_bottom = self.rows.saturating_sub(1);
    }

    /// Save cursor position
    pub fn save_cursor(&mut self) {
        self.saved_cursor = Some((self.cursor_row, self.cursor_col));
    }

    /// Restore cursor position
    pub fn restore_cursor(&mut self) {
        if let Some((r, c)) = self.saved_cursor {
            self.cursor_row = r.min(self.rows.saturating_sub(1));
            self.cursor_col = c.min(self.cols.saturating_sub(1));
        }
    }

    /// Delete n characters at cursor position, shifting remaining chars left
    pub fn delete_chars(&mut self, n: usize) {
        if self.cursor_row < self.rows {
            let row = &mut self.cells[self.cursor_row];
            for _ in 0..n {
                if self.cursor_col < row.len() {
                    row.remove(self.cursor_col);
                    row.push(Cell::default());
                }
            }
        }
    }

    /// Insert n blank characters at cursor position, shifting existing chars right
    pub fn insert_chars(&mut self, n: usize) {
        if self.cursor_row < self.rows {
            let row = &mut self.cells[self.cursor_row];
            for _ in 0..n {
                if self.cursor_col < row.len() {
                    row.insert(self.cursor_col, Cell::default());
                    row.truncate(self.cols);
                }
            }
        }
    }

    /// Insert n blank lines at cursor row, shifting existing lines down
    pub fn insert_lines(&mut self, n: usize) {
        for _ in 0..n {
            if self.cursor_row <= self.scroll_bottom {
                // Remove bottom line of scroll region
                if self.scroll_bottom < self.cells.len() {
                    self.cells.remove(self.scroll_bottom);
                }
                // Insert blank line at cursor
                self.cells
                    .insert(self.cursor_row, vec![Cell::default(); self.cols]);
            }
        }
    }

    /// Delete n lines at cursor row, shifting lines up
    pub fn delete_lines(&mut self, n: usize) {
        for _ in 0..n {
            if self.cursor_row <= self.scroll_bottom && self.cursor_row < self.cells.len() {
                self.cells.remove(self.cursor_row);
                // Insert blank line at bottom of scroll region
                let insert_pos = self.scroll_bottom.min(self.cells.len());
                self.cells
                    .insert(insert_pos, vec![Cell::default(); self.cols]);
            }
        }
    }

    /// Erase n characters from cursor position (overwrite with blanks, don't shift)
    pub fn erase_chars(&mut self, n: usize) {
        if self.cursor_row < self.rows {
            for i in 0..n {
                let c = self.cursor_col + i;
                if c < self.cols {
                    self.cells[self.cursor_row][c] = Cell::default();
                }
            }
        }
    }

    /// Resize the grid to new dimensions
    pub fn resize(&mut self, new_cols: usize, new_rows: usize) {
        // Adjust rows
        while self.cells.len() < new_rows {
            self.cells.push(vec![Cell::default(); new_cols]);
        }
        self.cells.truncate(new_rows);

        // Adjust cols in each row
        for row in &mut self.cells {
            row.resize(new_cols, Cell::default());
        }

        self.cols = new_cols;
        self.rows = new_rows;
        self.cursor_row = self.cursor_row.min(new_rows.saturating_sub(1));
        self.cursor_col = self.cursor_col.min(new_cols.saturating_sub(1));
        self.scroll_top = 0;
        self.scroll_bottom = new_rows.saturating_sub(1);
    }

    /// Get scrollback line count
    pub fn scrollback_len(&self) -> usize {
        self.scrollback.len()
    }

    /// Get a scrollback line (0 = oldest)
    pub fn scrollback_line(&self, idx: usize) -> Option<&[Cell]> {
        self.scrollback.get(idx).map(|v| v.as_slice())
    }

    /// Get the scroll region top (0-indexed)
    pub fn scroll_top(&self) -> usize {
        self.scroll_top
    }

    /// Tab stop — advance to next multiple of 8
    pub fn tab(&mut self) {
        let next = ((self.cursor_col / 8) + 1) * 8;
        self.cursor_col = next.min(self.cols.saturating_sub(1));
    }

    /// Backspace — move cursor back one column (no erase)
    pub fn backspace(&mut self) {
        self.cursor_col = self.cursor_col.saturating_sub(1);
    }

    /// Reset all attributes to defaults
    pub fn reset_attrs(&mut self) {
        self.current_fg = Color::XtermWhite;
        self.current_bg = Color::TransparentBg;
        self.current_bold = false;
        self.current_reverse = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_grid() {
        let grid = TerminalGrid::new(80, 24);
        assert_eq!(grid.cols, 80);
        assert_eq!(grid.rows, 24);
        assert_eq!(grid.cursor_row, 0);
        assert_eq!(grid.cursor_col, 0);
        assert_eq!(grid.cells.len(), 24);
        assert_eq!(grid.cells[0].len(), 80);
    }

    #[test]
    fn test_put_char() {
        let mut grid = TerminalGrid::new(80, 24);
        grid.put_char('H');
        grid.put_char('i');
        assert_eq!(grid.cells[0][0].ch, 'H');
        assert_eq!(grid.cells[0][1].ch, 'i');
        assert_eq!(grid.cursor_col, 2);
    }

    #[test]
    fn test_newline_and_scroll() {
        let mut grid = TerminalGrid::new(80, 3);
        grid.put_char('A');
        grid.cursor_row = 2; // Last row
        grid.cursor_col = 0;
        grid.put_char('C');
        grid.newline(); // Should scroll
        assert_eq!(grid.cursor_row, 2);
        assert_eq!(grid.scrollback_len(), 1);
    }

    #[test]
    fn test_clear_line() {
        let mut grid = TerminalGrid::new(80, 24);
        grid.put_char('X');
        grid.put_char('Y');
        grid.clear_line();
        assert_eq!(grid.cells[0][0].ch, ' ');
        assert_eq!(grid.cells[0][1].ch, ' ');
    }

    #[test]
    fn test_set_cursor() {
        let mut grid = TerminalGrid::new(80, 24);
        grid.set_cursor(5, 10); // 1-indexed
        assert_eq!(grid.cursor_row, 4); // 0-indexed
        assert_eq!(grid.cursor_col, 9);
    }

    #[test]
    fn test_resize() {
        let mut grid = TerminalGrid::new(80, 24);
        grid.put_char('A');
        grid.resize(40, 12);
        assert_eq!(grid.cols, 40);
        assert_eq!(grid.rows, 12);
        assert_eq!(grid.cells.len(), 12);
        assert_eq!(grid.cells[0].len(), 40);
        assert_eq!(grid.cells[0][0].ch, 'A');
    }

    #[test]
    fn test_auto_wrap() {
        let mut grid = TerminalGrid::new(3, 2);
        grid.put_char('A');
        grid.put_char('B');
        grid.put_char('C');
        // cursor_col is now 3, which is >= cols
        grid.put_char('D'); // Should wrap to next line
        assert_eq!(grid.cells[1][0].ch, 'D');
    }

    #[test]
    fn test_tab() {
        let mut grid = TerminalGrid::new(80, 24);
        grid.cursor_col = 3;
        grid.tab();
        assert_eq!(grid.cursor_col, 8);
    }

    #[test]
    fn test_delete_chars() {
        let mut grid = TerminalGrid::new(5, 1);
        for ch in "ABCDE".chars() {
            grid.put_char(ch);
        }
        grid.cursor_col = 1;
        grid.delete_chars(1);
        assert_eq!(grid.cells[0][1].ch, 'C');
        assert_eq!(grid.cells[0][2].ch, 'D');
        assert_eq!(grid.cells[0][3].ch, 'E');
        assert_eq!(grid.cells[0][4].ch, ' ');
    }
}
