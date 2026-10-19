#!/usr/bin/env bash
# PreToolUse(Bash) hook — run the full quality gates before a `git commit`,
# and block the commit (exit 2) if any gate fails. Non-commit commands exit 0.
#
# The gates run in the checkout the commit is made in, which is not always the
# main checkout: parallel agents commit from git worktrees. The directory comes
# from the payload's `cwd` (Claude's working directory, which follows it into a
# worktree) adjusted by any `cd <dir>` / `git -C <dir>` in the command. If it
# can't be determined, the main checkout is gated instead (fail closed).
#
# Env quirk: cargo is not on PATH in the VSCode Flatpak terminal; it lives in
# the project's Nix dev shell on the host. We run each gate via `nix develop`.
# Wired in .claude/settings.json. Tests: .claude/hooks/test-pre-commit-gates.sh.
set -uo pipefail

# Claude Code sets CLAUDE_PROJECT_DIR for hooks (always the main checkout);
# fall back to this script's location (.claude/hooks/ → repo root).
REPO="${CLAUDE_PROJECT_DIR:-$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)}"

# Reads the hook JSON on stdin and prints three lines:
#   1. "commit" if the command runs `git commit`, else "skip"
#   2. the directory that commit runs in, or "" if unknown
#   3. why the directory is unknown (empty when line 2 is set)
# shlex splits the command into simple commands at shell operators; each is
# walked in order so `cd` moves the current directory for what follows.
read -r -d '' PARSE <<'PY'
import json, os, re, shlex, sys

try:
    data = json.load(sys.stdin)
except Exception:
    data = {}
if not isinstance(data, dict):
    data = {}
tool_input = data.get("tool_input")
cmd = tool_input.get("command", "") if isinstance(tool_input, dict) else ""
cmd = cmd if isinstance(cmd, str) else ""
cwd = data.get("cwd")
cwd = cwd if isinstance(cwd, str) and os.path.isabs(cwd) else None


def resolve(base, path):
    path = os.path.expanduser(path)
    if os.path.isabs(path):
        return os.path.normpath(path)
    return os.path.normpath(os.path.join(base, path)) if base else None


def tokens(text):
    lex = shlex.shlex(text, posix=True, punctuation_chars="();<>|&\n")
    lex.whitespace = " \t\r"  # newline separates commands, so keep it a token
    lex.whitespace_split = True
    lex.commenters = ""
    return list(lex)


def find_commit(toks):
    """(found, dir) for the first `git ... commit` in the token stream."""
    cur, seg = cwd, []
    for tok in toks + [";"]:
        if tok and set(tok) <= set("();<>|&\n"):
            found, cur, git_dir = walk(seg, cur)
            if found:
                return True, git_dir
            seg = []
        else:
            seg.append(tok)
    return False, None


def walk(seg, cur):
    """Apply one simple command: returns (is_commit, new_cur, commit_dir)."""
    while seg and (seg[0] in ("{", "}", "!") or re.match(r"^[A-Za-z_]\w*=", seg[0])):
        seg = seg[1:]
    if not seg:
        return False, cur, None
    if seg[0] in ("cd", "pushd"):
        args = [a for a in seg[1:] if a != "--"]
        if args and args[0] != "-":
            return False, resolve(cur, args[0]), None
        return False, None, None  # `cd`, `cd -`: target unknown
    if seg[0] != "git":
        return False, cur, None
    d, i = cur, 1
    while i < len(seg) and seg[i].startswith("-"):
        opt = seg[i]
        if opt == "-C" and i + 1 < len(seg):
            d = resolve(d, seg[i + 1])
            i += 2
        elif opt in ("-c", "--git-dir", "--work-tree", "--namespace", "--exec-path") and i + 1 < len(seg):
            if opt in ("--git-dir", "--work-tree"):
                d = None  # repo chosen another way: don't guess
            i += 2
        else:
            if opt.startswith(("--git-dir=", "--work-tree=")):
                d = None
            i += 1
    return (i < len(seg) and seg[i] == "commit"), cur, d


found, git_dir, why = False, None, ""
try:
    found, git_dir = find_commit(tokens(cmd))
except ValueError:
    # Unbalanced quotes, typically an apostrophe in a bare heredoc body:
    # retry on the text before the heredoc, which holds the git command.
    try:
        found, git_dir = find_commit(tokens(cmd.split("<<", 1)[0]))
    except ValueError:
        pass
    if not found:
        why = "could not parse the command"

# Anything the parser missed that still looks like a commit (`bash -c "..."`,
# `$(...)`, `env git commit`) is gated too, against an unknown directory.
if not found and re.search(r"(^|[^\w])git(\s+-[Cc]\s+\S+)*\s+commit(\s|$)", cmd):
    found, git_dir = True, None
    why = why or "git commit is nested in another command"

if found and not git_dir and not why:
    why = "no cwd in the hook payload" if cwd is None else "the command changes directory in a way the hook can't follow"
print("commit" if found else "skip")
print(git_dir or "")
print(why if found and not git_dir else "")
PY

mapfile -t parsed < <(python3 -c "$PARSE" 2>/dev/null)
verdict="${parsed[0]:-}"
dir="${parsed[1]:-}"
why="${parsed[2]:-}"

# If the hook can't even parse its input, gate rather than let a commit through.
if [[ -z "$verdict" ]]; then
  verdict=commit
  why="could not read the hook payload"
fi

# Only gate real `git commit` invocations; let everything else through.
[[ "$verdict" == commit ]] || exit 0

note=""
if [[ -n "$dir" ]] && TARGET="$(git -C "$dir" rev-parse --show-toplevel 2>/dev/null)" && [[ -n "$TARGET" ]]; then
  # A commit in some other repository is not FerrisPad's to gate. Compare the
  # shared .git dirs: every worktree of this repo has the same one.
  common_dir() { (cd "$(git -C "$1" rev-parse --path-format=absolute --git-common-dir 2>/dev/null)" 2>/dev/null && pwd -P); }
  target_common="$(common_dir "$TARGET")"
  repo_common="$(common_dir "$REPO")"
  if [[ -n "$target_common" && -n "$repo_common" && "$target_common" != "$repo_common" ]]; then
    exit 0
  fi
else
  [[ -n "$dir" ]] && why="$dir is not inside a git checkout"
  TARGET="$REPO"
  note="could not determine the checkout this commit runs in (${why:-unknown}); gating the main checkout $REPO instead"
fi

# One log per checkout, so parallel worktrees don't overwrite each other's.
LOG="/tmp/ferrispad-precommit-gates-$(basename -- "$TARGET")-$(printf '%s' "$TARGET" | cksum | cut -d' ' -f1).log"
q_target="$(printf '%q' "$TARGET")"

gates='cargo fmt --check && cargo clippy --all-targets --all-features -- -D warnings && cargo test'

# Three ways to run the gates:
#  - FERRISPAD_GATES_CMD set: tests only, a stand-in for the cargo gates.
#  - VSCode Flatpak terminal: nix lives on the host, reached via flatpak-spawn.
#  - Host session (Claude Code CLI): nix is already on PATH, run directly.
if [[ -n "${FERRISPAD_GATES_CMD:-}" ]]; then
  run_gate() { bash -c "cd $q_target && $FERRISPAD_GATES_CMD"; }
elif command -v flatpak-spawn >/dev/null 2>&1; then
  run_gate() { flatpak-spawn --host bash -lc "cd $q_target && nix develop --command bash -c '$gates'"; }
else
  run_gate() { bash -c "cd $q_target && nix develop --command bash -c '$gates'"; }
fi

{
  echo "── pre-commit quality gates in $TARGET ──"
  [[ -n "$note" ]] && echo "note: $note"
  echo
} >"$LOG"
[[ -n "$note" ]] && echo "pre-commit gates: $note" >&2

if run_gate >>"$LOG" 2>&1; then
  exit 0
fi

{
  echo "── pre-commit quality gates FAILED in $TARGET — commit blocked ──"
  [[ -n "$note" ]] && echo "note: $note"
  echo "(full log: $LOG)"
  echo
  tail -n 40 "$LOG"
} >&2
exit 2
