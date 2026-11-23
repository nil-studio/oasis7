#!/usr/bin/env python3
"""Verify the two Hosted Login stages before producing the deployable bundle."""

import argparse
import hashlib
import json
from pathlib import Path, PurePosixPath
import re
import shutil
import subprocess
import tarfile

BINARIES = ("bin/oasis7_game_launcher_v3", "bin/oasis7_viewer_live")
NATIVE_FILES = {"BUILDINFO", "SHA256SUMS", *BINARIES}
VIEWER_MANIFEST = ".oasis7-viewer-dist-manifest.json"
VIEWER_PAYLOAD = "pixel-world-bridge/webgl2/pixel_world_bridge_bindgen_bg.wasm"
VIEWER_BROWSER_WASM = "browser-game-client/oasis7_browser_game_client_bg.wasm"
VIEWER_RENDER_FILES = (
    "pixel-world-bridge/webgl2/pixel_world_bridge.js",
    "pixel-world-bridge/webgl2/pixel_world_bridge_bindgen.js",
)
ROOT = Path(__file__).resolve().parents[1]


def require(condition, message):
    if not condition:
        raise ValueError(message)


def digest(path):
    sha256 = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            sha256.update(chunk)
    return sha256.hexdigest()


def stage_identity(stage, kind, source):
    return {"schema": 1, "kind": kind, "source": source, "sha256": digest(stage / "stage.tar.gz")}


def seal(stage, kind, source):
    receipt = stage_identity(stage, kind, source)
    (stage / "stage.json").write_text(json.dumps(receipt, sort_keys=True) + "\n")


def unpack(stage, kind, source, destination):
    require({p.name for p in stage.iterdir()} == {"stage.json", "stage.tar.gz"},
            f"{kind} stage must contain exactly one archive and one receipt")
    receipt = json.loads((stage / "stage.json").read_text())
    require(type(receipt) is dict and type(receipt.get("schema")) is int
            and receipt == stage_identity(stage, kind, source),
            f"{kind} stage schema, source or archive hash mismatch")
    members = set()
    with tarfile.open(stage / "stage.tar.gz", "r|gz") as archive:
        for member in archive:
            path = PurePosixPath(member.name)
            name = path.as_posix()
            require(not path.is_absolute() and ".." not in path.parts and name not in members,
                    f"{kind} stage contains an invalid or repeated path: {member.name}")
            regular = member.type in {tarfile.REGTYPE, tarfile.AREGTYPE}
            if kind == "native":
                allowed = (name in {".", "bin"} and member.isdir()
                           or name in NATIVE_FILES and regular)
            else:
                allowed = ((name == "web" and member.isdir())
                           or (name.startswith("web/") and (member.isdir() or regular)))
            require(allowed, f"{kind} stage contains an unexpected entry: {name}")
            members.add(name)
            output = destination / path
            if member.isdir():
                output.mkdir(parents=True, exist_ok=True)
            else:
                output.parent.mkdir(parents=True, exist_ok=True)
                with archive.extractfile(member) as contents, output.open("xb") as stream:
                    shutil.copyfileobj(contents, stream)
                output.chmod(member.mode & 0o777)
    if kind == "native":
        require(NATIVE_FILES <= members, "native stage is missing required files")


def verify_native(destination, source, profile):
    metadata = (
        f"git_sha={source}\nprofile={profile}\ntarget=x86_64-unknown-linux-gnu\n"
        f"binary={BINARIES[0]}\nviewer_live={BINARIES[1]}\nweb=web/\n"
    )
    require((destination / "BUILDINFO").read_text() == metadata, "native BUILDINFO mismatch")
    for name in BINARIES:
        path = destination / name
        with path.open("rb") as stream:
            header = stream.read(64)
        require(len(header) == 64 and header[:7] == b"\x7fELF\x02\x01\x01"
                and int.from_bytes(header[16:18], "little") in {2, 3}
                and int.from_bytes(header[18:20], "little") == 62
                and path.stat().st_mode & 0o111, f"{name} must be an executable x86-64 ELF")
    sums = "".join(f"{digest(destination / name)}  {name}\n" for name in BINARIES)
    require((destination / "SHA256SUMS").read_text() == sums, "native SHA256SUMS mismatch")


def verify_viewer(web):
    require(not (web / "optional-payloads.json").exists(), "Hosted Viewer requires unsplit delivery")
    actual_files = {
        path.relative_to(web).as_posix(): {"size": path.stat().st_size}
        for path in web.rglob("*") if path.is_file() and path != web / VIEWER_MANIFEST
    }
    manifest = json.loads((web / VIEWER_MANIFEST).read_text())
    require(type(manifest) is dict and manifest.get("distFiles") == actual_files, "Viewer manifest inventory mismatch")
    for name in ("index.html", VIEWER_PAYLOAD, VIEWER_BROWSER_WASM, *VIEWER_RENDER_FILES):
        require(actual_files.get(name, {}).get("size", 0) > 0, f"Viewer output missing or empty: {name}")
    for name in (VIEWER_PAYLOAD, VIEWER_BROWSER_WASM):
        with (web / name).open("rb") as stream:
            require(stream.read(8) == b"\x00asm\x01\x00\x00\x00", f"invalid Viewer WASM: {name}")
    subprocess.run(
        ["bash", "-ec", 'source "$1/scripts/viewer-web-dist-contract.sh"; viewer_web_dist_check_freshness "$1" "$2"',
         "check-viewer-dist", ROOT, web], check=True,
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=("seal", "assemble"))
    parser.add_argument("directory", type=Path)
    parser.add_argument("identity")
    parser.add_argument("selection")
    args = parser.parse_args()
    source = args.selection if args.mode == "seal" else args.identity
    require(re.fullmatch(r"[0-9a-f]{40}", source), "expected a full Git source SHA")
    if args.mode == "seal":
        require(args.identity in {"native", "viewer"}, "unknown producer")
        seal(args.directory, args.identity, source)
    else:
        require(args.selection in {"dev", "release"}, "unknown build profile")
        destination = args.directory / "hosted-login"
        destination.mkdir()
        for kind in ("native", "viewer"):
            unpack(args.directory / f"hosted-{kind}", kind, source, destination)
        verify_native(destination, source, args.selection)
        verify_viewer(destination / "web")


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, TypeError, tarfile.TarError, subprocess.CalledProcessError) as error:
        raise SystemExit(f"Hosted Login package rejected: {error}") from error
