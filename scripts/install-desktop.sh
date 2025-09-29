#!/bin/bash

# Install FerrisPad desktop entry and icons
# Resolve PROJECT_ROOT - handles both direct execution and symlinks
SCRIPT_PATH="${BASH_SOURCE[0]}"
# If it's a symlink, resolve it
if [ -L "$SCRIPT_PATH" ]; then
    SCRIPT_PATH="$(readlink "$SCRIPT_PATH")"
fi
PROJECT_ROOT="$(cd "$(dirname "$SCRIPT_PATH")" && cd .. && pwd)"

echo "Installing FerrisPad desktop integration..."

# Generate icons if they don't exist or if generate-icons.sh is newer
if [ ! -f "${PROJECT_ROOT}/icons/hicolor/48x48/apps/ferrispad.png" ] || \
   [ "${PROJECT_ROOT}/scripts/generate-icons.sh" -nt "${PROJECT_ROOT}/icons/hicolor/48x48/apps/ferrispad.png" ]; then
    echo "Generating icons with rounded corners..."
    if [ -x "${PROJECT_ROOT}/scripts/generate-icons.sh" ]; then
        "${PROJECT_ROOT}/scripts/generate-icons.sh"
    else
        echo "Warning: scripts/generate-icons.sh not found or not executable"
        echo "Icons may have sharp edges"
    fi
fi

# Create necessary directories
mkdir -p ~/.local/share/applications
mkdir -p ~/.local/share/icons/hicolor/{16x16,24x24,32x32,48x48,64x64,128x128,256x256,512x512}/apps

# Install icons in all standard sizes
echo "Installing icons..."
for size in 16x16 24x24 32x32 48x48 64x64 128x128 256x256 512x512; do
    if [ -f "${PROJECT_ROOT}/icons/hicolor/${size}/apps/ferrispad.png" ]; then
        cp "${PROJECT_ROOT}/icons/hicolor/${size}/apps/ferrispad.png" \
           ~/.local/share/icons/hicolor/${size}/apps/ferrispad.png
        echo "  Installed ${size} icon"
    fi
done

# Copy desktop entry with correct paths
sed "s|Icon=.*|Icon=ferrispad|g; s|Exec=.*|Exec=${PROJECT_ROOT}/target/release/FerrisPad|g" \
    "${PROJECT_ROOT}/FerrisPad.desktop" > ~/.local/share/applications/FerrisPad.desktop

# Make desktop entry executable
chmod +x ~/.local/share/applications/FerrisPad.desktop

# Update icon cache
echo "Updating icon cache..."
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
    gtk-update-icon-cache -f -t ~/.local/share/icons/hicolor/ 2>/dev/null || true
fi

# Update desktop database
if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database ~/.local/share/applications 2>/dev/null || true
fi

# Register icons with XDG
echo "Registering with desktop environment..."
for size in 16 24 32 48 64 128 256 512; do
    if command -v xdg-icon-resource >/dev/null 2>&1; then
        xdg-icon-resource install --size ${size} \
            "${PROJECT_ROOT}/icons/hicolor/${size}x${size}/apps/ferrispad.png" ferrispad 2>/dev/null || true
    fi
done

echo "✅ FerrisPad desktop integration installed successfully!"
echo ""
echo "The application should now appear in your application menu with the crab icon."
echo "You may need to log out and log back in, or restart your desktop environment"
echo "for the icon to appear in all places."