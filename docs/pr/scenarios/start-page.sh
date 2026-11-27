# Scenario: the start page on a fresh install (T0114).
#
# No recent files and no sessions: the throwaway config and data dirs of
# shoot.sh are empty, so the start page shows its empty Recent Sessions and
# Recent Files cards next to the Changelog card. Before T0114 that card cut
# off its sixth entry in exactly this state.
#
#   .claude/skills/new-pr/compare.sh docs/pr/scenarios/start-page.sh \
#       ~/Code/personal/ferrispad-wt/uitest-T0114 --before 0.9.10-rc.2

# Extra settings.json members (JSON, without braces). Tabs must be on for
# the start page; shoot.sh already sets that, so nothing is added here.
SETTINGS_EXTRA=""

scenario() {
  wait_idle
  shot start-page
}
