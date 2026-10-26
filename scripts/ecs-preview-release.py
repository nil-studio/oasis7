#!/usr/bin/env python3
"""Publish a qualified ECS archive, or select a retained release with a new intent."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
from ecs_preview_release import OssStore, publish, validate_manifest, sha256_file


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--endpoint", required=True)
    p.add_argument("--bucket", required=True)
    p.add_argument("--prefix", required=True)
    p.add_argument("--archive", type=Path)
    p.add_argument("--retained-release", default="")
    p.add_argument("--rollback", action="store_true")
    p.add_argument("--program-contract", default="")
    p.add_argument("--persistence-contract", default="")
    p.add_argument("--identity-contract", default="")
    p.add_argument("--components", default="player")
    args = p.parse_args()
    if os.environ.get("GITHUB_EVENT_NAME") != "workflow_dispatch" or os.environ.get("GITHUB_REF") != "refs/heads/main" or os.environ.get("GITHUB_WORKFLOW_SHA") != os.environ.get("GITHUB_SHA") or os.environ.get("GITHUB_REPOSITORY") != "nil-studio/oasis7":
        p.error("publication requires the trusted main workflow_dispatch entry")
    if bool(args.archive) == bool(args.retained_release) or args.rollback and not args.retained_release:
        p.error("select exactly one archive or retained release; rollback requires retained release")
    store = OssStore(args.endpoint, args.bucket, args.prefix)
    store.assert_immutable_bucket()
    if args.retained_release:
        from ecs_preview_release import RELEASE
        if not RELEASE.fullmatch(args.retained_release):
            p.error("invalid retained release identity")
        manifest = validate_manifest(json.loads(store.get(f"{args.prefix}/releases/{args.retained_release}/manifest.json")), args.prefix)
        archive = None
    else:
        archive = args.archive
        if archive.stat().st_size > 5 * 1024 ** 3:
            p.error("archive exceeds OSS single PutObject 5 GiB limit")
        source = os.environ["GITHUB_SHA"]
        manifest = dict(schema_version=1, release_id=f"{source}/{os.environ['GITHUB_RUN_ID']}-{os.environ['GITHUB_RUN_ATTEMPT']}", source_sha=source, archive_key=f"{args.prefix}/releases/{source}/{os.environ['GITHUB_RUN_ID']}-{os.environ['GITHUB_RUN_ATTEMPT']}/bundle.tar.gz", archive_sha256=sha256_file(archive), platform="linux-x64", components=args.components.split(","), compatibility=dict(program_contract=args.program_contract, persistence_contract=args.persistence_contract, identity_contract=args.identity_contract))
        # Compare the exact sidecar emitted by the already-qualified package job.
        if args.archive.with_suffix(args.archive.suffix + ".sha256").read_text().split()[0] != manifest["archive_sha256"]:
            p.error("qualified archive sidecar mismatch")
    def ancestor(old, new):
        return subprocess.run(["git", "merge-base", "--is-ancestor", old, new], check=False).returncode == 0
    result = publish(store, manifest, archive, os.environ["GITHUB_RUN_ID"], int(os.environ["GITHUB_RUN_NUMBER"]), ancestor, args.rollback)
    print(json.dumps(result, sort_keys=True))


if __name__ == "__main__":
    main()
