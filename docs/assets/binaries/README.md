# FerrisPad Binary Releases

This directory contains compiled binaries for different operating systems. Store your release binaries here for download from the website.

## Directory Structure

```
binaries/
├── windows/
│   └── FerrisPad-v0.1.0-windows-x64.zip
├── macos/
│   └── FerrisPad-v0.1.0-macos-universal.dmg
└── ubuntu/
    └── FerrisPad-v0.1.0-ubuntu-amd64.deb
```

## Naming Convention

Use the following naming pattern for consistency:

- **Windows**: `FerrisPad-v{VERSION}-windows-{ARCH}.zip`
  - Example: `FerrisPad-v0.1.0-windows-x64.zip`

- **macOS**: `FerrisPad-v{VERSION}-macos-{ARCH}.dmg` or `.tar.gz`
  - Example: `FerrisPad-v0.1.0-macos-universal.dmg`
  - Example: `FerrisPad-v0.1.0-macos-arm64.tar.gz`

- **Ubuntu/Linux**: `FerrisPad-v{VERSION}-ubuntu-{ARCH}.deb`
  - Example: `FerrisPad-v0.1.0-ubuntu-amd64.deb`
  - Example: `FerrisPad-v0.1.0-ubuntu-arm64.deb`

## Building Binaries

### Windows (from Windows or cross-compile)

```bash
# Native build on Windows
cargo build --release

# Package as zip
cd target/release
7z a FerrisPad-v0.1.0-windows-x64.zip FerrisPad.exe

# Cross-compile from Linux (requires mingw)
cargo build --release --target x86_64-pc-windows-gnu
```

### macOS (from macOS)

```bash
# Build universal binary (Intel + Apple Silicon)
cargo build --release --target x86_64-apple-darwin
cargo build --release --target aarch64-apple-darwin

# Combine into universal binary
lipo -create \
  target/x86_64-apple-darwin/release/FerrisPad \
  target/aarch64-apple-darwin/release/FerrisPad \
  -output FerrisPad-universal

# Create .app bundle (optional but recommended)
# See macOS packaging documentation

# Create DMG or tar.gz
tar -czf FerrisPad-v0.1.0-macos-universal.tar.gz FerrisPad-universal
```

### Ubuntu/Linux

```bash
# Build binary
cargo build --release

# Install cargo-deb for .deb packaging
cargo install cargo-deb

# Create .deb package
cargo deb

# The .deb will be in target/debian/
mv target/debian/ferrispad_*.deb docs/assets/binaries/ubuntu/FerrisPad-v0.1.0-ubuntu-amd64.deb
```

## Updating Website

After adding new binaries:

1. Place the files in the appropriate subdirectory
2. Update version numbers in `docs/index.html` if needed
3. Update the download links to match the new filenames
4. Test the download links locally before deploying

## Git LFS (Recommended for Large Files)

Binary files can be large. Consider using Git LFS (Large File Storage):

```bash
# Install Git LFS
git lfs install

# Track binary files (already configured in .gitignore)
git lfs track "docs/assets/binaries/**/*.zip"
git lfs track "docs/assets/binaries/**/*.dmg"
git lfs track "docs/assets/binaries/**/*.deb"

# Add and commit
git add .gitattributes
git commit -m "Track binary releases with Git LFS"
```

## Alternative: External Hosting

For better performance and to keep the repository size small, consider hosting binaries externally:

- **GitHub Releases**: Attach binaries to GitHub release tags
- **GitLab Releases**: Use GitLab's release feature
- **Cloud Storage**: AWS S3, Google Cloud Storage, or similar
- **CDN**: Cloudflare R2, Bunny CDN, etc.

Then update the download links in `docs/index.html` to point to the external URLs.

## Notes

- Binary files are ignored by Git (see `.gitignore`)
- Keep old versions for users who haven't upgraded
- Consider adding checksums (SHA256) for security verification
- Provide release notes with each version