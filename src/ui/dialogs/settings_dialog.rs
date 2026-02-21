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

use crate::app::{AppSettings, FontChoice, Message, SessionRestore, SyntaxTheme, ThemeMode, UpdateChannel};

// Layout constants
const DIALOG_WIDTH: i32 = 620;
const DIALOG_HEIGHT: i32 = 580;
const COL_WIDTH: i32 = 280;
const LEFT_COL: i32 = 15;
const RIGHT_COL: i32 = 320;
const LABEL_HEIGHT: i32 = 20;
const ITEM_HEIGHT: i32 = 22;
const SECTION_GAP: i32 = 15;

/// Show settings dialog and return updated settings if user clicked Save.
/// The sender is used to send live preview messages for theme changes.
pub fn show_settings_dialog(
    current_settings: &AppSettings,
    sender: &Sender<Message>,
    is_dark: bool,
) -> Option<AppSettings> {
    let mut dialog = Window::default()
        .with_size(DIALOG_WIDTH, DIALOG_HEIGHT)
        .with_label("Settings")
        .center_screen();
    dialog.make_modal(true);

    let vpack = Group::default()
        .with_size(DIALOG_WIDTH - 20, DIALOG_HEIGHT - 60)
        .with_pos(10, 10);

    // ============ LEFT COLUMN - Appearance ============
    let mut y = 15;

    // Theme section
    Frame::default().with_pos(LEFT_COL, y).with_size(COL_WIDTH, LABEL_HEIGHT)
        .with_label("Theme:")
        .with_align(fltk::enums::Align::Left | fltk::enums::Align::Inside);
    y += LABEL_HEIGHT + 5;

    let theme_group = Group::default().with_pos(LEFT_COL + 10, y).with_size(COL_WIDTH - 10, ITEM_HEIGHT * 3);
    let mut theme_light = RadioRoundButton::default().with_pos(LEFT_COL + 10, y).with_size(COL_WIDTH - 10, ITEM_HEIGHT).with_label("Light");
    let mut theme_dark = RadioRoundButton::default().with_pos(LEFT_COL + 10, y + ITEM_HEIGHT).with_size(COL_WIDTH - 10, ITEM_HEIGHT).with_label("Dark");
    let mut theme_system = RadioRoundButton::default().with_pos(LEFT_COL + 10, y + ITEM_HEIGHT * 2).with_size(COL_WIDTH - 10, ITEM_HEIGHT).with_label("System Default");
    theme_group.end();
    y += ITEM_HEIGHT * 3 + SECTION_GAP;

    match current_settings.theme_mode {
        ThemeMode::Light => theme_light.set_value(true),
        ThemeMode::Dark => theme_dark.set_value(true),
        ThemeMode::SystemDefault => theme_system.set_value(true),
    }

    // Syntax Theme (Light)
    Frame::default().with_pos(LEFT_COL, y).with_size(COL_WIDTH, LABEL_HEIGHT)
        .with_label("Syntax Theme (Light):")
        .with_align(fltk::enums::Align::Left | fltk::enums::Align::Inside);
    y += LABEL_HEIGHT + 3;

    let mut theme_light_choice = Choice::default().with_pos(LEFT_COL + 10, y).with_size(COL_WIDTH - 20, 25);
    for theme in SyntaxTheme::all() {
        theme_light_choice.add_choice(theme.display_name());
    }
    theme_light_choice.set_value(theme_index(current_settings.syntax_theme_light));
    y += 30;

    // Syntax Theme (Dark)
    Frame::default().with_pos(LEFT_COL, y).with_size(COL_WIDTH, LABEL_HEIGHT)
        .with_label("Syntax Theme (Dark):")
        .with_align(fltk::enums::Align::Left | fltk::enums::Align::Inside);
    y += LABEL_HEIGHT + 3;

    let mut theme_dark_choice = Choice::default().with_pos(LEFT_COL + 10, y).with_size(COL_WIDTH - 20, 25);
    for theme in SyntaxTheme::all() {
        theme_dark_choice.add_choice(theme.display_name());
    }
    theme_dark_choice.set_value(theme_index(current_settings.syntax_theme_dark));
    y += 30 + SECTION_GAP;

    // Live preview callbacks for theme changes
    let sender_light = *sender;
    let is_dark_for_light = is_dark;
    theme_light_choice.set_callback(move |c| {
        if !is_dark_for_light
            && let Some(theme) = index_to_theme(c.value())
        {
            sender_light.send(Message::PreviewSyntaxTheme(theme));
        }
    });

    let sender_dark = *sender;
    let is_dark_for_dark = is_dark;
    theme_dark_choice.set_callback(move |c| {
        if is_dark_for_dark
            && let Some(theme) = index_to_theme(c.value())
        {
            sender_dark.send(Message::PreviewSyntaxTheme(theme));
        }
    });

    // Font section
    Frame::default().with_pos(LEFT_COL, y).with_size(COL_WIDTH, LABEL_HEIGHT)
        .with_label("Font:")
        .with_align(fltk::enums::Align::Left | fltk::enums::Align::Inside);
    y += LABEL_HEIGHT + 5;

    let font_group = Group::default().with_pos(LEFT_COL + 10, y).with_size(COL_WIDTH - 10, ITEM_HEIGHT * 3);
    let mut font_screenbold = RadioRoundButton::default().with_pos(LEFT_COL + 10, y).with_size(COL_WIDTH - 10, ITEM_HEIGHT).with_label("Screen (Bold)");
    let mut font_courier = RadioRoundButton::default().with_pos(LEFT_COL + 10, y + ITEM_HEIGHT).with_size(COL_WIDTH - 10, ITEM_HEIGHT).with_label("Courier");
    let mut font_helvetica = RadioRoundButton::default().with_pos(LEFT_COL + 10, y + ITEM_HEIGHT * 2).with_size(COL_WIDTH - 10, ITEM_HEIGHT).with_label("Helvetica Mono");
    font_group.end();
    y += ITEM_HEIGHT * 3 + SECTION_GAP;

    match current_settings.font {
        FontChoice::ScreenBold => font_screenbold.set_value(true),
        FontChoice::Courier => font_courier.set_value(true),
        FontChoice::HelveticaMono => font_helvetica.set_value(true),
    }

    // Font size section
    Frame::default().with_pos(LEFT_COL, y).with_size(COL_WIDTH, LABEL_HEIGHT)
        .with_label("Font Size:")
        .with_align(fltk::enums::Align::Left | fltk::enums::Align::Inside);
    y += LABEL_HEIGHT + 5;

    let size_group = Group::default().with_pos(LEFT_COL + 10, y).with_size(COL_WIDTH - 10, ITEM_HEIGHT * 3);
    let mut size_12 = RadioRoundButton::default().with_pos(LEFT_COL + 10, y).with_size(COL_WIDTH - 10, ITEM_HEIGHT).with_label("Small (12)");
    let mut size_16 = RadioRoundButton::default().with_pos(LEFT_COL + 10, y + ITEM_HEIGHT).with_size(COL_WIDTH - 10, ITEM_HEIGHT).with_label("Medium (16)");
    let mut size_20 = RadioRoundButton::default().with_pos(LEFT_COL + 10, y + ITEM_HEIGHT * 2).with_size(COL_WIDTH - 10, ITEM_HEIGHT).with_label("Large (20)");
    size_group.end();
    y += ITEM_HEIGHT * 3 + SECTION_GAP;

    match current_settings.font_size {
        12 => size_12.set_value(true),
        16 => size_16.set_value(true),
        20 => size_20.set_value(true),
        _ => size_16.set_value(true),
    }

    // Tab size section
    Frame::default().with_pos(LEFT_COL, y).with_size(COL_WIDTH, LABEL_HEIGHT)
        .with_label("Tab Size:")
        .with_align(fltk::enums::Align::Left | fltk::enums::Align::Inside);
    y += LABEL_HEIGHT + 5;

    let tab_group = Group::default().with_pos(LEFT_COL + 10, y).with_size(COL_WIDTH - 10, ITEM_HEIGHT * 3);
    let mut tab_2 = RadioRoundButton::default().with_pos(LEFT_COL + 10, y).with_size(COL_WIDTH - 10, ITEM_HEIGHT).with_label("2 spaces");
    let mut tab_4 = RadioRoundButton::default().with_pos(LEFT_COL + 10, y + ITEM_HEIGHT).with_size(COL_WIDTH - 10, ITEM_HEIGHT).with_label("4 spaces");
    let mut tab_8 = RadioRoundButton::default().with_pos(LEFT_COL + 10, y + ITEM_HEIGHT * 2).with_size(COL_WIDTH - 10, ITEM_HEIGHT).with_label("8 spaces");
    tab_group.end();

    match current_settings.tab_size {
        2 => tab_2.set_value(true),
        8 => tab_8.set_value(true),
        _ => tab_4.set_value(true),
    }

    // ============ RIGHT COLUMN - Behavior ============
    let mut y = 15;

    // View options section
    Frame::default().with_pos(RIGHT_COL, y).with_size(COL_WIDTH, LABEL_HEIGHT)
        .with_label("View Options:")
        .with_align(fltk::enums::Align::Left | fltk::enums::Align::Inside);
    y += LABEL_HEIGHT + 5;

    let mut check_line_numbers = CheckButton::default().with_pos(RIGHT_COL + 10, y).with_size(COL_WIDTH - 10, ITEM_HEIGHT).with_label("Show Line Numbers");
    check_line_numbers.set_value(current_settings.line_numbers_enabled);
    y += ITEM_HEIGHT;

    let mut check_word_wrap = CheckButton::default().with_pos(RIGHT_COL + 10, y).with_size(COL_WIDTH - 10, ITEM_HEIGHT).with_label("Word Wrap");
    check_word_wrap.set_value(current_settings.word_wrap_enabled);
    y += ITEM_HEIGHT;

    let mut check_highlighting = CheckButton::default().with_pos(RIGHT_COL + 10, y).with_size(COL_WIDTH - 10, ITEM_HEIGHT).with_label("Syntax Highlighting");
    check_highlighting.set_value(current_settings.highlighting_enabled);
    y += ITEM_HEIGHT;

    let mut check_tabs_enabled = CheckButton::default().with_pos(RIGHT_COL + 10, y).with_size(COL_WIDTH - 10, ITEM_HEIGHT).with_label("Tabbed editing (restart)");
    check_tabs_enabled.set_value(current_settings.tabs_enabled);
    y += ITEM_HEIGHT + SECTION_GAP;

    // Session restore section
    Frame::default().with_pos(RIGHT_COL, y).with_size(COL_WIDTH, LABEL_HEIGHT)
        .with_label("Session Restore:")
        .with_align(fltk::enums::Align::Left | fltk::enums::Align::Inside);
    y += LABEL_HEIGHT + 5;

    let session_group = Group::default().with_pos(RIGHT_COL + 10, y).with_size(COL_WIDTH - 10, ITEM_HEIGHT * 3);
    let mut session_off = RadioRoundButton::default().with_pos(RIGHT_COL + 10, y).with_size(COL_WIDTH - 10, ITEM_HEIGHT).with_label("Off");
    let mut session_saved = RadioRoundButton::default().with_pos(RIGHT_COL + 10, y + ITEM_HEIGHT).with_size(COL_WIDTH - 10, ITEM_HEIGHT).with_label("Saved Files Only");
    let mut session_full = RadioRoundButton::default().with_pos(RIGHT_COL + 10, y + ITEM_HEIGHT * 2).with_size(COL_WIDTH - 10, ITEM_HEIGHT).with_label("Full (including unsaved)");
    session_group.end();
    y += ITEM_HEIGHT * 3 + SECTION_GAP;

    match current_settings.session_restore {
        SessionRestore::Off => session_off.set_value(true),
        SessionRestore::SavedFiles => session_saved.set_value(true),
        SessionRestore::Full => session_full.set_value(true),
    }

    // Updates section
    Frame::default().with_pos(RIGHT_COL, y).with_size(COL_WIDTH, LABEL_HEIGHT)
        .with_label("Updates:")
        .with_align(fltk::enums::Align::Left | fltk::enums::Align::Inside);
    y += LABEL_HEIGHT + 5;

    let mut check_auto_update = CheckButton::default().with_pos(RIGHT_COL + 10, y).with_size(COL_WIDTH - 10, ITEM_HEIGHT).with_label("Auto-check for updates");
    check_auto_update.set_value(current_settings.auto_check_updates);
    y += ITEM_HEIGHT;

    let mut check_prerelease = CheckButton::default().with_pos(RIGHT_COL + 10, y).with_size(COL_WIDTH - 10, ITEM_HEIGHT).with_label("Include pre-releases");
    check_prerelease.set_value(current_settings.update_channel == UpdateChannel::Beta);
    y += ITEM_HEIGHT + 10;

    // Info text
    let mut info_frame = Frame::default().with_pos(RIGHT_COL + 10, y).with_size(COL_WIDTH - 20, 35);
    info_frame.set_label("Checks GitHub once per day.\nNo personal data is sent.");
    info_frame.set_label_size(11);
    info_frame.set_label_color(Color::from_rgb(100, 100, 100));
    info_frame.set_align(fltk::enums::Align::Left | fltk::enums::Align::Inside | fltk::enums::Align::Wrap);

    vpack.end();

    // Buttons at bottom
    let btn_y = DIALOG_HEIGHT - 45;
    let mut save_btn = Button::default().with_pos(DIALOG_WIDTH - 200, btn_y).with_size(90, 30).with_label("Save");
    let mut cancel_btn = Button::default().with_pos(DIALOG_WIDTH - 100, btn_y).with_size(90, 30).with_label("Cancel");

    dialog.end();
    dialog.show();

    let result = Rc::new(RefCell::new(None));
    let result_save = result.clone();
    let result_cancel = result.clone();

    // Store original theme for reverting on cancel
    let original_theme = current_settings.current_syntax_theme(is_dark);
    let sender_cancel = *sender;

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
            tab_size: if tab_2.value() {
                2
            } else if tab_8.value() {
                8
            } else {
                4
            },
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
    let sender_close = *sender;
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
