# Changelog

All notable changes to FerrisPad will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.9.0] - 2026-02-19

### Added
- **Markdown Preview**: Live side-by-side preview for `.md` files with async image resizing. Toggle via View menu or settings.
- **Tab Groups**: Colored tab groups (Red, Orange, Yellow, Green, Blue, Purple, Grey) with group labels, collapse/expand into compact chips, right-click context menu for group management (create, rename, recolor, ungroup, close).
- **Drag-to-Group**: Drag a tab onto the center of another tab to group them. Drag to the edges to reorder. Visual feedback: 50% blended highlight for grouping, vertical insertion line for reordering.
- **Draggable Collapsed Groups**: Collapsed group chips can be dragged to reorder entire groups. Click-without-drag still toggles expand/collapse.
- **Group Reorder Protection**: Ungrouped tabs cannot be inserted between tabs of the same group; insertion point snaps to group boundary.
- **Crash-Safe Session Auto-Save**: Session auto-saves every 30 seconds so tabs survive task manager kills, crashes, and power loss.
- **"+" New Tab Button**: Quick new tab creation button in the tab bar.
- **Syntax Highlighting**: 50+ languages via syntect with oniguruma regex backend. Chunked non-blocking processing for large files with sparse checkpoints every 128 lines.
- **Tabbed Editing**: Custom-drawn tab bar with rounded top corners, shrink-to-fit sizing, per-tab close buttons, hover highlighting, middle-click close, and dark/light theme support.
- **Session Restore**: Three modes (Off, Saved Files Only, Full including unsaved content) with multi-instance session merging and schema versioning.
- **Select All** (Ctrl+A), **Find Previous** (Ctrl+Shift+G), **Go To Line** (Ctrl+G).
- **Dynamic Line Number Gutter**: Auto-sizes based on document line count.
- **macOS Dark Mode Detection** via `AppleInterfaceStyle` defaults key.
- **Windows Executable Metadata**: Version, description, and copyright via embedded resource manifest.

### Fixed
- **FLTK Widget Lifecycle Leaks**: Fixed modify callback, tile widget, and shared image cache leaks via direct FFI.
- **glibc Memory Not Returned**: Added targeted `malloc_trim(0)` calls after tab close and preview hide.
- **TextBuffer::text() Memory Leak**: Plugged 3.8MB/call leak via `buffer_text_no_leak()` helper.
- **Find No Longer Marks Document as Dirty**: Dirty flag only sets on actual text changes, not selection changes.
- **Memory Leaks in Syntax Highlighting**: Reduced highlighting memory from 290MB to 63MB on large files.
- **Native File Dialogs Restored**: Fixed regression that broke native file chooser on Linux.
- **Allow Closing App While Dialog is Open**: `run_dialog` checks `app::should_program_quit()`.

### Changed
- **Major Architecture Refactoring**: Decomposed monolithic `main.rs` into `src/app/` (business logic) and `src/ui/` (presentation) with message-passing event-driven architecture. ~5,200 lines across 28 files.
- Extracted `HighlightController`, `UpdateController`, `PreviewController`, `EditorContainer`, and `TabManager` from AppState.
- Added `thiserror`-based `AppError` enum for consistent error handling.
- Configured jemalloc dirty/muzzy decay to 0ms at startup for immediate page return.
- Updated README and website to reflect v0.9.0 feature set.

## [0.1.8] - 2026-02-12

### Added
- **Direct In-App Updates**: Download and install updates directly without visiting GitHub, with automatic backup and restart.
- **Edit Menu Enhancements**: Added Undo, Redo, Cut, Copy, and Paste with standard keyboard shortcuts.
- **Pre-release Channel Toggle**: Opt-in to beta/RC updates directly from the Settings dialog.
- **Native Wayland Support**: Enabled FLTK's Wayland backend for correct keyboard layout handling on Linux Wayland sessions.
- **Linux "Open With" Support**: Added `%f` to desktop entry so FerrisPad appears in the file manager's "Open With" dialog.
- **macOS File Association**: Associated FerrisPad with text files (.txt, .md, .rs, etc.) in Finder.
- **Website Enhancements**: "Feeling brave?" button for unstable releases with dual-track versioning.

### Fixed
- **Keyboard Layout on Wayland**: Resolved incorrect shortcut mapping when OS keyboard layout differed from hardware keyboard (e.g. AZERTY hardware with QWERTY configured).
- **Update Download Freeze**: Fixed silent error handling and thread-safety issues that caused the updater to freeze.
- **Update Banner Visibility**: Improved readability of the update notification in Dark Mode.
- **Dual Icon Bug**: Resolved issue where two icons appeared in the title bar on macOS and Windows.
- **Edit Menu Layout**: Removed extra vertical space and cleaned up item alignment.
- **Version Bump Script**: Hardened script to handle stable and unstable tracks independently.

### Technical
- Implemented robust self-replacement update strategy with automatic backup and restart.
- Switched to `app::awake_callback()` for thread-safe FLTK UI updates from background threads.
- Enhanced `Info.plist` generation in the macOS build process for better system integration.
- Improved CLI argument parser to reliably handle "Open With" file paths.

## [0.1.7] - 2026-02-09

### Added
- **Command-Line Argument Support**: Open files directly from the terminal or via OS "Open With"
  - Pass a file path as the first argument to FerrisPad to load it on startup
  - Automatic loading of files when double-clicked in file managers (macOS/Linux/Windows)
- **Robust Path Handling**: Improved safety when handling file paths
  - Safe conversion of non-UTF-8 paths to prevent application panics
  - Uses `to_string_lossy()` for handling paths from various character sets

### Technical
- Refactored file opening logic into a shared `open_file` helper function
- Modularized state management for easier extension of startup parameters

## [0.1.6] - 2025-10-03

### Added
- **Windows Dark Mode Detection**: Automatic system theme detection on Windows
  - Reads Windows Registry `AppsUseLightTheme` value
  - Respects Windows system dark/light mode preference
  - Content (editor, menus) automatically themes on startup
- **Windows Title Bar Theming**: Dark/light title bar support
  - Uses Windows DWM (Desktop Window Manager) API
  - Title bar matches application theme
  - Supports Windows 10 build 1809+ and Windows 11
  - Dual attribute ID support (19 and 20) for cross-version compatibility
  - Updates dynamically when theme changes via menu or settings

### Changed
- **Default Font**: Changed from Screen Bold to Courier for better readability
- Improved bump-version script to handle pre-release versions (e.g., 0.1.5-rc.1)
- Enhanced Windows security documentation with Defender exclusion instructions

### Fixed
- Version bump script now correctly updates download URLs for RC versions
- Windows Registry dark mode detection properly isolated to Windows platform
- Windows Defender quarantine workaround documented in README and website

### Technical
- Added platform-specific Windows dependencies:
  - `winreg` 0.52 (Windows Registry access)
  - `windows` 0.58 with `Win32_Foundation` and `Win32_Graphics_Dwm` features
- Implemented `set_windows_titlebar_theme()` function (Windows only)
  - Called after `window.show()` to ensure valid HWND
  - Uses `DwmSetWindowAttribute()` with both attribute IDs (19 and 20)
  - Graceful degradation on unsupported Windows versions
- Platform-conditional compilation with `#[cfg(target_os = "windows")]`
- Updated `detect_system_dark_mode()` with platform-specific blocks
- Cross-platform verification (builds on Windows and Linux targets)
- Comprehensive debugging documentation (32KB journey document)

### Platform Support
- Windows 11: Full support (dark mode + title bar)
- Windows 10 2004+: Full support (dark mode + title bar)
- Windows 10 1809-1903: Full support (dark mode + title bar via attribute 19)
- Windows 10 <1809: Dark mode detection only (no title bar theming)
- Linux: Unchanged (GNOME/GTK dark mode detection)
- macOS: Unchanged (defaults to light mode, detection planned for future)

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

[0.9.0]: https://github.com/fedro86/ferrispad/compare/0.1.8...0.9.0
[0.1.8]: https://github.com/fedro86/ferrispad/compare/0.1.7...0.1.8
[0.1.7]: https://github.com/fedro86/ferrispad/compare/0.1.6...0.1.7
[0.1.6]: https://github.com/fedro86/ferrispad/compare/0.1.5...0.1.6
[0.1.5]: https://github.com/fedro86/ferrispad/compare/0.1.4...0.1.5
[0.1.4]: https://github.com/fedro86/ferrispad/compare/0.1.3...0.1.4
[0.1.3]: https://github.com/fedro86/ferrispad/compare/0.1.2...0.1.3
[0.1.2]: https://github.com/fedro86/ferrispad/compare/0.1.1...0.1.2
[0.1.1]: https://github.com/fedro86/ferrispad/compare/0.1.0...0.1.1
[0.1.0]: https://github.com/fedro86/ferrispad/releases/tag/0.1.0
