use fltk::{
    button::{Button, CheckButton},
    enums::{Align, CallbackTrigger},
    frame::Frame,
    input::Input,
    prelude::*,
    text::{TextBuffer, TextEditor},
    window::Window,
};
use std::cell::RefCell;
use std::rc::Rc;

use crate::app::buffer_text_no_leak;
use crate::app::services::text_ops::{
    find_in_text, find_in_text_backward, find_in_text_regex, find_in_text_regex_backward,
    replace_all_in_text, replace_all_in_text_regex, replace_at_position_regex,
};

use super::{DialogTheme, label_width, max_label_width, show_themed_message};

/// Search state shared by one dialog's buttons; clones share the same cells.
#[derive(Clone)]
struct FindState {
    search_text: Rc<RefCell<String>>,
    /// Where the next search resumes: the last selected match, or an empty
    /// span at a cursor position. Each direction continues from its own side
    /// (see [`Direction::resume_pos`]), so switching between Find Next and
    /// Find Prev moves to the adjacent match instead of reselecting this one.
    resume_span: Rc<RefCell<(usize, usize)>>,
}

/// Which way Find Next / Find Prev walks through the document.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Direction {
    Forward,
    Backward,
}

/// The dialog's "Match case" / "Use regex" checkboxes.
#[derive(Clone, Copy, Debug)]
struct SearchOptions {
    case_sensitive: bool,
    use_regex: bool,
}

impl Direction {
    /// One search pass from `from`, without wrap-around. Forward finds the
    /// first match starting at or after `from`; backward finds the last match
    /// ending at or before `from`.
    fn search_once(
        self,
        text: &str,
        query: &str,
        from: usize,
        opts: SearchOptions,
    ) -> Result<Option<(usize, usize)>, String> {
        let cs = opts.case_sensitive;
        match (self, opts.use_regex) {
            (Direction::Forward, true) => find_in_text_regex(text, query, from, cs),
            (Direction::Backward, true) => find_in_text_regex_backward(text, query, from, cs),
            (Direction::Forward, false) => Ok(find_in_text(text, query, from, cs)),
            (Direction::Backward, false) => Ok(find_in_text_backward(text, query, from, cs)),
        }
    }

    /// Where the wrap-around retry starts (the far end of the document), or
    /// `None` when the first pass already covered the whole document.
    fn wrap_start(self, text_len: usize, from: usize) -> Option<usize> {
        match self {
            Direction::Forward => (from > 0).then_some(0),
            Direction::Backward => (from < text_len).then_some(text_len),
        }
    }

    /// The cursor position after selecting `found`, which is also where the
    /// next search in this direction continues from: the match end going
    /// forward, its start going backward.
    fn resume_pos(self, (start, end): (usize, usize)) -> usize {
        match self {
            Direction::Forward => end,
            Direction::Backward => start,
        }
    }

    /// The char boundary one character away from `pos` in this direction, or
    /// `None` at the end of the document (or if `pos` is not a boundary).
    fn step(self, text: &str, pos: usize) -> Option<usize> {
        match self {
            Direction::Forward => text.get(pos..)?.chars().next().map(|c| pos + c.len_utf8()),
            Direction::Backward => text
                .get(..pos)?
                .chars()
                .next_back()
                .map(|c| pos - c.len_utf8()),
        }
    }
}

/// Search `text` for `query` from `from` in `dir`, wrapping around once to
/// the other end of the document when the first pass finds nothing.
///
/// An empty regex match (`^`, `$`, `\b`, `a*`) sitting exactly at `from` is
/// where the previous search stopped, so it is stepped past: every Find Next /
/// Find Prev moves. It is still returned when it is the only match left, as
/// wrapping all the way round would.
///
/// Returns `(start, end)` byte offsets into `text`, or `Err` with the message
/// of an invalid regex.
fn search_wrapping(
    text: &str,
    query: &str,
    from: usize,
    dir: Direction,
    opts: SearchOptions,
) -> Result<Option<(usize, usize)>, String> {
    let empty_at_from = Some((from, from));
    let mut found = dir.search_once(text, query, from, opts)?;
    let stepped_past = found == empty_at_from;
    if stepped_past {
        found = match dir.step(text, from) {
            Some(next) => dir.search_once(text, query, next, opts)?,
            None => None,
        };
    }
    if found.is_none()
        && let Some(wrap) = dir.wrap_start(text.len(), from)
    {
        found = dir.search_once(text, query, wrap, opts)?;
    }
    Ok(if stepped_past {
        found.or(empty_at_from)
    } else {
        found
    })
}

impl FindState {
    fn new() -> Self {
        Self {
            search_text: Rc::new(RefCell::new(String::new())),
            resume_span: Rc::new(RefCell::new((0, 0))),
        }
    }

    /// Where this search starts: from the editor cursor when the query
    /// changed since the last search, otherwise from this direction's side
    /// of the last match.
    fn start_pos(&self, dir: Direction, query: &str, cursor: usize) -> usize {
        let query_changed = *self.search_text.borrow() != query;
        if query_changed {
            *self.search_text.borrow_mut() = query.to_string();
            *self.resume_span.borrow_mut() = (cursor, cursor);
            cursor
        } else {
            dir.resume_pos(*self.resume_span.borrow())
        }
    }

    /// The UI-free core of Find Next / Find Prev: pick the start position,
    /// search with wrap-around, and remember where the next search resumes.
    fn locate(
        &self,
        dir: Direction,
        text: &str,
        query: &str,
        cursor: usize,
        opts: SearchOptions,
    ) -> Result<Option<(usize, usize)>, String> {
        let start_pos = self.start_pos(dir, query, cursor);
        let found = search_wrapping(text, query, start_pos, dir, opts)?;
        if let Some(m) = found {
            *self.resume_span.borrow_mut() = m;
        }
        Ok(found)
    }

    /// Find Next / Find Prev: select the match in the editor, or explain
    /// why there is none.
    fn find(
        &self,
        dir: Direction,
        query: &str,
        buf: &mut TextBuffer,
        ed: &mut TextEditor,
        opts: SearchOptions,
        theme_bg: (u8, u8, u8),
    ) {
        let text = buffer_text_no_leak(buf);
        let cursor = ed.insert_position() as usize;
        match self.locate(dir, &text, query, cursor, opts) {
            Ok(Some(m @ (match_start, match_end))) => {
                buf.select(match_start as i32, match_end as i32);
                ed.set_insert_position(dir.resume_pos(m) as i32);
                ed.show_insert_position();
            }
            Ok(None) => {
                show_themed_message(theme_bg, "Find", &format!("Cannot find '{}'", query));
            }
            Err(e) => show_themed_message(theme_bg, "Invalid regex", &e),
        }
    }
}

/// Minimum width of the "Use regex" checkbox; the case-folding hint sits
/// right of it.
const REGEX_CHECK_W: i32 = 100;
/// Minimum width of the case-folding hint and the "Match case" checkbox.
const REGEX_HINT_W: i32 = 170;
const CASE_CHECK_W: i32 = 200;
/// Label size of the case-folding hint.
const HINT_SIZE: i32 = 11;
/// Width of the box FLTK draws left of a check button's label.
const CHECK_BOX_W: i32 = LABEL_SIZE + 8;
/// FLTK's default label size, which the labels of these dialogs use.
const LABEL_SIZE: i32 = 14;
/// Minimum width of the label column ("Find what:"); the input column
/// starts 10 px right of it, at [`INPUT_X`] when it is this wide.
const LABEL_COL_W: i32 = 80;
const INPUT_X: i32 = 110;
/// Minimum width of both dialogs.
const DIALOG_W: i32 = 400;

/// Measured widths of the Find and Find & Replace dialogs, with the fonts
/// installed: the label column fits each of `labels`, the checkboxes their
/// label plus the box FLTK draws left of it, the hint its text, also 2 pt
/// larger (see `label_width`). Never narrower than the fixed widths they had.
struct FindLayout {
    /// How far the input column moves right of [`INPUT_X`], and the dialog
    /// widens, for the label column.
    shift: i32,
    case_w: i32,
    regex_w: i32,
    hint_w: i32,
}

impl FindLayout {
    fn measure(labels: &[&str]) -> Self {
        let label_w = max_label_width(labels, LABEL_SIZE).max(LABEL_COL_W);
        let check = |label, min: i32| (label_width(label, LABEL_SIZE) + CHECK_BOX_W).max(min);
        Self {
            shift: label_w - LABEL_COL_W,
            case_w: check("Match case", CASE_CHECK_W),
            regex_w: check("Use regex", REGEX_CHECK_W),
            hint_w: label_width(REGEX_CASE_FOLDING_HINT, HINT_SIZE).max(REGEX_HINT_W),
        }
    }

    fn label_w(&self) -> i32 {
        LABEL_COL_W + self.shift
    }

    fn input_x(&self) -> i32 {
        INPUT_X + self.shift
    }

    /// Wider by the label column's growth, and enough to keep the
    /// case-folding hint inside.
    fn dialog_w(&self) -> i32 {
        (DIALOG_W + self.shift).max(self.input_x() + self.regex_w + self.hint_w)
    }
}

/// Hint shown next to "Use regex": the regex engine (`regex-lite`) folds case
/// for ASCII letters only, so `café` does not match `CAFÉ` with "Match case"
/// off, while plain search does (T0049).
const REGEX_CASE_FOLDING_HINT: &str = "(case folding: ASCII only)";

/// Draw the dimmed case-folding hint right of the "Use regex" checkbox at
/// `(x, y)`, one checkbox row high.
fn regex_case_folding_hint(layout: &FindLayout, x: i32, y: i32, theme: &DialogTheme) -> Frame {
    let mut hint = Frame::default()
        .with_pos(x + layout.regex_w, y)
        .with_size(layout.hint_w, 25)
        .with_label(REGEX_CASE_FOLDING_HINT)
        .with_align(Align::Left | Align::Inside);
    hint.set_label_size(HINT_SIZE);
    hint.set_label_color(theme.text_dim);
    hint
}

/// The Find & Replace dialog's window and widgets, built but not shown (a
/// seam for the layout tests).
pub struct ReplaceUi {
    pub dialog_win: Window,
    pub find_input: Input,
    pub replace_input: Input,
    pub case_check: CheckButton,
    pub regex_check: CheckButton,
    pub find_prev_btn: Button,
    pub find_btn: Button,
    pub replace_btn: Button,
    pub replace_all_btn: Button,
    pub close_btn: Button,
}

/// Build the Find & Replace dialog.
pub fn build_replace_dialog(theme: &DialogTheme) -> ReplaceUi {
    // Built top-level even if another group is current (T0058).
    let top_level = super::TopLevelScope::new();
    // The label column fits its labels; the input column and the right
    // aligned Close button move right by as much, the dialog widens by it.
    let layout = FindLayout::measure(&["Find what:", "Replace:"]);
    let (label_w, input_x) = (layout.label_w(), layout.input_x());
    let mut dialog_win = Window::default()
        .with_size(layout.dialog_w(), 245)
        .with_label("Find & Replace")
        .center_screen();
    dialog_win.set_color(theme.bg);

    let mut find_label = Frame::default()
        .with_pos(20, 20)
        .with_size(label_w, 30)
        .with_label("Find what:");
    find_label.set_label_color(theme.text);
    let find_input = theme.input().with_pos(input_x, 20).with_size(270, 30);

    let mut replace_label = Frame::default()
        .with_pos(20, 60)
        .with_size(label_w, 30)
        .with_label("Replace:");
    replace_label.set_label_color(theme.text);
    let replace_input = theme.input().with_pos(input_x, 60).with_size(270, 30);

    let case_check = theme
        .check_button()
        .with_pos(input_x, 100)
        .with_size(layout.case_w, 25)
        .with_label("Match case");

    let regex_check = theme
        .check_button()
        .with_pos(input_x, 125)
        .with_size(layout.regex_w, 25)
        .with_label("Use regex");
    regex_case_folding_hint(&layout, input_x, 125, theme);

    let find_prev_btn = theme
        .button()
        .with_pos(20, 160)
        .with_size(90, 30)
        .with_label("Find Prev");
    let find_btn = theme
        .button()
        .with_pos(120, 160)
        .with_size(90, 30)
        .with_label("Find Next");
    let replace_btn = theme
        .button()
        .with_pos(220, 160)
        .with_size(90, 30)
        .with_label("Replace");
    let replace_all_btn = theme
        .button()
        .with_pos(20, 205)
        .with_size(100, 30)
        .with_label("Replace All");
    let close_btn = theme
        .button()
        .with_pos(layout.dialog_w() - 100, 205)
        .with_size(90, 30)
        .with_label("Close");

    dialog_win.end();
    drop(top_level);
    ReplaceUi {
        dialog_win,
        find_input,
        replace_input,
        case_check,
        regex_check,
        find_prev_btn,
        find_btn,
        replace_btn,
        replace_all_btn,
        close_btn,
    }
}

/// Show Find & Replace dialog
pub fn show_replace_dialog(buffer: &TextBuffer, editor: &mut TextEditor, theme_bg: (u8, u8, u8)) {
    let theme = DialogTheme::from_theme_bg(theme_bg);

    let ReplaceUi {
        mut dialog_win,
        find_input,
        replace_input,
        case_check,
        regex_check,
        mut find_prev_btn,
        mut find_btn,
        mut replace_btn,
        mut replace_all_btn,
        mut close_btn,
    } = build_replace_dialog(&theme);
    dialog_win.make_resizable(false);
    dialog_win.show();
    theme.apply_titlebar(&dialog_win);

    let state = FindState::new();
    let text_buf = buffer.clone();
    let text_ed = editor.clone();

    // Find Next button
    let state1 = state.clone();
    let mut tb1 = text_buf.clone();
    let mut te1 = text_ed.clone();
    let find_input1 = find_input.clone();
    let case_check1 = case_check.clone();
    let regex_check1 = regex_check.clone();

    find_btn.set_callback(move |_| {
        let query = find_input1.value();
        if query.is_empty() {
            show_themed_message(theme_bg, "Find", "Please enter text to find");
            return;
        }
        let opts = SearchOptions {
            case_sensitive: case_check1.is_checked(),
            use_regex: regex_check1.is_checked(),
        };
        state1.find(
            Direction::Forward,
            &query,
            &mut tb1,
            &mut te1,
            opts,
            theme_bg,
        );
    });

    // Find Previous button
    let state_prev = state.clone();
    let mut tb_prev = text_buf.clone();
    let mut te_prev = text_ed.clone();
    let find_input_prev = find_input.clone();
    let case_check_prev = case_check.clone();
    let regex_check_prev = regex_check.clone();

    find_prev_btn.set_callback(move |_| {
        let query = find_input_prev.value();
        if query.is_empty() {
            show_themed_message(theme_bg, "Find", "Please enter text to find");
            return;
        }
        let opts = SearchOptions {
            case_sensitive: case_check_prev.is_checked(),
            use_regex: regex_check_prev.is_checked(),
        };
        state_prev.find(
            Direction::Backward,
            &query,
            &mut tb_prev,
            &mut te_prev,
            opts,
            theme_bg,
        );
    });

    // Replace button
    let sp2 = state.resume_span.clone();
    let mut tb2 = text_buf.clone();
    let mut te2 = text_ed.clone();
    let find_input2 = find_input.clone();
    let replace_input2 = replace_input.clone();
    let case_check2 = case_check.clone();
    let regex_check2 = regex_check.clone();
    let mut find_btn2 = find_btn.clone();

    replace_btn.set_callback(move |_| {
        let query = find_input2.value();
        let replacement = replace_input2.value();

        if query.is_empty() {
            show_themed_message(theme_bg, "Find", "Please enter text to find");
            return;
        }

        let case_sensitive = case_check2.is_checked();
        let use_regex = regex_check2.is_checked();

        if let Some((start, end)) = tb2.selection_position()
            && start != end
        {
            if use_regex {
                // Regex replace must run against the full document so anchors
                // (^, $) resolve against surrounding context, then verify the
                // match starts exactly at `start` and covers the selection.
                let text = buffer_text_no_leak(&tb2);
                match replace_at_position_regex(
                    &text,
                    &query,
                    &replacement,
                    start as usize,
                    case_sensitive,
                ) {
                    Ok(Some((replaced, match_len))) if match_len == (end - start) as usize => {
                        tb2.replace_selection(&replaced);
                        te2.set_insert_position(start + replaced.len() as i32);
                        let after = start as usize + replaced.len();
                        *sp2.borrow_mut() = (after, after);
                    }
                    Ok(_) => {}
                    Err(e) => {
                        show_themed_message(theme_bg, "Invalid regex", &e);
                        return;
                    }
                }
            } else {
                let selected = tb2.selection_text();
                let matches = if case_sensitive {
                    selected == query
                } else {
                    selected.to_lowercase() == query.to_lowercase()
                };

                if matches {
                    tb2.replace_selection(&replacement);
                    te2.set_insert_position(start + replacement.len() as i32);
                    let after = start as usize + replacement.len();
                    *sp2.borrow_mut() = (after, after);
                }
            }
        }

        find_btn2.do_callback();
    });

    // Replace All button
    let mut tb3 = text_buf.clone();
    let mut te3 = text_ed.clone();
    let find_input3 = find_input.clone();
    let replace_input3 = replace_input.clone();
    let case_check3 = case_check.clone();
    let regex_check3 = regex_check.clone();

    replace_all_btn.set_callback(move |_| {
        let query = find_input3.value();
        let replacement = replace_input3.value();

        if query.is_empty() {
            show_themed_message(theme_bg, "Find", "Please enter text to find");
            return;
        }

        let text = buffer_text_no_leak(&tb3);
        let case_sensitive = case_check3.is_checked();

        let (new_text, count) = if regex_check3.is_checked() {
            match replace_all_in_text_regex(&text, &query, &replacement, case_sensitive) {
                Ok(v) => v,
                Err(e) => {
                    show_themed_message(theme_bg, "Invalid regex", &e);
                    return;
                }
            }
        } else {
            replace_all_in_text(&text, &query, &replacement, case_sensitive)
        };

        if count > 0 {
            tb3.set_text(&new_text);
            te3.set_insert_position(0);
            show_themed_message(
                theme_bg,
                "Replace All",
                &format!("Replaced {} occurrence(s)", count),
            );
        } else {
            show_themed_message(theme_bg, "Find", &format!("Cannot find '{}'", query));
        }
    });

    // Enter key on find input triggers Find Next
    let mut find_btn_enter = find_btn.clone();
    let mut find_input_enter = find_input.clone();
    find_input_enter.set_trigger(CallbackTrigger::EnterKeyAlways);
    find_input_enter.set_callback(move |_| {
        find_btn_enter.do_callback();
    });

    let dialog_close = dialog_win.clone();
    close_btn.set_callback(move |_| {
        dialog_close.clone().hide();
    });

    let dialog_x = dialog_win.clone();
    dialog_win.set_callback(move |_| {
        dialog_x.clone().hide();
    });

    super::run_dialog(&dialog_win);
}

/// The Find dialog's window and widgets, built but not shown (a seam for
/// the layout tests).
pub struct FindUi {
    pub dialog_win: Window,
    pub find_input: Input,
    pub case_check: CheckButton,
    pub regex_check: CheckButton,
    pub find_prev_btn: Button,
    pub find_btn: Button,
    pub close_btn: Button,
}

/// Build the Find dialog.
pub fn build_find_dialog(theme: &DialogTheme) -> FindUi {
    // Built top-level even if another group is current (T0058).
    let top_level = super::TopLevelScope::new();
    // The label column fits its label; the input column and the button
    // row under it move right by as much, the dialog widens by it.
    let layout = FindLayout::measure(&["Find what:"]);
    let (label_w, input_x, shift) = (layout.label_w(), layout.input_x(), layout.shift);
    let mut dialog_win = Window::default()
        .with_size(layout.dialog_w(), 175)
        .with_label("Find")
        .center_screen();
    dialog_win.set_color(theme.bg);

    let mut find_label = Frame::default()
        .with_pos(20, 20)
        .with_size(label_w, 30)
        .with_label("Find what:");
    find_label.set_label_color(theme.text);
    let find_input = theme.input().with_pos(input_x, 20).with_size(270, 30);

    let case_check = theme
        .check_button()
        .with_pos(input_x, 60)
        .with_size(layout.case_w, 25)
        .with_label("Match case");

    let regex_check = theme
        .check_button()
        .with_pos(input_x, 87)
        .with_size(layout.regex_w, 25)
        .with_label("Use regex");
    regex_case_folding_hint(&layout, input_x, 87, theme);

    let find_prev_btn = theme
        .button()
        .with_pos(110 + shift, 125)
        .with_size(90, 30)
        .with_label("Find Prev");
    let find_btn = theme
        .button()
        .with_pos(210 + shift, 125)
        .with_size(90, 30)
        .with_label("Find Next");
    let close_btn = theme
        .button()
        .with_pos(310 + shift, 125)
        .with_size(80, 30)
        .with_label("Close");

    dialog_win.end();
    drop(top_level);
    FindUi {
        dialog_win,
        find_input,
        case_check,
        regex_check,
        find_prev_btn,
        find_btn,
        close_btn,
    }
}

/// Show Find dialog
pub fn show_find_dialog(buffer: &TextBuffer, editor: &mut TextEditor, theme_bg: (u8, u8, u8)) {
    let theme = DialogTheme::from_theme_bg(theme_bg);

    let FindUi {
        mut dialog_win,
        find_input,
        case_check,
        regex_check,
        find_prev_btn: mut find_prev_btn2,
        mut find_btn,
        mut close_btn,
    } = build_find_dialog(&theme);
    dialog_win.make_resizable(false);
    dialog_win.show();
    theme.apply_titlebar(&dialog_win);

    let state = FindState::new();

    // Find Next button (simple dialog)
    let state1 = state.clone();
    let mut tb1 = buffer.clone();
    let mut te1 = editor.clone();
    let find_input1 = find_input.clone();
    let case_check1 = case_check.clone();
    let regex_check1 = regex_check.clone();

    find_btn.set_callback(move |_| {
        let query = find_input1.value();
        if query.is_empty() {
            show_themed_message(theme_bg, "Find", "Please enter text to find");
            return;
        }
        let opts = SearchOptions {
            case_sensitive: case_check1.is_checked(),
            use_regex: regex_check1.is_checked(),
        };
        state1.find(
            Direction::Forward,
            &query,
            &mut tb1,
            &mut te1,
            opts,
            theme_bg,
        );
    });

    // Find Previous button (simple dialog)
    let state2 = state;
    let mut tb2 = buffer.clone();
    let mut te2 = editor.clone();
    let find_input2 = find_input.clone();
    let case_check2 = case_check.clone();
    let regex_check2 = regex_check.clone();

    find_prev_btn2.set_callback(move |_| {
        let query = find_input2.value();
        if query.is_empty() {
            show_themed_message(theme_bg, "Find", "Please enter text to find");
            return;
        }
        let opts = SearchOptions {
            case_sensitive: case_check2.is_checked(),
            use_regex: regex_check2.is_checked(),
        };
        state2.find(
            Direction::Backward,
            &query,
            &mut tb2,
            &mut te2,
            opts,
            theme_bg,
        );
    });

    // Enter key on find input triggers Find Next
    let mut find_btn_enter2 = find_btn.clone();
    let mut find_input_enter2 = find_input.clone();
    find_input_enter2.set_trigger(CallbackTrigger::EnterKeyAlways);
    find_input_enter2.set_callback(move |_| {
        find_btn_enter2.do_callback();
    });

    let dialog_close = dialog_win.clone();
    close_btn.set_callback(move |_| {
        dialog_close.clone().hide();
    });

    let dialog_x = dialog_win.clone();
    dialog_win.set_callback(move |_| {
        dialog_x.clone().hide();
    });

    super::run_dialog(&dialog_win);
}

#[cfg(test)]
mod tests {
    use super::*;
    use Direction::{Backward, Forward};

    const PLAIN: SearchOptions = SearchOptions {
        case_sensitive: true,
        use_regex: false,
    };
    const PLAIN_CI: SearchOptions = SearchOptions {
        case_sensitive: false,
        use_regex: false,
    };
    const REGEX: SearchOptions = SearchOptions {
        case_sensitive: true,
        use_regex: true,
    };
    const REGEX_CI: SearchOptions = SearchOptions {
        case_sensitive: false,
        use_regex: true,
    };

    fn search(
        dir: Direction,
        text: &str,
        query: &str,
        from: usize,
        opts: SearchOptions,
    ) -> Option<(usize, usize)> {
        search_wrapping(text, query, from, dir, opts).expect("valid query")
    }

    // ---- pure search core: one function, both directions ----

    #[test]
    fn forward_finds_match_starting_at_cursor() {
        let text = "foo bar foo";
        assert_eq!(search(Forward, text, "foo", 0, PLAIN), Some((0, 3)));
        assert_eq!(search(Forward, text, "foo", 8, PLAIN), Some((8, 11)));
    }

    #[test]
    fn forward_skips_match_that_starts_before_cursor() {
        assert_eq!(
            search(Forward, "foo bar foo", "foo", 1, PLAIN),
            Some((8, 11))
        );
    }

    #[test]
    fn forward_wraps_to_start_of_document() {
        let text = "foo bar foo";
        assert_eq!(search(Forward, text, "foo", 9, PLAIN), Some((0, 3)));
        assert_eq!(
            search(Forward, text, "foo", text.len(), PLAIN),
            Some((0, 3))
        );
        // A stale position past the end (the buffer shrank) also wraps.
        assert_eq!(search(Forward, text, "foo", 50, PLAIN), Some((0, 3)));
    }

    #[test]
    fn forward_without_any_match_is_none() {
        assert_eq!(search(Forward, "abc", "x", 0, PLAIN), None);
        assert_eq!(search(Forward, "abc", "x", 2, PLAIN), None);
    }

    #[test]
    fn backward_finds_match_ending_at_cursor() {
        let text = "foo bar foo";
        assert_eq!(search(Backward, text, "foo", 11, PLAIN), Some((8, 11)));
        assert_eq!(search(Backward, text, "foo", 3, PLAIN), Some((0, 3)));
    }

    #[test]
    fn backward_skips_match_that_ends_after_cursor() {
        assert_eq!(
            search(Backward, "foo bar foo", "foo", 10, PLAIN),
            Some((0, 3))
        );
    }

    #[test]
    fn backward_wraps_to_end_of_document() {
        let text = "foo bar foo";
        assert_eq!(search(Backward, text, "foo", 2, PLAIN), Some((8, 11)));
        assert_eq!(search(Backward, text, "foo", 0, PLAIN), Some((8, 11)));
    }

    #[test]
    fn backward_without_any_match_is_none() {
        assert_eq!(search(Backward, "abc", "x", 3, PLAIN), None);
        assert_eq!(search(Backward, "abc", "x", 1, PLAIN), None);
    }

    #[test]
    fn case_insensitive_matching_in_both_directions() {
        let text = "Foo FOO foo";
        assert_eq!(search(Forward, text, "foo", 1, PLAIN_CI), Some((4, 7)));
        assert_eq!(search(Forward, text, "foo", 1, PLAIN), Some((8, 11)));
        assert_eq!(search(Backward, text, "foo", 8, PLAIN_CI), Some((4, 7)));
        // Case-sensitive: nothing ends before 8, so it wraps to the only exact match.
        assert_eq!(search(Backward, text, "foo", 8, PLAIN), Some((8, 11)));
    }

    #[test]
    fn non_ascii_offsets_index_the_original_text() {
        // 'İ' (U+0130) is 2 bytes and lowercases to 3 bytes; offsets must
        // still point into the original text.
        let text = "İ foo FOO";
        assert_eq!(search(Forward, text, "FOO", 0, PLAIN_CI), Some((3, 6)));
        assert_eq!(
            search(Backward, text, "foo", text.len(), PLAIN_CI),
            Some((7, 10))
        );
        assert_eq!(&text[7..10], "FOO");

        let text = "café CAFÉ café"; // 'é' is 2 bytes: matches at 0..5, 6..11, 12..17
        assert_eq!(search(Forward, text, "café", 1, PLAIN_CI), Some((6, 11)));
        assert_eq!(search(Backward, text, "CAFÉ", 12, PLAIN_CI), Some((6, 11)));
        assert_eq!(&text[6..11], "CAFÉ");
    }

    #[test]
    fn cursor_off_a_char_boundary_does_not_panic() {
        let text = "café CAFÉ café"; // byte 4 is inside the first 'é'
        assert_eq!(search(Forward, text, "café", 4, PLAIN_CI), Some((6, 11)));
        assert_eq!(search(Forward, text, "CAFÉ", 4, REGEX), Some((6, 11)));
        // Byte 16 is inside the last 'é': that match does not end before it.
        assert_eq!(search(Backward, text, "café", 16, PLAIN_CI), Some((6, 11)));
        assert_eq!(search(Backward, text, "CAFÉ", 16, REGEX), Some((6, 11)));
    }

    #[test]
    fn regex_in_both_directions_with_wrap() {
        let text = "a1 b22 c333";
        assert_eq!(search(Forward, text, r"\d+", 0, REGEX), Some((1, 2)));
        assert_eq!(search(Forward, text, r"\d+", 3, REGEX), Some((4, 6)));
        assert_eq!(search(Forward, text, r"\d+", 11, REGEX), Some((1, 2)));
        assert_eq!(search(Backward, text, r"\d+", 11, REGEX), Some((8, 11)));
        assert_eq!(search(Backward, text, r"\d+", 4, REGEX), Some((1, 2)));
        assert_eq!(search(Backward, text, r"\d+", 1, REGEX), Some((8, 11)));
    }

    #[test]
    fn regex_case_sensitivity_follows_the_option() {
        assert_eq!(search(Forward, "Foo", "foo", 0, REGEX_CI), Some((0, 3)));
        assert_eq!(search(Forward, "Foo", "foo", 0, REGEX), None);
        assert_eq!(search(Backward, "Foo", "foo", 3, REGEX_CI), Some((0, 3)));
        assert_eq!(search(Backward, "Foo", "foo", 3, REGEX), None);
    }

    #[test]
    fn invalid_regex_is_an_error_in_both_directions() {
        for dir in [Forward, Backward] {
            assert!(search_wrapping("abc", "(", 1, dir, REGEX).is_err());
        }
    }

    // ---- search state: where repeated Find Next / Find Prev resume ----

    #[test]
    fn repeated_find_next_cycles_and_wraps() {
        let state = FindState::new();
        let text = "x a x a x";
        let hits: Vec<_> = (0..4)
            .map(|_| state.locate(Forward, text, "x", 0, PLAIN))
            .collect();
        assert_eq!(
            hits,
            [
                Ok(Some((0, 1))),
                Ok(Some((4, 5))),
                Ok(Some((8, 9))),
                Ok(Some((0, 1)))
            ]
        );
        assert_eq!(*state.resume_span.borrow(), (0, 1));
    }

    #[test]
    fn repeated_find_prev_cycles_and_wraps() {
        let state = FindState::new();
        let text = "x a x a x";
        let hits: Vec<_> = (0..4)
            .map(|_| state.locate(Backward, text, "x", text.len(), PLAIN))
            .collect();
        assert_eq!(
            hits,
            [
                Ok(Some((8, 9))),
                Ok(Some((4, 5))),
                Ok(Some((0, 1))),
                Ok(Some((8, 9)))
            ]
        );
        assert_eq!(*state.resume_span.borrow(), (8, 9));
    }

    #[test]
    fn same_query_resumes_from_last_match_not_cursor() {
        let state = FindState::new();
        let text = "x a x a x";
        assert_eq!(state.locate(Forward, text, "x", 0, PLAIN), Ok(Some((0, 1))));
        // The cursor argument is ignored while the query is unchanged.
        assert_eq!(state.locate(Forward, text, "x", 9, PLAIN), Ok(Some((4, 5))));
    }

    #[test]
    fn changed_query_restarts_from_cursor() {
        let state = FindState::new();
        let text = "x a x a x";
        assert_eq!(state.locate(Forward, text, "x", 0, PLAIN), Ok(Some((0, 1))));
        assert_eq!(state.locate(Forward, text, "a", 4, PLAIN), Ok(Some((6, 7))));
        assert_eq!(*state.search_text.borrow(), "a");
    }

    #[test]
    fn switching_direction_moves_to_adjacent_match() {
        // T0050 (replaces T0031's pin `switching_direction_reselects_the_current_match`):
        // the first search in the other direction resumes from the far side
        // of the current match, so it selects a different match.
        let state = FindState::new();
        let text = "foo foo foo";
        let next = |state: &FindState| state.locate(Forward, text, "foo", 0, PLAIN);
        let prev = |state: &FindState| state.locate(Backward, text, "foo", 0, PLAIN);
        assert_eq!(next(&state), Ok(Some((0, 3))));
        assert_eq!(next(&state), Ok(Some((4, 7))));
        assert_eq!(prev(&state), Ok(Some((0, 3))));
        // And back the other way: Prev, Prev, then Next.
        assert_eq!(prev(&state), Ok(Some((8, 11))));
        assert_eq!(prev(&state), Ok(Some((4, 7))));
        assert_eq!(next(&state), Ok(Some((8, 11))));
    }

    #[test]
    fn switching_direction_moves_past_empty_regex_matches() {
        // Empty matches have no far side: switching direction steps past the
        // current one (T0048's rule) to the adjacent line start. The cursor
        // starts at the end, so the first Find Next wraps to the top.
        let state = FindState::new();
        let text = "a\nb\nc";
        let end = text.len();
        let next = |state: &FindState| state.locate(Forward, text, "^", end, REGEX);
        let prev = |state: &FindState| state.locate(Backward, text, "^", end, REGEX);
        assert_eq!(next(&state), Ok(Some((0, 0))));
        assert_eq!(next(&state), Ok(Some((2, 2))));
        assert_eq!(next(&state), Ok(Some((4, 4))));
        assert_eq!(prev(&state), Ok(Some((2, 2))));
        assert_eq!(prev(&state), Ok(Some((0, 0))));
        assert_eq!(next(&state), Ok(Some((2, 2))));
    }

    #[test]
    fn switching_direction_with_a_single_match_keeps_it() {
        // Only one match: both directions wrap back onto it.
        let state = FindState::new();
        let text = "a foo b";
        assert_eq!(
            state.locate(Forward, text, "foo", 0, PLAIN),
            Ok(Some((2, 5)))
        );
        assert_eq!(
            state.locate(Backward, text, "foo", 0, PLAIN),
            Ok(Some((2, 5)))
        );
        assert_eq!(
            state.locate(Forward, text, "foo", 0, PLAIN),
            Ok(Some((2, 5)))
        );
    }

    #[test]
    fn empty_regex_match_advances_on_find_next() {
        // The cursor starts at the end of the text (where it sits after a
        // paste), so the first Find Next wraps to the top.
        let state = FindState::new();
        let text = "a\nb\nc";
        let hits: Vec<_> = (0..4)
            .map(|_| state.locate(Forward, text, "^", text.len(), REGEX))
            .collect();
        assert_eq!(
            hits,
            [
                Ok(Some((0, 0))),
                Ok(Some((2, 2))),
                Ok(Some((4, 4))),
                Ok(Some((0, 0)))
            ]
        );
    }

    #[test]
    fn empty_regex_match_steps_back_on_find_prev() {
        let state = FindState::new();
        let text = "a\nb\nc";
        let hits: Vec<_> = (0..4)
            .map(|_| state.locate(Backward, text, "$", 0, REGEX))
            .collect();
        assert_eq!(
            hits,
            [
                Ok(Some((5, 5))),
                Ok(Some((3, 3))),
                Ok(Some((1, 1))),
                Ok(Some((5, 5)))
            ]
        );
    }

    #[test]
    fn empty_match_at_the_start_point_is_skipped() {
        // Find Next / Find Prev never return an empty match sitting exactly
        // where the search starts, so every press moves.
        let text = "a\nb\nc";
        assert_eq!(search(Forward, text, "^", 2, REGEX), Some((4, 4)));
        assert_eq!(search(Backward, text, "$", 3, REGEX), Some((1, 1)));
        // An empty match at the far end is still found by the wrap pass.
        assert_eq!(search(Forward, "abc", "$", 3, REGEX), Some((3, 3)));
        assert_eq!(search(Backward, "abc", "^", 0, REGEX), Some((0, 0)));
        // A non-empty match ending at the start point is not lost.
        assert_eq!(search(Backward, "baa", "a*", 3, REGEX), Some((1, 3)));
    }

    #[test]
    fn miss_or_regex_error_keeps_resume_point() {
        let state = FindState::new();
        assert_eq!(state.locate(Forward, "abc", "zz", 2, PLAIN), Ok(None));
        assert_eq!(*state.resume_span.borrow(), (2, 2));
        assert_eq!(state.locate(Backward, "abc", "zz", 0, PLAIN), Ok(None));
        assert_eq!(*state.resume_span.borrow(), (2, 2));
        // A new query still resets the resume point to the cursor first.
        assert!(state.locate(Forward, "abc", "(", 1, REGEX).is_err());
        assert_eq!(*state.resume_span.borrow(), (1, 1));
    }
}
