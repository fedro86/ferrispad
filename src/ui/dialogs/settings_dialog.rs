use fltk::{
    enums::{CallbackTrigger, FrameType},
    frame::Frame,
    group::{Group, Scroll, ScrollType},
    menu::Choice,
    prelude::*,
    window::Window,
};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::app::domain::settings::default_syntax_theme;
use crate::app::{AppSettings, SessionRestore, SyntaxTheme, ThemeMode, ThemeRef, UpdateChannel};

use super::DialogTheme;
use super::settings_layout::{
    CONTENT_MARGIN, column_width, content_height, dialog_height, dialog_top, dialog_width,
    scroll_to_show,
};

// Layout constants. Widths are measured (`SettingsMetrics`); these are the
// historical minimums, so a dialog with short labels looks as before.
const DIALOG_HEIGHT: i32 = 650;
const COL_WIDTH: i32 = 280;
/// FLTK's default label size, used by every row of this dialog.
const LABEL_SIZE: i32 = 14;
const LEFT_COL: i32 = 15;
const LABEL_HEIGHT: i32 = 20;
const ITEM_HEIGHT: i32 = 22;
const SECTION_GAP: i32 = 15;

/// What the Settings dialog asks of its caller while it is open. The dialog
/// is modal, so the caller acts from inside the dialog loop.
pub enum SettingsHook {
    /// Show this syntax theme in the editor (a live preview, or the revert
    /// to the original theme on Cancel/close after a preview).
    Preview(ThemeRef),
    /// Open the theme manager. The caller returns the custom themes
    /// (reference, name) afterwards.
    ManageThemes,
}

/// Show settings dialog and return updated settings if user clicked Save.
/// `hook` is called while the dialog is open: [`SettingsHook::Preview`] each
/// time the user picks a syntax theme for the current mode (and once more
/// with the original theme when the dialog is cancelled or closed after a
/// preview), [`SettingsHook::ManageThemes`] for "Manage themes…", which
/// returns the custom themes to list afterwards. `custom_themes` lists the
/// loaded custom themes (reference, display name) offered beside the
/// built-ins.
pub fn show_settings_dialog(
    current_settings: &AppSettings,
    theme_bg: (u8, u8, u8),
    custom_themes: &[(ThemeRef, String)],
    mut hook: impl FnMut(SettingsHook) -> Option<Vec<(ThemeRef, String)>>,
) -> Option<AppSettings> {
    let SettingsUi {
        mut dialog,
        result,
        pending_manage,
        pending_preview,
        theme_light_choice,
        theme_dark_choice,
        light_choices,
        dark_choices,
        text_rgb: (tr, tg, tb),
        mut scroll,
    } = build_settings_dialog(current_settings, theme_bg, custom_themes);
    dialog.show();
    DialogTheme::from_theme_bg(theme_bg).apply_titlebar(&dialog);

    let (mut light_widget, mut dark_widget) = (theme_light_choice, theme_dark_choice);
    super::run_dialog_with(&dialog, || {
        // Keep the focused field visible when the content scrolls (T0083);
        // runs after each event, never on a timer.
        if let Some(focus) = fltk::app::focus()
            && focus.inside(&scroll)
            && let Some(pos) = scroll_to_show(
                focus.y(),
                focus.h(),
                scroll.y(),
                scroll.h(),
                scroll.yposition(),
            )
        {
            scroll.scroll_to(scroll.xposition(), pos);
        }
        if pending_manage.replace(false) {
            if let Some(customs) = hook(SettingsHook::ManageThemes) {
                // Deleted themes leave the lists; a choice that used one
                // falls back to its mode's default.
                refresh_choice(&mut light_widget, &light_choices, &customs, false);
                refresh_choice(&mut dark_widget, &dark_choices, &customs, true);
            }
            fltk::app::foreground(tr, tg, tb);
        }
        if let Some(theme) = pending_preview.take() {
            hook(SettingsHook::Preview(theme));
            // The preview re-themes the main window, menu text colour
            // included; this dialog keeps its own colours, so its Choice
            // popups need its foreground back.
            fltk::app::foreground(tr, tg, tb);
        }
    });

    result.borrow().clone()
}

/// A built, not yet shown Settings dialog (see [`build_settings_dialog`]).
pub struct SettingsUi {
    /// The dialog window.
    pub dialog: Window,
    result: Rc<RefCell<Option<AppSettings>>>,
    pending_manage: Rc<Cell<bool>>,
    pending_preview: Rc<Cell<Option<ThemeRef>>>,
    theme_light_choice: Choice,
    theme_dark_choice: Choice,
    light_choices: Rc<RefCell<Vec<(ThemeRef, String)>>>,
    dark_choices: Rc<RefCell<Vec<(ThemeRef, String)>>>,
    text_rgb: (u8, u8, u8),
    /// The scrolling content area (everything above Save/Cancel).
    scroll: Scroll,
}

/// Widths measured from the labels with the installed fonts (T0083):
/// `max_label_width` measures at the label size + 2 pt plus padding, so a
/// wider desktop font still fits.
struct SettingsMetrics {
    left_w: i32,
    right_w: i32,
    right_x: i32,
    dialog_w: i32,
    change_w: i32,
    large_label_w: i32,
    button_w: i32,
}

impl SettingsMetrics {
    fn measure() -> Self {
        let width = |labels: &[&str]| super::max_label_width(labels, LABEL_SIZE);
        // Check and radio rows: indented by 10, with the box left of the label.
        let check = |labels: &[&str]| width(labels) + LABEL_SIZE + 8 + 10;
        let change_w = width(&["Change..."]).max(80);
        let left_w = column_width(
            COL_WIDTH,
            &[
                width(&[
                    "Theme:",
                    "Syntax Theme (Light):",
                    "Syntax Theme (Dark):",
                    "Font:",
                    "Font Size:",
                    "Tab Size:",
                ]),
                check(&[
                    "Light",
                    "Dark",
                    "System Default",
                    "2 spaces",
                    "4 spaces",
                    "8 spaces",
                    "Use spaces instead of tabs",
                ]),
                width(&["Manage themes…"]) + 20,
                // Font row: a usable input, a gap and the Change button.
                10 + 120 + 10 + change_w,
            ],
        );
        let large_label_w =
            super::max_label_width(&["Show warning above (MB):", "Max editable size (MB):"], 12)
                .max(150);
        let right_w = column_width(
            COL_WIDTH,
            &[
                width(&[
                    "View Options:",
                    "Session Restore:",
                    "Updates:",
                    "Large Files:",
                ]),
                check(&[
                    "Show Line Numbers",
                    "Word Wrap",
                    "Highlight Current Line",
                    "Syntax Highlighting",
                    "Tabbed editing (restart)",
                    "Off",
                    "Saved Files Only",
                    "Full (including unsaved)",
                    "Auto-check for updates",
                    "Include pre-releases",
                    "Auto-check plugin updates",
                ]),
                // Label cell, gap, 60 px input.
                10 + large_label_w + 5 + 60,
            ],
        );
        let right_x = LEFT_COL + left_w + 25;
        Self {
            left_w,
            right_w,
            right_x,
            dialog_w: right_x + right_w + 20,
            change_w,
            large_label_w,
            button_w: width(&["Save", "Cancel"]).max(90),
        }
    }
}

/// Build the Settings dialog with its callbacks wired, without showing it.
/// [`show_settings_dialog`] is the real entry point; this is also used by
/// the UI construction tests (`tests/ui_dialogs.rs`).
pub fn build_settings_dialog(
    current_settings: &AppSettings,
    theme_bg: (u8, u8, u8),
    custom_themes: &[(ThemeRef, String)],
) -> SettingsUi {
    let light_choices = theme_choices(&current_settings.syntax_theme_light, custom_themes);
    let dark_choices = theme_choices(&current_settings.syntax_theme_dark, custom_themes);
    let theme = DialogTheme::from_theme_bg(theme_bg);
    let is_dark = theme.is_dark();

    // Set FLTK foreground color so menu popups (Choice) use the right text color
    let (tr, tg, tb) = theme.text_rgb();
    fltk::app::foreground(tr, tg, tb);

    // Built top-level even if another group is current (T0058).
    let top_level = super::TopLevelScope::new();
    let m = SettingsMetrics::measure();
    let mut dialog = Window::default()
        .with_size(m.dialog_w, DIALOG_HEIGHT)
        .with_label("Settings")
        .center_screen();
    dialog.make_modal(true);
    dialog.set_color(theme.bg);

    // The content scrolls when the screen is too short for it (T0083); the
    // Save/Cancel row below stays outside the scroll.
    let mut vpack = Scroll::default()
        .with_size(m.dialog_w - 20, DIALOG_HEIGHT - 60)
        .with_pos(10, 10);
    vpack.set_type(ScrollType::Vertical);
    vpack.set_frame(FrameType::FlatBox);
    vpack.set_color(theme.bg);
    theme.style_scroll(&mut vpack);

    // ============ LEFT COLUMN - Appearance ============
    let mut y = 15;

    // Theme section
    let mut theme_label = Frame::default()
        .with_pos(LEFT_COL, y)
        .with_size(m.left_w, LABEL_HEIGHT)
        .with_label("Theme:")
        .with_align(fltk::enums::Align::Left | fltk::enums::Align::Inside);
    theme_label.set_label_color(theme.text);
    y += LABEL_HEIGHT + 5;

    let mut theme_group = Group::default()
        .with_pos(LEFT_COL + 10, y)
        .with_size(m.left_w - 10, ITEM_HEIGHT * 3);
    theme_group.set_color(theme.bg);
    let mut theme_light = theme
        .radio()
        .with_pos(LEFT_COL + 10, y)
        .with_size(m.left_w - 10, ITEM_HEIGHT)
        .with_label("Light");
    let mut theme_dark = theme
        .radio()
        .with_pos(LEFT_COL + 10, y + ITEM_HEIGHT)
        .with_size(m.left_w - 10, ITEM_HEIGHT)
        .with_label("Dark");
    let mut theme_system = theme
        .radio()
        .with_pos(LEFT_COL + 10, y + ITEM_HEIGHT * 2)
        .with_size(m.left_w - 10, ITEM_HEIGHT)
        .with_label("System Default");
    theme_group.end();
    y += ITEM_HEIGHT * 3 + SECTION_GAP;

    match current_settings.theme_mode {
        ThemeMode::Light => theme_light.set_value(true),
        ThemeMode::Dark => theme_dark.set_value(true),
        ThemeMode::SystemDefault => theme_system.set_value(true),
    }

    // Syntax Theme (Light)
    let mut stl_label = Frame::default()
        .with_pos(LEFT_COL, y)
        .with_size(m.left_w, LABEL_HEIGHT)
        .with_label("Syntax Theme (Light):")
        .with_align(fltk::enums::Align::Left | fltk::enums::Align::Inside);
    stl_label.set_label_color(theme.text);
    y += LABEL_HEIGHT + 3;

    let mut theme_light_choice = theme
        .choice()
        .with_pos(LEFT_COL + 10, y)
        .with_size(m.left_w - 20, 25);
    for (_, label) in light_choices.borrow().iter() {
        theme_light_choice.add_choice(&choice_label(label));
    }
    theme_light_choice.set_value(choice_index(
        &light_choices.borrow(),
        &current_settings.syntax_theme_light,
    ));
    y += 30;

    // Syntax Theme (Dark)
    let mut std_label = Frame::default()
        .with_pos(LEFT_COL, y)
        .with_size(m.left_w, LABEL_HEIGHT)
        .with_label("Syntax Theme (Dark):")
        .with_align(fltk::enums::Align::Left | fltk::enums::Align::Inside);
    std_label.set_label_color(theme.text);
    y += LABEL_HEIGHT + 3;

    let mut theme_dark_choice = theme
        .choice()
        .with_pos(LEFT_COL + 10, y)
        .with_size(m.left_w - 20, 25);
    for (_, label) in dark_choices.borrow().iter() {
        theme_dark_choice.add_choice(&choice_label(label));
    }
    theme_dark_choice.set_value(choice_index(
        &dark_choices.borrow(),
        &current_settings.syntax_theme_dark,
    ));
    y += 30;

    let mut manage_btn = theme
        .button()
        .with_pos(LEFT_COL + 10, y)
        .with_size(m.left_w - 20, 25)
        .with_label("Manage themes…");
    let pending_manage = Rc::new(Cell::new(false));
    let pending_manage_btn = pending_manage.clone();
    manage_btn.set_callback(move |_| pending_manage_btn.set(true));
    y += 30 + SECTION_GAP;

    // Live preview callbacks for theme changes. The widget callbacks only
    // record the theme; the dialog loop hands it to `preview` (see the end).
    let pending_preview: Rc<Cell<Option<ThemeRef>>> = Rc::new(Cell::new(None));
    let previewed = Rc::new(Cell::new(false));

    let (pending_light, previewed_light) = (pending_preview.clone(), previewed.clone());
    let is_dark_for_light = is_dark;
    let light_for_preview = light_choices.clone();
    theme_light_choice.set_callback(move |c| {
        if !is_dark_for_light && let Some(theme) = choice_at(&light_for_preview.borrow(), c.value())
        {
            pending_light.set(Some(theme));
            previewed_light.set(true);
        }
    });

    let (pending_dark, previewed_dark) = (pending_preview.clone(), previewed.clone());
    let is_dark_for_dark = is_dark;
    let dark_for_preview = dark_choices.clone();
    theme_dark_choice.set_callback(move |c| {
        if is_dark_for_dark && let Some(theme) = choice_at(&dark_for_preview.borrow(), c.value()) {
            pending_dark.set(Some(theme));
            previewed_dark.set(true);
        }
    });

    // Font section
    let mut font_label = Frame::default()
        .with_pos(LEFT_COL, y)
        .with_size(m.left_w, LABEL_HEIGHT)
        .with_label("Font:")
        .with_align(fltk::enums::Align::Left | fltk::enums::Align::Inside);
    font_label.set_label_color(theme.text);
    y += LABEL_HEIGHT + 5;

    let mut font_value = theme
        .readonly_input()
        .with_pos(LEFT_COL + 10, y)
        .with_size(m.left_w - 10 - m.change_w - 10, ITEM_HEIGHT);
    font_value.set_value(&current_settings.font);

    let mut change_font_btn = theme
        .button()
        .with_pos(LEFT_COL + m.left_w - m.change_w, y)
        .with_size(m.change_w, ITEM_HEIGHT)
        .with_label("Change...");
    y += ITEM_HEIGHT + SECTION_GAP;

    // Font size section
    let mut size_label = Frame::default()
        .with_pos(LEFT_COL, y)
        .with_size(m.left_w, LABEL_HEIGHT)
        .with_label("Font Size:")
        .with_align(fltk::enums::Align::Left | fltk::enums::Align::Inside);
    size_label.set_label_color(theme.text);
    y += LABEL_HEIGHT + 5;

    let mut size_input = theme
        .int_input()
        .with_pos(LEFT_COL + 10, y)
        .with_size(70, ITEM_HEIGHT);
    size_input.set_value(&current_settings.font_size.to_string());
    size_input.set_trigger(CallbackTrigger::Changed);
    {
        let mut s = size_input.clone();
        size_input.set_callback(move |_| {
            if let Ok(v) = s.value().parse::<i32>() {
                let clamped = v.clamp(6, 96);
                if clamped != v {
                    s.set_value(&clamped.to_string());
                }
            }
        });
    }

    let mut size_hint = Frame::default()
        .with_pos(LEFT_COL + 90, y)
        .with_size(150, ITEM_HEIGHT)
        .with_label("(6\u{2013}96)")
        .with_align(fltk::enums::Align::Left | fltk::enums::Align::Inside);
    size_hint.set_label_color(theme.text_dim);
    y += ITEM_HEIGHT + SECTION_GAP;

    // [Change...] opens the font picker; on OK, mirror the result into both inputs.
    {
        let mut font_value = font_value.clone();
        let mut size_input = size_input.clone();
        let dialog_for_picker = dialog.clone();
        let theme_bg_for_picker = theme_bg;
        change_font_btn.set_callback(move |_| {
            let current_name = font_value.value();
            let current_size = size_input.value().parse::<u32>().unwrap_or(16);
            if let Some((name, size)) = super::font_picker::show_font_picker(
                &dialog_for_picker,
                &current_name,
                current_size,
                theme_bg_for_picker,
            ) {
                font_value.set_value(&name);
                size_input.set_value(&size.to_string());
            }
        });
    }

    // Tab size section
    let mut tabsize_label = Frame::default()
        .with_pos(LEFT_COL, y)
        .with_size(m.left_w, LABEL_HEIGHT)
        .with_label("Tab Size:")
        .with_align(fltk::enums::Align::Left | fltk::enums::Align::Inside);
    tabsize_label.set_label_color(theme.text);
    y += LABEL_HEIGHT + 5;

    let mut tab_group = Group::default()
        .with_pos(LEFT_COL + 10, y)
        .with_size(m.left_w - 10, ITEM_HEIGHT * 3);
    tab_group.set_color(theme.bg);
    let mut tab_2 = theme
        .radio()
        .with_pos(LEFT_COL + 10, y)
        .with_size(m.left_w - 10, ITEM_HEIGHT)
        .with_label("2 spaces");
    let mut tab_4 = theme
        .radio()
        .with_pos(LEFT_COL + 10, y + ITEM_HEIGHT)
        .with_size(m.left_w - 10, ITEM_HEIGHT)
        .with_label("4 spaces");
    let mut tab_8 = theme
        .radio()
        .with_pos(LEFT_COL + 10, y + ITEM_HEIGHT * 2)
        .with_size(m.left_w - 10, ITEM_HEIGHT)
        .with_label("8 spaces");
    tab_group.end();

    match current_settings.tab_size {
        2 => tab_2.set_value(true),
        8 => tab_8.set_value(true),
        _ => tab_4.set_value(true),
    }

    y += ITEM_HEIGHT * 3 + 5;
    let mut use_spaces = theme
        .check_button()
        .with_pos(LEFT_COL + 10, y)
        .with_size(m.left_w - 10, ITEM_HEIGHT)
        .with_label("Use spaces instead of tabs");
    use_spaces.set_value(current_settings.use_spaces);

    let left_bottom = y + ITEM_HEIGHT;

    // ============ RIGHT COLUMN - Behavior ============
    let mut y = 15;

    // View options section
    let mut view_label = Frame::default()
        .with_pos(m.right_x, y)
        .with_size(m.right_w, LABEL_HEIGHT)
        .with_label("View Options:")
        .with_align(fltk::enums::Align::Left | fltk::enums::Align::Inside);
    view_label.set_label_color(theme.text);
    y += LABEL_HEIGHT + 5;

    let mut check_line_numbers = theme
        .check_button()
        .with_pos(m.right_x + 10, y)
        .with_size(m.right_w - 10, ITEM_HEIGHT)
        .with_label("Show Line Numbers");
    check_line_numbers.set_value(current_settings.line_numbers_enabled);
    y += ITEM_HEIGHT;

    let mut check_word_wrap = theme
        .check_button()
        .with_pos(m.right_x + 10, y)
        .with_size(m.right_w - 10, ITEM_HEIGHT)
        .with_label("Word Wrap");
    check_word_wrap.set_value(current_settings.word_wrap_enabled);
    y += ITEM_HEIGHT;

    let mut check_current_line = theme
        .check_button()
        .with_pos(m.right_x + 10, y)
        .with_size(m.right_w - 10, ITEM_HEIGHT)
        .with_label("Highlight Current Line");
    check_current_line.set_value(current_settings.highlight_current_line);
    y += ITEM_HEIGHT;

    let mut check_highlighting = theme
        .check_button()
        .with_pos(m.right_x + 10, y)
        .with_size(m.right_w - 10, ITEM_HEIGHT)
        .with_label("Syntax Highlighting");
    check_highlighting.set_value(current_settings.highlighting_enabled);
    y += ITEM_HEIGHT;

    let mut check_tabs_enabled = theme
        .check_button()
        .with_pos(m.right_x + 10, y)
        .with_size(m.right_w - 10, ITEM_HEIGHT)
        .with_label("Tabbed editing (restart)");
    check_tabs_enabled.set_value(current_settings.tabs_enabled);
    y += ITEM_HEIGHT + SECTION_GAP;

    // Session restore section
    let mut session_label = Frame::default()
        .with_pos(m.right_x, y)
        .with_size(m.right_w, LABEL_HEIGHT)
        .with_label("Session Restore:")
        .with_align(fltk::enums::Align::Left | fltk::enums::Align::Inside);
    session_label.set_label_color(theme.text);
    y += LABEL_HEIGHT + 5;

    let mut session_group = Group::default()
        .with_pos(m.right_x + 10, y)
        .with_size(m.right_w - 10, ITEM_HEIGHT * 3);
    session_group.set_color(theme.bg);
    let mut session_off = theme
        .radio()
        .with_pos(m.right_x + 10, y)
        .with_size(m.right_w - 10, ITEM_HEIGHT)
        .with_label("Off");
    let mut session_saved = theme
        .radio()
        .with_pos(m.right_x + 10, y + ITEM_HEIGHT)
        .with_size(m.right_w - 10, ITEM_HEIGHT)
        .with_label("Saved Files Only");
    let mut session_full = theme
        .radio()
        .with_pos(m.right_x + 10, y + ITEM_HEIGHT * 2)
        .with_size(m.right_w - 10, ITEM_HEIGHT)
        .with_label("Full (including unsaved)");
    session_group.end();
    y += ITEM_HEIGHT * 3 + SECTION_GAP;

    match current_settings.session_restore {
        SessionRestore::Off => session_off.set_value(true),
        SessionRestore::SavedFiles => session_saved.set_value(true),
        SessionRestore::Full => session_full.set_value(true),
    }

    // Updates section
    let mut updates_label = Frame::default()
        .with_pos(m.right_x, y)
        .with_size(m.right_w, LABEL_HEIGHT)
        .with_label("Updates:")
        .with_align(fltk::enums::Align::Left | fltk::enums::Align::Inside);
    updates_label.set_label_color(theme.text);
    y += LABEL_HEIGHT + 5;

    let mut check_auto_update = theme
        .check_button()
        .with_pos(m.right_x + 10, y)
        .with_size(m.right_w - 10, ITEM_HEIGHT)
        .with_label("Auto-check for updates");
    check_auto_update.set_value(current_settings.auto_check_updates);
    y += ITEM_HEIGHT;

    let mut check_prerelease = theme
        .check_button()
        .with_pos(m.right_x + 10, y)
        .with_size(m.right_w - 10, ITEM_HEIGHT)
        .with_label("Include pre-releases");
    check_prerelease.set_value(current_settings.update_channel == UpdateChannel::Beta);
    y += ITEM_HEIGHT;

    let mut check_plugin_updates = theme
        .check_button()
        .with_pos(m.right_x + 10, y)
        .with_size(m.right_w - 10, ITEM_HEIGHT)
        .with_label("Auto-check plugin updates");
    check_plugin_updates.set_value(current_settings.auto_check_plugin_updates);
    y += ITEM_HEIGHT + 10;

    // Info text
    let mut info_frame = Frame::default()
        .with_pos(m.right_x + 10, y)
        .with_size(m.right_w - 20, 35);
    info_frame.set_label("Checks GitHub once per day.\nNo personal data is sent.");
    info_frame.set_label_size(11);
    info_frame.set_label_color(theme.text_dim);
    info_frame.set_align(
        fltk::enums::Align::Left | fltk::enums::Align::Inside | fltk::enums::Align::Wrap,
    );
    y += 35 + SECTION_GAP;

    // Large Files section
    let mut large_files_label = Frame::default()
        .with_pos(m.right_x, y)
        .with_size(m.right_w, LABEL_HEIGHT)
        .with_label("Large Files:")
        .with_align(fltk::enums::Align::Left | fltk::enums::Align::Inside);
    large_files_label.set_label_color(theme.text);
    y += LABEL_HEIGHT + 5;

    let mut warning_label = Frame::default()
        .with_pos(m.right_x + 10, y)
        .with_size(m.large_label_w, ITEM_HEIGHT)
        .with_label("Show warning above (MB):")
        .with_align(fltk::enums::Align::Left | fltk::enums::Align::Inside);
    warning_label.set_label_color(theme.text);
    warning_label.set_label_size(12);
    let mut warning_input = theme
        .input()
        .with_pos(m.right_x + 15 + m.large_label_w, y)
        .with_size(60, ITEM_HEIGHT);
    warning_input.set_value(&current_settings.large_file_warning_mb.to_string());
    y += ITEM_HEIGHT + 5;

    let mut max_edit_label = Frame::default()
        .with_pos(m.right_x + 10, y)
        .with_size(m.large_label_w, ITEM_HEIGHT)
        .with_label("Max editable size (MB):")
        .with_align(fltk::enums::Align::Left | fltk::enums::Align::Inside);
    max_edit_label.set_label_color(theme.text);
    max_edit_label.set_label_size(12);
    let mut max_edit_input = theme
        .input()
        .with_pos(m.right_x + 15 + m.large_label_w, y)
        .with_size(60, ITEM_HEIGHT);
    max_edit_input.set_value(&current_settings.max_editable_size_mb.to_string());

    let right_bottom = y + ITEM_HEIGHT;
    vpack.end();

    // Fit the dialog to the work area of its screen (T0083): content
    // scrolls when it is taller, Save/Cancel stay below it.
    let content_h = content_height(&[left_bottom, right_bottom]) - vpack.y() + 5;
    let screen = fltk::app::screen_num(dialog.x() + dialog.w() / 2, dialog.y() + dialog.h() / 2);
    let (wx, wy, ww, wh) = fltk::app::screen_work_area(screen);
    let fit = dialog_height(content_h, wh);
    let dialog_w = dialog_width(m.dialog_w, fit.scrolls, super::SCROLLBAR_SIZE);
    // Resizing must not rescale the children built for the final layout.
    dialog.make_resizable(false);
    dialog.resize(
        wx + (ww - dialog_w).max(0) / 2,
        dialog_top(wy, wh, fit.dialog_h),
        dialog_w,
        fit.dialog_h,
    );
    vpack.resize(
        CONTENT_MARGIN,
        CONTENT_MARGIN,
        dialog_w - 2 * CONTENT_MARGIN,
        fit.viewport_h,
    );

    // Buttons at bottom
    let btn_y = fit.dialog_h - 45;
    let mut save_btn = theme
        .button()
        .with_pos(dialog_w - 2 * m.button_w - 20, btn_y)
        .with_size(m.button_w, 30)
        .with_label("Save");
    let mut cancel_btn = theme
        .button()
        .with_pos(dialog_w - m.button_w - 10, btn_y)
        .with_size(m.button_w, 30)
        .with_label("Cancel");

    dialog.end();
    drop(top_level);

    let result = Rc::new(RefCell::new(None));
    let result_save = result.clone();
    let result_cancel = result.clone();

    // Store original theme for reverting on cancel
    let original_theme = current_settings.current_syntax_theme(is_dark);
    let original_theme_cancel = original_theme.clone();
    let (pending_cancel, previewed_cancel) = (pending_preview.clone(), previewed.clone());

    let dialog_save = dialog.clone();
    let current = current_settings.clone();
    let (light_choices_save, dark_choices_save) = (light_choices.clone(), dark_choices.clone());
    let (light_save, dark_save) = (theme_light_choice.clone(), theme_dark_choice.clone());
    let warning_input_save = warning_input.clone();
    let max_edit_input_save = max_edit_input.clone();
    save_btn.set_callback(move |_| {
        // Parse large file thresholds with clamping
        let max_editable_mb = max_edit_input_save
            .value()
            .trim()
            .parse::<u32>()
            .unwrap_or(150)
            .clamp(1, 10000);
        let mut large_file_warning_mb = warning_input_save
            .value()
            .trim()
            .parse::<u32>()
            .unwrap_or(50)
            .clamp(1, 10000);
        // Ensure warning <= max_editable
        if large_file_warning_mb > max_editable_mb {
            large_file_warning_mb = max_editable_mb;
        }

        let new_settings = AppSettings {
            theme_mode: if theme_light.value() {
                ThemeMode::Light
            } else if theme_dark.value() {
                ThemeMode::Dark
            } else {
                ThemeMode::SystemDefault
            },
            font: {
                let v = font_value.value();
                if v.trim().is_empty() {
                    "Courier".to_string()
                } else {
                    v
                }
            },
            font_size: size_input.value().parse::<u32>().unwrap_or(16).clamp(6, 96),
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
            syntax_theme_light: choice_at(&light_choices_save.borrow(), light_save.value())
                .unwrap_or_else(|| current.syntax_theme_light.clone()),
            syntax_theme_dark: choice_at(&dark_choices_save.borrow(), dark_save.value())
                .unwrap_or_else(|| current.syntax_theme_dark.clone()),
            tab_size: if tab_2.value() {
                2
            } else if tab_8.value() {
                8
            } else {
                4
            },
            use_spaces: use_spaces.value(),
            // Preserve plugin settings (not editable in this dialog, except auto-check)
            plugins_enabled: current.plugins_enabled,
            disabled_plugins: current.disabled_plugins.clone(),
            plugin_approvals: current.plugin_approvals.clone(),
            auto_check_plugin_updates: check_plugin_updates.value(),
            last_plugin_update_check: current.last_plugin_update_check,
            // Preserve Run All Checks settings (editable via Plugins > General > Settings)
            run_all_checks_plugins: current.run_all_checks_plugins.clone(),
            run_all_checks_shortcut: current.run_all_checks_shortcut.clone(),
            // Preserve per-plugin configs (editable via Plugins > {Plugin} > Settings)
            plugin_configs: current.plugin_configs.clone(),
            // Preserve shortcut overrides (editable via Edit > Key Shortcuts)
            shortcut_overrides: current.shortcut_overrides.clone(),
            large_file_warning_mb,
            max_editable_size_mb: max_editable_mb,
            // Window geometry is not edited here; it is saved on close
            window: current.window,
            highlight_current_line: check_current_line.value(),
        };

        *result_save.borrow_mut() = Some(new_settings);
        dialog_save.clone().hide();
    });

    let dialog_cancel = dialog.clone();
    cancel_btn.set_callback(move |_| {
        // Revert to original theme
        if previewed_cancel.get() {
            pending_cancel.set(Some(original_theme_cancel.clone()));
        }
        *result_cancel.borrow_mut() = None;
        dialog_cancel.clone().hide();
    });

    let result_close = result.clone();
    let (pending_close, previewed_close) = (pending_preview.clone(), previewed.clone());
    dialog.set_callback(move |w| {
        // Revert to original theme on close (X button)
        if previewed_close.get() {
            pending_close.set(Some(original_theme.clone()));
        }
        *result_close.borrow_mut() = None;
        w.hide();
    });

    SettingsUi {
        dialog,
        result,
        pending_manage,
        pending_preview,
        theme_light_choice,
        theme_dark_choice,
        light_choices,
        dark_choices,
        text_rgb: (tr, tg, tb),
        scroll: vpack,
    }
}

/// The entries of a syntax-theme choice: the built-ins, the custom themes,
/// and — so Save never silently rewrites it — the slot's current custom
/// reference when that theme is not loaded.
fn theme_choices(
    current: &ThemeRef,
    custom_themes: &[(ThemeRef, String)],
) -> Rc<RefCell<Vec<(ThemeRef, String)>>> {
    let mut choices: Vec<(ThemeRef, String)> = SyntaxTheme::all()
        .iter()
        .map(|t| (ThemeRef::Builtin(*t), t.display_name().to_string()))
        .chain(custom_themes.iter().cloned())
        .collect();
    if let ThemeRef::Custom(id) = current
        && !choices.iter().any(|(r, _)| r == current)
    {
        choices.push((current.clone(), format!("{id} (missing)")));
    }
    Rc::new(RefCell::new(choices))
}

/// Rebuild a syntax-theme choice after the theme manager changed the custom
/// themes: keep the selected theme if it still exists, else select the
/// light (`is_dark == false`) or dark default.
fn refresh_choice(
    widget: &mut Choice,
    choices: &RefCell<Vec<(ThemeRef, String)>>,
    custom_themes: &[(ThemeRef, String)],
    is_dark: bool,
) {
    let selected = choice_at(&choices.borrow(), widget.value());
    let builtin = ThemeRef::Builtin(default_syntax_theme(is_dark));
    let keep = match &selected {
        Some(ThemeRef::Builtin(_)) => selected.clone(),
        Some(custom) if custom_themes.iter().any(|(t, _)| t == custom) => selected.clone(),
        _ => None,
    }
    .unwrap_or(builtin);
    let fresh = theme_choices(&keep, custom_themes).borrow().clone();
    widget.clear();
    for (_, label) in &fresh {
        widget.add_choice(&choice_label(label));
    }
    widget.set_value(choice_index(&fresh, &keep));
    *choices.borrow_mut() = fresh;
}

/// A theme name as a Choice entry: FLTK reads `/` as a submenu, `\\` as an
/// escape, `&` as a shortcut marker and `|` as an entry separator.
fn choice_label(name: &str) -> String {
    name.replace('\\', "\\\\")
        .replace('/', "\\/")
        .replace('&', "&&")
        .replace('|', " ")
}

/// Dropdown index of `theme` (0, the first built-in, if absent).
fn choice_index(choices: &[(ThemeRef, String)], theme: &ThemeRef) -> i32 {
    choices
        .iter()
        .position(|(t, _)| t == theme)
        .and_then(|i| i32::try_from(i).ok())
        .unwrap_or(0)
}

/// The theme at dropdown `index`, if any.
fn choice_at(choices: &[(ThemeRef, String)], index: i32) -> Option<ThemeRef> {
    let index = usize::try_from(index).ok()?;
    choices.get(index).map(|(t, _)| t.clone())
}
