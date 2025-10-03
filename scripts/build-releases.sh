#!/bin/bash

# FerrisPad Release Build Script
# Builds binaries for multiple platforms

set -e

# Change to project root directory
cd "$(dirname "$0")/.."

VERSION="0.1.6-rc.1"
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

        # Create .app bundle
        echo -e "${YELLOW}Creating .app bundle...${NC}"
        APP_BUNDLE="${PROJECT_NAME}.app"
        rm -rf "$APP_BUNDLE"
        mkdir -p "${APP_BUNDLE}/Contents/MacOS"
        mkdir -p "${APP_BUNDLE}/Contents/Resources"

        # Copy binary
        cp "target/release/${PROJECT_NAME}" "${APP_BUNDLE}/Contents/MacOS/"

        # Create Info.plist
        cat > "${APP_BUNDLE}/Contents/Info.plist" << EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleExecutable</key>
    <string>${PROJECT_NAME}</string>
    <key>CFBundleIdentifier</key>
    <string>com.ferrispad.editor</string>
    <key>CFBundleName</key>
    <string>${PROJECT_NAME}</string>
    <key>CFBundleVersion</key>
    <string>${VERSION}</string>
    <key>CFBundleShortVersionString</key>
    <string>${VERSION}</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleIconFile</key>
    <string>icon.icns</string>
    <key>NSHighResolutionCapable</key>
    <true/>
</dict>
</plist>
EOF

        # Generate .icns icon with rounded corners (macOS standard sizes)
        echo -e "${YELLOW}Generating .icns icon...${NC}"
        if command -v sips &> /dev/null && [ -f "docs/assets/logo-transparent.png" ]; then
            ICONSET_DIR="${PROJECT_NAME}.iconset"
            rm -rf "$ICONSET_DIR"
            mkdir -p "$ICONSET_DIR"

            # Function to create macOS-style icon with background and rounded corners
            create_rounded_icon() {
                local size=$1
                local output=$2
                local radius=$((size / 5))  # macOS-like squircle radius

                if command -v magick &> /dev/null; then
                    # Create icon with gradient background, centered logo, and rounded corners
                    magick -size ${size}x${size} \
                        gradient:'#FF6B35-#F7931E' \
                        \( docs/assets/logo-transparent.png -resize $((size * 70 / 100))x$((size * 70 / 100)) \) \
                        -gravity center -composite \
                        \( +clone -alpha extract \
                           -draw "fill black polygon 0,0 0,${radius} ${radius},0 fill white circle ${radius},${radius} ${radius},0" \
                           \( +clone -flip \) -compose Multiply -composite \
                           \( +clone -flop \) -compose Multiply -composite \
                        \) -alpha off -compose CopyOpacity -composite \
                        "$output"
                elif command -v convert &> /dev/null; then
                    # Fallback to convert command (older ImageMagick)
                    convert -size ${size}x${size} \
                        gradient:'#FF6B35-#F7931E' \
                        \( docs/assets/logo-transparent.png -resize $((size * 70 / 100))x$((size * 70 / 100)) \) \
                        -gravity center -composite \
                        \( +clone -alpha extract \
                           -draw "fill black polygon 0,0 0,${radius} ${radius},0 fill white circle ${radius},${radius} ${radius},0" \
                           \( +clone -flip \) -compose Multiply -composite \
                           \( +clone -flop \) -compose Multiply -composite \
                        \) -alpha off -compose CopyOpacity -composite \
                        "$output"
                else
                    # Fallback to simple resize if ImageMagick not available
                    echo -e "${YELLOW}⚠ ImageMagick not found. Install with: brew install imagemagick${NC}"
                    sips -z $size $size "docs/assets/logo-transparent.png" --out "$output" &>/dev/null
                fi
            }

            # Generate all required icon sizes for .icns
            create_rounded_icon 16 "${ICONSET_DIR}/icon_16x16.png"
            create_rounded_icon 32 "${ICONSET_DIR}/icon_16x16@2x.png"
            create_rounded_icon 32 "${ICONSET_DIR}/icon_32x32.png"
            create_rounded_icon 64 "${ICONSET_DIR}/icon_32x32@2x.png"
            create_rounded_icon 128 "${ICONSET_DIR}/icon_128x128.png"
            create_rounded_icon 256 "${ICONSET_DIR}/icon_128x128@2x.png"
            create_rounded_icon 256 "${ICONSET_DIR}/icon_256x256.png"
            create_rounded_icon 512 "${ICONSET_DIR}/icon_256x256@2x.png"
            create_rounded_icon 512 "${ICONSET_DIR}/icon_512x512.png"
            create_rounded_icon 1024 "${ICONSET_DIR}/icon_512x512@2x.png"

            # Convert iconset to icns
            iconutil -c icns "$ICONSET_DIR" -o "${APP_BUNDLE}/Contents/Resources/icon.icns"
            rm -rf "$ICONSET_DIR"
            echo -e "${GREEN}✓ .icns icon created with rounded corners${NC}"
        else
            echo -e "${YELLOW}⚠ Icon generation skipped (requires sips and source icon)${NC}"
        fi

        echo -e "${GREEN}✓ .app bundle created${NC}"

        # Create DMG
        echo -e "${YELLOW}Creating .dmg...${NC}"

        # Check if create-dmg is installed
        if ! command -v create-dmg &> /dev/null; then
            echo -e "${YELLOW}⚠ create-dmg not found. Installing via Homebrew...${NC}"
            if command -v brew &> /dev/null; then
                brew install create-dmg
            else
                echo -e "${RED}✗ Homebrew not found. Please install create-dmg manually:${NC}"
                echo -e "${YELLOW}  brew install create-dmg${NC}"
                echo -e "${YELLOW}Creating .zip as fallback...${NC}"
                zip -r "${BINARY_DIR}/macos/${PROJECT_NAME}-v${VERSION}-macos.zip" "$APP_BUNDLE"
                echo -e "${GREEN}✓ macOS .zip created: ${BINARY_DIR}/macos/${PROJECT_NAME}-v${VERSION}-macos.zip${NC}"
                return 0
            fi
        fi

        # Create DMG with create-dmg
        create-dmg \
            --volname "${PROJECT_NAME}" \
            --volicon "${APP_BUNDLE}/Contents/Resources/icon.icns" \
            --window-pos 200 120 \
            --window-size 600 400 \
            --icon-size 100 \
            --icon "${PROJECT_NAME}.app" 175 190 \
            --hide-extension "${PROJECT_NAME}.app" \
            --app-drop-link 425 190 \
            "${BINARY_DIR}/macos/${PROJECT_NAME}-v${VERSION}-macos.dmg" \
            "$APP_BUNDLE"

        if [ $? -eq 0 ]; then
            echo -e "${GREEN}✓ macOS .dmg created: ${BINARY_DIR}/macos/${PROJECT_NAME}-v${VERSION}-macos.dmg${NC}"
            echo -e "${YELLOW}ℹ Users can mount the DMG and drag to Applications folder${NC}"
        else
            echo -e "${YELLOW}⚠ DMG creation failed, creating .zip as fallback...${NC}"
            zip -r "${BINARY_DIR}/macos/${PROJECT_NAME}-v${VERSION}-macos.zip" "$APP_BUNDLE"
            echo -e "${GREEN}✓ macOS .zip created: ${BINARY_DIR}/macos/${PROJECT_NAME}-v${VERSION}-macos.zip${NC}"
        fi
    else
        echo -e "${RED}✗ macOS build failed${NC}"
        return 1
    fi
}

# Windows binary (cross-compile from Linux/macOS)
build_windows() {
    echo -e "${YELLOW}Building Windows binary...${NC}"

    # Check if mingw target is installed
    if ! rustup target list | grep -q "x86_64-pc-windows-gnu (installed)"; then
        echo -e "${YELLOW}Installing Windows target...${NC}"
        rustup target add x86_64-pc-windows-gnu
    fi

    # Check if mingw is available
    if ! command -v x86_64-w64-mingw32-gcc &> /dev/null; then
        echo -e "${RED}✗ MinGW not found.${NC}"
        if [ "$CURRENT_OS" == "macos" ]; then
            echo -e "${YELLOW}Install with: brew install mingw-w64${NC}"
        else
            echo -e "${YELLOW}Install with: sudo apt-get install mingw-w64${NC}"
        fi
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