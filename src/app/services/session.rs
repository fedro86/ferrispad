use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

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

/// Default session name used when no --session flag is provided.
pub const DEFAULT_SESSION_NAME: &str = "default";

/// Returns the base session directory: data_dir/ferrispad/session/
fn session_base_dir() -> PathBuf {
    let mut path = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
    path.push("ferrispad");
    path.push("session");
    path
}

/// Returns the session directory for a named session: data_dir/ferrispad/session/{name}/
pub fn session_dir(name: &str) -> PathBuf {
    session_base_dir().join(name)
}

/// List all available session names (subdirectories of the session base dir).
pub fn list_sessions() -> Vec<String> {
    let base = session_base_dir();
    let mut names: Vec<String> = fs::read_dir(&base)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();
    names.sort();
    names
}

/// Delete a named session (removes its directory and all temp files).
pub fn delete_session(name: &str) -> Result<(), AppError> {
    let dir = session_dir(name);
    if dir.exists() {
        fs::remove_dir_all(&dir)?;
    }
    Ok(())
}

/// Migrate old flat session layout into the new named-session directory structure.
/// If `session_base_dir()/session.json` exists (old format), moves it and all
/// `.tmp` files into `session_base_dir()/default/`.
pub fn migrate_flat_session() {
    let base = session_base_dir();
    let old_session_file = base.join("session.json");
    if !old_session_file.exists() {
        return;
    }
    // Already migrated if default/ exists
    let default_dir = base.join(DEFAULT_SESSION_NAME);
    if default_dir.join("session.json").exists() {
        // Old file is stale leftover — remove it
        let _ = fs::remove_file(&old_session_file);
        return;
    }
    let _ = fs::create_dir_all(&default_dir);
    let _ = fs::rename(&old_session_file, default_dir.join("session.json"));
    // Move all .tmp files
    if let Ok(entries) = fs::read_dir(&base) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file()
                && path.extension().map(|e| e == "tmp").unwrap_or(false)
                && let Some(name) = path.file_name()
            {
                let _ = fs::rename(&path, default_dir.join(name));
            }
        }
    }
}

/// Sanitize a session name to safe filesystem characters [a-zA-Z0-9_-].
/// Returns None if the result would be empty.
pub fn sanitize_session_name(name: &str) -> Option<String> {
    let sanitized: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    let trimmed = sanitized.trim_matches('-');
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Check if a session name's instance_id refers to a still-running process.
pub fn session_is_locked(name: &str) -> bool {
    let session_file = session_dir(name).join("session.json");
    let Ok(contents) = fs::read_to_string(&session_file) else {
        return false;
    };
    let Ok(data) = serde_json::from_str::<SessionData>(&contents) else {
        return false;
    };
    let Some(ref pid_str) = data.instance_id else {
        return false;
    };
    let Ok(pid) = pid_str.parse::<u32>() else {
        return false;
    };
    // Check if the PID is our own process (not locked by another)
    if pid == std::process::id() {
        return false;
    }
    // Portable liveness check — the old `/proc/{pid}` probe silently never
    // engaged off Linux, so the lock did nothing on macOS/Windows (audit M7).
    process_is_alive(pid)
}

/// Whether a process with this PID is currently running, portably.
///
/// Replaces the Linux-only `/proc/{pid}` existence probe so session locking
/// engages on macOS and Windows too (audit M7). This is best-effort liveness,
/// not an advisory lock: a recycled PID can still read as alive, which only
/// yields a spurious "session open in another window", never data loss.
#[cfg(unix)]
fn process_is_alive(pid: u32) -> bool {
    // `kill(pid, 0)` delivers no signal — it only reports whether the process
    // exists and is signalable.
    unsafe extern "C" {
        fn kill(pid: i32, sig: i32) -> i32;
    }
    let Ok(pid) = i32::try_from(pid) else {
        return false;
    };
    // SAFETY: signal 0 delivers nothing and touches no memory; `pid` is a plain
    // integer. On a -1 return errno is set and read immediately below.
    if unsafe { kill(pid, 0) } == 0 {
        return true;
    }
    // EPERM (1): the process exists but is owned by another user — still alive.
    // ESRCH (3), or anything else: treat as gone.
    std::io::Error::last_os_error().raw_os_error() == Some(1)
}

#[cfg(windows)]
fn process_is_alive(pid: u32) -> bool {
    use windows::Win32::Foundation::{CloseHandle, FALSE};
    use windows::Win32::System::Threading::{
        GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    const STILL_ACTIVE: u32 = 259; // STATUS_PENDING — the process has not exited

    // SAFETY: each call takes a valid pid / the handle we just opened; every
    // failure path is handled and the handle is always closed.
    unsafe {
        let Ok(handle) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, FALSE, pid) else {
            return false; // cannot open → treat as not running
        };
        if handle.is_invalid() {
            return false;
        }
        let mut code: u32 = 0;
        let alive = GetExitCodeProcess(handle, &mut code).is_ok() && code == STILL_ACTIVE;
        let _ = CloseHandle(handle);
        alive
    }
}

#[cfg(not(any(unix, windows)))]
fn process_is_alive(_pid: u32) -> bool {
    // Unknown platform: fail open (report "not locked") rather than block the user.
    false
}

/// Save the current session to disk under the given session name.
pub fn save_session(
    tab_manager: &TabManager,
    mode: SessionRestore,
    last_open_directory: Option<&str>,
    session_name: &str,
) -> Result<(), AppError> {
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

    let dir = session_dir(session_name);
    fs::create_dir_all(&dir)?;
    let active_id = tab_manager.active_id();
    let active_index = active_id
        .and_then(|id| docs.iter().position(|d| d.id == id))
        .unwrap_or(0);

    // Build group sessions and a mapping from GroupId -> index
    let groups = tab_manager.groups();
    let group_sessions: Vec<GroupSession> = groups
        .iter()
        .map(|g| GroupSession {
            name: g.name.clone(),
            color: g.color.as_str().to_string(),
            collapsed: g.collapsed,
        })
        .collect();

    let mut doc_sessions = Vec::new();

    for doc in docs {
        let is_dirty = doc.is_dirty();
        let has_path = doc.file_path.is_some();

        // Find group index for this document
        let group_index = doc
            .group_id
            .and_then(|gid| groups.iter().position(|g| g.id == gid));

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

    let instance_id = std::process::id().to_string();
    merge_and_persist(
        doc_sessions,
        active_index,
        last_open_directory.map(|s| s.to_string()),
        group_sessions,
        mode,
        &dir,
        &instance_id,
    )
}

/// Merge `doc_sessions` with any session already on disk in `dir`, write the
/// result, and prune orphaned temp files. Split out of [`save_session`] so the
/// persistence logic can be exercised with an explicit `dir` and `instance_id`
/// (no global data-dir or `TabManager` dependency) in tests.
fn merge_and_persist(
    mut doc_sessions: Vec<DocumentSession>,
    active_index: usize,
    last_open_directory: Option<String>,
    group_sessions: Vec<GroupSession>,
    mode: SessionRestore,
    dir: &Path,
    instance_id: &str,
) -> Result<(), AppError> {
    let session_file = dir.join("session.json");

    // An instance with nothing to contribute must not clobber another instance's
    // saved session: writing our empty `documents` list would erase it and the
    // cleanup below would delete its temp files (audit M6). If the persisted
    // session belongs to a *different* instance, leave it untouched. A session we
    // already own (or no session at all) is still cleared normally below — that
    // is the legitimate "user closed all tabs" case.
    if doc_sessions.is_empty()
        && let Ok(existing_json) = fs::read_to_string(&session_file)
        && let Ok(existing) = serde_json::from_str::<SessionData>(&existing_json)
        && existing.instance_id.as_deref() != Some(instance_id)
    {
        return Ok(());
    }

    // Merge with existing session: keep docs from other instances that
    // aren't open in this one, so closing one instance doesn't erase another's tabs.
    // Skip merge when we have 0 docs — the user closed all tabs intentionally.
    if !doc_sessions.is_empty()
        && let Ok(existing_json) = fs::read_to_string(&session_file)
        && let Ok(existing) = serde_json::from_str::<SessionData>(&existing_json)
        && existing.instance_id.as_deref() != Some(instance_id)
    {
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
                    let temp_dup = doc
                        .temp_file
                        .as_ref()
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
        last_open_directory,
        groups: group_sessions,
        instance_id: Some(instance_id.to_string()),
    };

    let json = serde_json::to_string_pretty(&session_data)?;

    fs::write(&session_file, json)?;

    // Clean up orphaned temp files (not referenced in current session)
    cleanup_orphaned_temp_files(&session_data, dir);

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

/// Load session data from disk for the given session name.
pub fn load_session(mode: SessionRestore, session_name: &str) -> Option<SessionData> {
    if mode == SessionRestore::Off {
        return None;
    }

    let session_file = session_dir(session_name).join("session.json");
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
pub fn read_temp_file(temp_file: &str, session_name: &str) -> Option<String> {
    let path = session_dir(session_name).join(temp_file);
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
        assert_eq!(
            loaded.documents[0].file_path,
            Some("/tmp/test.txt".to_string())
        );
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
        let result = load_session(SessionRestore::Off, DEFAULT_SESSION_NAME);
        assert!(result.is_none());
    }

    #[test]
    fn test_session_dir_returns_path() {
        let dir = session_dir("default");
        assert!(
            dir.ends_with("ferrispad/session/default")
                || dir.ends_with("ferrispad\\session\\default")
        );
    }

    #[test]
    fn test_sanitize_session_name() {
        assert_eq!(
            sanitize_session_name("my-project"),
            Some("my-project".to_string())
        );
        assert_eq!(
            sanitize_session_name("My Project!"),
            Some("My-Project".to_string())
        );
        assert_eq!(sanitize_session_name(""), None);
        assert_eq!(sanitize_session_name("---"), None);
        assert_eq!(
            sanitize_session_name("hello world"),
            Some("hello-world".to_string())
        );
    }

    #[test]
    fn test_list_sessions_returns_vec() {
        // Just verify it doesn't panic; actual sessions depend on environment
        let _ = list_sessions();
    }

    // Regression (T0015, audit M6): an instance with nothing to contribute must
    // not overwrite another instance's persisted session — neither its
    // `documents` list nor its temp files.
    #[test]
    fn empty_instance_does_not_wipe_another_instances_session() {
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        let dir_path = dir.path();

        // Instance A ("111") persists a saved doc referencing a temp file, and
        // the temp file itself on disk.
        let a_temp = "aaaa.tmp";
        fs::write(dir_path.join(a_temp), b"instance A buffer").unwrap();
        let a_session = SessionData {
            version: CURRENT_SESSION_VERSION,
            active_index: 0,
            documents: vec![DocumentSession {
                file_path: Some("/home/user/a.rs".to_string()),
                display_name: "a.rs".to_string(),
                cursor_position: 10,
                temp_file: Some(a_temp.to_string()),
                was_dirty: true,
                group_index: None,
            }],
            last_open_directory: None,
            groups: vec![],
            instance_id: Some("111".to_string()),
        };
        fs::write(
            dir_path.join("session.json"),
            serde_json::to_string_pretty(&a_session).unwrap(),
        )
        .unwrap();

        // Instance B ("222") saves with nothing to contribute (only untitled
        // tabs → empty doc list in SavedFiles mode).
        merge_and_persist(
            Vec::new(),
            0,
            None,
            Vec::new(),
            SessionRestore::SavedFiles,
            dir_path,
            "222",
        )
        .unwrap();

        // A's session must survive untouched.
        let after: SessionData =
            serde_json::from_str(&fs::read_to_string(dir_path.join("session.json")).unwrap())
                .unwrap();
        assert_eq!(
            after.documents.len(),
            1,
            "empty instance B erased instance A's documents"
        );
        assert_eq!(
            after.documents[0].file_path.as_deref(),
            Some("/home/user/a.rs")
        );
        assert!(
            dir_path.join(a_temp).exists(),
            "empty instance B deleted instance A's temp file"
        );
    }

    // The guard must protect only FOREIGN sessions: an instance clearing its OWN
    // session (the user closed all tabs) still writes an empty document list.
    #[test]
    fn empty_instance_clears_its_own_session() {
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        let dir_path = dir.path();

        let own = SessionData {
            version: CURRENT_SESSION_VERSION,
            active_index: 0,
            documents: vec![DocumentSession {
                file_path: Some("/home/user/old.rs".to_string()),
                display_name: "old.rs".to_string(),
                cursor_position: 0,
                temp_file: None,
                was_dirty: false,
                group_index: None,
            }],
            last_open_directory: None,
            groups: vec![],
            instance_id: Some("222".to_string()),
        };
        fs::write(
            dir_path.join("session.json"),
            serde_json::to_string_pretty(&own).unwrap(),
        )
        .unwrap();

        merge_and_persist(
            Vec::new(),
            0,
            None,
            Vec::new(),
            SessionRestore::SavedFiles,
            dir_path,
            "222",
        )
        .unwrap();

        let after: SessionData =
            serde_json::from_str(&fs::read_to_string(dir_path.join("session.json")).unwrap())
                .unwrap();
        assert!(
            after.documents.is_empty(),
            "an instance must be able to clear its own session"
        );
        assert_eq!(after.instance_id.as_deref(), Some("222"));
    }

    // T0016 (audit M7): the portable liveness check recognises a live process
    // (here, ourselves) on every platform — the old `/proc` probe did so only on
    // Linux.
    #[test]
    fn process_is_alive_reports_our_own_process() {
        assert!(process_is_alive(std::process::id()));
    }

    // A reaped child's PID is no longer alive. Unix-only: a deterministic spawn +
    // reap; the Windows branch is exercised by the release build.
    #[cfg(unix)]
    #[test]
    fn process_is_alive_is_false_for_a_reaped_child() {
        let mut child = std::process::Command::new("true")
            .spawn()
            .expect("spawn `true`");
        let pid = child.id();
        child.wait().expect("reap child");
        assert!(
            !process_is_alive(pid),
            "a reaped child's pid should not read as alive"
        );
    }
}
