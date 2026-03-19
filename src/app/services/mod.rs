//! Services layer - business operations and utilities.
//!
//! This module contains business logic and operations:
//! - Session persistence
//! - Update checking
//! - Text operations
//! - Syntax highlighting
//! - Plugin registry

pub mod file_size;
pub mod plugin_registry;
pub mod plugin_update_checker;
pub mod plugin_verify;
pub mod session;
pub mod shortcut_registry;
pub mod syntax;
pub mod terminal;
pub mod text_ops;
pub mod updater;
pub mod yaml_parser;
