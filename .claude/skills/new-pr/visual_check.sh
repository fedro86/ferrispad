#!/usr/bin/env bash
# Technical visual check of a PR page (new-pr skill, §7 condition 1).
#
#   .claude/skills/new-pr/visual_check.sh docs/pr/PR-<n>-<slug>.html <out-dir>
#
# Renders the page in headless Chromium four times (1280 and 390 px wide,
# prefers-color-scheme light and dark). For each render it prints
# scrollWidth/innerWidth and any chip longer than 40 characters, and saves a
# full-page screenshot to <out-dir>. Look at the four PNGs yourself: the
# script cannot see a box whose content runs outside it.
#
# It runs inside the VSCode Flatpak sandbox, with the Playwright
# chrome-headless-shell in ~/.cache/ms-playwright/ (CHROME overrides it).
# The page itself is never modified: the measuring script goes into a copy
# in <out-dir>. Exit status 1 if any render overflows or has a long chip.
set -euo pipefail

page=${1:?usage: visual_check.sh <page.html> <out-dir>}
out=${2:?usage: visual_check.sh <page.html> <out-dir>}
chrome=${CHROME:-$(command ls -d "$HOME"/.cache/ms-playwright/chromium_headless_shell-*/chrome-headless-shell-linux64/chrome-headless-shell 2>/dev/null | sort -V | tail -1)}
[ -x "$chrome" ] || { echo "no chrome-headless-shell found; set CHROME" >&2; exit 2; }

mkdir -p "$out"
probe="$out/probe.html"
# The probe writes its measurements into <title>, which --dump-dom prints.
measure='<script>
var long=[].slice.call(document.querySelectorAll(".chip")).map(function(c){return c.textContent.trim()}).filter(function(t){return t.length>40});
document.title=JSON.stringify({sw:document.documentElement.scrollWidth,iw:window.innerWidth,sh:document.documentElement.scrollHeight,dark:matchMedia("(prefers-color-scheme: dark)").matches,long:long});
</script>'
python3 -I -c 'import sys
src = open(sys.argv[1], encoding="utf-8").read()
i = src.rfind("</body>")
assert i >= 0, "no </body>"
open(sys.argv[2], "w", encoding="utf-8").write(src[:i] + sys.argv[3] + src[i:])' "$page" "$probe" "$measure"

run() { timeout 60 "$chrome" --no-sandbox --disable-gpu --hide-scrollbars "$@" 2>/dev/null; }
fail=0
for width in 1280 390; do
  for scheme in light dark; do
    # preferredColorScheme: 0 = dark, 1 = light (--force-dark-mode does not
    # change prefers-color-scheme in the headless shell).
    pcs=$([ "$scheme" = dark ] && echo 0 || echo 1)
    flags=(--window-size="$width,900" --blink-settings=preferredColorScheme=$pcs)
    json=$(run "${flags[@]}" --dump-dom "file://$(realpath "$probe")" | sed -n 's:.*<title>\(.*\)</title>.*:\1:p' | sed 's/&quot;/"/g')
    read -r sw iw sh dark long < <(python3 -I -c 'import json,sys
d = json.loads(sys.argv[1])
print(d["sw"], d["iw"], d["sh"], d["dark"], json.dumps(d["long"]))' "$json")
    shot="$out/check-$width-$scheme.png"
    height=$(( sh > 16000 ? 16000 : sh ))
    run --window-size="$width,$height" --blink-settings=preferredColorScheme=$pcs \
      --screenshot="$shot" "file://$(realpath "$page")" >/dev/null
    status=ok
    if [ "$sw" -gt "$iw" ] || [ "$long" != "[]" ]; then status=FAIL; fail=1; fi
    echo "$width px $scheme: scrollWidth=$sw innerWidth=$iw dark=$dark long_chips=$long -> $status ($shot)"
  done
done
exit $fail
