#!/bin/bash

# FerrisPad Release Build Script
# Builds binaries for multiple platforms

set -e

# Change to project root directory
cd "$(dirname "$0")/.."

VERSION="0.1.0"
PROJECT_NAME="FerrisPad"
BINARY_DIR="docs/assets/binaries"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo "🦀 FerrisPad Release Builder v${VERSION}"
echo "========================================"

# Detect current OS
detect_os() {
    case "$(uname -s)" in
        Linux*)     echo "linux";;
        Darwin*)    echo "macos";;
        CYGWIN*|MINGW*|MSYS*) echo "windows";;
        *)          echo "unknown";;
    esac
}

CURRENT_OS=$(detect_os)
echo "📍 Current OS: ${CURRENT_OS}"
echo ""

# Function to build for current platform
build_native() {
    echo -e "${GREEN}Building native binary...${NC}"
    cargo build --release

    if [ $? -eq 0 ]; then
        echo -e "${GREEN}✓ Native build successful${NC}"
        return 0
    else
        echo -e "${RED}✗ Native build failed${NC}"
        return 1
    fi
}

# Linux binary and .deb package
build_linux() {
    echo -e "${YELLOW}Building Linux binary...${NC}"

    # Build the binary
    cargo build --release

    if [ $? -eq 0 ]; then
        echo -e "${GREEN}✓ Linux binary built${NC}"

        # Check if cargo-deb is installed
        if command -v cargo-deb &> /dev/null; then
            echo -e "${YELLOW}Creating .deb package...${NC}"
            cargo deb

            # Create output directory
            mkdir -p "${BINARY_DIR}/ubuntu"

            # Find and copy the .deb file
            DEB_FILE=$(find target/debian -name "*.deb" | head -n 1)
            if [ -f "$DEB_FILE" ]; then
                cp "$DEB_FILE" "${BINARY_DIR}/ubuntu/${PROJECT_NAME}-v${VERSION}-ubuntu-amd64.deb"
                echo -e "${GREEN}✓ .deb package created: ${BINARY_DIR}/ubuntu/${PROJECT_NAME}-v${VERSION}-ubuntu-amd64.deb${NC}"
            fi
        else
            echo -e "${YELLOW}⚠ cargo-deb not installed. Install with: cargo install cargo-deb${NC}"
            echo -e "${YELLOW}Creating tar.gz instead...${NC}"

            mkdir -p "${BINARY_DIR}/ubuntu"
            cd target/release
            tar -czf "../../${BINARY_DIR}/ubuntu/${PROJECT_NAME}-v${VERSION}-linux-x64.tar.gz" ${PROJECT_NAME}
            cd ../..
            echo -e "${GREEN}✓ Linux tar.gz created${NC}"
        fi
    else
        echo -e "${RED}✗ Linux build failed${NC}"
        return 1
    fi
}

# macOS binary and .dmg
build_macos() {
    echo -e "${YELLOW}Building macOS binary...${NC}"

    if [ "$CURRENT_OS" != "macos" ]; then
        echo -e "${RED}✗ macOS builds can only be created on macOS${NC}"
        return 1
    fi

    # Build for current architecture
    cargo build --release

    if [ $? -eq 0 ]; then
        echo -e "${GREEN}✓ macOS binary built${NC}"

        mkdir -p "${BINARY_DIR}/macos"

        # Create a simple tar.gz (DMG creation requires additional tools)
        cd target/release
        tar -czf "../../${BINARY_DIR}/macos/${PROJECT_NAME}-v${VERSION}-macos-$(uname -m).tar.gz" ${PROJECT_NAME}
        cd ../..

        echo -e "${GREEN}✓ macOS tar.gz created: ${BINARY_DIR}/macos/${PROJECT_NAME}-v${VERSION}-macos-$(uname -m).tar.gz${NC}"
        echo -e "${YELLOW}ℹ For .dmg creation, use tools like create-dmg or appdmg${NC}"
    else
        echo -e "${RED}✗ macOS build failed${NC}"
        return 1
    fi
}

# Windows binary (cross-compile from Linux)
build_windows() {
    echo -e "${YELLOW}Building Windows binary...${NC}"

    # Check if mingw target is installed
    if ! rustup target list | grep -q "x86_64-pc-windows-gnu (installed)"; then
        echo -e "${YELLOW}Installing Windows target...${NC}"
        rustup target add x86_64-pc-windows-gnu
    fi

    # Check if mingw is available
    if ! command -v x86_64-w64-mingw32-gcc &> /dev/null; then
        echo -e "${RED}✗ MinGW not found. Install with: sudo apt-get install mingw-w64${NC}"
        return 1
    fi

    cargo build --release --target x86_64-pc-windows-gnu

    if [ $? -eq 0 ]; then
        echo -e "${GREEN}✓ Windows binary built${NC}"

        mkdir -p "${BINARY_DIR}/windows"

        # Create zip file
        if command -v zip &> /dev/null; then
            cd target/x86_64-pc-windows-gnu/release
            zip "../../../${BINARY_DIR}/windows/${PROJECT_NAME}-v${VERSION}-windows-x64.zip" ${PROJECT_NAME}.exe
            cd ../../..
            echo -e "${GREEN}✓ Windows zip created: ${BINARY_DIR}/windows/${PROJECT_NAME}-v${VERSION}-windows-x64.zip${NC}"
        else
            echo -e "${YELLOW}⚠ zip not found. Copying .exe only${NC}"
            cp target/x86_64-pc-windows-gnu/release/${PROJECT_NAME}.exe "${BINARY_DIR}/windows/${PROJECT_NAME}.exe"
        fi
    else
        echo -e "${RED}✗ Windows build failed${NC}"
        echo -e "${YELLOW}Note: Windows builds from Linux may have issues with FLTK dependencies${NC}"
        return 1
    fi
}

# Main menu
show_menu() {
    echo ""
    echo "Choose build target:"
    echo "1) Native (current platform)"
    echo "2) Linux (.deb package)"
    echo "3) macOS (.tar.gz)"
    echo "4) Windows (.zip) - cross-compile"
    echo "5) All platforms (if possible)"
    echo "6) Exit"
    echo ""
    read -p "Enter choice [1-6]: " choice

    case $choice in
        1) build_native ;;
        2) build_linux ;;
        3) build_macos ;;
        4) build_windows ;;
        5)
            build_linux
            if [ "$CURRENT_OS" == "macos" ]; then
                build_macos
            fi
            # Windows cross-compile is optional
            read -p "Attempt Windows cross-compile? (y/n): " answer
            if [ "$answer" == "y" ]; then
                build_windows
            fi
            ;;
        6)
            echo "Exiting..."
            exit 0
            ;;
        *)
            echo -e "${RED}Invalid choice${NC}"
            show_menu
            ;;
    esac
}

# Check if cargo is installed
if ! command -v cargo &> /dev/null; then
    echo -e "${RED}✗ Cargo not found. Please install Rust: https://rustup.rs/${NC}"
    exit 1
fi

# Run menu
show_menu

echo ""
echo -e "${GREEN}🎉 Build process complete!${NC}"
echo -e "Binaries are in: ${BINARY_DIR}"
echo ""
echo "Next steps:"
echo "1. Test the binaries on their respective platforms"
echo "2. Upload them to the appropriate directories"
echo "3. Update version numbers in docs/index.html if needed"
echo "4. Deploy the website"