#!/usr/bin/env bash
# PreToolUse(Bash) hook — run the full quality gates before a `git commit`,
# and block the commit (exit 2) if any gate fails. Non-commit commands exit 0.
#
# Env quirk: cargo is not on PATH in the VSCode Flatpak terminal; it lives in
# the project's Nix dev shell on the host. We run each gate via `nix develop`.
# Wired in .claude/settings.json.
set -uo pipefail

# Claude Code sets CLAUDE_PROJECT_DIR for hooks; fall back to this script's
# location (.claude/hooks/ → repo root) for manual runs.
REPO="${CLAUDE_PROJECT_DIR:-$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)}"
LOG="/tmp/ferrispad-precommit-gates.log"

# The hook event arrives as JSON on stdin; pull out the bash command.
cmd="$(python3 -c 'import sys, json; print(json.load(sys.stdin).get("tool_input", {}).get("command", ""))' 2>/dev/null || true)"

# Only gate real `git commit` invocations; let everything else through.
if ! printf '%s' "$cmd" | grep -Eq '(^|[^[:alnum:]_])git[[:space:]]+commit([[:space:]]|$)'; then
  exit 0
fi

gates='cargo fmt --check && cargo clippy --all-targets --all-features -- -D warnings && cargo test'

# Two environments run this hook:
#  - VSCode Flatpak terminal: nix lives on the host, reached via flatpak-spawn.
#  - Host session (Claude Code CLI): nix is already on PATH, run directly.
if command -v flatpak-spawn >/dev/null 2>&1; then
  run_gate() { flatpak-spawn --host bash -lc "cd '$REPO' && nix develop --command bash -c '$gates'"; }
else
  run_gate() { bash -c "cd '$REPO' && nix develop --command bash -c '$gates'"; }
fi

if run_gate >"$LOG" 2>&1; then
  exit 0
fi

{
  echo "── pre-commit quality gates FAILED — commit blocked ──"
  echo "(full log: $LOG)"
  echo
  tail -n 40 "$LOG"
} >&2
exit 2
