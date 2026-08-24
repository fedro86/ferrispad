//! Path validation helpers for plugin sandbox.

use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};

use super::super::security::{PathValidation, validate_path};

/// Lexically fold `.` and `..` out of a path **without touching the filesystem**.
///
/// `..` pops the previous *normal* component and can never rise above the
/// root/prefix (an unmatched `..` is kept, so an out-of-root path stays
/// out-of-root). This makes a `..` that a later `create_dir_all`/`fs::write`
/// would resolve against the kernel visible to the containment check *before*
/// any write happens — the sandbox escape in T0002.
fn normalize_lexical(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for comp in path.components() {
        match comp {
            Component::Prefix(_) | Component::RootDir => out.push(comp.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                let pops_normal =
                    matches!(out.components().next_back(), Some(Component::Normal(_)));
                if pops_normal {
                    out.pop();
                } else {
                    out.push("..");
                }
            }
            Component::Normal(c) => out.push(c),
        }
    }
    out
}

/// Resolve a user-supplied path against project_root and validate it stays inside the sandbox.
/// Returns `Ok(Some(canonical))` on success, `Ok(None)` if the path is blocked or invalid.
pub(super) fn resolve_and_validate(
    path: &str,
    project_root: &Path,
) -> mlua::Result<Option<PathBuf>> {
    match validate_path(path, project_root) {
        PathValidation::Valid(canonical) => Ok(Some(canonical)),
        PathValidation::NotFound => {
            // For write ops the target doesn't exist yet (e.g. `create_dir_all`
            // of nested missing dirs). We can't canonicalize a path that isn't
            // there, so instead fold `.`/`..` lexically and require the result
            // to stay inside the *canonical* root. Basing the candidate on the
            // canonical root keeps a symlinked project root from skewing the
            // containment check. (Symlinks *inside* the path are out of scope —
            // that is T0008.)
            let canonical_root = std::fs::canonicalize(project_root).map_err(|e| {
                mlua::Error::RuntimeError(format!("Cannot canonicalize project root: {}", e))
            })?;
            let candidate = if Path::new(path).is_absolute() {
                PathBuf::from(path)
            } else {
                canonical_root.join(path)
            };
            let normalized = normalize_lexical(&candidate);
            if normalized.starts_with(&canonical_root) {
                Ok(Some(normalized))
            } else {
                eprintln!(
                    "[plugin:security] path blocked: '{}' resolves outside project root",
                    path
                );
                Ok(None)
            }
        }
        PathValidation::OutsideProjectRoot | PathValidation::InvalidPath(_) => {
            eprintln!(
                "[plugin:security] path blocked: '{}' outside project root",
                path
            );
            Ok(None)
        }
    }
}

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
    if current_depth > max_depth {
        return;
    }

    let entries = match std::fs::read_dir(current) {
        Ok(e) => e,
        Err(_) => return,
    };

    let mut sorted: Vec<_> = entries.filter_map(|e| e.ok()).collect();
    sorted.sort_by_key(|e| e.file_name());

    for entry in sorted {
        let name = entry.file_name().to_string_lossy().into_owned();
        let path = entry.path();
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

            if is_dir {
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
