#!/usr/bin/env bash
# Tests for page_dir.sh (T0121). Prints "ok <case>" per passing case and
# exits 0 only if every case passes.
set -uo pipefail

here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
tmp=$(mktemp -d)
trap 'rm -rf -- "$tmp"' EXIT
fails=0

ticket() { # ticket NAME FRONTMATTER-LINE...
  local f=$tmp/$1.md; shift
  { echo '---'; echo 'id: T9999'; printf '%s\n' "$@"; echo '---'; echo; echo '## Goal'; } > "$f"
  echo "$f"
}
expect_ok() { # expect_ok CASE FILE WANT
  local got
  if got=$("$here/page_dir.sh" "$2" 2>/dev/null) && [ "$got" = "$3" ]; then
    echo "ok $1"
  else
    echo "FAIL $1: got '${got:-}' (want '$3')"; fails=$((fails + 1))
  fi
}
expect_fail() { # expect_fail CASE FILE MESSAGE-PART
  local err
  if err=$("$here/page_dir.sh" "$2" 2>&1 >/dev/null); then
    echo "FAIL $1: exit 0"; fails=$((fails + 1))
  elif [[ $err != *"$3"* ]]; then
    echo "FAIL $1: message '$err' lacks '$3'"; fails=$((fails + 1))
  else
    echo "ok $1"
  fi
}

expect_ok   release        "$(ticket a 'release: 0.9.11')"                 docs/pr/0.9.11
expect_ok   with-comment   "$(ticket b 'release: 0.9.11   # planned')"    docs/pr/0.9.11
expect_ok   two-digit      "$(ticket c 'release: 1.10.0')"                 docs/pr/1.10.0
expect_fail no-field       "$(ticket d 'area: process')"                   "no release: field"
expect_fail tilde          "$(ticket e 'release: ~')"                      "is not a final release"
expect_fail word           "$(ticket f 'release: next')"                   "is not a final release"
expect_fail pre-release    "$(ticket g 'release: 0.9.11-rc.1')"            "is not a final release"
expect_fail missing-file   "$tmp/none.md"                                  "no such ticket file"
printf 'release: 0.9.11\n\n## Goal\n' > "$tmp/nofront.md"
expect_fail no-frontmatter "$tmp/nofront.md"                               "no frontmatter"
# A `release:` line in the body, not the frontmatter, does not count.
{ echo '---'; echo 'id: T9999'; echo '---'; echo 'release: 0.9.11'; } > "$tmp/body.md"
expect_fail body-only      "$tmp/body.md"                                  "no release: field"

if [ "$fails" -eq 0 ]; then
  echo "all ok"
else
  echo "$fails failed"
  exit 1
fi
