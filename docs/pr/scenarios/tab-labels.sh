# Scenario: a file named "Untitled" next to a new untitled document (T0112).
#
# The fixture `Untitled` opens on the command line; File > New (Ctrl+T, tabs on) then
# adds an untitled tab. Before T0112 both tabs read "Untitled"; after, the
# file's tab shows its folder ("Untitled (files)").

SETTINGS_EXTRA=""
OPEN_FILES=(Untitled)

scenario() {
  key ctrl+t
  shot tab-labels
}
