use std::cell::{Cell, RefCell};
use std::rc::Rc;

use fltk::{
    dialog,
    enums::Font,
    frame::Frame,
    group::Flex,
    menu::MenuBar,
    prelude::*,
    text::{TextBuffer, TextEditor, WrapMode},
    window::Window,
};
use std::fs;

use super::platform::detect_system_dark_mode;
use super::text_ops::extract_filename;
use super::file_filters::{get_all_files_filter, get_text_files_filter_multiline};
use super::settings::{AppSettings, FontChoice, ThemeMode};
use super::updater::ReleaseInfo;
use crate::ui::dialogs::settings_dialog::show_settings_dialog;
use crate::ui::file_dialogs::{native_open_dialog, native_save_dialog};
use crate::ui::theme::apply_theme;
#[cfg(target_os = "windows")]
use crate::ui::theme::set_windows_titlebar_theme;

pub struct AppState {
    pub buffer: TextBuffer,
    pub editor: TextEditor,
    pub window: Window,
    pub menu: MenuBar,
    pub flex: Flex,
    pub update_banner_frame: Frame,
    pub current_file_path: Option<String>,
    pub has_unsaved_changes: Rc<Cell<bool>>,
    pub settings: Rc<RefCell<AppSettings>>,
    pub dark_mode: bool,
    pub show_linenumbers: bool,
    pub word_wrap: bool,
    pub pending_update: Option<ReleaseInfo>,
}

impl AppState {
    pub fn new(
        buffer: TextBuffer,
        editor: TextEditor,
        window: Window,
        menu: MenuBar,
        flex: Flex,
        update_banner_frame: Frame,
        has_unsaved_changes: Rc<Cell<bool>>,
        settings: Rc<RefCell<AppSettings>>,
        dark_mode: bool,
        show_linenumbers: bool,
        word_wrap: bool,
    ) -> Self {
        Self {
            buffer,
            editor,
            window,
            menu,
            flex,
            update_banner_frame,
            current_file_path: None,
            has_unsaved_changes,
            settings,
            dark_mode,
            show_linenumbers,
            word_wrap,
            pending_update: None,
        }
    }

    // --- File operations ---

    pub fn open_file(&mut self, path: String) {
        match fs::read_to_string(&path) {
            Ok(content) => {
                self.buffer.set_text(&content);
                let filename = extract_filename(&path);
                self.window
                    .set_label(&format!("{} - \u{1f980} FerrisPad", filename));
                self.has_unsaved_changes.set(false);
                self.current_file_path = Some(path);
            }
            Err(e) => dialog::alert_default(&format!("Error opening file: {}", e)),
        }
    }

    pub fn file_new(&mut self) {
        self.buffer.set_text("");
        self.window
            .set_label("Untitled - \u{1f980} FerrisPad");
        self.has_unsaved_changes.set(false);
        self.current_file_path = None;
    }

    pub fn file_open(&mut self) {
        if let Some(path) = native_open_dialog("", &get_text_files_filter_multiline()) {
            self.open_file(path);
        }
    }

    pub fn file_save(&mut self) {
        if let Some(ref path) = self.current_file_path {
            match fs::write(path, self.buffer.text()) {
                Ok(_) => self.has_unsaved_changes.set(false),
                Err(e) => dialog::alert_default(&format!("Error saving file: {}", e)),
            }
        } else {
            self.file_save_as();
        }
    }

    pub fn file_save_as(&mut self) {
        if let Some(path) = native_save_dialog("All Files", &get_all_files_filter()) {
            match fs::write(&path, self.buffer.text()) {
                Ok(_) => {
                    let filename = extract_filename(&path);
                    self.window
                        .set_label(&format!("{} - \u{1f980} FerrisPad", filename));
                    self.has_unsaved_changes.set(false);
                    self.current_file_path = Some(path);
                }
                Err(e) => dialog::alert_default(&format!("Error saving file: {}", e)),
            }
        }
    }

    /// Handle quit request. Returns `true` if the app should exit.
    pub fn file_quit(&mut self) -> bool {
        if self.has_unsaved_changes.get() {
            let choice = dialog::choice2_default(
                "You have unsaved changes.",
                "Save",
                "Quit Without Saving",
                "Cancel",
            );

            match choice {
                Some(0) => {
                    self.file_save();
                    !self.has_unsaved_changes.get()
                }
                Some(1) => true,
                _ => false,
            }
        } else {
            true
        }
    }

    // --- View toggles ---

    pub fn toggle_line_numbers(&mut self) {
        self.show_linenumbers = !self.show_linenumbers;
        if self.show_linenumbers {
            self.editor.set_linenumber_width(40);
        } else {
            self.editor.set_linenumber_width(0);
        }
        self.editor.redraw();
    }

    pub fn toggle_word_wrap(&mut self) {
        self.word_wrap = !self.word_wrap;
        if self.word_wrap {
            self.editor.wrap_mode(WrapMode::AtBounds, 0);
        } else {
            self.editor.wrap_mode(WrapMode::None, 0);
        }
        self.editor.redraw();
    }

    pub fn toggle_dark_mode(&mut self) {
        self.dark_mode = !self.dark_mode;
        apply_theme(
            &mut self.editor,
            &mut self.window,
            &mut self.menu,
            Some(&mut self.update_banner_frame),
            self.dark_mode,
        );
        #[cfg(target_os = "windows")]
        set_windows_titlebar_theme(&self.window, self.dark_mode);
    }

    // --- Format ---

    pub fn set_font(&mut self, font: Font) {
        self.editor.set_text_font(font);
        self.editor.redraw();
    }

    pub fn set_font_size(&mut self, size: i32) {
        self.editor.set_text_size(size);
        self.editor.redraw();
    }

    // --- Settings ---

    pub fn open_settings(&mut self) {
        let current = self.settings.borrow().clone();
        if let Some(new_settings) = show_settings_dialog(&current) {
            if let Err(e) = new_settings.save() {
                dialog::alert_default(&format!("Failed to save settings: {}", e));
                return;
            }
            self.apply_settings(new_settings);
        }
    }

    pub fn apply_settings(&mut self, new_settings: AppSettings) {
        // Apply theme
        let is_dark = match new_settings.theme_mode {
            ThemeMode::Light => false,
            ThemeMode::Dark => true,
            ThemeMode::SystemDefault => detect_system_dark_mode(),
        };
        self.dark_mode = is_dark;
        apply_theme(
            &mut self.editor,
            &mut self.window,
            &mut self.menu,
            Some(&mut self.update_banner_frame),
            is_dark,
        );
        #[cfg(target_os = "windows")]
        set_windows_titlebar_theme(&self.window, is_dark);
        self.update_menu_checkbox("View/Toggle Dark Mode", is_dark);

        // Apply font
        let font = match new_settings.font {
            FontChoice::ScreenBold => Font::ScreenBold,
            FontChoice::Courier => Font::Courier,
            FontChoice::HelveticaMono => Font::Screen,
        };
        self.editor.set_text_font(font);
        self.editor.set_text_size(new_settings.font_size as i32);

        // Apply line numbers
        self.show_linenumbers = new_settings.line_numbers_enabled;
        if self.show_linenumbers {
            self.editor.set_linenumber_width(40);
        } else {
            self.editor.set_linenumber_width(0);
        }
        self.update_menu_checkbox("View/Toggle Line Numbers", self.show_linenumbers);

        // Apply word wrap
        self.word_wrap = new_settings.word_wrap_enabled;
        if self.word_wrap {
            self.editor.wrap_mode(WrapMode::AtBounds, 0);
        } else {
            self.editor.wrap_mode(WrapMode::None, 0);
        }
        self.update_menu_checkbox("View/Toggle Word Wrap", self.word_wrap);

        self.editor.redraw();

        // Store updated settings
        *self.settings.borrow_mut() = new_settings;
    }

    fn update_menu_checkbox(&self, path: &str, checked: bool) {
        let idx = self.menu.find_index(path);
        if idx >= 0 {
            if let Some(mut item) = self.menu.at(idx) {
                if checked {
                    item.set();
                } else {
                    item.clear();
                }
            }
        }
    }

    // --- Update banner ---

    pub fn show_update_banner(&mut self, version: &str) {
        self.update_banner_frame.set_label(&format!(
            "  \u{1f980} FerrisPad {} is available - Click to view details or press ESC to dismiss",
            version
        ));
        self.update_banner_frame.show();
        self.flex.fixed(&self.update_banner_frame, 30);
        self.window.redraw();
    }

    pub fn hide_update_banner(&mut self) {
        self.update_banner_frame.hide();
        self.flex.fixed(&self.update_banner_frame, 0);
        self.window.redraw();
    }
}
