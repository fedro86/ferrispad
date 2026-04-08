use std::cell::RefCell;
use std::fs;
use std::rc::Rc;
use std::time::Instant;

use super::tabs::{GroupColor, GroupId, TabGroup, TabManager};
use crate::app::domain::document::DocumentId;
use crate::app::domain::settings::AppSettings;
use crate::app::services::file_size::{FileSizeCheck, check_file_size, format_size};
use crate::app::services::session::{self, DEFAULT_SESSION_NAME, SessionRestore};

/// Result of restoring a session. AppState uses this to perform
/// cross-cutting operations (highlighting, buffer binding, etc.).
pub struct RestoreResult {
    pub last_open_directory: Option<String>,
    /// Documents that need syntax highlighting: (id, path)
    pub highlight_docs: Vec<(DocumentId, String)>,
}

/// Manages session persistence (auto-save, restore, dirty tracking).
pub struct SessionController {
    last_auto_save: Instant,
    session_dirty: bool,
    /// Name of the currently active session (e.g. "default", "my-project").
    current_session_name: String,
}

impl Default for SessionController {
    fn default() -> Self {
        Self {
            last_auto_save: Instant::now(),
            session_dirty: false,
            current_session_name: DEFAULT_SESSION_NAME.to_string(),
        }
    }
}

impl SessionController {
    /// Create a new SessionController with the given session name.
    pub fn with_session_name(name: String) -> Self {
        Self {
            current_session_name: name,
            ..Default::default()
        }
    }

    /// Get the current session name.
    pub fn current_session_name(&self) -> &str {
        &self.current_session_name
    }

    /// Set the current session name.
    pub fn set_session_name(&mut self, name: String) {
        self.current_session_name = name;
    }

    /// Mark that the session state has changed and should be auto-saved.
    pub fn mark_dirty(&mut self) {
        self.session_dirty = true;
    }

    /// Force-save the current session immediately, regardless of dirty state.
    pub fn force_save(
        &mut self,
        tab_manager: &TabManager,
        settings: &Rc<RefCell<AppSettings>>,
        last_open_directory: Option<&str>,
    ) {
        let session_mode = settings.borrow().session_restore;
        if session_mode == SessionRestore::Off {
            return;
        }
        if let Err(e) = session::save_session(
            tab_manager,
            session_mode,
            last_open_directory,
            &self.current_session_name,
        ) {
            eprintln!("Force-save session failed: {}", e);
        }
        self.session_dirty = false;
        self.last_auto_save = Instant::now();
    }

    /// Auto-save the session every 30 seconds if something changed.
    pub fn auto_save_if_needed(
        &mut self,
        tab_manager: &TabManager,
        settings: &Rc<RefCell<AppSettings>>,
        last_open_directory: Option<&str>,
    ) {
        const AUTO_SAVE_INTERVAL_SECS: u64 = 30;

        if !self.session_dirty {
            return;
        }
        if self.last_auto_save.elapsed().as_secs() < AUTO_SAVE_INTERVAL_SECS {
            return;
        }

        let session_mode = settings.borrow().session_restore;
        if session_mode == SessionRestore::Off {
            return;
        }

        if let Err(e) = session::save_session(
            tab_manager,
            session_mode,
            last_open_directory,
            &self.current_session_name,
        ) {
            eprintln!("Auto-save session failed: {}", e);
        }
        self.session_dirty = false;
        self.last_auto_save = Instant::now();
    }

    /// Load and restore session data into the tab manager.
    /// Returns `None` if session restore is off/disabled or no session exists.
    /// AppState must still call bind_active_buffer, update_window_title,
    /// rebuild_tab_bar, and plugin hooks after this returns.
    pub fn restore(
        tab_manager: &mut TabManager,
        settings: &Rc<RefCell<AppSettings>>,
        tabs_enabled: bool,
        session_name: &str,
    ) -> Option<RestoreResult> {
        let mode = settings.borrow().session_restore;
        if mode == SessionRestore::Off || !tabs_enabled {
            return None;
        }

        let session_data = session::load_session(mode, session_name)?;

        let last_open_directory = session_data.last_open_directory.clone();

        // Remove the initial untitled document
        if let Some(id) = tab_manager.active_id() {
            tab_manager.remove(id);
        }

        // Restore groups and build index -> GroupId mapping
        let restored_groups: Vec<TabGroup> = session_data
            .groups
            .iter()
            .map(|gs| TabGroup {
                id: GroupId(0), // placeholder, restore_groups assigns real ids
                name: gs.name.clone(),
                color: GroupColor::from_str(&gs.color).unwrap_or(GroupColor::Grey),
                collapsed: gs.collapsed,
            })
            .collect();
        let group_ids = tab_manager.restore_groups(restored_groups);

        let mut highlight_docs = Vec::new();
        let target_index = session_data.active_index;

        for (i, doc_session) in session_data.documents.iter().enumerate() {
            let group_id = doc_session
                .group_index
                .and_then(|idx| group_ids.get(idx).copied());

            if let Some(ref path) = doc_session.file_path {
                // Skip files that are too large (don't show dialogs at startup)
                let path_ref = std::path::Path::new(path);
                let warning_mb = settings.borrow().large_file_warning_mb as u64;
                let max_editable_mb = settings.borrow().max_editable_size_mb as u64;
                if let Ok(FileSizeCheck::TooLarge(size)) =
                    check_file_size(path_ref, warning_mb, max_editable_mb)
                {
                    eprintln!(
                        "[session] Skipping '{}' ({}) - exceeds size limit",
                        path,
                        format_size(size)
                    );
                    continue;
                }

                if let Ok(content) = fs::read_to_string(path) {
                    let id = tab_manager.add_from_file(path.clone(), &content);

                    highlight_docs.push((id, path.clone()));

                    if mode == SessionRestore::Full
                        && let Some(ref temp_file) = doc_session.temp_file
                        && let Some(temp_content) = session::read_temp_file(temp_file, session_name)
                        && let Some(doc) = tab_manager.doc_by_id_mut(id)
                    {
                        // Only overwrite the buffer if the temp content actually
                        // differs from the disk file (i.e., there are real unsaved
                        // edits). set_text() fires the modify callback which marks
                        // the document dirty, so skip it when content is identical.
                        if temp_content != content {
                            doc.buffer.set_text(&temp_content);
                        }
                    }

                    if let Some(doc) = tab_manager.doc_by_id_mut(id) {
                        doc.cursor_position = doc_session.cursor_position;
                        doc.group_id = group_id;
                        doc.disk_mtime = fs::metadata(path).ok().and_then(|m| m.modified().ok());
                    }

                    if i == target_index {
                        tab_manager.set_active(id);
                    }
                }
            } else if mode == SessionRestore::Full
                && let Some(ref temp_file) = doc_session.temp_file
                && let Some(temp_content) = session::read_temp_file(temp_file, session_name)
            {
                let id = tab_manager.add_untitled();
                if let Some(doc) = tab_manager.doc_by_id_mut(id) {
                    doc.buffer.set_text(&temp_content);
                    doc.cursor_position = doc_session.cursor_position;
                    doc.group_id = group_id;
                }
                if i == target_index {
                    tab_manager.set_active(id);
                }
            }
        }

        if tab_manager.count() == 0 {
            tab_manager.add_untitled();
        }

        Some(RestoreResult {
            last_open_directory,
            highlight_docs,
        })
    }
}
