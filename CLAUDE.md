# CLAUDE.md

Guidance for Claude Code when working in this repository. MikanLauncher is the top level launcher of the MikanXR package: a small Rust program with a Slint window that starts the MikanXR editor, a tracking app, and a character renderer, and installs the MikanXR plugin into VNyan or Warudo. [README.md](./README.md) covers usage, the package layout, the plugin zip contract, the gates, and packaging. Keep it current as those change.

## Layout

- `crates/mikan-launcher-core`: all the logic, with no window
	- `catalog.rs`: the app catalog (`catalog.json`)
	- `settings.rs`: the per-user settings file
	- `locate.rs`: finding an app's executable (browsed path, bundled, Steam)
	- `process.rs`: launch, running state by executable path, stop
	- `plugins.rs`: plugin state by content comparison, and install
	- `platform/`: the calls each OS spells its own way, the only place `unsafe` is allowed
	- `hub.rs`: the state behind the window, and what each slot shows and offers
- `crates/mikan-launcher`: the window (`ui/main.slint`) and its wiring to the hub
- `catalog.json`: the shipped catalog
- `components.json` and `tools/package.py`: the package assembly

## Working agreements

- New behavior goes in the core crate behind `Hub`, with a test under `crates/mikan-launcher-core/tests`. The window crate only maps hub state to the markup and callbacks to hub calls.
- Per-OS code goes in `platform/` behind a function every platform implements. The other modules stay free of `cfg` branches beyond a `cfg!(windows)` test.
- The launcher never starts a second copy of a running app. The editor does not survive one. Any new launch path keeps the running check.
- Plugin state is a content comparison. Do not add version parsing of compiled plugins or marker files inside a host app.
- `catalog.json` is data. A Steam app id or executable name is looked up and confirmed before it is added, never guessed.
- The window uses Slint's software renderer only. Do not add a GPU renderer feature.
- A new dependency passes `cargo deny check`. A license outside the allowlist is read before it is added to `deny.toml`.
- Run every gate in the README before calling a change done. Clippy's `all` and `pedantic` groups are warnings that fail CI, so a warning is fixed, not allowed away.
- Verification is objective: run the tests, and for a change to launch, stop, or path matching, run the drive against a real package.

## Git

- Git is handled by the user: query state read-only between tasks, and perform write operations only when asked.
- Commit messages are single line messages with no tags or attribution.

## Writing conventions

- Plain text only: no em dashes, no icons or emoji, sparing use of semicolons.
- Markdown paragraphs and list items stay on one line. Nested list items indent with tabs.
- Paths and path-like placeholders in markdown are backticked.
- Source comments state what the code needs in place and never point at a doc file.
