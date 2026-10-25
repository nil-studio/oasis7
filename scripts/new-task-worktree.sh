#!/usr/bin/env bash
# Cross-platform maintenance: preserve Windows Git Bash/PowerShell and Linux/macOS bootstrap behavior.
set -euo pipefail

CALLER_DIR="$(pwd -P)"
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"
source "$ROOT_DIR/scripts/worktree-harness-lib.sh"

if ! PYTHON_BIN="$("$ROOT_DIR/scripts/find-python-with-module.sh" ast)"; then
  echo "error: cannot find a functional Python interpreter required for task worktree bootstrap" >&2
  exit 1
fi

usage() {
  cat <<'USAGE'
Usage: ./scripts/new-task-worktree.sh <module> <task> [options]

Create or attach a standardized git worktree for one task slice.

Default conventions:
- branch: codex/<module>-<task>
- worktrees root: oasis7.task-worktrees-root Git config; fallback <repo-parent>/worktrees
- worktree path: <worktrees root>/<repo-name>-<module>-<task>
- base ref: HEAD

Options:
  --base <ref>            Base ref for a new branch (default: HEAD)
  --branch <name>         Override branch name
  --path <path>           Override target worktree path
  --worktrees-root <dir>  Override default worktrees root
  --allow-dirty-source    Allow creating from a dirty source worktree
  --init-docs             Inspect module PRD in the new worktree
  --with-harness          Asynchronously prewarm ./scripts/worktree-harness.sh up in the new worktree
  --resume-setup          Complete missing setup in an existing matching worktree
  --json                  Print machine-readable JSON summary only
  -h, --help              Show this help

Examples:
  ./scripts/new-task-worktree.sh scripts task-worktree-bootstrap
  ./scripts/new-task-worktree.sh scripts task-worktree-bootstrap --init-docs
  ./scripts/new-task-worktree.sh viewer hud-redesign --base main
  ./scripts/new-task-worktree.sh p2p hosted-flow --json --path ../worktrees/oasis7-codex-p2p-hosted-flow
  ./scripts/new-task-worktree.sh viewer hud-redesign --with-harness
USAGE
}

wh_require_git_worktree

ALLOW_DIRTY_SOURCE=0
INIT_DOCS=0
WITH_HARNESS=0
OUTPUT_JSON=0
RESUME_SETUP=0
BASE_REF="HEAD"
BRANCH_NAME=""
TARGET_PATH=""
WORKTREES_ROOT=""
POSITIONAL=()

while [[ $# -gt 0 ]]; do
  case "$1" in
    --base|--branch|--path|--worktrees-root)
      if [[ $# -lt 2 || "$2" == --* ]]; then echo "error: missing option value: $1" >&2; exit 2; fi
      case "$1" in
        --base) BASE_REF="$2" ;;
        --branch) BRANCH_NAME="$2" ;;
        --path) TARGET_PATH="$2" ;;
        --worktrees-root) WORKTREES_ROOT="$2" ;;
      esac
      shift 2
      ;;
    --allow-dirty-source)
      ALLOW_DIRTY_SOURCE=1
      shift
      ;;
    --init-docs)
      INIT_DOCS=1
      shift
      ;;
    --with-harness)
      WITH_HARNESS=1
      shift
      ;;
    --resume-setup)
      RESUME_SETUP=1
      shift
      ;;
    --json)
      OUTPUT_JSON=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    --*)
      echo "error: unknown option: $1" >&2
      exit 2
      ;;
    *)
      POSITIONAL+=("$1")
      shift
      ;;
  esac
done

if [[ "${#POSITIONAL[@]}" -ne 2 ]]; then
  echo "error: expected <module> and <task>" >&2
  usage >&2
  exit 2
fi

MODULE_INPUT="${POSITIONAL[0]}"
TASK_INPUT="${POSITIONAL[1]}"
[[ -n "$MODULE_INPUT" && -n "$TASK_INPUT" ]] || { echo "error: <module> and <task> cannot be empty" >&2; exit 2; }
[[ -n "$BASE_REF" ]] || { echo "error: --base cannot be empty" >&2; exit 2; }
slugify() {
  "$PYTHON_BIN" - "$1" <<'PY'
from __future__ import annotations

import re
import sys

value = sys.argv[1].strip().lower()
value = re.sub(r"[^a-z0-9]+", "-", value)
value = re.sub(r"-{2,}", "-", value).strip("-")
print(value)
PY
}

resolve_abs_path() {
  "$PYTHON_BIN" - "$CALLER_DIR" "$1" <<'PY'
from __future__ import annotations

from pathlib import Path
import sys

base = Path(sys.argv[1]).resolve()
raw = Path(sys.argv[2])
if raw.is_absolute():
    print(raw.resolve())
else:
    print((base / raw).resolve())
PY
}

worktree_id_for_path() {
  "$PYTHON_BIN" - "$1" <<'PY'
from __future__ import annotations

import hashlib
from pathlib import Path
import sys

path = Path(sys.argv[1]).resolve()
digest = hashlib.sha256(str(path).encode("utf-8")).hexdigest()[:8]
print(f"wt-{digest}")
PY
}

branch_checkout_path() {
  "$PYTHON_BIN" - "$COMMON_GIT_DIR" "$1" <<'PY'
from __future__ import annotations

import subprocess
import sys

git_dir = sys.argv[1]
target = f"refs/heads/{sys.argv[2]}"
current: dict[str, str] = {}
raw = subprocess.check_output(
    ["git", f"--git-dir={git_dir}", "worktree", "list", "--porcelain"],
    text=True,
)

def emit(record: dict[str, str]) -> None:
    if record.get("branch") == target:
        print(record.get("worktree", ""))
        raise SystemExit(0)

for line in raw.splitlines():
    if not line:
      if current:
        emit(current)
        current = {}
      continue
    key, _, value = line.partition(" ")
    current[key] = value

if current:
    emit(current)

raise SystemExit(1)
PY
}

MODULE_SLUG="$(slugify "$MODULE_INPUT")"
TASK_SLUG="$(slugify "$TASK_INPUT")"
[[ -n "$MODULE_SLUG" ]] || { echo "error: <module> becomes empty after slug normalization" >&2; exit 2; }
[[ -n "$TASK_SLUG" ]] || { echo "error: <task> becomes empty after slug normalization" >&2; exit 2; }
REPO_ROOT="$(wh_repo_root)"
COMMON_GIT_DIR="$(cd "$(git rev-parse --git-common-dir)" && pwd -P)"
CANONICAL_REPO_ROOT="$(cd "$COMMON_GIT_DIR/.." && pwd -P)"
CURRENT_REPO_PARENT_NAME="$(basename "$(dirname "$REPO_ROOT")")"
FAMILY_REPO_NAME="$(git config --local --get oasis7.task-worktree-family-name 2>/dev/null || true)"
FAMILY_WORKTREES_ROOT="$(git config --local --get oasis7.task-worktrees-root 2>/dev/null || true)"
if [[ -z "$FAMILY_REPO_NAME" ]]; then
  if [[ "$CURRENT_REPO_PARENT_NAME" == "worktrees" ]]; then
    FAMILY_REPO_NAME="$(basename "$CANONICAL_REPO_ROOT")"
  else
    FAMILY_REPO_NAME="$(basename "$REPO_ROOT")"
  fi
fi
if [[ -z "$FAMILY_WORKTREES_ROOT" ]]; then
  if [[ "$CURRENT_REPO_PARENT_NAME" == "worktrees" ]]; then
    FAMILY_WORKTREES_ROOT="$(dirname "$CANONICAL_REPO_ROOT")/worktrees"
  else
    FAMILY_WORKTREES_ROOT="$(dirname "$REPO_ROOT")/worktrees"
  fi
fi
if [[ -n "$WORKTREES_ROOT" ]]; then
  WORKTREES_ROOT="$(resolve_abs_path "$WORKTREES_ROOT")"
else
  WORKTREES_ROOT="$(resolve_abs_path "$FAMILY_WORKTREES_ROOT")"
fi

if [[ -z "$BRANCH_NAME" ]]; then
  BRANCH_NAME="codex/${MODULE_SLUG}-${TASK_SLUG}"
fi

if [[ -n "$TARGET_PATH" ]]; then
  TARGET_PATH="$(resolve_abs_path "$TARGET_PATH")"
else
  TARGET_PATH="$(resolve_abs_path "$WORKTREES_ROOT/$FAMILY_REPO_NAME-$MODULE_SLUG-$TASK_SLUG")"
fi


MODE="create_new_branch"
WORKTREE_CREATED=0
SETUP_STATUS="failed"
FAILED_STEP="identity"
CANONICAL_CONFIG_SOURCE="$CANONICAL_REPO_ROOT/config.toml"
TARGET_CONFIG_PATH="$TARGET_PATH/config.toml"
CANONICAL_CONFIG_EXISTS=0
CANONICAL_CONFIG_COPIED=0
RECOVERY_COMMAND="$(printf '%q ' "$ROOT_DIR/scripts/new-task-worktree.sh" "$MODULE_INPUT" "$TASK_INPUT" --branch "$BRANCH_NAME" --path "$TARGET_PATH" --resume-setup --json)"

report_failure() {
  local status=$?
  [[ "$status" != 0 ]] || return 0
  trap - EXIT
  local actual_head=""
  actual_head="$(git -C "$TARGET_PATH" rev-parse --verify HEAD 2>/dev/null || true)"
  if [[ "$(branch_checkout_path "$BRANCH_NAME" 2>/dev/null || true)" == "$TARGET_PATH" ]]; then WORKTREE_CREATED=1; fi
  echo "error: $FAILED_STEP failed; existing resources retained. Recovery: $RECOVERY_COMMAND" >&2
  if [[ "$OUTPUT_JSON" == 1 ]]; then
    "$PYTHON_BIN" - "$TARGET_PATH" "$BRANCH_NAME" "$actual_head" "$WORKTREE_CREATED" "$FAILED_STEP" "$RECOVERY_COMMAND" <<'RESULT'
import json,sys
print(json.dumps(dict(worktree=sys.argv[1],worktree_path=sys.argv[1],branch=sys.argv[2],head=sys.argv[3] or None,worktree_created=sys.argv[4]=='1',setup_status='failed',failed_step=sys.argv[5],recovery_command=sys.argv[6])))
RESULT
  fi
  [[ "$status" == 2 ]] || status=1
  exit "$status"
}
trap report_failure EXIT

if ! git check-ref-format --branch "$BRANCH_NAME" >/dev/null 2>&1; then exit 2; fi
if [[ "$RESUME_SETUP" == 1 ]]; then
  MODE="resume_setup"
  if [[ "$WITH_HARNESS" == 1 ]]; then
    echo 'error: --resume-setup cannot use --with-harness; run worktree-harness.sh up separately' >&2
    exit 2
  fi
  existing_branch_path="$(branch_checkout_path "$BRANCH_NAME" 2>/dev/null || true)"
  target_common="$(git -C "$TARGET_PATH" rev-parse --git-common-dir 2>/dev/null || true)"
  if [[ "$existing_branch_path" != "$TARGET_PATH" || ! -f "$TARGET_PATH/.git" || -z "$target_common" ]]; then exit 2; fi
  target_common="$(cd "$TARGET_PATH" && cd "$target_common" && pwd -P)"
  [[ "$target_common" == "$COMMON_GIT_DIR" ]] || exit 2
  WORKTREE_CREATED=1
else
  if [[ "$ALLOW_DIRTY_SOURCE" != 1 && -n "$(git status --short)" ]]; then
    echo 'error: source worktree is dirty; use --allow-dirty-source' >&2; exit 1
  fi
  git rev-parse --verify --quiet "$BASE_REF^{commit}" >/dev/null || exit 2
  [[ ! -e "$TARGET_PATH" && ! -L "$TARGET_PATH" ]] || exit 2
  if branch_checkout_path "$BRANCH_NAME" >/dev/null 2>&1; then exit 2; fi
  FAILED_STEP="git_add"
  mkdir -p "$(dirname "$TARGET_PATH")"
  if git show-ref --verify --quiet "refs/heads/$BRANCH_NAME"; then
    MODE="attach_existing_branch"
    git worktree add --quiet "$TARGET_PATH" "$BRANCH_NAME" >&2
  else
    git worktree add --quiet -b "$BRANCH_NAME" "$TARGET_PATH" "$BASE_REF" >&2
  fi
  WORKTREE_CREATED=1
fi
FAILED_STEP="family_config"
if ! git -C "$TARGET_PATH" config --local --get oasis7.task-worktree-family-name >/dev/null; then
  git -C "$TARGET_PATH" config --local oasis7.task-worktree-family-name "$FAMILY_REPO_NAME"
fi
if ! git -C "$TARGET_PATH" config --local --get oasis7.task-worktrees-root >/dev/null; then
  git -C "$TARGET_PATH" config --local oasis7.task-worktrees-root "$WORKTREES_ROOT"
fi
FAILED_STEP="config"
[[ ! -f "$CANONICAL_CONFIG_SOURCE" ]] || CANONICAL_CONFIG_EXISTS=1
if [[ "$CANONICAL_CONFIG_EXISTS" == 1 && ! -e "$TARGET_CONFIG_PATH" && ! -L "$TARGET_CONFIG_PATH" ]]; then
  # Link-publish a private copy: a concurrent config always wins without overwrite.
  CANONICAL_CONFIG_COPIED="$("$PYTHON_BIN" - "$CANONICAL_CONFIG_SOURCE" "$TARGET_CONFIG_PATH" <<'CONFIG'
import os,shutil,sys,tempfile
source,target=sys.argv[1:]
fd,temp=tempfile.mkstemp(prefix='.bootstrap-config-',dir=os.path.dirname(target))
os.close(fd)
try:
    shutil.copyfile(source,temp)
    try:
        os.link(temp,target)
        print("1")
    except FileExistsError: print("0")
finally: os.unlink(temp)
CONFIG
)"
fi
FAILED_STEP="optional_setup"

DOC_PRD_PATH=""
DOC_PRD_EXISTS=0
if [[ "$INIT_DOCS" == "1" ]]; then
  DOC_PRD_PATH="$TARGET_PATH/doc/$MODULE_SLUG/prd.md"
  [[ -f "$DOC_PRD_PATH" ]] && DOC_PRD_EXISTS=1
fi

HARNESS_STATE_FILE=""
HARNESS_BOOTSTRAP_LOG=""
HARNESS_STATUS=""
HARNESS_VIEWER_URL=""
if [[ "$WITH_HARNESS" == "1" ]]; then
  HARNESS_WORKTREE_ID="$(worktree_id_for_path "$TARGET_PATH")"
  HARNESS_STATE_FILE="$TARGET_PATH/output/harness/$HARNESS_WORKTREE_ID/state.json"
  HARNESS_BOOTSTRAP_LOG="$TARGET_PATH/output/harness/new-task-worktree-harness.log"
  mkdir -p "$(dirname "$HARNESS_BOOTSTRAP_LOG")"
  (
    cd "$TARGET_PATH"
    nohup ./scripts/worktree-harness.sh up >"$HARNESS_BOOTSTRAP_LOG" 2>&1 < /dev/null &
  )
  for _ in $(seq 1 5); do
    [[ -f "$HARNESS_STATE_FILE" ]] && break
    sleep 1
  done
  HARNESS_STATUS="$(wh_state_get "$HARNESS_STATE_FILE" status 2>/dev/null || true)"
  HARNESS_VIEWER_URL="$(wh_state_get "$HARNESS_STATE_FILE" viewer_url 2>/dev/null || true)"
  [[ -n "$HARNESS_STATUS" ]] || HARNESS_STATUS="booting"
fi

SETUP_STATUS="complete"
FAILED_STEP=""
HEAD_OID="$(git -C "$TARGET_PATH" rev-parse HEAD)"
SUMMARY_JSON="$("$PYTHON_BIN" - "$MODULE_INPUT" "$TASK_INPUT" "$MODULE_SLUG" "$TASK_SLUG" "$BRANCH_NAME" "$TARGET_PATH" "$BASE_REF" "$MODE" "$REPO_ROOT" "$FAMILY_REPO_NAME" "$WORKTREES_ROOT" "$CANONICAL_CONFIG_SOURCE" "$CANONICAL_CONFIG_EXISTS" "$TARGET_CONFIG_PATH" "$CANONICAL_CONFIG_COPIED" "$INIT_DOCS" "$DOC_PRD_PATH" "$DOC_PRD_EXISTS" "$WITH_HARNESS" "$HARNESS_BOOTSTRAP_LOG" "$HARNESS_STATE_FILE" "$HARNESS_STATUS" "$HARNESS_VIEWER_URL" "$HEAD_OID" "$RECOVERY_COMMAND" <<'PY'
from __future__ import annotations

import json
import sys

payload = {
    "worktree": sys.argv[6],
    "head": sys.argv[24],
    "worktree_created": True,
    "setup_status": "complete",
    "failed_step": None,
    "recovery_command": sys.argv[25],
    "module": sys.argv[1],
    "task": sys.argv[2],
    "module_slug": sys.argv[3],
    "task_slug": sys.argv[4],
    "branch": sys.argv[5],
    "worktree_path": sys.argv[6],
    "base_ref": sys.argv[7],
    "mode": sys.argv[8],
    "repo_root": sys.argv[9],
    "repo_name": sys.argv[10],
    "worktrees_root": sys.argv[11],
    "config": {
        "source_path": sys.argv[12],
        "source_exists": sys.argv[13] == "1",
        "target_path": sys.argv[14],
        "copied": sys.argv[15] == "1",
    },
}
if sys.argv[16] == "1":
    payload["doc_checks"] = {
        "prd": {"path": sys.argv[17], "exists": sys.argv[18] == "1"},
    }
if sys.argv[19] == "1":
    payload["harness"] = {
        "bootstrap_log": sys.argv[20],
        "state_file": sys.argv[21],
        "status": sys.argv[22],
        "viewer_url": sys.argv[23],
    }
print(json.dumps(payload, ensure_ascii=False))
PY
)"

if [[ "$OUTPUT_JSON" == "1" ]]; then
  printf '%s\n' "$SUMMARY_JSON"
  exit 0
fi

cat <<INFO
Task worktree is ready.
- module: $MODULE_INPUT
- task: $TASK_INPUT
- branch: $BRANCH_NAME
- path: $TARGET_PATH
- repo family: $FAMILY_REPO_NAME
- worktrees root: $WORKTREES_ROOT
- base ref: $BASE_REF
- mode: $MODE

INFO

cat <<INFO

Repo-local config:
- canonical source: $CANONICAL_CONFIG_SOURCE
- source file: $([[ "$CANONICAL_CONFIG_EXISTS" == "1" ]] && printf 'present' || printf 'missing')
- target path: $TARGET_CONFIG_PATH
- copied: $([[ "$CANONICAL_CONFIG_COPIED" == "1" ]] && printf 'yes' || printf 'no')
INFO

if [[ "$INIT_DOCS" == "1" ]]; then
  cat <<INFO

Docs bootstrap:
- module PRD: $([[ "$DOC_PRD_EXISTS" == "1" ]] && printf 'present' || printf 'missing') ($DOC_PRD_PATH)
INFO
fi

if [[ "$WITH_HARNESS" == "1" ]]; then
  cat <<INFO

Harness bootstrap:
- status: ${HARNESS_STATUS:-unknown}
- bootstrap log: $HARNESS_BOOTSTRAP_LOG
- state file: $HARNESS_STATE_FILE
- viewer url: ${HARNESS_VIEWER_URL:-unavailable}
INFO
fi

cat <<INFO

Next:
  cd $TARGET_PATH
INFO

if [[ "$INIT_DOCS" == "1" ]]; then
  if [[ "$DOC_PRD_EXISTS" == "1" ]]; then
    printf '  sed -n '\''1,160p'\'' %s\n' "${DOC_PRD_PATH#$TARGET_PATH/}"
  else
    printf '  mkdir -p %s\n' "doc/$MODULE_SLUG"
    printf '  # create %s\n' "${DOC_PRD_PATH#$TARGET_PATH/}"
  fi
else
  printf '  sed -n '\''1,160p'\'' %s\n' "doc/$MODULE_SLUG/prd.md"
fi
if [[ "$WITH_HARNESS" == "1" ]]; then
  printf '  ./scripts/worktree-harness.sh status --json\n'
fi
cat <<INFO
  git status -sb
INFO
