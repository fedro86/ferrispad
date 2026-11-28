# Scenario: the large-file warning (T0106, T0110): a generated 1.5 MB
# fixture over a 1 MB warning threshold. Helpers: _dialog-helpers.sh.

SETTINGS_EXTRA='"session_restore": "Off", "large_file_warning_mb": 1, "max_editable_size_mb": 2'
OPEN_FILES=(generated-1500k.log)

scenario_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=/dev/null
source "$scenario_dir/_dialog-helpers.sh"
gen_fixture generated-1500k.log 1536000

scenario() {
  shot large-file-warning '^Large File Warning$'
  close '^Large File Warning$'
}
