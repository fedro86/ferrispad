mod common;

use ferris_pad::app::services::session::{DocumentSession, GroupSession, SessionData};
use std::fs;
use tempfile::tempdir;

#[test]
fn test_session_roundtrip_via_disk() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("session.json");

    let data = SessionData {
        version: 1,
        active_index: 2,
        documents: vec![
            DocumentSession {
                file_path: Some("/tmp/test.rs".to_string()),
                display_name: "test.rs".to_string(),
                cursor_position: 55,
                temp_file: None,
                was_dirty: false,
                group_index: Some(0),
            },
            DocumentSession {
                file_path: None,
                display_name: "Untitled".to_string(),
                cursor_position: 0,
                temp_file: Some("deadbeef.tmp".to_string()),
                was_dirty: true,
                group_index: None,
            },
        ],
        last_open_directory: Some("/home/user".to_string()),
        groups: vec![],
        instance_id: Some("9999".to_string()),
    };

    let json = serde_json::to_string_pretty(&data).unwrap();
    fs::write(&path, &json).unwrap();

    let loaded_json = fs::read_to_string(&path).unwrap();
    let loaded: SessionData = serde_json::from_str(&loaded_json).unwrap();

    assert_eq!(loaded.version, 1);
    assert_eq!(loaded.active_index, 2);
    assert_eq!(loaded.documents.len(), 2);
    assert_eq!(loaded.documents[0].file_path.as_deref(), Some("/tmp/test.rs"));
    assert_eq!(loaded.documents[0].cursor_position, 55);
    assert_eq!(loaded.documents[0].group_index, Some(0));
    assert_eq!(loaded.documents[1].display_name, "Untitled");
    assert!(loaded.documents[1].was_dirty);
    assert_eq!(loaded.documents[1].temp_file.as_deref(), Some("deadbeef.tmp"));
    assert_eq!(loaded.last_open_directory.as_deref(), Some("/home/user"));
    assert_eq!(loaded.instance_id.as_deref(), Some("9999"));
}

#[test]
fn test_session_with_groups_roundtrip() {
    let data = SessionData {
        version: 1,
        active_index: 0,
        documents: vec![DocumentSession {
            file_path: Some("/a.txt".to_string()),
            display_name: "a.txt".to_string(),
            cursor_position: 0,
            temp_file: None,
            was_dirty: false,
            group_index: Some(1),
        }],
        last_open_directory: None,
        groups: vec![
            GroupSession {
                name: "Frontend".to_string(),
                color: "coral".to_string(),
                collapsed: false,
            },
            GroupSession {
                name: "Backend".to_string(),
                color: "sky".to_string(),
                collapsed: true,
            },
            GroupSession {
                name: "Tests".to_string(),
                color: "mint".to_string(),
                collapsed: false,
            },
        ],
        instance_id: None,
    };

    let json = serde_json::to_string(&data).unwrap();
    let loaded: SessionData = serde_json::from_str(&json).unwrap();

    assert_eq!(loaded.groups.len(), 3);
    assert_eq!(loaded.groups[0].name, "Frontend");
    assert_eq!(loaded.groups[0].color, "coral");
    assert!(!loaded.groups[0].collapsed);
    assert_eq!(loaded.groups[1].name, "Backend");
    assert!(loaded.groups[1].collapsed);
    assert_eq!(loaded.groups[2].color, "mint");
}

#[test]
fn test_session_backward_compat() {
    // Old format missing optional fields
    let json = r#"{
        "active_index": 0,
        "documents": [
            {
                "file_path": "/tmp/x.txt",
                "display_name": "x.txt",
                "cursor_position": 10,
                "temp_file": null,
                "was_dirty": false
            }
        ]
    }"#;

    let loaded: SessionData = serde_json::from_str(json).unwrap();
    assert_eq!(loaded.version, 1); // default_version
    assert!(loaded.last_open_directory.is_none());
    assert!(loaded.groups.is_empty());
    assert!(loaded.instance_id.is_none());
    assert!(loaded.documents[0].group_index.is_none());
}

#[test]
fn test_session_unicode_paths() {
    let data = SessionData {
        version: 1,
        active_index: 0,
        documents: vec![
            DocumentSession {
                file_path: Some("/home/user/\u{6587}\u{4EF6}.txt".to_string()),
                display_name: "\u{6587}\u{4EF6}.txt".to_string(),
                cursor_position: 0,
                temp_file: None,
                was_dirty: false,
                group_index: None,
            },
            DocumentSession {
                file_path: Some("/tmp/\u{1F600}emoji.md".to_string()),
                display_name: "\u{1F600}emoji.md".to_string(),
                cursor_position: 5,
                temp_file: None,
                was_dirty: false,
                group_index: None,
            },
        ],
        last_open_directory: Some("/home/user/\u{6587}\u{4EF6}\u{5939}".to_string()),
        groups: vec![],
        instance_id: None,
    };

    let json = serde_json::to_string(&data).unwrap();
    let loaded: SessionData = serde_json::from_str(&json).unwrap();

    assert_eq!(loaded.documents[0].display_name, "\u{6587}\u{4EF6}.txt");
    assert!(loaded.documents[1].file_path.as_ref().unwrap().contains("\u{1F600}"));
    assert!(loaded.last_open_directory.as_ref().unwrap().contains("\u{6587}\u{4EF6}\u{5939}"));
}
