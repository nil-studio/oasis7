#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"
source "$ROOT_DIR/scripts/worktree-harness-lib.sh"

usage() {
  cat <<'USAGE'
Usage: ./scripts/worktree-gc-report.sh [options]

Report read-only Git and filesystem facts about registered worktrees.

Options:
  --json           Print machine-readable JSON with all discovered worktrees
  --footprint      Include worktree target and Viewer dependency usage
  -h, --help       Show this help

Examples:
  ./scripts/worktree-gc-report.sh
  ./scripts/worktree-gc-report.sh --footprint
  ./scripts/worktree-gc-report.sh
  ./scripts/worktree-gc-report.sh --json
USAGE
}

wh_require_git_worktree

OUTPUT_JSON=0
INCLUDE_FOOTPRINT=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --json)
      OUTPUT_JSON=1
      shift
      ;;
    --footprint)
      INCLUDE_FOOTPRINT=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "error: unknown argument: $1" >&2
      usage >&2
      exit 1
      ;;
  esac
done

COMMON_GIT_DIR="$(cd "$(git rev-parse --git-common-dir)" && pwd -P)"
CANONICAL_REPO_ROOT="$(cd "$COMMON_GIT_DIR/.." && pwd -P)"
CURRENT_WORKTREE="$(pwd -P)"

python3 - "$COMMON_GIT_DIR" "$CANONICAL_REPO_ROOT" "$CURRENT_WORKTREE" "$OUTPUT_JSON" "$INCLUDE_FOOTPRINT" <<'PY'
from __future__ import annotations

import json
import os
import subprocess
import sys
from pathlib import Path


common_git_dir = Path(sys.argv[1]).resolve()
repo_root = Path(sys.argv[2]).resolve()
current_worktree = Path(sys.argv[3]).resolve()
output_json = sys.argv[4] == "1"
include_footprint = sys.argv[5] == "1"

def run(*args: str) -> str:
    return subprocess.check_output(args, text=True)


def human_size(size_bytes: int | None) -> str | None:
    if size_bytes is None:
        return None
    units = ["B", "KiB", "MiB", "GiB", "TiB"]
    value = float(size_bytes)
    for unit in units:
        if value < 1024 or unit == units[-1]:
            if unit == "B":
                return f"{int(value)} {unit}"
            return f"{value:.1f} {unit}"
        value /= 1024


def dir_size_bytes(path: Path) -> int | None:
    if not path.exists():
        return 0
    result = subprocess.run(
        ["du", "-sk", str(path)],
        check=False,
        stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL,
        text=True,
    )
    if result.returncode != 0:
        return None
    raw_size = result.stdout.split(None, 1)[0] if result.stdout.strip() else ""
    if not raw_size.isdigit():
        return None
    return int(raw_size) * 1024


def cached_dir_size_bytes(path: Path, cache: dict[str, int | None]) -> int | None:
    cache_key = str(path)
    if cache_key not in cache:
        cache[cache_key] = dir_size_bytes(path)
    return cache[cache_key]


def target_footprint(path: Path, linked_target_size_cache: dict[str, int | None]) -> dict[str, object]:
    target_path = path / "target"
    target_exists = target_path.exists()
    target_is_symlink = target_path.is_symlink()
    target_resolved_path = str(target_path.resolve(strict=False)) if target_is_symlink else None
    measured_path = Path(target_resolved_path) if target_resolved_path else target_path
    target_bytes = (
        cached_dir_size_bytes(measured_path, linked_target_size_cache)
        if target_exists and target_is_symlink
        else dir_size_bytes(measured_path)
        if target_exists
        else 0
    )
    return {
        "bytes": target_bytes,
        "is_symlink": target_is_symlink,
        "resolved_path": target_resolved_path,
    }


def parse_porcelain() -> list[dict[str, object]]:
    raw = subprocess.check_output(["git", f"--git-dir={common_git_dir}", "worktree", "list", "--porcelain", "-z"])
    records: list[dict[str, object]] = []
    current: dict[str, object] = {}
    for field in raw.split(b'\0'):
        if not field:
            if current:
                records.append(current)
                current = {}
            continue
        key, sep, value = field.partition(b' ')
        name = key.decode('ascii')
        current[name] = os.fsdecode(value) if sep else True
    if current:
        records.append(current)
    return records


def worktree_status(path: Path) -> dict[str, object]:
    empty = dict(dirty=None, dirty_entry_count=None, untracked_count=None, ignored_count=None)
    if not path.exists():
        return dict(empty, unreadable_reason="path_missing")
    result = subprocess.run(["git", "-C", str(path), "status", "--porcelain", "-z", "--untracked-files=all", "--ignored"], capture_output=True)
    if result.returncode:
        return dict(empty, unreadable_reason="git_status_failed")
    codes=[]
    skip=False
    for row in result.stdout.split(b'\0'):
        if skip:
            skip=False
            continue
        if not row: continue
        code=row[:2]
        codes.append(code)
        skip=b'R' in code or b'C' in code
    changed=[code for code in codes if code != b'!!']
    return dict(dirty=bool(changed), dirty_entry_count=len(changed), untracked_count=codes.count(b'??'), ignored_count=codes.count(b'!!'), unreadable_reason=None)


def is_ancestor(commit: str | None, ref: str) -> bool | None:
    if not commit:
        return None
    result = subprocess.run(
        ["git", f"--git-dir={common_git_dir}", "merge-base", "--is-ancestor", commit, ref],
        check=False,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        text=True,
    )
    if result.returncode == 0:
        return True
    if result.returncode == 1:
        return False
    return None


records = parse_porcelain()
entries: list[dict[str, object]] = []
dirty_count = 0
prunable_count = 0
known_target_bytes = 0
known_local_target_bytes = 0
known_linked_target_bytes = 0
known_linked_target_paths: set[str] = set()
linked_target_size_cache: dict[str, int | None] = {}
known_node_modules_bytes = 0

for record in records:
    path_value = str(record["worktree"])
    path_obj = Path(path_value)
    resolved_path = path_obj.resolve(strict=False)
    branch_ref = record.get("branch")
    branch = branch_ref.removeprefix("refs/heads/") if isinstance(branch_ref, str) else None
    head = record.get("HEAD") if isinstance(record.get("HEAD"), str) else None
    detached = bool(record.get("detached"))
    prunable_reason = record.get("prunable")
    prunable = prunable_reason is not None
    locked_reason = record.get("locked")
    exists = path_obj.exists()
    is_current = resolved_path == current_worktree
    status = worktree_status(path_obj)
    dirty = status["dirty"]
    if dirty:
        dirty_count += 1
    if prunable:
        prunable_count += 1

    target_bytes = None
    target_is_symlink = None
    target_resolved_path = None
    node_modules_bytes = None
    total_footprint_bytes = None
    if include_footprint and exists:
        target = target_footprint(path_obj, linked_target_size_cache)
        target_bytes = target["bytes"]
        target_is_symlink = target["is_symlink"]
        target_resolved_path = target["resolved_path"]
        node_modules_bytes = dir_size_bytes(path_obj / "crates" / "oasis7_viewer" / "node_modules")
        known_parts = [value for value in (target_bytes, node_modules_bytes) if value is not None]
        total_footprint_bytes = sum(known_parts) if len(known_parts) == 2 else None
        if target_bytes is not None:
            known_target_bytes += target_bytes
            if target_is_symlink and isinstance(target_resolved_path, str):
                known_linked_target_paths.add(target_resolved_path)
            else:
                known_local_target_bytes += target_bytes
        if node_modules_bytes is not None:
            known_node_modules_bytes += node_modules_bytes

    merged_to_main = is_ancestor(head, "refs/heads/main")

    entry = {
        "path": str(resolved_path),
        "branch": branch,
        "head": head,
        "detached": detached,
        "current": is_current,
        "exists": exists,
        **status,
        "prunable": prunable,
        "prunable_reason": prunable_reason,
        "locked": locked_reason is not None,
        "locked_reason": locked_reason,
        "merged_to_main": merged_to_main,
        "merged_to_origin_main": is_ancestor(head, "refs/remotes/origin/main"),
    }
    if include_footprint:
        entry["footprint"] = {
            "target_bytes": target_bytes,
            "target_human": human_size(target_bytes),
            "target_is_symlink": target_is_symlink,
            "target_resolved_path": target_resolved_path,
            "viewer_node_modules_bytes": node_modules_bytes,
            "viewer_node_modules_human": human_size(node_modules_bytes),
            "known_total_bytes": total_footprint_bytes,
            "known_total_human": human_size(total_footprint_bytes),
        }
    entries.append(entry)

payload = {
    "repo_root": str(repo_root),
    "current_worktree": str(current_worktree),
    "summary": {
        "total_worktrees": len(entries),
        "prunable_worktrees": prunable_count,
        "dirty_worktrees": dirty_count,
    },
    "entries": entries,
}
if include_footprint:
    for linked_target_path in sorted(known_linked_target_paths):
        linked_target_bytes = linked_target_size_cache.get(linked_target_path)
        if linked_target_bytes is not None:
            known_linked_target_bytes += linked_target_bytes
    payload["summary"].update(
        {
            "footprint_included": True,
            "known_target_bytes": known_target_bytes,
            "known_target_human": human_size(known_target_bytes),
            "known_local_target_bytes": known_local_target_bytes,
            "known_local_target_human": human_size(known_local_target_bytes),
            "known_linked_target_bytes": known_linked_target_bytes,
            "known_linked_target_human": human_size(known_linked_target_bytes),
            "known_deduplicated_target_bytes": known_local_target_bytes + known_linked_target_bytes,
            "known_deduplicated_target_human": human_size(known_local_target_bytes + known_linked_target_bytes),
            "known_viewer_node_modules_bytes": known_node_modules_bytes,
            "known_viewer_node_modules_human": human_size(known_node_modules_bytes),
            "known_footprint_bytes": known_target_bytes + known_node_modules_bytes,
            "known_footprint_human": human_size(known_target_bytes + known_node_modules_bytes),
        }
    )

if output_json:
    print(json.dumps(payload, ensure_ascii=True, indent=2))
    raise SystemExit(0)

print("worktree lifecycle report")
print(f"- repo_root: {repo_root}")
print(f"- current_worktree: {current_worktree}")
print(f"- total_worktrees: {len(entries)}")
print(f"- prunable_worktrees: {prunable_count}")
print(f"- dirty_worktrees: {dirty_count}")
if include_footprint:
    print(f"- known_target_size: {human_size(known_target_bytes)}")
    print(f"- known_deduplicated_target_size: {human_size(known_local_target_bytes + known_linked_target_bytes)}")
    print(f"- known_linked_target_size: {human_size(known_linked_target_bytes)}")
    print(f"- known_viewer_node_modules_size: {human_size(known_node_modules_bytes)}")
    print(f"- known_worktree_cache_size: {human_size(known_target_bytes + known_node_modules_bytes)}")

shown = entries
if not shown:
    print("- details: none")
    raise SystemExit(0)

print("- details:")
for entry in shown:
    label_parts = []
    if entry["branch"]:
        label_parts.append(str(entry["branch"]))
    elif entry["detached"]:
        label_parts.append("detached")
    else:
        label_parts.append("unknown-branch")
    if entry["current"]:
        label_parts.append("current")
    if entry["prunable"]:
        label_parts.append("prunable")
    if entry["dirty"] is True:
        label_parts.append(f"dirty={entry['dirty_entry_count']}")
    elif entry["dirty"] is False:
        label_parts.append("clean")
    else:
        label_parts.append("dirty=unknown")
    print(f"  - {' | '.join(label_parts)}")
    print(f"    path: {entry['path']}")
    print(f"    head: {entry['head']}")
    print(f"    untracked: {entry['untracked_count']}, ignored: {entry['ignored_count']}")
    print(f"    main_reachable: {entry['merged_to_main']}, origin_main_reachable: {entry['merged_to_origin_main']}")
    if entry["unreadable_reason"]:
        print(f"    unreadable: {entry['unreadable_reason']}")
    footprint = entry.get("footprint")
    if include_footprint and isinstance(footprint, dict):
        print(
            "    footprint: "
            f"target={footprint['target_human']}, "
            f"viewer_node_modules={footprint['viewer_node_modules_human']}, "
            f"known_total={footprint['known_total_human']}"
        )
PY
