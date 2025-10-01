# Cross-Compiling FerrisPad for Windows from Linux

This guide shows how to build Windows binaries (.exe) from Linux using MinGW.

## ⚠️ Important Note

Cross-compiling GUI applications (especially those using FLTK) from Linux to Windows can be challenging and may not work perfectly. The recommended approach is to build natively on Windows, but you can try cross-compilation as an alternative.

## Prerequisites

### 1. Install MinGW Cross-Compiler

```bash
# Ubuntu/Debian
sudo apt-get update
sudo apt-get install -y mingw-w64

# Verify installation
x86_64-w64-mingw32-gcc --version
```

### 2. Add Windows Target to Rust

```bash
# Add the Windows GNU target
rustup target add x86_64-pc-windows-gnu

# Verify it's installed
rustup target list | grep windows
```

### 3. Configure Cargo for Cross-Compilation

Create or edit `~/.cargo/config.toml`:

```bash
mkdir -p ~/.cargo
nano ~/.cargo/config.toml
```

Add this configuration:

```toml
[target.x86_64-pc-windows-gnu]
linker = "x86_64-w64-mingw32-gcc"
ar = "x86_64-w64-mingw32-ar"
```

## Building for Windows

### Method 1: Using the Build Script

```bash
# Run the automated build script
./scripts/build-releases.sh

# Choose option 4 (Windows)
```

### Method 2: Manual Build

```bash
# Build for Windows
cargo build --release --target x86_64-pc-windows-gnu

# The binary will be at:
# target/x86_64-pc-windows-gnu/release/FerrisPad.exe
```

### Method 3: With Optimizations

```bash
# Build with size optimizations
RUSTFLAGS="-C target-feature=+crt-static" \
  cargo build --release --target x86_64-pc-windows-gnu
```

## Creating Distribution Package

```bash
# Navigate to the build directory
cd target/x86_64-pc-windows-gnu/release

# Create zip archive
zip FerrisPad-v0.1.0-windows-x64.zip FerrisPad.exe

# Move to binaries directory
mv FerrisPad-v0.1.0-windows-x64.zip \
   ../../../docs/assets/binaries/windows/

cd ../../..
```

Or use 7-zip for better compression:

```bash
sudo apt-get install p7zip-full

cd target/x86_64-pc-windows-gnu/release
7z a -tzip FerrisPad-v0.1.0-windows-x64.zip FerrisPad.exe
```

## Common Issues and Solutions

### Issue 1: FLTK Dependencies Not Found

**Problem:** Cross-compilation fails because FLTK needs Windows-specific libraries.

**Solution:** The `fltk-rs` crate tries to build FLTK from source, which may fail. Options:

1. **Use a Docker container** with pre-built Windows FLTK libraries
2. **Build on actual Windows** (recommended for GUI apps)
3. **Use Windows in a VM** and build natively

### Issue 2: Missing DLL Errors on Windows

**Problem:** The .exe runs but complains about missing DLLs.

**Solution:** Build with static linking:

```bash
RUSTFLAGS="-C target-feature=+crt-static -C link-args=-static" \
  cargo build --release --target x86_64-pc-windows-gnu
```

### Issue 3: Binary Works but GUI Doesn't Display

**Problem:** The .exe runs but the window doesn't appear or crashes.

**Solution:** This is common with cross-compiled GUI apps. Your options:

1. Build natively on Windows
2. Use GitHub Actions with a Windows runner
3. Test on actual Windows hardware

## Testing the Windows Binary

You can't run the .exe directly on Linux. Options:

### Option 1: Wine (Limited)

```bash
sudo apt-get install wine64

wine target/x86_64-pc-windows-gnu/release/FerrisPad.exe
```

**Note:** Wine support for FLTK applications is limited and may not work.

### Option 2: Windows VM

1. Install VirtualBox or VMware
2. Create a Windows 10/11 VM
3. Copy the .exe to the VM
4. Test it there

### Option 3: GitHub Actions

Use the automated CI/CD workflow to build and test on real Windows.

## Alternative: Use GitHub Actions for Windows Builds

The most reliable way to build Windows binaries is using GitHub Actions with a Windows runner. Here's a workflow example:

```yaml
# .github/workflows/build-windows.yml
name: Build Windows Binary

on:
  push:
    tags:
      - 'v*'
  workflow_dispatch:

jobs:
  build-windows:
    runs-on: windows-latest
    steps:
      - uses: actions/checkout@v3

      - name: Install Rust
        uses: actions-rs/toolchain@v1
        with:
          toolchain: stable

      - name: Build
        run: cargo build --release

      - name: Create ZIP
        run: |
          cd target/release
          Compress-Archive -Path FerrisPad.exe -DestinationPath FerrisPad-windows-x64.zip

      - name: Upload artifact
        uses: actions/upload-artifact@v3
        with:
          name: windows-binary
          path: target/release/FerrisPad-windows-x64.zip
```

## Quick Commands Summary

```bash
# 1. Install MinGW
sudo apt-get install -y mingw-w64

# 2. Add Windows target
rustup target add x86_64-pc-windows-gnu

# 3. Configure Cargo
echo '[target.x86_64-pc-windows-gnu]
linker = "x86_64-w64-mingw32-gcc"
ar = "x86_64-w64-mingw32-ar"' >> ~/.cargo/config.toml

# 4. Build
cargo build --release --target x86_64-pc-windows-gnu

# 5. Package
cd target/x86_64-pc-windows-gnu/release
zip FerrisPad-v0.1.0-windows-x64.zip FerrisPad.exe
mv FerrisPad-v0.1.0-windows-x64.zip ../../../docs/assets/binaries/windows/
```

## Recommended Approach

For a production-ready Windows binary, I recommend:

1. **Use GitHub Actions** with a Windows runner (most reliable)
2. **Build on actual Windows** machine (second best)
3. **Cross-compile from Linux** (works for simple CLI apps, risky for GUI apps)

Cross-compilation is worth trying for experimentation, but for distributing to users, build natively on Windows or use CI/CD with real Windows runners.

## Resources

- [rust-cross Documentation](https://github.com/cross-rs/cross)
- [FLTK-RS Windows Guide](https://github.com/fltk-rs/fltk-rs#windows)
- [Cargo Configuration](https://doc.rust-lang.org/cargo/reference/config.html)

## Need Help?

If you encounter issues:
1. Check the FLTK-rs issue tracker
2. Try building a simple "Hello World" example first
3. Consider using native Windows builds instead