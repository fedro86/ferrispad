# Changelog

All notable changes to FerrisPad will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.4] - 2025-01-XX

### Added
- **Find & Replace**: Full text search and replace functionality
  - Find dialog (Ctrl+F) with case-sensitive option
  - Find & Replace dialog (Ctrl+H) with Replace and Replace All
  - Automatic text highlighting and scroll to match
  - Wrap-around search when reaching end of document
- **Settings Persistence**: Save user preferences across sessions
  - Settings dialog accessible from File menu
  - Configure theme (Light/Dark/System Default)
  - Choose font (Screen Bold/Courier/Helvetica Mono)
  - Set font size (Small 12/Medium 16/Large 20)
  - Toggle line numbers and word wrap
  - Settings saved to JSON at cross-platform config location
  - Settings sync with View menu checkboxes
- **Multi-Format File Support**: Save files in various formats
  - Support for .txt, .md, .rs, .py, .json, .xml, .yaml, .toml, .ini, .cfg, .log
  - Clean dropdown menu with format categories
  - "All Files" option for any extension
- **Smart Save Functionality**:
  - Quick Save (Ctrl+S) - saves to existing file without dialog
  - Save As (Ctrl+Shift+S) - opens dialog for new name/location
  - Shows confirmation dialog for unsaved changes on quit

### Fixed
- File dialogs now properly display .txt files on all platforms
- File dialog filters use FLTK-compatible format for cross-platform consistency

### Changed
- Improved file dialog UI with multi-line format dropdown
- Refactored save logic to eliminate code duplication
- Enhanced keyboard shortcuts for better workflow

### Technical
- Added `src/settings.rs` module for settings management
- Added serde and serde_json dependencies for JSON serialization
- Added dirs dependency for cross-platform config paths
- Comprehensive test coverage (22 tests)
- All features follow TDD (Test-Driven Development) principles

## [0.1.3] - 2025-01-15

### Added
- Desktop integration with proper icon support
- Beautiful crab mascot icons in all standard sizes (16x16 to 512x512)
- Application menu entry
- File title display in window title bar
- Installation and uninstallation scripts for Linux

### Changed
- Improved icon rendering across different desktop environments
- Enhanced desktop file with proper MIME types

## [0.1.2] - 2025-01-10

### Added
- View menu with toggles for Line Numbers, Word Wrap, and Dark Mode
- Format menu for font selection and font size
- Light and Dark theme support
- System theme detection (Linux)

### Fixed
- Cursor blinking now works properly
- Window positioning and sizing improvements

## [0.1.1] - 2025-01-05

### Added
- Basic keyboard shortcuts (Ctrl+N, Ctrl+O, Ctrl+S, Ctrl+Q)
- Line numbers toggle
- Word wrap toggle
- Font customization options

### Fixed
- File saving reliability improvements
- Memory leak fixes

## [0.1.0] - 2025-01-01

### Added
- Initial release
- Basic text editing functionality
- Native file dialogs for Open and Save
- Clean, minimal interface
- Cross-platform support (Linux, macOS, Windows)
- FLTK-based GUI
- Rust implementation for speed and safety

[0.1.4]: https://github.com/fedro86/ferrispad/compare/v0.1.3...v0.1.4
[0.1.3]: https://github.com/fedro86/ferrispad/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/fedro86/ferrispad/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/fedro86/ferrispad/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/fedro86/ferrispad/releases/tag/v0.1.0
