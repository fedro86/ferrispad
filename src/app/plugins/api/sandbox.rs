//! Path validation helpers for plugin sandbox.

use std::path::{Path, PathBuf};

use super::super::security::{validate_path, PathValidation};

/// Resolve a user-supplied path against project_root and validate it stays inside the sandbox.
/// Returns `Ok(Some(canonical))` on success, `Ok(None)` if the path is blocked or invalid.
pub(super) fn resolve_and_validate(path: &str, project_root: &Path) -> mlua::Result<Option<PathBuf>> {
    match validate_path(path, project_root) {
        PathValidation::Valid(canonical) => Ok(Some(canonical)),
        PathValidation::NotFound => {
            // For write ops the target doesn't exist yet — still valid if it
            // would land inside the sandbox. Walk up to the nearest existing
            // ancestor and verify it's within the project root.
            let full = if Path::new(path).is_absolute() {
                PathBuf::from(path)
            } else {
                project_root.join(path)
            };
            let canonical_root = std::fs::canonicalize(project_root).map_err(|e| {
                mlua::Error::RuntimeError(format!(
                    "Cannot canonicalize project root: {}",
                    e
                ))
            })?;
            // Walk up ancestors until we find one that exists and can be canonicalized
            let mut ancestor = full.as_path();
            loop {
                match std::fs::canonicalize(ancestor) {
                    Ok(canonical_ancestor)
                        if canonical_ancestor.starts_with(&canonical_root) =>
                    {
                        return Ok(Some(full));
                    }
                    Ok(_) => {
                        // Exists but outside project root
                        eprintln!(
                            "[plugin:security] path blocked: '{}' resolves outside project root",
                            path
                        );
                        return Ok(None);
                    }
                    Err(_) => {
                        // Doesn't exist — try parent
                        match ancestor.parent() {
                            Some(parent) if parent != ancestor => ancestor = parent,
                            _ => {
                                // Reached filesystem root without finding an existing ancestor
                                eprintln!(
                                    "[plugin:security] path blocked: '{}' no valid ancestor in project root",
                                    path
                                );
                                return Ok(None);
                            }
                        }
                    }
                }
            }
        }
        PathValidation::OutsideProjectRoot
        | PathValidation::InvalidPath(_) => {
            eprintln!(
                "[plugin:security] path blocked: '{}' outside project root",
                path
            );
            Ok(None)
        }
    }
}

/// Recursively scan a directory, collecting entries up to `max_depth`.
/// Paths are returned with `/` separators on all platforms.
pub(super) fn scan_dir_recursive(
    root: &Path,
    current: &Path,
    max_depth: u32,
    current_depth: u32,
    results: &mut Vec<(String, String, bool)>, // (name, rel_path, is_dir)
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

        // Build relative path with forward slashes
        let rel = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");

        results.push((name, rel, is_dir));

        if is_dir {
            scan_dir_recursive(root, &path, max_depth, current_depth + 1, results);
        }
    }
}
