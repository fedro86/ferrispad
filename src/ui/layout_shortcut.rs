//! Ctrl+letter shortcuts when the keyboard layouts disagree (T0057).
//!
//! With several xkb layouts (e.g. `fr,us`) and a non-first layout active, FLTK
//! reports `event_key()` from the FIRST layout while `event_text()` comes from
//! the active one. FLTK's menu shortcut matching trusts `event_key()`, so on
//! an AZERTY-first system typing US, Ctrl+Z ran Ctrl+W (close tab) and Ctrl+A
//! ran Ctrl+Q (quit).
//!
//! The text is right: with Ctrl held it is the control character of the letter
//! the user typed (Ctrl+Z gives 0x1A). [`intended_shortcut`] recovers that
//! letter when the two disagree, and [`install`] hooks FLTK's event dispatch so
//! the intended command runs instead of the menu item `event_key()` matches.

use std::cell::Cell;
use std::ffi::{c_int, c_void};

use fltk::{
    app::{self, Sender},
    enums::{Event, Key, Shortcut},
    prelude::*,
};

use crate::app::Message;
use crate::app::services::shortcut_registry::normalize_shortcut;

/// What the dispatch hook needs, as plain values: it is a C callback and
/// cannot capture anything. Widget pointers are only compared, never followed.
#[derive(Clone, Copy)]
struct HookTarget {
    sender: Sender<Message>,
    window: usize,
    editor: usize,
}

thread_local! {
    static HOOK_TARGET: Cell<Option<HookTarget>> = const { Cell::new(None) };
}

/// Route Ctrl+letter key events whose layouts disagree to the intended
/// shortcut (`Message::RunShortcut`), before FLTK matches the wrong menu item.
///
/// `Fl::event_dispatch` is the only hook that runs before FLTK hands a key to
/// the focus widget and, for `FL_SHORTCUT`, to the menu bar: `Fl::handle`
/// calls it for every event, including the `FL_SHORTCUT` that `Fl::handle_`
/// raises when the focus widget declines an `FL_KEYBOARD`. A window `handle`
/// callback is too late (a group holding the menu bar can match first), and
/// `app::add_handler` only sees events nobody used.
pub fn install(sender: Sender<Message>, window: &impl WindowExt, editor: &impl WidgetExt) {
    HOOK_TARGET.set(Some(HookTarget {
        sender,
        window: window.as_widget_ptr() as usize,
        editor: editor.as_widget_ptr() as usize,
    }));
    // SAFETY: `dispatch` has exactly the C signature FLTK expects, does not
    // unwind (no panicking calls), and hands every event it does not consume
    // to `Fl::handle_`, FLTK's default dispatcher.
    unsafe { fltk_sys::fl::Fl_event_dispatch(Some(dispatch)) };
}

extern "C" fn dispatch(event: c_int, window: *mut c_void) -> c_int {
    if let Some((sender, shortcut)) = remapped_shortcut(event, window as usize) {
        sender.send(Message::RunShortcut(shortcut));
        return 1;
    }
    // SAFETY: `window` is the window FLTK is dispatching this event to,
    // passed through unchanged, exactly as FLTK does without a hook.
    unsafe { fltk_sys::fl::Fl_handle_(event, window) }
}

/// The intended shortcut for this event, when the hook should take it.
///
/// Only the main window's events, with no modal dialog or menu grab open
/// (those dispatch their own keys). `FL_SHORTCUT` events are taken whatever
/// widget has focus: by then the focus widget has declined the key, and the
/// next step is the menu's `event_key()` match. `FL_KEYBOARD` events are
/// taken only for the main editor, because `Fl_Text_Editor` matches its own
/// Ctrl+Z / Ctrl+A bindings by `event_key()` too (Ctrl+W would undo); other
/// widgets keep the key (the terminal reads `event_text()`, which is right).
fn remapped_shortcut(event: c_int, window: usize) -> Option<(Sender<Message>, String)> {
    let target = HOOK_TARGET.get()?;
    let key_stage = event == Event::KeyDown.bits();
    if !(key_stage || event == Event::Shortcut.bits()) || window != target.window {
        return None;
    }
    if app::modal().is_some() || app::grab().is_some() {
        return None;
    }
    if key_stage && app::focus().map(|w| w.as_widget_ptr() as usize) != Some(target.editor) {
        return None;
    }
    let state = app::event_state();
    let shortcut = intended_shortcut(
        app::event_key(),
        &app::event_text(),
        state.contains(Shortcut::Ctrl),
        state.contains(Shortcut::Shift),
        state.contains(Shortcut::Alt),
    )?;
    Some((target.sender, shortcut))
}

/// The shortcut the user meant, when the layouts disagree.
///
/// Returns the normalized shortcut (`normalize_shortcut` form, e.g.
/// `"ctrl+shift+z"`) when Ctrl is held, `event_text` is a single control
/// character 0x01–0x1A, and `event_key` is a character key that is not the
/// letter that control character stands for. Otherwise `None`: the event is
/// left to FLTK's normal handling.
pub fn intended_shortcut(
    event_key: Key,
    event_text: &str,
    ctrl: bool,
    shift: bool,
    alt: bool,
) -> Option<String> {
    if !ctrl || is_named_key(event_key) {
        return None;
    }
    let &[control @ 0x01..=0x1a] = event_text.as_bytes() else {
        return None;
    };
    let letter = char::from(b'a' + (control - 1));
    let reported = u32::try_from(event_key.bits())
        .ok()
        .and_then(char::from_u32)
        .map(|c| c.to_ascii_lowercase());
    if reported == Some(letter) {
        return None; // the layouts agree: FLTK already matches the right key
    }
    let mut shortcut = String::from("ctrl+");
    if shift {
        shortcut.push_str("shift+");
    }
    if alt {
        shortcut.push_str("alt+");
    }
    shortcut.push(letter);
    Some(normalize_shortcut(&shortcut))
}

/// FLTK's non-character keys (Enter, Tab, BackSpace, keypad, F-keys,
/// modifiers, multimedia keys) live in 0xef00..=0xffff. With Ctrl they can
/// carry a control character too (Ctrl+Enter gives CR), but they are not
/// Ctrl+letter.
fn is_named_key(key: Key) -> bool {
    (0xef00..=0xffff).contains(&key.bits())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctrl(key: char, text: &str) -> Option<String> {
        intended_shortcut(Key::from_char(key), text, true, false, false)
    }

    #[test]
    fn ctrl_m_on_azerty_first_layout_is_ctrl_m() {
        // Diagnostic log: key=0x2c(',') text="\r".
        assert_eq!(ctrl(',', "\r").as_deref(), Some("ctrl+m"));
    }

    #[test]
    fn ctrl_z_reported_as_w_is_ctrl_z() {
        assert_eq!(ctrl('w', "\x1a").as_deref(), Some("ctrl+z"));
    }

    #[test]
    fn ctrl_a_reported_as_q_is_ctrl_a() {
        assert_eq!(ctrl('q', "\x01").as_deref(), Some("ctrl+a"));
    }

    #[test]
    fn ctrl_w_reported_as_z_is_ctrl_w() {
        assert_eq!(ctrl('z', "\x17").as_deref(), Some("ctrl+w"));
    }

    #[test]
    fn ctrl_shift_z_keeps_shift() {
        assert_eq!(
            intended_shortcut(Key::from_char('w'), "\x1a", true, true, false).as_deref(),
            Some("ctrl+shift+z")
        );
    }

    #[test]
    fn ctrl_alt_keeps_alt() {
        assert_eq!(
            intended_shortcut(Key::from_char('w'), "\x1a", true, false, true).as_deref(),
            Some("alt+ctrl+z")
        );
    }

    #[test]
    fn matching_key_is_left_alone() {
        // Single layout, or a genuine AZERTY typist: key and text agree.
        assert_eq!(ctrl('z', "\x1a"), None);
        assert_eq!(ctrl('m', "\r"), None);
        assert_eq!(ctrl('a', "\x01"), None);
        assert_eq!(
            intended_shortcut(Key::from_char('z'), "\x1a", true, true, false),
            None
        );
    }

    #[test]
    fn no_control_character_is_left_alone() {
        assert_eq!(ctrl('1', "1"), None);
        assert_eq!(ctrl(',', ","), None);
        assert_eq!(ctrl(',', ""), None);
        assert_eq!(ctrl('[', "\x1b"), None); // Ctrl+[ is ESC, not a letter
        assert_eq!(ctrl(',', "\r\r"), None);
    }

    #[test]
    fn without_ctrl_is_left_alone() {
        assert_eq!(
            intended_shortcut(Key::from_char(','), "\r", false, false, false),
            None
        );
    }

    #[test]
    fn named_keys_are_left_alone() {
        // Ctrl+Enter / Ctrl+Tab / Ctrl+BackSpace carry CR / TAB / BS text but
        // are not Ctrl+M / Ctrl+I / Ctrl+H.
        for (key, text) in [
            (Key::Enter, "\r"),
            (Key::KPEnter, "\r"),
            (Key::Tab, "\t"),
            (Key::BackSpace, "\x08"),
        ] {
            assert_eq!(
                intended_shortcut(key, text, true, false, false),
                None,
                "{key:?}"
            );
        }
    }
}
