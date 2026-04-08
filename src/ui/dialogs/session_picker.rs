use std::cell::RefCell;
use std::rc::Rc;

use fltk::{
    browser::HoldBrowser,
    button::Button,
    enums::{CallbackTrigger, FrameType},
    frame::Frame,
    input::Input,
    prelude::{BrowserExt, GroupExt, InputExt, WidgetExt, WindowExt},
    window::Window,
};

use crate::app::services::session;

use super::DialogTheme;

/// Result of the session picker dialog.
#[derive(Debug, Clone)]
pub enum SessionPickerResult {
    /// Switch the current instance to this session.
    Switch(String),
    /// Open this session in a new window.
    NewWindow(String),
    /// Delete this session.
    Delete(String),
    /// User cancelled.
    Cancelled,
}

/// Show the session picker dialog.
/// `current_session` is the name of the currently active session (highlighted in the list).
pub fn show_session_picker(current_session: &str, theme_bg: (u8, u8, u8)) -> SessionPickerResult {
    let theme = DialogTheme::from_theme_bg(theme_bg);
    let result: Rc<RefCell<SessionPickerResult>> =
        Rc::new(RefCell::new(SessionPickerResult::Cancelled));

    let mut dialog_win = Window::default()
        .with_size(360, 340)
        .with_label("Sessions")
        .center_screen();
    dialog_win.set_color(theme.bg);

    // Title label
    let mut title = Frame::default()
        .with_pos(20, 10)
        .with_size(320, 25)
        .with_label("Switch or create a session:");
    title.set_label_color(theme.text);

    // Session list browser
    let mut browser = HoldBrowser::default().with_pos(20, 40).with_size(320, 160);
    browser.set_color(theme.input_bg);
    browser.set_selection_color(theme.button_bg);
    browser.set_frame(FrameType::FlatBox);

    // Populate session list
    let sessions = session::list_sessions();
    let mut selected_line = 0i32;
    for (i, name) in sessions.iter().enumerate() {
        let label = if name == current_session {
            format!("{} (active)", name)
        } else {
            name.clone()
        };
        browser.add(&label);
        if name == current_session {
            selected_line = (i + 1) as i32; // FLTK browsers are 1-indexed
        }
    }
    if selected_line > 0 {
        browser.select(selected_line);
    }

    // New session input
    let mut new_label = Frame::default()
        .with_pos(20, 210)
        .with_size(40, 30)
        .with_label("New:");
    new_label.set_label_color(theme.text);
    let mut new_input = Input::default().with_pos(65, 210).with_size(275, 30);
    new_input.set_color(theme.input_bg);
    new_input.set_text_color(theme.text);
    new_input.set_selection_color(theme.button_bg);

    // Buttons row
    let btn_y = 255;
    let btn_h = 30;
    let btn_w = 100;

    let mut switch_btn = Button::default()
        .with_pos(20, btn_y)
        .with_size(btn_w, btn_h)
        .with_label("Switch");
    switch_btn.set_color(theme.button_bg);
    switch_btn.set_label_color(theme.text);

    let mut new_win_btn = Button::default()
        .with_pos(125, btn_y)
        .with_size(btn_w, btn_h)
        .with_label("New Window");
    new_win_btn.set_color(theme.button_bg);
    new_win_btn.set_label_color(theme.text);

    let mut delete_btn = Button::default()
        .with_pos(230, btn_y)
        .with_size(55, btn_h)
        .with_label("Delete");
    delete_btn.set_color(theme.button_bg);
    delete_btn.set_label_color(theme.text);

    let mut cancel_btn = Button::default()
        .with_pos(290, btn_y)
        .with_size(55, btn_h)
        .with_label("Cancel");
    cancel_btn.set_color(theme.button_bg);
    cancel_btn.set_label_color(theme.text);

    // Status label for errors
    let mut status = Frame::default()
        .with_pos(20, 295)
        .with_size(320, 25)
        .with_label("");
    status.set_label_color(theme.error_color());

    dialog_win.end();
    dialog_win.make_resizable(false);
    dialog_win.show();
    theme.apply_titlebar(&dialog_win);

    // Helper: get the selected session name from either browser selection or new input
    let get_target_name = {
        let sessions = sessions.clone();
        let new_input = new_input.clone();
        let browser = browser.clone();
        move || -> Option<String> {
            let new_text = new_input.value().trim().to_string();
            if !new_text.is_empty() {
                session::sanitize_session_name(&new_text)
            } else {
                let sel = browser.value();
                if sel > 0 && (sel as usize) <= sessions.len() {
                    Some(sessions[(sel - 1) as usize].clone())
                } else {
                    None
                }
            }
        }
    };

    // Switch button
    {
        let result = result.clone();
        let dialog = dialog_win.clone();
        let get_name = get_target_name.clone();
        let current = current_session.to_string();
        let mut status = status.clone();
        switch_btn.set_callback(move |_| {
            if let Some(name) = get_name() {
                if name == current {
                    status.set_label("Already on this session.");
                    return;
                }
                if session::session_is_locked(&name) {
                    status.set_label("Session is open in another window.");
                    return;
                }
                *result.borrow_mut() = SessionPickerResult::Switch(name);
                dialog.clone().hide();
            } else {
                status.set_label("Select or type a session name.");
            }
        });
    }

    // New Window button
    {
        let result = result.clone();
        let dialog = dialog_win.clone();
        let get_name = get_target_name.clone();
        let mut status = status.clone();
        new_win_btn.set_callback(move |_| {
            if let Some(name) = get_name() {
                if session::session_is_locked(&name) {
                    status.set_label("Session is open in another window.");
                    return;
                }
                *result.borrow_mut() = SessionPickerResult::NewWindow(name);
                dialog.clone().hide();
            } else {
                status.set_label("Select or type a session name.");
            }
        });
    }

    // Delete button
    {
        let result = result.clone();
        let dialog = dialog_win.clone();
        let get_name = get_target_name.clone();
        let current = current_session.to_string();
        let mut status = status.clone();
        delete_btn.set_callback(move |_| {
            if let Some(name) = get_name() {
                if name == current {
                    status.set_label("Cannot delete the active session.");
                    return;
                }
                if name == session::DEFAULT_SESSION_NAME {
                    status.set_label("Cannot delete the default session.");
                    return;
                }
                *result.borrow_mut() = SessionPickerResult::Delete(name);
                dialog.clone().hide();
            } else {
                status.set_label("Select a session to delete.");
            }
        });
    }

    // Cancel button
    {
        let dialog = dialog_win.clone();
        cancel_btn.set_callback(move |_| {
            dialog.clone().hide();
        });
    }

    // Enter key on input triggers Switch
    {
        let mut switch_btn = switch_btn.clone();
        new_input.set_trigger(CallbackTrigger::EnterKey);
        new_input.set_callback(move |_| {
            switch_btn.do_callback();
        });
    }

    // Double-click on browser triggers Switch
    {
        let mut switch_btn = switch_btn.clone();
        browser.set_callback(move |b| {
            if fltk::app::event_clicks() && b.value() > 0 {
                switch_btn.do_callback();
            }
        });
    }

    // Window close
    {
        let dialog = dialog_win.clone();
        dialog_win.set_callback(move |_| {
            dialog.clone().hide();
        });
    }

    super::run_dialog(&dialog_win);

    result.borrow().clone()
}

/// Show a small dialog that asks for a session name.
/// Returns `Some(sanitized_name)` or `None` if cancelled.
pub fn show_new_session_dialog(theme_bg: (u8, u8, u8)) -> Option<String> {
    let theme = DialogTheme::from_theme_bg(theme_bg);
    let result: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));

    let mut dialog_win = Window::default()
        .with_size(300, 120)
        .with_label("New Session")
        .center_screen();
    dialog_win.set_color(theme.bg);

    let mut label = Frame::default()
        .with_pos(20, 15)
        .with_size(100, 30)
        .with_label("Session name:");
    label.set_label_color(theme.text);

    let mut name_input = Input::default().with_pos(130, 15).with_size(150, 30);
    name_input.set_color(theme.input_bg);
    name_input.set_text_color(theme.text);
    name_input.set_selection_color(theme.button_bg);

    let mut status = Frame::default()
        .with_pos(20, 50)
        .with_size(260, 20)
        .with_label("");
    status.set_label_color(theme.error_color());

    let mut ok_btn = Button::default()
        .with_pos(100, 75)
        .with_size(80, 30)
        .with_label("Create");
    ok_btn.set_color(theme.button_bg);
    ok_btn.set_label_color(theme.text);

    let mut cancel_btn = Button::default()
        .with_pos(190, 75)
        .with_size(80, 30)
        .with_label("Cancel");
    cancel_btn.set_color(theme.button_bg);
    cancel_btn.set_label_color(theme.text);

    dialog_win.end();
    dialog_win.make_resizable(false);
    dialog_win.show();
    theme.apply_titlebar(&dialog_win);

    // OK button
    {
        let result = result.clone();
        let dialog = dialog_win.clone();
        let name_input = name_input.clone();
        let mut status = status.clone();
        ok_btn.set_callback(move |_| {
            let raw = name_input.value();
            if let Some(name) = session::sanitize_session_name(raw.trim()) {
                *result.borrow_mut() = Some(name);
                dialog.clone().hide();
            } else {
                status.set_label("Invalid name (use letters, numbers, -)");
            }
        });
    }

    // Enter key triggers OK
    {
        let mut ok_btn = ok_btn.clone();
        name_input.set_trigger(CallbackTrigger::EnterKey);
        name_input.set_callback(move |_| {
            ok_btn.do_callback();
        });
    }

    // Cancel
    {
        let dialog = dialog_win.clone();
        cancel_btn.set_callback(move |_| {
            dialog.clone().hide();
        });
    }

    {
        let dialog = dialog_win.clone();
        dialog_win.set_callback(move |_| {
            dialog.clone().hide();
        });
    }

    super::run_dialog(&dialog_win);

    result.borrow().clone()
}
