# Scenario: "File Too Large" and the read-only viewer (T0106, T0110): a
# generated 3 MB fixture over a 2 MB editable limit, then "View Read-Only".
# Helpers: _dialog-helpers.sh.

SETTINGS_EXTRA='"session_restore": "Off", "large_file_warning_mb": 1, "max_editable_size_mb": 2'
OPEN_FILES=(generated-3000k.log)

scenario_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=/dev/null
source "$scenario_dir/_dialog-helpers.sh"
gen_fixture generated-3000k.log 3072000

scenario() {
  shot too-large '^File Too Large$'
  # "View Read-Only", the first button under the message.
  click_in '^File Too Large$' 225 115
  shot readonly-viewer 'Read-Only Viewer\)$'
  close 'Read-Only Viewer\)$'
}
