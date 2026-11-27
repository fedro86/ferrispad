---
name: new-pr
description: Open a FerrisPad pull request with its presentation. Open the PR as a draft on GitHub to get its number. See every visible change in the real app with shoot.sh and compare.sh, which launch, drive and screenshot FerrisPad in an isolated Xvfb (master or a ref vs the branch, light and dark, labelled before|after montages), then read every shot edge by edge; or state "No visible change". Write the English HTML page from docs/pr/_template.html as docs/pr/<release>/PR-<n>-<slug>.html, where <release> is the ticket's release: field (page_dir.sh), copy it to the main checkout's docs/internal/pr/<release>/ for the user (again after every fix), and commit and push it. The PR stays a draft until the user verifies. Merge only with the ticket in 3-done, all checks green on the head SHA and the user's explicit consent for that PR, then delete the copy. Use when a ticket's implementation is done and its PR is opened (work-sequence step 2), or when asked for a PR presentation.
---

# Open a pull request (with its presentation)

Layout and lifecycle: `docs/pr/README.md`. Every FerrisPad PR gets **one**
presentation, `docs/pr/<release>/PR-<n>-<slug>.html`, written in **English**
from `docs/pr/_template.html`. `<release>` is the release the ticket is
planned for, its `release:` frontmatter field (e.g. `0.9.11`). The GitHub body is not a second document. It is a
short paragraph, the page's path and `Closes #<issue>`. The one exemption,
a PR that only annotates tickets, is at the end of §8.

A PR here is **one ticket** (`.claude/rules/work-sequence.md`), squash-merged
into one commit on `master`. So the page is about that ticket, and the
ticket card is its heart.

**Images are a step of their own (§2), before the text.** Every change a
user can see gets screenshots of the real app. A ticket card without images
says "No visible change" and why.

All commands below run from the ticket's worktree, inside the VSCode
Flatpak sandbox. `nix` and Xvfb are reached through `flatpak-spawn --host`.
`gh` needs `GH_CONFIG_DIR=/home/fedro/.config/gh` and
`PATH=/home/fedro/.nix-profile/bin:$PATH`.

## 0. Preconditions

- The ticket is implemented on `ticket/T<NNNN>`, its file is in
  `docs/tickets/2-review/` with "What was done" filled in, and its GitHub
  issue exists (work-sequence step 2).
- The ticket's frontmatter has `release:`, the final release it is planned
  for (`X.Y.Z`). The page's folder comes from it, and the helper refuses a
  ticket without it:
  ```bash
  D=$(.claude/skills/new-pr/page_dir.sh docs/tickets/2-review/T<NNNN>-<slug>.md)   # docs/pr/0.9.11
  ```
- `origin/master` is merged into the branch (merge, never rebase, once
  pushed), and the gates are green on the result:
  `flatpak-spawn --host bash -lc "cd $PWD && nix develop -c cargo test"`,
  plus clippy (`-- -D warnings`) and `cargo fmt --check`.
- The branch is pushed.

## 1. Get the number: open the PR as a draft

Never guess the next number. Parallel sessions open PRs and issues too, and
they share one sequence.

```bash
gh pr create --draft --base master --head ticket/T<NNNN> \
  --title "<type>(<scope>): <outcome> (T<NNNN>)" \
  --body "Presentation: docs/pr/<release>/PR-<n>-<slug>.html (being written). Closes #<issue>"
gh pr view ticket/T<NNNN> --json number,url
```

The title is the Conventional-Commits subject the squash commit will carry.
Put the number you read back into the body (`gh pr edit <n> --body …`).

## 2. Images first: every visible change gets them

**Do this before writing a word of the page.** Ask of the ticket: *can the
change be seen?* If it can, the card shows it. This covers:

- **Windows and dialogs**: Settings, the theme manager, Find, Go to line,
  the plugin dialogs, the session picker, message boxes.
- **The main window**: the menus and their enabled state, the tab bar, the
  status bar, the start page, the file tree, the diagnostics and terminal
  panels, the split/diff view.
- **The editor itself**: highlighting, the gutter, the selection, the
  current-line band, a large file in read-only or tail mode.
- **Text a person reads**: `FerrisPad --help`, an error dialog's message.

### How to show it

- **Before / after:** the same scenario on a build of `master` (or a
  named ref) and on the branch, as one labelled montage (`compare.sh`).
- **Light and dark**, at a real font: Noto Sans from nixpkgs, editor font
  16, and 18 for dialogs, since the larger size catches clipping.
- **What Xvfb cannot show** is said in the card: *"Pending user on
  <native Wayland | Windows | macOS | multi-monitor>: <what to look at>."*
- **A UI card with no image** is not ready. Only a card whose change truly
  cannot be seen says *"No visible change: <why>"*. Examples:
  - "a skill and its docs only";
  - "a refactor with identical output, checked by `<test>`";
  - "a CI-only change".

### How to capture: `shoot.sh` and `compare.sh`

These two tools are **the** way to see FerrisPad. Never hand-write an Xvfb
script: the tools carry the isolation barriers below, and a hand-written
script does not.

**`compare.sh` makes the before/after pair:**

```bash
.claude/skills/new-pr/compare.sh docs/pr/scenarios/<scenario>.sh \
    ~/Code/personal/ferrispad-wt/uitest-T<NNNN> [--before <ref>] [--font 18] [--size 1100x740]
```

It does the following:

1. It builds `<ref>` in a throwaway worktree (default `origin/master`, or a
   release tag such as `0.9.10-rc.2`) and the branch from this worktree.
   Both are debug builds, through `flatpak-spawn --host … nix develop -c
   cargo build`.
2. It runs the same scenario on both, light and dark.
3. It writes `<shot>-<theme>-compare.png`, a labelled `before | after`
   montage at most 1600 px wide, ready to inline.

The single shots and each run's log stay in `before/` and `after/`. Run it
again with `--font 18`: the larger font is the one that catches clipping.

**`shoot.sh` runs one scenario on one binary:**

```bash
.claude/skills/new-pr/shoot.sh target/debug/FerrisPad <outdir> <scenario> --theme dark --font 18
```

Use it while you work on a dialog. `compare.sh` calls it.

**A scenario** is a small bash file in `docs/pr/scenarios/`, described in
that folder's README:

- `OPEN_FILES=(<fixture>)` opens fixture files from
  `docs/pr/scenarios/fixtures/` on the command line;
- `SETTINGS_EXTRA` adds `settings.json` members;
- `scenario() { … }` drives the app with the helpers `click`, `dclick`,
  `key`, `type`, `menu File|Edit|View|Format|Plugins|Help`, `wait_idle`,
  `shot <name> [<window-title-regex>]` and `shot_screen`.

Start from `start-page.sh` (no interaction) or `editor.sh` (a fixture, a
menu, the Find dialog). Commit a ticket's scenario with its PR, so the
next change to that UI can be compared against it.

**Isolation: the app must never reach the user's desktop.** On 2026-11-27
a hand-driven run pressed Ctrl+O. FerrisPad spawned zenity, and zenity
opened "Open Files" dialogs on the user's real GNOME session, one per
run. `shoot.sh` refuses to start without these barriers (its header has
the details):

- **a private D-Bus session:** `dbus-run-session` with no service
  activation, so no portal;
- **no compositor:** a private, empty `XDG_RUNTIME_DIR`, no
  `WAYLAND_DISPLAY`, `GDK_BACKEND=x11`, and a `DISPLAY` that is the run's
  own Xvfb (`:90`+), never `:0`/`:1`;
- **shims:** `zenity`, `kdialog`, `xdg-open`, `gio` and the other openers
  are shims that fail and log, and any shim call fails the run;
- **no network:** update and plugin-update checks are off;
- **no file dialog in a scenario:** fixtures open on the command line, and
  `key` refuses `ctrl+o`, `ctrl+s` and `ctrl+shift+s`;
- **one process group:** the app runs in its own group, which is killed
  at the end, and a leftover process fails the run.

**`shoot.sh --selftest <outdir>`** checks every barrier without starting
FerrisPad. Run it after any change to `shoot.sh`. After a run, confirm on
the host that nothing is left over:

```bash
flatpak-spawn --host bash -c 'pgrep -af "zenity|Xvfb|dbus-daemon --config-file" | grep -v pgrep || echo clean'
```

It also never touches the user's files: every run has a fresh
`XDG_CONFIG_HOME`, `XDG_DATA_HOME` and `XDG_CACHE_HOME`, and only fixture
text. A page enters git history for good.

**Read every PNG yourself, edge by edge**, the single shots and the
montages. Any of these is a **blocking** defect, never cosmetic:

- a label touching or crossing a border;
- text running into a neighbouring widget;
- a clipped widget, or two widgets overlapping.

Fix it in the ticket before the page goes out. The log next to each shot
holds the app's stdout/stderr, where fltk-rs debug asserts show up.
`shoot.sh` fails on a panic, an assert or a crash.

### Where they go

The montages go in the ticket's card, one `<figure>` per theme, each with
a one-line `<figcaption>`. The caption says the refs (the montage labels
them too), light or dark, and **what to look at**. The PNG goes inline as
base64 (`base64 -w0 <shot>-<theme>-compare.png`), so the page stays one
file. The template's style keeps it inside the card.

## 3. Write the page

Copy `docs/pr/_template.html` to `$D/PR-<n>-<slug>.html` (`$D` from §0,
e.g. `docs/pr/0.9.11/`; create the folder if it is the release's first
page). `<slug>` is
the ticket file's slug (`T0114-release-0.9.10-rc.3.md` →
`release-0.9.10-rc.3`). Replace every `{{placeholder}}`, or delete its
element. The rules:

- **The style block stays byte-identical** to the template's. No new CSS,
  no external asset, no script.
- **English**, short sentences, one idea each. Code identifiers go in
  `<code>`.
- **No literal `{{` in a finished page.** When a page has to show the
  placeholder syntax itself, write it as `&#123;&#123;`. Then
  `grep -n '{{' $D/PR-<n>-*.html` finds only real placeholders and
  prints nothing for a finished page.
- **Facts only.** Take them from the sources below. A number that is not
  known yet stays a visible `{{placeholder}}`. Never estimate one.
  - **Commits:** `git log --reverse --format='%h %s' origin/master..HEAD`.
    Every SHA is a `<code class="sha">`, which never wraps. The table lists
    the branch's commits for reference: master gets one squash commit.
  - **The size chip:** everything except the page itself, so the number
    stays right across the page's own commits:
    `git diff --shortstat origin/master...HEAD -- . ':(exclude)docs/pr/*/PR-*.html'`
    (three dots: from the merge-base).
  - **The ticket:** its file in `docs/tickets/`, which holds the decisions,
    the red test and the recipe.
  - **Measurements and verification output:** the values recorded in the
    ticket.
- **A story.**
  - Under the title, a *Before / After* pair says what a user could not do
    before the PR and what they can do now.
  - Write concrete actions: "the sixth changelog entry was cut off on a
    fresh install", not "changed `left_card_h`".
  - The ticket card opens with its own Before / After.
- **The images from §2** go in the ticket card. A card with none carries
  the "No visible change: <why>" sentence.
- **The callouts.**
  - `.why` holds a reason.
  - `.followup` holds a consequence, or a decision the reviewer may
    overturn. Say whose decision it was.
  - `.bad` holds the defect with its red test and failing output, or an
    approach that was tried and dropped.
- **Name who decided.** The user's decisions, and the ones the
  orchestrator or a reviewer took on their behalf, are what the user
  reviews.
- **Release notes and compatibility.** Give the CHANGELOG entry the change
  will get, or "none: process only". Also say what existing files meet:
  `settings.json`, session files, plugins, custom themes.
- **"How to test T<NNNN>"** comes from the ticket's own recipe:
  - the `nix develop -c cargo test <names>` call, with the red failure it
    gave before the fix;
  - the click-path in the app;
  - each step with its expected outcome.
- **The page ends with an end-to-end verification.** This is not a repeat
  of the ticket's tests. It is one scenario the way a user meets the
  change:
  - it starts from a throwaway `XDG_CONFIG_HOME` and `XDG_DATA_HOME` and
    `nix develop -c cargo run`;
  - it ends at what the user sees;
  - every step is a recipe a human can run blind, with its expected
    outcome;
  - the gates come last.

Before going on, run the page's rendering check (§7 condition 1) on the
page. It runs again on the committed page before the merge.

## 4. Deliver it to the user: a copy in the main checkout

The user reads PR pages in `docs/internal/pr/<release>/` of the **main
checkout**, the same folder layout as `docs/pr/`,
not on the branch. The main checkout is the first entry of
`git worktree list`, whatever branch it has checked out: never write its
path by hand. `docs/internal/*` is gitignored there, so the copy is never
committed. From the ticket's worktree:

```bash
M=$(git worktree list --porcelain | sed -n '1s/^worktree //p') && mkdir -p "$M/docs/internal/${D#docs/}" && cp $D/PR-<n>-<slug>.html "$M/docs/internal/${D#docs/}/"
```

`--porcelain` keeps a path with spaces whole.

- Copy the page as soon as it exists, and **again after every fix**. The
  copy the user reads must never be older than the branch's page.
- Tell the user the copy's full path. Their feedback is a merge condition
  (§7), separate from the page's rendering check.

## 5. Commit and push it

Commit the page on its own: `docs: present PR #<n>`, plus the
`Co-Authored-By` trailer. Then push. The pre-commit hook runs the cargo
gates. Every push to the PR runs CI on the new head.

## 6. Hand it to the user (the PR stays a draft)

Set the body. **Do not mark the PR ready.** On GitHub, the draft flag
means "waiting for the user's verification", and the landing refuses a
draft. The body has four parts:

- one paragraph: what the PR delivers;
- `Presentation: docs/pr/<release>/PR-<n>-<slug>.html`;
- `Closes #<issue>`;
- the PR attribution line from the session's instructions.

Write it to a file in the scratchpad and pass
`gh pr edit <n> --body-file <file>`.

Then confirm the state on GitHub, not from gh's printout:

```bash
gh pr view <n> --json state,isDraft,headRefOid,mergeable,url
```

Then **stop**. Tell the user that the PR is ready for their verification,
with:

- its link;
- the path of the page's copy;
- the head SHA and its check status.

**After the user's verification** (work-sequence step 3):

1. Move the ticket to `3-done/` in a new commit.
2. Update the page in the same commit: the ticket chip, the commits
   table, the footer.
3. Push, and copy the page to the user again.
4. Mark the PR ready (`gh pr ready <n>`), together with the `3-done/`
   move and never before it.
5. Run the page's rendering check again on the new head.

## 7. Merge only on the user's explicit consent

Before a merge, all five hold:

1. **The page's own rendering check has passed** on the committed page,
   after the last fix. This is a minor check of the HTML page, not of
   FerrisPad (that is §2):
   ```bash
   .claude/skills/new-pr/visual_check.sh $D/PR-<n>-<slug>.html <scratchpad>/vc-<n>
   ```
   It renders the page in headless Chromium four times: 1280 px and
   390 px wide, each with `prefers-color-scheme` light and dark. It uses
   the Playwright `chrome-headless-shell` in `~/.cache/ms-playwright/`,
   which runs inside the Flatpak sandbox. Its dark mode is
   `--blink-settings=preferredColorScheme=0`; `--force-dark-mode` does not
   change `prefers-color-scheme` there. It passes when, in all four:
   - `scrollWidth <= innerWidth`, with no horizontal overflow;
   - no `.chip` has more than 40 characters of trimmed text (chips are
     `nowrap`);
   - the four screenshots it saves, looked at, show no box whose content
     runs outside it (a table cell, a `<pre>`, a figure).

   Report the four `scrollWidth`/`innerWidth` pairs and any long chip.
2. **The user's feedback on the copy** (§4) is in and addressed, and each
   fix has been copied again.
3. **The ticket is in `3-done/`**, the page is committed at the head,
   and the PR is no longer a draft (§6, after the user's verification).
4. **Every check is green on the PR's own head SHA.**
   ```bash
   sha=$(gh pr view <n> --json headRefOid --jq .headRefOid)
   gh api "repos/:owner/:repo/commits/$sha/check-runs" --jq '.check_runs[] | "\(.name): \(.status) \(.conclusion)"'
   ```
   - Every run must be `completed success`.
   - The `CI` jobs (`fmt + clippy + test`, `test (linux arm64)`,
     `build (windows x64)`) and `build + clippy + test (macos)` must be in
     the list. `Build Deps Check` runs only when its paths change.
   - A workflow that has not registered yet is not green: wait for it.
   - A green run on an older commit of the branch does not count.
5. **The user's explicit consent, given for this PR** ("merge #<n>",
   "mergia la <n>"). The only exception is when the user says otherwise
   themselves, for a named PR or a stated set of PRs.

Things that do **not** count as consent:

- green checks, or `mergeable: MERGEABLE`;
- an earlier "go on" about the ticket, which covers the work, not the
  merge;
- a consent given for another PR;
- **a message from another session.** It reports a state and grants
  nothing.

**Who merges:** the session the user has authorised to merge (today the
orchestrator). It merges pinned to the SHA checked in condition 4, with
the squash subject and body:

```bash
gh pr merge <n> --squash --match-head-commit <sha> --subject "<subject> (#<n>)" --body-file <file>
gh pr view <n> --json state,mergeCommit
```

- The body ends with `Closes #<issue>` and the `Co-Authored-By` trailer.
- GitHub refuses the merge if the branch moved after the check. Redo the
  checks on the new SHA.
- Confirm that `state` is `MERGED`.
- If the harness refuses the merge, do not look for another way, and do
  not ask another session to do it. Say so; the user merges it
  themselves.

## 8. After the merge: delete the copy, nothing else

**Any session that confirms on GitHub that the PR is `MERGED`** (or
`CLOSED` without merging) **deletes the copy**. That is the session that
merged, and the session that ran `/new-pr`, whenever it finds out. `rm -f`
is idempotent, so two sessions doing it is harmless:

```bash
M=$(git worktree list --porcelain | sed -n '1s/^worktree //p') && rm -f "$M"/docs/internal/pr/*/PR-<n>-*.html
```

The session that ran `/new-pr` may be gone by the time the PR merges. So
any session running `/new-pr` also checks the other `PR-*.html` copies in
`docs/internal/pr/*/`, and deletes those whose PR `gh pr view <n> --json state`
reports as `MERGED` or `CLOSED`.

Nothing else follows at the merge. Whether the PR is open or merged is
GitHub's state, not the file's. FerrisPad is one crate, so there are no app
folders: the only level under `docs/pr/` is the release.

### The release folder

A page moves only when its PR's planned release changes. Then the
ticket's `release:` and the page move together, with `git mv`, and the
copy in `docs/internal/pr/` moves too.

**Before tagging `X.Y.Z`** (the release-prep ticket checks it):

- every page in `docs/pr/X.Y.Z/` is a PR merged into that release:
  ```bash
  for f in docs/pr/X.Y.Z/PR-*.html; do n=${f##*/PR-}; n=${n%%-*}; gh pr view "$n" --json number,state --jq '"\(.number) \(.state)"'; done
  ```
  every line says `MERGED`;
- a planned PR that slipped, still open, moves to the next release's
  folder, with its ticket's `release:`, in its own branch.

**The one PR without a presentation.** A PR whose diff only modifies
tickets already in `3-done/` and/or adds new tickets to `1-todo/` gets no
page. Post-tag records and user-verification notes are examples, as in #230
and #233. Every line of

```bash
git diff --name-status origin/master...HEAD
```

must be `M` on `docs/tickets/3-done/…` or `A` on `docs/tickets/1-todo/…`.
Its one-paragraph GitHub body is enough.

- Open it as in §1, set its body as in §6, and merge it as in §7. With no
  page and no ticket of its own, only conditions 4 and 5 apply, plus "not
  a draft".
- It has no `3-done/` move, so mark it ready (`gh pr ready <n>`) once the
  user has seen it, before asking for consent.
- Anything else gets a page: code, configuration, CI, skills, docs
  outside those two folders, a ticket moved between states, an edit to a
  ticket in `1-todo/` or `2-review/`, a deletion, or a change to
  `docs/tickets/_template.md` or `docs/tickets/README.md`.
