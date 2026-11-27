#!/usr/bin/env python3
"""Conservative source/tool compatibility for the release Web image.

The executing workflow supplies its frozen image source, never candidate input.
Only a complete matching tool closure admits the image; historical sources keep
using the existing host lane. This does not qualify a release or authorize save.
"""
import argparse
import os
import sys
import shutil
import re
import subprocess
from pathlib import Path


TOOL_INPUTS = (
    "rust-toolchain.toml",
    "docker/ci-base/Dockerfile",
    "docker/ci-base/build.sh",
    "docker/ci-base/runtime-packages.txt",
    "docker/ci-base/build-packages.txt",
    "scripts/ci-browser-deps.py",
    "scripts/ci_process_runner.py",
    "scripts/agent-browser-lib.sh",
    "scripts/viewer-web-dist-contract.sh",
    "scripts/ci-apt-install.py",
    "scripts/ci-apt-dns.py",
    "scripts/ci-browser-tools/package.json",
    "scripts/ci-browser-tools/package-lock.json",
    "scripts/ensure-wasm-bindgen-cli.sh",
    "scripts/wasm-bindgen-cli-common.sh",
)
SHA = re.compile(r"[0-9a-f]{40}")


def compatible(root, image_source, source):
    if not all(SHA.fullmatch(value or "") for value in (image_source, source)):
        return False
    def git(*args):
        return subprocess.run(["git", "-C", str(root), *args],
                              stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
                              check=False)
    compiler_inputs = []
    for revision in (image_source, source):
        if git("cat-file", "-e", revision + "^{commit}").returncode:
            return False
        for name in (*TOOL_INPUTS, ".github/workflows/rust.yml"):
            result = git("ls-tree", revision, "--", name)
            # Symlinks, absent files and directories cannot satisfy this tool
            # closure, even if both revisions contain the same invalid entry.
            if result.returncode or not re.fullmatch(
                    rb"100(?:644|755) blob [0-9a-f]{40}\t" + name.encode() + rb"\n",
                    result.stdout):
                return False
        workflow = git("show", revision + ":.github/workflows/rust.yml")
        if workflow.returncode:
            return False
        values = []
        for key in ("RUST_TOOLCHAIN", "OASIS7_WASM_TOOLCHAIN", "OASIS7_WASM_TARGET"):
            matches = re.findall(rb"(?m)^  " + key.encode() + rb": ([^\n]+)$", workflow.stdout)
            if len(matches) != 1:
                return False
            values.append(matches[0])
        compiler_inputs.append(values)
    if compiler_inputs[0] != compiler_inputs[1]:
        return False
    return git("diff", "--quiet", "--no-ext-diff", "--no-textconv",
               image_source, source, "--", *TOOL_INPUTS).returncode == 0


def preflight(root, source):
    """Run only from the executing workflow's staged helper, before builds."""
    root = root.resolve(strict=True)
    if (sys.platform != "linux" or sys.version_info[:2] != (3, 12)
            or os.getuid() != 1001 or os.getgid() != 1001
            or not SHA.fullmatch(source or "")
            or root != Path(os.environ["GITHUB_WORKSPACE"]).resolve(strict=True)):
        raise ValueError("unexpected release Web image/source context")
    release = Path("/etc/os-release").read_text()
    if not re.search(r'(?m)^ID=ubuntu$', release) or not re.search(r'(?m)^VERSION_ID="24[.]04"$', release):
        raise ValueError("release Web image requires Ubuntu24.04")
    def run(command, env=None):
        return subprocess.check_output(command, cwd=root, text=True,
                                       env=env, timeout=60).strip()
    actual = run(["git", "-c", "safe.directory=" + str(root), "rev-parse", "HEAD"])
    if actual != source:
        raise ValueError("release Web source differs from immutable plan")
    version = os.environ["RUST_TOOLCHAIN"]
    if not re.fullmatch(r"[0-9]+[.][0-9]+[.][0-9]+", version):
        raise ValueError("invalid release compiler version")
    rust_env = dict(os.environ, RUSTUP_AUTO_INSTALL="0")
    if not run(["rustc", "+" + version, "--version"], rust_env).startswith("rustc " + version + " "):
        raise ValueError("release compiler differs from image contract")
    targets = run(["rustup", "target", "list", "--installed", "--toolchain", version], rust_env)
    if "wasm32-unknown-unknown" not in targets.splitlines():
        raise ValueError("preinstalled stable WASM target is missing")
    if not run(["/usr/local/bin/node", "--version"]).startswith("v24."):
        raise ValueError("browser activation requires the inherited Node24")
    node22 = "/opt/oasis7-node22/bin"
    cli_env = dict(os.environ, PATH=node22 + ":" + os.environ["PATH"])
    if not run([node22 + "/node", "--version"]).startswith("v22."):
        raise ValueError("release CLI requires the original Node22")
    run([node22 + "/npm", "--version"], cli_env)
    for tool in ("xvfb-run", "Xvfb", "xauth", "nc", "pkg-config", "zstd"):
        if not shutil.which(tool):
            raise ValueError("release Web image is missing " + tool)
    run(["pkg-config", "--exists", "wayland-client", "wayland-protocols", "xkbcommon",
         "x11", "xi", "xcursor", "xrandr", "xinerama", "alsa", "libudev"])
    # The pinned browser helper checks Node24 before exposing its native CLI.
    browser_env = dict(os.environ, PATH="/usr/local/bin:" + os.environ["PATH"])
    run(["/usr/bin/python3", "/opt/oasis7-viewer-browser/scripts/ci-browser-deps.py",
         "activate", "--prebuilt", "--browser", "agent", "--work-dir",
         str(Path(os.environ["RUNNER_TEMP"]) / "release-web-browser"),
         "--github-env", os.environ["GITHUB_ENV"]], browser_env)
    agent = "/opt/oasis7-viewer-browser/scripts/ci-browser-tools/node_modules/@agent-browser/cli-linux-x64-gnu/bin"
    with Path(os.environ["GITHUB_PATH"]).open("a") as output:
        output.write(node22 + "\n" + agent + "\n")
    print("release_web_image_preflight=passed source=" + source)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo-root", type=Path, default=Path.cwd())
    parser.add_argument("--image-source")
    parser.add_argument("--preflight", action="store_true")
    parser.add_argument("--source", required=True)
    parser.add_argument("--github-output", type=Path)
    args = parser.parse_args()
    if args.preflight:
        preflight(args.repo_root, args.source)
        return
    value = "true" if compatible(args.repo_root, args.image_source, args.source) else "false"
    print("release_web_image_compatible=" + value)
    if args.github_output:
        with args.github_output.open("a") as output:
            output.write("release_web_image_compatible=" + value + "\n")


if __name__ == "__main__":
    main()
