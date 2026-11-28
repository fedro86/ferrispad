# Helpers for the dialog scenarios (sourced by dialogs*.sh, not a scenario).
#
# There is no window manager, so nothing moves the keyboard focus: `focus`
# gives it to a dialog (xdotool windowfocus), `close` does that and presses
# Escape, and `editor` clicks the editor to give the main window the
# keyboard back. Windows are found by title among the app's own windows
# ($pid, set by shoot.sh).

editor() { click 600 400; }
# The newest visible window of the app whose title matches $1.
window_of() {
  local w
  w=$(xdotool search --all --onlyvisible --pid "$pid" --name "$1" | tail -1)
  [ -n "$w" ] || die "no window matching '$1'"
  echo "$w"
}
focus() { xdotool windowfocus --sync "$(window_of "$1")"; wait_idle; }
close() { focus "$1"; key Escape; }
# click_in REGEX X Y: click at X,Y inside that window.
click_in() {
  xdotool mousemove --window "$(window_of "$1")" "$2" "$3"; sleep 0.2
  xdotool click 1; wait_idle
}
# Copy the fixture plugin (and optionally the registry cache) into the run's
# throwaway config.
install_demo_plugin() {
  mkdir -p "$XDG_CONFIG_HOME/ferrispad/plugins" "$XDG_CONFIG_HOME/ferrispad/cache"
  cp -r "$scenario_dir/plugins/dialogs-demo" "$XDG_CONFIG_HOME/ferrispad/plugins/"
  cp "$scenario_dir/plugins/registry-cache.json" "$XDG_CONFIG_HOME/ferrispad/cache/plugins.json"
}
# gen_fixture NAME BYTES: a deterministic plain-text fixture, generated once
# (gitignored), so the repository carries no megabyte files. shoot.sh
# sources scenarios under `set -euo pipefail`, and `yes` ends on SIGPIPE
# (status 141) when `head` has enough: `|| true` keeps that from killing
# the run (found by ferrispad-39 on a first run).
gen_fixture() {
  local f=$scenario_dir/fixtures/$1
  if [ "$(stat -c %s "$f" 2>/dev/null)" != "$2" ]; then
    { yes 'a line of plain text 0123456789' || true; } | head -c "$2" > "$f"
  fi
}
