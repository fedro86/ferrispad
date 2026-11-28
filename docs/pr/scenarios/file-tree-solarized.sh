# Scenario: file-tree.sh on other built-in themes (T0109): SolarizedLight (light)
# and SolarizedDark (dark). Together with file-tree.sh (the Base16 Ocean pair) the
# four runs cover the seven built-in themes.

. "$(dirname "${BASH_SOURCE[0]}")/file-tree.sh"
SETTINGS_EXTRA="$SETTINGS_EXTRA, \"syntax_theme_light\": \"SolarizedLight\", \"syntax_theme_dark\": \"SolarizedDark\""
