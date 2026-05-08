use fltk::{
    button::{Button, CheckButton},
    enums::CallbackTrigger,
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

use super::{DialogTheme, show_themed_message};

struct FindState {
    search_text: Rc<RefCell<String>>,
    search_pos: Rc<RefCell<usize>>,
}

impl FindState {
    fn new() -> Self {
        Self {
            search_text: Rc::new(RefCell::new(String::new())),
            search_pos: Rc::new(RefCell::new(0usize)),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn find_next(
        st: Rc<RefCell<String>>,
        sp: Rc<RefCell<usize>>,
        query: &str,
        buf: &mut TextBuffer,
        ed: &mut TextEditor,
        case_sensitive: bool,
        use_regex: bool,
        theme_bg: (u8, u8, u8),
    ) {
        let text = buffer_text_no_leak(buf);

        let start_pos = if *st.borrow() != query {
            *st.borrow_mut() = query.to_string();
            let cursor = ed.insert_position() as usize;
            *sp.borrow_mut() = cursor;
            cursor
        } else {
            *sp.borrow()
        };

        let found: Option<(usize, usize)> = if use_regex {
            match find_in_text_regex(&text, query, start_pos, case_sensitive) {
                Err(e) => {
                    show_themed_message(theme_bg, "Invalid regex", &e);
                    return;
                }
                Ok(None) if start_pos > 0 => {
                    match find_in_text_regex(&text, query, 0, case_sensitive) {
                        Ok(v) => v,
                        Err(e) => {
                            show_themed_message(theme_bg, "Invalid regex", &e);
                            return;
                        }
                    }
                }
                Ok(v) => v,
            }
        } else {
            find_in_text(&text, query, start_pos, case_sensitive)
                .map(|pos| (pos, pos + query.len()))
                .or_else(|| {
                    if start_pos > 0 {
                        find_in_text(&text, query, 0, case_sensitive)
                            .map(|pos| (pos, pos + query.len()))
                    } else {
                        None
                    }
                })
        };

        if let Some((match_start, match_end)) = found {
            buf.select(match_start as i32, match_end as i32);
            ed.set_insert_position(match_end as i32);
            ed.show_insert_position();
            *sp.borrow_mut() = match_end;
        } else {
            show_themed_message(theme_bg, "Find", &format!("Cannot find '{}'", query));
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn find_prev(
        st: Rc<RefCell<String>>,
        sp: Rc<RefCell<usize>>,
        query: &str,
        buf: &mut TextBuffer,
        ed: &mut TextEditor,
        case_sensitive: bool,
        use_regex: bool,
        theme_bg: (u8, u8, u8),
    ) {
        let text = buffer_text_no_leak(buf);

        let start_pos = if *st.borrow() != query {
            *st.borrow_mut() = query.to_string();
            let cursor = ed.insert_position() as usize;
            *sp.borrow_mut() = cursor;
            cursor
        } else {
            *sp.borrow()
        };

        let found: Option<(usize, usize)> = if use_regex {
            match find_in_text_regex_backward(&text, query, start_pos, case_sensitive) {
                Err(e) => {
                    show_themed_message(theme_bg, "Invalid regex", &e);
                    return;
                }
                Ok(None) if start_pos < text.len() => {
                    match find_in_text_regex_backward(&text, query, text.len(), case_sensitive) {
                        Ok(v) => v,
                        Err(e) => {
                            show_themed_message(theme_bg, "Invalid regex", &e);
                            return;
                        }
                    }
                }
                Ok(v) => v,
            }
        } else {
            find_in_text_backward(&text, query, start_pos, case_sensitive)
                .map(|pos| (pos, pos + query.len()))
                .or_else(|| {
                    if start_pos < text.len() {
                        find_in_text_backward(&text, query, text.len(), case_sensitive)
                            .map(|pos| (pos, pos + query.len()))
                    } else {
                        None
                    }
                })
        };

        if let Some((match_start, match_end)) = found {
            buf.select(match_start as i32, match_end as i32);
            ed.set_insert_position(match_start as i32);
            ed.show_insert_position();
            *sp.borrow_mut() = match_start;
        } else {
            show_themed_message(theme_bg, "Find", &format!("Cannot find '{}'", query));
        }
    }
}

/// Show Find & Replace dialog
pub fn show_replace_dialog(buffer: &TextBuffer, editor: &mut TextEditor, theme_bg: (u8, u8, u8)) {
    let theme = DialogTheme::from_theme_bg(theme_bg);

    let mut dialog_win = Window::default()
        .with_size(400, 245)
        .with_label("Find & Replace")
        .center_screen();
    dialog_win.set_color(theme.bg);

    let mut find_label = Frame::default()
        .with_pos(20, 20)
        .with_size(80, 30)
        .with_label("Find what:");
    find_label.set_label_color(theme.text);
    let mut find_input = Input::default().with_pos(110, 20).with_size(270, 30);
    find_input.set_color(theme.input_bg);
    find_input.set_text_color(theme.text);
    find_input.set_selection_color(theme.button_bg);

    let mut replace_label = Frame::default()
        .with_pos(20, 60)
        .with_size(80, 30)
        .with_label("Replace:");
    replace_label.set_label_color(theme.text);
    let mut replace_input = Input::default().with_pos(110, 60).with_size(270, 30);
    replace_input.set_color(theme.input_bg);
    replace_input.set_text_color(theme.text);
    replace_input.set_selection_color(theme.button_bg);

    let mut case_check = CheckButton::default()
        .with_pos(110, 100)
        .with_size(200, 25)
        .with_label("Match case");
    case_check.set_label_color(theme.text);
    case_check.set_color(theme.bg);
    case_check.set_selection_color(theme.button_bg);

    let mut regex_check = CheckButton::default()
        .with_pos(110, 125)
        .with_size(200, 25)
        .with_label("Use regex");
    regex_check.set_label_color(theme.text);
    regex_check.set_color(theme.bg);
    regex_check.set_selection_color(theme.button_bg);

    let mut find_prev_btn = Button::default()
        .with_pos(20, 160)
        .with_size(90, 30)
        .with_label("Find Prev");
    find_prev_btn.set_color(theme.button_bg);
    find_prev_btn.set_label_color(theme.text);
    let mut find_btn = Button::default()
        .with_pos(120, 160)
        .with_size(90, 30)
        .with_label("Find Next");
    find_btn.set_color(theme.button_bg);
    find_btn.set_label_color(theme.text);
    let mut replace_btn = Button::default()
        .with_pos(220, 160)
        .with_size(90, 30)
        .with_label("Replace");
    replace_btn.set_color(theme.button_bg);
    replace_btn.set_label_color(theme.text);
    let mut replace_all_btn = Button::default()
        .with_pos(20, 205)
        .with_size(100, 30)
        .with_label("Replace All");
    replace_all_btn.set_color(theme.button_bg);
    replace_all_btn.set_label_color(theme.text);
    let mut close_btn = Button::default()
        .with_pos(300, 205)
        .with_size(90, 30)
        .with_label("Close");
    close_btn.set_color(theme.button_bg);
    close_btn.set_label_color(theme.text);

    dialog_win.end();
    dialog_win.make_resizable(false);
    dialog_win.show();
    theme.apply_titlebar(&dialog_win);

    let state = FindState::new();
    let text_buf = buffer.clone();
    let text_ed = editor.clone();

    // Find Next button
    let st = state.search_text.clone();
    let sp = state.search_pos.clone();
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
        FindState::find_next(
            st.clone(),
            sp.clone(),
            &query,
            &mut tb1,
            &mut te1,
            case_check1.is_checked(),
            regex_check1.is_checked(),
            theme_bg,
        );
    });

    // Find Previous button
    let st_prev = state.search_text.clone();
    let sp_prev = state.search_pos.clone();
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
        FindState::find_prev(
            st_prev.clone(),
            sp_prev.clone(),
            &query,
            &mut tb_prev,
            &mut te_prev,
            case_check_prev.is_checked(),
            regex_check_prev.is_checked(),
            theme_bg,
        );
    });

    // Replace button
    let sp2 = state.search_pos.clone();
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
                        *sp2.borrow_mut() = start as usize + replaced.len();
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
                    *sp2.borrow_mut() = start as usize + replacement.len();
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

/// Show Find dialog
pub fn show_find_dialog(buffer: &TextBuffer, editor: &mut TextEditor, theme_bg: (u8, u8, u8)) {
    let theme = DialogTheme::from_theme_bg(theme_bg);

    let mut dialog_win = Window::default()
        .with_size(400, 175)
        .with_label("Find")
        .center_screen();
    dialog_win.set_color(theme.bg);

    let mut find_label = Frame::default()
        .with_pos(20, 20)
        .with_size(80, 30)
        .with_label("Find what:");
    find_label.set_label_color(theme.text);
    let mut find_input = Input::default().with_pos(110, 20).with_size(270, 30);
    find_input.set_color(theme.input_bg);
    find_input.set_text_color(theme.text);
    find_input.set_selection_color(theme.button_bg);

    let mut case_check = CheckButton::default()
        .with_pos(110, 60)
        .with_size(200, 25)
        .with_label("Match case");
    case_check.set_label_color(theme.text);
    case_check.set_color(theme.bg);
    case_check.set_selection_color(theme.button_bg);

    let mut regex_check = CheckButton::default()
        .with_pos(110, 87)
        .with_size(200, 25)
        .with_label("Use regex");
    regex_check.set_label_color(theme.text);
    regex_check.set_color(theme.bg);
    regex_check.set_selection_color(theme.button_bg);

    let mut find_prev_btn2 = Button::default()
        .with_pos(110, 125)
        .with_size(90, 30)
        .with_label("Find Prev");
    find_prev_btn2.set_color(theme.button_bg);
    find_prev_btn2.set_label_color(theme.text);
    let mut find_btn = Button::default()
        .with_pos(210, 125)
        .with_size(90, 30)
        .with_label("Find Next");
    find_btn.set_color(theme.button_bg);
    find_btn.set_label_color(theme.text);
    let mut close_btn = Button::default()
        .with_pos(310, 125)
        .with_size(80, 30)
        .with_label("Close");
    close_btn.set_color(theme.button_bg);
    close_btn.set_label_color(theme.text);

    dialog_win.end();
    dialog_win.make_resizable(false);
    dialog_win.show();
    theme.apply_titlebar(&dialog_win);

    let state = FindState::new();

    // Find Next button (simple dialog)
    let st = state.search_text.clone();
    let sp = state.search_pos.clone();
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
        FindState::find_next(
            st.clone(),
            sp.clone(),
            &query,
            &mut tb1,
            &mut te1,
            case_check1.is_checked(),
            regex_check1.is_checked(),
            theme_bg,
        );
    });

    // Find Previous button (simple dialog)
    let st2 = state.search_text.clone();
    let sp2 = state.search_pos.clone();
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
        FindState::find_prev(
            st2.clone(),
            sp2.clone(),
            &query,
            &mut tb2,
            &mut te2,
            case_check2.is_checked(),
            regex_check2.is_checked(),
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
