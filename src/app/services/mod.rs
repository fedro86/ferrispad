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
pub mod session;
pub mod syntax;
pub mod text_ops;
pub mod updater;
