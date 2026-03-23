//! YAML to TreeNode parser.
//!
//! Converts YAML content into a hierarchical TreeNode structure
//! for display in the Tree View widget.

use crate::app::plugins::widgets::TreeNode;
use serde_yaml::Value;

/// Parse YAML content into a TreeNode structure.
///
/// # Arguments
/// * `yaml_content` - The YAML string to parse
/// * `root_label` - Label for the root node
///
/// # Returns
/// A TreeNode representing the YAML structure, or an error TreeNode if parsing fails.
pub fn parse_yaml_to_tree(yaml_content: &str, root_label: &str) -> TreeNode {
    match serde_yaml::from_str::<Value>(yaml_content) {
        Ok(value) => {
            let mut root = TreeNode::new(root_label);
            root.expanded = true;
            root.icon = Some("none".to_string());
            root.children = value_to_children(&value);
            // Append type indicator to root label
            match &value {
                Value::Mapping(map) => root.label = format!("{} {{{}}}", root_label, map.len()),
                Value::Sequence(seq) => root.label = format!("{} [{}]", root_label, seq.len()),
                _ => {}
            }
            root
        }
        Err(e) => {
            let mut error_node = TreeNode::new(format!("Error: {}", e));
            error_node.icon = Some("error".to_string());
            error_node
        }
    }
}

/// Convert a serde_yaml::Value to a vector of TreeNode children
fn value_to_children(value: &Value) -> Vec<TreeNode> {
    match value {
        Value::Mapping(map) => {
            map.iter()
                .map(|(k, v)| {
                    let key_str = value_to_string(k);
                    let mut node = TreeNode::new(&key_str);
                    node.icon = Some("none".to_string());

                    match v {
                        Value::Mapping(m) => {
                            node.label = format!("{} {{{}}}", key_str, m.len());
                            node.expanded = true;
                            node.children = value_to_children(v);
                        }
                        Value::Sequence(s) => {
                            node.label = format!("{} [{}]", key_str, s.len());
                            node.expanded = true;
                            node.children = value_to_children(v);
                        }
                        _ => {
                            // Leaf value - show key: value
                            let value_str = value_to_string(v);
                            node.label = format!("{}: {}", key_str, value_str);
                            node.data = Some(value_str);
                        }
                    }
                    node
                })
                .collect()
        }
        Value::Sequence(seq) => {
            seq.iter()
                .enumerate()
                .map(|(i, v)| {
                    let mut node = TreeNode::new(format!("[{}]", i));
                    node.icon = Some("none".to_string());

                    match v {
                        Value::Mapping(m) => {
                            node.label = format!("{} {{{}}}", i, m.len());
                            node.expanded = false; // Arrays collapsed by default
                            node.children = value_to_children(v);
                        }
                        Value::Sequence(s) => {
                            node.label = format!("{} [{}]", i, s.len());
                            node.expanded = false;
                            node.children = value_to_children(v);
                        }
                        _ => {
                            let value_str = value_to_string(v);
                            node.label = format!("[{}]: {}", i, value_str);
                            node.data = Some(value_str);
                        }
                    }
                    node
                })
                .collect()
        }
        _ => Vec::new(),
    }
}

/// Convert a YAML value to a display string
fn value_to_string(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => {
            // Truncate long strings for display
            if s.len() > 50 {
                format!("{}...", &s[..47])
            } else {
                s.clone()
            }
        }
        Value::Sequence(seq) => format!("[{} items]", seq.len()),
        Value::Mapping(map) => format!("{{{} keys}}", map.len()),
        Value::Tagged(tagged) => format!("!{} {}", tagged.tag, value_to_string(&tagged.value)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_yaml() {
        let yaml = r#"
name: test
version: 1.0.0
"#;
        let tree = parse_yaml_to_tree(yaml, "config.yaml");
        assert_eq!(tree.label, "config.yaml {2}");
        assert_eq!(tree.children.len(), 2);
        assert!(tree.children[0].label.contains("name"));
        assert!(tree.children[1].label.contains("version"));
    }

    #[test]
    fn test_parse_nested_yaml() {
        let yaml = r#"
database:
  host: localhost
  port: 5432
"#;
        let tree = parse_yaml_to_tree(yaml, "config.yaml");
        assert_eq!(tree.children.len(), 1);
        let db = &tree.children[0];
        assert_eq!(db.label, "database {2}");
        assert_eq!(db.children.len(), 2);
    }

    #[test]
    fn test_parse_array_yaml() {
        let yaml = r#"
items:
  - first
  - second
  - third
"#;
        let tree = parse_yaml_to_tree(yaml, "config.yaml");
        let items = &tree.children[0];
        assert_eq!(items.label, "items [3]");
        assert_eq!(items.children.len(), 3);
    }

    #[test]
    fn test_parse_invalid_yaml() {
        let yaml = "invalid: yaml: content:";
        let tree = parse_yaml_to_tree(yaml, "config.yaml");
        // Should have an error node as root
        assert!(
            tree.label.contains("Error") || tree.children.iter().any(|c| c.label.contains("Error"))
        );
    }

    #[test]
    fn test_value_to_string_truncation() {
        let long_string = "a".repeat(100);
        let result = value_to_string(&Value::String(long_string));
        assert!(result.len() < 60);
        assert!(result.ends_with("..."));
    }
}
