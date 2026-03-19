//! Editor API exposed to Lua plugins.
//!
//! This module provides a read-only API for plugins to access
//! document information and perform logging.
//!
//! Also provides controlled access to external commands for linting.
//!
//! ## Security
//!
//! All file system operations are sandboxed to the project root directory.
//! Path traversal attacks (e.g., `../../etc/passwd`) are blocked.

mod commands;
mod editor;
mod filesystem;
mod sandbox;

use mlua::{UserData, UserDataMethods};
use std::collections::HashMap;
use std::path::PathBuf;

use super::security::find_project_root;

/// Editor state passed to plugin hooks.
/// This is a snapshot of the current document state.
#[derive(Debug, Clone, Default)]
pub struct EditorApi {
    /// Current document content (for hooks that need it)
    pub text: Option<String>,

    /// Current file path (None for untitled documents)
    pub file_path: Option<String>,

    /// Detected language/syntax name
    pub language: Option<String>,

    /// Whether the document has unsaved changes
    pub is_dirty: bool,

    /// Cursor position in the buffer
    pub cursor_position: i32,

    /// Selected text (if any)
    pub selection: Option<String>,

    /// Project root directory for sandbox validation.
    /// File system operations are restricted to this directory.
    pub project_root: Option<PathBuf>,

    /// Plugin name (for permission checking and logging)
    pub plugin_name: Option<String>,

    /// Commands this plugin is allowed to execute.
    /// Empty means no commands are allowed (strict mode).
    pub allowed_commands: Vec<String>,

    /// Plugin-specific configuration from user settings.
    /// Key-value pairs configured via Plugins > {Plugin} > Settings.
    pub config: HashMap<String, String>,
}

impl EditorApi {
    /// Compute project root from a file path
    fn compute_project_root(path: Option<&str>) -> Option<PathBuf> {
        path.and_then(|p| find_project_root(std::path::Path::new(p)))
    }

    /// Create an EditorApi with a known project root directory (for init/shutdown hooks)
    pub fn with_project_root(root: Option<String>) -> Self {
        Self {
            project_root: root.map(PathBuf::from),
            ..Default::default()
        }
    }

    /// Create an EditorApi with just a file path (for open/close hooks)
    pub fn with_path(path: Option<String>) -> Self {
        let project_root = Self::compute_project_root(path.as_deref());
        Self {
            file_path: path,
            project_root,
            ..Default::default()
        }
    }

    /// Create an EditorApi with content for save hooks
    pub fn with_content(path: String, content: String) -> Self {
        let project_root = Self::compute_project_root(Some(&path));
        Self {
            text: Some(content),
            file_path: Some(path),
            project_root,
            ..Default::default()
        }
    }

    /// Create an EditorApi with optional path and content for highlight request hooks
    pub fn with_path_and_content(path: Option<String>, content: String) -> Self {
        let project_root = Self::compute_project_root(path.as_deref());
        Self {
            text: Some(content),
            file_path: path,
            project_root,
            ..Default::default()
        }
    }

    /// Create an EditorApi for text change hooks
    pub fn for_text_change(
        position: i32,
        _inserted_len: i32,
        _deleted_len: i32,
        path: Option<String>,
    ) -> Self {
        let project_root = Self::compute_project_root(path.as_deref());
        Self {
            file_path: path,
            cursor_position: position,
            project_root,
            ..Default::default()
        }
    }
}

impl UserData for EditorApi {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // ── Editor query methods ─────────────────────────────────────
        methods.add_method("get_text", editor::get_text);
        methods.add_method("get_file_path", editor::get_file_path);
        methods.add_method("get_language", editor::get_language);
        methods.add_method("is_dirty", editor::is_dirty);
        methods.add_method("get_cursor_position", editor::get_cursor_position);
        methods.add_method("get_selection", editor::get_selection);
        methods.add_method("get_line", editor::get_line);
        methods.add_method("get_file_extension", editor::get_file_extension);
        methods.add_method("log", editor::log);
        methods.add_method("get_file_dir", editor::get_file_dir);
        methods.add_method("get_project_root", editor::get_project_root);
        methods.add_method("get_config", editor::get_config);
        methods.add_method("get_config_number", editor::get_config_number);
        methods.add_method("get_config_bool", editor::get_config_bool);
        methods.add_method("get_mcp_port", editor::get_mcp_port);
        methods.add_method("get_binary_path", editor::get_binary_path);

        // ── Command execution ────────────────────────────────────────
        methods.add_method("run_command", commands::run_command);
        methods.add_method("command_exists", commands::command_exists);

        // ── Filesystem operations ────────────────────────────────────
        methods.add_method("file_exists", filesystem::file_exists);
        methods.add_method("read_file", filesystem::read_file);
        methods.add_method("is_file", filesystem::is_file);
        methods.add_method("list_dir", filesystem::list_dir);
        methods.add_method("scan_dir", filesystem::scan_dir);
        methods.add_method("create_file", filesystem::create_file);
        methods.add_method("write_file", filesystem::write_file);
        methods.add_method("create_dir", filesystem::create_dir);
        methods.add_method("rename", filesystem::rename);
        methods.add_method("remove", filesystem::remove);

        // ── Git & diff ───────────────────────────────────────────────
        methods.add_method("git_status", filesystem::git_status);
        methods.add_method("diff_text", filesystem::diff_text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_editor_api_default() {
        let api = EditorApi::default();
        assert!(api.text.is_none());
        assert!(api.file_path.is_none());
        assert!(!api.is_dirty);
        assert_eq!(api.cursor_position, 0);
    }

    #[test]
    fn test_editor_api_with_path() {
        let api = EditorApi::with_path(Some("/test/file.txt".to_string()));
        assert_eq!(api.file_path, Some("/test/file.txt".to_string()));
    }

    #[test]
    fn test_editor_api_with_content() {
        let api = EditorApi::with_content("/test/file.txt".to_string(), "hello world".to_string());
        assert_eq!(api.file_path, Some("/test/file.txt".to_string()));
        assert_eq!(api.text, Some("hello world".to_string()));
    }

    // ── Filesystem API tests ────────────────────────────────────────

    use mlua::ObjectLike;
    use tempfile::tempdir;

    /// Helper: create an EditorApi with a temp dir as project root
    fn api_with_root(root: &Path) -> EditorApi {
        EditorApi {
            project_root: Some(root.to_path_buf()),
            ..Default::default()
        }
    }

    #[test]
    fn test_resolve_and_validate_inside() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        std::fs::write(root.join("file.txt"), "hi").unwrap();

        let result = sandbox::resolve_and_validate("file.txt", root).unwrap();
        assert!(result.is_some());
    }

    #[test]
    fn test_resolve_and_validate_outside_blocked() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        let result = sandbox::resolve_and_validate("/etc/passwd", root).unwrap();
        assert!(result.is_none(), "Path outside project root should be None");
    }

    #[test]
    fn test_scan_dir_recursive_depth() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        // Create: root/a/b/c/d.txt (depth 3 dirs + 1 file)
        let deep = root.join("a").join("b").join("c");
        std::fs::create_dir_all(&deep).unwrap();
        std::fs::write(deep.join("d.txt"), "deep").unwrap();

        // max_depth=2 should NOT reach c/ or d.txt
        let mut results = Vec::new();
        sandbox::scan_dir_recursive(root, root, 2, 1, &mut results);
        let paths: Vec<&str> = results.iter().map(|(_, r, _)| r.as_str()).collect();
        assert!(paths.contains(&"a"), "Should find a/");
        assert!(paths.contains(&"a/b"), "Should find a/b/");
        assert!(!paths.contains(&"a/b/c"), "Depth 2 should not reach a/b/c/");
        assert!(
            !paths.contains(&"a/b/c/d.txt"),
            "Depth 2 should not reach a/b/c/d.txt"
        );
    }

    #[test]
    fn test_scan_dir_recursive_full() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir(root.join("sub")).unwrap();
        std::fs::write(root.join("sub").join("hello.txt"), "hi").unwrap();
        std::fs::write(root.join("top.txt"), "top").unwrap();

        let mut results = Vec::new();
        sandbox::scan_dir_recursive(root, root, 5, 1, &mut results);

        let names: Vec<&str> = results.iter().map(|(n, _, _)| n.as_str()).collect();
        assert!(names.contains(&"sub"));
        assert!(names.contains(&"hello.txt"));
        assert!(names.contains(&"top.txt"));

        // Check is_dir flag
        let sub_entry = results.iter().find(|(n, _, _)| n == "sub").unwrap();
        assert!(sub_entry.2, "sub should be a directory");
        let file_entry = results.iter().find(|(n, _, _)| n == "top.txt").unwrap();
        assert!(!file_entry.2, "top.txt should not be a directory");
    }

    // The following tests exercise the methods via mlua to confirm they work end-to-end.

    #[test]
    fn test_lua_is_file() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        std::fs::write(root.join("exists.txt"), "data").unwrap();
        std::fs::create_dir(root.join("adir")).unwrap();

        let lua = mlua::Lua::new();
        let api = api_with_root(root);
        lua.scope(|scope| {
            let ud = scope.create_userdata(api).unwrap();
            let is_file: mlua::Function = ud.get("is_file").unwrap();

            let yes: bool = is_file
                .call((&ud, root.join("exists.txt").to_str().unwrap().to_string()))
                .unwrap();
            assert!(yes, "exists.txt should be a file");

            let no: bool = is_file
                .call((&ud, root.join("adir").to_str().unwrap().to_string()))
                .unwrap();
            assert!(!no, "adir is a directory, not a file");

            let no2: bool = is_file
                .call((&ud, root.join("nope.txt").to_str().unwrap().to_string()))
                .unwrap();
            assert!(!no2, "nope.txt does not exist");
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn test_lua_list_dir() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        std::fs::write(root.join("a.txt"), "").unwrap();
        std::fs::create_dir(root.join("sub")).unwrap();

        let lua = mlua::Lua::new();
        let api = api_with_root(root);
        lua.scope(|scope| {
            let ud = scope.create_userdata(api).unwrap();
            let list_dir: mlua::Function = ud.get("list_dir").unwrap();

            let tbl: mlua::Table = list_dir
                .call((&ud, root.to_str().unwrap().to_string()))
                .unwrap();
            let len = tbl.len().unwrap();
            assert!(len >= 2, "Should have at least 2 entries, got {}", len);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn test_lua_create_file_and_remove() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        let lua = mlua::Lua::new();
        let api = api_with_root(root);
        let file_path = root.join("new.txt");
        let file_str = file_path.to_str().unwrap().to_string();

        lua.scope(|scope| {
            let ud = scope.create_userdata(api).unwrap();

            // create_file
            let create: mlua::Function = ud.get("create_file").unwrap();
            let (ok, err): (bool, String) = create.call((&ud, file_str.clone())).unwrap();
            assert!(ok, "create_file should succeed: {}", err);
            assert!(file_path.exists());

            // create_file again should fail (create_new)
            let (ok2, _): (bool, String) = create.call((&ud, file_str.clone())).unwrap();
            assert!(!ok2, "create_file on existing file should fail");

            // remove
            let remove: mlua::Function = ud.get("remove").unwrap();
            let (ok3, err3): (bool, String) = remove.call((&ud, file_str.clone())).unwrap();
            assert!(ok3, "remove should succeed: {}", err3);
            assert!(!file_path.exists());
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn test_lua_create_dir_and_rename() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        let lua = mlua::Lua::new();
        let api = api_with_root(root);
        let dir_path = root.join("nested").join("deep");
        let dir_str = dir_path.to_str().unwrap().to_string();
        let renamed = root.join("nested").join("renamed");
        let renamed_str = renamed.to_str().unwrap().to_string();

        lua.scope(|scope| {
            let ud = scope.create_userdata(api).unwrap();

            // create_dir (nested)
            let mkdir: mlua::Function = ud.get("create_dir").unwrap();
            let (ok, err): (bool, String) = mkdir.call((&ud, dir_str.clone())).unwrap();
            assert!(ok, "create_dir should succeed: {}", err);
            assert!(dir_path.is_dir());

            // rename
            let rename_fn: mlua::Function = ud.get("rename").unwrap();
            let (ok2, err2): (bool, String) = rename_fn.call((&ud, dir_str, renamed_str)).unwrap();
            assert!(ok2, "rename should succeed: {}", err2);
            assert!(renamed.is_dir());
            assert!(!dir_path.exists());
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn test_lua_remove_dir_recursive() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        let sub = root.join("to_delete");
        std::fs::create_dir(&sub).unwrap();
        std::fs::write(sub.join("child.txt"), "data").unwrap();

        let lua = mlua::Lua::new();
        let api = api_with_root(root);

        lua.scope(|scope| {
            let ud = scope.create_userdata(api).unwrap();
            let remove: mlua::Function = ud.get("remove").unwrap();
            let (ok, err): (bool, String) = remove
                .call((&ud, sub.to_str().unwrap().to_string()))
                .unwrap();
            assert!(ok, "remove dir should succeed: {}", err);
            assert!(!sub.exists());
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn test_lua_scan_dir() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir(root.join("src")).unwrap();
        std::fs::write(root.join("src").join("main.rs"), "fn main(){}").unwrap();
        std::fs::write(root.join("README.md"), "# Hi").unwrap();

        let lua = mlua::Lua::new();
        let api = api_with_root(root);

        lua.scope(|scope| {
            let ud = scope.create_userdata(api).unwrap();
            let scan: mlua::Function = ud.get("scan_dir").unwrap();
            let tbl: mlua::Table = scan
                .call((&ud, root.to_str().unwrap().to_string()))
                .unwrap();

            let len = tbl.len().unwrap();
            assert!(
                len >= 3,
                "Should have src/, src/main.rs, README.md; got {}",
                len
            );
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn test_lua_blocked_outside_root() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        let lua = mlua::Lua::new();
        let api = api_with_root(root);

        lua.scope(|scope| {
            let ud = scope.create_userdata(api).unwrap();

            // create_file outside root
            let create: mlua::Function = ud.get("create_file").unwrap();
            let (ok, _): (bool, String) = create
                .call((&ud, "/tmp/should_not_exist_ferrispad_test.txt".to_string()))
                .unwrap();
            assert!(!ok, "create_file outside root should fail");

            // remove outside root
            let remove: mlua::Function = ud.get("remove").unwrap();
            let (ok2, _): (bool, String) = remove.call((&ud, "/etc/passwd".to_string())).unwrap();
            assert!(!ok2, "remove outside root should fail");
            Ok(())
        })
        .unwrap();
    }

    // ── S1: resolve_and_validate NotFound bypass tests ────────────────

    #[test]
    fn test_resolve_and_validate_notfound_absolute_outside_blocked() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        // Absolute path where parent doesn't exist — should be blocked, not bypassed
        let result = sandbox::resolve_and_validate("/nonexistent_dir_xyz/evil.txt", root).unwrap();
        assert!(
            result.is_none(),
            "Absolute path with nonexistent parent outside root should be None"
        );
    }

    #[test]
    fn test_resolve_and_validate_traversal_nonexistent_parent_blocked() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        // Traversal where intermediate dir doesn't exist
        let result = sandbox::resolve_and_validate("../../nonexistent/file.txt", root).unwrap();
        assert!(
            result.is_none(),
            "Traversal through nonexistent parent should be blocked"
        );
    }

    #[test]
    fn test_resolve_and_validate_notfound_inside_allowed() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        // Create a subdir so the parent exists inside the root
        std::fs::create_dir(root.join("subdir")).unwrap();

        // New file in existing subdir inside root — should be allowed
        let result = sandbox::resolve_and_validate("subdir/new_file.txt", root).unwrap();
        assert!(
            result.is_some(),
            "New file in existing subdir inside root should be allowed"
        );
    }

    // ── S4: file_exists / is_file without project root ────────────────

    #[test]
    fn test_file_exists_no_project_root_returns_false() {
        let lua = mlua::Lua::new();
        let api = EditorApi::default(); // no project_root

        lua.scope(|scope| {
            let ud = scope.create_userdata(api).unwrap();
            let file_exists: mlua::Function = ud.get("file_exists").unwrap();
            let result: bool = file_exists.call((&ud, "/etc/hosts".to_string())).unwrap();
            assert!(
                !result,
                "file_exists without project root should return false"
            );
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn test_is_file_no_project_root_returns_false() {
        let lua = mlua::Lua::new();
        let api = EditorApi::default(); // no project_root

        lua.scope(|scope| {
            let ud = scope.create_userdata(api).unwrap();
            let is_file: mlua::Function = ud.get("is_file").unwrap();
            let result: bool = is_file.call((&ud, "/etc/hosts".to_string())).unwrap();
            assert!(!result, "is_file without project root should return false");
            Ok(())
        })
        .unwrap();
    }

    // ── S2: git_status path validation ────────────────────────────────

    #[test]
    fn test_lua_git_status_outside_root_blocked() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        let lua = mlua::Lua::new();
        let api = api_with_root(root);

        lua.scope(|scope| {
            let ud = scope.create_userdata(api).unwrap();
            let git_status: mlua::Function = ud.get("git_status").unwrap();
            // Attempting to query git status on a path outside project root
            let result: mlua::Value = git_status.call((&ud, "/tmp".to_string())).unwrap();
            assert!(
                matches!(result, mlua::Value::Nil),
                "git_status outside project root should return nil"
            );
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn test_lua_git_status_no_project_root_returns_nil() {
        let lua = mlua::Lua::new();
        let api = EditorApi::default(); // no project_root

        lua.scope(|scope| {
            let ud = scope.create_userdata(api).unwrap();
            let git_status: mlua::Function = ud.get("git_status").unwrap();
            let result: mlua::Value = git_status.call((&ud, "/tmp".to_string())).unwrap();
            assert!(
                matches!(result, mlua::Value::Nil),
                "git_status without project root should return nil"
            );
            Ok(())
        })
        .unwrap();
    }

    // ── A4: read_file tests ─────────────────────────────────────────

    #[test]
    fn test_lua_read_file_success() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        std::fs::write(root.join("hello.txt"), "Hello, world!").unwrap();

        let lua = mlua::Lua::new();
        let api = api_with_root(root);

        lua.scope(|scope| {
            let ud = scope.create_userdata(api).unwrap();
            let read_file: mlua::Function = ud.get("read_file").unwrap();
            let (content, err): (String, mlua::Value) = read_file
                .call((&ud, root.join("hello.txt").to_str().unwrap().to_string()))
                .unwrap();
            assert_eq!(content, "Hello, world!");
            assert!(
                matches!(err, mlua::Value::Nil),
                "error should be nil on success"
            );
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn test_lua_read_file_outside_root_blocked() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        let lua = mlua::Lua::new();
        let api = api_with_root(root);

        lua.scope(|scope| {
            let ud = scope.create_userdata(api).unwrap();
            let read_file: mlua::Function = ud.get("read_file").unwrap();
            let (content, err): (mlua::Value, Option<String>) =
                read_file.call((&ud, "/etc/hosts".to_string())).unwrap();
            assert!(
                matches!(content, mlua::Value::Nil),
                "content should be nil for blocked path"
            );
            assert!(err.is_some(), "should return an error message");
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn test_lua_read_file_no_project_root() {
        let lua = mlua::Lua::new();
        let api = EditorApi::default(); // no project_root

        lua.scope(|scope| {
            let ud = scope.create_userdata(api).unwrap();
            let read_file: mlua::Function = ud.get("read_file").unwrap();
            let (content, err): (mlua::Value, Option<String>) =
                read_file.call((&ud, "/etc/hosts".to_string())).unwrap();
            assert!(
                matches!(content, mlua::Value::Nil),
                "content should be nil without project root"
            );
            assert_eq!(err, Some("No project root".to_string()));
            Ok(())
        })
        .unwrap();
    }
}
