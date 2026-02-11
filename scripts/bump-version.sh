#!/bin/bash

# FerrisPad Version Bump Script
# Automatically updates version across all project files

set -e

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Change to project root
cd "$(dirname "$0")/.."

echo -e "${BLUE}🦀 FerrisPad Version Bump Script${NC}"
echo "=================================="
echo ""

# Get current version from Cargo.toml
CURRENT_VERSION=$(grep '^version = ' Cargo.toml | head -1 | sed 's/version = "\(.*\)"/\1/')
echo -e "Current version: ${YELLOW}${CURRENT_VERSION}${NC}"
echo ""

# Prompt for new version
if [ -z "$1" ]; then
    echo "Usage: $0 <new-version>"
    echo ""
    echo "Examples:"
    echo "  $0 0.1.4          # Stable release"
    echo "  $0 0.2.0-beta.1   # Beta release"
    echo "  $0 0.2.0-rc.1     # Release candidate"
    echo ""
    exit 1
fi

NEW_VERSION="$1"

# Validate version format
if ! [[ "$NEW_VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[a-z]+\.[0-9]+)?$ ]]; then
    echo -e "${RED}✗ Invalid version format: $NEW_VERSION${NC}"
    echo "Version must be X.Y.Z or X.Y.Z-suffix.N (e.g., 0.1.4 or 0.2.0-beta.1)"
    exit 1
fi

echo -e "New version: ${GREEN}${NEW_VERSION}${NC}"
echo ""

# Confirm
read -p "Update version from $CURRENT_VERSION to $NEW_VERSION? (y/n): " -n 1 -r
echo ""
if [[ ! $REPLY =~ ^[Yy]$ ]]; then
    echo "Cancelled."
    exit 0
fi

echo ""
echo "Updating files..."
echo ""

# Function to run sed in-place compatibly across macOS and Linux
run_sed() {
    local pattern="$1"
    local file="$2"
    if sed --version &>/dev/null 2>&1; then
        # GNU sed (Linux)
        sed -i -E "$pattern" "$file"
    else
        # BSD sed (macOS)
        sed -i "" -E "$pattern" "$file"
    fi
}

# 1. Update Cargo.toml
echo -e "${YELLOW}→${NC} Updating Cargo.toml..."
run_sed "s/^version = \".*\"/version = \"$NEW_VERSION\"/" Cargo.toml

# 2. Update docs/js/main.js (download URLs)
echo -e "${YELLOW}→${NC} Updating docs/js/main.js..."
# Match version in tag (path segment) and in filename separately to preserve platform names
# Pattern matches versions like 0.1.8 or 0.1.8-rc.1 and stops before the next / or -
run_sed "s|releases/download/[0-9][^/]*|releases/download/$NEW_VERSION|g" docs/js/main.js
run_sed "s|FerrisPad-v[0-9][^-]*(-[a-z0-9.]*)?|FerrisPad-v$NEW_VERSION|g" docs/js/main.js

# 3. Update docs/index.html (version display and download URLs)
echo -e "${YELLOW}→${NC} Updating docs/index.html..."
run_sed "s/Latest version: v[0-9.a-z-]*/Latest version: v$NEW_VERSION/" docs/index.html
# Update SEO metadata
run_sed "s/\"softwareVersion\": \"[0-9.a-z-]*\"/\"softwareVersion\": \"$NEW_VERSION\"/" docs/index.html
# Match version in tag (path segment) and in filename separately to preserve platform names
run_sed "s|releases/download/[0-9][^/]*|releases/download/$NEW_VERSION|g" docs/index.html
run_sed "s|FerrisPad-v[0-9][^-]*(-[a-z0-9.]*)?|FerrisPad-v$NEW_VERSION|g" docs/index.html

# 4. Update README.md (download URLs and version)
echo -e "${YELLOW}→${NC} Updating README.md..."
# Match version in tag (path segment) and in filename separately to preserve platform names
run_sed "s|releases/download/[0-9][^/]*|releases/download/$NEW_VERSION|g" README.md
run_sed "s|FerrisPad-v[0-9][^-]*(-[a-z0-9.]*)?|FerrisPad-v$NEW_VERSION|g" README.md

# 5. Update scripts/build-releases.sh VERSION variable
echo -e "${YELLOW}→${NC} Updating scripts/build-releases.sh..."
run_sed "s/^VERSION=\".*\"/VERSION=\"$NEW_VERSION\"/" scripts/build-releases.sh

echo ""
echo -e "${GREEN}✓ Version updated successfully!${NC}"
echo ""
echo "Files updated:"
echo "  • Cargo.toml"
echo "  • docs/js/main.js"
echo "  • docs/index.html"
echo "  • README.md"
echo "  • scripts/build-releases.sh"
echo ""
echo -e "${BLUE}Next steps:${NC}"
echo "  1. Review changes: git diff"
echo "  2. Commit changes: git add -A && git commit -m \"chore: bump version to $NEW_VERSION\""
echo "  3. Push changes: git push"
echo "  4. Create tag: git tag -a \"$NEW_VERSION\" -m \"Release $NEW_VERSION\""
echo "  5. Push tag: git push origin \"$NEW_VERSION\""
echo ""
echo -e "${YELLOW}GitHub Actions will automatically build and create the release!${NC}"
