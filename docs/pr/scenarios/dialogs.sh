# Scenario: every dialog reachable from the menus and shortcuts (T0106, T0110).
#
# The fixture plugin plugins/dialogs-demo (a menu item, four settings and a
# command permission, already approved here) and a fresh copy of
# plugins/registry-cache.json (so the Plugin Manager's Official tab never
# fetches) go into the run's throwaway config at source time. Helpers:
# _dialog-helpers.sh.

SETTINGS_EXTRA='"plugins_enabled": true, "session_restore": "Off", "plugin_approvals": {"dialogs-demo": {"approved_commands": ["ruff"]}}'
OPEN_FILES=(hello.rs)

scenario_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=/dev/null
source "$scenario_dir/_dialog-helpers.sh"
install_demo_plugin

scenario() {
  editor; key ctrl+f;         shot find '^Find$';               close '^Find$'
  editor; key ctrl+h;         shot replace '^Find & Replace$';  close '^Find & Replace$'
  editor; key ctrl+g;         shot goto '^Go To Line$'
  # Line 99 of a 6-line file: the themed message popup (also "Go To Line").
  focus '^Go To Line$'; key 9 9 Return; shot goto-message '^Go To Line$'
  close '^Go To Line$';       close '^Go To Line$'
  editor; key ctrl+shift+n;   shot new-session '^New Session$'; close '^New Session$'
  # Edit → Key Shortcuts…
  click 61 14; click 95 257;  shot shortcuts '^Key Shortcuts$'; close '^Key Shortcuts$'
  # Format → Font → More Fonts…
  click 160 14; click 175 41; click 258 113; shot font-picker '^Choose Font$'; close '^Choose Font$'
  # Help → About FerrisPad
  click 280 14; click 316 41; shot about '^About FerrisPad$';  close '^About FerrisPad$'
  # Plugins → General → Plugin Manager…, then Settings…
  click 224 14; click 223 41; click 523 113; shot plugin-manager '^Plugin Manager$'
  click_in '^Plugin Manager$' 109 25; shot plugin-manager-official '^Plugin Manager$'; close '^Plugin Manager$'
  click 224 14; click 223 41; click 498 137; shot plugin-settings '^Plugin Settings$'; close '^Plugin Settings$'
  # Plugins → Dialogs Demo → Settings…
  click 224 14; click 244 89; click 498 113; shot plugin-config 'Settings$'; close 'Settings$'
}
