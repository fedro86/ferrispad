# FerrisPad

<div align="center">
  <img src="assets/crab-notepad-emoji-8bit.png" alt="FerrisPad Logo" width="200" style="border-radius: 50%; border: 3px solid #333;"/>
  <p><em>A minimalist text editor written in Rust</em></p>
</div>

## Overview

FerrisPad is a single-binary text editor with syntax highlighting for 50+ languages, tab groups, named multi-project sessions, a Lua plugin system, a built-in terminal, regex find/replace, and an arbitrary system-font picker. It follows a strict [philosophy](PHILOSOPHY.md): 0% CPU when idle, no runtime dependencies, no telemetry.

**[Website](https://www.ferrispad.com)** | **[Wiki](https://github.com/fedro86/ferrispad/wiki)** | **[Downloads](https://github.com/fedro86/ferrispad/releases)** | **[Changelog](CHANGELOG.md)**

## Install

Download the latest release from the [releases page](https://github.com/fedro86/ferrispad/releases/latest) or [www.ferrispad.com](https://www.ferrispad.com):

| Platform | Download |
|---|---|
| **Windows** (x86_64) | `FerrisPad-v<version>-windows-x64.zip` (portable) |
| **macOS** (Intel + Apple Silicon) | `FerrisPad-v<version>-macos.dmg` |
| **Debian / Ubuntu** (x86_64, ARM64) | `FerrisPad-v<version>-ubuntu-amd64.deb` or `-ubuntu-arm64.deb` → `sudo apt install ./FerrisPad-v<version>-ubuntu-<arch>.deb` |
| **Other Linux** (Fedora, Arch, openSUSE…; x86_64, ARM64) | Flatpak from [FlatPark](https://flatpark.org/apps/com.ferrispad.FerrisPad/), see below |
| **Any Linux** (x86_64, ARM64) | `FerrisPad-v<version>-linux-amd64` or `-linux-arm64`: a standalone binary, `chmod +x` and run |

The Linux builds need glibc 2.39 or newer (Ubuntu 24.04, Debian 13, Fedora 40 and later). On older systems, [build from source](#build-from-source).

**Flatpak (FlatPark):**

[![Get it on FlatPark](https://flatpark.org/badges/get-it-on-flatpark.svg)](https://flatpark.org/apps/com.ferrispad.FerrisPad/)

```bash
flatpak remote-add --if-not-exists flatpark https://dl.flatpark.org/flatpark.flatpakrepo
flatpak install flatpark com.ferrispad.FerrisPad
```
The Flatpak installs the official release binary and updates with `flatpak update`. Elsewhere, FerrisPad checks GitHub for a new release at startup (at most once a day; it can be turned off in Settings). The portable builds can replace themselves, and only with a binary that carries a valid ed25519 signature (the `.sig` files on each release). A `.deb` install is updated by installing the new `.deb`.

## Build from Source

**Prerequisites:** Rust (latest stable)

**Linux (Ubuntu/Debian):**
```bash
sudo apt-get install libfltk1.3-dev libfontconfig1-dev libxext-dev libxft-dev \
  libxinerama-dev libxcursor-dev libxrender-dev libxfixes-dev libpango1.0-dev \
  libgl1-mesa-dev libglu1-mesa-dev libwayland-dev wayland-protocols \
  libdbus-1-dev libxkbcommon-dev
```

**Linux (Nix):** the repo ships a dev flake with the complete build environment:
```bash
nix develop     # one-off shell
direnv allow    # or once with direnv: auto-loads every time you cd in
```

**macOS:**
```bash
xcode-select --install
```

**Build and run:**
```bash
git clone https://github.com/fedro86/ferrispad
cd ferrispad
cargo build --release
./target/release/FerrisPad
```

**Distribution packages:**
```bash
# Interactive build menu (deb, dmg, zip)
./scripts/build-releases.sh

# Or build .deb directly (Linux)
cargo install cargo-deb
cargo deb
```

**Desktop integration (Linux):**
```bash
./scripts/install-desktop.sh    # Install icons and .desktop entry
./scripts/uninstall-desktop.sh  # Remove
```

See [BUILD_GUIDE.md](docs/guides/BUILD_GUIDE.md) for detailed instructions.

## Running Tests

```bash
cargo test                              # Unit and integration tests
cargo clippy --all-targets --all-features  # Lint (must be warning-free)
cargo fmt --check                       # Format check
```

## Architecture

~43,600 lines across 101 Rust source files. Message-passing event-driven architecture with Clean Architecture layers.

```
src/
  main.rs              # Entry point, FLTK event loop
  dispatch.rs          # Grouped handler functions (file, tab, edit, view, ...)
  lib.rs               # Library crate (ferris_pad)
  app/
    state.rs             # Central coordinator (AppState, mediates controllers)
    domain/              # Core data types (Document, Message enum, AppSettings)
    controllers/         # Orchestration (10 controllers)
      file.rs              # File I/O, returns Vec<FileAction>
      highlight.rs         # 3-tier syntax highlighting engine
      tabs.rs              # Tab/group lifecycle (TabManager)
      widget.rs            # Plugin widget lifecycle (tree/split views)
      plugin.rs            # Plugin dialogs, toggle/reload
      session.rs           # Auto-save, restore
      view.rs              # UI toggles (dark mode, line numbers, word wrap)
      preview.rs           # Markdown preview
      update.rs            # Update banner
      hook_dispatch.rs     # Plugin hook dispatch (free functions)
    services/            # Business logic
      syntax/              # Chunked highlighter, sparse checkpoints, style map
      terminal/            # PTY, VTE parser, grid model
      session.rs           # Session persistence
      shortcut_registry.rs # Keyboard shortcut management
      editor_context.rs    # Structured editor state for MCP/plugins
      plugin_registry.rs   # Plugin registry with caching
      plugin_update_checker.rs # Plugin update check scheduling
      plugin_verify.rs     # Signature and checksum verification
      updater.rs           # Update checker (stable/beta channels)
      http.rs              # HTTPS client (ureq + rustls), size-capped downloads
      text_ops.rs          # Text manipulation helpers
      yaml_parser.rs       # YAML/JSON tree parser
      file_size.rs         # Large file thresholds and streaming
      font_catalog.rs      # System font enumeration (font picker)
    plugins/             # Lua plugin system
      api/                 # Plugin API (editor, filesystem, commands, sandbox)
      hooks.rs             # Hook definitions and dispatch
      runtime.rs           # Lua VM lifecycle
      loader.rs            # Plugin discovery and loading
      security.rs          # Static source analysis, sandbox enforcement
      annotations.rs       # Diagnostic annotations (errors, warnings)
      diff.rs              # Git diff computation
      widgets/             # Plugin widget backends (tree, split, terminal)
    mcp/                 # MCP server (JSON-RPC over TCP, stdio bridge)
    infrastructure/      # FFI helpers (buffer leak fix), errors, platform detection
  ui/
    main_window.rs       # Widget layout
    editor_container.rs  # Editor widget wrapper
    tab_bar.rs           # Custom tab bar with drag-and-drop
    terminal_panel.rs    # Embedded terminal widget
    diagnostic_panel.rs  # Lint/diagnostic display panel
    split_panel.rs       # Plugin split view widget
    tree_panel.rs        # Plugin tree view widget
    start_page.rs        # Welcome view shown when no tabs are open
    status_bar.rs        # Cursor position, file path, language
    menu.rs              # Menu bar with shortcut registry
    layout_shortcut.rs   # Ctrl+letter shortcuts across keyboard layouts
    file_dialogs.rs      # Native file picker helpers
    theme.rs             # Dark/light themes, platform titlebar
    toast.rs             # Toast notifications
    dialogs/             # Find (regex), Settings, GoTo, About, Update, Plugin Manager,
                         #   Shortcuts, Large File, Read-Only Viewer, Community Install,
                         #   Font Picker, Session Picker
```

All UI interactions flow through a `Message` enum (~100 variants) dispatched in the main event loop. Controllers return action enums (e.g. `Vec<FileAction>`) for cross-cutting effects.

## Technology Stack

- **Language**: Rust (2024 edition)
- **GUI Framework**: FLTK via fltk-rs
- **Syntax Highlighting**: syntect with oniguruma regex backend
- **Plugin Runtime**: Lua 5.4 via mlua (statically linked)
- **Allocator**: jemalloc (Linux/macOS) for reduced memory fragmentation

## Plugin Development

Plugins are Lua scripts loaded from `~/.config/ferrispad/plugins/`. The plugin system provides 11 event hooks, a rich editor API, and widget primitives (tree views, split panels, terminal).

- **Official plugins** are maintained in [ferrispad-plugins](https://github.com/fedro86/ferrispad-plugins)
- **Community plugins** are listed in `community-plugins.json` with pinned git tags and SHA-256 checksums
- See the [Plugin Development Guide](https://github.com/fedro86/ferrispad-plugins/blob/master/CONTRIBUTING.md) for the API reference and examples

## Known Limitations

- **Maximum editable file size: ~1.9 GB.** FLTK's `Fl_Text_Buffer` uses 32-bit `int` for buffer positions. Files at or above 2 GiB (2^31 bytes) overflow and crash. FerrisPad enforces a hard cap at 1.9 GiB — larger files can still be viewed read-only (memory-mapped) or opened partially via tail/chunk mode.

## Contributing

Contributions are welcome. Fork the repo, make your changes, and submit a pull request.

Before submitting, ensure `cargo test`, `cargo clippy`, and `cargo fmt --check` all pass with zero warnings.

## License

[MIT License](LICENSE)
