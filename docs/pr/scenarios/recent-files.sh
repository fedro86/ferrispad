# Scenario: the recent-files list (T0116).
#
# Four fixtures are opened on the command line, then all tabs are closed.
# Session restore is off, so nothing else remembers them. Before T0116 the
# start page's Recent Files card is empty and the File menu has no Open
# Recent; after it, the files are listed, most recent first.

SETTINGS_EXTRA='"session_restore": "Off"'
# notes@home.md and R&D.md: FLTK treats '@' and '&' as specials in labels.
OPEN_FILES=(notes.txt todo.md 'notes@home.md' 'R&D.md')

scenario() {
  key ctrl+w
  key ctrl+w
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
