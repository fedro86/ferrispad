use fltk::{
    app,
    button::Button,
    enums::{Color, Font},
    frame::Frame,
    group::Flex,
    prelude::*,
    window::Window,
};

/// Show About dialog
pub fn show_about_dialog() {
    let version = env!("CARGO_PKG_VERSION");
    let mut dialog = Window::default()
        .with_size(450, 400)
        .with_label("About FerrisPad")
        .center_screen();
    dialog.make_modal(true);

    let mut flex = Flex::new(10, 10, 430, 380, None);
    flex.set_type(fltk::group::FlexType::Column);
    flex.set_spacing(10);

    // App icon/logo
    let mut title = Frame::default();
    title.set_label("🦀 FerrisPad");
    title.set_label_size(24);
    title.set_label_font(Font::HelveticaBold);
    flex.fixed(&title, 40);

    // Version
    let mut version_frame = Frame::default();
    version_frame.set_label(&format!("Version {}", version));
    version_frame.set_label_size(14);
    flex.fixed(&version_frame, 25);

    // Description
    let mut desc_frame = Frame::default();
    desc_frame.set_label("A blazingly fast, minimalist notepad written in Rust");
    desc_frame.set_label_size(12);
    desc_frame.set_label_color(Color::from_rgb(100, 100, 100));
    flex.fixed(&desc_frame, 25);

    // Spacing
    let mut _spacer1 = Frame::default();
    flex.fixed(&_spacer1, 10);

    // Info section
    let info_text = format!(
        "Copyright \u{00a9} 2025 FerrisPad Contributors\n\
         Licensed under the MIT License\n\n\
         Built with Rust \u{1f980} and FLTK\n\n\
         Website: www.ferrispad.com\n\
         GitHub: github.com/fedro86/ferrispad"
    );

    let mut info_frame = Frame::default();
    info_frame.set_label(&info_text);
    info_frame.set_label_size(12);
    info_frame.set_align(fltk::enums::Align::Center | fltk::enums::Align::Inside);
    flex.fixed(&info_frame, 120);

    // Spacing
    let mut _spacer2 = Frame::default();
    flex.fixed(&_spacer2, 10);

    // Credits
    let mut credits_frame = Frame::default();
    credits_frame.set_label("Made with \u{2764}\u{fe0f} by developers who believe\nsoftware should be fast and simple");
    credits_frame.set_label_size(11);
    credits_frame.set_label_color(Color::from_rgb(100, 100, 100));
    credits_frame.set_align(fltk::enums::Align::Center | fltk::enums::Align::Inside);
    flex.fixed(&credits_frame, 40);

    // Close button
    let mut close_btn = Button::default().with_label("Close");
    flex.fixed(&close_btn, 35);

    flex.end();
    dialog.end();

    let mut dialog_close = dialog.clone();
    close_btn.set_callback(move |_| {
        dialog_close.hide();
    });

    dialog.show();
    while dialog.shown() {
        app::wait();
    }
}
