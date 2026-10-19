---
id: T0054
title: Run the pre-commit gates in the worktree being committed, not the main checkout
status: review
created: 2026-10-19
severity: moderate
area: process
depends-on: []
issue: 81
---

## Goal

`.claude/hooks/pre-commit-gates.sh` runs fmt, clippy and test in
`$CLAUDE_PROJECT_DIR`, which is always the main checkout. When a commit is
made from a git worktree (parallel agents, `isolation: worktree`), the hook
checks whatever branch the main checkout has, not the code being committed:

- a broken worktree commit can pass;
- a good one can be blocked by a failure in unrelated code;
- parallel agents all compile in the same target dir and wait on its lock.

Seen repeatedly during the T0019–T0044 batch (2026-10-19). The gate is the
only local quality bar, so it must check the code that is actually being committed.

## In scope

- Find the directory the `git commit` runs in. Use the hook payload's `cwd` if
  present, and handle a leading `cd <dir> &&` and `git -C <dir>` in the
  command. Then run the gates in that directory's `git rev-parse --show-toplevel`.
- Also catch `git -C <dir> commit`, which today does not match the hook's
  regex at all, so the gate is silently skipped.
- Per-worktree log file instead of the shared `/tmp/ferrispad-precommit-gates.log`.
- Fail closed: if the target directory can't be determined, gate the main
  checkout as today and say so in the message.

## Out of scope

- CI gates (T0036 already runs them on push).
- Speeding up the gates.

## What was done

**Payload format checked.** The Claude Code hooks reference lists `cwd` as a
common input field: "`cwd` follows Claude: the `cwd` field in the hook's input
JSON is the worktree root after Claude enters a worktree, and the new directory
after Claude runs `cd`". `CLAUDE_PROJECT_DIR` stays at the main checkout.

**`.claude/hooks/pre-commit-gates.sh`**
- An embedded `python3` parser (stdlib only: `json`, `shlex`, `re`; the hook
  already depended on python3) splits the command into simple commands at shell
  operators and newlines, then walks them in order, starting from the
  payload's `cwd`. `cd <dir>` / `pushd <dir>` move the current directory
  (relative paths resolve against it). `git [-C <dir>]... [-c k=v]... commit`
  is detected with any global options before `commit`, and the `-C` paths
  are applied cumulatively, as git does. Leading `VAR=value` assignments and
  `{`/`!` are skipped. Quoted text such as `echo 'git commit'` is no longer
  mistaken for a commit.
- The gates run in `git -C <dir> rev-parse --show-toplevel`, so a commit from a
  worktree subdirectory gates that worktree.
- **Fail closed.** The hook gates the main checkout and prints
  `pre-commit gates: could not determine the checkout this commit runs in
  (<reason>); gating the main checkout <path> instead`, in the log and the
  block message, when the directory is unknown. That happens when the payload
  has no `cwd` and the command names no absolute path, after `cd -` or a bare
  `cd`, with `--git-dir` / `--work-tree`, when the directory is not in a git
  checkout, or when the parser can't follow the command. A commit nested in
  `bash -c "..."` or `$(...)` is still detected by a regex fallback and gated
  this way. If the command has unbalanced quotes (an apostrophe in a bare
  heredoc body), it is re-parsed up to the first `<<`.
- **Commits in other repositories are not gated.** Before this change, a
  commit in an unrelated repo from a FerrisPad session was gated against
  FerrisPad's main checkout, which was pointless but passed. Now that the hook
  gates the real target, it would run `nix develop` + cargo in that repo and
  block the commit. The hook compares `git rev-parse --git-common-dir`, which
  is shared by every worktree of a repository, and lets a foreign-repo commit
  through.
- **Per-checkout log**:
  `/tmp/ferrispad-precommit-gates-<checkout-basename>-<cksum-of-path>.log`.
  The first line names the gated checkout, and the block message prints the
  path (`(full log: ...)`).
- The target path is `printf %q`-quoted into the `cd` that starts the gates
  (before, a path with a `'` broke the command).
- **Test-only override** `FERRISPAD_GATES_CMD`: when set, it replaces the
  `nix develop` + cargo step. Claude Code never sets it.

**`.claude/hooks/test-pre-commit-gates.sh`** (new): a hermetic shell test. It
builds a throwaway main repo, two worktrees of it, and an unrelated repo
under `mktemp -d`. It sets `CLAUDE_PROJECT_DIR` to the throwaway main repo and
`FERRISPAD_GATES_CMD='test ! -e BROKEN'`, so a checkout "fails the gates"
exactly when it contains a `BROKEN` file. It covers the ticket's cases 1–4
plus 10 more (17 assertions). It runs in about a second.

### Red run (before the fix)

To run the test against the original targeting logic, the old hook first got
only the `FERRISPAD_GATES_CMD` override, with no other change. Output of
`bash .claude/hooks/test-pre-commit-gates.sh` against it:

```
FAIL 1. cwd=broken worktree blocks the commit: expected exit 2, got 0
FAIL 1. log names the worktree: log '<none>' does not mention '$TMP/wt-broken'
FAIL 2. git -C <broken worktree> commit blocks: expected exit 2, got 0
ok   3. clean worktree passes (exit 0)
ok   4. non-commit command is not gated (exit 0)
FAIL 5. broken main does not block a clean worktree: expected exit 0, got 2
     | ── pre-commit quality gates FAILED — commit blocked ──
     | (full log: /tmp/ferrispad-precommit-gates.log)
     |
ok   6. undeterminable target gates the main checkout (exit 2)
FAIL 6. message says it fell back to the main checkout: stderr does not mention 'main checkout'
     | ── pre-commit quality gates FAILED — commit blocked ──
     | (full log: /tmp/ferrispad-precommit-gates.log)
     |
FAIL 7. cd <broken worktree> && git commit blocks: expected exit 2, got 0
FAIL 8. relative cd into a worktree subdir blocks: expected exit 2, got 0
FAIL 9. git -C with options before commit blocks: expected exit 2, got 0
ok   10. commit in an unrelated repo is not gated (exit 0)
ok   11. echo 'git commit' is not gated (exit 0)
FAIL 12. heredoc commit message, broken worktree: expected exit 2, got 0

5 passed, 9 failed
```

Cases 1 and 2 fail for the bug's reason: the gate ran in the clean main
checkout, or not at all. Cases 13–14 were added after the fix. After the fix,
all `17 passed, 0 failed`.

**Real-gate check (agent, during implementation).** `fn  t0054( ){}` was
appended to `src/lib.rs` in the agent's worktree only, and the fixed hook was
run with `CLAUDE_PROJECT_DIR` set to the main checkout. Both payloads gave
exit 2 and `FAILED in .../worktrees/agent-a2a4fbe61619a7b72`, with the
`cargo fmt --check` diff of that worktree's `src/lib.rs`: `{"cwd":<worktree>,
... "git commit -m x"}` and `{... "git -C <worktree> commit -m x"}` with no
`cwd`. The log was
`/tmp/ferrispad-precommit-gates-agent-a2a4fbe61619a7b72-<cksum>.log`. The file
was then restored.

### `.claude/settings.json` `if` filter — applied by the user (2026-10-19)

The hook is wired with `"if": "Bash(git commit*)"`. That is a permission-rule
prefix match, so Claude Code **never starts the hook** for
`git -C <dir> commit` or `git -c k=v commit`, whatever the script does. A
leading `cd <dir> && git commit` is fine, because each subcommand is matched
separately. To make case 2 work end to end, the filter must become
`"if": "Bash(git *)"`. The script exits 0 within milliseconds for non-commit
git commands. The agent's attempt to edit `.claude/settings.json` was denied
by the auto-mode classifier as self-modification, so the one-line change is
left to the user:

```diff
-            "if": "Bash(git commit*)",
+            "if": "Bash(git *)",
```

## How to test

(a) Hook unit tests. These are hermetic, need no cargo, and take about 1 s:

```bash
bash .claude/hooks/test-pre-commit-gates.sh     # expect: 17 passed, 0 failed
```

The Rust gates are unaffected, since no Rust code changed:

```bash
nix develop -c cargo test
nix develop -c cargo clippy --all-targets --all-features -- -D warnings
nix develop -c cargo fmt --check
```

(b) Manual check with the real cargo gates (no app UI is involved; this is a
process change):

1. `git worktree add /tmp/t0054-wt -b t0054-check origin/master`
2. Break formatting in the worktree only: `printf 'fn  t0054( ){}\n' >> /tmp/t0054-wt/src/lib.rs`
3. From the main checkout, run
   `echo '{"cwd":"/tmp/t0054-wt","tool_input":{"command":"git commit -m x"}}' | bash .claude/hooks/pre-commit-gates.sh; echo "exit=$?"`
   → `exit=2`. The message reads `FAILED in /tmp/t0054-wt` and shows the
   `cargo fmt --check` diff, and the log path contains `t0054-wt`.
4. Same with no `cwd`:
   `echo '{"tool_input":{"command":"git -C /tmp/t0054-wt commit -m x"}}' | bash .claude/hooks/pre-commit-gates.sh; echo "exit=$?"` → `exit=2`.
5. `echo '{"cwd":"/tmp/t0054-wt","tool_input":{"command":"git status"}}' | bash .claude/hooks/pre-commit-gates.sh; echo "exit=$?"` → `exit=0`, returns immediately.
6. Clean up: `git worktree remove --force /tmp/t0054-wt && git branch -D t0054-check`.
7. After applying the `settings.json` change above, ask Claude for a commit
   from a worktree. The status line shows the gates running, and the log at
   `/tmp/ferrispad-precommit-gates-<worktree>-*.log` names that worktree.

## Acceptance criteria

- [x] Cases 1–4 behave as expected; 1 and 2 fail on today's script.
- [x] Rust gates unaffected.
- [x] `git -C <dir> commit` gated end to end: the user applied the `settings.json` `if`
      change below by hand on 2026-10-19; this branch carries their exact diff.

## Affected files

- `.claude/hooks/pre-commit-gates.sh`
- `.claude/hooks/test-pre-commit-gates.sh` (new)
- `.claude/settings.json`: `"if": "Bash(git commit*)"` → `"Bash(git *)"` (made by the user)

## Notes

- Origin: observed by every agent of the 2026-10-19 batch.
- The new log path differs from the old shared
  `/tmp/ferrispad-precommit-gates.log`. The block message always prints the
  path, but any brief or doc that hardcodes the old name should be updated.
