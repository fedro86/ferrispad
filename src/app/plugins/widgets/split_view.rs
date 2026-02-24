//! Split view widget types for plugin API.
//!
//! Allows plugins to display side-by-side content views, useful for:
//! - Git diffs (original vs modified)
//! - AI suggestions (current vs suggested)
//! - File comparisons

use serde::{Deserialize, Serialize};

/// A request to show a split view, returned from plugin hooks
#[derive(Debug, Clone, Default)]
pub struct SplitViewRequest {
    /// Title shown in the split view header
    pub title: String,
    /// Left pane content and settings
    pub left: SplitPane,
    /// Right pane content and settings
    pub right: SplitPane,
    /// Action buttons to show (e.g., Accept, Reject)
    pub actions: Vec<SplitViewAction>,
}

/// Content and settings for one pane of a split view
#[derive(Debug, Clone, Default)]
pub struct SplitPane {
    /// Text content to display
    pub content: String,
    /// Label shown above the pane
    pub label: String,
    /// Whether to show line numbers
    pub line_numbers: bool,
    /// Whether the pane is read-only (default true)
    #[allow(dead_code)]  // Used when split view allows editing
    pub read_only: bool,
    /// Line highlights (e.g., for diff coloring)
    pub highlights: Vec<LineHighlight>,
}

/// A line highlight for diff visualization
#[derive(Debug, Clone)]
pub struct LineHighlight {
    /// Line number (1-indexed)
    pub line: u32,
    /// Highlight color
    pub color: HighlightColor,
}

/// Colors for diff highlighting
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HighlightColor {
    /// Added line (green)
    Added,
    /// Removed line (red)
    Removed,
    /// Modified line (yellow)
    Modified,
    /// Custom RGB color
    Rgb(u8, u8, u8),
}

impl HighlightColor {
    /// Parse from Lua string
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "green" | "added" | "add" => Some(Self::Added),
            "red" | "removed" | "delete" | "deleted" => Some(Self::Removed),
            "yellow" | "modified" | "change" | "changed" => Some(Self::Modified),
            _ => None,
        }
    }

    /// Get RGB values for this color
    pub fn to_rgb(&self) -> (u8, u8, u8) {
        match self {
            Self::Added => (200, 255, 200),    // Light green
            Self::Removed => (255, 200, 200),  // Light red
            Self::Modified => (255, 255, 200), // Light yellow
            Self::Rgb(r, g, b) => (*r, *g, *b),
        }
    }

    /// Get RGB values for dark mode
    pub fn to_rgb_dark(&self) -> (u8, u8, u8) {
        match self {
            Self::Added => (40, 80, 40),       // Dark green
            Self::Removed => (80, 40, 40),     // Dark red
            Self::Modified => (80, 80, 40),    // Dark yellow
            Self::Rgb(r, g, b) => (*r / 3, *g / 3, *b / 3),
        }
    }
}

/// An action button in the split view
#[derive(Debug, Clone)]
pub struct SplitViewAction {
    /// Button label
    pub label: String,
    /// Action name sent back to plugin on click
    pub action: String,
}

impl SplitViewRequest {
    /// Parse a split view request from a Lua table
    pub fn from_lua_table(table: &mlua::Table) -> Option<Self> {
        let title: String = table.get("title").unwrap_or_default();

        // Parse left pane
        let left = if let Ok(mlua::Value::Table(left_table)) = table.get::<mlua::Value>("left") {
            SplitPane::from_lua_table(&left_table)
        } else {
            SplitPane::default()
        };

        // Parse right pane
        let right = if let Ok(mlua::Value::Table(right_table)) = table.get::<mlua::Value>("right") {
            SplitPane::from_lua_table(&right_table)
        } else {
            SplitPane::default()
        };

        // Parse actions
        let actions = if let Ok(mlua::Value::Table(actions_table)) = table.get::<mlua::Value>("actions") {
            actions_table
                .pairs::<i32, mlua::Table>()
                .flatten()
                .filter_map(|(_, action_table)| SplitViewAction::from_lua_table(&action_table))
                .collect()
        } else {
            Vec::new()
        };

        Some(Self {
            title,
            left,
            right,
            actions,
        })
    }

    /// Check if this is a valid request (has content in at least one pane)
    pub fn is_valid(&self) -> bool {
        !self.left.content.is_empty() || !self.right.content.is_empty()
    }
}

impl SplitPane {
    /// Parse a split pane from a Lua table
    pub fn from_lua_table(table: &mlua::Table) -> Self {
        let content: String = table.get("content").unwrap_or_default();
        let label: String = table.get("label").unwrap_or_default();
        let line_numbers: bool = table.get("line_numbers").unwrap_or(true);
        let read_only: bool = table.get("read_only").unwrap_or(true);

        // Parse highlights
        let highlights = if let Ok(mlua::Value::Table(highlights_table)) = table.get::<mlua::Value>("highlights") {
            highlights_table
                .pairs::<i32, mlua::Table>()
                .flatten()
                .filter_map(|(_, hl_table)| LineHighlight::from_lua_table(&hl_table))
                .collect()
        } else {
            Vec::new()
        };

        Self {
            content,
            label,
            line_numbers,
            read_only,
            highlights,
        }
    }
}

impl LineHighlight {
    /// Parse a line highlight from a Lua table
    pub fn from_lua_table(table: &mlua::Table) -> Option<Self> {
        let line: u32 = table.get("line").ok()?;

        // Parse color - try string first, then RGB table
        let color = if let Ok(color_str) = table.get::<String>("color") {
            HighlightColor::from_str(&color_str)?
        } else if let Ok(mlua::Value::Table(color_table)) = table.get::<mlua::Value>("color") {
            let r: u8 = color_table.get("r").unwrap_or(0);
            let g: u8 = color_table.get("g").unwrap_or(0);
            let b: u8 = color_table.get("b").unwrap_or(0);
            HighlightColor::Rgb(r, g, b)
        } else {
            return None;
        };

        Some(Self { line, color })
    }
}

impl SplitViewAction {
    /// Parse an action from a Lua table
    pub fn from_lua_table(table: &mlua::Table) -> Option<Self> {
        let label: String = table.get("label").ok()?;
        let action: String = table.get("action").ok()?;
        Some(Self { label, action })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_highlight_color_from_str() {
        assert_eq!(HighlightColor::from_str("green"), Some(HighlightColor::Added));
        assert_eq!(HighlightColor::from_str("added"), Some(HighlightColor::Added));
        assert_eq!(HighlightColor::from_str("red"), Some(HighlightColor::Removed));
        assert_eq!(HighlightColor::from_str("yellow"), Some(HighlightColor::Modified));
        assert_eq!(HighlightColor::from_str("unknown"), None);
    }

    #[test]
    fn test_highlight_color_to_rgb() {
        let added = HighlightColor::Added;
        let (r, g, b) = added.to_rgb();
        assert!(g > r && g > b); // Green is dominant

        let removed = HighlightColor::Removed;
        let (r, g, b) = removed.to_rgb();
        assert!(r > g && r > b); // Red is dominant
    }

    #[test]
    fn test_split_pane_default() {
        let pane = SplitPane::default();
        assert!(pane.content.is_empty());
        assert!(pane.label.is_empty());
        assert!(!pane.line_numbers);
        assert!(!pane.read_only);
    }

    #[test]
    fn test_split_view_request_validity() {
        let mut request = SplitViewRequest::default();
        assert!(!request.is_valid());

        request.left.content = "some content".to_string();
        assert!(request.is_valid());
    }
}
