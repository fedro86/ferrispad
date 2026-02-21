//! Application layer - organized by Clean Architecture principles.
//!
//! # Structure
//!
//! - `domain/` - Core data structures (Document, Settings, Messages)
//! - `controllers/` - Orchestration (TabManager, HighlightController, etc.)
//! - `services/` - Business operations (session, updater, text_ops, syntax)
//! - `infrastructure/` - External integrations (FLTK buffer, platform, error)
//! - `state.rs` - Main application coordinator

pub mod controllers;
pub mod domain;
pub mod infrastructure;
pub mod services;
pub mod state;

// Re-exports for convenient external access
pub use controllers::tabs::{GroupColor, GroupId, TabGroup};
pub use domain::{AppSettings, Document, DocumentId, FontChoice, Message, SyntaxTheme, ThemeMode};
pub use infrastructure::buffer::buffer_text_no_leak;
pub use infrastructure::platform::detect_system_dark_mode;
pub use services::session::SessionRestore;
pub use services::updater::UpdateChannel;
