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
    // Win32 rewrites some names when it creates them (trailing dots/spaces
    // dropped, `name:stream`, 8.3 aliases), so the checked name would not be
    // the written one: refuse them outright on Windows (T0092).
    if cfg!(windows)
        && input_path.components().any(|c| match c {
            Component::Normal(name) => windows_name_is_unsafe(&name.to_string_lossy()),
            _ => false,
        })
    {
        return PathValidation::InvalidPath(format!("Unsafe Windows file name: {}", path));
    }
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

/// File systems where `.git` and `.GIT` are the same entry (T0092).
pub const CASE_INSENSITIVE_NAMES: bool = cfg!(any(windows, target_os = "macos"));

/// Whether one path component names a git directory or file (`.git`).
/// `.github`, `x.git` and `.gitkeep` do not.
pub fn is_git_component(name: &str, case_insensitive: bool) -> bool {
    if case_insensitive {
        name.eq_ignore_ascii_case(".git")
    } else {
        name == ".git"
    }
}

/// Whether `physical` (a resolved path) lies in or is a `.git` entry below
/// `canonical_root`.
pub fn path_has_git_component(
    physical: &Path,
    canonical_root: &Path,
    case_insensitive: bool,
) -> bool {
    physical
        .strip_prefix(canonical_root)
        .unwrap_or(physical)
        .components()
        .any(|c| match c {
            Component::Normal(name) => is_git_component(&name.to_string_lossy(), case_insensitive),
            _ => false,
        })
}

/// A Windows path component that Win32 would rewrite on creation, so the
/// checked name is not the written one: a `:` (alternate data stream), a
/// trailing `.` or space (stripped), or a `~` followed by a digit (8.3 alias,
/// e.g. `GIT~1` for `.git`).
pub fn windows_name_is_unsafe(name: &str) -> bool {
    name.contains(':')
        || name.ends_with('.')
        || name.ends_with(' ')
        || name
            .as_bytes()
            .windows(2)
            .any(|w| w[0] == b'~' && w[1].is_ascii_digit())
}

/// Extensions Windows runs directly: `PATHEXT` (or its default when unset)
/// plus `.PS1` and the implicit `.EXE`, compared case-insensitively.
pub fn has_executable_extension(name: &str, pathext: Option<&str>) -> bool {
    const DEFAULT_PATHEXT: &str = ".COM;.EXE;.BAT;.CMD;.VBS;.VBE;.JS;.JSE;.WSF;.WSH;.MSC";
    let Some(dot) = name.rfind('.') else {
        return false;
    };
    let ext = &name[dot..];
    pathext
        .filter(|p| !p.trim().is_empty())
        .unwrap_or(DEFAULT_PATHEXT)
        .split(';')
        .chain([".PS1", ".EXE"])
        .any(|e| !e.trim().is_empty() && e.trim().eq_ignore_ascii_case(ext))
}

/// Whether `path` (followed if a symlink) is an existing executable regular
/// file. On Windows executability is the extension, checked by name.
pub fn is_existing_executable(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        false
    }
}

/// Whether a plugin-supplied link may be handed to the OS opener
/// (`open::that`: ShellExecute / `open` / `xdg-open`), which opens or even
/// *runs* a local path. Only an `http://` or `https://` URL (scheme compared
/// case-insensitively) with a non-empty host part qualifies; anything else
/// (`file:`, a bare or Windows path, `javascript:`, whitespace or control
/// characters anywhere) is refused (T0090).
pub fn is_web_url(url: &str) -> bool {
    let Some((scheme, rest)) = url.split_once("://") else {
        return false;
    };
    let web_scheme = scheme.eq_ignore_ascii_case("https") || scheme.eq_ignore_ascii_case("http");
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    web_scheme && !host.is_empty() && !url.chars().any(|c| c.is_whitespace() || c.is_control())
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

    // ── T0092 name rules (pure: tested on every platform) ─────────────

    #[test]
    fn git_component_matching() {
        assert!(is_git_component(".git", false));
        assert!(!is_git_component(".GIT", false));
        assert!(is_git_component(".GIT", true));
        assert!(is_git_component(".Git", true));
        for other in [".github", "x.git", ".gitkeep", "git", ".git2"] {
            assert!(!is_git_component(other, true), "{other}");
        }
        let root = Path::new("/r");
        assert!(path_has_git_component(
            Path::new("/r/.git/config"),
            root,
            false
        ));
        assert!(path_has_git_component(Path::new("/r/a/.git"), root, false));
        assert!(path_has_git_component(
            Path::new("/r/.GIT/hooks/x"),
            root,
            true
        ));
        assert!(!path_has_git_component(
            Path::new("/r/.github/ci.yml"),
            root,
            true
        ));
    }

    #[test]
    fn windows_unsafe_names() {
        for bad in [
            "evil.exe.",
            "evil.exe ",
            ".git.",
            ".git ",
            "evil.exe::$DATA",
            "a:b",
            "GIT~1",
            "PROGRA~2",
        ] {
            assert!(windows_name_is_unsafe(bad), "{bad:?}");
        }
        for good in ["main.rs", ".git", "a~b", "x~", "notes.txt"] {
            assert!(!windows_name_is_unsafe(good), "{good:?}");
        }
    }

    #[test]
    fn windows_executable_extensions() {
        for name in [
            "a.exe", "a.EXE", "a.bat", "a.Cmd", "a.ps1", "a.js", "a.msc", "a.com",
        ] {
            assert!(has_executable_extension(name, None), "{name}");
        }
        for name in ["a.txt", "a", "exe", "a.exe.txt", "a.rs"] {
            assert!(!has_executable_extension(name, None), "{name}");
        }
        // A custom PATHEXT adds entries; .EXE and .PS1 stay in any case.
        assert!(has_executable_extension("a.py", Some(".COM;.PY")));
        assert!(has_executable_extension("a.exe", Some(".PY")));
        assert!(has_executable_extension("a.ps1", Some(".PY")));
        assert!(!has_executable_extension("a.bat", Some(".PY")));
        assert!(
            has_executable_extension("a.bat", Some("  ")),
            "blank → default"
        );
    }
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
