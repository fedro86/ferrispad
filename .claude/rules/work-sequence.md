# Work sequence — how a change lands

Every implementation change in this repo lands **through a ticket**. This is
not optional bookkeeping; it is the work loop. Source of truth:
`docs/tickets/README.md`.

## The state machine

```
∅ ──create──▶ 1-todo/ ──work done──▶ 2-review/ ──user verified──▶ 3-done/
```

A ticket's state **is** the directory its file lives in, under `docs/tickets/`:

| Dir | Meaning |
|-----|---------|
| `1-todo/` | Specified, not started or in progress. |
| `2-review/` | Implementation done, code on disk, tests pass — **not committed**, waiting for human verification. |
| `3-done/` | User verified via the recipe, commit landed. |

The file keeps the same name across all three folders; moving it is the only
thing that changes. Naming: `T<NNNN>-<short-slug>.md` — zero-padded, monotonic
ID, never reused or renumbered. Copy `docs/tickets/_template.md` for new tickets.

## What I (Claude) do at each transition

1. **∅ → `1-todo/`** — Write the ticket from `_template.md`: Goal, In scope,
   Out of scope, **How to test** (a concrete `cargo test` recipe and/or a
   manual repro — "run the test suite" is *not* a recipe), Acceptance criteria,
   Affected files, Notes.
2. **`1-todo/` → `2-review/`** — **Open the tracking GitHub issue first** (see
   *GitHub issue tracking* below), then implement (see the bugfix loop below).
   Update the ticket with what was *actually* done and the working verification
   recipe. Run the gates. **Do not commit.** Hand back to the user with the
   recipe to run.
3. **`2-review/` → `3-done/`** — *Only after the user approves.* Move the
   ticket to `3-done/`, **then** `git commit` with `Closes #<issue>` in the body
   (which closes the tracking issue when the commit reaches `master`).

## GitHub issue tracking

FerrisPad is an AI-assisted project; every ticket we actively work is mirrored
by a GitHub issue so the work is auditable outside the local ticket folder.
Personal project → use `gh` (it targets this repo's `origin` remote
automatically — no owner/name hardcoded).

- **Open at the start of implementation** (the `1-todo/` → `2-review/` step):
  ```bash
  gh issue create --title "T<NNNN>: <ticket title>" \
    --label "<bug|enhancement|documentation>" \
    --body "<one-line goal>\n\nTicket: docs/tickets/2-review/T<NNNN>-<slug>.md"
  ```
  Record the number in the ticket's `issue:` frontmatter field.
- **Close when the ticket lands** (`3-done/` + commit): put `Closes #<issue>` in
  the commit body so pushing to `master` auto-closes it. If the commit is not
  pushed right away, `gh issue close <issue>` once it is.
- **One ticket ⇄ one issue.** Don't open issues for tickets you aren't working
  yet, and don't retro-file issues for already-`3-done/` tickets.

## Bugfix tickets — red test first, simplify before patching

For any ticket whose Goal is a defect (observed wrong behaviour), the
implementation step follows this fixed sequence:

1. **Red test first.** Before touching the fix, write the regression test that
   reproduces the bug exactly as the ticket describes it, and RUN it: it must
   FAIL on the current code, for the bug's reason (not a compile/setup error).
   That failing run is the proof the test exercises the bug — record it in the
   ticket.
2. **Simplify instead of layering.** Before stacking a special case on top of
   existing logic, check whether restructuring removes the bug's whole class —
   one coherent rule (a shared helper, a reordered decision) instead of
   patch-on-patch. Bounded by regression safety: every pre-existing test stays
   green; behaviour changes only where the ticket says so.
3. **Implement**, in the simplified shape when one exists.
4. **Verify.** The red test is now green, and the full gates pass — no
   regressions elsewhere.

A bugfix diff whose regression test never failed is not reviewable — "the code
looks right" is not evidence the bug existed or is gone.

## Hard rules (do not violate)

- **No commit while any ticket is in `1-todo/` or `2-review/`.** Human review
  is non-negotiable before commit. The pre-commit hook (fmt → clippy → test) is
  *necessary* but not *sufficient* — the ticket being in `3-done/` is what
  authorises the commit.
- **One ticket, one diff, one commit** (typically). Don't bundle tickets into
  one commit; don't split one ticket across commits. Exceptions must be
  explicit ("depends on T0002 landing first").
- **Small.** A ticket fits one sitting and one diff. If it grows past ~5
  in-scope bullets, split it into dependent tickets.
- **Self-contained.** A ticket is readable without prior ticket context.

## The gates

Run from a Nix dev shell (`cargo` is not on the Flatpak-terminal PATH):

```bash
nix develop -c cargo test
nix develop -c cargo clippy --all-targets --all-features -- -D warnings
nix develop -c cargo fmt --check
```

All three must be warning-free (`CLAUDE.md` project rule). Prefer adding
`-- -D warnings` to clippy so a warning fails the gate rather than passing.

## Commit conventions (personal GitHub project)

- Personal project → `gh`, not `glab`. Tags have **no** `v` prefix.
- Follow the existing Conventional-Commits style (`feat:`, `fix:`, `chore:`,
  `docs:`…). Reference the ticket ID (e.g. `(T0020)`) and close the tracking
  issue with a `Closes #<issue>` line in the body.
- End commit messages with the `Co-Authored-By` trailer per the global git rules
  **only if** the user wants AI co-authorship recorded — note that this trailer
  is exactly what a hostile downstream reviewer scans for, so it is the user's
  call per commit. (This repo's default is to keep it — FerrisPad is an
  AI-assisted project.)
