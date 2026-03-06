//! Security utilities for the plugin sandbox.
//!
//! Provides path validation, command sanitization, and other security
//! primitives to protect against malicious plugins.

use std::path::{Path, PathBuf};
use std::time::Duration;

/// Default timeout for external commands (30 seconds)
pub const DEFAULT_COMMAND_TIMEOUT: Duration = Duration::from_secs(30);

/// Characters that indicate shell injection attempts
const SHELL_INJECTION_CHARS: &[char] = &[';', '&', '|', '`', '$', '(', ')', '{', '}', '<', '>', '\n', '\r'];

/// Result of path validation
#[derive(Debug, Clone, PartialEq)]
pub enum PathValidation {
    /// Path is valid and within allowed scope
    Valid(PathBuf),
    /// Path is outside project root
    OutsideProjectRoot,
    /// Path does not exist
    NotFound,
    /// Path canonicalization failed
    InvalidPath(String),
}

/// Validate and canonicalize a path, ensuring it's within the project root.
///
/// # Arguments
/// * `path` - The path to validate (can be relative or absolute)
/// * `project_root` - The project root directory (files must be within this)
///
/// # Returns
/// * `PathValidation::Valid(canonical_path)` if path is safe
/// * Error variant otherwise
///
/// # Security
/// - Uses `fs::canonicalize()` to resolve symlinks and `..` components
/// - Checks that the canonical path starts with the project root
/// - Prevents path traversal attacks like `../../etc/passwd`
pub fn validate_path(path: &str, project_root: &Path) -> PathValidation {
    // Quick check for obvious traversal patterns
    if path.contains("..") {
        // Could be legitimate (e.g., "../sibling/file.txt" from subdir)
        // We'll let canonicalize handle it, but log for awareness
    }

    // Convert to Path
    let input_path = Path::new(path);

    // If relative, resolve against project root
    let full_path = if input_path.is_absolute() {
        input_path.to_path_buf()
    } else {
        project_root.join(input_path)
    };

    // Canonicalize to resolve symlinks and .. components
    let canonical = match std::fs::canonicalize(&full_path) {
        Ok(p) => p,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // File doesn't exist - check if parent is valid
            // This allows checking paths for files that will be created
            if let Some(parent) = full_path.parent() {
                match std::fs::canonicalize(parent) {
                    Ok(canonical_parent) => {
                        // Verify parent is within project root
                        let canonical_root = match std::fs::canonicalize(project_root) {
                            Ok(r) => r,
                            Err(e) => return PathValidation::InvalidPath(format!("Cannot canonicalize project root: {}", e)),
                        };
                        if !canonical_parent.starts_with(&canonical_root) {
                            return PathValidation::OutsideProjectRoot;
                        }
                        // Parent is valid, construct full path
                        if let Some(file_name) = full_path.file_name() {
                            return PathValidation::Valid(canonical_parent.join(file_name));
                        }
                    }
                    Err(_) => return PathValidation::NotFound,
                }
            }
            return PathValidation::NotFound;
        }
        Err(e) => return PathValidation::InvalidPath(e.to_string()),
    };

    // Canonicalize project root for comparison
    let canonical_root = match std::fs::canonicalize(project_root) {
        Ok(r) => r,
        Err(e) => return PathValidation::InvalidPath(format!("Cannot canonicalize project root: {}", e)),
    };

    // Check that canonical path is within project root
    if canonical.starts_with(&canonical_root) {
        PathValidation::Valid(canonical)
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
/// Walks up the directory tree looking for common project markers:
/// - `.git` directory
/// - `Cargo.toml` (Rust)
/// - `package.json` (Node.js)
/// - `pyproject.toml` or `setup.py` (Python)
///
/// If no marker is found, returns the file's parent directory.
pub fn find_project_root(file_path: &Path) -> Option<PathBuf> {
    let mut current = if file_path.is_file() {
        file_path.parent()?.to_path_buf()
    } else {
        file_path.to_path_buf()
    };

    let markers = [
        ".git",
        "Cargo.toml",
        "package.json",
        "pyproject.toml",
        "setup.py",
        ".ferrispad",  // Our own marker
    ];

    loop {
        for marker in &markers {
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
            PathValidation::NotFound => (), // On some systems /etc/passwd might not exist
            other => panic!("Expected OutsideProjectRoot or NotFound, got {:?}", other),
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
