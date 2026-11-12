#!/usr/bin/env bash
# Cross-platform maintenance: preserve Windows Git Bash/PowerShell and Linux/macOS governance-check behavior.
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

if ! PYTHON_BIN="$("$repo_root/scripts/find-python-with-module.sh" markdown_it)"; then
  echo "doc-governance-check: markdown-it-py is required; install scripts/doc-governance-requirements.txt" >&2
  exit 1
fi

usage() {
  cat <<'USAGE'
Usage: ./scripts/doc-governance-check.sh [--full-corpus]

Checks:
  1. Actual top-level documentation directories have a README and an exact
     Markdown navigation link from doc/README.md; no duplicate registry.
  2. Selected documentation and reverse references resolve existing paths.
  3. Product identities, declared requirement/design/validation relations and
     positive completion claims retain their integrity checks.
  4. Reintroduced project.md / *.project.md ledgers remain errors.
  - Writing conventions, prose terminology and local-path examples are advisory.
  - Selected markdown files over 1000 lines receive an advisory only.
  - Unrelated implementation changes skip documentation content checks.
  --full-corpus additionally reports historical product and professional design
  findings as warnings; invalid version inputs and execution errors still fail.
USAGE
}

full_corpus=0
if [[ "${1:-}" == "--full-corpus" ]]; then
  full_corpus=1
  shift
fi

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi

if [[ $# -ne 0 ]]; then
  usage
  exit 1
fi

failures=0

fail() {
  echo "doc-governance-check: FAIL: $*"
  failures=$((failures + 1))
}

resolve_doc_range() {
  base_oid="${OASIS7_PRODUCT_DOC_BASE:-}"
  head_oid="${OASIS7_PRODUCT_DOC_HEAD:-}"
  if [[ -n "$base_oid" || -n "$head_oid" ]]; then
    if [[ -z "$base_oid" || -z "$head_oid" ]]; then
      echo "doc-governance-check: explicit base/head must be supplied together" >&2
      return 1
    fi
  else
    if [[ -n "${GITHUB_EVENT_NAME:-}" || -n "${GITHUB_EVENT_PATH:-}" || "${GITHUB_ACTIONS:-}" == true ]]; then
      echo "doc-governance-check: CI event requires explicit base/head; local fallback is forbidden" >&2
      return 1
    fi
    head_oid="$(git rev-parse --verify HEAD^{commit})"
    base_oid="$(git merge-base HEAD origin/main 2>/dev/null || git merge-base HEAD main 2>/dev/null || git rev-parse --verify HEAD^ 2>/dev/null)" || return 1
  fi
}

run_product_doc_content_check() {
  "$PYTHON_BIN" scripts/product-doc-governance-check.py --enforce-changed \
    --repo-root "$repo_root" --base "$base_oid" --head "$head_oid" --worktree || return $?
  "$PYTHON_BIN" scripts/product-doc-content-check.py \
    --repo-root "$repo_root" --base "$base_oid" --head "$head_oid" --worktree || return $?
  "$PYTHON_BIN" scripts/doc-core-content-check.py \
    --repo-root "$repo_root" --base "$base_oid" --head "$head_oid" --worktree || return $?
}

run_system_design_traceability_check() {
  "$PYTHON_BIN" scripts/system-design-traceability-check.py \
    --repo-root "$repo_root" --base "$base_oid" --head "$head_oid" --worktree
}

regex_match_with_line_numbers() {
  local regex="$1"
  shift
  local status=1 matched=0 file
  local -a batch=()
  if command -v rg >/dev/null 2>&1; then
    if [[ "$#" -eq 0 ]]; then
      rg -n -e "$regex"
      return $?
    fi
    for file in "$@"; do
      batch+=("$file")
      if [[ "${#batch[@]}" -lt 64 ]]; then
        continue
      fi
      if rg -n -e "$regex" "${batch[@]}"; then
        matched=1
      else
        status=$?
        [[ "$status" -eq 1 ]] || return "$status"
      fi
      batch=()
    done
    if [[ "${#batch[@]}" -gt 0 ]]; then
      if rg -n -e "$regex" "${batch[@]}"; then
        matched=1
      else
        status=$?
        [[ "$status" -eq 1 ]] || return "$status"
      fi
    fi
    [[ "$matched" -eq 1 ]]
    return
  fi
  local scan_error=0
  for file in "$@"; do
    if grep -nE -- "$regex" "$file"; then
      status=0
    elif [[ "$?" -gt 1 ]]; then
      scan_error=2
    fi
  done
  [[ "$scan_error" -eq 0 ]] || return "$scan_error"
  return "$status"
}

check_directory_navigation() {
  "$PYTHON_BIN" - "$head_oid" <<'PY_NAV'
from pathlib import Path
import posixpath
import runpy
import sys
from urllib.parse import unquote, urlsplit

sys.path.insert(0, "scripts")
from product_doc_markdown import parse_markdown_links

root = Path.cwd()
entry = root / "doc/README.md"
if entry.is_symlink() or not entry.is_file():
    raise SystemExit("doc-navigation: doc/README.md must be a real entry file")
scope = runpy.run_path("scripts/doc-governance-scope.py")
head = sys.argv[1]
staged = {path: status for status, path in scope["changes"](root, "--cached", head)}
unstaged = {path: status for status, path in scope["changes"](root)}
read = scope["snapshot_reader"](root, head, True, staged, unstaged, scope["tree_files"](root, head))
text = read("doc/README.md")
if text is None:
    raise SystemExit("doc-navigation: doc/README.md is absent from the selected input")
targets = set()
for link in parse_markdown_links(text):
    parsed = urlsplit(link.target)
    if parsed.scheme or parsed.netloc or not parsed.path:
        continue
    target = unquote(parsed.path)
    if target.startswith("/"):
        continue
    targets.add(posixpath.normpath("doc/" + target))
errors = []
for directory in sorted((root / "doc").iterdir()):
    if directory.name.startswith("."):
        continue
    rel = directory.relative_to(root).as_posix()
    if directory.is_symlink():
        errors.append(f"{rel} must be a real directory, not a symlink")
        continue
    if not directory.is_dir():
        continue
    landing = directory / "README.md"
    expected = landing.relative_to(root).as_posix()
    if landing.is_symlink() or not landing.is_file():
        errors.append(f"{rel} has no real README.md entry")
    elif expected not in targets:
        errors.append(f"{expected} is absent from doc/README.md Markdown navigation")
if errors:
    raise SystemExit("\n".join(errors))
PY_NAV
}

resolve_doc_range || exit 1
scope_file=$(mktemp)
trap 'rm -f "$scope_file"' EXIT
scope_args=()
if ((full_corpus == 1)); then scope_args=(--full-corpus); fi
if scope=$("$PYTHON_BIN" scripts/doc-governance-scope.py --repo-root "$repo_root" \
  --base "$base_oid" --head "$head_oid" --worktree "${scope_args[@]}" --output "$scope_file"); then
  :
else
  scope_status=$?
  if [[ "$scope_status" -ne 1 ]]; then
    echo "doc-governance-check: document input selection failed" >&2
    exit 1
  fi
  fail "document link integrity failed"
fi
if [[ "$scope" == skip ]]; then
  echo "doc-governance-check: OK (no documentation or affected reference changes)"
  exit 0
fi
all_doc_files=()
while IFS= read -r -d '' file; do
  all_doc_files+=("$file")
done < "$scope_file"

# 0) project-ledger retirement is permanent
project_ledger_paths=()
scan_roots=()
for directory in doc site tools skills .agents; do
  if [[ -d "$directory" ]]; then scan_roots+=("$directory"); fi
done
if ledger_paths=$(find "${scan_roots[@]}" -type f \
    \( -name 'project.md' -o -name '*.project.md' \) \
    ! -path '*/third_party/*' ! -path '*/target/*' ! -path '*/node_modules/*' \
    | sort); then
  while IFS= read -r file; do
    [[ -z "$file" ]] || project_ledger_paths+=("$file")
  done <<< "$ledger_paths"
else
  fail "project ledger path scan failed"
fi
if [[ ${#project_ledger_paths[@]} -gt 0 ]]; then
  printf 'doc-governance-check: retired project ledger paths:\n%s\n' "${project_ledger_paths[*]}"
  fail "project.md / *.project.md ledgers are retired; use GitHub Issue/Project task truth"
fi

active_governance_docs=()
if governance_paths=$(find "${scan_roots[@]}" -type f -name '*.md' \
    ! -path 'doc/devlog/*' ! -path '*/archive/*' ! -path '*/evidence/*' \
    ! -path 'doc/engineering/workflow/source-of-truth.md' \
    | sort); then
  while IFS= read -r file; do
    [[ -z "$file" ]] || active_governance_docs+=("$file")
  done <<< "$governance_paths"
else
  fail "governance wording path scan failed"
fi
if [[ ${#active_governance_docs[@]} -gt 0 ]]; then
if project_reference_hits=$(regex_match_with_line_numbers '(^|[^A-Za-z0-9_])([A-Za-z0-9_./*-]+)?(\.project|/project)\.md([^A-Za-z0-9_]|$)|同名[ `]*(project|项目)|配套[ `]*project( 文档)?|项目管理文档|same[- ]name[ `]*project|same[- ]named[[:space:]]+design[[:space:]]+and[[:space:]]+project|GitHub Issue/Project task truth[:：].*`[^`]+\.md`' "${active_governance_docs[@]}"); then
  echo "doc-governance-check: retired project ledger reference hits:"
  echo "$project_reference_hits"
  echo "doc-governance-check: advisory: review legacy project-ledger wording in its actual context"
else
  scan_status=$?
  [[ "$scan_status" -eq 1 ]] || fail "governance wording scan failed"
fi
fi

if ! navigation_errors=$(check_directory_navigation 2>&1); then
  echo "$navigation_errors"
  fail "top-level directory navigation failed"
fi

# 1) absolute path check
if [[ ${#all_doc_files[@]} -gt 0 ]]; then
if abs_hits=$(regex_match_with_line_numbers '/(Users|home)/[^[:space:]]+' "${all_doc_files[@]}"); then
  echo "doc-governance-check: absolute path hits:"
  echo "$abs_hits"
  echo "doc-governance-check: advisory: use portable paths in instructions; literal examples are permitted"
else
  scan_status=$?
  [[ "$scan_status" -eq 1 ]] || fail "absolute path scan failed"
fi
fi

# 2) editorial length advisory; never a submission or CI failure
if [[ ${#all_doc_files[@]} -gt 0 ]]; then
  if ! line_counts=$(awk 'FNR == 1 { if (NR > 1) print count "\t" prev; prev = FILENAME; count = 0 } { count++ } END { if (NR > 0) print count "\t" prev }' "${all_doc_files[@]}"); then
    fail "markdown line count scan failed"
  else
    while IFS=$'\t' read -r line_count file; do
      [[ -n "$file" ]] || continue
      if ((line_count > 1000)); then
        echo "doc-governance-check: advisory: $file exceeds 1000 lines (${line_count}); review readability, not a gate"
      fi
    done <<< "$line_counts"
  fi
fi

if ! run_product_doc_content_check; then
  fail "product document content contract failed"
fi

if ! run_system_design_traceability_check; then
  fail "system-design traceability contract failed"
fi

# Run historical reports separately, after both scoped strict gates. Their
# warning policy must never replace or reset a changed-range failure.
if ((full_corpus == 1)); then
  for checker in product-doc-content-check doc-core-content-check system-design-traceability-check; do
    if ! "$PYTHON_BIN" "scripts/${checker}.py" --warn-only \
      --repo-root "$repo_root" --full-corpus; then
      fail "${checker} full-corpus check failed"
    fi
  done
fi

if ((failures > 0)); then
  echo "doc-governance-check: failed with ${failures} issue(s)"
  exit 1
fi

echo "doc-governance-check: OK"
