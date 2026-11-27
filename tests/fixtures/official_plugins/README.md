# Official plugin manifests (T0073)

Verbatim copies of `plugin.toml` from each official plugin in
[fedro86/ferrispad-plugins](https://github.com/fedro86/ferrispad-plugins),
taken at commit `cce6006`, with `rust-lint` at 1.5.1 from ferrispad-plugins
PR #1 (branch `fix/rust-lint-shortcut`, `adb75f5`). The unit test
`plugin_shortcut_conflicts` (`src/ui/menu.rs`) binds every declared
menu-item shortcut against the core shortcut table the way the Plugins menu
does, and fails on a clash.

When an official plugin changes a shortcut or a new official plugin is
added, copy its `plugin.toml` here (one directory per plugin, named like the
plugin's directory in that repo) and note the new commit above.
