#!/usr/bin/env python3
"""Assemble the package tree the launcher ships in.

Reads components.json, fetches each component's pinned release zip with the gh
CLI, extracts it to its place in the tree, copies in the launcher and its
catalog, and writes package.json naming each component's version.

Usage: python tools/package.py --launcher <MikanLauncher exe> [--out <dir>]
                               [--components <file>] [--from-dir <dir>]
  --launcher <exe>     the built launcher to ship
  --out <dir>          the tree to write, package/ by default; replaced if present
  --components <file>  the pins, components.json by default
  --from-dir <dir>     a folder searched for each asset before GitHub, so a zip
                       built on this machine can stand in for a release

A component in a private repository names, as "token_env", the environment
variable holding a token that can read its releases. Its download runs under that
token when the variable is set, and under the caller's own gh sign-in when it is
not, so a run on a developer's machine needs no extra setup.

A component whose pin has no tag, or whose asset cannot be fetched, is a gap. A
gap in an optional component is reported and skipped. A gap in a required one
fails the run, and so does any archive that is unreadable or holds an entry
outside its folder. Exits 0 when the whole tree was written and 1 otherwise.
"""

import argparse
import fnmatch
import json
import os
import shutil
import subprocess
import sys
import tempfile
import zipfile
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent


def find_asset(component, from_dir, scratch):
    """The component's zip and its version, or None with the reason it is absent."""
    pattern = component["asset"]

    if from_dir is not None:
        matches = sorted(p for p in from_dir.iterdir() if fnmatch.fnmatch(p.name, pattern))

        if matches:
            return matches[-1], version_of(matches[-1].name, pattern, component.get("tag")), None

    tag = component.get("tag")

    if not tag:
        return None, None, "no release is pinned"

    target = scratch / component["name"]
    target.mkdir()

    fetched = subprocess.run(
        ["gh", "release", "download", tag, "-R", component["repo"], "-p", pattern, "-D", str(target)],
        capture_output=True,
        text=True,
        check=False,
        env=download_environment(component),
    )

    if fetched.returncode != 0:
        return None, None, fetched.stderr.strip() or "the download failed"

    matches = sorted(target.iterdir())

    if not matches:
        return None, None, f"release {tag} has no asset matching {pattern}"

    return matches[-1], version_of(matches[-1].name, pattern, tag), None


def download_environment(component):
    """The environment gh downloads the component under: the caller's, with the
    component's own token as GH_TOKEN when its "token_env" variable is set."""
    environment = dict(os.environ)
    token = os.environ.get(component.get("token_env") or "", "")

    if token:
        environment["GH_TOKEN"] = token

    return environment


def version_of(name, pattern, tag):
    """The version a tag names, or the part of the asset's name its wildcard matched."""
    if tag:
        return tag.removeprefix("v")

    head, _, tail = pattern.partition("*")

    return name[len(head) : len(name) - len(tail)] or "unknown"


def extract(archive, dest):
    """Extracts a zip, refusing any entry that would land outside its destination."""
    dest.mkdir(parents=True, exist_ok=True)
    root = dest.resolve()

    with zipfile.ZipFile(archive) as bundle:
        for entry in bundle.infolist():
            target = (root / entry.filename).resolve()

            if root != target and root not in target.parents:
                raise ValueError(f"{archive.name} holds an entry outside its folder: {entry.filename}")

        bundle.extractall(root)


def main():
    parser = argparse.ArgumentParser(description="Assemble the launcher's package tree.")
    parser.add_argument("--launcher", type=Path, required=True)
    parser.add_argument("--out", type=Path, default=REPO / "package")
    parser.add_argument("--components", type=Path, default=REPO / "components.json")
    parser.add_argument("--from-dir", type=Path)
    arguments = parser.parse_args()

    if not arguments.launcher.is_file():
        print(f"error: no launcher at {arguments.launcher}", file=sys.stderr)
        return 1

    components = json.loads(arguments.components.read_text(encoding="utf-8"))["components"]
    out = arguments.out

    if out.exists():
        shutil.rmtree(out)

    out.mkdir(parents=True)

    versions = {}
    lines = []
    failed = False

    with tempfile.TemporaryDirectory() as scratch:
        for component in components:
            name = component["name"]
            archive, version, reason = find_asset(component, arguments.from_dir, Path(scratch))

            if archive is None:
                if component.get("optional", False):
                    lines.append(f"- {name}: skipped, {reason}")
                else:
                    lines.append(f"- {name}: MISSING, {reason}")
                    failed = True

                continue

            # An archive that cannot be trusted or read fails the run, optional or not.
            try:
                extract(archive, out / component["dest"])
            except (ValueError, zipfile.BadZipFile) as problem:
                lines.append(f"- {name}: REFUSED, {problem}")
                failed = True

                continue

            versions[name] = version
            lines.append(f"- {name}: {version} from {archive.name}")

    shutil.copy2(arguments.launcher, out / arguments.launcher.name)
    shutil.copy2(REPO / "catalog.json", out / "catalog.json")
    (out / "package.json").write_text(
        json.dumps({"components": versions}, indent=4) + "\n", encoding="utf-8"
    )

    summary = "\n".join(["Package components:", *lines])

    print(summary)

    if os.environ.get("GITHUB_STEP_SUMMARY"):
        with open(os.environ["GITHUB_STEP_SUMMARY"], "a", encoding="utf-8") as report:
            report.write(summary + "\n")

    if failed:
        print("error: a component is missing or was refused", file=sys.stderr)
        return 1

    return 0


if __name__ == "__main__":
    sys.exit(main())
