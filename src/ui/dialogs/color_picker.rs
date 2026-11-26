//! Themed color picker (T0080): FLTK's `ColorChooser` widget, a `#RRGGBB`
//! field and OK / Cancel, styled with [`DialogTheme`] like every other
//! FerrisPad dialog. FLTK's own `dialog::color_chooser` opens an unthemed,
//! standalone grey window, so it is not used.

use std::cell::RefCell;
use std::rc::Rc;

use fltk::{
    button::Button, enums::CallbackTrigger, frame::Frame, group::ColorChooser, prelude::*,
    window::Window,
};

use super::{DialogTheme, label_width, max_label_width, style_dialog_button};
use crate::app::controllers::theme_manager::{format_hex_color, parse_hex_color};

// Heights are fixed; widths are measured from the text they show
// (`label_width`), with these minimums.
const MIN_DIALOG_WIDTH: i32 = 300;
const DIALOG_HEIGHT: i32 = 250;
const PADDING: i32 = 15;
const GAP: i32 = 8;
const MIN_BUTTON_WIDTH: i32 = 80;
const BUTTON_HEIGHT: i32 = 28;
const CHOOSER_HEIGHT: i32 = 125;
const ERROR_SIZE: i32 = 12;

/// The message shown for text that is not a hex color.
const HEX_ERROR: &str = "Enter a color as #RRGGBB";

/// Show the picker starting at `initial`. Returns the chosen color on OK,
/// `None` on Cancel, Escape or the close button.
pub fn show_color_picker(
    theme_bg: (u8, u8, u8),
    title: &str,
    initial: (u8, u8, u8),
) -> Option<(u8, u8, u8)> {
    let (mut dialog, result) = build_color_picker(theme_bg, title, initial);
    dialog.show();
    DialogTheme::from_theme_bg(theme_bg).apply_titlebar(&dialog);
    super::run_dialog(&dialog);
    *result.borrow()
}

/// The chosen color, set by OK.
pub type PickResult = Rc<RefCell<Option<(u8, u8, u8)>>>;

/// Build the picker with its callbacks wired, without showing it. For the UI
/// construction tests (`tests/ui_dialogs.rs`); [`show_color_picker`] is the
/// real entry point.
pub fn build_color_picker(
    theme_bg: (u8, u8, u8),
    title: &str,
    initial: (u8, u8, u8),
) -> (Window, PickResult) {
    let theme = DialogTheme::from_theme_bg(theme_bg);

    // ---- Measured layout ----
    let size = fltk::app::font_size();
    let hex_label_w = label_width("Hex:", size);
    let hex_input_w = max_label_width(&["#888888", "#dddddd", "#bbbbbb"], size) + 2 * GAP;
    let button_w = max_label_width(&["OK", "Cancel"], size).max(MIN_BUTTON_WIDTH);
    let dialog_w = (label_width(HEX_ERROR, ERROR_SIZE) + 2 * PADDING)
        .max(2 * PADDING + hex_label_w + GAP + hex_input_w)
        .max(3 * PADDING + 2 * button_w)
        .max(MIN_DIALOG_WIDTH);

    // Built top-level even if another group is current (T0058).
    let top_level = super::TopLevelScope::new();
    let mut dialog = Window::default()
        .with_size(dialog_w, DIALOG_HEIGHT)
        .with_label(title)
        .center_screen();
    dialog.make_modal(true);
    dialog.set_color(theme.bg);

    let mut chooser = ColorChooser::new(
        PADDING,
        PADDING,
        dialog_w - PADDING * 2,
        CHOOSER_HEIGHT,
        None,
    );
    theme.style_color_chooser(&mut chooser);
    chooser.end();
    let _ = chooser.set_tuple_rgb(initial);

    let hex_y = PADDING + CHOOSER_HEIGHT + 12;
    let mut hex_label = Frame::default()
        .with_pos(PADDING, hex_y)
        .with_size(hex_label_w, 26)
        .with_label("Hex:")
        .with_align(fltk::enums::Align::Left | fltk::enums::Align::Inside);
    hex_label.set_label_color(theme.text);
    let mut hex_input = theme
        .input()
        .with_pos(PADDING + hex_label_w + GAP, hex_y)
        .with_size(hex_input_w, 26);
    hex_input.set_value(&format_hex_color(initial));

    let mut error = Frame::default()
        .with_pos(PADDING, hex_y + 28)
        .with_size(dialog_w - PADDING * 2, 18)
        .with_align(fltk::enums::Align::Left | fltk::enums::Align::Inside);
    error.set_label_color(theme.error_color());
    error.set_label_size(ERROR_SIZE);

    let btn_y = DIALOG_HEIGHT - PADDING - BUTTON_HEIGHT;
    let mut cancel_btn = Button::default()
        .with_pos(dialog_w - PADDING - button_w, btn_y)
        .with_size(button_w, BUTTON_HEIGHT)
        .with_label("Cancel");
    style_dialog_button(&mut cancel_btn, &theme);
    let mut ok_btn = Button::default()
        .with_pos(dialog_w - PADDING * 2 - button_w * 2, btn_y)
        .with_size(button_w, BUTTON_HEIGHT)
        .with_label("OK");
    style_dialog_button(&mut ok_btn, &theme);

    dialog.end();
    drop(top_level);

    let result: PickResult = Rc::new(RefCell::new(None));

    // Chooser → hex field.
    let (mut hex_sync, mut error_sync) = (hex_input.clone(), error.clone());
    chooser.set_callback(move |c| {
        hex_sync.set_value(&format_hex_color(c.rgb_color()));
        error_sync.set_label("");
    });

    // Hex field → chooser, as the user types. Invalid text changes nothing.
    hex_input.set_trigger(CallbackTrigger::Changed);
    let (mut chooser_sync, mut error_hex) = (chooser.clone(), error.clone());
    hex_input.set_callback(move |input| match parse_hex_color(&input.value()) {
        Some(rgb) => {
            let _ = chooser_sync.set_tuple_rgb(rgb);
            error_hex.set_label("");
        }
        None => error_hex.set_label(HEX_ERROR),
    });

    let (result_ok, hex_ok, mut error_ok, mut dialog_ok) = (
        result.clone(),
        hex_input.clone(),
        error.clone(),
        dialog.clone(),
    );
    ok_btn.set_callback(move |_| match parse_hex_color(&hex_ok.value()) {
        Some(rgb) => {
            *result_ok.borrow_mut() = Some(rgb);
            dialog_ok.hide();
        }
        None => error_ok.set_label(HEX_ERROR),
    });

    let mut dialog_cancel = dialog.clone();
    cancel_btn.set_callback(move |_| dialog_cancel.hide());
    // Escape and the close button: no change.
    dialog.set_callback(|w| w.hide());

    (dialog, result)
}
