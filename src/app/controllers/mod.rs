//! Controllers layer - orchestration and coordination.
//!
//! This module contains controllers that coordinate between
//! domain models, services, and the UI:
//! - File operations (open, save, new)
//! - Tab management
//! - Syntax highlighting orchestration
//! - Markdown preview
//! - Update management
//! - View state (line numbers, word wrap, fonts)
//! - Session persistence
//! - Plugin management coordination

pub mod file;
pub mod highlight;
pub mod hook_dispatch;
pub mod plugin;
pub mod preview;
pub mod session;
pub mod tabs;
pub mod update;
pub mod view;
pub mod widget;
