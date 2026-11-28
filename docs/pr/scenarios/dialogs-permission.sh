# Scenario: the plugin permission request at startup (T0106, T0110). The
# fixture plugin asks to run `ruff`, not approved yet. Helpers:
# _dialog-helpers.sh.

SETTINGS_EXTRA='"plugins_enabled": true, "session_restore": "Off"'
OPEN_FILES=(hello.rs)

scenario_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=/dev/null
source "$scenario_dir/_dialog-helpers.sh"
install_demo_plugin

scenario() {
  shot permission '^Plugin Permission Request$'
  close '^Plugin Permission Request$'
}
