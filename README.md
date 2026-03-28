# FerrisPad

<div align="center">
  <img src="assets/crab-notepad-emoji-8bit.png" alt="FerrisPad Logo" width="200" style="border-radius: 50%; border: 3px solid #333;"/>
  <p><em>A fast, lightweight text editor built with Rust and FLTK</em></p>
</div>

## Overview

FerrisPad is a single-binary text editor with syntax highlighting for 50+ languages, tab groups, session restore, a Lua plugin system, and a built-in terminal. It follows a strict [philosophy](PHILOSOPHY.md): 0% CPU when idle, no runtime dependencies, no telemetry.

**[Website](https://www.ferrispad.com)** | **[Wiki](https://github.com/fedro86/ferrispad/wiki)** | **[Downloads](https://github.com/fedro86/ferrispad/releases)** | **[Changelog](CHANGELOG.md)**

## Build from Source

**Prerequisites:** Rust (latest stable)

**Linux (Ubuntu/Debian):**
```bash
sudo apt-get install libfltk1.3-dev libfontconfig1-dev libxext-dev libxft-dev \
  libxinerama-dev libxcursor-dev libxrender-dev libxfixes-dev libpango1.0-dev \
  libgl1-mesa-dev libglu1-mesa-dev
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

~26,000 lines across 85 Rust source files. Message-passing event-driven architecture with Clean Architecture layers.

```
src/
  main.rs            # Entry point, FLTK event loop
  dispatch.rs        # Grouped handler functions (file, tab, edit, view, ...)
  lib.rs             # Library crate (ferris_pad)
  app/
    state.rs           # Central coordinator (AppState, mediates controllers)
    domain/            # Core data types (Document, Message enum, AppSettings)
    controllers/       # Orchestration (11 controllers)
      file.rs            # File I/O, returns Vec<FileAction>
      highlight.rs       # 3-tier syntax highlighting engine
      tabs.rs            # Tab/group lifecycle (TabManager)
      widget.rs          # Plugin widget lifecycle (tree/split views)
      plugin.rs          # Plugin dialogs, toggle/reload
      session.rs         # Auto-save, restore
      view.rs            # UI toggles (dark mode, line numbers, word wrap)
      preview.rs         # Markdown preview
      update.rs          # Update banner
      hook_dispatch.rs   # Plugin hook dispatch (free functions)
    services/          # Business logic (syntax, session, updater, shortcuts)
    plugins/           # Lua plugin system (api, hooks, loader, runtime, security)
    infrastructure/    # FFI helpers, errors, platform detection
  ui/
    tab_bar.rs         # Custom tab bar with drag-and-drop
    main_window.rs     # Widget layout
    split_panel.rs     # Plugin split view widget
    tree_panel.rs      # Plugin tree view widget
    menu.rs            # Menu bar with shortcut registry
    dialogs/           # Find, Settings, GoTo, About, Update, Plugin Manager, Shortcuts
    theme.rs           # Dark/light themes, platform titlebar
```

All UI interactions flow through a `Message` enum (~90 variants) dispatched in the main event loop. Controllers return action enums (e.g. `Vec<FileAction>`) for cross-cutting effects.

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

## Contributing

Contributions are welcome. Fork the repo, make your changes, and submit a pull request.

Before submitting, ensure `cargo test`, `cargo clippy`, and `cargo fmt --check` all pass with zero warnings.

## License

[MIT License](LICENSE)
