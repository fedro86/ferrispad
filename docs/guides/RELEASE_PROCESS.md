# Release Process

This document explains how to create a new release of FerrisPad with automated builds for all platforms.

## Overview

FerrisPad uses GitHub Actions to automatically build binaries for:
- **Linux** (Ubuntu/Debian amd64) - `.deb` package
- **Windows** (x86_64) - `.zip` archive
- **macOS** (Intel + Apple Silicon universal binary) - `.dmg` installer

## Prerequisites

- Commit and push all changes to the `master` branch

## Creating a Release

### 1. Bump the Version

Use the automated version bump script to update all files:

```bash
# For a stable release
./scripts/bump-version.sh 0.1.4

# For a pre-release
./scripts/bump-version.sh 0.2.0-beta.1
```

This script automatically updates:
- `Cargo.toml` - version field
- `docs/js/main.js` - download URLs
- `docs/index.html` - version display and download URLs
- `README.md` - installation instructions and download URLs
- `scripts/build-releases.sh` - VERSION variable

The script automatically commits the changes after updating all files. Use `-y` to skip the confirmation prompt.

```bash
# With confirmation prompt
./scripts/bump-version.sh 0.1.4

# Skip confirmation (CI/automation)
./scripts/bump-version.sh -y 0.1.4
```

### 2. Create and Push the Release Tag

#### For Stable Releases

```bash
# Example for version 0.1.2
VERSION="0.1.2"

# Create annotated tag (without 'v' prefix to match existing tags)
git tag -a "${VERSION}" -m "Release ${VERSION}"

# Push the tag to GitHub
git push origin "${VERSION}"

VERSION="0.1.5-rc.1" && git tag -a "${VERSION}" -m "Release ${VERSION}" && git push origin "${VERSION}" 
```

**Note:** This project uses tags **without** the "v" prefix (e.g., `0.1.2` instead of `v0.1.2`) to match the existing tagging convention.

#### For Pre-releases (Beta, RC, Alpha)

Pre-releases are perfect for testing before a stable release. They're marked as "pre-release" on GitHub and can be hidden from the main releases list.

```bash
# Beta release
VERSION="0.2.0-beta.1"
git tag -a "${VERSION}" -m "Beta release ${VERSION}"
git push origin "${VERSION}"

# Release Candidate
VERSION="0.2.0-rc.1"
git tag -a "${VERSION}" -m "Release candidate ${VERSION}"
git push origin "${VERSION}"

# Alpha release
VERSION="0.2.0-alpha.1"
git tag -a "${VERSION}" -m "Alpha release ${VERSION}"
git push origin "${VERSION}"
```

**Pre-release detection:** Tags containing `-alpha`, `-beta`, or `-rc` will automatically be marked as pre-releases in the workflow.

### 3. Automatic Build Process

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

### 4. Monitor the Build

1. Go to the **Actions** tab in GitHub
2. Click on the "Build and Release" workflow
3. Monitor each job (Linux, Windows, macOS)
4. Build typically takes 5-10 minutes total

### 5. Release Notes (Fully Automated! ✨)

**GitHub Actions automatically extracts release notes from CHANGELOG.md!**

The workflow:
1. Reads the version section from CHANGELOG.md
2. Populates the GitHub release body automatically
3. No manual editing needed!

**How it works:**
- Workflow extracts the `[X.Y.Z]` section from CHANGELOG.md
- Preserves all markdown formatting
- Falls back to generic message if version not found in CHANGELOG

**Result:** Your release notes are ready immediately after the build completes!

#### Manual Override (Optional)

If you want to add additional notes or customize:

1. Go to the **Releases** page
2. Find your release and click **Edit**
3. The notes from CHANGELOG.md are already there
4. Add any additional information (testing notes, screenshots, etc.)

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

### 6. Sign the Release Binaries (Automated)

**Important:** FerrisPad's auto-updater verifies signatures before installing updates. Without signatures, users cannot auto-update.

Signing is **fully automated** by the `sign-binaries` job in the GitHub Actions workflow. After the build jobs complete, the workflow:

1. Checks out the `ferrispad-plugins` repo and builds the signer tool
2. Decodes the `SIGNING_KEY` secret to a temporary file
3. Signs all platform binaries using `plugin-signer sign-release`
4. Uploads `.sig` files as release artifacts alongside the binaries
5. Cleans up the key material

**No manual action is needed** — `.sig` files appear automatically in the GitHub release.

#### GitHub Secret Setup (One-Time)

The `SIGNING_KEY` secret must be configured in the repository:

```bash
# Encode the existing signing key as base64
base64 < ~/.config/ferrispad/signing/plugin_signing_key.bin

# Add to GitHub: Settings → Secrets and variables → Actions → New repository secret
# Name: SIGNING_KEY
# Value: (paste the base64 output)
```

#### Manual Signing Fallback

If CI signing fails or you need to sign manually:

```bash
cd ~/code-folder/continuous_learning/ferrispad-plugins/tools/signer

# Sign Linux binary
./target/release/plugin-signer sign-release ~/Downloads/FerrisPad-linux-amd64 0.9.1 linux-amd64

# Sign macOS binary
./target/release/plugin-signer sign-release ~/Downloads/FerrisPad-macos-universal 0.9.1 macos-universal

# Sign Windows binary
./target/release/plugin-signer sign-release ~/Downloads/FerrisPad-windows-x64.exe 0.9.1 windows-x64.exe
```

Then upload the `.sig` files to the GitHub release manually.

**Platform identifiers** (must match exactly):
| Platform | Identifier |
|----------|------------|
| Linux | `linux-amd64` |
| macOS | `macos-universal` |
| Windows | `windows-x64.exe` |

**Note:** The signing key is stored at `~/.config/ferrispad/signing/plugin_signing_key.bin`. Keep this key secure and backed up. The CI uses a base64-encoded copy stored as a GitHub Secret.

### 7. Verify the Release

After signing and uploading:

1. Verify all three binaries AND their `.sig` files are attached
2. Download and test each binary on its platform
3. Test the auto-updater from an older version to verify signature verification works
4. Check that the release notes render correctly
5. For pre-releases, ensure the "Pre-release" badge is visible
6. If everything looks good, the release is ready!

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

- [ ] `CHANGELOG.md` - Add new version section with changes (do this FIRST!)
- [ ] Run `./scripts/bump-version.sh X.Y.Z` (updates Cargo.toml, docs, README, build script — auto-commits)
- [ ] Run `./scripts/release.sh` (pushes, creates tag, syncs website)
- [ ] Wait for GitHub Actions to build binaries
- [ ] **Verify `.sig` files** are attached to the release (automated by CI)
- [ ] Auto-populate release notes from CHANGELOG.md (use `gh` one-liner)

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
| Stable | `X.Y.Z` | `0.1.2` | No |
| Beta | `X.Y.Z-beta.N` | `0.2.0-beta.1` | Yes |
| RC | `X.Y.Z-rc.N` | `0.2.0-rc.1` | Yes |
| Alpha | `X.Y.Z-alpha.N` | `0.2.0-alpha.1` | Yes |

**Note:** Tags do NOT include the "v" prefix (e.g., `0.1.2` not `v0.1.2`)

### Quick Release Workflow (Fully Automated)

```bash
# 1. Update CHANGELOG.md manually
# Edit CHANGELOG.md to add new [X.Y.Z] section with all changes

# 2. Bump version, update all files, and commit
./scripts/bump-version.sh X.Y.Z

# 3. Tag, push, and sync website (automated script)
./scripts/release.sh
```

That's it! `bump-version.sh` updates all files and commits. `release.sh` handles:
- Creating and pushing the annotated tag
- **Syncing website files to master** (if releasing from a feature branch — see below)

Then GitHub Actions will automatically:
- Build binaries for all platforms (5-10 minutes)
- Create GitHub release (marked as pre-release for `-rc`, `-beta`, `-alpha` tags)
- Auto-populate release notes from CHANGELOG.md

### Releasing from a Feature Branch

The website is served from `docs/` on the `master` branch. When you release from a different branch (e.g., `enhancement/some-feature`), the website won't reflect the new version automatically.

`release.sh` detects this and offers to sync `docs/js/main.js` to `master`, so the "Feeling brave?" unstable download button appears on the live website immediately.

The sync process:
1. Switches to `master` and pulls latest
2. Copies `docs/js/main.js` from the release branch
3. Commits and pushes to `master`
4. Switches back to the release branch

You can skip this step if you plan to merge the branch into master soon.

**Manual alternative (after bump-version.sh has committed):**
```bash
# Push and create tag
git push && VERSION="X.Y.Z" && git tag -a "${VERSION}" -m "Release ${VERSION}" && git push origin "${VERSION}"
```

### One-Line Commands (After version bump)

```bash
# Stable release tag
VERSION="0.1.4" && git tag -a "${VERSION}" -m "Release ${VERSION}" && git push origin "${VERSION}"

# Beta release tag
VERSION="0.2.0-beta.1" && git tag -a "${VERSION}" -m "Beta ${VERSION}" && git push origin "${VERSION}"

# Release candidate tag
VERSION="0.2.0-rc.1" && git tag -a "${VERSION}" -m "RC ${VERSION}" && git push origin "${VERSION}"
```

### Workflow Status

Check build progress: `https://github.com/fedro86/ferrispad/actions`

View releases: `https://github.com/fedro86/ferrispad/releases`
