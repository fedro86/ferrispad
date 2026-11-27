#!/usr/bin/env bash
# Launch, drive and screenshot FerrisPad in a virtual X display (new-pr §2).
#
#   .claude/skills/new-pr/shoot.sh <binary> <outdir> <scenario.sh>
#       [--theme light|dark] [--font N] [--size WxH]
#   .claude/skills/new-pr/shoot.sh --selftest <outdir>
#
# - <binary>    a FerrisPad build, e.g. target/debug/FerrisPad (use debug:
#               fltk-rs debug asserts fire only there).
# - <outdir>    where the PNGs and the log go. It must be visible to the
#               host, e.g. under ~/Code/personal/ferrispad-wt/: the Flatpak
#               sandbox's /tmp is not the host's.
# - <scenario>  a bash file (see docs/pr/scenarios/README.md). It may set
#               SETTINGS_EXTRA (extra settings.json members) and OPEN_FILES
#               (fixture names opened on the command line), and must define
#               a function `scenario` that drives the app with the helpers
#               below.
# - --selftest  checks the isolation barriers below without starting
#               FerrisPad, and exits 0 only if they all hold.
#
# ISOLATION. The app must never reach the user's real desktop. On
# 2026-11-27 a scenario pressed Ctrl+O, FerrisPad spawned zenity, and zenity
# opened "Open Files" dialogs on the user's GNOME session instead of in Xvfb.
# Every run therefore has these barriers, and refuses to start without them:
#   1. a private D-Bus session bus (dbus-run-session): no portal, no
#      desktop services of the user's session;
#   2. a private, empty XDG_RUNTIME_DIR, no WAYLAND_DISPLAY/WAYLAND_SOCKET,
#      GDK_BACKEND=x11 and QT_QPA_PLATFORM=xcb: nothing it spawns can find
#      the real compositor's socket;
#   3. a shim directory first in PATH: zenity, kdialog, xdg-open, gio,
#      gnome-open, kde-open(5), wslview, x-www-browser, sensible-browser and
#      notify-send are stubs that log the call and exit 1, so no external
#      dialog, browser or opener starts;
#   4. no file dialogs in scenarios: fixtures open on the command line
#      (OPEN_FILES), and `key` refuses ctrl+o, ctrl+s and ctrl+shift+s;
#   5. the display is the Xvfb this run started (:90 and up), never :0/:1 or
#      the user's DISPLAY; update and plugin-update checks are off, so a
#      run makes no network requests;
#   6. the app runs in its own process group; the whole group is killed at
#      the end, and any process left over fails the run;
#   7. the app's environment is an allowlist (env -i) with its own HOME,
#      cwd, TMPDIR and SHELL=/bin/sh, and a per-run SHOOT_RUN_ID marker: a
#      process carrying it after the run (even one that left the group with
#      setsid) is killed and fails the run.
# Two runs started at the same moment may pick the same :9N display; the
# second Xvfb then fails to start and that run aborts (it fails safe).
# It also never reads or writes the user's settings, sessions, recent
# files or documents: fresh XDG_CONFIG_HOME, XDG_DATA_HOME, XDG_CACHE_HOME,
# the app started from $HOME, and only fixture files copied into the run's
# own folder.
#
# Helpers for the scenario (coordinates are relative to the main window):
#   click X Y          left click
#   dclick X Y         double click
#   key KEYS...        xdotool key names: ctrl+t, Escape, Return, Down
#   type "text"        type text into the focused widget
#   menu NAME          open a menu of the menu bar (File, Edit, View,
#                      Format, Plugins, Help); there is no window manager,
#                      so Alt shortcuts do not open menus
#   wait_idle [S]      wait until the app uses no CPU (at most S s, default 15)
#   shot NAME [REGEX]  PNG of the main window, or of the window whose title
#                      matches REGEX (a dialog); at most 1600 px wide
#   shot_screen NAME   PNG of the whole virtual screen
# Do not click items that open a file dialog or a browser (File → Open…,
# Save As…, the start page's Open File and Wiki buttons, Details links): the
# shims make them fail, but the shot is then of nothing useful.
#
# Exit status: 0 on success; 1 if a barrier is missing, the window never
# appears, the app exits or crashes during the scenario, the log shows a
# panic or an assert, a shim was called, or a process outlives the app.
set -euo pipefail

self=$(realpath "$0")

if [ -z "${SHOOT_INNER:-}" ]; then
  # Outer half: on the host, start a virtual X display and a private D-Bus
  # session with the tools and the Noto fonts, then run this script again
  # inside them. SHOOT_OUTER_BUS records the user's bus so the inner half
  # can prove it is not on it.
  args=$(printf ' %q' "$@")
  # The bus config has no <servicedir>: nothing (no portal) can be
  # auto-started on the private bus.
  host_cmd="set -e
export SHOOT_INNER=1 SHOOT_OUTER_BUS=\"\${DBUS_SESSION_BUS_ADDRESS:-unix:path=/run/user/\$(id -u)/bus}\"
export SHOOT_OUTER_DISPLAY=\"\${DISPLAY:-}\"
n=90; while [ -e /tmp/.X\$n-lock ] || [ -e /tmp/.X11-unix/X\$n ]; do n=\$((n + 1)); done
export SHOOT_DISPLAY=:\$n
export NOTO_FONTS=\$(nix build --no-link --print-out-paths nixpkgs#noto-fonts)
bus_conf=\$(mktemp \"\${TMPDIR:-/tmp}/ferrispad-shoot-bus.XXXXXX\")
trap 'rm -f -- \"\$bus_conf\"' EXIT
printf '%s\\n' '<busconfig><type>session</type><listen>unix:tmpdir=/tmp</listen><auth>EXTERNAL</auth>' \\
  '<policy context=\"default\"><allow send_destination=\"*\" eavesdrop=\"true\"/><allow eavesdrop=\"true\"/><allow own=\"*\"/></policy></busconfig>' > \"\$bus_conf\"
nix shell nixpkgs#xvfb-run nixpkgs#xdotool nixpkgs#imagemagick nixpkgs#dbus -c \\
  xvfb-run -n \$n -s '-screen 0 1920x1200x24' \\
  dbus-run-session --config-file=\"\$bus_conf\" -- bash $(printf %q "$self")$args"
  if [ -e /.flatpak-info ]; then
    exec flatpak-spawn --host bash -lc "$host_cmd"
  else
    exec bash -lc "$host_cmd"
  fi
fi

# ---- Inner half: on the host, inside Xvfb and a private D-Bus session. ------
usage() {
  echo "usage: shoot.sh <binary> <outdir> <scenario.sh> [--theme light|dark] [--font N] [--size WxH]" >&2
  echo "       shoot.sh --selftest <outdir>" >&2
  exit 2
}
selftest=""
if [ "${1:-}" = --selftest ]; then
  [ $# -eq 2 ] || usage
  selftest=1; out=$2; bin=""; scen=""
  theme=light; font=16; size=1100x740
else
  [ $# -ge 3 ] || usage
  bin=$(realpath "$1"); out=$2; scen=$(realpath "$3"); shift 3
  theme=light; font=16; size=1100x740
  while [ $# -gt 0 ]; do
    case "$1" in
      --theme) theme=$2; shift 2 ;;
      --font) font=$2; shift 2 ;;
      --size) size=$2; shift 2 ;;
      *) usage ;;
    esac
  done
  [ -x "$bin" ] || { echo "not an executable: $bin" >&2; exit 2; }
  [ -f "$scen" ] || { echo "no scenario: $scen" >&2; exit 2; }
fi
case "$theme" in light) mode=Light ;; dark) mode=Dark ;; *) usage ;; esac
win_w=${size%x*}; win_h=${size#*x}
mkdir -p "$out"; out=$(realpath "$out")

work=$(mktemp -d "${TMPDIR:-/tmp}/ferrispad-shoot.XXXXXX")
trap 'rm -rf -- "$work"' EXIT

# ---- barriers ------------------------------------------------------------------
refuse() { echo "shoot.sh: refusing to run: $*" >&2; exit 1; }

# 1. Private D-Bus session: dbus-run-session set a new address.
case "${DBUS_SESSION_BUS_ADDRESS:-}" in
  "") refuse "no private D-Bus session (DBUS_SESSION_BUS_ADDRESS is empty)" ;;
  "$SHOOT_OUTER_BUS") refuse "DBUS_SESSION_BUS_ADDRESS is the user's session bus" ;;
  *"/run/user/"*) refuse "DBUS_SESSION_BUS_ADDRESS points into /run/user: $DBUS_SESSION_BUS_ADDRESS" ;;
esac

# 2. No compositor: private, empty runtime dir; X11 only.
export XDG_RUNTIME_DIR=$work/runtime
mkdir -m 700 "$XDG_RUNTIME_DIR"
unset WAYLAND_DISPLAY WAYLAND_SOCKET
export GDK_BACKEND=x11 QT_QPA_PLATFORM=xcb FLTK_BACKEND=x11
# The display must be the Xvfb the outer half started (:90 and up), never
# the user's own X or XWayland display, which flatpak-spawn passes on.
display_ok() {
  case "${DISPLAY:-}" in :0|:0.*|:1|:1.*|"") return 1 ;; esac
  [ "${DISPLAY}" = "${SHOOT_DISPLAY:-}" ] && [ "${DISPLAY}" != "${SHOOT_OUTER_DISPLAY:-}" ]
}
display_ok || refuse "DISPLAY '${DISPLAY:-}' is not this run's Xvfb ('${SHOOT_DISPLAY:-}'); user display '${SHOOT_OUTER_DISPLAY:-}'"

# 3. Shims for everything that opens a dialog, a browser or a notification.
shims=$work/shims; shim_log=$work/shim.log
mkdir -p "$shims"; : > "$shim_log"
for tool in zenity kdialog xdg-open gio gnome-open kde-open kde-open5 wslview \
            x-www-browser sensible-browser notify-send; do
  printf '#!/bin/sh\necho "%s $*" >> %q\nexit 1\n' "$tool" "$shim_log" > "$shims/$tool"
  chmod +x "$shims/$tool"
done
export PATH=$shims:$PATH

# Fresh config, data and cache; fixtures only.
export XDG_CONFIG_HOME=$work/config XDG_DATA_HOME=$work/data XDG_CACHE_HOME=$work/cache
mkdir -p "$XDG_CONFIG_HOME/ferrispad" "$XDG_DATA_HOME" "$XDG_CACHE_HOME" "$work/files" \
  "$work/home" "$work/tmp"

# 7. The app's whole environment is an allowlist (env -i): nothing of the
# user's session (AT_SPI_BUS_ADDRESS, SESSION_MANAGER, BROWSER, …) leaks in.
# HOME, cwd and TMPDIR are the run's own: the terminal panel would source the
# real ~/.bashrc, Markdown preview writes into $TMPDIR, and the MCP project
# root is the cwd. SHOOT_RUN_ID marks every process the run starts, also a
# child that leaves the process group with setsid.
run_id=shoot-$(basename "$work")
app_env=(
  "PATH=$shims:$PATH" "HOME=$work/home" "SHELL=/bin/sh" "TMPDIR=$work/tmp"
  "DISPLAY=$DISPLAY" "XAUTHORITY=${XAUTHORITY:-}"
  "DBUS_SESSION_BUS_ADDRESS=$DBUS_SESSION_BUS_ADDRESS" "XDG_RUNTIME_DIR=$XDG_RUNTIME_DIR"
  "XDG_CONFIG_HOME=$XDG_CONFIG_HOME" "XDG_DATA_HOME=$XDG_DATA_HOME" "XDG_CACHE_HOME=$XDG_CACHE_HOME"
  "FONTCONFIG_FILE=$work/fonts.conf" "GDK_BACKEND=x11" "QT_QPA_PLATFORM=xcb" "FLTK_BACKEND=x11"
  "LANG=C.UTF-8" "SHOOT_RUN_ID=$run_id"
)
# Processes that carry this run's marker (kill and report them).
marked() {
  local f p
  for f in /proc/[0-9]*/environ; do
    p=${f#/proc/}; p=${p%/environ}
    [ "$p" = "$$" ] && continue
    grep -qa "SHOOT_RUN_ID=$run_id" "$f" 2>/dev/null && echo "$p"
  done
  return 0
}

# A real font: Noto Sans / Noto Sans Mono from nixpkgs, preferred over
# whatever the host has, so shots are comparable between machines.
cat > "$work/fonts.conf" <<EOF
<?xml version="1.0"?>
<!DOCTYPE fontconfig SYSTEM "fonts.dtd">
<fontconfig>
  <include ignore_missing="yes">/etc/fonts/fonts.conf</include>
  <dir>${NOTO_FONTS:-/nonexistent}/share/fonts</dir>
  <cachedir>$work/fc-cache</cachedir>
  <alias binding="strong"><family>sans-serif</family><prefer><family>Noto Sans</family></prefer></alias>
  <alias binding="strong"><family>Helvetica</family><prefer><family>Noto Sans</family></prefer></alias>
  <alias binding="strong"><family>monospace</family><prefer><family>Noto Sans Mono</family></prefer></alias>
  <alias binding="strong"><family>Courier</family><prefer><family>Noto Sans Mono</family></prefer></alias>
</fontconfig>
EOF
export FONTCONFIG_FILE=$work/fonts.conf

# 4. Keys that open a file dialog are refused (also used by --selftest).
forbidden_key() {
  local k
  for k in "$@"; do
    case "${k,,}" in
      ctrl+o|ctrl+s|ctrl+shift+s|shift+ctrl+s) return 0 ;;
    esac
  done
  return 1
}

if [ -n "$selftest" ]; then
  log=$out/selftest.log
  fail=0
  check() { if eval "$2"; then echo "ok   $1"; else echo "FAIL $1"; fail=1; fi; }
  {
    echo "DBUS_SESSION_BUS_ADDRESS=$DBUS_SESSION_BUS_ADDRESS"
    echo "user bus=$SHOOT_OUTER_BUS"
    echo "XDG_RUNTIME_DIR=$XDG_RUNTIME_DIR DISPLAY=$DISPLAY (user: ${SHOOT_OUTER_DISPLAY:-none}) GDK_BACKEND=$GDK_BACKEND"
    check "private bus differs from the user's" '[ "$DBUS_SESSION_BUS_ADDRESS" != "$SHOOT_OUTER_BUS" ]'
    check "no portal on the private bus" '! dbus-send --session --print-reply --dest=org.freedesktop.portal.Desktop /org/freedesktop/portal/desktop org.freedesktop.DBus.Peer.Ping >/dev/null 2>&1'
    check "runtime dir is empty and private" '[ -z "$(ls -A "$XDG_RUNTIME_DIR")" ] && [ "$(stat -c %a "$XDG_RUNTIME_DIR")" = 700 ]'
    check "no WAYLAND_DISPLAY" '[ -z "${WAYLAND_DISPLAY:-}" ]'
    check "DISPLAY is this run's Xvfb, not the user's" 'display_ok && [ "$DISPLAY" != "${SHOOT_OUTER_DISPLAY:-}" ]'
    check "the display guard refuses :0 and :1" '! (DISPLAY=:0; display_ok) && ! (DISPLAY=:1; display_ok) && ! (SHOOT_DISPLAY=:0 DISPLAY=:0; display_ok)'
    check "zenity is the shim and fails" '[ "$(command -v zenity)" = "$shims/zenity" ] && ! zenity --info --text=selftest'
    check "xdg-open is the shim and fails" '[ "$(command -v xdg-open)" = "$shims/xdg-open" ] && ! xdg-open https://example.invalid'
    check "the shim logged both calls" 'grep -q "^zenity --info" "$shim_log" && grep -q "^xdg-open https" "$shim_log"'
    check "key refuses ctrl+o" 'forbidden_key ctrl+o'
    check "key refuses ctrl+shift+s" 'forbidden_key ctrl+shift+s'
    check "key allows ctrl+t" '! forbidden_key ctrl+t'
    # The app's environment: only the allowlist, with private HOME and TMPDIR.
    appenv=$(env -i "${app_env[@]}" /usr/bin/env)
    check "app env drops the session's variables" '! grep -qE "^(WAYLAND_DISPLAY|AT_SPI_BUS_ADDRESS|SESSION_MANAGER|BROWSER|XDG_SESSION|GNOME_|SSH_AUTH_SOCK)" <<<"$appenv"'
    check "app env has a private HOME, TMPDIR and SHELL=/bin/sh" 'grep -qx "HOME=$work/home" <<<"$appenv" && grep -qx "TMPDIR=$work/tmp" <<<"$appenv" && grep -qx "SHELL=/bin/sh" <<<"$appenv"'
    check "app env puts the shims first in PATH" 'grep -q "^PATH=$shims:" <<<"$appenv"'
    check "the real font still resolves with the private HOME" '[ "$(env -i "${app_env[@]}" fc-match -f "%{family[0]}" sans)" = "Noto Sans" ]'
    # A child that leaves the process group is still found by its marker.
    env -i "${app_env[@]}" setsid sleep 60 &
    disown
    sleep 0.5
    stray=$(marked)
    check "the marker sweep finds a setsid child" '[ -n "$stray" ]'
    kill -9 $stray 2>/dev/null || true; sleep 0.3
    check "the marker sweep is clean after the kill" '[ -z "$(marked)" ]'
  } 2>&1 | tee "$log"
  grep -q '^FAIL' "$log" && exit 1
  exit 0
fi

# ---- the run -------------------------------------------------------------------
SETTINGS_EXTRA=""
OPEN_FILES=()
# shellcheck source=/dev/null
source "$scen"
declare -F scenario >/dev/null || { echo "$scen defines no scenario function" >&2; exit 2; }
fixtures=$(dirname "$scen")/fixtures
args=()
for f in "${OPEN_FILES[@]}"; do
  [ -f "$fixtures/$f" ] || { echo "no fixture: $fixtures/$f" >&2; exit 2; }
  cp "$fixtures/$f" "$work/files/"
  args+=("$work/files/$f")
done
extra=${SETTINGS_EXTRA:+, $SETTINGS_EXTRA}
cat > "$XDG_CONFIG_HOME/ferrispad/settings.json" <<EOF
{"theme_mode": "$mode", "font_size": $font, "tabs_enabled": true,
 "auto_check_updates": false, "auto_check_plugin_updates": false,
 "window": {"x": 0, "y": 0, "width": $win_w, "height": $win_h, "maximized": false}$extra}
EOF

name=$(basename "$scen" .sh)
log=$out/$name-$theme.log
{
  echo "binary: $bin"
  echo "scenario: $scen  theme: $theme  font: $font  size: $size"
  echo "open: ${OPEN_FILES[*]:-(none)}"
  echo "sans: $(fc-match sans)  mono: $(fc-match monospace)"
  echo "bus: $DBUS_SESSION_BUS_ADDRESS  runtime: $XDG_RUNTIME_DIR  display: $DISPLAY"
  echo "settings: $(tr -d '\n' < "$XDG_CONFIG_HOME/ferrispad/settings.json")"
  echo "---- app stdout/stderr ----"
} > "$log"

# 6. Own process group, so the end of the run can kill everything it spawned;
# the SHOOT_RUN_ID sweep below catches what leaves the group.
(cd "$work/files" && exec env -i "${app_env[@]}" setsid "$bin" "${args[@]}") >>"$log" 2>&1 &
pid=$!

stop_group() {
  kill -- -"$pid" 2>/dev/null || kill "$pid" 2>/dev/null || true
  sleep 0.5
  kill -9 -- -"$pid" 2>/dev/null || true
}
die() {
  echo "shoot.sh: $*" | tee -a "$log" >&2
  stop_group
  exit 1
}

# Wait for the main window. xdotool ORs its criteria unless --all is given.
wid=""
for _ in $(seq 1 60); do
  kill -0 "$pid" 2>/dev/null || die "the app exited before its window appeared (see $log)"
  wid=$(xdotool search --all --onlyvisible --pid "$pid" --name 'FerrisPad' 2>/dev/null | head -1 || true)
  [ -n "$wid" ] && break
  sleep 0.5
done
[ -n "$wid" ] || die "no FerrisPad window after 30 s (see $log)"
xdotool windowfocus --sync "$wid" 2>/dev/null || true

# ---- helpers ----------------------------------------------------------------
cpu_ticks() { awk '{print $14 + $15}' "/proc/$pid/stat" 2>/dev/null || echo gone; }
wait_idle() {
  local limit=${1:-15} last now i
  last=$(cpu_ticks)
  for i in $(seq 1 $((limit * 4))); do
    sleep 0.25
    now=$(cpu_ticks)
    [ "$now" = gone ] && die "the app exited during the scenario (see $log)"
    if [ "$now" = "$last" ]; then sleep 0.25; return 0; fi
    last=$now
  done
  echo "shoot.sh: still busy after ${limit}s" >> "$log"
}
# FLTK must see the pointer arrive before the button: a click chained to the
# move in one xdotool call is lost. Hence the separate calls and the pause.
point() { xdotool mousemove --window "$wid" "$1" "$2"; sleep 0.2; }
click() { point "$1" "$2"; xdotool click 1; wait_idle; }
dclick() { point "$1" "$2"; xdotool click --repeat 2 --delay 80 1; wait_idle; }
key() {
  forbidden_key "$@" && die "key $* would open a file dialog: open fixtures with OPEN_FILES instead"
  xdotool key --clearmodifiers "$@"; wait_idle
}
type() { xdotool type --delay 25 -- "$1"; wait_idle; }
menu() {
  local order=(File Edit View Format Plugins Help) i n=-1
  for i in "${!order[@]}"; do [ "${order[$i]}" = "$1" ] && n=$i; done
  [ "$n" -ge 0 ] || die "unknown menu: $1"
  point 20 12
  xdotool click 1
  sleep 0.3
  for ((i = 0; i < n; i++)); do xdotool key Right; sleep 0.1; done
  wait_idle
}
save_png() {
  magick "$1" -resize '1600x>' -strip -define png:compression-level=9 "$1"
  echo "shot: $1 ($2)" | tee -a "$log"
}
shot() {
  local file=$out/$1-$theme.png target=$wid geo x y w h
  if [ -n "${2:-}" ]; then
    target=$(xdotool search --all --onlyvisible --pid "$pid" --name "$2" 2>/dev/null | tail -1 || true)
    [ -n "$target" ] || die "no window matching '$2' for shot $1"
  fi
  geo=$(xdotool getwindowgeometry --shell "$target")
  x=$(sed -n 's/^X=//p' <<<"$geo"); y=$(sed -n 's/^Y=//p' <<<"$geo")
  w=$(sed -n 's/^WIDTH=//p' <<<"$geo"); h=$(sed -n 's/^HEIGHT=//p' <<<"$geo")
  # Crop the screen, not the window, so open menus and popups over it show.
  import -window root -crop "${w}x${h}+${x}+${y}" +repage "$file"
  save_png "$file" "${w}x${h}"
}
shot_screen() {
  local file=$out/$1-$theme.png
  import -window root "$file"
  save_png "$file" screen
}

# ---- run --------------------------------------------------------------------
wait_idle
# Without a window manager FLTK takes keyboard focus only from a click:
# click an empty spot near the bottom-right corner (start-page background
# or editor text area) before the scenario starts.
click "$((win_w - 40))" "$((win_h - 60))"
scenario
kill -0 "$pid" 2>/dev/null || die "the app exited during the scenario (see $log)"
stop_group
wait "$pid" 2>/dev/null || true
status=0
if pgrep -g "$pid" >/dev/null 2>&1; then
  echo "shoot.sh: processes outlived the app: $(pgrep -a -g "$pid" | tr '\n' ';')" | tee -a "$log" >&2
  kill -9 -- -"$pid" 2>/dev/null || true
  status=1
fi
left=$(marked)
if [ -n "$left" ]; then
  echo "shoot.sh: processes with this run's marker outlived it: $(for p in $left; do tr '\0' ' ' < /proc/$p/cmdline 2>/dev/null; echo ';'; done)" | tee -a "$log" >&2
  kill -9 $left 2>/dev/null || true
  status=1
fi
if [ -s "$shim_log" ]; then
  { echo "shoot.sh: the app called a shimmed tool:"; cat "$shim_log"; } | tee -a "$log" >&2
  status=1
fi
if grep -n -E "panicked|assertion|Assertion|Segmentation fault|core dumped" "$log" >&2; then
  echo "shoot.sh: the log shows a panic or an assert (see $log)" >&2
  status=1
fi
[ "$status" = 0 ] && echo "ok: $name ($theme) -> $out" | tee -a "$log"
exit "$status"
