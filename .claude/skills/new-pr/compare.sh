#!/usr/bin/env bash
# Before / after screenshots of a scenario, master vs this branch (new-pr §2).
#
#   .claude/skills/new-pr/compare.sh <scenario.sh> <outdir>
#       [--before <ref>] [--font N] [--size WxH] [--keep]
#
# Run it from the ticket's worktree. It
#   1. builds the "before" binary in a throwaway worktree of <ref>
#      (default origin/master; e.g. a release tag such as 0.9.10-rc.2),
#   2. builds the "after" binary from this worktree (uncommitted changes
#      included),
#   3. runs the same scenario on both with shoot.sh, light and dark, under
#      shoot.sh's isolation barriers,
#   4. writes one labelled side-by-side montage per shot and theme,
#      <outdir>/<shot>-<theme>-compare.png ("before | after", at most
#      1600 px wide), ready to inline in the PR page.
# The single shots stay in <outdir>/before/ and <outdir>/after/, with the
# logs. <outdir> must be visible to the host (e.g. under
# ~/Code/personal/ferrispad-wt/), not the sandbox's /tmp. The before
# worktree is removed at the end unless --keep is given.
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
usage() { echo "usage: compare.sh <scenario.sh> <outdir> [--before <ref>] [--font N] [--size WxH] [--keep]" >&2; exit 2; }
[ $# -ge 2 ] || usage
scen=$(realpath "$1"); out=$2; shift 2
before_ref=origin/master; font=16; size=1100x740; keep=""
while [ $# -gt 0 ]; do
  case "$1" in
    --before) before_ref=$2; shift 2 ;;
    --font) font=$2; shift 2 ;;
    --size) size=$2; shift 2 ;;
    --keep) keep=1; shift ;;
    *) usage ;;
  esac
done
[ -f "$scen" ] || { echo "no scenario: $scen" >&2; exit 2; }
case "$(realpath -m "$out")" in
  "$HOME"/*) ;;
  *) echo "outdir must be under \$HOME (host-visible): $out" >&2; exit 2 ;;
esac
mkdir -p "$out"; out=$(realpath "$out")

top=$(git rev-parse --show-toplevel)
before_sha=$(git rev-parse --short=7 "$before_ref^{commit}")
after_sha=$(git rev-parse --short=7 HEAD)
[ -z "$(git status --porcelain --untracked-files=no)" ] || after_sha="$after_sha+changes"
before_wt=$(dirname "$top")/compare-$before_sha

host() {
  if [ -e /.flatpak-info ]; then flatpak-spawn --host bash -lc "$1"; else bash -lc "$1"; fi
}

# Build a worktree's debug binary. A failed build fails the comparison (the
# pipe keeps cargo's status), and a binary older than any source file of its
# worktree is refused: a montage must never label a stale build. (cargo does
# not relink when nothing changed, so "newer than the sources" is the test,
# not "newer than the build start".)
build() {
  local wt=$1 bin=$1/target/debug/FerrisPad newest
  host "set -o pipefail; cd $(printf %q "$wt") && nix develop -c cargo build 2>&1 | tail -3" \
    || { echo "compare.sh: the build failed in $wt" >&2; exit 1; }
  [ -x "$bin" ] || { echo "compare.sh: no binary after the build: $bin" >&2; exit 1; }
  newest=$(cd "$wt" && find src Cargo.toml Cargo.lock $( [ -e build.rs ] && echo build.rs ) -type f -printf '%T@\n' | sort -n | tail -1)
  if [ "$(stat -c %Y "$bin")" -lt "${newest%.*}" ]; then
    echo "compare.sh: $bin is older than its sources: refusing to label a stale build" >&2
    exit 1
  fi
}

echo "before: $before_ref ($before_sha) in $before_wt"
if [ ! -d "$before_wt" ]; then
  git worktree add --detach "$before_wt" "$before_sha" >/dev/null
fi
# Remove the throwaway worktree on every exit, a failed build included.
[ -n "$keep" ] || trap 'git -C "$top" worktree remove --force "$before_wt" 2>/dev/null || true' EXIT
build "$before_wt"
echo "after: $after_sha in $top"
build "$top"

status=0
for theme in light dark; do
  "$here/shoot.sh" "$before_wt/target/debug/FerrisPad" "$out/before" "$scen" --theme "$theme" --font "$font" --size "$size" || status=1
  "$here/shoot.sh" "$top/target/debug/FerrisPad" "$out/after" "$scen" --theme "$theme" --font "$font" --size "$size" || status=1
done

# Side-by-side montages, labelled with the refs, in Noto Sans.
host "set -e
noto=\$(nix build --no-link --print-out-paths nixpkgs#noto-fonts)/share/fonts/noto
font=\$noto/NotoSans.ttf
cd $(printf %q "$out")
for b in before/*.png; do
  n=\$(basename \"\$b\" .png); a=after/\$n.png
  [ -f \"\$a\" ] || { echo \"no after shot for \$n\"; continue; }
  nix shell nixpkgs#imagemagick -c montage -font \"\$font\" -pointsize 22 -background '#d8dde3' \\
    -label $(printf %q "before · $before_ref ($before_sha)") \"\$b\" \\
    -label $(printf %q "after · $after_sha") \"\$a\" \\
    -tile 2x1 -geometry +12+12 \"\$n-compare.png\"
  nix shell nixpkgs#imagemagick -c magick \"\$n-compare.png\" -resize '1600x>' -strip \"\$n-compare.png\"
  # Montages are for reading, so a palette is fine; single shots stay lossless.
  nix shell nixpkgs#pngquant -c pngquant --quality 80-95 --strip --force --output \"\$n-compare.png\" \"\$n-compare.png\" || true
  echo \"montage: $(printf %q "$out")/\$n-compare.png\"
done"

exit "$status"
