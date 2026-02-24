//! Plugin hook definitions.
//!
//! All hooks are synchronous and blocking - they fire, execute, and return.
//! No async, no background threads. This ensures 0% CPU when idle.

use super::annotations::LineAnnotation;
use crate::ui::toast::ToastLevel;

/// Diagnostic severity level
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DiagnosticLevel {
    /// Error - highest priority (red)
    Error = 0,
    /// Warning - medium priority (yellow/orange)
    Warning = 1,
    /// Info - low priority (blue)
    Info = 2,
    /// Hint - lowest priority (gray)
    Hint = 3,
}

impl DiagnosticLevel {
    /// Parse from Lua string
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "error" => Self::Error,
            "warning" | "warn" => Self::Warning,
            "info" => Self::Info,
            "hint" => Self::Hint,
            _ => Self::Info,
        }
    }
}

/// A single diagnostic message from a plugin
#[derive(Debug, Clone)]
pub struct Diagnostic {
    /// Line number (1-indexed)
    pub line: u32,
    /// Column number (1-indexed, optional)
    pub column: Option<u32>,
    /// Diagnostic message
    pub message: String,
    /// Severity level
    pub level: DiagnosticLevel,
    /// Source plugin name
    pub source: String,
    /// Optional fix suggestion (e.g., "Organize imports")
    pub fix_message: Option<String>,
    /// Optional documentation URL
    pub url: Option<String>,
}

/// Plugin hooks that can be registered and called.
#[allow(dead_code)]  // OnTextChanged reserved for future use
#[derive(Debug, Clone)]
pub enum PluginHook {
    /// Called once when plugin loads
    Init,

    /// Called when application is shutting down
    Shutdown,

    /// Called after a document is opened
    OnDocumentOpen {
        path: Option<String>,
    },

    /// Called before a document is saved.
    /// Plugin can return modified content.
    OnDocumentSave {
        path: String,
        content: String,
    },

    /// Called when a document is closed
    OnDocumentClose {
        path: Option<String>,
    },

    /// Called after text is changed in the editor
    OnTextChanged {
        position: i32,
        inserted_len: i32,
        deleted_len: i32,
    },

    /// Called when theme changes between light/dark
    OnThemeChanged {
        is_dark: bool,
    },

    /// Called after save to lint/check the document.
    /// Plugins return a list of diagnostics and optional line annotations.
    OnDocumentLint {
        path: String,
        content: String,
    },

    /// Called on manual highlight request (Ctrl+Shift+L).
    /// Plugins return line annotations for highlighting.
    OnHighlightRequest {
        path: Option<String>,
        content: String,
    },

    /// Called when user triggers a plugin's custom menu action.
    /// The action name comes from plugin.toml [[menu_items]].
    /// Plugin can return diagnostics, modified content, or status message.
    OnMenuAction {
        action: String,
        path: Option<String>,
        content: String,
    },
}

impl PluginHook {
    /// Get the Lua function name for this hook
    pub fn lua_name(&self) -> &'static str {
        match self {
            Self::Init => "init",
            Self::Shutdown => "shutdown",
            Self::OnDocumentOpen { .. } => "on_document_open",
            Self::OnDocumentSave { .. } => "on_document_save",
            Self::OnDocumentClose { .. } => "on_document_close",
            Self::OnTextChanged { .. } => "on_text_changed",
            Self::OnThemeChanged { .. } => "on_theme_changed",
            Self::OnDocumentLint { .. } => "on_document_lint",
            Self::OnHighlightRequest { .. } => "on_highlight_request",
            Self::OnMenuAction { .. } => "on_menu_action",
        }
    }
}

/// A status message to display to the user
#[derive(Debug, Clone)]
pub struct StatusMessage {
    /// Toast level (determines color)
    pub level: ToastLevel,
    /// Message text
    pub text: String,
}

/// Result from calling plugin hooks
#[derive(Debug, Default)]
pub struct HookResult {
    /// Modified content (for OnDocumentSave hook)
    pub modified_content: Option<String>,
    /// Diagnostics from lint hooks
    pub diagnostics: Vec<Diagnostic>,
    /// Line annotations for gutter/inline highlighting
    pub line_annotations: Vec<LineAnnotation>,
    /// Status message to show in toast
    pub status_message: Option<StatusMessage>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diagnostic_level_from_str() {
        assert_eq!(DiagnosticLevel::from_str("error"), DiagnosticLevel::Error);
        assert_eq!(DiagnosticLevel::from_str("ERROR"), DiagnosticLevel::Error);
        assert_eq!(DiagnosticLevel::from_str("warning"), DiagnosticLevel::Warning);
        assert_eq!(DiagnosticLevel::from_str("warn"), DiagnosticLevel::Warning);
        assert_eq!(DiagnosticLevel::from_str("info"), DiagnosticLevel::Info);
        assert_eq!(DiagnosticLevel::from_str("hint"), DiagnosticLevel::Hint);
        assert_eq!(DiagnosticLevel::from_str("unknown"), DiagnosticLevel::Info);
    }

    #[test]
    fn test_diagnostic_level_ordering() {
        // Errors should be most severe (lowest value)
        assert!(DiagnosticLevel::Error < DiagnosticLevel::Warning);
        assert!(DiagnosticLevel::Warning < DiagnosticLevel::Info);
        assert!(DiagnosticLevel::Info < DiagnosticLevel::Hint);
    }

    #[test]
    fn test_hook_result_default() {
        let result = HookResult::default();
        assert!(result.modified_content.is_none());
        assert!(result.diagnostics.is_empty());
    }

    #[test]
    fn test_plugin_hook_lua_names() {
        assert_eq!(PluginHook::Init.lua_name(), "init");
        assert_eq!(PluginHook::Shutdown.lua_name(), "shutdown");
        assert_eq!(
            PluginHook::OnDocumentLint {
                path: String::new(),
                content: String::new()
            }
            .lua_name(),
            "on_document_lint"
        );
    }
}
