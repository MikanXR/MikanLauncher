#!/usr/bin/env python3
"""Assemble a package tree from this machine's own builds, for running and debugging.

Where tools/package.py builds the tree from release zips, this builds it from local
build folders: each app's folder is linked into the tree (a junction on Windows, a
symlink elsewhere), so a rebuild of the app is picked up with no second run, and
each plugin's built files are copied in beside a plugin.json.

Usage: python tools/dev_package.py [--out <dir>] [--clean]
                                   [--mikanxr <dir>] [--mikantrack <dir>]
                                   [--mikanstudio <dir>]
                                   [--vnyan-plugin <repo>] [--warudo-plugin <repo>]
  --out <dir>             the tree to write, package/ by default; replaced if present
  --clean                 remove the tree and exit
  --mikanxr <dir>         the folder holding Mikan.exe
  --mikantrack <dir>      the folder holding MikanTrack.exe
  --mikanstudio <dir>     the folder holding mikan-studio.exe
  --vnyan-plugin <repo>   the MikanXR_VNyan checkout, built
  --warudo-plugin <repo>  the MikanXR_Warudo checkout, built

A path not given on the command line is read from dev_package.local.json at the
repository root, a file each machine keeps for itself:

    { "mikanxr": "D:/src/MikanXR/build/src/Editor/RelWithDebInfo" }

and after that from a checkout beside this one. A component that is not found is
reported and left out, so the tree holds whatever this machine has built.

The tree's app folders are links. Remove the tree with --clean, which unlinks them,
and never with a recursive delete that might follow them into the builds.
"""

import argparse
import json
import os
import re
import shutil
import stat
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
SIBLINGS = REPO.parent
LOCAL_CONFIG = REPO / "dev_package.local.json"

# Each app: its option, its folder in the tree, the executable that proves the folder is
# the right one, and where a checkout beside this one would have built it.
APPS = [
    ("mikanxr", "MikanXR", "Mikan.exe", SIBLINGS / "MikanXR/build/src/Editor/RelWithDebInfo"),
    ("mikantrack", "MikanTrack", "MikanTrack.exe", SIBLINGS / "MikanTrack/build/Release"),
    ("mikanstudio", "MikanStudio", "mikan-studio.exe", None),
]

# Each plugin: its option, its folder under plugins/, the manifest it gets when its
# repository ships none of its own, and where its version is written.
PLUGINS = [
    {
        "option": "vnyan_plugin",
        "folder": "vnyan",
        "default": SIBLINGS / "MikanXR_VNyan",
        "id": "mikanxr-vnyan",
        "host": "vnyan",
        "files": [
            ("bin/Release/netstandard2.1/MikanXR_VNyan.dll", "Items/Assemblies/MikanXR_VNyan.dll"),
            ("Build/mikanxr_shaders.bundle", "Items/Assemblies/MikanXR/mikanxr_shaders.bundle"),
        ],
        "version_file": "MikanXR_VNyan.csproj",
        "version_pattern": r"<Version>([^<]+)</Version>",
    },
    {
        "option": "warudo_plugin",
        "folder": "warudo",
        "default": SIBLINGS / "MikanXR_Warudo",
        "id": "mikanxr-warudo",
        "host": "warudo",
        "files": [
            ("Mod/MikanXR.warudo", "Warudo_Data/StreamingAssets/Plugins/MikanXR.warudo"),
        ],
        "version_file": "Assets/MikanXR/Plugin/MikanXRPlugin.cs",
        "version_pattern": r'Version\s*=\s*"([^"]+)"',
    },
]


def is_link(path):
    """Whether the path is a symlink or a Windows junction, either of which is unlinked
    rather than deleted through. A junction is a reparse point, which is read from the
    path's own attributes since older Pythons have no test for one."""
    if path.is_symlink():
        return True

    attributes = getattr(os.lstat(path), "st_file_attributes", 0)

    return bool(attributes & stat.FILE_ATTRIBUTE_REPARSE_POINT)


def remove_tree(out):
    """Removes the tree without following its links into the builds they point at."""
    if not out.exists():
        return

    for entry in out.iterdir():
        if is_link(entry):
            # A junction and a directory symlink are both removed as an empty folder is.
            if entry.is_dir():
                os.rmdir(entry)
            else:
                entry.unlink()
        elif entry.is_dir():
            shutil.rmtree(entry)
        else:
            entry.unlink()

    out.rmdir()


def link_folder(target, link):
    """Makes `link` a folder that is `target`. A junction needs no elevation on Windows,
    where a symlink does."""
    if os.name == "nt":
        made = subprocess.run(
            ["cmd", "/c", "mklink", "/J", str(link), str(target)],
            capture_output=True,
            text=True,
            check=False,
        )

        if made.returncode != 0:
            raise OSError(made.stderr.strip() or made.stdout.strip())
    else:
        link.symlink_to(target, target_is_directory=True)


def plugin_version(plugin, repo):
    """The version the plugin's source declares, or a placeholder when it cannot be read."""
    try:
        text = (repo / plugin["version_file"]).read_text(encoding="utf-8", errors="replace")
    except OSError:
        return "local build"

    found = re.search(plugin["version_pattern"], text)

    return found.group(1) if found else "local build"


def add_plugin(plugin, repo, dest):
    """Copies a plugin's built files and writes its manifest. Returns the version, or
    None with the reason when a built file is missing."""
    missing = [src for src, _ in plugin["files"] if not (repo / src).is_file()]

    if missing:
        return None, f"not built: {repo / missing[0]} is missing"

    dest.mkdir(parents=True)

    files = []

    for src, host_path in plugin["files"]:
        name = Path(src).name

        shutil.copy2(repo / src, dest / name)
        files.append({"src": name, "dest": host_path})

    version = plugin_version(plugin, repo)
    manifest = {"id": plugin["id"], "version": version, "host": plugin["host"], "files": files}

    (dest / "plugin.json").write_text(json.dumps(manifest, indent=4) + "\n", encoding="utf-8")

    return version, None


def main():
    parser = argparse.ArgumentParser(description="Assemble a package tree from local builds.")
    parser.add_argument("--out", type=Path, default=REPO / "package")
    parser.add_argument("--clean", action="store_true")

    for option, _, _, _ in APPS:
        parser.add_argument(f"--{option}", type=Path)

    for plugin in PLUGINS:
        parser.add_argument(f"--{plugin['option'].replace('_', '-')}", type=Path)

    arguments = parser.parse_args()
    out = arguments.out.resolve()

    remove_tree(out)

    if arguments.clean:
        print(f"removed {out}")
        return 0

    local = {}

    if LOCAL_CONFIG.is_file():
        local = json.loads(LOCAL_CONFIG.read_text(encoding="utf-8"))

    def source(option, default):
        """The path for an option: the command line, then the local file, then a sibling."""
        given = getattr(arguments, option) or local.get(option) or default

        return Path(given).resolve() if given else None

    out.mkdir(parents=True)
    shutil.copy2(REPO / "catalog.json", out / "catalog.json")

    versions = {}
    lines = []

    for option, folder, exe, default in APPS:
        target = source(option, default)

        if target is None:
            lines.append(f"- {folder}: skipped, no folder given (--{option})")
        elif not (target / exe).is_file():
            lines.append(f"- {folder}: skipped, no {exe} in {target}")
        else:
            link_folder(target, out / folder)
            versions[folder] = "local build"
            lines.append(f"- {folder}: linked to {target}")

    for plugin in PLUGINS:
        repo = source(plugin["option"], plugin["default"])
        name = f"plugins/{plugin['folder']}"
        version, reason = add_plugin(plugin, repo, out / "plugins" / plugin["folder"])

        if version is None:
            lines.append(f"- {name}: skipped, {reason}")
        else:
            versions[plugin["id"]] = version
            lines.append(f"- {name}: {version} from {repo}")

    (out / "package.json").write_text(
        json.dumps({"components": versions}, indent=4) + "\n", encoding="utf-8"
    )

    print(f"Dev package at {out}:")
    print("\n".join(lines))

    return 0


if __name__ == "__main__":
    sys.exit(main())
