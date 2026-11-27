#!/usr/bin/env python3
"""Conservative source/tool compatibility for the release Web image.

The executing workflow supplies its frozen image source, never candidate input.
Only a complete matching tool closure admits the image; historical sources keep
using the existing host lane. This does not qualify a release or authorize save.
"""
import argparse
import re
import subprocess
from pathlib import Path


TOOL_INPUTS = (
    "rust-toolchain.toml",
    "scripts/ci-browser-deps.py",
    "scripts/ci_process_runner.py",
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
    for revision in (image_source, source):
        if git("cat-file", "-e", revision + "^{commit}").returncode:
            return False
        for name in TOOL_INPUTS:
            result = git("ls-tree", revision, "--", name)
            # Symlinks, absent files and directories cannot satisfy this tool
            # closure, even if both revisions contain the same invalid entry.
            if result.returncode or not re.fullmatch(
                    rb"100(?:644|755) blob [0-9a-f]{40}\t" + name.encode() + rb"\n",
                    result.stdout):
                return False
    return git("diff", "--quiet", "--no-ext-diff", "--no-textconv",
               image_source, source, "--", *TOOL_INPUTS).returncode == 0


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo-root", type=Path, default=Path.cwd())
    parser.add_argument("--image-source", required=True)
    parser.add_argument("--source", required=True)
    parser.add_argument("--github-output", type=Path)
    args = parser.parse_args()
    value = "true" if compatible(args.repo_root, args.image_source, args.source) else "false"
    print("release_web_image_compatible=" + value)
    if args.github_output:
        with args.github_output.open("a") as output:
            output.write("release_web_image_compatible=" + value + "\n")


if __name__ == "__main__":
    main()
