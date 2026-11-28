#!/usr/bin/env bash
# The folder of a PR's presentation (new-pr skill, T0121).
#
#   .claude/skills/new-pr/page_dir.sh docs/tickets/<state>/T<NNNN>-<slug>.md
#
# Prints docs/pr/<release>, where <release> is the ticket's `release:`
# frontmatter field: the final release the ticket is planned for, X.Y.Z
# (a ticket that ships in 0.9.11-rc.1 is still planned for 0.9.11).
# Fails loudly (exit 1, the reason on stderr) when the ticket is missing,
# has no frontmatter, no `release:` field, or a value that is not X.Y.Z.
set -euo pipefail

ticket=${1:?usage: page_dir.sh <ticket.md>}
fail() { echo "page_dir.sh: $ticket: $*" >&2; exit 1; }
[ -f "$ticket" ] || fail "no such ticket file"
# The frontmatter: the lines between the first two '---' lines.
front=$(awk 'NR == 1 && $0 != "---" { exit } NR > 1 && $0 == "---" { exit } NR > 1 { print }' "$ticket")
[ -n "$front" ] || fail "no frontmatter"
line=$(grep -E '^release:' <<<"$front" || true)
[ -n "$line" ] || fail "no release: field (add the planned release, e.g. 'release: 0.9.11')"
value=$(sed -E 's/^release:[[:space:]]*//; s/[[:space:]]*(#.*)?$//' <<<"$line")
[[ $value =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || fail "release: '$value' is not a final release X.Y.Z"
echo "docs/pr/$value"
