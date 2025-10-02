# Changelog

All notable changes to FerrisPad will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.5] - 2025-10-02

### Added
- **In-App Update Checker**: Privacy-first update notification system
  - Manual update check via Help → Check for Updates menu
  - Auto-check on startup (once per 24 hours, fully optional)
  - Notification banner when updates are available
  - Settings toggle to enable/disable auto-updates
  - GitHub API integration for release checking
  - Semantic version comparison
  - Background thread for non-blocking checks
  - No telemetry or tracking - only checks GitHub public API
- **About Dialog**: Standard application information
  - Help → About FerrisPad menu item
  - Displays version, copyright, license information
  - Project website and GitHub links
- **Privacy-First Design**: Complete transparency and user control
  - Update checker can be completely disabled
  - No personal data collection
  - No usage tracking or analytics
  - Open source and auditable code

### Changed
- Extended Settings dialog with Updates section
- Enhanced Help menu with About and Check for Updates items
- Updated website with privacy section and new features
- Improved README with privacy commitments and update checker documentation

### Fixed
- Settings dialog close button no longer closes entire application
- Proper event handling for modal dialogs

### Technical
- Added `src/updater.rs` module (267 lines)
  - GitHub API integration
  - Semantic versioning with semver crate
  - Background update checking
  - Thread-safe state management with Arc<Mutex<T>>
  - Comprehensive test coverage (12 unit tests)
- Extended `src/settings.rs` with update preferences
  - auto_check_updates field
  - update_channel (Stable/Beta)
  - last_update_check timestamp
  - skipped_versions list
  - Backward compatibility with old config files
- Enhanced `src/main.rs` (+288 lines)
  - Background update check on startup
  - Update notification banner
  - About dialog implementation
  - Manual check dialog
- Added dependencies:
  - reqwest (with rustls-tls for better portability)
  - semver (for version comparison)
  - open (for opening URLs in browser)
- Total test count: 18 unit tests (all passing)
- Comprehensive documentation (15 files, ~170 KB in docs/temp/0.1.5/)

## [0.1.4] - 2025-10-02

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

## [0.1.3] - 2025-10-01

### Added
- Professional icons for all platforms (Windows, macOS, Linux)
- Windows executable with embedded icon
- Desktop integration improvements

### Changed
- Updated to Rust edition 2024
- Improved build and release workflows

### Fixed
- macOS universal binary build
- Download URLs for release artifacts
- GitHub Actions workflow permissions

## [0.1.2] - 2025-10-01

### Fixed
- GitHub Actions workflow to support tags without 'v' prefix

## [0.1.1] - 2025-10-01

### Added
- MIT License
- GitHub Releases integration
- Build guides and documentation

### Fixed
- Binary file distribution issues
- Download links to use GitHub releases

## [0.1.0] - 2025-09-30

### Added
- Initial release
- Basic text editing functionality
- Native file dialogs for Open and Save
- Clean, minimal interface
- Cross-platform support (Linux, macOS, Windows)
- FLTK-based GUI
- Rust implementation for speed and safety

[0.1.5]: https://github.com/fedro86/ferrispad/compare/0.1.4...0.1.5
[0.1.4]: https://github.com/fedro86/ferrispad/compare/0.1.3...0.1.4
[0.1.3]: https://github.com/fedro86/ferrispad/compare/0.1.2...0.1.3
[0.1.2]: https://github.com/fedro86/ferrispad/compare/0.1.1...0.1.2
[0.1.1]: https://github.com/fedro86/ferrispad/compare/0.1.0...0.1.1
[0.1.0]: https://github.com/fedro86/ferrispad/releases/tag/0.1.0
