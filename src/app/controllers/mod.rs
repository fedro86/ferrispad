//! Controllers layer - orchestration and coordination.
//!
//! This module contains controllers that coordinate between
//! domain models, services, and the UI:
//! - Tab management
//! - Syntax highlighting orchestration
//! - Markdown preview
//! - Update management

pub mod highlight;
pub mod preview;
pub mod tabs;
pub mod update;
