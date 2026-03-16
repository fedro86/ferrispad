#![allow(dead_code)]

use std::fs;
use std::path::Path;

/// Create a plugin directory with init.lua and plugin.toml.
pub fn create_plugin_dir(base: &Path, name: &str, init_lua: &str, plugin_toml: &str) {
    let dir = base.join(name);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("init.lua"), init_lua).unwrap();
    fs::write(dir.join("plugin.toml"), plugin_toml).unwrap();
}

/// Returns a standard plugin.toml template.
pub fn default_plugin_toml(name: &str, version: &str) -> String {
    format!(
        r#"name = "{name}"
version = "{version}"
description = "Test plugin: {name}"
"#
    )
}

/// Returns a SessionData JSON string with multiple documents and groups.
pub fn sample_session_json() -> String {
    r#"{
        "version": 1,
        "active_index": 1,
        "documents": [
            {
                "file_path": "/tmp/hello.rs",
                "display_name": "hello.rs",
                "cursor_position": 42,
                "temp_file": null,
                "was_dirty": false,
                "group_index": 0
            },
            {
                "file_path": null,
                "display_name": "Untitled",
                "cursor_position": 0,
                "temp_file": "abc123.tmp",
                "was_dirty": true,
                "group_index": null
            },
            {
                "file_path": "/home/user/notes.md",
                "display_name": "notes.md",
                "cursor_position": 100,
                "temp_file": null,
                "was_dirty": false,
                "group_index": 1
            }
        ],
        "last_open_directory": "/tmp",
        "groups": [
            {
                "name": "Backend",
                "color": "coral",
                "collapsed": false
            },
            {
                "name": "Docs",
                "color": "sky",
                "collapsed": true
            }
        ],
        "instance_id": "12345"
    }"#
    .to_string()
}
