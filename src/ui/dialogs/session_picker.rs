use std::cell::RefCell;
use std::rc::Rc;

use fltk::{
    enums::CallbackTrigger,
    frame::Frame,
    prelude::{BrowserExt, GroupExt, InputExt, WidgetExt, WindowExt},
    window::Window,
};

use crate::app::services::session;

use super::{DialogTheme, label_width};

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

/// Height of the Sessions picker; its width follows the measured row.
const DH: i32 = 340;
/// Margin around the content and the minimum list width.
const MARGIN: i32 = 20;
const LIST_W: i32 = 320;
/// FLTK's default label size, which these labels use.
const LABEL_SIZE: i32 = 14;

/// The Sessions picker's window and widgets, built but not shown (a seam
/// for the layout tests).
pub struct SessionPickerUi {
    pub dialog_win: Window,
    pub browser: fltk::browser::HoldBrowser,
    pub new_input: fltk::input::Input,
    pub switch_btn: fltk::button::Button,
    pub new_win_btn: fltk::button::Button,
    pub delete_btn: fltk::button::Button,
    pub cancel_btn: fltk::button::Button,
    pub status: Frame,
}

/// Build the Sessions picker listing `sessions`, with `current_session`
/// selected.
pub fn build_session_picker(
    theme: &DialogTheme,
    current_session: &str,
    sessions: &[String],
) -> SessionPickerUi {
    // Regular xdg_toplevel (not modal) — on Wayland/GNOME, make_modal(true) on the
    // first invocation maps the surface before the decoration + modal-hint round-trip
    // completes, yielding an undecorated, ungrabbed window with no input.
    // Modality is enforced manually below by deactivating the parent while the dialog runs.
    // Built top-level even if another group is current (T0058).
    let top_level = super::TopLevelScope::new();
    // Measured layout: the list, the New row and the button row share one
    // width, wide enough for every label (also 2 pt larger).
    let gap = 5;
    let labels = ["Switch", "New Window", "Delete", "Cancel"];
    let needed: Vec<i32> = labels
        .iter()
        .map(|l| label_width(l, LABEL_SIZE).max(60))
        .collect();
    let row_w = (needed.iter().sum::<i32>() + gap * 3).max(LIST_W);
    let mut dialog_win = Window::default()
        .with_size(row_w + 2 * MARGIN, DH)
        .with_label("Sessions")
        .center_screen();
    dialog_win.set_color(theme.bg);

    // Title label
    let mut title = Frame::default()
        .with_pos(MARGIN, 10)
        .with_size(row_w, 25)
        .with_label("Switch or create a session:");
    title.set_label_color(theme.text);

    // Session list browser
    let mut browser = theme.list().with_pos(MARGIN, 40).with_size(row_w, 160);

    // Populate session list (use @C format code for text color on dark themes)
    let (tr, tg, tb) = theme.text_rgb();
    let color_code = ((tr as u32) << 24) | ((tg as u32) << 16) | ((tb as u32) << 8);
    let mut selected_line = 0i32;
    for (i, name) in sessions.iter().enumerate() {
        let label = if name == current_session {
            format!("@C{} {} (active)", color_code, name)
        } else {
            format!("@C{} {}", color_code, name)
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
    let new_label_w = label_width("New:", LABEL_SIZE);
    let mut new_label = Frame::default()
        .with_pos(MARGIN, 210)
        .with_size(new_label_w, 30)
        .with_label("New:");
    new_label.set_label_color(theme.text);
    let new_x = MARGIN + new_label_w + gap;
    let new_input = theme
        .input()
        .with_pos(new_x, 210)
        .with_size(MARGIN + row_w - new_x, 30);

    // The row spans exactly the list's width: it used to end 5 px past it.
    // Spare width goes to the buttons evenly, the remainder to the last.
    let btn_y = 255;
    let btn_h = 30;
    let spare = row_w - needed.iter().sum::<i32>() - gap * 3;
    let mut widths: Vec<i32> = needed.iter().map(|w| w + spare / 4).collect();
    widths[3] += spare % 4;
    let mut xs = vec![MARGIN];
    for i in 1..4 {
        xs.push(xs[i - 1] + widths[i - 1] + gap);
    }

    let switch_btn = theme
        .button()
        .with_pos(xs[0], btn_y)
        .with_size(widths[0], btn_h)
        .with_label("Switch");

    let new_win_btn = theme
        .button()
        .with_pos(xs[1], btn_y)
        .with_size(widths[1], btn_h)
        .with_label("New Window");

    let delete_btn = theme
        .button()
        .with_pos(xs[2], btn_y)
        .with_size(widths[2], btn_h)
        .with_label("Delete");

    let cancel_btn = theme
        .secondary_button()
        .with_pos(xs[3], btn_y)
        .with_size(widths[3], btn_h)
        .with_label("Cancel");

    // Status label for errors
    let mut status = Frame::default()
        .with_pos(MARGIN, 295)
        .with_size(row_w, 25)
        .with_label("");
    status.set_label_color(theme.error_color());

    dialog_win.end();
    drop(top_level);
    SessionPickerUi {
        dialog_win,
        browser,
        new_input,
        switch_btn,
        new_win_btn,
        delete_btn,
        cancel_btn,
        status,
    }
}

/// Show the session picker dialog.
/// `parent` is the main window used to center the dialog (reliable on Wayland).
/// `current_session` is the name of the currently active session (highlighted in the list).
pub fn show_session_picker(
    parent: &Window,
    current_session: &str,
    theme_bg: (u8, u8, u8),
) -> SessionPickerResult {
    let theme = DialogTheme::from_theme_bg(theme_bg);
    let result: Rc<RefCell<SessionPickerResult>> =
        Rc::new(RefCell::new(SessionPickerResult::Cancelled));

    let sessions = session::list_sessions();
    let SessionPickerUi {
        mut dialog_win,
        mut browser,
        mut new_input,
        mut switch_btn,
        mut new_win_btn,
        mut delete_btn,
        mut cancel_btn,
        status,
    } = build_session_picker(&theme, current_session, &sessions);
    dialog_win.make_resizable(false);
    dialog_win.show();
    theme.apply_titlebar(&dialog_win);
    // Reposition to center on the parent AFTER show().
    dialog_win.resize(
        parent.x() + (parent.w() - dialog_win.w()) / 2,
        parent.y() + (parent.h() - DH) / 2,
        dialog_win.w(),
        DH,
    );

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

/// Size of the New Session dialog; it widens to fit the measured label.
const NEW_SESSION_DW: i32 = 300;
const NEW_SESSION_DH: i32 = 120;

/// The New Session dialog's window and widgets, built but not shown (a seam
/// for the layout tests).
pub struct NewSessionUi {
    pub dialog_win: Window,
    pub name_input: fltk::input::Input,
    pub status: Frame,
    pub ok_btn: fltk::button::Button,
    pub cancel_btn: fltk::button::Button,
}

/// Build the New Session dialog.
pub fn build_new_session_dialog(theme: &DialogTheme) -> NewSessionUi {
    const DW: i32 = NEW_SESSION_DW;
    const DH: i32 = NEW_SESSION_DH;
    // Regular xdg_toplevel (not modal) — see show_session_picker for rationale.
    // Built top-level even if another group is current (T0058).
    let top_level = super::TopLevelScope::new();
    // The label fits its text (also 2 pt larger); the input and the button
    // row move right by as much, the status line and the dialog widen by it.
    let label_w = label_width("Session name:", LABEL_SIZE).max(100);
    let shift = label_w - 100;
    let mut dialog_win = Window::default()
        .with_size(DW + shift, DH)
        .with_label("New Session")
        .center_screen();
    dialog_win.set_color(theme.bg);

    let mut label = Frame::default()
        .with_pos(20, 15)
        .with_size(label_w, 30)
        .with_label("Session name:");
    label.set_label_color(theme.text);

    let name_input = theme.input().with_pos(130 + shift, 15).with_size(150, 30);

    let mut status = Frame::default()
        .with_pos(20, 50)
        .with_size(260 + shift, 20)
        .with_label("");
    status.set_label_color(theme.error_color());

    let ok_btn = theme
        .button()
        .with_pos(100 + shift, 75)
        .with_size(80, 30)
        .with_label("Create");

    let cancel_btn = theme
        .button()
        .with_pos(190 + shift, 75)
        .with_size(80, 30)
        .with_label("Cancel");

    dialog_win.end();
    drop(top_level);
    NewSessionUi {
        dialog_win,
        name_input,
        status,
        ok_btn,
        cancel_btn,
    }
}

/// Show a small dialog that asks for a session name.
/// `parent` is the main window used to center the dialog (reliable on Wayland).
/// Returns `Some(sanitized_name)` or `None` if cancelled.
pub fn show_new_session_dialog(parent: &Window, theme_bg: (u8, u8, u8)) -> Option<String> {
    let theme = DialogTheme::from_theme_bg(theme_bg);
    let result: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));

    let NewSessionUi {
        mut dialog_win,
        mut name_input,
        status,
        mut ok_btn,
        mut cancel_btn,
    } = build_new_session_dialog(&theme);
    dialog_win.make_resizable(false);
    dialog_win.show();
    theme.apply_titlebar(&dialog_win);
    // Reposition to center on the parent AFTER show(), at the built width
    // (it follows the measured label).
    dialog_win.resize(
        parent.x() + (parent.w() - dialog_win.w()) / 2,
        parent.y() + (parent.h() - NEW_SESSION_DH) / 2,
        dialog_win.w(),
        NEW_SESSION_DH,
    );

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
