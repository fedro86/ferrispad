#!/bin/bash

# Generate FerrisPad icons with rounded corners from source logo
PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && cd .. && pwd)"
SOURCE_LOGO="${PROJECT_ROOT}/assets/crab-notepad-emoji-8bit.png"

echo "Generating FerrisPad icons with rounded corners..."

# Check if source logo exists
if [ ! -f "$SOURCE_LOGO" ]; then
    echo "Error: Source logo not found at $SOURCE_LOGO"
    exit 1
fi

# Check if ImageMagick is available
if ! command -v convert >/dev/null 2>&1; then
    echo "Error: ImageMagick (convert) is required but not installed"
    echo "Install with: sudo apt-get install imagemagick"
    exit 1
fi

# Create icons directory structure
for size in 16x16 24x24 32x32 48x48 64x64 128x128 256x256 512x512; do
    mkdir -p "${PROJECT_ROOT}/icons/hicolor/${size}/apps"
done

# Function to create rounded corner mask
create_rounded_mask() {
    local size=$1
    local radius=$((size / 8))  # Adjust this ratio for more/less rounding

    convert -size ${size}x${size} xc:none \
        -draw "roundrectangle 0,0 $((size-1)),$((size-1)) ${radius},${radius}" \
        -alpha extract \
        /tmp/mask_${size}.png
}

# Generate icons for each size
for size in 16 24 32 48 64 128 256 512; do
    echo "  Generating ${size}x${size} icon..."

    # Create rounded corner mask for this size
    create_rounded_mask $size

    # Resize source image and apply rounded corners
    convert "$SOURCE_LOGO" \
        -resize ${size}x${size} \
        -gravity center \
        -extent ${size}x${size} \
        /tmp/mask_${size}.png \
        -alpha off \
        -compose CopyOpacity \
        -composite \
        "${PROJECT_ROOT}/icons/hicolor/${size}x${size}/apps/ferrispad.png"

    # Clean up temporary mask
    rm -f /tmp/mask_${size}.png

    echo "    ✅ Created ${PROJECT_ROOT}/icons/hicolor/${size}x${size}/apps/ferrispad.png"
done

# Generate .ico file for Windows
echo "  Generating ferrispad.ico for Windows..."
convert "${PROJECT_ROOT}/icons/hicolor/256x256/apps/ferrispad.png" \
        "${PROJECT_ROOT}/icons/hicolor/128x128/apps/ferrispad.png" \
        "${PROJECT_ROOT}/icons/hicolor/64x64/apps/ferrispad.png" \
        "${PROJECT_ROOT}/icons/hicolor/48x48/apps/ferrispad.png" \
        "${PROJECT_ROOT}/icons/hicolor/32x32/apps/ferrispad.png" \
        "${PROJECT_ROOT}/icons/hicolor/16x16/apps/ferrispad.png" \
        "${PROJECT_ROOT}/ferrispad.ico"

echo "✅ All icons generated successfully with rounded corners!"
echo ""
echo "Icons created:"
echo "  - ${PROJECT_ROOT}/icons/hicolor/*/apps/ferrispad.png (all sizes)"
echo "  - ${PROJECT_ROOT}/ferrispad.ico (Windows icon)"
echo ""
echo "To install: ./install-desktop.sh"