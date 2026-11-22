# MikanLauncher

The top level launcher for the MikanXR package. It starts the MikanXR editor, a tracking app, and a character renderer from one window, lets you choose which app fills the tracking and renderer slots, and keeps the MikanXR plugin current inside VNyan or Warudo.

It is a small native program: Rust with a [Slint](https://slint.dev) window drawn by the software renderer, so it holds no GPU context beside the apps it starts. Windows is the first platform. The code that differs per platform sits behind one module so macOS and Linux can follow.

## Using it

Each of the three rows is a slot:

- **MikanXR**: the editor
- **Tracking App**: MikanTrack, XR Animator, Virtual Motion Capture, Webcam Motion Capture, or any program you pick under Custom
- **Character Renderer**: MikanStudio, VNyan, Warudo, or any program you pick under Custom

Pick "None" to leave a slot out. Apps shipped in the package are found on their own, and Warudo is found through your Steam library. For anything else, press Browse once and the launcher remembers where it is.

Launch starts a slot's app and Launch All starts every slot that is ready. A running app's button becomes Stop, which asks the app to close the way closing its window would. If it has not closed after five seconds the button becomes Force stop, which ends it without saving. The launcher never starts a second copy of an app that is already running, whoever started the first.

When VNyan or Warudo is selected the launcher compares the MikanXR plugin inside that app with the one in the package and offers Install or Update when they differ. It compares file contents, so it reports "differs" without claiming which copy is newer. VNyan also needs "Allow 3rd party Mods/Plugins" turned on in its Misc settings, and the launcher says so when it is off.

Your choices are kept in `%APPDATA%\MikanXR\Launcher\settings.json`.

## Package layout

The launcher expects this tree around it:

```
MikanLauncher.exe
catalog.json             the apps each slot offers
package.json             each bundled component's version
MikanXR/                 the editor
MikanTrack/
MikanStudio/
plugins/vnyan/           plugin.json and the plugin's files
plugins/warudo/          plugin.json and the plugin's files
```

`catalog.json` is data. Each entry names an app, its slot, the places to look for it (`bundled`, relative to the launcher, or `steam`, by app id), and the plugin folder it takes, if any. An entry with no sources is found through Browse.

## Plugin zip contract

A plugin repository's release zip has `plugin.json` at its root beside the files it names:

```json
{
    "id": "mikanxr-vnyan",
    "version": "0.2.0",
    "host": "vnyan",
    "files": [
        { "src": "MikanXR_VNyan.dll", "dest": "Items/Assemblies/MikanXR_VNyan.dll" },
        { "src": "mikanxr_shaders.bundle", "dest": "Items/Assemblies/MikanXR/mikanxr_shaders.bundle" }
    ]
}
```

`src` is relative to the zip and `dest` is relative to the host app's install folder, the one holding its executable. Both must be plain relative paths. `host` is the app's id in `catalog.json`.

## Building

`tools/setup-dev-environment.bat` prepares a Windows machine: Git, the MSVC C++ toolset when absent, rustup, the toolchain `rust-toolchain.toml` pins, and cargo-deny. Then:

```sh
cargo run -- --root .
```

`--root <dir>` names the package folder (the launcher's own folder by default), `--settings <file>` names another settings file, and `--status` prints each slot's state as JSON and exits without opening a window.

In VS Code, F5 builds and debugs the launcher through the configurations in `.vscode/launch.json`, which use the C/C++ extension's debugger. Tests get Run and Debug links above each test from rust-analyzer.

A run against the repository root has no bundled apps or plugins. `tools/dev_package.py` (the "Dev package" task) assembles `package/` from this machine's own builds instead: it links each app's build folder into the tree and copies each plugin's built files in beside a generated `plugin.json`. The "Launcher (package folder)" debug configuration then runs against it.

```sh
python tools/dev_package.py
cargo run -- --root package
```

It looks for each build in a checkout beside this one. A build that lives elsewhere is named on the command line or in `dev_package.local.json` at the repository root, which is not checked in:

```json
{ "mikanxr": "D:/src/MikanXR/build/src/Editor/RelWithDebInfo" }
```

The app folders in that tree are links to the builds, so a rebuilt app is picked up without running the script again. Remove the tree with `python tools/dev_package.py --clean`, which unlinks them, never with a recursive delete. Run the script again after editing `catalog.json` or rebuilding a plugin.

The gates, each also a VS Code task:

```sh
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
cargo deny check
```

The drive starts the real apps of a package, proves a second launch starts nothing, and stops them. It opens windows on the desktop, so it runs only when asked for:

```sh
MIKAN_LAUNCHER_DRIVE_ROOT=<package folder> MIKAN_LAUNCHER_DRIVE_SETTINGS=<settings file> cargo test -p mikan-launcher-core --test drive -- --ignored --nocapture
```

## Packaging

`tools/package.py` assembles the package tree: it fetches the release zip `components.json` pins for each component, extracts each to its place, and adds the launcher, the catalog, and `package.json`.

```sh
cargo build --release -p mikan-launcher
python tools/package.py --launcher target/release/MikanLauncher.exe --from-dir <folder of locally built zips>
```

`--from-dir` lets a zip built on this machine stand in for a release. The Package workflow runs the same script on a `v*` tag or by hand and keeps the tree as an artifact. Upload to Steam is not part of it yet.

MikanStudio is pinned at its first release. No other component has a published release yet, so their pins in `components.json` are empty. `MikanXR/MikanStudio` is a private repository, so its pin names a token of its own (`token_env`): the Package workflow passes the `MIKANSTUDIO_RELEASE_TOKEN` secret, a fine-grained token limited to that repository with read-only Contents, and a run on a developer's machine falls back to its own `gh` sign-in.

## Credits

The development setup script, the lint and release profile settings, and the dependency audit policy follow [aughra](https://github.com/tamarmstrong/aughra) by Tam Armstrong, used under its MIT license.

The window is built with Slint under its royalty-free desktop license. The About dialog carries its attribution.
