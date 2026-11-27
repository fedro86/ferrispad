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
# fltk_main_thread, ...), so they never run on the caller's screen:
#   - DISPLAY, WAYLAND_DISPLAY and WAYLAND_SOCKET are removed, and xvfb-run -a
#     starts a private Xvfb for them, as CI does;
#   - FLTK_BACKEND=x11 and an empty, private XDG_RUNTIME_DIR: fltk is built
#     with use-wayland, and where its Wayland backend is compiled in, it
#     tries wl_display_connect(NULL) first, which falls back to wayland-0 in
#     XDG_RUNTIME_DIR, the user's compositor;
#   - DBUS_SESSION_BUS_ADDRESS is removed, so no test reaches the user's
#     session bus (portals, notifications).
# FERRISPAD_REQUIRE_UI_TESTS=1 makes a missing display a failure instead of
# a skip, and makes the UI tests check that FLTK really opened that private
# X display (tests/support/ui_layout.rs). The pre-commit hook
# (.claude/hooks/pre-commit-gates.sh) runs this script before every commit.
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

runtime=$(mktemp -d "${TMPDIR:-/tmp}/ferrispad-gates-runtime.XXXXXX")
chmod 700 "$runtime"
trap 'rm -rf -- "$runtime"' EXIT
env -u DISPLAY -u WAYLAND_DISPLAY -u WAYLAND_SOCKET -u DBUS_SESSION_BUS_ADDRESS \
  FLTK_BACKEND=x11 XDG_RUNTIME_DIR="$runtime" FERRISPAD_REQUIRE_UI_TESTS=1 \
  "${xvfb[@]}" "${devshell[@]}" bash -c "$tests"
