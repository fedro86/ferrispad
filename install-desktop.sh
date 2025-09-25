#!/bin/bash

# Install FerrisPad desktop entry and icons
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "Installing FerrisPad desktop integration..."

# Create necessary directories
mkdir -p ~/.local/share/applications
mkdir -p ~/.local/share/icons/hicolor/{16x16,24x24,32x32,48x48,64x64,128x128,256x256,512x512}/apps

# Install icons in all standard sizes
echo "Installing icons..."
for size in 16x16 24x24 32x32 48x48 64x64 128x128 256x256 512x512; do
    if [ -f "${SCRIPT_DIR}/icons/hicolor/${size}/apps/ferrispad.png" ]; then
        cp "${SCRIPT_DIR}/icons/hicolor/${size}/apps/ferrispad.png" \
           ~/.local/share/icons/hicolor/${size}/apps/ferrispad.png
        echo "  Installed ${size} icon"
    fi
done

# Copy desktop entry with correct paths
sed "s|Icon=.*|Icon=ferrispad|g; s|Exec=.*|Exec=${SCRIPT_DIR}/target/release/FerrisPad|g" \
    "${SCRIPT_DIR}/FerrisPad.desktop" > ~/.local/share/applications/FerrisPad.desktop

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
            "${SCRIPT_DIR}/icons/hicolor/${size}x${size}/apps/ferrispad.png" ferrispad 2>/dev/null || true
    fi
done

echo "✅ FerrisPad desktop integration installed successfully!"
echo ""
echo "The application should now appear in your application menu with the crab icon."
echo "You may need to log out and log back in, or restart your desktop environment"
echo "for the icon to appear in all places."