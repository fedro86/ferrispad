# Scenario: the file tree with a selected row (T0104, T0109).
#
# The fixture plugin plugins/tree-demo shows a tree whose rows carry the
# git-status label colors (modified, added, conflict, ignored, plain). It is
# copied into the run's throwaway config dir here, at source time; shoot.sh
# sources a scenario after creating that dir and before starting the app.

SETTINGS_EXTRA='"plugins_enabled": true'
OPEN_FILES=(hello.rs)

scenario_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
mkdir -p "$XDG_CONFIG_HOME/ferrispad/plugins"
cp -r "$scenario_dir/plugins/tree-demo" "$XDG_CONFIG_HOME/ferrispad/plugins/"

scenario() {
  wait_idle 20
  shot tree-unselected
  # "modified.rs", the second row (the ticket's hard case on light themes).
  click 110 119
  shot tree-selected
}
