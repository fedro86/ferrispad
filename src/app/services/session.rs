use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::collections::HashSet;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;

use crate::app::controllers::tabs::TabManager;
use crate::app::infrastructure::buffer::buffer_text_no_leak;
use crate::app::infrastructure::error::AppError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SessionRestore {
    #[default]
    Off,
    SavedFiles,
    Full,
}

const CURRENT_SESSION_VERSION: u32 = 1;

fn default_version() -> u32 {
    CURRENT_SESSION_VERSION
}

#[derive(Serialize, Deserialize)]
pub struct SessionData {
    #[serde(default = "default_version")]
    pub version: u32,
    pub active_index: usize,
    pub documents: Vec<DocumentSession>,
    #[serde(default)]
    pub last_open_directory: Option<String>,
    #[serde(default)]
    pub groups: Vec<GroupSession>,
    #[serde(default)]
    pub instance_id: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct DocumentSession {
    pub file_path: Option<String>,
    pub display_name: String,
    pub cursor_position: i32,
    pub temp_file: Option<String>,
    pub was_dirty: bool,
    #[serde(default)]
    pub group_index: Option<usize>,
}

#[derive(Serialize, Deserialize)]
pub struct GroupSession {
    pub name: String,
    pub color: String,
    pub collapsed: bool,
}

/// Returns the session directory path: data_dir/ferrispad/session/
pub fn session_dir() -> PathBuf {
    let mut path = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
    path.push("ferrispad");
    path.push("session");
    path
}

/// Save the current session to disk.
pub fn save_session(tab_manager: &TabManager, mode: SessionRestore, last_open_directory: Option<&str>) -> Result<(), AppError> {
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
    fs::create_dir_all(&dir)?;
    let active_id = tab_manager.active_id();
    let active_index = active_id
        .and_then(|id| docs.iter().position(|d| d.id == id))
        .unwrap_or(0);

    // Build group sessions and a mapping from GroupId -> index
    let groups = tab_manager.groups();
    let group_sessions: Vec<GroupSession> = groups.iter().map(|g| GroupSession {
        name: g.name.clone(),
        color: g.color.as_str().to_string(),
        collapsed: g.collapsed,
    }).collect();

    let mut doc_sessions = Vec::new();

    for doc in docs {
        let is_dirty = doc.is_dirty();
        let has_path = doc.file_path.is_some();

        // Find group index for this document
        let group_index = doc.group_id.and_then(|gid| {
            groups.iter().position(|g| g.id == gid)
        });

        match mode {
            SessionRestore::SavedFiles => {
                if has_path {
                    doc_sessions.push(DocumentSession {
                        file_path: doc.file_path.clone(),
                        display_name: doc.display_name.clone(),
                        cursor_position: doc.cursor_position,
                        temp_file: None,
                        was_dirty: false,
                        group_index,
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
                    // Use file_path if available (stable across sessions),
                    // otherwise fall back to display_name + id (stable within session)
                    let hash_key = doc.file_path.as_deref().unwrap_or(&doc.display_name);
                    let hash = make_hash(hash_key, doc.id.0);
                    let filename = format!("{:016x}.tmp", hash);
                    let temp_path = dir.join(&filename);
                    fs::write(&temp_path, &content)?;
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
                    group_index,
                });
            }
            SessionRestore::Off => unreachable!(),
        }
    }

    // Merge with existing session: keep docs from other instances that
    // aren't open in this one, so closing one instance doesn't erase another's tabs.
    // Skip merge when we have 0 docs — the user closed all tabs intentionally.
    let instance_id = std::process::id().to_string();
    let session_file = dir.join("session.json");
    if !doc_sessions.is_empty()
        && let Ok(existing_json) = fs::read_to_string(&session_file)
        && let Ok(existing) = serde_json::from_str::<SessionData>(&existing_json)
        && existing.instance_id.as_deref() != Some(&instance_id) {
            // Clone to owned HashSets to allow mutable push below (borrow checker requirement)
            let our_paths: HashSet<String> = doc_sessions
                .iter()
                .filter_map(|d| d.file_path.clone())
                .collect();
            let our_temp_files: HashSet<String> = doc_sessions
                .iter()
                .filter_map(|d| d.temp_file.clone())
                .collect();
            let our_untitled_names: HashSet<String> = doc_sessions
                .iter()
                .filter(|d| d.file_path.is_none())
                .map(|d| d.display_name.clone())
                .collect();

            for doc in existing.documents {
                match &doc.file_path {
                    Some(path) if !our_paths.contains(path) => {
                        // Saved file from another instance — keep it
                        doc_sessions.push(doc);
                    }
                    None if mode == SessionRestore::Full && doc.temp_file.is_some() => {
                        // Untitled doc from another instance — only keep if not a duplicate
                        // Check both temp file and display name to catch the same doc
                        // that may have gotten a new temp file hash due to id change
                        let temp_dup = doc.temp_file.as_ref()
                            .is_some_and(|tf| our_temp_files.contains(tf));
                        let name_dup = our_untitled_names.contains(&doc.display_name);

                        if !temp_dup && !name_dup {
                            doc_sessions.push(doc);
                        }
                    }
                    _ => {} // duplicate or empty — skip
                }
            }
        }

    let session_data = SessionData {
        version: CURRENT_SESSION_VERSION,
        active_index,
        documents: doc_sessions,
        last_open_directory: last_open_directory.map(|s| s.to_string()),
        groups: group_sessions,
        instance_id: Some(instance_id),
    };

    let json = serde_json::to_string_pretty(&session_data)?;

    fs::write(&session_file, json)?;

    // Clean up orphaned temp files (not referenced in current session)
    cleanup_orphaned_temp_files(&session_data, &dir);

    Ok(())
}

/// Remove .tmp files that are no longer referenced by any document in the session.
fn cleanup_orphaned_temp_files(session: &SessionData, dir: &std::path::Path) {
    // Collect all referenced temp files
    let referenced: HashSet<&str> = session
        .documents
        .iter()
        .filter_map(|d| d.temp_file.as_deref())
        .collect();

    // Find and delete orphaned .tmp files
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(filename) = path.file_name().and_then(|n| n.to_str())
                && filename.ends_with(".tmp")
                && !referenced.contains(filename)
            {
                let _ = fs::remove_file(&path);
            }
        }
    }
}

/// Load session data from disk.
pub fn load_session(mode: SessionRestore) -> Option<SessionData> {
    if mode == SessionRestore::Off {
        return None;
    }

    let session_file = session_dir().join("session.json");
    let contents = fs::read_to_string(&session_file).ok()?;
    let session_data: SessionData = serde_json::from_str(&contents).ok()?;

    if session_data.version > CURRENT_SESSION_VERSION {
        eprintln!(
            "Warning: session file version {} is newer than supported version {}",
            session_data.version, CURRENT_SESSION_VERSION
        );
    }

    if session_data.documents.is_empty() {
        return None;
    }

    Some(session_data)
}

/// Read temp file content from the session directory.
pub fn read_temp_file(temp_file: &str) -> Option<String> {
    let path = session_dir().join(temp_file);
    fs::read_to_string(&path).ok()
}

/// Create a stable hash for temp file naming.
/// Uses only name and id - NOT timestamp - so the same document
/// always gets the same temp filename across auto-saves.
fn make_hash(name: &str, id: u64) -> u64 {
    let mut hasher = DefaultHasher::new();
    name.hash(&mut hasher);
    id.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_restore_default() {
        let mode: SessionRestore = SessionRestore::default();
        assert_eq!(mode, SessionRestore::Off);
    }

    #[test]
    fn test_session_data_serialization() {
        let data = SessionData {
            version: 1,
            active_index: 0,
            documents: vec![DocumentSession {
                file_path: Some("/tmp/test.txt".to_string()),
                display_name: "test.txt".to_string(),
                cursor_position: 42,
                temp_file: None,
                was_dirty: false,
                group_index: None,
            }],
            last_open_directory: Some("/tmp".to_string()),
            groups: vec![],
            instance_id: None,
        };

        let json = serde_json::to_string(&data).unwrap();
        let loaded: SessionData = serde_json::from_str(&json).unwrap();

        assert_eq!(loaded.version, 1);
        assert_eq!(loaded.active_index, 0);
        assert_eq!(loaded.documents.len(), 1);
        assert_eq!(loaded.documents[0].file_path, Some("/tmp/test.txt".to_string()));
        assert_eq!(loaded.documents[0].cursor_position, 42);
    }

    #[test]
    fn test_session_data_missing_version_uses_default() {
        // Old format without version field
        let json = r#"{
            "active_index": 0,
            "documents": []
        }"#;

        let loaded: SessionData = serde_json::from_str(json).unwrap();
        assert_eq!(loaded.version, CURRENT_SESSION_VERSION);
    }

    #[test]
    fn test_document_session_serialization() {
        let doc = DocumentSession {
            file_path: None,
            display_name: "Untitled".to_string(),
            cursor_position: 0,
            temp_file: Some("abc123.tmp".to_string()),
            was_dirty: true,
            group_index: Some(0),
        };

        let json = serde_json::to_string(&doc).unwrap();
        let loaded: DocumentSession = serde_json::from_str(&json).unwrap();

        assert!(loaded.file_path.is_none());
        assert_eq!(loaded.display_name, "Untitled");
        assert_eq!(loaded.temp_file, Some("abc123.tmp".to_string()));
        assert!(loaded.was_dirty);
        assert_eq!(loaded.group_index, Some(0));
    }

    #[test]
    fn test_group_session_serialization() {
        let group = GroupSession {
            name: "Test Group".to_string(),
            color: "coral".to_string(),
            collapsed: true,
        };

        let json = serde_json::to_string(&group).unwrap();
        let loaded: GroupSession = serde_json::from_str(&json).unwrap();

        assert_eq!(loaded.name, "Test Group");
        assert_eq!(loaded.color, "coral");
        assert!(loaded.collapsed);
    }

    #[test]
    fn test_load_session_off_returns_none() {
        let result = load_session(SessionRestore::Off);
        assert!(result.is_none());
    }

    #[test]
    fn test_session_dir_returns_path() {
        let dir = session_dir();
        assert!(dir.ends_with("ferrispad/session") || dir.ends_with("ferrispad\\session"));
    }
}
