use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::collections::HashSet;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;

use super::buffer_utils::buffer_text_no_leak;
use super::tab_manager::TabManager;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SessionRestore {
    #[default]
    Off,
    SavedFiles,
    Full,
}

#[derive(Serialize, Deserialize)]
pub struct SessionData {
    pub active_index: usize,
    pub documents: Vec<DocumentSession>,
    #[serde(default)]
    pub last_open_directory: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct DocumentSession {
    pub file_path: Option<String>,
    pub display_name: String,
    pub cursor_position: i32,
    pub temp_file: Option<String>,
    pub was_dirty: bool,
}

/// Returns the session directory path: data_dir/ferrispad/session/
pub fn session_dir() -> PathBuf {
    let mut path = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
    path.push("ferrispad");
    path.push("session");
    path
}

/// Save the current session to disk.
pub fn save_session(tab_manager: &TabManager, mode: SessionRestore, last_open_directory: Option<&str>) -> Result<(), String> {
    if mode == SessionRestore::Off {
        return Ok(());
    }

    let docs = tab_manager.documents();

    // Don't overwrite an existing session if this instance has nothing meaningful
    // (e.g. a single empty untitled doc from a second app instance)
    let is_trivial = docs.len() == 1
        && docs[0].file_path.is_none()
        && buffer_text_no_leak(&docs[0].buffer).is_empty();
    if is_trivial {
        return Ok(());
    }

    let dir = session_dir();
    fs::create_dir_all(&dir).map_err(|e| format!("Failed to create session dir: {}", e))?;
    let active_id = tab_manager.active_id();
    let active_index = active_id
        .and_then(|id| docs.iter().position(|d| d.id == id))
        .unwrap_or(0);

    let mut doc_sessions = Vec::new();

    for doc in docs {
        let is_dirty = doc.is_dirty();
        let has_path = doc.file_path.is_some();

        match mode {
            SessionRestore::SavedFiles => {
                if has_path {
                    doc_sessions.push(DocumentSession {
                        file_path: doc.file_path.clone(),
                        display_name: doc.display_name.clone(),
                        cursor_position: doc.cursor_position,
                        temp_file: None,
                        was_dirty: false,
                    });
                }
            }
            SessionRestore::Full => {
                let content = buffer_text_no_leak(&doc.buffer);

                // Skip empty untitled docs entirely
                if !has_path && content.is_empty() {
                    continue;
                }

                let temp_file = if is_dirty || !has_path {
                    let hash = make_hash(&doc.display_name, doc.id.0);
                    let filename = format!("{:016x}.tmp", hash);
                    let temp_path = dir.join(&filename);
                    fs::write(&temp_path, &content)
                        .map_err(|e| format!("Failed to write temp file: {}", e))?;
                    Some(filename)
                } else {
                    None
                };

                doc_sessions.push(DocumentSession {
                    file_path: doc.file_path.clone(),
                    display_name: doc.display_name.clone(),
                    cursor_position: doc.cursor_position,
                    temp_file,
                    was_dirty: is_dirty,
                });
            }
            SessionRestore::Off => unreachable!(),
        }
    }

    // Merge with existing session: keep docs from other instances that
    // aren't open in this one, so closing one instance doesn't erase another's tabs.
    let session_file = dir.join("session.json");
    if let Ok(existing_json) = fs::read_to_string(&session_file) {
        if let Ok(existing) = serde_json::from_str::<SessionData>(&existing_json) {
            // Collect file paths this instance knows about (owned to avoid borrow conflict)
            let our_paths: HashSet<String> = doc_sessions
                .iter()
                .filter_map(|d| d.file_path.clone())
                .collect();

            for doc in existing.documents {
                match &doc.file_path {
                    Some(path) if !our_paths.contains(path) => {
                        // Saved file from another instance — keep it
                        doc_sessions.push(doc);
                    }
                    None if mode == SessionRestore::Full && doc.temp_file.is_some() => {
                        // Untitled doc with content from another instance — keep it
                        doc_sessions.push(doc);
                    }
                    _ => {} // duplicate or empty — skip
                }
            }
        }
    }

    let session_data = SessionData {
        active_index,
        documents: doc_sessions,
        last_open_directory: last_open_directory.map(|s| s.to_string()),
    };

    let json = serde_json::to_string_pretty(&session_data)
        .map_err(|e| format!("Failed to serialize session: {}", e))?;

    fs::write(&session_file, json)
        .map_err(|e| format!("Failed to write session file: {}", e))?;

    Ok(())
}

/// Load session data from disk.
pub fn load_session(mode: SessionRestore) -> Option<SessionData> {
    if mode == SessionRestore::Off {
        return None;
    }

    let session_file = session_dir().join("session.json");
    let contents = fs::read_to_string(&session_file).ok()?;
    let session_data: SessionData = serde_json::from_str(&contents).ok()?;

    if session_data.documents.is_empty() {
        return None;
    }

    Some(session_data)
}

/// Delete session.json and all .tmp files in the session directory.
pub fn clear_session() {
    let dir = session_dir();
    let session_file = dir.join("session.json");
    let _ = fs::remove_file(&session_file);

    if let Ok(entries) = fs::read_dir(&dir) {
        for entry in entries.flatten() {
            if let Some(ext) = entry.path().extension() {
                if ext == "tmp" {
                    let _ = fs::remove_file(entry.path());
                }
            }
        }
    }
}

/// Read temp file content from the session directory.
pub fn read_temp_file(temp_file: &str) -> Option<String> {
    let path = session_dir().join(temp_file);
    fs::read_to_string(&path).ok()
}

fn make_hash(name: &str, id: u64) -> u64 {
    let mut hasher = DefaultHasher::new();
    name.hash(&mut hasher);
    id.hash(&mut hasher);
    // Include a timestamp-like value to avoid collisions across sessions
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
        .hash(&mut hasher);
    hasher.finish()
}
