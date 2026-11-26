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
        methods.add_method("setup_mcp_config", editor::setup_mcp_config);

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
        sandbox::scan_dir_recursive(
            root,
            root,
            2,
            1,
            &mut results,
            &std::collections::HashSet::new(),
        );
        let paths: Vec<&str> = results.iter().map(|e| e.rel_path.as_str()).collect();
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
        sandbox::scan_dir_recursive(
            root,
            root,
            5,
            1,
            &mut results,
            &std::collections::HashSet::new(),
        );

        let names: Vec<&str> = results.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"sub"));
        assert!(names.contains(&"hello.txt"));
        assert!(names.contains(&"top.txt"));

        // Check is_dir flag
        let sub_entry = results.iter().find(|e| e.name == "sub").unwrap();
        assert!(sub_entry.is_dir, "sub should be a directory");
        let file_entry = results.iter().find(|e| e.name == "top.txt").unwrap();
        assert!(!file_entry.is_dir, "top.txt should not be a directory");
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

    // Regression (T0002, audit S2): traversal hidden BEHIND a missing directory.
    // Because `missing/` doesn't exist, `canonicalize` fails on every ancestor
    // that contains it, so the old ancestor-walk popped straight up to `root`
    // (which is in-root) and returned the raw joined path with `..` intact.
    // `create_dir_all`/`fs::write` then resolve the `..` against the kernel and
    // materialise a target OUTSIDE the sandbox.
    #[test]
    fn test_resolve_and_validate_traversal_behind_missing_dir_blocked() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        let result = sandbox::resolve_and_validate("missing/../../escaped", root).unwrap();
        assert!(
            result.is_none(),
            "traversal hidden behind a missing dir must be blocked, got {:?}",
            result
        );
    }

    // Positive counterpart: deep not-yet-existing dirs that stay inside the root
    // (the create_dir_all use case) must still be allowed and returned in-root.
    #[test]
    fn test_resolve_and_validate_deep_new_dirs_inside_allowed() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        let result = sandbox::resolve_and_validate("a/b/c.txt", root).unwrap();
        let resolved = result.expect("deep new dirs inside root should be allowed");
        let canonical_root = std::fs::canonicalize(root).unwrap();
        assert!(
            resolved.starts_with(&canonical_root),
            "returned path must stay inside root: {:?}",
            resolved
        );
    }

    // ── T0090: one physical path rule (in-root symlinks) ──────────────

    /// `root/` (the project) and `outside/` (must stay untouched), side by side.
    #[cfg(unix)]
    fn root_and_outside() -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
        let dir = tempdir().unwrap();
        let root = dir.path().join("root");
        let outside = dir.path().join("outside");
        std::fs::create_dir(&root).unwrap();
        std::fs::create_dir(&outside).unwrap();
        (dir, root, outside)
    }

    #[cfg(unix)]
    fn call2(api: EditorApi, method: &str, a: &str, b: &str) -> (bool, String) {
        let lua = mlua::Lua::new();
        lua.scope(|scope| {
            let ud = scope.create_userdata(api).unwrap();
            let f: mlua::Function = ud.get(method).unwrap();
            Ok(
                f.call::<(bool, String)>((&ud, a.to_string(), b.to_string()))
                    .unwrap(),
            )
        })
        .unwrap()
    }

    #[cfg(unix)]
    fn call1(api: EditorApi, method: &str, a: &str) -> (bool, String) {
        let lua = mlua::Lua::new();
        lua.scope(|scope| {
            let ud = scope.create_userdata(api).unwrap();
            let f: mlua::Function = ud.get(method).unwrap();
            Ok(f.call::<(bool, String)>((&ud, a.to_string())).unwrap())
        })
        .unwrap()
    }

    #[cfg(unix)]
    #[test]
    fn plugin_path_write_file_through_in_root_symlink_is_rejected() {
        let (_d, root, outside) = root_and_outside();
        std::os::unix::fs::symlink(&outside, root.join("link")).unwrap();
        let (ok, _) = call2(
            api_with_root(&root),
            "write_file",
            "link/newdir/pwned.txt",
            "x",
        );
        assert!(
            !outside.join("newdir").exists(),
            "write_file escaped through root/link"
        );
        assert!(!ok, "write_file through an in-root symlink must fail");
    }

    #[cfg(unix)]
    #[test]
    fn plugin_path_create_dir_through_in_root_symlink_is_rejected() {
        let (_d, root, outside) = root_and_outside();
        std::os::unix::fs::symlink(&outside, root.join("link")).unwrap();
        let (ok, _) = call1(api_with_root(&root), "create_dir", "link/a/b");
        assert!(
            !outside.join("a").exists(),
            "create_dir escaped through root/link"
        );
        assert!(!ok, "create_dir through an in-root symlink must fail");
    }

    #[cfg(unix)]
    #[test]
    fn plugin_path_dangling_symlink_is_not_written_through() {
        let (_d, root, outside) = root_and_outside();
        let target = outside.join("new.txt");
        std::os::unix::fs::symlink(&target, root.join("dangle")).unwrap();
        let (ok_write, _) = call2(api_with_root(&root), "write_file", "dangle", "x");
        assert!(
            !target.exists(),
            "write_file followed a dangling in-root symlink"
        );
        assert!(!ok_write);
        let (ok_create, _) = call1(api_with_root(&root), "create_file", "dangle");
        assert!(
            !target.exists(),
            "create_file followed a dangling in-root symlink"
        );
        assert!(!ok_create);
    }

    #[cfg(unix)]
    #[test]
    fn plugin_path_mcp_config_does_not_follow_symlinks() {
        // A dangling .mcp.json link pointing outside.
        let (_d, root, outside) = root_and_outside();
        let target = outside.join("mcp.json");
        std::os::unix::fs::symlink(&target, root.join(".mcp.json")).unwrap();
        let canonical_root = std::fs::canonicalize(&root).unwrap();
        let res = editor::write_mcp_files(&canonical_root, &root, "/bin/ferrispad");
        assert!(!target.exists(), ".mcp.json written through a symlink");
        assert!(res.is_err());

        // A .gitignore link to an outside file.
        let (_d2, root2, outside2) = root_and_outside();
        let victim = outside2.join("victim");
        std::fs::write(&victim, "keep\n").unwrap();
        std::os::unix::fs::symlink(&victim, root2.join(".gitignore")).unwrap();
        let canonical_root2 = std::fs::canonicalize(&root2).unwrap();
        let res2 = editor::write_mcp_files(&canonical_root2, &root2, "/bin/ferrispad");
        assert_eq!(
            std::fs::read_to_string(&victim).unwrap(),
            "keep\n",
            ".gitignore written through a symlink"
        );
        assert!(res2.is_err());
    }

    /// The macOS `/var` → `/private/var` case: the root is given through an
    /// alias symlink and the plugin passes absolute paths under that alias.
    #[cfg(unix)]
    #[test]
    fn plugin_path_absolute_path_under_root_alias_is_accepted() {
        let (d, real_root, _outside) = root_and_outside();
        let alias = d.path().join("alias");
        std::os::unix::fs::symlink(&real_root, &alias).unwrap();
        let deep = alias.join("nested").join("deep");
        let (ok, err) = call1(api_with_root(&alias), "create_dir", deep.to_str().unwrap());
        assert!(ok, "create_dir under the root alias must succeed: {err}");
        assert!(real_root.join("nested/deep").is_dir());
        let renamed = alias.join("nested").join("renamed");
        let (ok2, err2) = call2(
            api_with_root(&alias),
            "rename",
            deep.to_str().unwrap(),
            renamed.to_str().unwrap(),
        );
        assert!(ok2, "rename under the root alias must succeed: {err2}");
        assert!(real_root.join("nested/renamed").is_dir());
    }

    // ── T0092: no writes into executables or under .git ───────────────

    #[cfg(unix)]
    fn make_executable(path: &std::path::Path, content: &str) {
        use std::os::unix::fs::PermissionsExt;
        std::fs::write(path, content).unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    /// `fs::write` onto an existing file keeps its mode: overwriting an
    /// executable (e.g. `.venv/bin/ruff`) and then running it through an
    /// approved command would be code execution.
    #[cfg(unix)]
    #[test]
    fn plugin_write_refuses_existing_executables() {
        let (_d, root, _outside) = root_and_outside();
        std::fs::create_dir_all(root.join(".venv/bin")).unwrap();
        let ruff = root.join(".venv/bin/ruff");
        make_executable(&ruff, "#!/bin/sh\necho real ruff\n");
        let (ok, _) = call2(
            api_with_root(&root),
            "write_file",
            ".venv/bin/ruff",
            "#!/bin/sh\nevil\n",
        );
        assert_eq!(
            std::fs::read_to_string(&ruff).unwrap(),
            "#!/bin/sh\necho real ruff\n"
        );
        assert!(!ok, "write_file overwrote an executable");

        // Through an in-root symlink to the executable: same physical target.
        std::os::unix::fs::symlink(&ruff, root.join("alias")).unwrap();
        let (ok, _) = call2(api_with_root(&root), "write_file", "alias", "evil");
        assert!(!ok, "write_file overwrote an executable through a symlink");

        // rename INTO an existing executable is refused…
        std::fs::write(root.join("payload"), "evil").unwrap();
        let (ok, _) = call2(api_with_root(&root), "rename", "payload", ".venv/bin/ruff");
        assert!(!ok, "rename replaced an executable");
        assert_eq!(
            std::fs::read_to_string(&ruff).unwrap(),
            "#!/bin/sh\necho real ruff\n"
        );
    }

    /// Moving an executable away, removing it, and writing a plain new file
    /// stay allowed: the new file is not executable.
    #[cfg(unix)]
    #[test]
    fn plugin_write_allows_moving_or_removing_executables() {
        let (_d, root, _outside) = root_and_outside();
        let tool = root.join("tool.sh");
        make_executable(&tool, "#!/bin/sh\n");
        let (ok, err) = call2(api_with_root(&root), "rename", "tool.sh", "old-tool.sh");
        assert!(ok, "moving an executable away: {err}");
        let (ok, err) = call2(api_with_root(&root), "write_file", "tool.sh", "plain");
        assert!(ok, "a new file where the executable was: {err}");
        let (ok, err) = call1(api_with_root(&root), "remove", "old-tool.sh");
        assert!(ok, "removing an executable: {err}");
    }

    /// git-diff has execute = ["git"]: a plugin that writes `.git/config`
    /// (core.fsmonitor, core.pager, aliases) or a hook runs code through it.
    #[cfg(unix)]
    #[test]
    fn plugin_write_refuses_paths_under_dot_git() {
        let (_d, root, _outside) = root_and_outside();
        std::fs::create_dir_all(root.join(".git/hooks")).unwrap();
        std::fs::write(root.join(".git/config"), "[core]\n").unwrap();
        std::fs::write(root.join(".git/hooks/pre-commit.sample"), "x").unwrap();
        std::fs::create_dir(root.join("sub")).unwrap();
        std::os::unix::fs::symlink(root.join(".git"), root.join("gitlink")).unwrap();

        for (method, args) in [
            (
                "write_file",
                vec![".git/config", "[core]\nfsmonitor = evil\n"],
            ),
            ("write_file", vec!["sub/../.git/config", "x"]),
            ("write_file", vec!["gitlink/config", "x"]),
            (
                "write_file",
                vec![".git/hooks/post-checkout", "#!/bin/sh\n"],
            ),
            (
                "rename",
                vec![".git/hooks/pre-commit.sample", ".git/hooks/pre-commit"],
            ),
            ("rename", vec!["sub", ".git/sub"]),
        ] {
            let (ok, _) = call2(api_with_root(&root), method, args[0], args[1]);
            assert!(!ok, "{method}{args:?} under .git must fail");
        }
        for (method, arg) in [
            ("create_dir", ".git/hooks/x"),
            ("create_file", ".git/hooks/pre-push"),
            ("remove", ".git/hooks/pre-commit.sample"),
        ] {
            let (ok, _) = call1(api_with_root(&root), method, arg);
            assert!(!ok, "{method}({arg}) under .git must fail");
        }
        assert_eq!(
            std::fs::read_to_string(root.join(".git/config")).unwrap(),
            "[core]\n"
        );
        assert!(root.join(".git/hooks/pre-commit.sample").exists());
        assert!(!root.join(".git/hooks/post-checkout").exists());
        assert!(!root.join(".git/hooks/pre-commit").exists());

        // Reads stay allowed; look-alikes are not .git.
        let lua = mlua::Lua::new();
        lua.scope(|scope| {
            let ud = scope.create_userdata(api_with_root(&root)).unwrap();
            let read: mlua::Function = ud.get("read_file").unwrap();
            let (content, err): (Option<String>, Option<String>) =
                read.call((&ud, ".git/config".to_string())).unwrap();
            assert_eq!(content.as_deref(), Some("[core]\n"), "{err:?}");
            Ok(())
        })
        .unwrap();
        for ok_path in [".github/workflows/ci.yml", "x.git/file", "docs/.gitkeep"] {
            let (ok, err) = call2(api_with_root(&root), "write_file", ok_path, "x");
            assert!(ok, "{ok_path}: {err}");
        }
    }

    /// `.git` as a FILE (worktrees, submodules) is covered too.
    #[cfg(unix)]
    #[test]
    fn plugin_write_refuses_dot_git_file() {
        let (_d, root, _outside) = root_and_outside();
        std::fs::write(root.join(".git"), "gitdir: /elsewhere\n").unwrap();
        let (ok, _) = call2(
            api_with_root(&root),
            "write_file",
            ".git",
            "gitdir: /evil\n",
        );
        assert!(!ok);
        assert_eq!(
            std::fs::read_to_string(root.join(".git")).unwrap(),
            "gitdir: /elsewhere\n"
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
