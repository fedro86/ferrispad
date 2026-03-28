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

# Parse flags
AUTO_YES=false
POSITIONAL=()
for arg in "$@"; do
    case "$arg" in
        -y|--yes) AUTO_YES=true ;;
        *) POSITIONAL+=("$arg") ;;
    esac
done

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
if [ ${#POSITIONAL[@]} -eq 0 ]; then
    echo "Usage: $0 [-y|--yes] <new-version>"
    echo ""
    echo "Examples:"
    echo "  $0 0.1.4          # Stable release"
    echo "  $0 0.2.0-beta.1   # Beta release"
    echo "  $0 0.2.0-rc.1     # Release candidate"
    echo "  $0 -y 0.1.4       # Skip confirmation"
    echo ""
    exit 1
fi

NEW_VERSION="${POSITIONAL[0]}"

# Validate version format
if ! [[ "$NEW_VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[a-z]+\.[0-9]+)?$ ]]; then
    echo -e "${RED}✗ Invalid version format: $NEW_VERSION${NC}"
    echo "Version must be X.Y.Z or X.Y.Z-suffix.N (e.g., 0.1.4 or 0.2.0-beta.1)"
    exit 1
fi

echo -e "New version: ${GREEN}${NEW_VERSION}${NC}"
echo ""

# Confirm (skip with -y)
if [ "$AUTO_YES" = false ]; then
    read -p "Update version from $CURRENT_VERSION to $NEW_VERSION? (y/n): " -n 1 -r
    echo ""
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        echo "Cancelled."
        exit 0
    fi
fi

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

# 2. Update docs/js/main.js
echo -e "${YELLOW}→${NC} Updating docs/js/main.js..."
# If NEW_VERSION contains a pre-release tag (rc, beta, alpha), update UNSTABLE_VERSION
if [[ "$NEW_VERSION" == *"-rc"* ]] || [[ "$NEW_VERSION" == *"-beta"* ]] || [[ "$NEW_VERSION" == *"-alpha"* ]]; then
    run_sed "s/const UNSTABLE_VERSION = \".*\"/const UNSTABLE_VERSION = \"$NEW_VERSION\"/" docs/js/main.js
else
    # It's a stable release, update BOTH
    run_sed "s/const STABLE_VERSION = \".*\"/const STABLE_VERSION = \"$NEW_VERSION\"/" docs/js/main.js
    run_sed "s/const UNSTABLE_VERSION = \".*\"/const UNSTABLE_VERSION = \"$NEW_VERSION\"/" docs/js/main.js
fi

# 3. Update docs/index.html
echo -e "${YELLOW}→${NC} Updating docs/index.html..."
# Only update the visible "Latest version" text if it's a STABLE release
if [[ ! "$NEW_VERSION" == *"-rc"* ]] && [[ ! "$NEW_VERSION" == *"-beta"* ]] && [[ ! "$NEW_VERSION" == *"-alpha"* ]]; then
    run_sed "s/Latest version: v[0-9.a-z-]*/Latest version: v$NEW_VERSION/" docs/index.html
    # Also update SEO metadata for stable releases
    run_sed "s/\"softwareVersion\": \"[0-9.a-z-]*\"/\"softwareVersion\": \"$NEW_VERSION\"/" docs/index.html
fi

# 4. Update README.md (point to website for now or stable version if deep linked)
echo -e "${YELLOW}→${NC} Updating README.md..."
# For README, we stick to the new version as it often describes current development
run_sed "s|releases/download/[0-9][^/]*|releases/download/$NEW_VERSION|g" README.md
run_sed "s|FerrisPad-v[0-9][^-]*(-[a-z0-9.]*)?|FerrisPad-v$NEW_VERSION|g" README.md

# 5. Update scripts/build-releases.sh VERSION variable
echo -e "${YELLOW}→${NC} Updating scripts/build-releases.sh..."
run_sed "s/^VERSION=\".*\"/VERSION=\"$NEW_VERSION\"/" scripts/build-releases.sh

echo ""
echo -e "${GREEN}✓ Files updated${NC}"

# Auto-commit the version bump
echo ""
echo "Committing version bump..."
git add Cargo.toml docs/js/main.js docs/index.html README.md scripts/build-releases.sh CHANGELOG.md
git commit -m "chore: bump version to ${NEW_VERSION}"

echo ""
echo -e "${GREEN}✓ Version bumped and committed: ${NEW_VERSION}${NC}"
echo ""
echo -e "${BLUE}Next step:${NC} ./scripts/release.sh"
