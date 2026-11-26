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
    /// The instance that wrote this entry as its own (T0101). Missing in
    /// files from before T0101: then the file's `instance_id` stands in.
    #[serde(default)]
    pub owner: Option<String>,
}

/// A session document's identity (T0099/T0101): the file path of a file,
/// the temp file of an unsaved untitled document.
pub fn session_identity(doc: &DocumentSession) -> Option<String> {
    doc.file_path.clone().or_else(|| doc.temp_file.clone())
}

/// The instance a session entry belongs to: its `owner`, or for entries
/// from before T0101 the instance that wrote the file.
pub fn entry_owner(doc: &DocumentSession, file: &SessionData) -> String {
    doc.owner
        .clone()
        .or_else(|| file.instance_id.clone())
        .unwrap_or_default()
}

/// This process's instance id, as written to `owner` and `instance_id`
/// (T0101): `<pid>-<nonce>`, the nonce random per run. A bare pid is not
/// enough: a new process that gets a crashed instance's pid would take that
/// instance's entries for its own and drop them.
pub fn instance_id() -> &'static str {
    static ID: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    ID.get_or_init(|| {
        use std::hash::BuildHasher;
        // `RandomState` is seeded from the OS's random source; the time and
        // the pid only add to it.
        let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
        std::process::id().hash(&mut hasher);
        std::time::SystemTime::now().hash(&mut hasher);
        format!("{}-{:016x}", std::process::id(), hasher.finish())
    })
}

/// What an instance id names, seen from this process (T0101).
#[derive(Debug, PartialEq, Eq)]
enum InstanceState {
    /// This process.
    Us,
    /// An instance that is no longer running.
    Gone,
    /// Another instance that is still running.
    Alive,
    /// Not an instance id (empty, or not `<pid>[-<nonce>]`).
    Unknown,
}

fn instance_state(id: &str) -> InstanceState {
    if id == instance_id() {
        return InstanceState::Us;
    }
    match instance_pid(id) {
        None => InstanceState::Unknown,
        // Another run with our pid: it is gone, we hold its pid now.
        Some(pid) if pid == std::process::id() => InstanceState::Gone,
        Some(pid) if process_is_alive(pid) => InstanceState::Alive,
        Some(_) => InstanceState::Gone,
    }
}

/// The process id an instance id names: its `<pid>` prefix (a bare pid in
/// files from before T0101).
fn instance_pid(id: &str) -> Option<u32> {
    id.split('-').next()?.parse().ok()
}

/// Whether a restore may take over a restored entry's temp file (T0101):
/// only from an instance that is gone. If another instance still runs (or
/// the owner cannot be told), the restored copy gets a temp file of its
/// own, so no temp file ever has two writers; a duplicate tab on a later
/// restore is acceptable, a lost text is not.
pub fn reuse_restored_temp(owner: &str) -> bool {
    matches!(
        instance_state(owner),
        InstanceState::Us | InstanceState::Gone
    )
}

/// Who is saving (T0101): the instance and the copies it closed.
struct Saver<'a> {
    instance_id: &'a str,
    closed: &'a HashSet<(String, String)>,
}

/// What one instance has had in the session, so its saves can tell "another
/// instance has this open" (keep) from "this instance closed it" (drop)
/// (T0101). In memory only, for the life of the process.
#[derive(Debug, Default)]
pub struct Ownership {
    /// Identities this instance wrote as its own in its last save.
    owned: HashSet<String>,
    /// Copies (identity, owner) it took over at restore from an instance
    /// that is gone.
    adopted: HashSet<(String, String)>,
    /// Copies (identity, owner) it had and closed (or renamed by Save As).
    closed: HashSet<(String, String)>,
}

impl Ownership {
    /// Record a copy taken over at restore.
    pub fn adopt(&mut self, identity: String, owner: String) {
        self.adopted.insert((identity, owner));
    }

    /// This instance now has exactly `current` (identities) open, and is
    /// `us`: whatever it had and no longer has was closed by it; whatever
    /// it has again is no longer closed.
    pub fn update(&mut self, current: &HashSet<String>, us: &str) {
        for id in self.owned.difference(current) {
            self.closed.insert((id.clone(), us.to_string()));
        }
        let (still, gone): (HashSet<_>, HashSet<_>) = std::mem::take(&mut self.adopted)
            .into_iter()
            .partition(|(id, _)| current.contains(id));
        self.closed.extend(gone);
        self.adopted = still;
        for id in current {
            self.closed.remove(&(id.clone(), us.to_string()));
        }
        self.owned = current.clone();
    }

    /// The copies this instance closed.
    pub fn closed(&self) -> &HashSet<(String, String)> {
        &self.closed
    }
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
    // Portable liveness check — the old `/proc/{pid}` probe silently never
    // engaged off Linux, so the lock did nothing on macOS/Windows (audit M7).
    data.instance_id
        .as_deref()
        .is_some_and(|id| instance_state(id) == InstanceState::Alive)
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
    use windows::Win32::Foundation::{CloseHandle, ERROR_INVALID_PARAMETER, FALSE};
    use windows::Win32::System::Threading::{
        GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    const STILL_ACTIVE: u32 = 259; // STATUS_PENDING — the process has not exited

    // SAFETY: each call takes a valid pid / the handle we just opened; every
    // failure path is handled and the handle is always closed.
    unsafe {
        let handle = match OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, FALSE, pid) {
            Ok(handle) => handle,
            // Only "no such process" (ERROR_INVALID_PARAMETER) means gone.
            // Anything else, e.g. access denied for another user's or an
            // elevated process, reads as alive: a caller then keeps that
            // instance's documents or forks its temp file (T0101).
            Err(e) => {
                return e.code() != windows::core::HRESULT::from_win32(ERROR_INVALID_PARAMETER.0);
            }
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
    ownership: &mut Ownership,
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

    let instance_id = instance_id().to_string();
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
                        owner: Some(instance_id.clone()),
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
                    let hash_key = doc.file_path.as_deref().unwrap_or(&doc.display_name);
                    let filename = temp_file_name(
                        hash_key,
                        doc.id.0,
                        &instance_id,
                        doc.restored_temp_file.as_deref(),
                    );
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
                    owner: Some(instance_id.clone()),
                });
            }
            SessionRestore::Off => unreachable!(),
        }
    }

    let current: HashSet<String> = doc_sessions.iter().filter_map(session_identity).collect();
    ownership.update(&current, &instance_id);
    merge_and_persist(
        doc_sessions,
        active_index,
        last_open_directory.map(|s| s.to_string()),
        group_sessions,
        &dir,
        Saver {
            instance_id: &instance_id,
            closed: ownership.closed(),
        },
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
    dir: &Path,
    saver: Saver,
) -> Result<(), AppError> {
    let Saver {
        instance_id,
        closed,
    } = saver;
    let session_file = dir.join("session.json");

    // Merge with the session on disk (T0101): our current documents replace
    // every entry we own; another instance's entry stays unless we closed
    // that copy. No liveness check: an entry nobody closed is kept (a
    // crashed instance's unsaved text survives), and our own earlier file is
    // merged too (it may hold another running instance's documents). This
    // also keeps an instance with nothing open from clobbering a foreign
    // session (audit M6): it writes the foreign entries back.
    if let Ok(existing_json) = fs::read_to_string(&session_file)
        && let Ok(existing) = serde_json::from_str::<SessionData>(&existing_json)
    {
        let our_ids: HashSet<String> = doc_sessions.iter().filter_map(session_identity).collect();
        let our_temp_files: HashSet<String> = doc_sessions
            .iter()
            .filter_map(|d| d.temp_file.clone())
            .collect();
        let file_owner = existing.instance_id.clone();
        for mut doc in existing.documents {
            let owner = doc
                .owner
                .clone()
                .or_else(|| file_owner.clone())
                .unwrap_or_default();
            if owner == instance_id {
                continue; // ours: replaced by our current documents
            }
            let Some(id) = session_identity(&doc) else {
                continue;
            };
            if closed.contains(&(id.clone(), owner.clone())) {
                continue; // we closed this copy
            }
            let keep = match (&doc.file_path, &doc.temp_file) {
                // Unsaved text of another instance: keep it, in any mode
                // (we never drop what we did not close; a SavedFiles restore
                // skips it anyway), even for a file we also have open (its
                // edits are its own), unless it is the very temp file we
                // write.
                (_, Some(temp)) => !our_temp_files.contains(temp),
                // A clean file: once in the session is enough.
                (Some(_), None) => !our_ids.contains(&id),
                (None, None) => false,
            };
            if keep {
                doc.owner = Some(owner);
                doc_sessions.push(doc);
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
    read_temp_file_in(&session_dir(session_name), temp_file)
}

/// Read the temp file `temp_file` of the session directory `dir`. The name
/// comes from session.json (untrusted): like the write side
/// ([`temp_file_name`]), only a plain `*.tmp` name is read; anything else
/// is skipped with a log line, and the restore goes on without it (T0099).
fn read_temp_file_in(dir: &Path, temp_file: &str) -> Option<String> {
    if !is_plain_temp_name(temp_file) {
        eprintln!("[session] Skipping temp file {temp_file:?}: not a plain .tmp name");
        return None;
    }
    fs::read_to_string(dir.join(temp_file)).ok()
}

/// The temp file holding a document's unsaved text in the session (T0099).
/// The temp file IS the document's identity in the session:
/// - a document restored from a temp file keeps writing to that file, so a
///   restart (new process, new document id) does not create a second copy;
/// - a new document's name hashes the instance id along with `hash_key`
///   (its file path, or the display name of an untitled document) and its
///   per-process `doc_id`: ids start at 1 in every instance, so without
///   the instance two instances' "Untitled" #2 would share one file.
///
/// The restored name comes from `session.json`, so it is only reused when
/// it is a plain `*.tmp` file name (no path); anything else gets a fresh
/// name.
pub(crate) fn temp_file_name(
    hash_key: &str,
    doc_id: u64,
    instance_id: &str,
    restored_temp_file: Option<&str>,
) -> String {
    if let Some(name) = restored_temp_file.filter(|n| is_plain_temp_name(n)) {
        return name.to_string();
    }
    let mut hasher = DefaultHasher::new();
    hash_key.hash(&mut hasher);
    doc_id.hash(&mut hasher);
    instance_id.hash(&mut hasher);
    format!("{:016x}.tmp", hasher.finish())
}

/// A bare `*.tmp` file name: no directory part, no `..`.
fn is_plain_temp_name(name: &str) -> bool {
    name.ends_with(".tmp")
        && !name.starts_with('.')
        && Path::new(name).file_name().and_then(|f| f.to_str()) == Some(name)
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
                owner: None,
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
            owner: None,
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

    // --- T0101: a doc leaves the session only when an instance closed it ---

    /// An entry `id` owned by `owner`: untitled with temp `temp`, or a file
    /// at `path` (unsaved edits in `temp` if given).
    fn entry(path: Option<&str>, temp: Option<&str>, owner: &str) -> DocumentSession {
        DocumentSession {
            file_path: path.map(str::to_string),
            display_name: "x".to_string(),
            cursor_position: 0,
            temp_file: temp.map(str::to_string),
            was_dirty: temp.is_some(),
            group_index: None,
            owner: Some(owner.to_string()),
        }
    }

    fn write_session(dir: &Path, writer: &str, docs: Vec<DocumentSession>) {
        let data = SessionData {
            version: CURRENT_SESSION_VERSION,
            active_index: 0,
            documents: docs,
            last_open_directory: None,
            groups: vec![],
            instance_id: Some(writer.to_string()),
        };
        fs::write(
            dir.join("session.json"),
            serde_json::to_string_pretty(&data).unwrap(),
        )
        .unwrap();
    }

    fn save(dir: &Path, us: &str, ours: Vec<DocumentSession>, closed: &[(&str, &str)]) {
        let closed: HashSet<(String, String)> = closed
            .iter()
            .map(|(id, o)| (id.to_string(), o.to_string()))
            .collect();
        merge_and_persist(
            ours,
            0,
            None,
            Vec::new(),
            dir,
            Saver {
                instance_id: us,
                closed: &closed,
            },
        )
        .unwrap();
    }

    fn ids(dir: &Path) -> Vec<String> {
        let mut v: Vec<String> = persisted(dir)
            .documents
            .iter()
            .filter_map(session_identity)
            .collect();
        v.sort();
        v
    }

    /// (b) A's own file also lists B's unsaved b1 (merged earlier). A's next
    /// save must keep it and its temp file: B is running and may never save
    /// again before a crash.
    #[test]
    fn session_closed_live_owner_entry_survives_own_save() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        fs::write(d.join("ta.tmp"), "a1").unwrap();
        fs::write(d.join("tb.tmp"), "b1").unwrap();
        write_session(
            d,
            "A",
            vec![
                entry(None, Some("ta.tmp"), "A"),
                entry(None, Some("tb.tmp"), "B"),
            ],
        );
        save(d, "A", vec![entry(None, Some("ta.tmp"), "A")], &[]);
        assert_eq!(ids(d), ["ta.tmp", "tb.tmp"], "B's document was dropped");
        assert_eq!(
            fs::read_to_string(d.join("tb.tmp")).ok().as_deref(),
            Some("b1")
        );
    }

    /// (a) B closed b1 and saves with nothing open: b1 leaves the session,
    /// A's document stays.
    #[test]
    fn session_closed_in_other_instance_stays_closed() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        fs::write(d.join("ta.tmp"), "a1").unwrap();
        fs::write(d.join("tb.tmp"), "b1").unwrap();
        write_session(
            d,
            "A",
            vec![
                entry(None, Some("ta.tmp"), "A"),
                entry(None, Some("tb.tmp"), "B"),
            ],
        );
        save(d, "B", vec![], &[("tb.tmp", "B")]);
        assert_eq!(ids(d), ["ta.tmp"], "the closed b1 came back");
        assert!(!d.join("tb.tmp").exists());
        // A saves afterwards: still out.
        save(d, "A", vec![entry(None, Some("ta.tmp"), "A")], &[]);
        assert_eq!(ids(d), ["ta.tmp"]);
    }

    /// B closed file X; A has X open (its own copy): A's copy survives B's
    /// save.
    #[test]
    fn session_closed_reopen_in_other_instance_survives() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        write_session(d, "A", vec![entry(Some("/w/x.rs"), None, "A")]);
        save(d, "B", vec![], &[("/w/x.rs", "B")]);
        assert_eq!(ids(d), ["/w/x.rs"]);
    }

    /// A crashed instance's unsaved document that nobody closed is kept.
    #[test]
    fn session_closed_crashed_instance_docs_are_kept() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        fs::write(d.join("tc.tmp"), "unsaved").unwrap();
        write_session(d, "100", vec![entry(None, Some("tc.tmp"), "100")]);
        save(d, "200", vec![entry(Some("/w/z.rs"), None, "200")], &[]);
        assert_eq!(ids(d), ["/w/z.rs", "tc.tmp"]);
        assert!(d.join("tc.tmp").exists());
    }

    /// The same file open with unsaved edits in two instances: both temp
    /// files survive (a duplicate tab on restore beats a loss).
    #[test]
    fn session_closed_shared_dirty_file_keeps_both_edits() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        fs::write(d.join("ta.tmp"), "A's edits").unwrap();
        fs::write(d.join("tb.tmp"), "B's edits").unwrap();
        write_session(d, "A", vec![entry(Some("/w/p.rs"), Some("ta.tmp"), "A")]);
        save(
            d,
            "B",
            vec![entry(Some("/w/p.rs"), Some("tb.tmp"), "B")],
            &[],
        );
        let temps: Vec<_> = persisted(d)
            .documents
            .iter()
            .filter_map(|x| x.temp_file.clone())
            .collect();
        assert!(
            temps.contains(&"ta.tmp".to_string()),
            "A's unsaved edits dropped: {temps:?}"
        );
        assert_eq!(
            fs::read_to_string(d.join("ta.tmp")).ok().as_deref(),
            Some("A's edits")
        );
    }

    /// An entry from before T0101 (no owner) belongs to the file's writer.
    #[test]
    fn session_closed_old_entries_without_owner() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        fs::write(d.join("tx.tmp"), "x").unwrap();
        let mut old = entry(None, Some("tx.tmp"), "ignored");
        old.owner = None;
        write_session(d, "100", vec![old]);
        let json = fs::read_to_string(d.join("session.json")).unwrap();
        let json = json
            .replace("      \"owner\": null\n", "")
            .replace(",\n    }", "\n    }");
        fs::write(d.join("session.json"), json).unwrap();
        // Restored (adopted from 100) and closed: dropped.
        save(d, "200", vec![], &[("tx.tmp", "100")]);
        assert_eq!(ids(d), Vec::<String>::new());
    }

    /// Close and reopen in the same instance; a restored copy closed later.
    #[test]
    fn session_closed_ownership_close_and_reopen() {
        let set = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<HashSet<_>>();
        let mut o = Ownership::default();
        o.update(&set(&["x"]), "A");
        o.update(&set(&[]), "A");
        assert!(o.closed().contains(&("x".into(), "A".into())));
        o.update(&set(&["x"]), "A");
        assert!(!o.closed().contains(&("x".into(), "A".into())), "reopened");
        o.adopt("y".into(), "100".into());
        o.update(&set(&["x", "y"]), "A");
        assert!(!o.closed().contains(&("y".into(), "100".into())));
        o.update(&set(&["x"]), "A");
        assert!(o.closed().contains(&("y".into(), "100".into())));
        assert!(o.closed().contains(&("y".into(), "A".into())));
    }

    /// Adoption at restore: take over a temp file only from an instance that
    /// is gone. Another live instance (or one we cannot tell) keeps writing
    /// it, so the restored copy gets its own temp file.
    #[test]
    fn session_closed_adoption_forks_when_owner_alive() {
        let mut child = std::process::Command::new("true").spawn().unwrap();
        let dead = child.id().to_string();
        child.wait().unwrap();
        assert!(reuse_restored_temp(&dead), "a gone instance: take over");
        assert!(reuse_restored_temp(instance_id()), "ours: keep writing it");
        assert!(!reuse_restored_temp("1"), "a live instance: fork");
        assert!(!reuse_restored_temp(""), "unknown owner: fork");
        assert!(!reuse_restored_temp("not-a-pid"), "unknown owner: fork");
    }

    /// PID reuse: an instance that crashed with the pid we now run under
    /// wrote its unsaved `old.tmp` as its own (a bare pid, or the pid with
    /// another run's nonce). Our save must not take it for ours and drop it.
    #[test]
    fn session_closed_pid_reuse_keeps_crashed_entries() {
        let pid = std::process::id();
        for crashed in [pid.to_string(), format!("{pid}-0123456789abcdef")] {
            let dir = tempfile::tempdir().unwrap();
            let d = dir.path();
            fs::write(d.join("old.tmp"), "unsaved").unwrap();
            write_session(d, &crashed, vec![entry(None, Some("old.tmp"), &crashed)]);
            save(d, instance_id(), vec![], &[]);
            assert_eq!(
                ids(d),
                ["old.tmp"],
                "{crashed}: the crashed instance's doc was dropped"
            );
            assert_eq!(
                fs::read_to_string(d.join("old.tmp")).ok().as_deref(),
                Some("unsaved"),
                "{crashed}: its temp file was deleted"
            );
        }
    }

    /// An instance id that names our pid but is not ours belongs to an
    /// instance that is gone (we hold its pid): its temp file can be taken
    /// over, and it does not lock the session.
    #[test]
    fn session_closed_same_pid_other_run_is_gone() {
        let pid = std::process::id();
        let other_run = format!("{pid}-0123456789abcdef");
        assert_ne!(instance_id(), other_run);
        assert!(reuse_restored_temp(&other_run));
        assert_eq!(instance_state(&other_run), InstanceState::Gone);
        assert_eq!(instance_state(&pid.to_string()), InstanceState::Gone);
        assert_eq!(instance_state(instance_id()), InstanceState::Us);
    }

    // --- T0099: the identity of an unsaved document is its temp file ---

    fn untitled(temp_file: &str) -> DocumentSession {
        DocumentSession {
            file_path: None,
            display_name: "Untitled".to_string(),
            cursor_position: 0,
            temp_file: Some(temp_file.to_string()),
            was_dirty: true,
            group_index: None,
            owner: None,
        }
    }

    fn persisted(dir: &Path) -> SessionData {
        serde_json::from_str(&fs::read_to_string(dir.join("session.json")).unwrap()).unwrap()
    }

    /// Two instances, each with an unsaved "Untitled" of different text:
    /// after the second one saves, both texts are still in the session.
    #[test]
    fn session_identity_foreign_untitled_survives_merge() {
        let dir = tempfile::tempdir().unwrap();
        let dir_path = dir.path();
        // Instance 111 persisted its untitled document first.
        fs::write(dir_path.join("aaaa.tmp"), "from instance 111").unwrap();
        let first = SessionData {
            version: CURRENT_SESSION_VERSION,
            active_index: 0,
            documents: vec![untitled("aaaa.tmp")],
            last_open_directory: None,
            groups: vec![],
            instance_id: Some("111".to_string()),
        };
        fs::write(
            dir_path.join("session.json"),
            serde_json::to_string_pretty(&first).unwrap(),
        )
        .unwrap();

        // Instance 222 saves its own, different, untitled document.
        fs::write(dir_path.join("bbbb.tmp"), "from instance 222").unwrap();
        merge_and_persist(
            vec![untitled("bbbb.tmp")],
            0,
            None,
            Vec::new(),
            dir_path,
            Saver {
                instance_id: "222",
                closed: &HashSet::new(),
            },
        )
        .unwrap();

        let temps: Vec<_> = persisted(dir_path)
            .documents
            .iter()
            .filter_map(|d| d.temp_file.clone())
            .collect();
        assert!(
            temps.contains(&"aaaa.tmp".to_string()),
            "instance 111's unsaved text was dropped from the session: {temps:?}"
        );
        assert!(temps.contains(&"bbbb.tmp".to_string()));
        assert_eq!(
            fs::read_to_string(dir_path.join("aaaa.tmp"))
                .ok()
                .as_deref(),
            Some("from instance 111"),
            "instance 111's temp file was deleted"
        );
    }

    /// Document ids start at 1 in every process: the same name and id in two
    /// instances must not share a temp file.
    #[test]
    fn session_identity_temp_names_differ_across_instances() {
        let a = temp_file_name("Untitled", 2, "111", None);
        let b = temp_file_name("Untitled", 2, "222", None);
        assert_ne!(a, b);
        // Stable within one instance (auto-saves overwrite the same file).
        assert_eq!(a, temp_file_name("Untitled", 2, "111", None));
        assert!(a.ends_with(".tmp"));
    }

    /// A document restored from a temp file writes back to it (whatever this
    /// process's id for it), so the merge recognises it and does not keep
    /// a second copy.
    #[test]
    fn session_identity_restored_doc_keeps_its_temp_file() {
        assert_eq!(
            temp_file_name("Untitled", 7, "333", Some("xyz.tmp")),
            "xyz.tmp"
        );

        let dir = tempfile::tempdir().unwrap();
        let dir_path = dir.path();
        fs::write(dir_path.join("xyz.tmp"), "restored").unwrap();
        let earlier = SessionData {
            version: CURRENT_SESSION_VERSION,
            active_index: 0,
            documents: vec![untitled("xyz.tmp")],
            last_open_directory: None,
            groups: vec![],
            instance_id: Some("111".to_string()),
        };
        fs::write(
            dir_path.join("session.json"),
            serde_json::to_string_pretty(&earlier).unwrap(),
        )
        .unwrap();
        let name = temp_file_name("Untitled", 7, "333", Some("xyz.tmp"));
        merge_and_persist(
            vec![untitled(&name)],
            0,
            None,
            Vec::new(),
            dir_path,
            Saver {
                instance_id: "333",
                closed: &HashSet::new(),
            },
        )
        .unwrap();
        assert_eq!(persisted(dir_path).documents.len(), 1, "no duplicate");
    }

    /// A restored name from session.json is only reused when it is a plain
    /// `*.tmp` name; a path is never written to.
    #[test]
    fn session_identity_restored_name_must_be_plain() {
        for bad in [
            "../evil.tmp",
            "/tmp/x.tmp",
            "sub/x.tmp",
            "..",
            ".tmp",
            "x.txt",
        ] {
            let name = temp_file_name("Untitled", 1, "1", Some(bad));
            assert_ne!(name, bad);
            assert!(is_plain_temp_name(&name), "{bad} -> {name}");
        }
    }

    /// A temp file name from session.json is read only when it is a plain
    /// `*.tmp` name inside the session directory: an absolute path or `..`
    /// must never pull another file into a tab on Full restore.
    #[test]
    fn session_identity_read_only_plain_temp_names() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("default");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("good.tmp"), "unsaved").unwrap();
        fs::write(root.path().join("x.tmp"), "outside the session dir").unwrap();
        let outside = root.path().join("x.tmp");
        let outside = outside.to_str().unwrap();

        assert_eq!(
            read_temp_file_in(&dir, "good.tmp").as_deref(),
            Some("unsaved")
        );
        for bad in ["../x.tmp", outside, "/etc/hostname"] {
            assert_eq!(read_temp_file_in(&dir, bad), None, "{bad} must not be read");
        }
    }

    /// Several simulated runs of one session: each run restores the unsaved
    /// documents (reusing their temp files), maybe adds or closes one, and
    /// saves. The temp dir never holds more files than the session lists.
    #[test]
    fn session_identity_no_temp_file_leak_over_runs() {
        let dir = tempfile::tempdir().unwrap();
        let dir_path = dir.path();
        // One "run": the documents the instance holds -> what it saves.
        let save = |instance: &str, docs: &[(&str, u64, Option<String>, &str)]| {
            let sessions: Vec<DocumentSession> = docs
                .iter()
                .map(|(name, id, restored, text)| {
                    let file = temp_file_name(name, *id, instance, restored.as_deref());
                    fs::write(dir_path.join(&file), text).unwrap();
                    DocumentSession {
                        display_name: name.to_string(),
                        owner: None,
                        ..untitled(&file)
                    }
                })
                .collect();
            merge_and_persist(
                sessions,
                0,
                None,
                Vec::new(),
                dir_path,
                Saver {
                    instance_id: instance,
                    closed: &HashSet::new(),
                },
            )
            .unwrap();
        };
        let tmp_count = || {
            fs::read_dir(dir_path)
                .unwrap()
                .filter(|e| {
                    e.as_ref()
                        .unwrap()
                        .file_name()
                        .to_string_lossy()
                        .ends_with(".tmp")
                })
                .count()
        };
        let restored = || -> Vec<String> {
            persisted(dir_path)
                .documents
                .iter()
                .filter_map(|d| d.temp_file.clone())
                .collect()
        };

        // Run 1 (pid 100): two new unsaved documents.
        save(
            "100",
            &[("Untitled", 1, None, "a"), ("Untitled 2", 2, None, "b")],
        );
        assert_eq!(tmp_count(), 2);
        // Runs 2-4 (new pids, new ids): restore both and save again.
        for (run, pid) in ["200", "300", "400"].iter().enumerate() {
            let r = restored();
            save(
                pid,
                &[
                    ("Untitled", 10 + run as u64, Some(r[0].clone()), "a"),
                    ("Untitled 2", 20 + run as u64, Some(r[1].clone()), "b"),
                ],
            );
            assert_eq!(tmp_count(), 2, "run {pid}: no new files for restored docs");
            assert_eq!(
                persisted(dir_path).documents.len(),
                2,
                "run {pid}: no duplicates"
            );
        }
        // Run 5: one more document; run 6: the user closed one.
        let r = restored();
        save(
            "500",
            &[
                ("Untitled", 1, Some(r[0].clone()), "a"),
                ("Untitled 2", 2, Some(r[1].clone()), "b"),
                ("Untitled 3", 3, None, "c"),
            ],
        );
        assert_eq!(tmp_count(), 3);
        // Run 6: restores all three; its first (auto-)save makes the session
        // its own; then the user closes one and it saves again.
        let r = restored();
        let all = [
            ("Untitled", 1, Some(r[0].clone()), "a"),
            ("Untitled 2", 2, Some(r[1].clone()), "b"),
            ("Untitled 3", 3, Some(r[2].clone()), "c"),
        ];
        save("600", &all);
        assert_eq!(tmp_count(), 3);
        save("600", &[all[0].clone(), all[2].clone()]);
        assert_eq!(tmp_count(), 2, "the closed document's temp file is removed");
        assert_eq!(persisted(dir_path).documents.len(), 2);
    }

    /// A session written by 0.9.9 / 0.9.10-rc.2: old-style temp names
    /// (hash of name + id only), no instance in them. It restores as before,
    /// and the first save after the upgrade keeps exactly those documents
    /// and files: no loss, no duplicate.
    #[test]
    fn session_identity_old_session_upgrades_cleanly() {
        let dir = tempfile::tempdir().unwrap();
        let dir_path = dir.path();
        let old_json = r#"{
  "version": 1,
  "active_index": 1,
  "documents": [
    {"file_path": null, "display_name": "Untitled 2", "cursor_position": 3,
     "temp_file": "6cbb5b99cd7f50b2.tmp", "was_dirty": true, "group_index": null},
    {"file_path": null, "display_name": "Untitled 3", "cursor_position": 0,
     "temp_file": "620ea35db537a593.tmp", "was_dirty": true, "group_index": null}
  ],
  "last_open_directory": null,
  "groups": [],
  "instance_id": "4242"
}"#;
        fs::write(dir_path.join("session.json"), old_json).unwrap();
        fs::write(dir_path.join("6cbb5b99cd7f50b2.tmp"), "first unsaved").unwrap();
        fs::write(dir_path.join("620ea35db537a593.tmp"), "second unsaved").unwrap();

        // Restore: the old format parses, and each temp file is readable.
        let old: SessionData = persisted(dir_path);
        let texts: Vec<String> = old
            .documents
            .iter()
            .map(|d| fs::read_to_string(dir_path.join(d.temp_file.as_ref().unwrap())).unwrap())
            .collect();
        assert_eq!(texts, ["first unsaved", "second unsaved"]);

        // First save by the new version (new pid, new ids, T0091 names):
        // each restored document writes back to its old temp file.
        let sessions: Vec<DocumentSession> = old
            .documents
            .iter()
            .enumerate()
            .map(|(i, d)| {
                let name = if i == 0 { "Untitled" } else { "Untitled 2" };
                let file = temp_file_name(name, i as u64 + 1, "777", d.temp_file.as_deref());
                fs::write(dir_path.join(&file), &texts[i]).unwrap();
                DocumentSession {
                    display_name: name.to_string(),
                    owner: None,
                    ..untitled(&file)
                }
            })
            .collect();
        merge_and_persist(
            sessions,
            1,
            None,
            Vec::new(),
            dir_path,
            Saver {
                instance_id: "777",
                closed: &HashSet::new(),
            },
        )
        .unwrap();
        let after = persisted(dir_path);
        let files: Vec<_> = after
            .documents
            .iter()
            .filter_map(|d| d.temp_file.clone())
            .collect();
        assert_eq!(files, ["6cbb5b99cd7f50b2.tmp", "620ea35db537a593.tmp"]);
        assert_eq!(
            fs::read_to_string(dir_path.join("6cbb5b99cd7f50b2.tmp")).unwrap(),
            "first unsaved"
        );
        assert_eq!(
            fs::read_to_string(dir_path.join("620ea35db537a593.tmp")).unwrap(),
            "second unsaved"
        );
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
                owner: None,
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
            dir_path,
            Saver {
                instance_id: "222",
                closed: &HashSet::new(),
            },
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
                owner: None,
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
            dir_path,
            Saver {
                instance_id: "222",
                closed: &HashSet::new(),
            },
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
