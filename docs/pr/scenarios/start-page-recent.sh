# Scenario: the start page with four recent files (T0108, after T0116).
#
# Four fixtures are opened on the command line, then every tab is closed,
# so the start page shows them in its Recent Files card (T0116's list).
# Session restore is off, so nothing else remembers them. Run it at a
# narrow size (one column) and a wide one (two columns):
#
#   .claude/skills/new-pr/compare.sh docs/pr/scenarios/start-page-recent.sh \
#       ~/Code/personal/ferrispad-wt/uitest-T0108/recent-narrow --size 500x600

SETTINGS_EXTRA='"session_restore": "Off"'
OPEN_FILES=(notes.txt todo.md 'notes@home.md' 'R&D.md')

scenario() {
  key ctrl+w
  key ctrl+w
  key ctrl+w
  key ctrl+w
  wait_idle
  shot start-page
}
