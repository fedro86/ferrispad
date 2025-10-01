#!/bin/bash

# Generate FerrisPad icons with rounded corners from source logo
PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && cd .. && pwd)"
SOURCE_LOGO="${PROJECT_ROOT}/docs/assets/logo-transparent.png"

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

# Function to create professional icon with gradient background and rounded corners
create_professional_icon() {
    local size=$1
    local output=$2
    local radius=$((size / 5))  # Professional squircle radius

    # Create icon with gradient background, centered logo, and rounded corners
    if command -v magick &> /dev/null; then
        magick -size ${size}x${size} \
            gradient:'#FF6B35-#F7931E' \
            \( "$SOURCE_LOGO" -resize $((size * 70 / 100))x$((size * 70 / 100)) \) \
            -gravity center -composite \
            \( +clone -alpha extract \
               -draw "fill black polygon 0,0 0,${radius} ${radius},0 fill white circle ${radius},${radius} ${radius},0" \
               \( +clone -flip \) -compose Multiply -composite \
               \( +clone -flop \) -compose Multiply -composite \
            \) -alpha off -compose CopyOpacity -composite \
            "$output"
    else
        convert -size ${size}x${size} \
            gradient:'#FF6B35-#F7931E' \
            \( "$SOURCE_LOGO" -resize $((size * 70 / 100))x$((size * 70 / 100)) \) \
            -gravity center -composite \
            \( +clone -alpha extract \
               -draw "fill black polygon 0,0 0,${radius} ${radius},0 fill white circle ${radius},${radius} ${radius},0" \
               \( +clone -flip \) -compose Multiply -composite \
               \( +clone -flop \) -compose Multiply -composite \
            \) -alpha off -compose CopyOpacity -composite \
            "$output"
    fi
}

# Generate icons for each size
for size in 16 24 32 48 64 128 256 512; do
    echo "  Generating ${size}x${size} icon..."
    create_professional_icon $size "${PROJECT_ROOT}/icons/hicolor/${size}x${size}/apps/ferrispad.png"
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