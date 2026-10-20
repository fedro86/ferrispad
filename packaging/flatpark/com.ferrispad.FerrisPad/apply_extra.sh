#!/bin/sh
set -eu

# Runs offline at install time inside org.freedesktop.Platform. The release
# asset is FerrisPad's official Linux x86_64 build: one self-contained
# executable (Lua and the syntax definitions are compiled in), so there is
# nothing to unpack. Move it to the stable path the wrapper runs.

extra_root="${EXTRA_ROOT:-/app/extra}"
cd "$extra_root"

[ -f FerrisPad-linux-amd64 ] || {
  echo "missing extra-data: FerrisPad-linux-amd64" >&2
  exit 1
}

rm -f FerrisPad
mv FerrisPad-linux-amd64 FerrisPad
chmod 755 FerrisPad
