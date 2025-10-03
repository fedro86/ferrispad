#!/bin/bash

# FerrisPad Release Script
# Commits version bump and creates release tag

set -e

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Change to project root
cd "$(dirname "$0")/.."

echo -e "${BLUE}🦀 FerrisPad Release Script${NC}"
echo "=============================="
echo ""

# Get version from Cargo.toml
VERSION=$(grep '^version = ' Cargo.toml | head -1 | sed 's/version = "\(.*\)"/\1/')

echo -e "Current version: ${YELLOW}${VERSION}${NC}"
echo ""

# Check for uncommitted changes
if ! git diff-index --quiet HEAD --; then
    echo -e "${YELLOW}⚠ You have uncommitted changes${NC}"
    echo ""
    git status --short
    echo ""
    read -p "Commit these changes and create release tag for v${VERSION}? (y/n): " -n 1 -r
    echo ""
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        echo "Cancelled."
        exit 0
    fi

    # Commit changes
    echo ""
    echo "Staging all changes..."
    git add -A

    echo "Committing..."
    git commit -m "chore: bump version to ${VERSION}"

    echo -e "${GREEN}✓ Changes committed${NC}"
else
    echo -e "${GREEN}✓ No uncommitted changes${NC}"
fi

# Confirm tag creation
echo ""
read -p "Create and push tag ${VERSION}? (y/n): " -n 1 -r
echo ""
if [[ ! $REPLY =~ ^[Yy]$ ]]; then
    echo "Cancelled."
    exit 0
fi

# Push commits first
echo ""
echo "Pushing commits to remote..."
git push

echo -e "${GREEN}✓ Commits pushed${NC}"

# Create tag
echo ""
echo "Creating tag ${VERSION}..."
git tag -a "${VERSION}" -m "Release ${VERSION}"

# Push tag
echo "Pushing tag to remote..."
git push origin "${VERSION}"

echo ""
echo -e "${GREEN}✓ Tag ${VERSION} created and pushed!${NC}"
echo ""
echo "GitHub Actions will now:"
echo "  1. Build binaries for all platforms"
echo "  2. Create GitHub release"
echo "  3. Auto-populate release notes from CHANGELOG.md"
echo ""
echo "Monitor progress: https://github.com/fedro86/ferrispad/actions"
echo "View releases: https://github.com/fedro86/ferrispad/releases"
echo ""
