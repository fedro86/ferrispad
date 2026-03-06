//! Line annotation types for plugin highlighting API.
//!
//! Plugins can return line annotations to highlight lines in the editor:
//! - Gutter marks: full-line background colors
//! - Inline highlights: background colors for specific text ranges

/// Semantic colors that adapt to light/dark theme
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnnotationColor {
    /// Error - red
    Error,
    /// Warning - yellow/orange
    Warning,
    /// Info - blue
    Info,
    /// Hint - gray
    Hint,
    /// Git: added line - green
    Added,
    /// Git: modified line - yellow
    Modified,
    /// Git: deleted line - red
    Deleted,
    /// Custom RGB color
    Rgb(u8, u8, u8),
}

impl AnnotationColor {
    /// Parse from Lua string or return None for invalid input
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "error" => Some(Self::Error),
            "warning" | "warn" => Some(Self::Warning),
            "info" => Some(Self::Info),
            "hint" => Some(Self::Hint),
            "added" | "add" => Some(Self::Added),
            "modified" | "mod" | "changed" => Some(Self::Modified),
            "deleted" | "del" | "removed" => Some(Self::Deleted),
            _ => None,
        }
    }

    /// Get priority for this color (lower = higher priority, shown on top)
    /// Error/Deleted are highest priority, then Warning/Modified, then Info/Added, then Hint
    pub fn priority(&self) -> u8 {
        match self {
            Self::Error | Self::Deleted => 0,
            Self::Warning | Self::Modified => 1,
            Self::Info | Self::Added => 2,
            Self::Hint => 3,
            Self::Rgb(_, _, _) => 4, // Custom colors have lowest priority
        }
    }
}

/// Gutter mark - full-line background color
#[derive(Debug, Clone)]
pub struct GutterMark {
    /// Color for the full line background
    pub color: AnnotationColor,
}

/// Inline highlight - background color for a character range
#[derive(Debug, Clone)]
pub struct InlineHighlight {
    /// Start column (1-indexed, inclusive)
    pub start_col: u32,
    /// End column (1-indexed, exclusive). None means end of line.
    pub end_col: Option<u32>,
    /// Background color for the highlight
    pub color: AnnotationColor,
}

/// A line annotation combining gutter and inline highlights
#[derive(Debug, Clone)]
pub struct LineAnnotation {
    /// Line number (1-indexed)
    pub line: u32,
    /// Optional gutter mark for this line (full-line background)
    pub gutter: Option<GutterMark>,
    /// Inline highlights for this line (can have multiple)
    pub inline: Vec<InlineHighlight>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_annotation_color_from_str() {
        assert_eq!(AnnotationColor::from_str("error"), Some(AnnotationColor::Error));
        assert_eq!(AnnotationColor::from_str("ERROR"), Some(AnnotationColor::Error));
        assert_eq!(AnnotationColor::from_str("warning"), Some(AnnotationColor::Warning));
        assert_eq!(AnnotationColor::from_str("warn"), Some(AnnotationColor::Warning));
        assert_eq!(AnnotationColor::from_str("info"), Some(AnnotationColor::Info));
        assert_eq!(AnnotationColor::from_str("hint"), Some(AnnotationColor::Hint));
        assert_eq!(AnnotationColor::from_str("added"), Some(AnnotationColor::Added));
        assert_eq!(AnnotationColor::from_str("add"), Some(AnnotationColor::Added));
        assert_eq!(AnnotationColor::from_str("modified"), Some(AnnotationColor::Modified));
        assert_eq!(AnnotationColor::from_str("deleted"), Some(AnnotationColor::Deleted));
        assert_eq!(AnnotationColor::from_str("unknown"), None);
    }

    #[test]
    fn test_gutter_mark_creation() {
        let mark = GutterMark {
            color: AnnotationColor::Added,
        };
        assert_eq!(mark.color, AnnotationColor::Added);
    }

    #[test]
    fn test_inline_highlight_creation() {
        let highlight = InlineHighlight {
            start_col: 5,
            end_col: Some(10),
            color: AnnotationColor::Error,
        };
        assert_eq!(highlight.start_col, 5);
        assert_eq!(highlight.end_col, Some(10));
        assert_eq!(highlight.color, AnnotationColor::Error);
    }

    #[test]
    fn test_line_annotation_creation() {
        let annotation = LineAnnotation {
            line: 42,
            gutter: Some(GutterMark {
                color: AnnotationColor::Warning,
            }),
            inline: vec![InlineHighlight {
                start_col: 1,
                end_col: Some(15),
                color: AnnotationColor::Warning,
            }],
        };
        assert_eq!(annotation.line, 42);
        assert!(annotation.gutter.is_some());
        assert_eq!(annotation.inline.len(), 1);
    }
}
