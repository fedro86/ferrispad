#!/usr/bin/env bash
# Tests for pre-commit-gates.sh (T0054). Not wired as a hook; run by hand:
#
#   bash .claude/hooks/test-pre-commit-gates.sh
#
# Builds a throwaway "main checkout" repo plus git worktrees of it under a
# temp dir, then pipes synthetic PreToolUse payloads into the hook. The real
# cargo gates are replaced by FERRISPAD_GATES_CMD='test ! -e BROKEN', so a
# directory "fails the gates" exactly when it contains a BROKEN file. That
# keeps the suite to a second or two and independent of the Rust code.
set -uo pipefail

HOOK="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/pre-commit-gates.sh"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

main="$tmp/main"
wt="$tmp/wt-broken"
wt_clean="$tmp/wt-clean"
other="$tmp/other-repo"

quiet_git() { git "$@" >/dev/null 2>&1; }
quiet_git init -q -b master "$main"
quiet_git -C "$main" -c user.name=t -c user.email=t@t commit -q --allow-empty -m init
quiet_git -C "$main" worktree add -q -b wt-broken "$wt"
quiet_git -C "$main" worktree add -q -b wt-clean "$wt_clean"
quiet_git init -q -b master "$other"
mkdir -p "$wt/src"
touch "$wt/BROKEN" "$other/BROKEN"

export CLAUDE_PROJECT_DIR="$main"
export FERRISPAD_GATES_CMD='test ! -e BROKEN'

# payload <command> [cwd] — a PreToolUse(Bash) payload; cwd omitted if empty.
payload() {
  python3 -c '
import json, sys
d = {"hook_event_name": "PreToolUse", "tool_name": "Bash",
     "tool_input": {"command": sys.argv[1]}}
if len(sys.argv) > 2 and sys.argv[2]:
    d["cwd"] = sys.argv[2]
print(json.dumps(d))' "$@"
}

pass=0
fail=0
# check <name> <expected-exit> <command> [cwd] — runs the hook, compares exit.
# Leaves the hook's stderr in $tmp/stderr for follow-up assertions.
check() {
  local name="$1" want="$2" cmd="$3" cwd="${4:-}" got
  payload "$cmd" "$cwd" | bash "$HOOK" >/dev/null 2>"$tmp/stderr"
  got=$?
  if [[ "$got" == "$want" ]]; then
    echo "ok   $name (exit $got)"
    pass=$((pass + 1))
  else
    echo "FAIL $name: expected exit $want, got $got"
    sed 's/^/     | /' "$tmp/stderr"
    fail=$((fail + 1))
  fi
}

# expect_stderr <name> <fixed-string> — the last hook run's stderr mentions it.
expect_stderr() {
  if grep -Fq -- "$2" "$tmp/stderr"; then
    echo "ok   $1"
    pass=$((pass + 1))
  else
    echo "FAIL $1: stderr does not mention '$2'"
    sed 's/^/     | /' "$tmp/stderr"
    fail=$((fail + 1))
  fi
}

# expect_log <name> <fixed-string> — the log named in stderr mentions it.
expect_log() {
  local log
  log="$(sed -n 's/^(full log: \(.*\))$/\1/p' "$tmp/stderr" | head -n 1)"
  if [[ -n "$log" && -f "$log" ]] && grep -Fq -- "$2" "$log"; then
    echo "ok   $1"
    pass=$((pass + 1))
  else
    echo "FAIL $1: log '${log:-<none>}' does not mention '$2'"
    fail=$((fail + 1))
  fi
}

# Ticket case 1: broken worktree, clean main checkout, payload cwd = worktree.
check "1. cwd=broken worktree blocks the commit" 2 "git commit -m x" "$wt"
expect_log "1. log names the worktree" "$wt"

# Ticket case 2: no cwd, the worktree is named with git -C.
check "2. git -C <broken worktree> commit blocks" 2 "git -C $wt commit -m x"

# Ticket case 3: clean worktree passes.
check "3. clean worktree passes" 0 "git commit -m x" "$wt_clean"

# Ticket case 4: a non-commit command exits 0 without running any gate.
FERRISPAD_GATES_CMD=false check "4. non-commit command is not gated" 0 "git status" "$wt"

# A failure in the main checkout no longer blocks a clean worktree's commit.
touch "$main/BROKEN"
check "5. broken main does not block a clean worktree" 0 "git commit -m x" "$wt_clean"

# Fail closed: no cwd and nothing in the command names a directory, so the
# hook gates the main checkout and says so.
check "6. undeterminable target gates the main checkout" 2 "git commit -m x"
expect_stderr "6. message says it fell back to the main checkout" "main checkout"
rm "$main/BROKEN"

# A leading `cd <dir> &&` wins over the payload cwd.
check "7. cd <broken worktree> && git commit blocks" 2 "cd $wt && git commit -m x" "$main"

# Relative cd / -C resolve against cwd; a subdirectory maps to its toplevel.
check "8. relative cd into a worktree subdir blocks" 2 "cd ../wt-broken/src && git commit -m x" "$main"
check "9. git -C with options before commit blocks" 2 "git -c core.quotepath=off -C $wt commit -am x" "$main"

# A commit in a different repository is not FerrisPad's to gate.
check "10. commit in an unrelated repo is not gated" 0 "git commit -m x" "$other"

# A quoted mention of `git commit` is not a commit.
FERRISPAD_GATES_CMD=false check "11. echo 'git commit' is not gated" 0 "echo 'git commit'" "$wt"

# Multi-line heredoc commit message (the usual Claude Code form).
check "12. heredoc commit message, broken worktree" 2 "git commit -m \"\$(cat <<'EOF'
fix: it's done

Body with \"quotes\".
EOF
)\"" "$wt"

# A bare heredoc whose body has an apostrophe (unbalanced for shlex).
check "13. bare heredoc with apostrophe, broken worktree" 2 "git commit -F - <<'EOF'
it's done
EOF" "$wt"

# Nested commit the parser can't follow: gated (main checkout), not skipped.
touch "$main/BROKEN"
check "14. bash -c 'git commit' is still gated" 2 "bash -c 'cd /somewhere && git commit -m x'" "$wt_clean"
expect_stderr "14. message says it fell back to the main checkout" "main checkout"
rm "$main/BROKEN"

echo
echo "$pass passed, $fail failed"
[[ "$fail" -eq 0 ]]
