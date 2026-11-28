# Scenario: file-tree.sh on other built-in themes (T0109): InspiredGitHub (light)
# and Base16EightiesDark (dark). Together with file-tree.sh (the Base16 Ocean pair) the
# four runs cover the seven built-in themes.

. "$(dirname "${BASH_SOURCE[0]}")/file-tree.sh"
SETTINGS_EXTRA="$SETTINGS_EXTRA, \"syntax_theme_light\": \"InspiredGitHub\", \"syntax_theme_dark\": \"Base16EightiesDark\""
