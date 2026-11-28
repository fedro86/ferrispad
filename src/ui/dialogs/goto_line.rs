use fltk::{
    enums::CallbackTrigger,
    frame::Frame,
    prelude::*,
    text::{TextBuffer, TextEditor},
    window::Window,
};

use crate::app::buffer_text_no_leak;
use crate::app::services::text_ops::line_number_to_byte_position;

use super::{DialogTheme, label_width};

/// FLTK's default label size, which the label uses.
const LABEL_SIZE: i32 = 14;

/// The Go To Line dialog's window and widgets, built but not shown (a seam
/// for the layout tests).
pub struct GotoLineUi {
    pub dialog_win: Window,
    pub line_input: fltk::input::IntInput,
    pub go_btn: fltk::button::Button,
    pub cancel_btn: fltk::button::Button,
}

/// Build the Go To Line dialog.
pub fn build_goto_line_dialog(theme: &DialogTheme) -> GotoLineUi {
    // Built top-level even if another group is current (T0058).
    let top_level = super::TopLevelScope::new();
    // The label fits its text (also 2 pt larger); the input and the right
    // aligned button row move right by as much, the dialog widens by it.
    let label_w = label_width("Line number:", LABEL_SIZE).max(100);
    let shift = label_w - 100;
    let mut dialog_win = Window::default()
        .with_size(250 + shift, 120)
        .with_label("Go To Line")
        .center_screen();
    dialog_win.set_color(theme.bg);
    let mut label = Frame::default()
        .with_pos(20, 20)
        .with_size(label_w, 30)
        .with_label("Line number:");
    label.set_label_color(theme.text);
    let line_input = theme
        .int_input()
        .with_pos(130 + shift, 20)
        .with_size(100, 30);

    let go_btn = theme
        .button()
        .with_pos(60 + shift, 70)
        .with_size(80, 30)
        .with_label("Go");
    let cancel_btn = theme
        .button()
        .with_pos(150 + shift, 70)
        .with_size(80, 30)
        .with_label("Cancel");

    dialog_win.end();
    drop(top_level);
    GotoLineUi {
        dialog_win,
        line_input,
        go_btn,
        cancel_btn,
    }
}

/// Show Go To Line dialog
pub fn show_goto_line_dialog(buffer: &TextBuffer, editor: &mut TextEditor, theme_bg: (u8, u8, u8)) {
    let theme = DialogTheme::from_theme_bg(theme_bg);

    let GotoLineUi {
        mut dialog_win,
        mut line_input,
        mut go_btn,
        mut cancel_btn,
    } = build_goto_line_dialog(&theme);
    dialog_win.make_resizable(false);
    dialog_win.show();
    theme.apply_titlebar(&dialog_win);

    let mut tb = buffer.clone();
    let mut te = editor.clone();
    let dialog_go = dialog_win.clone();
    let line_input_go = line_input.clone();

    go_btn.set_callback(move |_| {
        let input_val = line_input_go.value();
        let line_num: usize = match input_val.trim().parse() {
            Ok(n) => n,
            Err(_) => {
                super::show_themed_message(
                    theme_bg,
                    "Go To Line",
                    "Please enter a valid line number",
                );
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
            super::show_themed_message(
                theme_bg,
                "Go To Line",
                &format!("Line number must be between 1 and {}", total_lines),
            );
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

    super::run_dialog(&dialog_win);
}
