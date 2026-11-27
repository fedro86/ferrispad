# Scenario: the recent-files list (T0116).
#
# Two fixtures are opened on the command line, then both tabs are closed.
# Session restore is off, so nothing else remembers them. Before T0116 the
# start page's Recent Files card is empty and the File menu has no Open
# Recent; after it, both files are listed, most recent first.

SETTINGS_EXTRA='"session_restore": "Off"'
OPEN_FILES=(notes.txt todo.md)

scenario() {
  key ctrl+w
  key ctrl+w
  shot start-page
  menu File
  shot file-menu
  # New, Open..., Open Recent, then into the submenu.
  key Down Down Down
  key Right
  shot open-recent
  key Escape Escape
}
