#!/usr/bin/env python3
"""Resolve package source once; refs remain data, never shell source."""
import argparse
import importlib.util
from pathlib import Path
import json
import os
import re
import subprocess


def git(*args):
    return subprocess.check_output(["git", *args], text=True).strip()


def resolve(requested, release=False):
    if not requested or any(ord(c) < 32 or ord(c) == 127 for c in requested):
        raise ValueError("invalid package ref")
    if release:
        tag = requested.removeprefix("refs/tags/")
        if not re.fullmatch(r"(?:v[0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?|20[0-9]{2}-[0-9]{2}-[0-9]{2}_v[0-9]+)", tag):
            raise ValueError("invalid release tag")
        ref, kind = "refs/tags/" + tag, "tag"
    elif requested.startswith(("refs/heads/", "refs/tags/")):
        ref, kind = requested, "branch" if requested.startswith("refs/heads/") else "tag"
    elif re.fullmatch(r"[0-9a-fA-F]{40}", requested):
        ref, kind = requested, "commit"
    else:
        git("check-ref-format", "refs/heads/" + requested)
        matches = [r for r in ("refs/heads/" + requested, "refs/tags/" + requested)
                   if subprocess.run(["git", "show-ref", "--verify", "--quiet", r.replace("refs/heads/", "refs/remotes/origin/", 1)]).returncode == 0]
        if len(matches) != 1:
            raise ValueError("missing or ambiguous package ref")
        ref = matches[0]
        kind = "tag" if ref.startswith("refs/tags/") else "branch"
    if kind != "commit":
        git("check-ref-format", ref)
    lookup = ref.replace("refs/heads/", "refs/remotes/origin/", 1) if kind == "branch" else ref
    sha = git("rev-parse", "--verify", lookup + "^{commit}")
    default = git("rev-parse", "--verify", os.environ.get("TRUSTED_DEFAULT_REF", "refs/remotes/origin/main") + "^{commit}")
    if release and subprocess.run(["git", "merge-base", "--is-ancestor", sha, default]).returncode:
        raise ValueError("release commit is outside protected default history")
    return {"repository": os.environ.get("GITHUB_REPOSITORY", ""),
            "workflowSha": os.environ.get("GITHUB_WORKFLOW_SHA", ""),
            "eventSha": os.environ.get("GITHUB_SHA", ""), "requestedKind": kind,
            "requestedRef": requested, "resolvedRef": ref,
            "tagObjectSha": git("rev-parse", "--verify", ref) if kind == "tag" else None,
            "checkoutSha": sha, "trustedDefaultSha": default,
            "version": ref.removeprefix("refs/tags/").removeprefix("v") if kind == "tag" else sha[:12]}


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--release", action="store_true")
    parser.add_argument("--release-web-image-source")
    args = parser.parse_args()
    plan = resolve(os.environ["REQUESTED_REF"], args.release)
    image_compatible = None
    if args.release_web_image_source:
        spec = importlib.util.spec_from_file_location("release_web_image", Path(__file__).with_name("ci-release-web-image.py"))
        image = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(image)
        image_compatible = all(image.compatible(Path.cwd(), args.release_web_image_source, sha)
                               for sha in (plan["checkoutSha"], plan["workflowSha"]))
    with open(os.environ["GITHUB_OUTPUT"], "a") as output:
        for key in ("checkoutSha", "version", "resolvedRef"):
            output.write(key + "=" + plan[key] + "\n")
        output.write("sourcePlan=" + json.dumps(plan, separators=(",", ":")) + "\n")
        if image_compatible is not None:
            output.write("webImageCompatible=" + str(image_compatible).lower() + "\n")
    if os.environ.get("GITHUB_STEP_SUMMARY"):
        with open(os.environ["GITHUB_STEP_SUMMARY"], "a") as summary:
            summary.write("```json\n" + json.dumps(plan, indent=2) + "\n```\n")
