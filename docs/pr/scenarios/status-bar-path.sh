# Scenario: the status bar with a long non-ASCII file path (T0124). The
# fixture's name is mostly Japanese, so the path's left cut falls inside a
# multi-byte character at many widths. The window narrows step by step; one
# shot per width. Before T0124, FerrisPad panics at one of these widths.

SETTINGS_EXTRA='"session_restore": "Off"'
OPEN_FILES=("メモ-日本語のノート-プレゼンテーション資料-最終版-Größenänderung.txt")

scenario() {
  local w
  for w in 900 820 760 700 640 580; do
    xdotool windowsize "$wid" "$w" 400
    wait_idle
    shot "width-$w"
  done
}
