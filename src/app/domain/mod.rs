//! Domain layer - core data structures and types.
//!
//! This module contains the fundamental domain models:
//! - Document and DocumentId
//! - Application settings
//! - Message types for the event system

pub mod document;
pub mod messages;
pub mod settings;

pub use document::{Document, DocumentId};
pub use messages::Message;
pub use settings::{AppSettings, FontChoice, SyntaxTheme, ThemeMode};
