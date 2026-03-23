//! Terminal view widget types for plugin API.
//!
//! Allows plugins to display an embedded terminal emulator.
//! Used for CLI tools like Claude Code.

/// A request to show a terminal view, returned from plugin hooks
#[derive(Debug, Clone, Default)]
pub struct TerminalViewRequest {
    /// Title shown in the terminal view header
    pub title: String,
    /// Command to run (None = default shell from $SHELL)
    pub command: Option<String>,
    /// CLI arguments for the command
    pub args: Vec<String>,
    /// Working directory (None = project root or home)
    pub working_dir: Option<String>,
    /// If true, this terminal persists across tab switches
    pub persistent: bool,
}

impl TerminalViewRequest {
    /// Parse a terminal view request from a Lua table
    pub fn from_lua_table(table: &mlua::Table) -> Option<Self> {
        let title: String = table.get("title").unwrap_or_default();
        let command: Option<String> = table.get("command").ok();
        let persistent: bool = table.get("persistent").unwrap_or(false);
        let working_dir: Option<String> = table.get("working_dir").ok();

        // Parse args array
        let args = if let Ok(mlua::Value::Table(args_table)) = table.get::<mlua::Value>("args") {
            args_table
                .pairs::<i32, String>()
                .flatten()
                .map(|(_, s)| s)
                .collect()
        } else {
            Vec::new()
        };

        Some(Self {
            title,
            command,
            args,
            working_dir,
            persistent,
        })
    }

    /// Check if this is a valid request (has a title)
    pub fn is_valid(&self) -> bool {
        !self.title.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_terminal_view_request_default() {
        let request = TerminalViewRequest::default();
        assert!(!request.is_valid());
        assert!(request.command.is_none());
        assert!(request.args.is_empty());
        assert!(!request.persistent);
    }

    #[test]
    fn test_terminal_view_request_valid() {
        let request = TerminalViewRequest {
            title: "Claude Code".to_string(),
            command: Some("claude".to_string()),
            persistent: true,
            ..Default::default()
        };
        assert!(request.is_valid());
        assert_eq!(request.command.as_deref(), Some("claude"));
    }
}
