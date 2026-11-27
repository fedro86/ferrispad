#!/usr/bin/env bash
# The quality gates (T0122): fmt, clippy with -D warnings, then the full test
# suite, in the project's Nix dev shell. Run it on the host, where nix is:
#
#   scripts/gates.sh [<checkout>]          # default: the current directory
#
# From the VSCode Flatpak sandbox:
#
#   flatpak-spawn --host bash -lc "<checkout>/scripts/gates.sh <checkout>"
#
# The tests build real FLTK windows (tests/ui_dialogs.rs, start_page_layout,
# fltk_main_thread, ...), so they never run on the caller's display: the
# user's DISPLAY, WAYLAND_DISPLAY and WAYLAND_SOCKET are removed, and
# xvfb-run -a starts a private Xvfb for them, as CI does.
# FERRISPAD_REQUIRE_UI_TESTS=1 makes a missing display a failure instead of
# a skip. The pre-commit hook (.claude/hooks/pre-commit-gates.sh) runs this
# script before every commit.
#
# Test seams (.claude/hooks/test-pre-commit-gates.sh), never set them by hand:
#   FERRISPAD_GATES_CMD   replaces the cargo gates; it runs as the test step,
#                         in the private-display environment, outside nix.
#   FERRISPAD_GATES_XVFB  replaces `xvfb-run -a` (from nixpkgs).
set -euo pipefail

cd "${1:-.}"

if [[ -n "${FERRISPAD_GATES_CMD:-}" ]]; then
  devshell=()
  lint=true
  tests=$FERRISPAD_GATES_CMD
else
  devshell=(nix develop --command)
  lint='cargo fmt --check && cargo clippy --all-targets --all-features -- -D warnings'
  tests='cargo test'
fi
if [[ -n "${FERRISPAD_GATES_XVFB:-}" ]]; then
  xvfb=("$FERRISPAD_GATES_XVFB")
else
  xvfb=(nix shell nixpkgs#xvfb-run --command xvfb-run -a)
fi

"${devshell[@]}" bash -c "$lint"
exec env -u DISPLAY -u WAYLAND_DISPLAY -u WAYLAND_SOCKET FERRISPAD_REQUIRE_UI_TESTS=1 \
  "${xvfb[@]}" "${devshell[@]}" bash -c "$tests"
