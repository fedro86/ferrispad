# 🦀 FerrisPad

<div align="center">
  <img src="assets/crab-notepad-emoji-8bit.png" alt="FerrisPad Logo" width="200" style="border-radius: 50%; border: 3px solid #333;"/>
  <p><em>A blazingly fast, minimalist notepad written in Rust</em></p>
</div>

## Overview

FerrisPad is a simple, ultra-fast text editor built with Rust and FLTK. Named after **Ferris** 🦀 (the beloved mascot of the Rust programming language), this notepad embodies the Rust philosophy of being fast, reliable, and memory-safe.

It's designed for those who want something that just works - no bells and whistles, no feature bloat, just a clean, responsive notepad that opens instantly and gets out of your way.

This isn't meant to change the landscape of text editors. It's meant to be that reliable tool you can always count on when you need to quickly jot something down or edit a file without waiting for heavy editors to load.

> **About Ferris**: Ferris is the unofficial mascot of Rust, a friendly orange crab that represents the community values of the language: safety, speed, and concurrency. Just like Ferris helps make Rust development enjoyable, FerrisPad aims to make text editing simple and delightful.

## Features

✨ **Lightning Fast** - Opens instantly, no splash screens or loading delays
🎨 **Light & Dark Modes** - Automatically detects your system theme
📝 **Clean Interface** - Minimal, distraction-free design
📄 **Essential Tools** - Line numbers, word wrap, font customization
💾 **Smart Saving** - Quick save (Ctrl+S) and Save As (Ctrl+Shift+S)
🔍 **Find & Replace** - Search and replace text with case-sensitive option
📁 **Multi-Format Support** - Save as .txt, .md, .rs, .py, .json, and more
⚙️ **Persistent Settings** - Save your preferences (theme, font, view options)
🔔 **Auto-Update Check** - Checks GitHub once per day for new versions (can be disabled)
🔒 **Privacy-First** - No telemetry, user control, transparent data usage
⌨️ **Keyboard Shortcuts** - Ctrl+N, Ctrl+O, Ctrl+S, Ctrl+F, Ctrl+H, Ctrl+Q
🖋️ **Font Options** - Multiple monospace fonts and sizes
👁️ **Blinking Cursor** - Clear visual feedback
🦀 **Proper Icons** - Beautiful crab mascot icons in all standard sizes
🖥️ **Desktop Integration** - Application menu entry and system icon support
📄 **File Title Display** - Shows filename (or "Untitled") in window title
ℹ️ **About Dialog** - Version info, copyright, license, and helpful links

## Screenshots

<div align="center">
  <h3>Light Mode</h3>
  <img src="assets/screenshot-adv-1-light.png" alt="FerrisPad Light Mode" width="600"/>
  <p><em>Clean, bright interface with line numbers and monospace font</em></p>
</div>

<div align="center">
  <h3>Dark Mode</h3>
  <img src="assets/screenshot-adv-1-dark.png" alt="FerrisPad Dark Mode" width="600"/>
  <p><em>Easy on the eyes dark theme with syntax-friendly colors</em></p>
</div>

## Installation

### Download Pre-Built Binaries

**Visit our website:** 👉 **[www.ferrispad.com](https://www.ferrispad.com)** 👈

Download ready-to-use binaries for:
- **Linux** - `.deb` package or standalone binary
- **macOS** - Universal binary (Intel + Apple Silicon)
- **Windows** - Portable `.zip` archive

### Quick Install (Linux/Ubuntu)

```bash
# Download and install the .deb package
wget https://www.ferrispad.com/assets/binaries/ubuntu/FerrisPad-v0.9.0-rc.3-amd64.deb
sudo dpkg -i FerrisPad-v0.9.0-rc.3-amd64.deb

# Launch from application menu or run
FerrisPad
```

### Windows Security Warning

When running FerrisPad on Windows for the first time, you may see a "Windows protected your PC" warning or Windows Defender may quarantine the executable. This is **normal** for open source software that is not code-signed with a commercial certificate (which costs $100-400/year).

**FerrisPad is safe to run**. The source code is completely open and auditable on GitHub.

**If you see "Windows protected your PC" warning:**
1. Click "More info" on the warning dialog
2. Click "Run anyway"

**If Windows Defender deletes the executable:**
1. Open **Windows Security** (search in Start menu)
2. Go to **Virus & threat protection** → **Manage settings**
3. Scroll down to **Exclusions** → Click **Add or remove exclusions**
4. Click **Add an exclusion** → Choose **Folder**
5. Select the folder where you extracted FerrisPad
6. Re-extract the `.zip` file to restore the executable

**Alternative (before extraction):**
1. Right-click the downloaded `.zip` file
2. Select "Properties"
3. Check the "Unblock" box at the bottom
4. Click "OK"
5. Extract and run normally

This warning appears for many open source projects including GIMP, Audacity, and other community-developed software.

### macOS Security Warning

When opening FerrisPad on macOS for the first time, you may see a warning that the app "cannot be opened because it is from an unidentified developer" or "cannot be opened because Apple cannot check it for malicious software." This is **normal** for open source software not signed with an Apple Developer certificate (which costs $99/year).

**FerrisPad is safe to run**. The source code is completely open and auditable on GitHub.

**To open FerrisPad (choose one method):**

**Method 1 (Terminal - Most Reliable):**
```bash
xattr -cr /Applications/FerrisPad.app
```
This removes the quarantine flag. After running this, open FerrisPad normally from Applications.

**Method 2 (System Settings):**
1. Try to open FerrisPad normally (it will be blocked)
2. Go to System Settings → Privacy & Security
3. Scroll down to the Security section
4. Click "Open Anyway" next to the FerrisPad message
5. Click "Open" to confirm

**Method 3 (Right-click - May Not Work):**
1. Right-click (or Control-click) FerrisPad.app in Applications
2. Select "Open" from the menu
3. If an "Open" button appears in the dialog, click it
4. Note: This method doesn't work reliably on newer macOS versions

This warning appears for many open source projects including GIMP, Audacity, Blender, and other community-developed software that aren't code-signed by Apple.

### Build from Source

#### Prerequisites
- Rust (latest stable version)
- FLTK dependencies for your system

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

#### Build Commands

```bash
git clone https://github.com/fedro86/ferrispad
cd ferrispad
cargo build --release
```

**Build Distribution Packages:**
```bash
# Interactive build menu for all platforms
./scripts/build-releases.sh

# Or build .deb package directly (Linux)
cargo install cargo-deb
cargo deb
```

See [BUILD_GUIDE.md](docs/guides/BUILD_GUIDE.md) for detailed build instructions

### Install Desktop Integration (Linux)

For proper icon display and application menu integration when building from source:
```bash
./scripts/install-desktop.sh
```

This will:
- Install FerrisPad icons in all standard sizes (16x16 to 512x512)
- Create desktop entry for application menu
- Register with the desktop environment
- Enable proper icon display in taskbar/dock

To uninstall:
```bash
./scripts/uninstall-desktop.sh
```

**Note**: The `.deb` package automatically handles desktop integration.

### Run
```bash
# Run from source
cargo run --release

# Or run the compiled binary
./target/release/FerrisPad

# Or launch from application menu (after installing desktop integration)
```

## Usage

### Keyboard Shortcuts

**File Operations:**
- **Ctrl+N** - New file
- **Ctrl+O** - Open file
- **Ctrl+S** - Save (quick save to existing file)
- **Ctrl+Shift+S** - Save As (save with new name/location)
- **Ctrl+Q** - Quit

**Edit Operations:**
- **Ctrl+F** - Find text
- **Ctrl+H** - Find & Replace

### Menu Options

**File Menu:**
- **Settings...** - Configure theme, font, view preferences, and auto-update behavior (saved automatically)

**Edit Menu:**
- **Find...** - Search for text with case-sensitive option
- **Replace...** - Find and replace text with Replace and Replace All

**View Menu:**
- **Toggle Line Numbers** - Show/hide line numbers
- **Toggle Word Wrap** - Enable/disable text wrapping
- **Toggle Dark Mode** - Switch between light and dark themes

**Format Menu:**
- **Font** - Choose from Screen (Bold), Courier, or Helvetica Mono
- **Font Size** - Select from Small (12), Medium (16), or Large (20)

**Help Menu:**
- **About FerrisPad** - View version, copyright, license, and helpful links
- **Check for Updates...** - Manually check for new versions on GitHub

### Privacy & Updates

FerrisPad includes an optional auto-update checker that:
- Checks GitHub's public API once per 24 hours for new releases
- Can be completely disabled in **File → Settings**
- Sends **no personal data** or telemetry
- Is fully transparent and auditable (open source)
- Notifies you with a subtle banner when updates are available
- Never downloads or installs anything automatically

The update check is privacy-first by design, following best practices from projects like VS Code, Firefox, and Notepad++.

## Building Your Own Features

FerrisPad is intentionally simple and well-structured, making it easy to extend. The codebase is clean and documented, perfect for:

- **Learning Rust GUI development** with FLTK
- **Adding your own features** (syntax highlighting, tabs, etc.)
- **Customizing the interface** to your needs
- **Understanding cross-platform desktop app architecture**

Key files:
- `src/main.rs` - Main application logic and UI
- `src/settings.rs` - Settings persistence module
- `assets/` - Application icons and resources
- `scripts/` - Build and installation scripts
- `docs/` - Website and documentation

See [BUILD_GUIDE.md](BUILD_GUIDE.md) for building distribution packages

## Technology Stack

- **Language**: Rust 🦀
- **GUI Framework**: FLTK (Fast Light Toolkit)
- **Image Handling**: Embedded PNG assets
- **Platform Support**: Linux, macOS, Windows

## Philosophy

FerrisPad follows the Unix philosophy: do one thing and do it well. It's not trying to be VSCode or Vim or Emacs. It's just a notepad that:

- Starts instantly
- Handles text editing reliably
- Stays out of your way
- Provides a solid foundation for customization

## Troubleshooting

### Icons Not Showing
If the crab icon doesn't appear in your taskbar or application menu:

1. **Run the installation script**:
   ```bash
   ./install-desktop.sh
   ```

2. **Clear icon cache**:
   ```bash
   rm -rf ~/.cache/icon-theme.cache
   gtk-update-icon-cache -f -t ~/.local/share/icons/hicolor/
   ```

3. **Restart your desktop environment** or log out and log back in

4. **Check if icons were installed**:
   ```bash
   ls ~/.local/share/icons/hicolor/32x32/apps/ferrispad.png
   ```

### Application Not in Menu
If FerrisPad doesn't appear in your application menu:
```bash
update-desktop-database ~/.local/share/applications
```

## Contributing

This project welcomes contributions! Whether you want to:
- Fix bugs
- Add simple features
- Improve documentation
- Optimize performance

Feel free to fork the repository and submit pull requests.

## License

[MIT License](LICENSE) - Feel free to use this project as a starting point for your own text editor adventures!

---

<div align="center">
  <p>Built with ❤️ and 🦀 by developers who believe software should be fast and simple</p>
</div>