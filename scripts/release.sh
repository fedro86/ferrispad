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

# ===================================
# Sync website files to master
# ===================================
# When releasing from a feature branch, the website (served from docs/ on master)
# won't reflect the new version. This step cherry-picks the docs changes to master
# so the download buttons (especially "Feeling brave?" for pre-releases) update.

CURRENT_BRANCH=$(git branch --show-current)
MAIN_BRANCH="master"

if [ "$CURRENT_BRANCH" != "$MAIN_BRANCH" ]; then
    echo ""
    echo -e "${YELLOW}You are releasing from branch '${CURRENT_BRANCH}', not '${MAIN_BRANCH}'.${NC}"
    echo "The website is served from docs/ on ${MAIN_BRANCH}."
    echo ""
    read -p "Sync website files (docs/js/main.js) to ${MAIN_BRANCH}? (y/n): " -n 1 -r
    echo ""
    if [[ $REPLY =~ ^[Yy]$ ]]; then
        echo ""
        echo "Switching to ${MAIN_BRANCH}..."
        git checkout "${MAIN_BRANCH}"
        git pull --ff-only origin "${MAIN_BRANCH}"

        echo "Cherry-picking website version update..."
        # Checkout only the website files from the release branch
        git checkout "${CURRENT_BRANCH}" -- docs/js/main.js

        if git diff --quiet --cached docs/js/main.js 2>/dev/null && git diff --quiet docs/js/main.js 2>/dev/null; then
            echo -e "${GREEN}✓ Website files already up to date on ${MAIN_BRANCH}${NC}"
        else
            git add docs/js/main.js
            git commit -m "chore(website): update unstable version to ${VERSION}"
            git push origin "${MAIN_BRANCH}"
            echo -e "${GREEN}✓ Website updated on ${MAIN_BRANCH}${NC}"
        fi

        echo "Switching back to ${CURRENT_BRANCH}..."
        git checkout "${CURRENT_BRANCH}"
    else
        echo -e "${YELLOW}⚠ Skipped. Website won't show the new version until ${MAIN_BRANCH} is updated.${NC}"
    fi
fi

echo ""
echo "GitHub Actions will now:"
echo "  1. Build binaries for all platforms"
echo "  2. Create GitHub release"
echo "  3. Auto-populate release notes from CHANGELOG.md"
echo ""
echo "Monitor progress: https://github.com/fedro86/ferrispad/actions"
echo "View releases: https://github.com/fedro86/ferrispad/releases"
echo ""
