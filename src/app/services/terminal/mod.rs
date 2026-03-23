//! Terminal emulation engine for embedded terminal widgets.
//!
//! Provides a VTE-based terminal emulator with PTY support.
//! Zero cost when not instantiated (lazy loading).

pub mod grid;
pub mod pty;
pub mod vte_handler;
