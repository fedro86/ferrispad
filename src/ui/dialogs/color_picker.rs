//! Themed color picker (T0080): FLTK's `ColorChooser` widget, a `#RRGGBB`
//! field and OK / Cancel, styled with [`DialogTheme`] like every other
//! FerrisPad dialog. FLTK's own `dialog::color_chooser` opens an unthemed,
//! standalone grey window, so it is not used.

use std::cell::RefCell;
use std::rc::Rc;

use fltk::{
    button::Button,
    enums::{CallbackTrigger, FrameType},
    frame::Frame,
    group::ColorChooser,
    input::Input,
    prelude::*,
    window::Window,
};

use super::DialogTheme;
use crate::app::controllers::theme_manager::{format_hex_color, parse_hex_color};

const DIALOG_WIDTH: i32 = 300;
const DIALOG_HEIGHT: i32 = 250;
const PADDING: i32 = 15;
const BUTTON_WIDTH: i32 = 80;
const BUTTON_HEIGHT: i32 = 28;

/// The message shown for text that is not a hex color.
const HEX_ERROR: &str = "Enter a color as #RRGGBB";

/// Show the picker starting at `initial`. Returns the chosen color on OK,
/// `None` on Cancel, Escape or the close button.
pub fn show_color_picker(
    theme_bg: (u8, u8, u8),
    title: &str,
    initial: (u8, u8, u8),
) -> Option<(u8, u8, u8)> {
    let theme = DialogTheme::from_theme_bg(theme_bg);

    // Built top-level even if another group is current (T0058).
    let top_level = super::TopLevelScope::new();
    let mut dialog = Window::default()
        .with_size(DIALOG_WIDTH, DIALOG_HEIGHT)
        .with_label(title)
        .center_screen();
    dialog.make_modal(true);
    dialog.set_color(theme.bg);

    let mut chooser = ColorChooser::new(PADDING, PADDING, DIALOG_WIDTH - PADDING * 2, 125, None);
    chooser.set_color(theme.bg);
    // The chooser's own parts (mode choice, value fields, hue/value boxes).
    for i in 0..chooser.children() {
        if let Some(mut part) = chooser.child(i) {
            part.set_color(theme.input_bg);
            part.set_selection_color(theme.button_bg);
            part.set_label_color(theme.text);
        }
    }
    chooser.end();
    let _ = chooser.set_tuple_rgb(initial);

    let hex_y = PADDING + 125 + 12;
    let mut hex_label = Frame::default()
        .with_pos(PADDING, hex_y)
        .with_size(40, 26)
        .with_label("Hex:")
        .with_align(fltk::enums::Align::Left | fltk::enums::Align::Inside);
    hex_label.set_label_color(theme.text);
    let mut hex_input = Input::default()
        .with_pos(PADDING + 45, hex_y)
        .with_size(110, 26);
    hex_input.set_color(theme.input_bg);
    hex_input.set_text_color(theme.text);
    hex_input.set_selection_color(theme.button_bg);
    hex_input.set_value(&format_hex_color(initial));

    let mut error = Frame::default()
        .with_pos(PADDING, hex_y + 28)
        .with_size(DIALOG_WIDTH - PADDING * 2, 18)
        .with_align(fltk::enums::Align::Left | fltk::enums::Align::Inside);
    error.set_label_color(theme.error_color());
    error.set_label_size(12);

    let btn_y = DIALOG_HEIGHT - PADDING - BUTTON_HEIGHT;
    let mut cancel_btn = Button::default()
        .with_pos(DIALOG_WIDTH - PADDING - BUTTON_WIDTH, btn_y)
        .with_size(BUTTON_WIDTH, BUTTON_HEIGHT)
        .with_label("Cancel");
    cancel_btn.set_frame(FrameType::RFlatBox);
    cancel_btn.set_color(theme.secondary_button_bg);
    cancel_btn.set_label_color(theme.text_dim);
    let mut ok_btn = Button::default()
        .with_pos(DIALOG_WIDTH - PADDING * 2 - BUTTON_WIDTH * 2, btn_y)
        .with_size(BUTTON_WIDTH, BUTTON_HEIGHT)
        .with_label("OK");
    ok_btn.set_frame(FrameType::RFlatBox);
    ok_btn.set_color(theme.button_bg);
    ok_btn.set_label_color(theme.text);

    dialog.end();
    drop(top_level);

    let result: Rc<RefCell<Option<(u8, u8, u8)>>> = Rc::new(RefCell::new(None));

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

    dialog.show();
    theme.apply_titlebar(&dialog);
    super::run_dialog(&dialog);

    *result.borrow()
}
