use std::cell::RefCell;
use std::rc::Rc;

use fltk::{
    app::Sender,
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

use super::document::DocumentId;
use super::tab_manager::TabManager;
use super::platform::detect_system_dark_mode;
use super::file_filters::{get_all_files_filter, get_text_files_filter_multiline};
use super::messages::Message;
use super::settings::{AppSettings, FontChoice, ThemeMode};
use super::updater::ReleaseInfo;
use crate::ui::dialogs::settings_dialog::show_settings_dialog;
use crate::ui::file_dialogs::{native_open_dialog, native_save_dialog};
use crate::ui::tab_bar::TabBar;
use crate::ui::theme::apply_theme;
#[cfg(target_os = "windows")]
use crate::ui::theme::set_windows_titlebar_theme;

pub struct AppState {
    pub tab_manager: TabManager,
    pub tabs_enabled: bool,
    pub tab_bar: Option<TabBar>,
    pub editor: TextEditor,
    pub window: Window,
    pub menu: MenuBar,
    pub flex: Flex,
    pub update_banner_frame: Frame,
    pub sender: Sender<Message>,
    pub settings: Rc<RefCell<AppSettings>>,
    pub dark_mode: bool,
    pub show_linenumbers: bool,
    pub word_wrap: bool,
    pub pending_update: Option<ReleaseInfo>,
}

impl AppState {
    pub fn new(
        editor: TextEditor,
        window: Window,
        menu: MenuBar,
        flex: Flex,
        update_banner_frame: Frame,
        sender: Sender<Message>,
        settings: Rc<RefCell<AppSettings>>,
        dark_mode: bool,
        show_linenumbers: bool,
        word_wrap: bool,
        tabs_enabled: bool,
        tab_bar: Option<TabBar>,
    ) -> Self {
        let mut tab_manager = TabManager::new();
        tab_manager.add_untitled();

        Self {
            tab_manager,
            tabs_enabled,
            tab_bar,
            editor,
            window,
            menu,
            flex,
            update_banner_frame,
            sender,
            settings,
            dark_mode,
            show_linenumbers,
            word_wrap,
            pending_update: None,
        }
    }

    /// Get the active document's buffer
    pub fn active_buffer(&self) -> TextBuffer {
        self.tab_manager
            .active_buffer()
            .expect("No active document")
    }

    /// Bind the active document's buffer to the editor
    pub fn bind_active_buffer(&mut self) {
        if let Some(doc) = self.tab_manager.active_doc() {
            self.editor.set_buffer(doc.buffer.clone());
        }
    }

    /// Update the window title based on active document
    pub fn update_window_title(&mut self) {
        if let Some(doc) = self.tab_manager.active_doc() {
            let prefix = if doc.is_dirty() { "*" } else { "" };
            self.window.set_label(&format!(
                "{}{} - \u{1f980} FerrisPad",
                prefix, doc.display_name
            ));
        } else {
            self.window
                .set_label("Untitled - \u{1f980} FerrisPad");
        }
    }

    /// Switch the editor to display a different document
    pub fn switch_to_document(&mut self, id: DocumentId) {
        // Save current doc's cursor/scroll state
        if let Some(current) = self.tab_manager.active_doc_mut() {
            current.cursor_position = self.editor.insert_position();
        }

        // Set new active
        self.tab_manager.set_active(id);

        // Bind new buffer and restore state
        if let Some(doc) = self.tab_manager.active_doc() {
            let buffer = doc.buffer.clone();
            let cursor = doc.cursor_position;
            self.editor.set_buffer(buffer);
            self.editor.set_insert_position(cursor);
            self.editor.show_insert_position();
        }

        self.update_window_title();
    }

    /// Rebuild the tab bar UI from current documents
    pub fn rebuild_tab_bar(&mut self) {
        if let Some(ref mut tab_bar) = self.tab_bar {
            let active_id = self.tab_manager.active_id();
            tab_bar.rebuild(
                self.tab_manager.documents(),
                active_id,
                &self.sender,
                self.dark_mode,
            );
        }
    }

    /// Close a tab by id. Returns true if the app should exit (no tabs remaining).
    pub fn close_tab(&mut self, id: DocumentId) -> bool {
        // Check if document is dirty
        if let Some(doc) = self.tab_manager.doc_by_id(id) {
            if doc.is_dirty() {
                let name = doc.display_name.clone();
                let choice = dialog::choice2_default(
                    &format!("\"{}\" has unsaved changes.", name),
                    "Save",
                    "Discard",
                    "Cancel",
                );

                match choice {
                    Some(0) => {
                        // Save first — need to temporarily switch if not active
                        let was_active = self.tab_manager.active_id();
                        if was_active != Some(id) {
                            self.switch_to_document(id);
                        }
                        self.file_save();
                        // If still dirty (save was cancelled), abort close
                        if let Some(doc) = self.tab_manager.doc_by_id(id) {
                            if doc.is_dirty() {
                                // Restore previous active if we switched
                                if let Some(prev) = was_active {
                                    if prev != id {
                                        self.switch_to_document(prev);
                                    }
                                }
                                return false;
                            }
                        }
                    }
                    Some(1) => {} // Discard — proceed with close
                    _ => return false, // Cancel
                }
            }
        }

        self.tab_manager.remove(id);

        if self.tab_manager.count() == 0 {
            return true; // No tabs remain, app should exit
        }

        // Switch to the newly active document
        if let Some(active_id) = self.tab_manager.active_id() {
            self.switch_to_document(active_id);
        }
        self.rebuild_tab_bar();
        false
    }

    // --- File operations ---

    pub fn open_file(&mut self, path: String) {
        match fs::read_to_string(&path) {
            Ok(content) => {
                if self.tabs_enabled {
                    // Check if file is already open in a tab
                    if let Some(existing_id) = self.tab_manager.find_by_path(&path) {
                        self.switch_to_document(existing_id);
                        self.rebuild_tab_bar();
                        return;
                    }
                    let id = self.tab_manager.add_from_file(path, &content);
                    self.switch_to_document(id);
                    self.rebuild_tab_bar();
                } else {
                    // Classic single-doc mode: replace current buffer
                    if let Some(doc) = self.tab_manager.active_doc_mut() {
                        doc.buffer.set_text(&content);
                        doc.has_unsaved_changes.set(false);
                        doc.file_path = Some(path.clone());
                        doc.update_display_name();
                    }
                    self.update_window_title();
                }
            }
            Err(e) => dialog::alert_default(&format!("Error opening file: {}", e)),
        }
    }

    pub fn file_new(&mut self) {
        if self.tabs_enabled {
            let id = self.tab_manager.add_untitled();
            self.switch_to_document(id);
            self.rebuild_tab_bar();
        } else {
            // Classic single-doc mode: clear current buffer
            if let Some(doc) = self.tab_manager.active_doc_mut() {
                doc.buffer.set_text("");
                doc.has_unsaved_changes.set(false);
                doc.file_path = None;
                doc.display_name = "Untitled".to_string();
            }
            self.update_window_title();
        }
    }

    pub fn file_open(&mut self) {
        if let Some(path) = native_open_dialog("", &get_text_files_filter_multiline()) {
            self.open_file(path);
        }
    }

    pub fn file_save(&mut self) {
        let (file_path, text) = {
            if let Some(doc) = self.tab_manager.active_doc() {
                (doc.file_path.clone(), doc.buffer.text())
            } else {
                return;
            }
        };

        if let Some(ref path) = file_path {
            match fs::write(path, &text) {
                Ok(_) => {
                    if let Some(doc) = self.tab_manager.active_doc_mut() {
                        doc.mark_clean();
                    }
                    self.update_window_title();
                    self.rebuild_tab_bar();
                }
                Err(e) => dialog::alert_default(&format!("Error saving file: {}", e)),
            }
        } else {
            self.file_save_as();
        }
    }

    pub fn file_save_as(&mut self) {
        let text = {
            if let Some(doc) = self.tab_manager.active_doc() {
                doc.buffer.text()
            } else {
                return;
            }
        };

        if let Some(path) = native_save_dialog("All Files", &get_all_files_filter()) {
            match fs::write(&path, &text) {
                Ok(_) => {
                    if let Some(doc) = self.tab_manager.active_doc_mut() {
                        doc.file_path = Some(path);
                        doc.update_display_name();
                        doc.mark_clean();
                    }
                    self.update_window_title();
                    self.rebuild_tab_bar();
                }
                Err(e) => dialog::alert_default(&format!("Error saving file: {}", e)),
            }
        }
    }

    /// Handle quit request. Returns `true` if the app should exit.
    pub fn file_quit(&mut self) -> bool {
        if self.tabs_enabled {
            // Check all documents for unsaved changes
            let dirty_docs: Vec<DocumentId> = self
                .tab_manager
                .documents()
                .iter()
                .filter(|d| d.is_dirty())
                .map(|d| d.id)
                .collect();

            if dirty_docs.is_empty() {
                return true;
            }

            let choice = dialog::choice2_default(
                "You have unsaved changes in one or more tabs.",
                "Save All",
                "Quit Without Saving",
                "Cancel",
            );

            match choice {
                Some(0) => {
                    // Save all dirty docs
                    for id in dirty_docs {
                        self.switch_to_document(id);
                        self.file_save();
                        // If save was cancelled (still dirty), abort quit
                        if let Some(doc) = self.tab_manager.doc_by_id(id) {
                            if doc.is_dirty() {
                                return false;
                            }
                        }
                    }
                    true
                }
                Some(1) => true,
                _ => false,
            }
        } else {
            // Classic single-doc mode
            let is_dirty = self
                .tab_manager
                .active_doc()
                .map_or(false, |d| d.is_dirty());

            if is_dirty {
                let choice = dialog::choice2_default(
                    "You have unsaved changes.",
                    "Save",
                    "Quit Without Saving",
                    "Cancel",
                );

                match choice {
                    Some(0) => {
                        self.file_save();
                        !self
                            .tab_manager
                            .active_doc()
                            .map_or(false, |d| d.is_dirty())
                    }
                    Some(1) => true,
                    _ => false,
                }
            } else {
                true
            }
        }
    }

    /// Switch to next tab
    pub fn switch_to_next_tab(&mut self) {
        if let Some(next_id) = self.tab_manager.next_doc_id() {
            self.switch_to_document(next_id);
            self.rebuild_tab_bar();
        }
    }

    /// Switch to previous tab
    pub fn switch_to_previous_tab(&mut self) {
        if let Some(prev_id) = self.tab_manager.prev_doc_id() {
            self.switch_to_document(prev_id);
            self.rebuild_tab_bar();
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
        if let Some(ref mut tab_bar) = self.tab_bar {
            tab_bar.apply_theme(self.dark_mode);
        }
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
        if let Some(ref mut tab_bar) = self.tab_bar {
            tab_bar.apply_theme(is_dark);
        }
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
