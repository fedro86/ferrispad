#!/bin/sh
set -eu

# Runs offline at install time inside org.freedesktop.Platform. The release
# asset is FerrisPad's official Linux build for this architecture (x86_64 or
# aarch64; both are pinned under the same filename): one self-contained
# executable (Lua and the syntax definitions are compiled in), so there is
# nothing to unpack. Move it to the stable path the wrapper runs.

extra_root="${EXTRA_ROOT:-/app/extra}"
cd "$extra_root"

[ -f FerrisPad-linux ] || {
  echo "missing extra-data: FerrisPad-linux" >&2
  exit 1
}

rm -f FerrisPad
mv FerrisPad-linux FerrisPad
chmod 755 FerrisPad
