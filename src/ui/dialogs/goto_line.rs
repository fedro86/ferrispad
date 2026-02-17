use fltk::{
    app,
    button::Button,
    dialog,
    enums::CallbackTrigger,
    frame::Frame,
    input::IntInput,
    prelude::*,
    text::{TextBuffer, TextEditor},
    window::Window,
};

use crate::app::state::buffer_text_no_leak;
use crate::app::text_ops::line_number_to_byte_position;

/// Show Go To Line dialog
pub fn show_goto_line_dialog(buffer: &TextBuffer, editor: &mut TextEditor) {
    let mut dialog_win = Window::default()
        .with_size(250, 120)
        .with_label("Go To Line")
        .center_screen();
    Frame::default().with_pos(20, 20).with_size(100, 30).with_label("Line number:");
    let mut line_input = IntInput::default().with_pos(130, 20).with_size(100, 30);

    let mut go_btn = Button::default()
        .with_pos(60, 70).with_size(80, 30).with_label("Go");
    let mut cancel_btn = Button::default()
        .with_pos(150, 70).with_size(80, 30).with_label("Cancel");

    dialog_win.end();
    dialog_win.make_resizable(false);
    dialog_win.show();

    let mut tb = buffer.clone();
    let mut te = editor.clone();
    let dialog_go = dialog_win.clone();
    let line_input_go = line_input.clone();

    go_btn.set_callback(move |_| {
        let input_val = line_input_go.value();
        let line_num: usize = match input_val.trim().parse() {
            Ok(n) => n,
            Err(_) => {
                dialog::message_default("Please enter a valid line number");
                return;
            }
        };

        let text = buffer_text_no_leak(&tb);
        let total_lines = text.chars().filter(|c| *c == '\n').count() + 1;

        if let Some(pos) = line_number_to_byte_position(&text, line_num) {
            tb.unselect();
            te.set_insert_position(pos as i32);
            te.show_insert_position();
            dialog_go.clone().hide();
        } else {
            dialog::message_default(&format!(
                "Line number must be between 1 and {}", total_lines
            ));
        }
    });

    // Enter key on input triggers Go
    let mut go_btn2 = go_btn.clone();
    line_input.set_trigger(CallbackTrigger::EnterKey);
    line_input.set_callback(move |_| {
        go_btn2.do_callback();
    });

    let dialog_close = dialog_win.clone();
    cancel_btn.set_callback(move |_| {
        dialog_close.clone().hide();
    });

    let dialog_x = dialog_win.clone();
    dialog_win.set_callback(move |_| {
        dialog_x.clone().hide();
    });

    while dialog_win.shown() {
        app::wait();
    }
}
