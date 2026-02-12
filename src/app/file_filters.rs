/// Get filter pattern for text file formats with multiple options
///
/// Returns a multi-line filter string where each line is a separate filter option.
/// FLTK format: "Description\tPattern\nDescription2\tPattern2"
/// Note: FLTK automatically adds "All Files (*)" option, so we don't include it
pub fn get_text_files_filter_multiline() -> String {
    vec![
        "Text Files\t*.txt",
        "Markdown Files\t*.{md,markdown}",
        "Rust Files\t*.rs",
        "Python Files\t*.py",
        "JavaScript Files\t*.{js,jsx,ts,tsx}",
        "Config Files\t*.{json,yaml,yml,toml,ini,cfg,conf}",
        "Web Files\t*.{html,css,scss,sass}",
    ].join("\n")
}

/// Get filter pattern for all files (used in Save dialogs)
pub fn get_all_files_filter() -> String {
    "*".to_string()
}

/// Generate platform-specific file filter string for native dialogs
///
/// FLTK accepts these filter formats:
/// - Simple wildcard: "*.txt"
/// - Multiple wildcards: "*.{txt,md,rst}"
/// - With description (optional): "Text Files\t*.txt"
/// - Multiple filters: "Text Files\t*.txt\nMarkdown\t*.md"
///
/// For maximum compatibility, we use the simple format without description.
pub fn get_platform_filter(_description: &str, pattern: &str) -> String {
    // FLTK handles the platform-specific format internally
    // We just pass the pattern directly
    pattern.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_platform_filter_simple() {
        let filter = get_platform_filter("Text Files", "*.txt");
        assert_eq!(filter, "*.txt");
    }

    #[test]
    fn test_platform_filter_multiple_extensions() {
        let filter = get_platform_filter("Text Files", "*.{txt,md,rst}");
        assert_eq!(filter, "*.{txt,md,rst}");
    }

    #[test]
    fn test_platform_filter_all_files() {
        let filter = get_platform_filter("All Files", "*");
        assert_eq!(filter, "*");
    }

    #[test]
    fn test_platform_filter_ignores_description() {
        let filter1 = get_platform_filter("Text Files", "*.txt");
        let filter2 = get_platform_filter("Different Description", "*.txt");
        assert_eq!(filter1, filter2);
        assert_eq!(filter1, "*.txt");
    }

    #[test]
    fn test_all_files_filter() {
        let filter = get_all_files_filter();
        assert_eq!(filter, "*");
    }

    #[test]
    fn test_multiline_filter_format() {
        let filter = get_text_files_filter_multiline();
        assert!(filter.contains("\n"));
        assert!(filter.contains("\t"));
        assert!(filter.contains("Text Files"));
        assert!(filter.contains("Markdown Files"));
        assert!(filter.contains("Rust Files"));
        assert!(filter.contains("Python Files"));
        assert!(filter.contains("Config Files"));
    }
}
