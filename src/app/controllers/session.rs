use std::cell::RefCell;
use std::fs;
use std::rc::Rc;
use std::time::Instant;

use super::tabs::{GroupColor, GroupId, TabGroup, TabManager};
use crate::app::domain::document::DocumentId;
use crate::app::domain::settings::AppSettings;
use crate::app::services::file_size::{FileSizeCheck, check_file_size, format_size};
use crate::app::services::session::{self, DEFAULT_SESSION_NAME, SessionData, SessionRestore};

/// Result of restoring a session. AppState uses this to perform
/// cross-cutting operations (highlighting, buffer binding, etc.).
pub struct RestoreResult {
    pub last_open_directory: Option<String>,
    /// Copies (identity, owner) taken over from an instance that is gone
    /// (T0101): see [`SessionController::adopt`].
    pub adopted: Vec<(String, String)>,
    /// Documents that need syntax highlighting: (id, path)
    pub highlight_docs: Vec<(DocumentId, String)>,
}

/// Manages session persistence (auto-save, restore, dirty tracking).
pub struct SessionController {
    last_auto_save: Instant,
    session_dirty: bool,
    /// Name of the currently active session (e.g. "default", "my-project").
    current_session_name: String,
    /// What this instance has had in the current session (T0101).
    ownership: session::Ownership,
}

impl Default for SessionController {
    fn default() -> Self {
        Self {
            last_auto_save: Instant::now(),
            session_dirty: false,
            current_session_name: DEFAULT_SESSION_NAME.to_string(),
            ownership: session::Ownership::default(),
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
        // A different session file: nothing had in it yet (T0101).
        self.ownership = session::Ownership::default();
    }

    /// Save the session now (every save goes through here, T0101): this
    /// instance's documents, merged with what other instances have in the
    /// file, minus the copies this instance closed.
    pub fn save_now(
        &mut self,
        tab_manager: &TabManager,
        mode: SessionRestore,
        last_open_directory: Option<&str>,
    ) -> Result<(), crate::app::infrastructure::error::AppError> {
        session::save_session(
            tab_manager,
            mode,
            last_open_directory,
            &self.current_session_name,
            &mut self.ownership,
        )
    }

    /// Record the copies taken over by a restore (T0101).
    pub fn adopt(&mut self, adopted: Vec<(String, String)>) {
        for (identity, owner) in adopted {
            self.ownership.adopt(identity, owner);
        }
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
        if let Err(e) = self.save_now(tab_manager, session_mode, last_open_directory) {
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

        if let Err(e) = self.save_now(tab_manager, session_mode, last_open_directory) {
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
        Some(Self::restore_data(
            tab_manager,
            settings,
            mode,
            &session_data,
            |temp_file| session::read_temp_file(temp_file, session_name),
        ))
    }

    /// Restore already-loaded session data into the tab manager.
    /// `read_temp` reads an unsaved-content temp file of the session.
    pub fn restore_data(
        tab_manager: &mut TabManager,
        settings: &Rc<RefCell<AppSettings>>,
        mode: SessionRestore,
        session_data: &SessionData,
        read_temp: impl Fn(&str) -> Option<String>,
    ) -> RestoreResult {
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
        let mut adopted = Vec::new();
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
                        && let Some(temp_content) = read_temp(temp_file)
                        && let Some(doc) = tab_manager.doc_by_id_mut(id)
                    {
                        // Only overwrite the buffer if the temp content actually
                        // differs from the disk file (i.e., there are real unsaved
                        // edits). set_text() fires the modify callback which marks
                        // the document dirty, so skip it when content is identical.
                        if temp_content != content {
                            doc.buffer.set_text(&temp_content);
                        }
                        // Keep the session identity (save back to this file),
                        // unless another live instance still writes it
                        // (T0101).
                        let owner = session::entry_owner(doc_session, &session_data);
                        if session::reuse_restored_temp(&owner) {
                            doc.restored_temp_file = Some(temp_file.clone());
                            adopted.push((path.clone(), owner));
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
                && let Some(temp_content) = read_temp(temp_file)
            {
                let id = tab_manager.add_untitled();
                if let Some(doc) = tab_manager.doc_by_id_mut(id) {
                    doc.buffer.set_text(&temp_content);
                    // Keep the session identity (save back to this file),
                    // unless another live instance still writes it (T0101).
                    let owner = session::entry_owner(doc_session, &session_data);
                    if session::reuse_restored_temp(&owner) {
                        doc.restored_temp_file = Some(temp_file.clone());
                        adopted.push((temp_file.clone(), owner));
                    }
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

        RestoreResult {
            last_open_directory,
            adopted,
            highlight_docs,
        }
    }
}

/// Checks that build documents (hence FLTK `TextBuffer`s), so they must run
/// on the process main thread (see `tabs::main_thread_checks`). Called by
/// `tests/fltk_main_thread.rs` (T0089); not part of the public API.
#[doc(hidden)]
pub mod main_thread_checks {
    use super::*;
    use crate::app::domain::messages::Message;
    use crate::app::services::session::DocumentSession;

    fn untitled_session(temp_file: &str) -> DocumentSession {
        DocumentSession {
            file_path: None,
            display_name: "Untitled".to_string(),
            cursor_position: 0,
            temp_file: Some(temp_file.to_string()),
            was_dirty: true,
            group_index: None,
        }
    }

    /// T0091: a Full restore of two unsaved untitled tabs names them
    /// "Untitled" and "Untitled 2", although startup created (and restore
    /// removed) a default untitled document first.
    pub fn untitled_full_restore_numbers_from_one() {
        let (sender, _receiver) = fltk::app::channel::<Message>();
        let mut tabs = TabManager::new(sender);
        tabs.add_untitled(); // AppState::new's default document
        let settings = Rc::new(RefCell::new(AppSettings {
            session_restore: SessionRestore::Full,
            ..AppSettings::default()
        }));
        let data = SessionData {
            version: 1,
            active_index: 0,
            documents: vec![untitled_session("a.tmp"), untitled_session("b.tmp")],
            last_open_directory: None,
            groups: vec![],
            instance_id: None,
        };
        SessionController::restore_data(
            &mut tabs,
            &settings,
            SessionRestore::Full,
            &data,
            |name| Some(format!("unsaved text from {name}")),
        );
        let names: Vec<&str> = tabs
            .documents()
            .iter()
            .map(|d| d.display_name.as_str())
            .collect();
        assert_eq!(names, ["Untitled", "Untitled 2"]);
    }
}
