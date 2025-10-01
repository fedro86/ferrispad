# Release Process

This document explains how to create a new release of FerrisPad with automated builds for all platforms.

## Overview

FerrisPad uses GitHub Actions to automatically build binaries for:
- **Linux** (Ubuntu/Debian amd64) - `.deb` package
- **Windows** (x86_64) - `.zip` archive
- **macOS** (Intel + Apple Silicon universal binary) - `.dmg` installer

## Prerequisites

- Commit and push all changes to the `master` branch
- Ensure version numbers are updated in:
  - `Cargo.toml` (version field)
  - `docs/js/main.js` (download URLs)
  - `docs/index.html` (version display)
  - `README.md` (installation instructions)

## Creating a Release

### 1. Create and Push a Version Tag

#### For Stable Releases

```bash
# Example for version 0.1.1
VERSION="0.1.1"

# Create annotated tag
git tag -a "v${VERSION}" -m "Release v${VERSION}"

# Push the tag to GitHub
git push origin "v${VERSION}"
```

#### For Pre-releases (Beta, RC, Alpha)

Pre-releases are perfect for testing before a stable release. They're marked as "pre-release" on GitHub and can be hidden from the main releases list.

```bash
# Beta release
VERSION="0.2.0-beta.1"
git tag -a "v${VERSION}" -m "Beta release v${VERSION}"
git push origin "v${VERSION}"

# Release Candidate
VERSION="0.2.0-rc.1"
git tag -a "v${VERSION}" -m "Release candidate v${VERSION}"
git push origin "v${VERSION}"

# Alpha release
VERSION="0.2.0-alpha.1"
git tag -a "v${VERSION}" -m "Alpha release v${VERSION}"
git push origin "v${VERSION}"
```

**Pre-release detection:** Tags containing `-alpha`, `-beta`, or `-rc` will automatically be marked as pre-releases in the workflow.

### 2. Automatic Build Process

Once the tag is pushed, GitHub Actions will automatically:

1. **Create a GitHub Release** for the tag
2. **Build on native runners:**
   - Linux binary on `ubuntu-latest`
   - Windows binary on `windows-latest`
   - macOS universal binary on `macos-latest`
3. **Upload artifacts** to the GitHub Release:
   - `FerrisPad-v0.1.1-ubuntu-amd64.deb`
   - `FerrisPad-v0.1.1-windows-x64.zip`
   - `FerrisPad-v0.1.1-macos.dmg`

### 3. Monitor the Build

1. Go to the **Actions** tab in GitHub
2. Click on the "Build and Release" workflow
3. Monitor each job (Linux, Windows, macOS)
4. Build typically takes 5-10 minutes total

### 4. Add Release Notes and Description

Once the build completes, enhance the release with proper documentation:

1. Go to the **Releases** page
2. Find your release and click **Edit**
3. You'll see a basic automated description - replace or enhance it with comprehensive notes

**Note:** The workflow automatically creates a basic release body with download instructions. You should edit this to add detailed what's new, bug fixes, and changes.

#### Release Notes Template

```markdown
## 🦀 FerrisPad v0.1.1

### 🎉 What's New

- Feature: Added syntax highlighting for popular languages
- Feature: New keyboard shortcut Ctrl+F for find functionality
- Improvement: Faster startup time (50% reduction)

### 🐛 Bug Fixes

- Fixed: Window not restoring to previous size on launch
- Fixed: Line numbers misalignment with large fonts
- Fixed: Dark mode not persisting between sessions

### 🔧 Changes

- Updated FLTK to version 1.4.1
- Improved memory usage for large files
- Changed default font to JetBrains Mono

### 📦 Downloads

Choose your platform:

- **Linux**: `FerrisPad-v0.1.1-ubuntu-amd64.deb` (Debian/Ubuntu)
- **Windows**: `FerrisPad-v0.1.1-windows-x64.zip` (Windows 10/11)
- **macOS**: `FerrisPad-v0.1.1-macos.dmg` (Intel + Apple Silicon)

### 📝 Installation

**Linux:**
```bash
sudo dpkg -i FerrisPad-v0.1.1-ubuntu-amd64.deb
```

**Windows:**
Extract the zip and run `FerrisPad.exe`

**macOS:**
Open the DMG and drag to Applications folder

### 🔗 Links

- [Installation Guide](https://github.com/fedro86/ferrispad#installation)
- [Report Issues](https://github.com/fedro86/ferrispad/issues)
- [Website](https://www.ferrispad.com)

---

**Full Changelog**: https://github.com/fedro86/ferrispad/compare/v0.1.0...v0.1.1
```

#### Pre-release Notes Template

For beta/RC/alpha releases, use this template:

```markdown
## 🧪 FerrisPad v0.2.0-beta.1

**⚠️ This is a pre-release version for testing purposes. Not recommended for production use.**

### 🔬 What's Being Tested

- Experimental: New plugin system
- Experimental: Multi-tab support
- Testing: Performance improvements for files > 10MB

### 🐛 Known Issues

- Tab switching may cause occasional flicker
- Plugin API is not finalized and may change
- Some keyboard shortcuts conflict with system shortcuts on Linux

### 💬 Feedback Requested

Please test and report issues:
- Does multi-tab work reliably?
- Any performance regressions?
- Are the keyboard shortcuts intuitive?

### 📦 Downloads

**Note:** These builds are for testing only.

---

**Feedback**: Please report issues at https://github.com/fedro86/ferrispad/issues
```

### 5. Verify the Release

After adding release notes:

1. Verify all three binaries are attached
2. Download and test each binary on its platform
3. Check that the release notes render correctly
4. For pre-releases, ensure the "Pre-release" badge is visible
5. If everything looks good, the release is ready!

## Manual Trigger

You can also manually trigger a build without creating a tag:

1. Go to **Actions** tab
2. Select "Build and Release" workflow
3. Click "Run workflow"
4. This will create draft releases you can test

## Troubleshooting

### Build Fails on Linux
- Check FLTK dependencies are correctly installed
- Review the build logs in Actions tab

### Build Fails on Windows
- FLTK should work out-of-box on Windows
- Check for Rust compilation errors

### Build Fails on macOS
- Ensure `create-dmg` installation succeeds
- Check icon generation steps
- DMG creation may show warnings but should succeed

### Release Already Exists
If you need to recreate a release:
```bash
# Delete the tag locally
git tag -d v0.1.1

# Delete the tag remotely
git push --delete origin v0.1.1

# Delete the release on GitHub (via web interface)
# Then recreate the tag
```

## Version Bump Checklist

Before creating a new release, update:

- [ ] `Cargo.toml` - version = "X.Y.Z"
- [ ] `docs/js/main.js` - update all three download URLs
- [ ] `docs/index.html` - update version display
- [ ] `README.md` - update download URLs
- [ ] `scripts/build-releases.sh` - VERSION variable (if used locally)
- [ ] Commit all changes
- [ ] Create and push tag

## Benefits of GitHub Actions

✅ **Native builds** - Each platform builds on its native runner (no cross-compilation issues)

✅ **Correct architectures** - Linux amd64, Windows x86_64, macOS universal (Intel + ARM)

✅ **Automated** - Push a tag and get all binaries automatically

✅ **Consistent** - Same build environment every time

✅ **Free** - GitHub Actions is free for public repositories

✅ **Fast** - Parallel builds complete in ~5-10 minutes

## Testing Locally

While GitHub Actions handles official releases, you can still build locally for testing:

```bash
# Build for your current platform
cargo build --release

# Run locally
./target/release/FerrisPad

# Create .deb package (Linux only)
cargo install cargo-deb
cargo deb
```

For cross-platform testing, use the GitHub Actions workflow instead of local cross-compilation.

## Quick Reference

### Release Types

| Type | Version Format | Example | Marked as Pre-release? |
|------|---------------|---------|------------------------|
| Stable | `X.Y.Z` | `v0.1.1` | No |
| Beta | `X.Y.Z-beta.N` | `v0.2.0-beta.1` | Yes |
| RC | `X.Y.Z-rc.N` | `v0.2.0-rc.1` | Yes |
| Alpha | `X.Y.Z-alpha.N` | `v0.2.0-alpha.1` | Yes |

### One-Line Release Commands

```bash
# Stable release
VERSION="0.1.1" && git tag -a "v${VERSION}" -m "Release v${VERSION}" && git push origin "v${VERSION}"

# Beta release
VERSION="0.2.0-beta.1" && git tag -a "v${VERSION}" -m "Beta v${VERSION}" && git push origin "v${VERSION}"

# Release candidate
VERSION="0.2.0-rc.1" && git tag -a "v${VERSION}" -m "RC v${VERSION}" && git push origin "v${VERSION}"
```

### Workflow Status

Check build progress: `https://github.com/fedro86/ferrispad/actions`

View releases: `https://github.com/fedro86/ferrispad/releases`
