//! Path validation helpers for plugin sandbox.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use super::super::security::{PathValidation, validate_path};

/// Resolve a user-supplied path against project_root and validate it stays
/// inside the sandbox, using the one physical rule of
/// [`validate_path`](super::super::security::validate_path) (T0090).
/// Returns `Ok(Some(path))` on success (the path may not exist yet),
/// `Ok(None)` if it is blocked or invalid.
pub(super) fn resolve_and_validate(
    path: &str,
    project_root: &Path,
) -> mlua::Result<Option<PathBuf>> {
    match validate_path(path, project_root) {
        PathValidation::Valid(resolved) => Ok(Some(resolved)),
        PathValidation::OutsideProjectRoot | PathValidation::InvalidPath(_) => {
            eprintln!(
                "[plugin:security] path blocked: '{}' outside project root",
                path
            );
            Ok(None)
        }
    }
}

/// Check that no entry below `dir` is named `.git` (same case rules as the
/// `.git` path check), walking at most [`MAX_REMOVE_DEPTH`] levels and
/// [`MAX_REMOVE_ENTRIES`] entries without following symlinks. A symlink is
/// judged by its own name only: removing or moving the directory acts on the
/// link, not on its target. Past a bound, or on any read error, it fails
/// closed.
fn dir_tree_is_git_free(dir: &Path) -> Result<(), &'static str> {
    use super::super::security::{CASE_INSENSITIVE_NAMES, is_git_component};
    let mut stack = vec![(dir.to_path_buf(), 0usize)];
    let mut seen = 0usize;
    while let Some((current, depth)) = stack.pop() {
        let entries = std::fs::read_dir(&current).map_err(|_| "Cannot inspect directory")?;
        for entry in entries {
            let entry = entry.map_err(|_| "Cannot inspect directory")?;
            seen += 1;
            if seen > MAX_REMOVE_ENTRIES {
                return Err("Directory too large for a plugin to remove or move");
            }
            if is_git_component(&entry.file_name().to_string_lossy(), CASE_INSENSITIVE_NAMES) {
                return Err("Directory contains .git");
            }
            let file_type = entry.file_type().map_err(|_| "Cannot inspect directory")?;
            if file_type.is_dir() {
                if depth + 1 >= MAX_REMOVE_DEPTH {
                    return Err("Directory too deep for a plugin to remove or move");
                }
                stack.push((entry.path(), depth + 1));
            }
        }
    }
    Ok(())
}

/// Bounds of the walk that checks a directory before removing or moving it.
pub(super) const MAX_REMOVE_DEPTH: usize = 32;
/// See [`MAX_REMOVE_DEPTH`].
pub(super) const MAX_REMOVE_ENTRIES: usize = 10_000;

/// What a write operation does to its target (T0092).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum WriteKind {
    /// Creates or replaces the target's content (write, create, rename
    /// destination): not under `.git`, not onto an existing executable,
    /// not an executable extension on Windows.
    Create,
    /// Moves or deletes the target (rename source, remove): not under `.git`.
    /// Moving or removing an executable is allowed.
    Modify,
}

/// [`resolve_and_validate`] for an operation that changes the file system,
/// plus the write policy (T0092). `Err` carries the reason for the plugin.
///
/// The policy closes the plugin-controlled write → execute path: a plugin
/// with an approved command could otherwise overwrite that command's binary
/// (`fs::write` keeps an existing file's mode), drop a runnable file on
/// Windows, or rewrite `.git/config` / hooks that the approved `git` runs.
pub(super) fn resolve_for_write(
    path: &str,
    project_root: &Path,
    kind: WriteKind,
) -> Result<PathBuf, String> {
    use super::super::security::{
        CASE_INSENSITIVE_NAMES, has_executable_extension, is_existing_executable,
        path_has_git_component,
    };
    let Ok(Some(physical)) = resolve_and_validate(path, project_root) else {
        return Err("Path outside project root".to_string());
    };
    let canonical_root = std::fs::canonicalize(project_root)
        .map_err(|e| format!("Cannot canonicalize project root: {}", e))?;
    let refuse = |why: &str| {
        eprintln!("[plugin:security] write blocked: '{}': {}", path, why);
        Err(why.to_string())
    };
    if path_has_git_component(&physical, &canonical_root, CASE_INSENSITIVE_NAMES) {
        return refuse("Plugins may not modify .git");
    }
    // The project root itself — "", ".", "sub/..", an alias or a symlink to
    // it all resolve here — is never removed, moved or replaced.
    if physical == canonical_root {
        return refuse("Plugins may not remove, move or replace the project root");
    }
    // Removing or moving a directory takes everything below it along: walk
    // it first (bounded, symlinks not followed) so no `.git` goes with it.
    if kind == WriteKind::Modify
        && std::fs::symlink_metadata(&physical).is_ok_and(|m| m.is_dir())
        && let Err(why) = dir_tree_is_git_free(&physical)
    {
        return refuse(why);
    }
    if kind == WriteKind::Create {
        let name = physical
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if cfg!(windows)
            && has_executable_extension(&name, std::env::var("PATHEXT").ok().as_deref())
        {
            return refuse("Plugins may not create executable files");
        }
        if is_existing_executable(&physical) {
            return refuse("Plugins may not overwrite an executable");
        }
    }
    Ok(physical)
}

/// Hard cap on the number of entries a single directory scan may return.
///
/// Bounds Rust-side work even on a pathologically large real tree, and (with the
/// symlink guard below) guarantees termination regardless of the directory
/// shape (T0008, audit M3).
pub(super) const MAX_SCAN_ENTRIES: usize = 10_000;

/// Entry from a directory scan.
pub(super) struct ScanEntry {
    pub name: String,
    pub rel_path: String,
    pub is_dir: bool,
    /// For directories at the depth boundary: true if the directory has children.
    /// Used by plugins to mark lazy-loadable nodes.
    pub has_children: Option<bool>,
}

/// Recursively scan a directory, collecting entries up to `max_depth`.
/// Paths are returned with `/` separators on all platforms.
/// Directories at the depth boundary get a `has_children` flag so plugins
/// can show them as expandable even before their contents are loaded.
/// `skip_dirs` contains directory names to skip entirely during the walk.
pub(super) fn scan_dir_recursive(
    root: &Path,
    current: &Path,
    max_depth: u32,
    current_depth: u32,
    results: &mut Vec<ScanEntry>,
    skip_dirs: &HashSet<String>,
) {
    if current_depth > max_depth || results.len() >= MAX_SCAN_ENTRIES {
        return;
    }

    let entries = match std::fs::read_dir(current) {
        Ok(e) => e,
        Err(_) => return,
    };

    let mut sorted: Vec<_> = entries.filter_map(|e| e.ok()).collect();
    sorted.sort_by_key(|e| e.file_name());

    for entry in sorted {
        if results.len() >= MAX_SCAN_ENTRIES {
            break;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let path = entry.path();
        // Detect symlinks with the DirEntry's own file type (does NOT follow the
        // link), so a symlinked directory is listed but never descended into —
        // otherwise a cycle (e.g. a link to `.`) explodes the scan (T0008).
        let is_symlink = entry.file_type().map(|t| t.is_symlink()).unwrap_or(false);
        let is_dir = path.is_dir();

        // Skip ignored directories before recursing into them
        if is_dir && skip_dirs.contains(&name) {
            continue;
        }

        // Build relative path with forward slashes
        let rel = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");

        if is_dir && current_depth == max_depth {
            // At the depth boundary: peek to check if dir has children
            let has_children = std::fs::read_dir(&path)
                .map(|mut rd| rd.next().is_some())
                .unwrap_or(false);
            results.push(ScanEntry {
                name,
                rel_path: rel,
                is_dir: true,
                has_children: Some(has_children),
            });
        } else {
            results.push(ScanEntry {
                name,
                rel_path: rel,
                is_dir,
                has_children: None,
            });

            // Recurse into real directories only, never through a symlink, so
            // symlink cycles cannot make the scan diverge (T0008).
            if is_dir && !is_symlink {
                scan_dir_recursive(
                    root,
                    &path,
                    max_depth,
                    current_depth + 1,
                    results,
                    skip_dirs,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    // Regression (T0008, audit M3): the scan must not follow symlinked
    // directories, so a symlink cycle (a link back to the scanned dir) cannot
    // make it diverge. Before the fix, `path.is_dir()` followed the link and the
    // walk descended through it, producing entries nested under the symlink up
    // to the depth cap.
    #[cfg(unix)]
    #[test]
    fn scan_does_not_follow_symlinked_directories() {
        use std::os::unix::fs::symlink;

        let dir = tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join("real.txt"), b"x").unwrap();
        fs::create_dir(root.join("sub")).unwrap();
        fs::write(root.join("sub").join("inner.txt"), b"y").unwrap();
        // Cycle: a symlink pointing back at the scanned root.
        symlink(root, root.join("loop")).unwrap();

        let mut results = Vec::new();
        scan_dir_recursive(root, root, 10, 1, &mut results, &HashSet::new());

        // The symlink itself is listed...
        assert!(
            results.iter().any(|e| e.name == "loop"),
            "the symlink entry should still be reported"
        );
        // ...but nothing is reported *under* it: the walk never descended.
        assert!(
            results.iter().all(|e| !e.rel_path.contains("loop/")),
            "scan followed a symlinked directory (cycle): {:?}",
            results
                .iter()
                .map(|e| e.rel_path.clone())
                .collect::<Vec<_>>()
        );
        // Real subdirectories are still traversed.
        assert!(results.iter().any(|e| e.rel_path == "sub/inner.txt"));
        // And the scan stays bounded.
        assert!(results.len() < MAX_SCAN_ENTRIES);
    }

    // The total number of returned entries is capped even for a large real tree.
    #[test]
    fn scan_output_is_capped() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        // Create comfortably more than the cap of flat files.
        for i in 0..(MAX_SCAN_ENTRIES + 500) {
            fs::write(root.join(format!("f{i:05}.txt")), b"").unwrap();
        }

        let mut results = Vec::new();
        scan_dir_recursive(root, root, 5, 1, &mut results, &HashSet::new());

        assert!(
            results.len() <= MAX_SCAN_ENTRIES,
            "scan returned {} entries, over the {} cap",
            results.len(),
            MAX_SCAN_ENTRIES
        );
    }
}
