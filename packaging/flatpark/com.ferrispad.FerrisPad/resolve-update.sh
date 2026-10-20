#!/usr/bin/env bash
# Update resolver for FerrisPad.
#
# Prints the latest release and its Linux x86_64 executable as JSON on stdout;
# logs go to stderr. FerrisPad tags have no "v" prefix (0.9.6), while the asset
# names carry one (FerrisPad-v0.9.6-linux-amd64). The asset name is built from
# the tag and must match exactly: the release also ships a .sig next to it, a
# .deb and the macOS and Windows builds.
set -euo pipefail

repo="fedro86/ferrispad"

need() { command -v "$1" >/dev/null 2>&1 || { echo "missing command: $1" >&2; exit 1; }; }
need curl; need jq

rel="$(curl -fsSL ${GITHUB_TOKEN:+-H "Authorization: Bearer $GITHUB_TOKEN"} \
        "https://api.github.com/repos/$repo/releases/latest")"

version="$(jq -r '.tag_name | ltrimstr("v")' <<<"$rel")"
date="$(jq -r '.published_at' <<<"$rel" | cut -c1-10)"
asset="FerrisPad-v${version}-linux-amd64"
url="$(jq -r --arg a "$asset" '.assets[] | select(.name == $a) | .browser_download_url' <<<"$rel")"

[ -n "$version" ] && [ -n "$url" ] || { echo "failed to resolve FerrisPad release" >&2; exit 1; }
echo "resolved FerrisPad $version ($date): $url" >&2

jq -n --arg v "$version" --arg d "$date" --arg u "$url" \
  --arg r "https://github.com/$repo/releases/tag/$version" \
  '{version:$v, releaseDate:$d, releaseUrl:$r, sources:[{filename:"FerrisPad-linux-amd64", url:$u}]}'
