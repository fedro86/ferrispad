use fltk::{
    enums::{Color, Font, FrameType},
    frame::Frame,
    group::Flex,
    prelude::*,
    window::Window,
};

use super::DialogTheme;

/// The About dialog's window and widgets, built but not shown (a seam for
/// the layout tests).
pub struct AboutUi {
    pub dialog: Window,
    pub close_btn: fltk::button::Button,
}

/// Build the About dialog. `theme_bg` is the editor background, which the
/// dialog uses as is (not the derived `theme.bg`).
pub fn build_about_dialog(theme: &DialogTheme, theme_bg: (u8, u8, u8)) -> AboutUi {
    let version = env!("CARGO_PKG_VERSION");

    // Use original editor background for dialog (not derived/darkened)
    let dialog_bg = Color::from_rgb(theme_bg.0, theme_bg.1, theme_bg.2);

    // Built top-level even if another group is current (T0058).
    let top_level = super::TopLevelScope::new();
    let mut dialog = Window::default()
        .with_size(450, 330)
        .with_label("About FerrisPad")
        .center_screen();
    dialog.set_color(dialog_bg);

    let mut flex = Flex::new(10, 10, 430, 310, None);
    flex.set_type(fltk::group::FlexType::Column);
    flex.set_spacing(10);
    flex.set_color(dialog_bg);

    // App icon/logo
    let mut title = Frame::default();
    title.set_label("FerrisPad");
    title.set_label_size(24);
    title.set_label_font(Font::HelveticaBold);
    title.set_label_color(theme.text);
    title.set_frame(FrameType::FlatBox);
    title.set_color(dialog_bg);
    flex.fixed(&title, 40);

    // Version
    let mut version_frame = Frame::default();
    version_frame.set_label(&format!("Version {}", version));
    version_frame.set_label_size(14);
    version_frame.set_label_color(theme.text);
    version_frame.set_frame(FrameType::FlatBox);
    version_frame.set_color(dialog_bg);
    flex.fixed(&version_frame, 25);

    // Description
    let mut desc_frame = Frame::default();
    desc_frame.set_label("A minimalist text editor written in Rust");
    desc_frame.set_label_size(12);
    desc_frame.set_label_color(theme.text_dim);
    desc_frame.set_frame(FrameType::FlatBox);
    desc_frame.set_color(dialog_bg);
    flex.fixed(&desc_frame, 25);

    // Spacing
    let mut _spacer1 = Frame::default();
    _spacer1.set_frame(FrameType::FlatBox);
    _spacer1.set_color(dialog_bg);
    flex.fixed(&_spacer1, 10);

    // Info section
    let info_text = "Copyright \u{00a9} 2025 FerrisPad Contributors\n\
         Licensed under the MIT License\n\n\
         Built with Rust, FLTK, and Lua\n\n\
         Website: www.ferrispad.com\n\
         GitHub: github.com/fedro86/ferrispad"
        .to_string();

    let mut info_frame = Frame::default();
    info_frame.set_label(&info_text);
    info_frame.set_label_size(12);
    info_frame.set_label_color(theme.text);
    info_frame.set_align(fltk::enums::Align::Center | fltk::enums::Align::Inside);
    info_frame.set_frame(FrameType::FlatBox);
    info_frame.set_color(dialog_bg);
    flex.fixed(&info_frame, 120);

    // Close button
    let close_btn = theme.button().with_label("Close");
    flex.fixed(&close_btn, 35);

    flex.end();
    dialog.end();
    drop(top_level);
    AboutUi { dialog, close_btn }
}

/// Show About dialog
pub fn show_about_dialog(theme_bg: (u8, u8, u8)) {
    let theme = DialogTheme::from_theme_bg(theme_bg);

    let AboutUi {
        mut dialog,
        mut close_btn,
    } = build_about_dialog(&theme, theme_bg);
    // Only a flag until show(): setting it after the build draws the same.
    dialog.make_modal(true);

    let mut dialog_close = dialog.clone();
    close_btn.set_callback(move |_| {
        dialog_close.hide();
    });

    dialog.show();
    theme.apply_titlebar(&dialog);
    super::run_dialog(&dialog);
}
