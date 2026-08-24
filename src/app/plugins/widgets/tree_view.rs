//! Tree view widget types for plugin API.
//!
//! Allows plugins to display hierarchical data, useful for:
//! - File browsers
//! - YAML/JSON viewers
//! - Outline views

/// Hard cap on tree nesting depth when parsing plugin-supplied tables or YAML.
///
/// Plugin hook return values are untrusted: a cyclic table (`t.children = {t}`)
/// or a pathologically deep one would otherwise recurse without bound and abort
/// the process via stack overflow (T0004). Any legitimate tree is far
/// shallower; branches beyond this depth are truncated rather than descended
/// into. The cap is low enough that constructing and dropping a maxed-out tree
/// stays well within the stack.
pub(crate) const MAX_TREE_DEPTH: usize = 256;

/// Click mode for tree view node activation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TreeClickMode {
    /// Activate node on single click
    SingleClick,
    /// Activate node on double click (default)
    #[default]
    DoubleClick,
}

/// Target node type for a context menu item
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContextMenuTarget {
    /// Show only for folder nodes
    Folder,
    /// Show only for file (leaf) nodes
    File,
    /// Show only when right-clicking empty area (no node)
    Empty,
    /// Show for all node types and empty area
    All,
}

/// A context menu item defined by a plugin
#[derive(Debug, Clone)]
pub struct ContextMenuItem {
    /// Display label (e.g., "New File...", "Delete")
    pub label: String,
    /// Action name sent back to plugin (e.g., "new_file", "delete")
    pub action: String,
    /// Which node types this item appears for
    pub target: ContextMenuTarget,
    /// If set, show an input dialog with this prompt before sending the action
    pub input_prompt: Option<String>,
    /// If set, show a confirmation dialog with this message before sending
    pub confirm_prompt: Option<String>,
    /// If true, pre-fill the input dialog with the current node name (for rename)
    pub input_prefill_node_name: bool,
    /// If true, copy the node's full path to clipboard (no action sent to plugin)
    pub clipboard: bool,
}

impl ContextMenuItem {
    /// Parse a context menu item from a Lua table
    pub fn from_lua_table(table: &mlua::Table) -> Option<Self> {
        let label: String = table.get("label").ok()?;
        let action: String = table.get("action").unwrap_or_default();
        let target_str: String = table.get("target").unwrap_or_else(|_| "all".to_string());
        let target = match target_str.as_str() {
            "folder" => ContextMenuTarget::Folder,
            "file" => ContextMenuTarget::File,
            "empty" => ContextMenuTarget::Empty,
            _ => ContextMenuTarget::All,
        };
        let input_prompt: Option<String> = table.get("input").ok();
        let confirm_prompt: Option<String> = table.get("confirm").ok();
        let input_prefill_node_name: bool = table.get("prefill_name").unwrap_or(false);
        let clipboard: bool = table.get("clipboard").unwrap_or(false);

        Some(Self {
            label,
            action,
            target,
            input_prompt,
            confirm_prompt,
            input_prefill_node_name,
            clipboard,
        })
    }
}

/// A request to show a tree view, returned from plugin hooks
#[derive(Debug, Clone, Default)]
pub struct TreeViewRequest {
    /// Title shown in the tree view header
    pub title: String,
    /// Root node of the tree
    pub root: Option<TreeNode>,
    /// YAML content to parse into a tree (alternative to root)
    pub yaml_content: Option<String>,
    /// Action name sent back to plugin on node click
    pub on_click_action: Option<String>,
    /// How many levels to auto-expand (0 = none, -1 = all)
    pub expand_depth: i32,
    /// Click mode: "single" or "double" (default)
    pub click_mode: TreeClickMode,
    /// Project root path for reconstructing full paths (e.g., for Copy Path)
    pub context_path: Option<String>,
    /// Plugin-defined context menu items
    pub context_menu: Vec<ContextMenuItem>,
    /// If true, this tree view persists across tab switches (e.g., file explorer)
    pub persistent: bool,
}

/// A node in the tree
#[derive(Debug, Clone, Default)]
pub struct TreeNode {
    /// Display label for this node
    pub label: String,
    /// Optional data payload (e.g., file path, metadata)
    pub data: Option<String>,
    /// Child nodes
    pub children: Vec<TreeNode>,
    /// Whether this node is initially expanded
    pub expanded: bool,
    /// Optional icon hint (e.g., "file", "folder", "error")
    pub icon: Option<String>,
    /// Optional semantic color name for the label (e.g., "modified", "added", "untracked", "conflict", "ignored")
    pub label_color: Option<String>,
    /// Lazy-load marker: node has children that haven't been loaded yet.
    /// The tree panel will show it as expandable and fire `node_expanded`
    /// when the user opens it.
    pub lazy: bool,
}

impl TreeViewRequest {
    /// Parse a tree view request from a Lua table
    pub fn from_lua_table(table: &mlua::Table) -> Option<Self> {
        let title: String = table.get("title").unwrap_or_default();

        // Parse root node if present
        let root = if let Ok(mlua::Value::Table(root_table)) = table.get::<mlua::Value>("root") {
            Some(TreeNode::from_lua_table(&root_table))
        } else {
            None
        };

        // Check for YAML content
        let yaml_content: Option<String> = table.get("yaml_content").ok();

        // Action to trigger on node click
        let on_click_action: Option<String> = table.get("on_click").ok();

        // Expand depth
        let expand_depth: i32 = table.get("expand_depth").unwrap_or(1);

        // Click mode: "single" or "double" (default)
        let click_mode = match table.get::<String>("click_mode") {
            Ok(ref s) if s == "single" => TreeClickMode::SingleClick,
            _ => TreeClickMode::DoubleClick,
        };

        // Context path (project root) for full path reconstruction
        let context_path: Option<String> = table.get("context_path").ok();

        // Parse context menu items
        let context_menu =
            if let Ok(mlua::Value::Table(menu_table)) = table.get::<mlua::Value>("context_menu") {
                menu_table
                    .pairs::<i32, mlua::Table>()
                    .flatten()
                    .filter_map(|(_, item_table)| ContextMenuItem::from_lua_table(&item_table))
                    .collect()
            } else {
                Vec::new()
            };

        // Persistent flag: tree survives tab switches (e.g., file explorer)
        let persistent: bool = table.get("persistent").unwrap_or(false);

        Some(Self {
            title,
            root,
            yaml_content,
            on_click_action,
            expand_depth,
            click_mode,
            context_path,
            context_menu,
            persistent,
        })
    }

    /// Check if this is a valid request (has root or YAML content)
    pub fn is_valid(&self) -> bool {
        self.root.is_some() || self.yaml_content.is_some()
    }
}

impl TreeNode {
    /// Create a new tree node with just a label
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            ..Default::default()
        }
    }

    /// Create a new tree node with label and children
    #[allow(dead_code)] // Used by tests and future API extensions
    pub fn with_children(label: impl Into<String>, children: Vec<TreeNode>) -> Self {
        Self {
            label: label.into(),
            children,
            expanded: true, // Nodes with children are expanded by default
            ..Default::default()
        }
    }

    /// Parse a tree node from a Lua table.
    ///
    /// Depth-bounded (see [`MAX_TREE_DEPTH`]): the table comes from an untrusted
    /// plugin and may be cyclic or extremely deep. At the cap the node's
    /// children are not descended into (the subtree is truncated) instead of
    /// recursing further and overflowing the stack.
    pub fn from_lua_table(table: &mlua::Table) -> Self {
        Self::from_lua_table_depth(table, 0)
    }

    fn from_lua_table_depth(table: &mlua::Table, depth: usize) -> Self {
        let label: String = table.get("label").unwrap_or_default();
        let data: Option<String> = table.get("data").ok();
        let expanded: bool = table.get("expanded").unwrap_or(false);
        let icon: Option<String> = table.get("icon").ok();
        let label_color: Option<String> = table.get("label_color").ok();
        let lazy: bool = table.get("lazy").unwrap_or(false);

        // Parse children recursively, but never past the depth cap: an untrusted
        // cyclic or very deep table must not abort the process via stack overflow.
        let children = if depth >= MAX_TREE_DEPTH {
            Vec::new()
        } else if let Ok(mlua::Value::Table(children_table)) = table.get::<mlua::Value>("children")
        {
            children_table
                .pairs::<i32, mlua::Table>()
                .flatten()
                .map(|(_, child_table)| TreeNode::from_lua_table_depth(&child_table, depth + 1))
                .collect()
        } else {
            Vec::new()
        };

        Self {
            label,
            data,
            children,
            expanded,
            icon,
            label_color,
            lazy,
        }
    }

    /// Check if this node has children
    pub fn has_children(&self) -> bool {
        !self.children.is_empty()
    }

    /// Count total nodes in this subtree
    #[allow(dead_code)]
    pub fn count_nodes(&self) -> usize {
        1 + self.children.iter().map(|c| c.count_nodes()).sum::<usize>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tree_node_new() {
        let node = TreeNode::new("test");
        assert_eq!(node.label, "test");
        assert!(node.children.is_empty());
        assert!(!node.expanded);
    }

    #[test]
    fn test_tree_node_with_children() {
        let child1 = TreeNode::new("child1");
        let child2 = TreeNode::new("child2");
        let parent = TreeNode::with_children("parent", vec![child1, child2]);

        assert_eq!(parent.label, "parent");
        assert_eq!(parent.children.len(), 2);
        assert!(parent.expanded);
        assert!(parent.has_children());
    }

    #[test]
    fn test_tree_node_count() {
        let leaf = TreeNode::new("leaf");
        assert_eq!(leaf.count_nodes(), 1);

        let parent = TreeNode::with_children(
            "parent",
            vec![
                TreeNode::new("child1"),
                TreeNode::with_children("child2", vec![TreeNode::new("grandchild")]),
            ],
        );
        assert_eq!(parent.count_nodes(), 4);
    }

    #[test]
    fn test_tree_view_request_validity() {
        let mut request = TreeViewRequest::default();
        assert!(!request.is_valid());

        request.root = Some(TreeNode::new("root"));
        assert!(request.is_valid());

        request.root = None;
        request.yaml_content = Some("key: value".to_string());
        assert!(request.is_valid());
    }

    /// Iterative chain depth (number of `children[0]` hops), safe on deep trees.
    fn chain_depth(root: &TreeNode) -> usize {
        let mut cur = root;
        let mut depth = 0;
        while let Some(first) = cur.children.first() {
            depth += 1;
            cur = first;
        }
        depth
    }

    // Regression (T0004, audit S1): a plugin table deeper than the cap must be
    // truncated, not descended into without bound. Before the depth guard the
    // resulting tree was as deep as the input (assertion below fails); a truly
    // deep input would overflow the stack and abort the process.
    #[test]
    fn deep_lua_table_is_truncated_at_cap() {
        let lua = mlua::Lua::new();
        let mut current = lua.create_table().unwrap();
        current.set("label", "leaf").unwrap();
        // Build a 350-deep single chain (well past MAX_TREE_DEPTH = 256).
        for i in 0..350 {
            let parent = lua.create_table().unwrap();
            parent.set("label", format!("n{}", i)).unwrap();
            let children = lua.create_table().unwrap();
            children.set(1, current).unwrap();
            parent.set("children", children).unwrap();
            current = parent;
        }

        let node = TreeNode::from_lua_table(&current);
        assert_eq!(
            chain_depth(&node),
            MAX_TREE_DEPTH,
            "a table deeper than the cap must be truncated to exactly the cap"
        );
    }

    // Regression (T0004, audit S1): a *cyclic* plugin table (`t.children = {t}`)
    // must not recurse forever. Before the fix this aborts the process via stack
    // overflow; after the fix the cycle is made finite at the depth cap.
    #[test]
    fn cyclic_lua_table_does_not_overflow() {
        let lua = mlua::Lua::new();
        let t = lua.create_table().unwrap();
        t.set("label", "x").unwrap();
        let children = lua.create_table().unwrap();
        children.set(1, t.clone()).unwrap(); // t.children = { t }
        t.set("children", children).unwrap();

        let node = TreeNode::from_lua_table(&t);
        assert_eq!(node.label, "x");
        assert_eq!(
            chain_depth(&node),
            MAX_TREE_DEPTH,
            "a self-cycle must be truncated to exactly the cap"
        );
    }
}
