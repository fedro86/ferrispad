#!/usr/bin/env bash
# Update resolver for FerrisPad.
#
# Prints the latest release and its Linux executables (x86_64 and aarch64) as
# JSON on stdout; logs go to stderr. FerrisPad tags have no "v" prefix (0.9.8),
# while the asset names carry one (FerrisPad-v0.9.8-linux-amd64). The asset
# names are built from the tag and must match exactly: the release also ships
# a .sig next to each binary, .deb packages and the macOS and Windows builds.
set -euo pipefail

repo="fedro86/ferrispad"

need() { command -v "$1" >/dev/null 2>&1 || { echo "missing command: $1" >&2; exit 1; }; }
need curl; need jq

rel="$(curl -fsSL ${GITHUB_TOKEN:+-H "Authorization: Bearer $GITHUB_TOKEN"} \
        "https://api.github.com/repos/$repo/releases/latest")"

version="$(jq -r '.tag_name | ltrimstr("v")' <<<"$rel")"
date="$(jq -r '.published_at' <<<"$rel" | cut -c1-10)"
asset_url() {
  jq -r --arg a "FerrisPad-v${version}-linux-$1" \
    '.assets[] | select(.name == $a) | .browser_download_url' <<<"$rel"
}
url_x86_64="$(asset_url amd64)"
url_aarch64="$(asset_url arm64)"

[ -n "$version" ] && [ -n "$url_x86_64" ] && [ -n "$url_aarch64" ] || {
  echo "failed to resolve FerrisPad release" >&2; exit 1; }
echo "resolved FerrisPad $version ($date): $url_x86_64 $url_aarch64" >&2

jq -n --arg v "$version" --arg d "$date" --arg ux "$url_x86_64" --arg ua "$url_aarch64" \
  --arg r "https://github.com/$repo/releases/tag/$version" \
  '{version:$v, releaseDate:$d, releaseUrl:$r, sources:[
     {filename:"FerrisPad-linux", arch:"x86_64",  url:$ux},
     {filename:"FerrisPad-linux", arch:"aarch64", url:$ua}]}'
