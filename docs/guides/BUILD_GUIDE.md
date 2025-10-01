# FerrisPad Build Guide

Complete guide for building FerrisPad binaries for distribution.

## Quick Start

Use the automated build script:

```bash
./build-releases.sh
```

This interactive script will guide you through building for different platforms.

## Manual Build Instructions

### Prerequisites

**All Platforms:**
- Rust toolchain (install from https://rustup.rs/)

**Linux:**
```bash
# Ubuntu/Debian
sudo apt-get install libfltk1.3-dev libfontconfig1-dev libxext-dev \
  libxft-dev libxinerama-dev libxcursor-dev libxrender-dev \
  libxfixes-dev libpango1.0-dev libgl1-mesa-dev libglu1-mesa-dev

# For .deb packaging
cargo install cargo-deb

# For .zip creation
sudo apt-get install zip
```

**macOS:**
```bash
# Install Xcode Command Line Tools
xcode-select --install
```

**Windows:**
- Visual Studio Build Tools or MinGW (for cross-compilation)

---

## Building for Linux

### Option 1: Build .deb Package (Recommended)

```bash
# Install cargo-deb if not already installed
cargo install cargo-deb

# Build the package
cargo deb

# Output will be in: target/debian/ferrispad_0.1.0_amd64.deb
```

The .deb package includes:
- Binary installed to `/usr/bin/FerrisPad`
- Desktop entry for application menu
- Icons in all standard sizes
- Automatic dependency resolution

### Option 2: Build Standalone Binary

```bash
# Build release binary
cargo build --release

# Binary will be at: target/release/FerrisPad

# Create tar.gz for distribution
cd target/release
tar -czf FerrisPad-v0.1.0-linux-x64.tar.gz FerrisPad
```

### Copy to Website

```bash
# For .deb
cp target/debian/ferrispad_0.1.0_amd64.deb \
   docs/assets/binaries/ubuntu/FerrisPad-v0.1.0-ubuntu-amd64.deb

# For tar.gz
cp target/release/FerrisPad-v0.1.0-linux-x64.tar.gz \
   docs/assets/binaries/ubuntu/
```

---

## Building for macOS

### Native Build (on macOS)

```bash
# Build for current architecture
cargo build --release

# Binary will be at: target/release/FerrisPad
```

### Universal Binary (Intel + Apple Silicon)

```bash
# Add targets
rustup target add x86_64-apple-darwin
rustup target add aarch64-apple-darwin

# Build for both architectures
cargo build --release --target x86_64-apple-darwin
cargo build --release --target aarch64-apple-darwin

# Combine into universal binary
lipo -create \
  target/x86_64-apple-darwin/release/FerrisPad \
  target/aarch64-apple-darwin/release/FerrisPad \
  -output FerrisPad-universal

# Create tar.gz
tar -czf FerrisPad-v0.1.0-macos-universal.tar.gz FerrisPad-universal
```

### Create .app Bundle (Optional)

```bash
# Create bundle structure
mkdir -p FerrisPad.app/Contents/{MacOS,Resources}

# Copy binary
cp target/release/FerrisPad FerrisPad.app/Contents/MacOS/

# Create Info.plist
cat > FerrisPad.app/Contents/Info.plist << 'EOF'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleExecutable</key>
    <string>FerrisPad</string>
    <key>CFBundleIdentifier</key>
    <string>com.ferrispad.app</string>
    <key>CFBundleName</key>
    <string>FerrisPad</string>
    <key>CFBundleVersion</key>
    <string>0.1.0</string>
    <key>CFBundleShortVersionString</key>
    <string>0.1.0</string>
    <key>CFBundleIconFile</key>
    <string>ferrispad.icns</string>
</dict>
</plist>
EOF

# Copy icon (requires converting PNG to ICNS)
# Use tools like: png2icns or iconutil
```

### Create .dmg (Optional)

Using `create-dmg`:

```bash
# Install create-dmg
brew install create-dmg

# Create DMG
create-dmg \
  --volname "FerrisPad" \
  --window-pos 200 120 \
  --window-size 600 400 \
  --icon-size 100 \
  --app-drop-link 450 185 \
  FerrisPad-v0.1.0.dmg \
  FerrisPad.app/
```

### Copy to Website

```bash
cp FerrisPad-v0.1.0-macos-universal.tar.gz \
   docs/assets/binaries/macos/
# or
cp FerrisPad-v0.1.0.dmg \
   docs/assets/binaries/macos/
```

---

## Building for Windows

### Option 1: Native Build (on Windows)

```bash
# Build with MSVC toolchain
cargo build --release

# Binary will be at: target\release\FerrisPad.exe

# Create zip (using PowerShell)
Compress-Archive -Path target\release\FerrisPad.exe `
  -DestinationPath FerrisPad-v0.1.0-windows-x64.zip
```

### Option 2: Cross-Compile from Linux

```bash
# Install MinGW
sudo apt-get install mingw-w64

# Add Windows target
rustup target add x86_64-pc-windows-gnu

# Build
cargo build --release --target x86_64-pc-windows-gnu

# Binary will be at: target/x86_64-pc-windows-gnu/release/FerrisPad.exe

# Create zip
cd target/x86_64-pc-windows-gnu/release
zip FerrisPad-v0.1.0-windows-x64.zip FerrisPad.exe
```

**Note:** Cross-compiling Windows builds from Linux may have issues with FLTK dependencies. Native Windows builds are recommended.

### Copy to Website

```bash
cp FerrisPad-v0.1.0-windows-x64.zip \
   docs/assets/binaries/windows/
```

---

## Testing Binaries

### Linux

```bash
# Test standalone binary
./target/release/FerrisPad

# Test .deb installation
sudo dpkg -i target/debian/ferrispad_0.1.0_amd64.deb
ferrispad

# Uninstall
sudo dpkg -r ferrispad
```

### macOS

```bash
# Test binary
./target/release/FerrisPad

# Test .app bundle
open FerrisPad.app
```

### Windows

```powershell
# Test binary
.\target\release\FerrisPad.exe
```

---

## Automated CI/CD Builds

### GitHub Actions Example

Create `.github/workflows/release.yml`:

```yaml
name: Release Builds

on:
  push:
    tags:
      - 'v*'

jobs:
  build-linux:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - uses: actions-rs/toolchain@v1
        with:
          toolchain: stable
      - name: Install dependencies
        run: |
          sudo apt-get update
          sudo apt-get install -y libfltk1.3-dev libfontconfig1-dev
      - name: Install cargo-deb
        run: cargo install cargo-deb
      - name: Build .deb
        run: cargo deb
      - name: Upload artifact
        uses: actions/upload-artifact@v3
        with:
          name: linux-deb
          path: target/debian/*.deb

  build-macos:
    runs-on: macos-latest
    steps:
      - uses: actions/checkout@v3
      - uses: actions-rs/toolchain@v1
        with:
          toolchain: stable
      - name: Build universal binary
        run: |
          rustup target add x86_64-apple-darwin aarch64-apple-darwin
          cargo build --release --target x86_64-apple-darwin
          cargo build --release --target aarch64-apple-darwin
          lipo -create \
            target/x86_64-apple-darwin/release/FerrisPad \
            target/aarch64-apple-darwin/release/FerrisPad \
            -output FerrisPad-universal
          tar -czf FerrisPad-macos-universal.tar.gz FerrisPad-universal
      - name: Upload artifact
        uses: actions/upload-artifact@v3
        with:
          name: macos-binary
          path: FerrisPad-macos-universal.tar.gz

  build-windows:
    runs-on: windows-latest
    steps:
      - uses: actions/checkout@v3
      - uses: actions-rs/toolchain@v1
        with:
          toolchain: stable
      - name: Build
        run: cargo build --release
      - name: Create zip
        run: Compress-Archive -Path target\release\FerrisPad.exe -DestinationPath FerrisPad-windows-x64.zip
      - name: Upload artifact
        uses: actions/upload-artifact@v3
        with:
          name: windows-binary
          path: FerrisPad-windows-x64.zip
```

---

## Version Management

When releasing a new version:

1. Update version in `Cargo.toml`
2. Update version in `docs/index.html`
3. Build all platform binaries
4. Create a git tag: `git tag v0.1.0`
5. Push tag: `git push origin v0.1.0`
6. Upload binaries to website or GitHub Releases

---

## Troubleshooting

### FLTK Build Errors

**Linux:**
```bash
# Make sure all dependencies are installed
sudo apt-get install build-essential cmake
```

**macOS:**
```bash
# Make sure Xcode tools are installed
xcode-select --install
```

### cargo-deb Not Found

```bash
cargo install cargo-deb
```

### Cross-compilation Issues

Windows cross-compilation from Linux is tricky with GUI apps. Recommended approaches:
1. Build natively on Windows
2. Use a Windows VM
3. Use GitHub Actions with windows-latest runner

---

## Distribution Checklist

Before releasing binaries:

- [ ] Test on target platform
- [ ] Verify binary size is reasonable
- [ ] Check that icons/assets are embedded
- [ ] Test installation process
- [ ] Verify uninstallation works (Linux)
- [ ] Create checksums (SHA256)
- [ ] Update website download links
- [ ] Update documentation
- [ ] Tag release in git
- [ ] Create release notes

---

## Questions?

See the main [README.md](README.md) or open an issue on GitHub.