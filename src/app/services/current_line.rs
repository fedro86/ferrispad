//! Current-line highlight (T0081): pure twin/untwin logic over style bytes.
//!
//! FLTK has no native current-line highlight. FerrisPad gives every
//! plain/syntax style entry (`'A'..='Z'`) a "twin" at `index + 26`
//! (`'['..='t'`): same foreground, plus the line color as a `BgColorExt`
//! background, which FLTK paints to the right edge of each display row. The
//! cursor's line uses twin chars. Marker entries (diff/lint backgrounds) are
//! never twinned, so annotations keep their own background.
//!
//! The style buffer is shared with the syntax highlighter, which only ever
//! *writes* it (`set_text`/`replace`), so twin chars never confuse its
//! checkpoints; a rewrite just marks the twin stale and the next update
//! re-applies it.
//!
//! Everything here is pure (bytes and ranges, no FLTK) and unit-tested; the
//! FLTK glue is `AppState::update_current_line_highlight`.

/// Primary style chars are `'A'..='Z'` (the `StyleMap` caps at 26 entries).
pub const PRIMARY_STYLES: usize = 26;
/// A primary char's twin is `char + TWIN_OFFSET`.
pub const TWIN_OFFSET: u8 = PRIMARY_STYLES as u8;
/// Lines longer than this (minified JS, logs) are not highlighted, so moving
/// the cursor never rewrites a huge run of style bytes.
pub const MAX_LINE_BYTES: i32 = 64 * 1024;

const FIRST_PRIMARY: u8 = b'A';
const LAST_PRIMARY: u8 = FIRST_PRIMARY + TWIN_OFFSET - 1; // 'Z'
const FIRST_TWIN: u8 = FIRST_PRIMARY + TWIN_OFFSET; // '['
const LAST_TWIN: u8 = LAST_PRIMARY + TWIN_OFFSET; // 't'

/// Whether `b` is a twin style char.
pub fn is_twin(b: u8) -> bool {
    (FIRST_TWIN..=LAST_TWIN).contains(&b)
}

/// Replace every twinnable primary char with its twin. `twinnable[i]` says
/// whether primary `'A' + i` is a plain/syntax entry (not a marker).
pub fn twin_bytes(styles: &mut [u8], twinnable: &[bool; PRIMARY_STYLES]) {
    for b in styles {
        if (FIRST_PRIMARY..=LAST_PRIMARY).contains(b) && twinnable[usize::from(*b - FIRST_PRIMARY)]
        {
            *b += TWIN_OFFSET;
        }
    }
}

/// Turn every twin char back into its primary. Idempotent; leaves every
/// other byte alone, so untwinning a range wider than the twinned line is
/// harmless.
pub fn untwin_bytes(styles: &mut [u8]) {
    for b in styles {
        if is_twin(*b) {
            *b -= TWIN_OFFSET;
        }
    }
}

/// A half-open byte range `[start, end)` of the style buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ByteRange {
    pub start: i32,
    pub end: i32,
}

impl ByteRange {
    pub const fn new(start: i32, end: i32) -> Self {
        Self { start, end }
    }

    pub fn len(&self) -> i32 {
        self.end.saturating_sub(self.start).max(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Follow a buffer edit (FLTK modify callback: `deleted` bytes removed at
/// `pos`, then `inserted` bytes inserted there) so the range still covers
/// every twin char it covered before. Conservative: text inserted at either
/// edge is included. Covering too much is harmless (untwin only touches
/// twin chars); covering too little would leave an orphan band.
pub fn shift_range(range: ByteRange, pos: i32, inserted: i32, deleted: i32) -> ByteRange {
    let (inserted, deleted) = (inserted.max(0), deleted.max(0));
    let after_delete = |x: i32| {
        if x <= pos {
            x
        } else if x >= pos.saturating_add(deleted) {
            x - deleted
        } else {
            pos
        }
    };
    let (start, end) = (after_delete(range.start), after_delete(range.end));
    let start = if pos < start {
        start.saturating_add(inserted)
    } else {
        start
    };
    let end = if pos <= end {
        end.saturating_add(inserted)
    } else {
        end
    };
    ByteRange::new(start, end)
}

/// Per-document twin state. `Copy` so it can live in a `Cell` shared with
/// the document's FLTK modify callback (no `RefCell` borrow in a callback).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CurrentLineTwin {
    /// Bytes that may hold twin chars (the twinned line, shifted by edits).
    twinned: Option<ByteRange>,
    /// The style bytes changed since the last apply (edit or restyle).
    stale: bool,
    /// Cursor position at the last apply; lets an idle update skip the line
    /// lookup entirely.
    last_cursor: Option<i32>,
}

/// What an update must do to the style buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TwinPlan {
    /// Untwin this range first (the previously twinned line).
    pub untwin: Option<ByteRange>,
    /// Then twin this range (the cursor's line), if any.
    pub twin: Option<ByteRange>,
}

impl CurrentLineTwin {
    /// A text edit happened (modify callback).
    pub fn on_modify(&mut self, pos: i32, inserted: i32, deleted: i32) {
        self.twinned = self.twinned.map(|r| shift_range(r, pos, inserted, deleted));
        self.stale = true;
    }

    /// The style buffer was rewritten (`set_text` / `replace`): twin chars
    /// there may be gone, re-apply on the next update.
    pub fn mark_stale(&mut self) {
        self.stale = true;
    }

    /// Nothing changed since the last apply and the cursor did not move:
    /// the update can return without looking at the buffer.
    pub fn is_current(&self, cursor: i32) -> bool {
        !self.stale && self.last_cursor == Some(cursor)
    }

    /// Decide the update for the cursor's line: `line` is the range to twin
    /// (`None` when the feature is off, the line is empty or too long).
    /// Returns `None` when nothing has to change.
    pub fn plan(&self, line: Option<ByteRange>) -> Option<TwinPlan> {
        if !self.stale && self.twinned == line {
            return None;
        }
        Some(TwinPlan {
            untwin: self.twinned,
            twin: line,
        })
    }

    /// Record that `plan` (or no plan) was applied at `cursor`.
    pub fn applied(&mut self, twinned: Option<ByteRange>, cursor: Option<i32>) {
        self.twinned = twinned;
        self.stale = false;
        self.last_cursor = cursor;
    }

    /// The range that may hold twin chars.
    pub fn twinned(&self) -> Option<ByteRange> {
        self.twinned
    }
}

/// The range to twin for a line `[line_start, line_end)` (`line_end`
/// excludes the `'\n'`): `None` for an empty line (FLTK cannot band it, see
/// the ticket's spike result) and for a line over [`MAX_LINE_BYTES`].
pub fn line_to_twin(line_start: i32, line_end: i32) -> Option<ByteRange> {
    let range = ByteRange::new(line_start, line_end);
    (!range.is_empty() && range.len() <= MAX_LINE_BYTES).then_some(range)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 'A' plain, 'B'..'G' markers, 'H'.. syntax, plus a custom BgColor
    /// marker at 'K'.
    fn mask() -> [bool; PRIMARY_STYLES] {
        let mut m = [true; PRIMARY_STYLES];
        for c in b'B'..=b'G' {
            m[usize::from(c - b'A')] = false;
        }
        m[usize::from(b'K' - b'A')] = false;
        m
    }

    /// A simulated document: text plus its style bytes, edited the way the
    /// FLTK modify callback keeps them in sync ('A' fillers on insert).
    struct Doc {
        text: Vec<u8>,
        styles: Vec<u8>,
        twin: CurrentLineTwin,
    }

    impl Doc {
        fn new(lines: usize) -> Self {
            let mut text = Vec::new();
            for i in 0..lines {
                text.extend_from_slice(format!("line {i} let x = {i};\n").as_bytes());
            }
            // A mix of plain, syntax and marker chars.
            let styles = text
                .iter()
                .enumerate()
                .map(|(i, _)| b"AHIE"[i % 4])
                .collect();
            Self {
                text,
                styles,
                twin: CurrentLineTwin::default(),
            }
        }

        fn edit(&mut self, pos: usize, deleted: usize, inserted: &[u8]) {
            self.text
                .splice(pos..pos + deleted, inserted.iter().copied());
            self.styles
                .splice(pos..pos + deleted, inserted.iter().map(|_| b'A'));
            self.twin
                .on_modify(pos as i32, inserted.len() as i32, deleted as i32);
        }

        fn line_range(&self, line: usize) -> (i32, i32) {
            let mut start = 0;
            for _ in 0..line {
                start += self.text[start..].iter().position(|&b| b == b'\n').unwrap() + 1;
            }
            let end = start
                + self.text[start..]
                    .iter()
                    .position(|&b| b == b'\n')
                    .unwrap_or(self.text.len() - start);
            (start as i32, end as i32)
        }

        /// What `AppState::update_current_line_highlight` does.
        fn update(&mut self, cursor_line: usize) {
            let (s, e) = self.line_range(cursor_line);
            let line = line_to_twin(s, e);
            if let Some(plan) = self.twin.plan(line) {
                if let Some(r) = plan.untwin {
                    let end = (r.end as usize).min(self.styles.len());
                    let start = (r.start as usize).min(end);
                    untwin_bytes(&mut self.styles[start..end]);
                }
                if let Some(r) = plan.twin {
                    twin_bytes(&mut self.styles[r.start as usize..r.end as usize], &mask());
                }
                self.twin.applied(plan.twin, Some(s));
            }
        }

        /// Lines that hold at least one twin char.
        fn twinned_lines(&self) -> Vec<usize> {
            let mut lines = Vec::new();
            let mut line = 0;
            let mut has = false;
            for (t, s) in self.text.iter().zip(&self.styles) {
                has |= is_twin(*s);
                if *t == b'\n' {
                    if has {
                        lines.push(line);
                    }
                    line += 1;
                    has = false;
                }
            }
            if has {
                lines.push(line);
            }
            lines
        }
    }

    #[test]
    fn current_line_twin_and_untwin_roundtrip() {
        let original: Vec<u8> = (b'A'..=b'Z').chain(*b"AHHZ").collect();
        let mut bytes = original.clone();
        twin_bytes(&mut bytes, &[true; PRIMARY_STYLES]);
        assert!(bytes.iter().all(|b| is_twin(*b)));
        untwin_bytes(&mut bytes);
        assert_eq!(bytes, original);
    }

    #[test]
    fn current_line_markers_not_twinned() {
        let mut bytes = b"ABCDEFGHKZ".to_vec();
        twin_bytes(&mut bytes, &mask());
        // 'A', 'H', 'Z' twinned; markers 'B'..'G' and custom 'K' untouched.
        assert_eq!(
            bytes,
            vec![b'[', b'B', b'C', b'D', b'E', b'F', b'G', b'b', b'K', b't']
        );
        untwin_bytes(&mut bytes);
        assert_eq!(bytes, b"ABCDEFGHKZ".to_vec());
    }

    #[test]
    fn current_line_twin_chars_are_ascii() {
        assert_eq!(FIRST_TWIN, b'[');
        assert_eq!(LAST_TWIN, b't');
        const { assert!(LAST_TWIN < 0x80) };
    }

    #[test]
    fn current_line_no_orphans_after_edits() {
        let mut doc = Doc::new(30);
        doc.update(10);
        assert_eq!(doc.twinned_lines(), vec![10]);

        // Insert 3 lines above (cursor stays on the same text: now line 13).
        let (s, _) = doc.line_range(2);
        doc.edit(s as usize, 0, b"a\nb\nc\n");
        doc.update(13);
        assert_eq!(doc.twinned_lines(), vec![13]);

        // Delete one line above (now line 12).
        let (s, e) = doc.line_range(0);
        doc.edit(s as usize, (e - s + 1) as usize, b"");
        doc.update(12);
        assert_eq!(doc.twinned_lines(), vec![12]);

        // Type inside the twinned line, then paste a multi-line block into
        // it; the cursor ends on the last pasted line.
        let (s, _) = doc.line_range(12);
        doc.edit(s as usize + 3, 0, b"xyz");
        doc.update(12);
        assert_eq!(doc.twinned_lines(), vec![12]);
        doc.edit(s as usize + 5, 0, b"p1\np2\np3");
        doc.update(14);
        assert_eq!(doc.twinned_lines(), vec![14]);

        // Undo the paste (a delete of the same bytes); cursor back on 12.
        doc.edit(s as usize + 5, 8, b"");
        doc.update(12);
        assert_eq!(doc.twinned_lines(), vec![12]);

        // Delete the twinned line together with its neighbours.
        let (s, _) = doc.line_range(11);
        let (_, e) = doc.line_range(13);
        doc.edit(s as usize, (e - s + 1) as usize, b"");
        doc.update(11);
        assert_eq!(doc.twinned_lines(), vec![11]);

        // Move the cursor without editing.
        doc.update(25);
        assert_eq!(doc.twinned_lines(), vec![25]);
    }

    #[test]
    fn current_line_restyle_marks_stale() {
        let mut doc = Doc::new(5);
        doc.update(2);
        assert!(
            doc.twin
                .plan(line_to_twin(doc.line_range(2).0, doc.line_range(2).1))
                .is_none()
        );

        // The highlighter rewrites the whole style buffer (set_text).
        doc.styles.iter_mut().for_each(|b| *b = b'H');
        doc.twin.mark_stale();
        assert!(!doc.twin.is_current(doc.line_range(2).0));
        assert!(doc.twinned_lines().is_empty());

        doc.update(2);
        assert_eq!(doc.twinned_lines(), vec![2]);
    }

    #[test]
    fn current_line_skips_long_lines() {
        let mut doc = Doc::new(3);
        // Make line 1 a 100 KiB line.
        let (s, _) = doc.line_range(1);
        doc.edit(s as usize, 0, &vec![b'x'; 100 * 1024]);
        doc.update(1);
        assert!(doc.twinned_lines().is_empty());
        doc.update(2);
        assert_eq!(doc.twinned_lines(), vec![2]);
        // Back onto the long line: the short one is untwinned, nothing new.
        doc.update(1);
        assert!(doc.twinned_lines().is_empty());
    }

    #[test]
    fn current_line_empty_line_not_twinned() {
        let mut doc = Doc::new(3);
        doc.update(1);
        assert_eq!(doc.twinned_lines(), vec![1]);
        // An empty line in the middle: line 2 becomes empty.
        let (s, e) = doc.line_range(2);
        doc.edit(s as usize, (e - s) as usize, b"");
        doc.update(2);
        assert!(doc.twinned_lines().is_empty(), "previous line untwinned");
        // An empty last line with no trailing '\n'.
        assert_eq!(
            line_to_twin(doc.text.len() as i32, doc.text.len() as i32),
            None
        );
    }

    #[test]
    fn current_line_idle_update_is_a_no_op() {
        let mut twin = CurrentLineTwin::default();
        twin.applied(Some(ByteRange::new(10, 20)), Some(12));
        assert!(twin.is_current(12));
        assert!(!twin.is_current(13));
        assert_eq!(twin.plan(Some(ByteRange::new(10, 20))), None);
        twin.on_modify(0, 1, 0);
        assert!(!twin.is_current(12));
        assert_eq!(twin.twinned(), Some(ByteRange::new(11, 21)));
    }

    #[test]
    fn current_line_shift_range_cases() {
        let r = ByteRange::new(10, 20);
        // Insert before, inside, at either edge, after.
        assert_eq!(shift_range(r, 2, 5, 0), ByteRange::new(15, 25));
        assert_eq!(shift_range(r, 15, 5, 0), ByteRange::new(10, 25));
        assert_eq!(shift_range(r, 10, 5, 0), ByteRange::new(10, 25));
        assert_eq!(shift_range(r, 20, 5, 0), ByteRange::new(10, 25));
        assert_eq!(shift_range(r, 30, 5, 0), r);
        // Delete before, overlapping the start, inside, covering it all.
        assert_eq!(shift_range(r, 0, 0, 5), ByteRange::new(5, 15));
        assert_eq!(shift_range(r, 5, 0, 10), ByteRange::new(5, 10));
        assert_eq!(shift_range(r, 12, 0, 3), ByteRange::new(10, 17));
        assert_eq!(shift_range(r, 0, 0, 100), ByteRange::new(0, 0));
        // Replace (delete + insert) across the whole buffer.
        assert_eq!(shift_range(r, 0, 50, 100), ByteRange::new(0, 50));
    }
}
