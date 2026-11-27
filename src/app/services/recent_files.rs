//! The recently opened files list (T0116): most recent first, at most
//! [`MAX_ENTRIES`] named files, kept across restarts.
//!
//! The list is state, not a setting: it lives next to the sessions in the
//! data dir (`<data_dir>/ferrispad/recent_files.json`), never in
//! `settings.json`. It changes only on user actions (open, Save As, Clear
//! Recent); nothing watches the files or checks in the background whether
//! they still exist.
//!
//! The file is untrusted input: a missing, empty, malformed or oversized
//! file reads as an empty list, every entry is checked, and the list is
//! capped, so a bad file can never break startup.

use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// How many files the list keeps.
pub const MAX_ENTRIES: usize = 10;
/// How many of them the start page's Recent Files card shows.
pub const START_PAGE_ENTRIES: usize = 4;
/// A larger file is not read at all (the real one is under 5 KB).
const MAX_FILE_BYTES: u64 = 64 * 1024;
/// A longer entry is dropped.
const MAX_PATH_BYTES: usize = 4096;
/// A menu label longer than this is elided in the middle.
const MAX_LABEL_CHARS: usize = 60;

#[derive(Serialize, Deserialize)]
struct RecentFilesData {
    version: u32,
    files: Vec<String>,
}

/// Puts `path` at the front of `list`, removing an earlier copy, and keeps
/// at most [`MAX_ENTRIES`].
pub fn push(list: &mut Vec<String>, path: &str) {
    list.retain(|p| p != path);
    list.insert(0, path.to_string());
    list.truncate(MAX_ENTRIES);
}

/// Removes `path` from `list`; true if it was there.
pub fn remove(list: &mut Vec<String>, path: &str) -> bool {
    let before = list.len();
    list.retain(|p| p != path);
    list.len() != before
}

/// `<data_dir>/ferrispad/recent_files.json`.
pub fn store_path() -> PathBuf {
    let mut path = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
    path.push("ferrispad");
    path.push("recent_files.json");
    path
}

/// The saved list, or an empty one when there is none or it is unusable.
pub fn load() -> Vec<String> {
    load_from(&store_path())
}

/// [`load`] from a given file.
pub fn load_from(path: &Path) -> Vec<String> {
    match fs::metadata(path) {
        Ok(meta) if meta.len() > MAX_FILE_BYTES => {
            eprintln!(
                "[recent-files] {} is {} bytes, over the {} byte limit: ignored",
                path.display(),
                meta.len(),
                MAX_FILE_BYTES
            );
            return Vec::new();
        }
        Ok(_) => {}
        Err(_) => return Vec::new(),
    }
    let Ok(text) = fs::read_to_string(path) else {
        return Vec::new();
    };
    match serde_json::from_str::<RecentFilesData>(&text) {
        Ok(data) => sanitize(data.files),
        Err(e) => {
            eprintln!("[recent-files] {} is not valid: {e}", path.display());
            Vec::new()
        }
    }
}

/// Keeps the usable entries: absolute paths, not empty, no NUL, at most
/// [`MAX_PATH_BYTES`], first copy of each, at most [`MAX_ENTRIES`].
fn sanitize(files: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    files
        .into_iter()
        .filter(|p| {
            !p.is_empty()
                && p.len() <= MAX_PATH_BYTES
                && !p.contains('\0')
                && Path::new(p).is_absolute()
        })
        .filter(|p| seen.insert(p.clone()))
        .take(MAX_ENTRIES)
        .collect()
}

/// Writes the list to [`store_path`].
pub fn save(list: &[String]) -> io::Result<()> {
    save_to(&store_path(), list)
}

/// [`save`] to a given file, through a temporary file and a rename, so a
/// crash mid-write never leaves a half-written list.
pub fn save_to(path: &Path, list: &[String]) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let data = RecentFilesData {
        version: 1,
        files: list.iter().take(MAX_ENTRIES).cloned().collect(),
    };
    let json = serde_json::to_string_pretty(&data).map_err(io::Error::other)?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, json)?;
    fs::rename(&tmp, path)
}

/// The file name of `path`, or the whole path when it has none.
fn file_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string())
}

/// The name of the folder holding `path`.
fn parent_name(path: &str) -> Option<String> {
    Path::new(path)
        .parent()
        .and_then(|p| p.file_name())
        .map(|f| f.to_string_lossy().to_string())
}

/// `s` cut to [`MAX_LABEL_CHARS`] characters, with "…" in the middle.
fn elide(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= MAX_LABEL_CHARS {
        return s.to_string();
    }
    let keep = MAX_LABEL_CHARS - 1;
    let head = keep / 2;
    let tail = keep - head;
    let mut out: String = chars[..head].iter().collect();
    out.push('…');
    out.extend(&chars[chars.len() - tail..]);
    out
}

/// One label per entry: the file name, and " — <folder>" where two entries
/// share a name. Long labels are elided in the middle. Labels are unique (an
/// FLTK menu replaces an item added twice under the same label), so a label
/// that still repeats gets " (2)", " (3)", …. The labels are plain text: see
/// [`escape_menu_label`] before handing one to an FLTK menu.
pub fn labels(list: &[String]) -> Vec<String> {
    let names: Vec<String> = list.iter().map(|p| file_name(p)).collect();
    let mut seen: Vec<String> = Vec::with_capacity(list.len());
    for (path, name) in list.iter().zip(&names) {
        let shared = names.iter().filter(|n| *n == name).count() > 1;
        let label = match (shared, parent_name(path)) {
            (true, Some(folder)) => format!("{name} — {folder}"),
            _ => name.clone(),
        };
        let mut label = elide(&label);
        let base = label.clone();
        let mut n = 2;
        while seen.contains(&label) {
            label = format!("{base} ({n})");
            n += 1;
        }
        seen.push(label);
    }
    seen
}

/// `label` as one FLTK menu item: `/` would start a submenu, `&` marks a
/// shortcut letter, `\` escapes, and a leading `_` draws a divider.
pub fn escape_menu_label(label: &str) -> String {
    let mut out = String::with_capacity(label.len());
    for (i, c) in label.chars().enumerate() {
        match c {
            '\\' => out.push_str("\\\\"),
            '/' => out.push_str("\\/"),
            '&' => out.push_str("&&"),
            '_' if i == 0 => out.push_str("\\_"),
            _ => out.push(c),
        }
    }
    out
}

/// The start page's Recent Files rows: (file name, path), the first
/// [`START_PAGE_ENTRIES`] of the list.
pub fn start_page_entries(list: &[String]) -> Vec<(String, String)> {
    list.iter()
        .take(START_PAGE_ENTRIES)
        .map(|p| (file_name(p), p.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn abs(name: &str) -> String {
        if cfg!(windows) {
            format!("C:\\work\\{name}")
        } else {
            format!("/work/{name}")
        }
    }

    #[test]
    fn recent_files_push_moves_an_existing_path_to_the_front() {
        let mut list = vec![abs("a.txt"), abs("b.txt"), abs("c.txt")];
        push(&mut list, &abs("c.txt"));
        assert_eq!(list, vec![abs("c.txt"), abs("a.txt"), abs("b.txt")]);
        push(&mut list, &abs("d.txt"));
        assert_eq!(list[0], abs("d.txt"));
        assert_eq!(list.len(), 4);
    }

    #[test]
    fn recent_files_push_caps_the_list() {
        let mut list = Vec::new();
        for i in 0..25 {
            push(&mut list, &abs(&format!("f{i}.txt")));
        }
        assert_eq!(list.len(), MAX_ENTRIES);
        assert_eq!(list[0], abs("f24.txt"));
        assert_eq!(list[MAX_ENTRIES - 1], abs("f15.txt"));
    }

    #[test]
    fn recent_files_remove() {
        let mut list = vec![abs("a.txt"), abs("b.txt")];
        assert!(remove(&mut list, &abs("a.txt")));
        assert!(!remove(&mut list, &abs("a.txt")));
        assert_eq!(list, vec![abs("b.txt")]);
    }

    #[test]
    fn recent_files_save_load_round_trip() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file = dir.path().join("nested").join("recent_files.json");
        let list = vec![abs("a.txt"), abs("b.txt")];
        save_to(&file, &list).expect("save");
        assert_eq!(load_from(&file), list);
        assert!(!file.with_extension("json.tmp").exists());
    }

    #[test]
    fn recent_files_unusable_files_read_as_empty() {
        let dir = tempfile::tempdir().expect("tempdir");
        let missing = dir.path().join("missing.json");
        assert!(load_from(&missing).is_empty());

        let empty = dir.path().join("empty.json");
        fs::write(&empty, "").expect("write");
        assert!(load_from(&empty).is_empty());

        let malformed = dir.path().join("malformed.json");
        fs::write(&malformed, "{\"version\": 1, \"files\": [").expect("write");
        assert!(load_from(&malformed).is_empty());

        let wrong_type = dir.path().join("wrong.json");
        fs::write(&wrong_type, "{\"version\": 1, \"files\": [1, 2]}").expect("write");
        assert!(load_from(&wrong_type).is_empty());

        let oversized = dir.path().join("big.json");
        let entry = abs(&"x".repeat(1000));
        let big: Vec<String> = (0..100).map(|_| entry.clone()).collect();
        let json = serde_json::to_string(&RecentFilesData {
            version: 1,
            files: big,
        })
        .expect("json");
        assert!(json.len() as u64 > MAX_FILE_BYTES);
        fs::write(&oversized, json).expect("write");
        assert!(load_from(&oversized).is_empty());
    }

    #[test]
    fn recent_files_load_keeps_only_valid_entries() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file = dir.path().join("recent_files.json");
        let mut files = vec![
            abs("a.txt"),
            String::new(),
            "relative/b.txt".to_string(),
            format!("{}\0x", abs("nul")),
            abs(&"y".repeat(MAX_PATH_BYTES)),
            abs("a.txt"),
        ];
        for i in 0..20 {
            files.push(abs(&format!("f{i}.txt")));
        }
        let json = serde_json::to_string(&RecentFilesData { version: 1, files }).expect("json");
        fs::write(&file, json).expect("write");
        let list = load_from(&file);
        assert_eq!(list.len(), MAX_ENTRIES);
        assert_eq!(list[0], abs("a.txt"));
        assert_eq!(list[1], abs("f0.txt"));
    }

    #[test]
    fn recent_files_labels_add_the_folder_for_shared_names() {
        let list = if cfg!(windows) {
            vec![
                "C:\\one\\notes.txt".to_string(),
                "C:\\two\\notes.txt".to_string(),
                "C:\\one\\main.rs".to_string(),
            ]
        } else {
            vec![
                "/one/notes.txt".to_string(),
                "/two/notes.txt".to_string(),
                "/one/main.rs".to_string(),
            ]
        };
        assert_eq!(
            labels(&list),
            vec!["notes.txt — one", "notes.txt — two", "main.rs"]
        );
    }

    #[test]
    fn recent_files_labels_are_unique() {
        let list = if cfg!(windows) {
            vec![
                "C:\\a\\docs\\n.txt".to_string(),
                "C:\\b\\docs\\n.txt".to_string(),
            ]
        } else {
            vec!["/a/docs/n.txt".to_string(), "/b/docs/n.txt".to_string()]
        };
        assert_eq!(labels(&list), vec!["n.txt — docs", "n.txt — docs (2)"]);
    }

    #[test]
    fn recent_files_labels_elide_long_names() {
        let list = vec![abs(&format!("{}.txt", "n".repeat(200)))];
        let label = &labels(&list)[0];
        assert_eq!(label.chars().count(), MAX_LABEL_CHARS);
        assert!(label.contains('…'));
        assert!(label.ends_with(".txt"));
    }

    #[test]
    fn recent_files_menu_labels_are_escaped() {
        assert_eq!(escape_menu_label("a/b"), "a\\/b");
        assert_eq!(escape_menu_label("R&D.txt"), "R&&D.txt");
        assert_eq!(escape_menu_label("_draft.md"), "\\_draft.md");
        assert_eq!(escape_menu_label("x_y.md"), "x_y.md");
        assert_eq!(escape_menu_label("a\\b"), "a\\\\b");
    }

    #[test]
    fn recent_files_start_page_shows_the_first_four() {
        let list: Vec<String> = (0..7).map(|i| abs(&format!("f{i}.txt"))).collect();
        let rows = start_page_entries(&list);
        assert_eq!(rows.len(), START_PAGE_ENTRIES);
        assert_eq!(rows[0], ("f0.txt".to_string(), abs("f0.txt")));
        assert_eq!(rows[3].1, abs("f3.txt"));
    }
}
