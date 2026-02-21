use fltk::{
    app::Sender,
    button::{Button, CheckButton, RadioRoundButton},
    enums::Color,
    frame::Frame,
    group::Group,
    menu::Choice,
    prelude::*,
    window::Window,
};
use std::cell::RefCell;
use std::rc::Rc;

use crate::app::messages::Message;
use crate::app::session::SessionRestore;
use crate::app::settings::{AppSettings, FontChoice, SyntaxTheme, ThemeMode};
use crate::app::updater::UpdateChannel;

/// Show settings dialog and return updated settings if user clicked Save.
/// The sender is used to send live preview messages for theme changes.
pub fn show_settings_dialog(
    current_settings: &AppSettings,
    sender: &Sender<Message>,
    is_dark: bool,
) -> Option<AppSettings> {
    let mut dialog = Window::default()
        .with_size(350, 890)
        .with_label("Settings")
        .center_screen();
    dialog.make_modal(true);

    let vpack = Group::default()
        .with_size(320, 800)
        .with_pos(15, 15);

    // Theme section
    Frame::default().with_pos(15, 15).with_size(320, 25).with_label("Theme:").with_align(fltk::enums::Align::Left | fltk::enums::Align::Inside);
    let theme_group = Group::default().with_pos(30, 45).with_size(280, 75);
    let mut theme_light = RadioRoundButton::default().with_pos(30, 45).with_size(280, 25).with_label("Light");
    let mut theme_dark = RadioRoundButton::default().with_pos(30, 70).with_size(280, 25).with_label("Dark");
    let mut theme_system = RadioRoundButton::default().with_pos(30, 95).with_size(280, 25).with_label("System Default");
    theme_group.end();

    match current_settings.theme_mode {
        ThemeMode::Light => theme_light.set_value(true),
        ThemeMode::Dark => theme_dark.set_value(true),
        ThemeMode::SystemDefault => theme_system.set_value(true),
    }

    // Syntax Theme section
    Frame::default().with_pos(15, 130).with_size(320, 25).with_label("Syntax Theme (Light Mode):").with_align(fltk::enums::Align::Left | fltk::enums::Align::Inside);
    let mut theme_light_choice = Choice::default().with_pos(30, 155).with_size(280, 25);
    for theme in SyntaxTheme::all() {
        theme_light_choice.add_choice(theme.display_name());
    }
    theme_light_choice.set_value(theme_index(current_settings.syntax_theme_light));

    Frame::default().with_pos(15, 185).with_size(320, 25).with_label("Syntax Theme (Dark Mode):").with_align(fltk::enums::Align::Left | fltk::enums::Align::Inside);
    let mut theme_dark_choice = Choice::default().with_pos(30, 210).with_size(280, 25);
    for theme in SyntaxTheme::all() {
        theme_dark_choice.add_choice(theme.display_name());
    }
    theme_dark_choice.set_value(theme_index(current_settings.syntax_theme_dark));

    // Live preview callbacks for theme changes
    let sender_light = sender.clone();
    let is_dark_for_light = is_dark;
    theme_light_choice.set_callback(move |c| {
        if !is_dark_for_light {
            if let Some(theme) = index_to_theme(c.value()) {
                sender_light.send(Message::PreviewSyntaxTheme(theme));
            }
        }
    });

    let sender_dark = sender.clone();
    let is_dark_for_dark = is_dark;
    theme_dark_choice.set_callback(move |c| {
        if is_dark_for_dark {
            if let Some(theme) = index_to_theme(c.value()) {
                sender_dark.send(Message::PreviewSyntaxTheme(theme));
            }
        }
    });

    // Font section
    Frame::default().with_pos(15, 245).with_size(320, 25).with_label("Font:").with_align(fltk::enums::Align::Left | fltk::enums::Align::Inside);
    let font_group = Group::default().with_pos(30, 275).with_size(280, 75);
    let mut font_screenbold = RadioRoundButton::default().with_pos(30, 275).with_size(280, 25).with_label("Screen (Bold)");
    let mut font_courier = RadioRoundButton::default().with_pos(30, 300).with_size(280, 25).with_label("Courier");
    let mut font_helvetica = RadioRoundButton::default().with_pos(30, 325).with_size(280, 25).with_label("Helvetica Mono");
    font_group.end();

    match current_settings.font {
        FontChoice::ScreenBold => font_screenbold.set_value(true),
        FontChoice::Courier => font_courier.set_value(true),
        FontChoice::HelveticaMono => font_helvetica.set_value(true),
    }

    // Font size section
    Frame::default().with_pos(15, 360).with_size(320, 25).with_label("Font Size:").with_align(fltk::enums::Align::Left | fltk::enums::Align::Inside);
    let size_group = Group::default().with_pos(30, 390).with_size(280, 75);
    let mut size_12 = RadioRoundButton::default().with_pos(30, 390).with_size(280, 25).with_label("Small (12)");
    let mut size_16 = RadioRoundButton::default().with_pos(30, 415).with_size(280, 25).with_label("Medium (16)");
    let mut size_20 = RadioRoundButton::default().with_pos(30, 440).with_size(280, 25).with_label("Large (20)");
    size_group.end();

    match current_settings.font_size {
        12 => size_12.set_value(true),
        16 => size_16.set_value(true),
        20 => size_20.set_value(true),
        _ => size_16.set_value(true),
    }

    // View options section
    Frame::default().with_pos(15, 475).with_size(320, 25).with_label("View Options:").with_align(fltk::enums::Align::Left | fltk::enums::Align::Inside);
    let mut check_line_numbers = CheckButton::default().with_pos(30, 505).with_size(280, 25).with_label("Show Line Numbers");
    let mut check_word_wrap = CheckButton::default().with_pos(30, 530).with_size(280, 25).with_label("Word Wrap");

    let mut check_highlighting = CheckButton::default().with_pos(30, 555).with_size(280, 25).with_label("Syntax Highlighting");
    check_highlighting.set_value(current_settings.highlighting_enabled);

    let mut check_tabs_enabled = CheckButton::default().with_pos(30, 580).with_size(280, 25).with_label("Enable tabbed editing (requires restart)");
    check_tabs_enabled.set_value(current_settings.tabs_enabled);

    check_line_numbers.set_value(current_settings.line_numbers_enabled);
    check_word_wrap.set_value(current_settings.word_wrap_enabled);

    // Session restore section
    Frame::default().with_pos(15, 615).with_size(320, 25).with_label("Session Restore:").with_align(fltk::enums::Align::Left | fltk::enums::Align::Inside);
    let session_group = Group::default().with_pos(30, 645).with_size(280, 75);
    let mut session_off = RadioRoundButton::default().with_pos(30, 645).with_size(280, 25).with_label("Off");
    let mut session_saved = RadioRoundButton::default().with_pos(30, 670).with_size(280, 25).with_label("Saved Files Only");
    let mut session_full = RadioRoundButton::default().with_pos(30, 695).with_size(280, 25).with_label("Full (including unsaved)");
    session_group.end();

    match current_settings.session_restore {
        SessionRestore::Off => session_off.set_value(true),
        SessionRestore::SavedFiles => session_saved.set_value(true),
        SessionRestore::Full => session_full.set_value(true),
    }

    // Updates section
    Frame::default().with_pos(15, 730).with_size(320, 25).with_label("Updates:").with_align(fltk::enums::Align::Left | fltk::enums::Align::Inside);
    let mut check_auto_update = CheckButton::default().with_pos(30, 760).with_size(280, 25).with_label("Automatically check for updates");
    check_auto_update.set_value(current_settings.auto_check_updates);

    let mut check_prerelease = CheckButton::default().with_pos(30, 785).with_size(280, 25).with_label("Include pre-releases (beta/rc)");
    check_prerelease.set_value(current_settings.update_channel == UpdateChannel::Beta);

    // Info text
    let mut info_frame = Frame::default().with_pos(30, 815).with_size(290, 35);
    info_frame.set_label("FerrisPad checks GitHub once per day.\nNo personal data is sent.");
    info_frame.set_label_size(11);
    info_frame.set_label_color(Color::from_rgb(100, 100, 100));
    info_frame.set_align(fltk::enums::Align::Left | fltk::enums::Align::Inside | fltk::enums::Align::Wrap);

    vpack.end();

    // Buttons at bottom
    let mut save_btn = Button::default().with_pos(150, 855).with_size(90, 30).with_label("Save");
    let mut cancel_btn = Button::default().with_pos(250, 855).with_size(90, 30).with_label("Cancel");

    dialog.end();
    dialog.show();

    let result = Rc::new(RefCell::new(None));
    let result_save = result.clone();
    let result_cancel = result.clone();

    // Store original theme for reverting on cancel
    let original_theme = current_settings.current_syntax_theme(is_dark);
    let sender_cancel = sender.clone();

    let dialog_save = dialog.clone();
    let current = current_settings.clone();
    save_btn.set_callback(move |_| {
        let new_settings = AppSettings {
            theme_mode: if theme_light.value() {
                ThemeMode::Light
            } else if theme_dark.value() {
                ThemeMode::Dark
            } else {
                ThemeMode::SystemDefault
            },
            font: if font_screenbold.value() {
                FontChoice::ScreenBold
            } else if font_courier.value() {
                FontChoice::Courier
            } else {
                FontChoice::HelveticaMono
            },
            font_size: if size_12.value() {
                12
            } else if size_20.value() {
                20
            } else {
                16
            },
            line_numbers_enabled: check_line_numbers.value(),
            word_wrap_enabled: check_word_wrap.value(),
            highlighting_enabled: check_highlighting.value(),
            auto_check_updates: check_auto_update.value(),
            update_channel: if check_prerelease.value() {
                UpdateChannel::Beta
            } else {
                UpdateChannel::Stable
            },
            last_update_check: current.last_update_check,
            skipped_versions: current.skipped_versions.clone(),
            tabs_enabled: check_tabs_enabled.value(),
            session_restore: if session_saved.value() {
                SessionRestore::SavedFiles
            } else if session_full.value() {
                SessionRestore::Full
            } else {
                SessionRestore::Off
            },
            preview_enabled: current.preview_enabled,
            syntax_theme_light: index_to_theme(theme_light_choice.value()).unwrap_or(current.syntax_theme_light),
            syntax_theme_dark: index_to_theme(theme_dark_choice.value()).unwrap_or(current.syntax_theme_dark),
        };

        *result_save.borrow_mut() = Some(new_settings);
        dialog_save.clone().hide();
    });

    let dialog_cancel = dialog.clone();
    cancel_btn.set_callback(move |_| {
        // Revert to original theme
        sender_cancel.send(Message::PreviewSyntaxTheme(original_theme));
        *result_cancel.borrow_mut() = None;
        dialog_cancel.clone().hide();
    });

    let result_close = result.clone();
    let sender_close = sender.clone();
    dialog.set_callback(move |w| {
        // Revert to original theme on close (X button)
        sender_close.send(Message::PreviewSyntaxTheme(original_theme));
        *result_close.borrow_mut() = None;
        w.hide();
    });

    super::run_dialog(&dialog);

    result.borrow().clone()
}

/// Convert SyntaxTheme to dropdown index
fn theme_index(theme: SyntaxTheme) -> i32 {
    SyntaxTheme::all()
        .iter()
        .position(|t| *t == theme)
        .map(|i| i as i32)
        .unwrap_or(0)
}

/// Convert dropdown index to SyntaxTheme
fn index_to_theme(index: i32) -> Option<SyntaxTheme> {
    if index < 0 {
        return None;
    }
    SyntaxTheme::all().get(index as usize).copied()
}
