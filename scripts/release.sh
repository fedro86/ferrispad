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

# Parse flags
AUTO_YES=false
for arg in "$@"; do
    case "$arg" in
        -y|--yes) AUTO_YES=true ;;
    esac
done

# Change to project root
cd "$(dirname "$0")/.."

echo -e "${BLUE}🦀 FerrisPad Release Script${NC}"
echo "=============================="
echo ""

# Get version from Cargo.toml
VERSION=$(grep '^version = ' Cargo.toml | head -1 | sed 's/version = "\(.*\)"/\1/')

echo -e "Current version: ${YELLOW}${VERSION}${NC}"
echo ""

# Abort on uncommitted changes — bump-version.sh should have committed already
if ! git diff-index --quiet HEAD --; then
    echo -e "${RED}✗ Uncommitted changes detected:${NC}"
    echo ""
    git status --short
    echo ""
    echo "Run ./scripts/bump-version.sh first, or commit your changes manually."
    exit 1
fi

echo -e "${GREEN}✓ Working tree clean${NC}"

# Confirm tag creation (skip with -y)
if [ "$AUTO_YES" = false ]; then
    echo ""
    read -p "Create and push tag ${VERSION}? (y/n): " -n 1 -r
    echo ""
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        echo "Cancelled."
        exit 0
    fi
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
echo "The website (www.ferrispad.com, separate fedro86/fedro-websites repo) reads"
echo "the latest tag from the GitHub API, so it picks up ${VERSION} by itself."
echo ""
echo "Monitor progress: https://github.com/fedro86/ferrispad/actions"
echo "View releases: https://github.com/fedro86/ferrispad/releases"
echo ""
