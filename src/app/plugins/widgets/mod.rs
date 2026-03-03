//! Widget API for plugins.
//!
//! Provides a way for plugins to create and manage UI widgets such as:
//! - Split views for diffs and suggestions
//! - Tree views for file browsers and YAML viewers
//!
//! ## Architecture
//!
//! Plugins return widget requests in their HookResult. The WidgetManager
//! creates and tracks widget sessions. User interactions with widgets
//! are sent back to plugins via the OnWidgetAction hook.

pub mod split_view;
pub mod tree_view;

pub use split_view::{HighlightColor, IntralineSpan, LineHighlight, SplitDisplayMode, SplitPane, SplitViewAction, SplitViewRequest};
pub use tree_view::{ContextMenuItem, ContextMenuTarget, TreeClickMode, TreeNode, TreeViewRequest};

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};

/// Global session ID counter
static NEXT_SESSION_ID: AtomicU32 = AtomicU32::new(1);

/// Generate a unique session ID for a widget
fn next_session_id() -> u32 {
    NEXT_SESSION_ID.fetch_add(1, Ordering::SeqCst)
}

/// A widget session tracks an active widget created by a plugin
#[derive(Debug, Clone)]
pub struct WidgetSession {
    /// Unique session ID
    #[allow(dead_code)]  // Used for session identification in future features
    pub id: u32,
    /// Name of the plugin that created this widget
    pub plugin_name: String,
    /// Type of widget
    #[allow(dead_code)]  // Used for widget-type-specific behavior in future
    pub widget_type: WidgetType,
}

/// Type of widget being managed
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WidgetType {
    SplitView,
    TreeView,
}

impl WidgetType {
    /// Get the type name as a string for Lua callbacks
    #[allow(dead_code)]  // Used by plugins when reporting widget type
    pub fn as_str(&self) -> &'static str {
        match self {
            WidgetType::SplitView => "split_view",
            WidgetType::TreeView => "tree_view",
        }
    }
}

/// Manages widget sessions created by plugins
#[derive(Debug, Default)]
pub struct WidgetManager {
    /// Active sessions by ID
    sessions: HashMap<u32, WidgetSession>,
}

impl WidgetManager {
    /// Create a new widget manager
    pub fn new() -> Self {
        Self {
            sessions: HashMap::new(),
        }
    }

    /// Create a new split view session
    pub fn create_split_view_session(&mut self, plugin_name: &str) -> u32 {
        let id = next_session_id();
        self.sessions.insert(
            id,
            WidgetSession {
                id,
                plugin_name: plugin_name.to_string(),
                widget_type: WidgetType::SplitView,
            },
        );
        id
    }

    /// Create a new tree view session
    pub fn create_tree_view_session(&mut self, plugin_name: &str) -> u32 {
        let id = next_session_id();
        self.sessions.insert(
            id,
            WidgetSession {
                id,
                plugin_name: plugin_name.to_string(),
                widget_type: WidgetType::TreeView,
            },
        );
        id
    }

    /// Get a session by ID
    pub fn get_session(&self, id: u32) -> Option<&WidgetSession> {
        self.sessions.get(&id)
    }

    /// Remove a session
    pub fn remove_session(&mut self, id: u32) -> Option<WidgetSession> {
        self.sessions.remove(&id)
    }

    /// Check if a session exists
    #[allow(dead_code)]
    pub fn has_session(&self, id: u32) -> bool {
        self.sessions.contains_key(&id)
    }

    /// Get all active sessions for a plugin
    #[allow(dead_code)]
    pub fn sessions_for_plugin(&self, plugin_name: &str) -> Vec<&WidgetSession> {
        self.sessions
            .values()
            .filter(|s| s.plugin_name == plugin_name)
            .collect()
    }

    /// Remove all sessions for a plugin (called when plugin is unloaded)
    #[allow(dead_code)]
    pub fn clear_plugin_sessions(&mut self, plugin_name: &str) {
        self.sessions
            .retain(|_, s| s.plugin_name != plugin_name);
    }

    /// Find any active tree view session and return its ID.
    /// Used for toggle behavior (show/hide on repeated menu action).
    pub fn any_tree_view_session(&self) -> Option<u32> {
        self.sessions.values()
            .find(|s| s.widget_type == WidgetType::TreeView)
            .map(|s| s.id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_id_generation() {
        let id1 = next_session_id();
        let id2 = next_session_id();
        assert!(id2 > id1);
    }

    #[test]
    fn test_widget_manager() {
        let mut manager = WidgetManager::new();

        let split_id = manager.create_split_view_session("test-plugin");
        let tree_id = manager.create_tree_view_session("test-plugin");

        assert!(manager.has_session(split_id));
        assert!(manager.has_session(tree_id));

        let session = manager.get_session(split_id).unwrap();
        assert_eq!(session.plugin_name, "test-plugin");
        assert_eq!(session.widget_type, WidgetType::SplitView);

        let sessions = manager.sessions_for_plugin("test-plugin");
        assert_eq!(sessions.len(), 2);

        manager.remove_session(split_id);
        assert!(!manager.has_session(split_id));
        assert!(manager.has_session(tree_id));
    }

    #[test]
    fn test_clear_plugin_sessions() {
        let mut manager = WidgetManager::new();

        manager.create_split_view_session("plugin-a");
        manager.create_tree_view_session("plugin-a");
        manager.create_split_view_session("plugin-b");

        manager.clear_plugin_sessions("plugin-a");

        assert_eq!(manager.sessions_for_plugin("plugin-a").len(), 0);
        assert_eq!(manager.sessions_for_plugin("plugin-b").len(), 1);
    }
}
