use fltk::{
    button::{Button, CheckButton},
    dialog,
    enums::CallbackTrigger,
    frame::Frame,
    input::Input,
    prelude::*,
    text::{TextBuffer, TextEditor},
    window::Window,
};
use std::cell::RefCell;
use std::rc::Rc;

use crate::app::buffer_utils::buffer_text_no_leak;
use crate::app::text_ops::{find_in_text, find_in_text_backward, replace_all_in_text};

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

    fn find_next(
        st: Rc<RefCell<String>>,
        sp: Rc<RefCell<usize>>,
        query: &str,
        buf: &mut TextBuffer,
        ed: &mut TextEditor,
        case_sensitive: bool,
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

        let found = find_in_text(&text, query, start_pos, case_sensitive)
            .or_else(|| if start_pos > 0 { find_in_text(&text, query, 0, case_sensitive) } else { None });

        if let Some(pos) = found {
            buf.select(pos as i32, (pos + query.len()) as i32);
            ed.set_insert_position((pos + query.len()) as i32);
            ed.show_insert_position();
            *sp.borrow_mut() = pos + query.len();
        } else {
            dialog::message_default(&format!("Cannot find '{}'", query));
        }
    }

    fn find_prev(
        st: Rc<RefCell<String>>,
        sp: Rc<RefCell<usize>>,
        query: &str,
        buf: &mut TextBuffer,
        ed: &mut TextEditor,
        case_sensitive: bool,
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

        let found = find_in_text_backward(&text, query, start_pos, case_sensitive)
            .or_else(|| if start_pos < text.len() { find_in_text_backward(&text, query, text.len(), case_sensitive) } else { None });

        if let Some(pos) = found {
            buf.select(pos as i32, (pos + query.len()) as i32);
            ed.set_insert_position(pos as i32);
            ed.show_insert_position();
            *sp.borrow_mut() = pos;
        } else {
            dialog::message_default(&format!("Cannot find '{}'", query));
        }
    }
}

/// Show Find & Replace dialog
pub fn show_replace_dialog(buffer: &TextBuffer, editor: &mut TextEditor) {
    let mut dialog_win = Window::default()
        .with_size(400, 220)
        .with_label("Find & Replace")
        .center_screen();

    Frame::default().with_pos(20, 20).with_size(80, 30).with_label("Find what:");
    let find_input = Input::default().with_pos(110, 20).with_size(270, 30);

    Frame::default().with_pos(20, 60).with_size(80, 30).with_label("Replace:");
    let replace_input = Input::default().with_pos(110, 60).with_size(270, 30);

    let case_check = CheckButton::default()
        .with_pos(110, 100).with_size(200, 25).with_label("Match case");

    let mut find_prev_btn = Button::default()
        .with_pos(20, 140).with_size(90, 30).with_label("Find Prev");
    let mut find_btn = Button::default()
        .with_pos(120, 140).with_size(90, 30).with_label("Find Next");
    let mut replace_btn = Button::default()
        .with_pos(220, 140).with_size(90, 30).with_label("Replace");
    let mut replace_all_btn = Button::default()
        .with_pos(20, 180).with_size(100, 30).with_label("Replace All");
    let mut close_btn = Button::default()
        .with_pos(300, 180).with_size(90, 30).with_label("Close");

    dialog_win.end();
    dialog_win.make_resizable(false);
    dialog_win.show();

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

    find_btn.set_callback(move |_| {
        let query = find_input1.value();
        if query.is_empty() {
            dialog::message_default("Please enter text to find");
            return;
        }
        FindState::find_next(st.clone(), sp.clone(), &query, &mut tb1, &mut te1, case_check1.is_checked());
    });

    // Find Previous button
    let st_prev = state.search_text.clone();
    let sp_prev = state.search_pos.clone();
    let mut tb_prev = text_buf.clone();
    let mut te_prev = text_ed.clone();
    let find_input_prev = find_input.clone();
    let case_check_prev = case_check.clone();

    find_prev_btn.set_callback(move |_| {
        let query = find_input_prev.value();
        if query.is_empty() {
            dialog::message_default("Please enter text to find");
            return;
        }
        FindState::find_prev(st_prev.clone(), sp_prev.clone(), &query, &mut tb_prev, &mut te_prev, case_check_prev.is_checked());
    });

    // Replace button
    let sp2 = state.search_pos.clone();
    let mut tb2 = text_buf.clone();
    let mut te2 = text_ed.clone();
    let find_input2 = find_input.clone();
    let replace_input2 = replace_input.clone();
    let case_check2 = case_check.clone();
    let mut find_btn2 = find_btn.clone();

    replace_btn.set_callback(move |_| {
        let query = find_input2.value();
        let replacement = replace_input2.value();

        if query.is_empty() {
            dialog::message_default("Please enter text to find");
            return;
        }

        if let Some((start, end)) = tb2.selection_position() {
            if start != end {
                let selected = tb2.selection_text();
                let case_sensitive = case_check2.is_checked();

                let matches = if case_sensitive {
                    selected == query
                } else {
                    selected.to_lowercase() == query.to_lowercase()
                };

                if matches {
                    tb2.replace_selection(&replacement);
                    te2.set_insert_position(start + replacement.len() as i32);
                    *sp2.borrow_mut() = (start as usize) + replacement.len();
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

    replace_all_btn.set_callback(move |_| {
        let query = find_input3.value();
        let replacement = replace_input3.value();

        if query.is_empty() {
            dialog::message_default("Please enter text to find");
            return;
        }

        let text = buffer_text_no_leak(&tb3);
        let case_sensitive = case_check3.is_checked();

        let (new_text, count) = replace_all_in_text(&text, &query, &replacement, case_sensitive);

        if count > 0 {
            tb3.set_text(&new_text);
            te3.set_insert_position(0);
            dialog::message_default(&format!("Replaced {} occurrence(s)", count));
        } else {
            dialog::message_default(&format!("Cannot find '{}'", query));
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
pub fn show_find_dialog(buffer: &TextBuffer, editor: &mut TextEditor) {
    let mut dialog_win = Window::default()
        .with_size(400, 150)
        .with_label("Find")
        .center_screen();

    Frame::default().with_pos(20, 20).with_size(80, 30).with_label("Find what:");
    let find_input = Input::default().with_pos(110, 20).with_size(270, 30);

    let case_check = CheckButton::default()
        .with_pos(110, 60).with_size(200, 25).with_label("Match case");

    let mut find_prev_btn2 = Button::default()
        .with_pos(110, 100).with_size(90, 30).with_label("Find Prev");
    let mut find_btn = Button::default()
        .with_pos(210, 100).with_size(90, 30).with_label("Find Next");
    let mut close_btn = Button::default()
        .with_pos(310, 100).with_size(80, 30).with_label("Close");

    dialog_win.end();
    dialog_win.make_resizable(false);
    dialog_win.show();

    let state = FindState::new();

    // Find Next button (simple dialog)
    let st = state.search_text.clone();
    let sp = state.search_pos.clone();
    let mut tb1 = buffer.clone();
    let mut te1 = editor.clone();
    let find_input1 = find_input.clone();
    let case_check1 = case_check.clone();

    find_btn.set_callback(move |_| {
        let query = find_input1.value();
        if query.is_empty() {
            dialog::message_default("Please enter text to find");
            return;
        }
        FindState::find_next(st.clone(), sp.clone(), &query, &mut tb1, &mut te1, case_check1.is_checked());
    });

    // Find Previous button (simple dialog)
    let st2 = state.search_text.clone();
    let sp2 = state.search_pos.clone();
    let mut tb2 = buffer.clone();
    let mut te2 = editor.clone();
    let find_input2 = find_input.clone();
    let case_check2 = case_check.clone();

    find_prev_btn2.set_callback(move |_| {
        let query = find_input2.value();
        if query.is_empty() {
            dialog::message_default("Please enter text to find");
            return;
        }
        FindState::find_prev(st2.clone(), sp2.clone(), &query, &mut tb2, &mut te2, case_check2.is_checked());
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
