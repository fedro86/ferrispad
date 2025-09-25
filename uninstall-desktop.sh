#!/bin/bash

# Uninstall FerrisPad desktop integration

echo "Uninstalling FerrisPad desktop integration..."

# Remove desktop entry
if [ -f ~/.local/share/applications/FerrisPad.desktop ]; then
    rm ~/.local/share/applications/FerrisPad.desktop
    echo "  Removed desktop entry"
fi

# Remove icons
for size in 16x16 24x24 32x32 48x48 64x64 128x128 256x256 512x512; do
    if [ -f ~/.local/share/icons/hicolor/${size}/apps/ferrispad.png ]; then
        rm ~/.local/share/icons/hicolor/${size}/apps/ferrispad.png
        echo "  Removed ${size} icon"
    fi
done

# Unregister icons with XDG
echo "Unregistering from desktop environment..."
for size in 16 24 32 48 64 128 256 512; do
    if command -v xdg-icon-resource >/dev/null 2>&1; then
        xdg-icon-resource uninstall --size ${size} ferrispad 2>/dev/null || true
    fi
done

# Update icon cache
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
    gtk-update-icon-cache -f -t ~/.local/share/icons/hicolor/ 2>/dev/null || true
fi

# Update desktop database
if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database ~/.local/share/applications 2>/dev/null || true
fi

echo "✅ FerrisPad desktop integration uninstalled successfully!"