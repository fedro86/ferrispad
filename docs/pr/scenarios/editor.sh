# Scenario: the editor with a fixture file, a menu and the Find dialog.
#
# A model for driving the editor. The fixture opens on the command line
# (OPEN_FILES): scenarios never open a file dialog (see shoot.sh, ISOLATION).

SETTINGS_EXTRA=""
OPEN_FILES=(hello.rs)

scenario() {
  shot editor
  menu View
  shot view-menu
  key Escape
  key ctrl+f
  shot find-dialog 'Find'
  key Escape
}
