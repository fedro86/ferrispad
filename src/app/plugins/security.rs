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

/// The user's home directory as a trusted place for plugin programs (T0094),
/// or `None`. Fails closed: unset, empty, relative or uncanonicalizable means
/// "no home", and so does a home that is the filesystem root (`HOME=/` in some
/// services and containers), which would trust every directory, or a
/// world-writable one (`HOME=/tmp`), where anyone can drop a binary.
pub fn trusted_home(value: Option<&std::ffi::OsStr>) -> Option<PathBuf> {
    let value = value.filter(|v| !v.is_empty())?;
    let path = Path::new(value);
    if !path.is_absolute() {
        return None;
    }
    let canonical = std::fs::canonicalize(path).ok()?;
    if canonical.parent().is_none() || is_world_writable(&canonical) {
        return None;
    }
    Some(canonical)
}

/// A directory anyone may write to (Unix: the `o+w` bit, sticky or not).
fn is_world_writable(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(path).map_or(true, |m| m.permissions().mode() & 0o002 != 0)
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        false
    }
}

/// A Windows program path that is neither absolute nor plainly relative:
/// drive-relative (`C:ruff`, resolved against that drive's own current
/// directory) or root-relative (`\ruff`, `/ruff`: the current drive's root).
/// Joining either to the project root would replace the root, so they are
/// refused as ambiguous. UNC paths (`\\server\share`) are absolute.
pub fn is_ambiguous_windows_program(cmd: &str) -> bool {
    let b = cmd.as_bytes();
    let sep = |c: u8| c == b'/' || c == b'\\';
    let drive_relative =
        b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':' && !(b.len() >= 3 && sep(b[2]));
    let root_relative = !b.is_empty() && sep(b[0]) && !(b.len() >= 2 && sep(b[1]));
    drive_relative || root_relative
}

/// [`trusted_home`] from the environment: `USERPROFILE` on Windows, `HOME`
/// elsewhere.
pub fn trusted_home_from_env() -> Option<PathBuf> {
    let var = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    trusted_home(std::env::var_os(var).as_deref())
}

/// A program given by name only, which the OS looks up on `PATH`. Anything
/// with a separator or a drive colon is a path.
pub fn is_bare_program(cmd: &str) -> bool {
    !cmd.contains(['/', '\\', ':'])
}

/// Where a non-bare plugin program may be run from (T0094). Returns the path to
/// pass to the OS, or why it is refused.
///
/// - A relative program is anchored at the project root (no root: refused) and
///   run by that absolute path, so the platforms' different handling of a
///   relative program with `current_dir` never matters.
/// - With `windows`, a program without an extension gets `.exe`, which is what
///   the OS would run.
/// - Its **parent directory**, physically resolved, must lie inside the
///   project root or the trusted home. The file itself may be a symlink
///   anywhere (a venv's `bin/python` → `/usr/bin/python3`) as long as it
///   resolves to an existing executable regular file. This stops a binary
///   dropped in a shared directory such as `/tmp` from passing as an
///   approved command by its name.
///
/// Invariant: plugins cannot create symlinks (rename and remove act on the
/// resolved target, and there is no symlink API). A future plugin API that
/// creates links re-opens this parent-directory rule.
pub fn resolve_plugin_program(
    cmd: &str,
    project_root: Option<&Path>,
    home: Option<&Path>,
    windows: bool,
    pathext: Option<&str>,
) -> Result<PathBuf, String> {
    // A path the OS itself calls absolute is never ambiguous; on Windows,
    // `C:ruff`, `\\ruff` and `/ruff` are not absolute.
    if windows && !Path::new(cmd).is_absolute() && is_ambiguous_windows_program(cmd) {
        return Err("ambiguous program path (drive- or root-relative)".into());
    }
    let requested = Path::new(cmd);
    let mut program = if requested.is_absolute() {
        requested.to_path_buf()
    } else {
        project_root
            .ok_or("a relative program needs a project root")?
            .join(requested)
    };
    if windows && program.extension().is_none() {
        program.set_extension("exe");
    }
    let parent = program
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .ok_or("program has no directory")?;
    let parent = std::fs::canonicalize(parent).map_err(|_| "program directory not found")?;
    let canonical_root = project_root.and_then(|r| std::fs::canonicalize(r).ok());
    let trusted = canonical_root
        .as_deref()
        .is_some_and(|root| parent.starts_with(root))
        || home.is_some_and(|home| parent.starts_with(home));
    if !trusted {
        return Err("program directory is outside the project and the home directory".into());
    }
    let meta = std::fs::metadata(&program).map_err(|_| "program not found")?;
    let name = program
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let executable = meta.is_file()
        && if windows {
            has_executable_extension(&name, pathext)
        } else {
            is_existing_executable(&program)
        };
    if !executable {
        return Err("program is not an executable file".into());
    }
    Ok(program)
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

    // ── T0094: trusted home and program paths ─────────────────────────

    #[test]
    fn trusted_home_fails_closed() {
        use std::ffi::OsStr;
        assert_eq!(trusted_home(None), None, "unset");
        assert_eq!(trusted_home(Some(OsStr::new(""))), None, "empty");
        assert_eq!(
            trusted_home(Some(OsStr::new("relative/home"))),
            None,
            "relative"
        );
        assert_eq!(
            trusted_home(Some(OsStr::new("/nonexistent/ferrispad-home"))),
            None,
            "uncanonicalizable"
        );
        #[cfg(unix)]
        assert_eq!(
            trusted_home(Some(OsStr::new("/"))),
            None,
            "the filesystem root"
        );
        let dir = tempdir().unwrap();
        assert_eq!(
            trusted_home(Some(dir.path().as_os_str())),
            Some(fs::canonicalize(dir.path()).unwrap())
        );
        // A world-writable home (HOME=/tmp in services, containers, CI).
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            for mode in [0o1777, 0o777, 0o757] {
                fs::set_permissions(dir.path(), fs::Permissions::from_mode(mode)).unwrap();
                assert_eq!(trusted_home(Some(dir.path().as_os_str())), None, "{mode:o}");
            }
            fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o755)).unwrap();
        }
    }

    #[test]
    fn bare_program_names() {
        for bare in ["ruff", "cargo", "git", "ruff.exe"] {
            assert!(is_bare_program(bare), "{bare}");
        }
        for path in [
            "./ruff",
            ".venv/bin/ruff",
            "/usr/bin/ruff",
            r"tools\ruff",
            "C:ruff",
        ] {
            assert!(!is_bare_program(path), "{path}");
        }
    }

    /// The Windows naming is plain logic: exercised on every platform.
    #[test]
    fn program_rule_windows_naming() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("tools")).unwrap();
        fs::write(root.join("tools/ruff.exe"), "MZ").unwrap();
        fs::write(root.join("tools/ruff.txt"), "x").unwrap();
        // No extension: the `.exe` the OS would run is what gets validated.
        let p = resolve_plugin_program("tools/ruff", Some(root), None, true, None).unwrap();
        assert_eq!(p, root.join("tools/ruff.exe"));
        // A non-executable extension is refused.
        assert!(resolve_plugin_program("tools/ruff.txt", Some(root), None, true, None).is_err());
        // Only a trusted directory: the home also counts.
        assert!(resolve_plugin_program("tools/ruff", None, None, true, None).is_err());
        let abs = root.join("tools/ruff").to_string_lossy().into_owned();
        assert!(resolve_plugin_program(&abs, None, None, true, None).is_err());
        let home = fs::canonicalize(root).unwrap();
        assert!(resolve_plugin_program(&abs, None, Some(&home), true, None).is_ok());
        // Drive- and root-relative programs are ambiguous on Windows.
        for ambiguous in ["C:ruff", "c:tools/ruff", "\\ruff", "/ruff", "\\tools\\ruff"] {
            assert!(is_ambiguous_windows_program(ambiguous), "{ambiguous}");
            // ("/ruff" is absolute on the host running this test, so only
            // the pure rule applies to it here; on Windows it is not.)
            if !Path::new(ambiguous).is_absolute() {
                assert!(
                    resolve_plugin_program(ambiguous, Some(root), Some(&home), true, None).is_err(),
                    "{ambiguous}"
                );
            }
        }
        for clear in [
            "C:/x/ruff",
            "C:\\x\\ruff",
            "\\\\server\\share\\ruff",
            "tools/ruff",
            "ruff",
        ] {
            assert!(!is_ambiguous_windows_program(clear), "{clear}");
        }
    }

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
