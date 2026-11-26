//! Security utilities for the plugin sandbox.
//!
//! Provides path validation, command sanitization, and other security
//! primitives to protect against malicious plugins.

use std::path::{Component, Path, PathBuf};
use std::time::Duration;

/// Default timeout for a single external command (15 seconds).
///
/// `run_command` waits on the child **synchronously on the UI thread**, so this
/// is a hard ceiling on how long one plugin command can freeze the editor
/// (T0011). Kept generous enough for heavy linters (ruff/eslint finish in well
/// under a second; mypy/tsc on a single file typically within a few seconds)
/// while halving the previous 30 s worst-case freeze. The per-hook wall-clock
/// deadline (`runtime::DEFAULT_HOOK_DEADLINE`) sits above this so a legitimate
/// single-command hook is never cut off mid-run.
pub const DEFAULT_COMMAND_TIMEOUT: Duration = Duration::from_secs(15);

/// Characters that indicate shell injection attempts
const SHELL_INJECTION_CHARS: &[char] = &[
    ';', '&', '|', '`', '$', '(', ')', '{', '}', '<', '>', '\n', '\r', '\\', '*', '?', '[', ']',
];

/// Result of path validation
#[derive(Debug, Clone, PartialEq)]
pub enum PathValidation {
    /// Path is valid and within allowed scope
    Valid(PathBuf),
    /// Path is outside project root
    OutsideProjectRoot,
    /// The path or the project root cannot be resolved (fail closed)
    InvalidPath(String),
}

/// Validate and resolve a plugin-supplied path, ensuring it stays within the
/// project root. This is the **one** path rule of the plugin sandbox (T0090);
/// every plugin filesystem entry point goes through it.
///
/// # Arguments
/// * `path` - The path to validate (relative to the project root, or absolute)
/// * `project_root` - The project root directory (files must be within this)
///
/// # Returns
/// * `PathValidation::Valid(path)`: the physical path to operate on. It may
///   not exist yet (files and directories a write op will create).
/// * `OutsideProjectRoot` / `InvalidPath` otherwise.
///
/// # Rule (physical, never lexical)
/// 1. Raw path: absolute input as-is, relative input joined to the root.
/// 2. Its longest ancestor that exists as a directory entry (`lstat`, so a
///    dangling symlink *is* an existing entry, never part of the new tail).
/// 3. Canonicalize that ancestor: the kernel resolves every symlink and `..`
///    in it. Failure (e.g. a dangling link) rejects the path.
/// 4. The not-yet-existing tail may only hold plain names; `..`, `.`, a root
///    or a prefix there rejects the path.
/// 5. `ancestor + tail` must start with the canonical root.
///
/// So a write can only ever create plain new names under a directory that
/// physically is inside the root: an in-root symlink to the outside (with or
/// without missing intermediate directories) and a dangling in-root symlink
/// are both rejected, while an absolute path under a symlinked alias of the
/// root (macOS `/var` → `/private/var`) is accepted.
///
/// An existing path resolves to its physical target, so `remove` / `rename`
/// of an in-root symlink whose target is also inside the root act on the
/// target, not on the link (as before T0090).
///
/// Out of scope: a symlink created or swapped between this check and the
/// operation (TOCTOU).
pub fn validate_path(path: &str, project_root: &Path) -> PathValidation {
    let canonical_root = match std::fs::canonicalize(project_root) {
        Ok(r) => r,
        Err(e) => {
            return PathValidation::InvalidPath(format!("Cannot canonicalize project root: {}", e));
        }
    };

    let input_path = Path::new(path);
    let raw = if input_path.is_absolute() {
        input_path.to_path_buf()
    } else {
        project_root.join(input_path)
    };

    // Longest ancestor that exists as an entry (symlinks not followed). Only
    // "does not exist" lets the walk go up; any other error (permission,
    // name too long, …) fails closed.
    let mut existing = None;
    for ancestor in raw.ancestors().filter(|a| !a.as_os_str().is_empty()) {
        match std::fs::symlink_metadata(ancestor) {
            Ok(_) => {
                existing = Some(ancestor);
                break;
            }
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
                ) => {}
            Err(e) => return PathValidation::InvalidPath(e.to_string()),
        }
    }
    let Some(existing) = existing else {
        return PathValidation::InvalidPath(format!("No existing ancestor: {}", path));
    };
    let base = match std::fs::canonicalize(existing) {
        Ok(b) => b,
        Err(e) => return PathValidation::InvalidPath(e.to_string()),
    };

    let Ok(tail) = raw.strip_prefix(existing) else {
        return PathValidation::InvalidPath(format!("Cannot split path: {}", path));
    };
    let mut resolved = base;
    for component in tail.components() {
        match component {
            Component::Normal(name) => resolved.push(name),
            _ => return PathValidation::OutsideProjectRoot,
        }
    }

    if resolved.starts_with(&canonical_root) {
        PathValidation::Valid(resolved)
    } else {
        PathValidation::OutsideProjectRoot
    }
}

/// Check if a command argument contains shell injection characters.
///
/// # Returns
/// * `Ok(())` if argument is safe
/// * `Err(reason)` if argument contains dangerous characters
pub fn validate_command_arg(arg: &str) -> Result<(), String> {
    for ch in SHELL_INJECTION_CHARS {
        if arg.contains(*ch) {
            return Err(format!("Argument contains forbidden character: '{}'", ch));
        }
    }
    Ok(())
}

/// Get the project root from a file path.
///
/// Uses a two-pass approach to correctly handle workspaces:
/// 1. First, walk up looking for `.git` — the authoritative VCS root
/// 2. If no `.git` found, fall back to other project markers
///
/// This ensures that workspace subprojects (uv, npm, cargo) don't shadow
/// the actual workspace root where `.git` lives.
///
/// Supported markers:
/// - `.git` directory (preferred — always wins)
/// - `Cargo.toml` (Rust)
/// - `package.json` (Node.js)
/// - `pyproject.toml` or `setup.py` (Python)
/// - `.ferrispad` (custom marker)
///
/// If no marker is found, returns the file's parent directory.
pub fn find_project_root(file_path: &Path) -> Option<PathBuf> {
    let start = if file_path.is_file() {
        file_path.parent()?.to_path_buf()
    } else {
        file_path.to_path_buf()
    };

    // Pass 1: prefer .git (handles workspaces correctly)
    let mut current = start.clone();
    loop {
        if current.join(".git").exists() {
            return Some(current);
        }
        match current.parent() {
            Some(parent) => current = parent.to_path_buf(),
            None => break,
        }
    }

    // Pass 2: fallback to other project markers
    let fallback_markers = [
        "Cargo.toml",
        "package.json",
        "pyproject.toml",
        "setup.py",
        ".ferrispad",
    ];
    current = start;
    loop {
        for marker in &fallback_markers {
            if current.join(marker).exists() {
                return Some(current);
            }
        }
        match current.parent() {
            Some(parent) => current = parent.to_path_buf(),
            None => break,
        }
    }

    // No marker found, return file's parent directory
    if file_path.is_file() {
        file_path.parent().map(|p| p.to_path_buf())
    } else {
        Some(file_path.to_path_buf())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    // T0011 (audit M4): `run_command` waits on the child synchronously on the UI
    // thread, so this timeout is a hard ceiling on how long one plugin command
    // can freeze the editor. Keep it short enough to stay UI-appropriate.
    #[test]
    fn command_timeout_stays_ui_appropriate() {
        assert!(
            DEFAULT_COMMAND_TIMEOUT <= Duration::from_secs(15),
            "command timeout {DEFAULT_COMMAND_TIMEOUT:?} is too long to block the UI thread"
        );
    }

    #[test]
    fn test_validate_path_within_project() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        // Create a file
        let file_path = root.join("test.txt");
        fs::write(&file_path, "test").unwrap();

        // Should be valid
        match validate_path("test.txt", root) {
            PathValidation::Valid(p) => assert_eq!(p, file_path.canonicalize().unwrap()),
            other => panic!("Expected Valid, got {:?}", other),
        }
    }

    #[test]
    fn test_validate_path_traversal_blocked() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        let subdir = root.join("subdir");
        fs::create_dir(&subdir).unwrap();

        // Create a file outside the subdir
        let outside = root.join("outside.txt");
        fs::write(&outside, "secret").unwrap();

        // Trying to access it from subdir via .. should fail
        match validate_path("../outside.txt", &subdir) {
            PathValidation::OutsideProjectRoot => (),
            other => panic!("Expected OutsideProjectRoot, got {:?}", other),
        }
    }

    #[test]
    fn test_validate_path_absolute_outside() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        // Absolute path outside project should fail
        match validate_path("/etc/passwd", root) {
            PathValidation::OutsideProjectRoot => (),
            other => panic!("Expected OutsideProjectRoot, got {:?}", other),
        }
    }

    #[test]
    fn test_validate_command_arg_safe() {
        assert!(validate_command_arg("--output-format=json").is_ok());
        assert!(validate_command_arg("/path/to/file.py").is_ok());
        assert!(validate_command_arg("check").is_ok());
    }

    #[test]
    fn test_validate_command_arg_injection() {
        assert!(validate_command_arg("; rm -rf /").is_err());
        assert!(validate_command_arg("file.txt && cat /etc/passwd").is_err());
        assert!(validate_command_arg("$(whoami)").is_err());
        assert!(validate_command_arg("`id`").is_err());
        assert!(validate_command_arg("test | cat").is_err());
        assert!(validate_command_arg("file\\.txt").is_err());
        assert!(validate_command_arg("*.py").is_err());
        assert!(validate_command_arg("test[0]").is_err());
    }

    #[test]
    fn test_find_project_root_git() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        // Create .git directory
        fs::create_dir(root.join(".git")).unwrap();

        // Create nested file
        let subdir = root.join("src").join("lib");
        fs::create_dir_all(&subdir).unwrap();
        let file = subdir.join("main.rs");
        fs::write(&file, "fn main() {}").unwrap();

        let found = find_project_root(&file).unwrap();
        assert_eq!(found, root);
    }

    #[test]
    fn test_find_project_root_no_marker() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        // Create file without any project markers
        let file = root.join("test.txt");
        fs::write(&file, "test").unwrap();

        let found = find_project_root(&file).unwrap();
        assert_eq!(found, root);
    }
}
