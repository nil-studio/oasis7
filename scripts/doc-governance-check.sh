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
  1. Repository-owned documentation must not reintroduce project.md / *.project.md
     ledgers or active references to them.
  2. Non-archive/non-devlog markdown files must not contain absolute /Users/... or /home/... paths.
  - Non-archive/non-devlog markdown files over 1000 lines receive an advisory only.
  3. Root-level markdown files under doc/ must match the tracked allowlist.
  4. Root-level markdown files under each module (doc/<module>/*.md) must match
     the tracked allowlist (archive/devlog/.governance excluded).
  5. Non-archive/non-devlog markdown files must not reference missing markdown
     paths under doc/ (wildcards/templates and explicit exemption docs excluded).
  6. The thin product overlay must contain exactly four product-owned PRDs with
     stable metadata, lifecycle, authority backlinks, and acceptance traceability.
  7. Every live top-level doc directory must be registered with a valid entrypoint,
     root-navigation coverage, and lifecycle fields for controlled exceptions.
  8. New or materially changed product PRD/design files must pass the scoped
     content contract with the selected base/head and worktree changes.
  9. New or materially changed professional system-design files must carry
      exact upstream, local-design, and validation traceability.
  --full-corpus additionally reports historical product and professional design
  findings as warnings after the scoped strict checks; input errors still fail.
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

readonly REFERENCE_EXISTENCE_EXEMPT_DOCS=(
  "__no_reference_existence_exempt_docs__"
)
readonly DOC_ROOT_MD_ALLOWLIST_FILE="doc/.governance/doc-root-md-allowlist.txt"
readonly MODULE_ROOT_MD_ALLOWLIST_FILE="doc/.governance/module-root-md-allowlist.txt"
readonly TOP_LEVEL_DIRECTORY_REGISTRY="doc/.governance/top-level-directory-registry.json"

fail() {
  echo "doc-governance-check: FAIL: $*"
  failures=$((failures + 1))
}

if ! "$PYTHON_BIN" "$repo_root/scripts/document-corpus-inventory-check.py" --repo-root "$repo_root"; then
  fail "document corpus inventory contract"
fi


run_product_doc_content_check() {
  local base_oid="${OASIS7_PRODUCT_DOC_BASE:-}"
  local head_oid="${OASIS7_PRODUCT_DOC_HEAD:-}"
  if [[ -n "$base_oid" || -n "$head_oid" ]]; then
    if [[ -z "$base_oid" || -z "$head_oid" ]]; then
      echo "product-doc-content: error: explicit base/head must be supplied together" >&2
      return 1
    fi
  else
    head_oid="$(git rev-parse --verify HEAD^{commit})"
    base_oid="$(git merge-base HEAD main 2>/dev/null || true)"
    if [[ -z "$base_oid" ]]; then
      base_oid="$(git rev-parse --verify HEAD^ 2>/dev/null || true)"
    fi
  fi
  if [[ -z "$base_oid" || -z "$head_oid" ]]; then
    echo "product-doc-content: error: unable to derive explicit base/head for worktree check" >&2
    return 1
  fi
  "$PYTHON_BIN" scripts/product-doc-governance-check.py --enforce-changed \
    --repo-root "$repo_root" --base "$base_oid" --head "$head_oid" --worktree || return $?
  "$PYTHON_BIN" scripts/product-doc-content-check.py \
    --repo-root "$repo_root" --base "$base_oid" --head "$head_oid" --worktree || return $?
  "$PYTHON_BIN" scripts/doc-core-content-check.py \
    --repo-root "$repo_root" --base "$base_oid" --head "$head_oid" --worktree || return $?
}

run_system_design_traceability_check() {
  local base_oid="${OASIS7_PRODUCT_DOC_BASE:-}"
  local head_oid="${OASIS7_PRODUCT_DOC_HEAD:-}"
  if [[ -n "$base_oid" || -n "$head_oid" ]]; then
    if [[ -z "$base_oid" || -z "$head_oid" ]]; then
      echo "system-design-traceability: explicit base/head must be supplied together" >&2
      return 1
    fi
  else
    head_oid="$(git rev-parse --verify HEAD^{commit})"
    base_oid="$(git merge-base HEAD main 2>/dev/null || true)"
    if [[ -z "$base_oid" ]]; then
      base_oid="$(git rev-parse --verify HEAD^ 2>/dev/null || true)"
    fi
  fi
  if [[ -z "$base_oid" || -z "$head_oid" ]]; then
    echo "system-design-traceability: error: unable to derive explicit base/head for worktree check" >&2
    return 1
  fi
  "$PYTHON_BIN" scripts/system-design-traceability-check.py \
    --repo-root "$repo_root" --base "$base_oid" --head "$head_oid" --worktree
}

regex_match_file() {
  local regex="$1"
  local file="$2"
  if command -v rg >/dev/null 2>&1; then
    rg -q -e "$regex" "$file"
    return $?
  fi
  grep -Eq -- "$regex" "$file"
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
  for file in "$@"; do
    if grep -nE -- "$regex" "$file"; then
      status=0
    elif [[ "$?" -gt 1 ]]; then
      status=2
    fi
  done
  return "$status"
}

contains_literal() {
  local needle="$1"
  local file="$2"
  if command -v rg >/dev/null 2>&1; then
    rg -Fq -- "$needle" "$file"
    return $?
  fi
  grep -Fq -- "$needle" "$file"
}

collect_headings() {
  local file="$1"
  if command -v rg >/dev/null 2>&1; then
    rg '^#{1,6}[[:space:]].*$' "$file" || true
    return
  fi
  grep -E '^#{1,6}[[:space:]].*$' "$file" || true
}

headings_match_pattern() {
  local headings="$1"
  local pattern="$2"
  local regex="^#{1,6}[[:space:]]*([0-9]+([.][0-9]+)*[.]?[[:space:]]*)?${pattern}.*$"
  local line
  while IFS= read -r line; do
    if [[ "$line" =~ $regex ]]; then
      return 0
    fi
  done <<< "$headings"
  return 1
}

check_required_sections() {
  local file="$1"
  local headings="$2"
  shift 2
  local missing=()
  local token
  for token in "$@"; do
    if ! headings_match_pattern "$headings" "$token"; then
      missing+=("$token")
    fi
  done
  if [[ ${#missing[@]} -gt 0 ]]; then
    fail "$file missing sections: ${missing[*]}"
  fi
}

has_strict_prd_sections() {
  local headings="$1"
  headings_match_pattern "$headings" "Executive Summary"     && headings_match_pattern "$headings" "User Experience[[:space:]]*&[[:space:]]*Functionality"     && headings_match_pattern "$headings" "AI System Requirements[[:space:]]*\(If Applicable\)"     && headings_match_pattern "$headings" "Technical Specifications"     && headings_match_pattern "$headings" "Risks[[:space:]]*&[[:space:]]*Roadmap"     && headings_match_pattern "$headings" "Validation[[:space:]]*&[[:space:]]*Decision Record"
}

check_allowlist_match() {
  local label="$1"
  local allowlist_file="$2"
  local actual_file="$3"
  local allowlist_tmp
  allowlist_tmp=$(mktemp)

  if [[ ! -f "$allowlist_file" ]]; then
    fail "${label} allowlist file missing: ${allowlist_file}"
    rm -f "$allowlist_tmp"
    return
  fi

  grep -Ev '^[[:space:]]*($|#)' "$allowlist_file" | sort -u > "$allowlist_tmp"
  sort -u -o "$actual_file" "$actual_file"

  local unexpected missing
  unexpected=$(comm -23 "$actual_file" "$allowlist_tmp" || true)
  missing=$(comm -13 "$actual_file" "$allowlist_tmp" || true)

  if [[ -n "$unexpected" ]]; then
    echo "doc-governance-check: ${label} unexpected entries:"
    echo "$unexpected"
    fail "${label} contains paths not tracked in allowlist"
  fi

  if [[ -n "$missing" ]]; then
    echo "doc-governance-check: ${label} missing entries (stale allowlist):"
    echo "$missing"
    fail "${label} allowlist contains paths that no longer exist"
  fi

  rm -f "$allowlist_tmp"
}

check_doc_path_references_batch() {
  local exempt_tmp doc_list_tmp
  local status=0
  exempt_tmp=$(mktemp)
  doc_list_tmp=$(mktemp)
  printf '%s\n' "${REFERENCE_EXISTENCE_EXEMPT_DOCS[@]}" > "$exempt_tmp"
  printf '%s\0' "${all_doc_files[@]}" > "$doc_list_tmp"

  "$PYTHON_BIN" - "$exempt_tmp" "$doc_list_tmp" <<'PY' || status=$?
from __future__ import annotations

from pathlib import Path
import re
import subprocess
import sys


exempt_path = Path(sys.argv[1])
doc_list_path = Path(sys.argv[2])
doc_files = [
    entry.decode("utf-8")
    for entry in doc_list_path.read_bytes().split(b"\0")
    if entry
]
exempt_docs = {
    line.strip()
    for line in exempt_path.read_text(encoding="utf-8").splitlines()
    if line.strip()
}

reference_re = re.compile(r"doc/[A-Za-z0-9_./-]+\.md")
# Historical references are exempt only when they pin this repository's exact
# 40-hex commit and the named Markdown blob exists in that commit.
historical_reference_re = re.compile(
    r"(?:eng-cc/oasis7@(?P<repo_commit>[0-9a-fA-F]{40}):|https://github\.com/eng-cc/oasis7/blob/(?P<web_commit>[0-9a-fA-F]{40})/)"
    r"(?P<path>doc/[A-Za-z0-9_./-]+\.md)(?=[#?)/\s`]|$)"
)
skip_markers = ("*", "?", "[", "]", "{", "}", "YYYY-MM-DD")
historical_object_cache: dict[tuple[str, str], bool] = {}


def historical_blob_exists(commit: str, ref_path: str) -> bool:
    normalized = Path(ref_path)
    if normalized.is_absolute() or ".." in normalized.parts:
        return False
    key = (commit.lower(), ref_path)
    if key not in historical_object_cache:
        commit_result = subprocess.run(
            ["git", "cat-file", "-t", key[0]],
            capture_output=True,
            text=True,
            check=False,
        )
        blob_result = subprocess.run(
            ["git", "cat-file", "-t", f"{key[0]}:{ref_path}"],
            capture_output=True,
            text=True,
            check=False,
        ) if commit_result.returncode == 0 and commit_result.stdout.strip() == "commit" else None
        historical_object_cache[key] = (
            blob_result is not None
            and blob_result.returncode == 0
            and blob_result.stdout.strip() == "blob"
        )
    return historical_object_cache[key]

for file in doc_files:
    if file in exempt_docs:
        continue
    path = Path(file)
    text = path.read_text(encoding="utf-8")
    historical_matches = list(historical_reference_re.finditer(text))
    unresolved_references = set()
    for reference_match in reference_re.finditer(text):
        ref_path = reference_match.group(0)
        if any(marker in ref_path for marker in skip_markers):
            continue
        if not Path(ref_path).is_file():
            pinned_historical_ref = any(
                history_match.span("path") == reference_match.span()
                and history_match.group("path") == ref_path
                and historical_blob_exists(
                    history_match.group("repo_commit") or history_match.group("web_commit"),
                    ref_path,
                )
                for history_match in historical_matches
            )
            if pinned_historical_ref:
                continue
            unresolved_references.add(ref_path)
    for ref_path in sorted(unresolved_references):
        print(f"{file}\t{ref_path}")
PY

  rm -f "$exempt_tmp"
  rm -f "$doc_list_tmp"
  return "$status"
}

check_top_level_directory_registry() {
  "$PYTHON_BIN" - "$TOP_LEVEL_DIRECTORY_REGISTRY" "doc/README.md" <<'PY'
from __future__ import annotations
import json
from pathlib import Path
import sys

registry_path = Path(sys.argv[1])
root_readme = Path(sys.argv[2])
required_fields = ("name", "type", "owner", "entry", "reason", "exception")
exception_types = {"retired_archive", "ephemeral_evidence_pool"}
allowed_types = {"professional_domain", "evidence_domain", "product_overlay", *exception_types}
if not registry_path.is_file():
    raise SystemExit(f"top-level-directory-registry: missing registry: {registry_path}")
if not root_readme.is_file():
    raise SystemExit(f"top-level-directory-registry: missing root navigation: {root_readme}")
try:
    payload = json.loads(registry_path.read_text(encoding="utf-8"))
except json.JSONDecodeError as error:
    raise SystemExit(f"top-level-directory-registry: invalid JSON: {error}")
if payload.get("version") != 1 or not isinstance(payload.get("directories"), list):
    raise SystemExit("top-level-directory-registry: expected version 1 and directories list")
doc_root = Path("doc")
symlink_entries = sorted(
    path.name
    for path in doc_root.iterdir()
    if path.is_symlink() and not path.name.startswith(".")
)
actual = {
    path.name
    for path in doc_root.iterdir()
    if path.is_dir() and not path.is_symlink() and not path.name.startswith(".")
}
entries = payload["directories"]
registered, errors = [], []
for name in symlink_entries:
    errors.append(
        f"top-level-directory-registry: doc/{name} must be a real directory, not a symlink"
    )
owners = {path.stem for path in Path(".agents/roles").glob("*.md")}
root_navigation = root_readme.read_text(encoding="utf-8")
for index, entry in enumerate(entries):
    prefix = f"top-level-directory-registry: directories[{index}]"
    if not isinstance(entry, dict):
        errors.append(f"{prefix} must be an object")
        continue
    missing = [field for field in required_fields if field not in entry or entry[field] in (None, "")]
    if missing and not (missing == ["exception"] and entry.get("type") not in exception_types):
        errors.append(f"{prefix} missing fields: {', '.join(missing)}")
    name = entry.get("name")
    if not isinstance(name, str) or not name:
        errors.append(f"{prefix} has invalid name")
        continue
    registered.append(name)
    if entry.get("type") not in allowed_types:
        errors.append(f"{prefix} {name} has invalid type: {entry.get('type')!r}")
    if entry.get("owner") not in owners:
        errors.append(f"{prefix} {name} has unknown owner: {entry.get('owner')!r}")
    path = entry.get("entry")
    if not isinstance(path, str) or not Path(path).is_file():
        errors.append(f"{prefix} {name} has missing entry: {path!r}")
    elif f"doc/{name}/" not in root_navigation:
        errors.append(f"{prefix} {name} directory is absent from doc/README.md navigation")
    if entry.get("type") in exception_types:
        lifecycle = entry.get("exception")
        if not isinstance(lifecycle, dict):
            errors.append(f"{prefix} {name} exception lifecycle must be an object")
        else:
            for field in ("entry_conditions", "review_trigger", "exit_conditions"):
                if not isinstance(lifecycle.get(field), str) or not lifecycle[field].strip():
                    errors.append(f"{prefix} {name} exception missing {field}")
    elif entry.get("exception") is not None:
        errors.append(f"{prefix} {name} non-exception must use null exception")
if len(registered) != len(set(registered)):
    errors.append("top-level-directory-registry: duplicate directory registration")
if set(registered) != actual:
    errors.append(f"top-level-directory-registry: directory set mismatch: expected {sorted(actual)!r}, got {sorted(set(registered))!r}")
product_overlays = sum(entry.get("type") == "product_overlay" for entry in entries if isinstance(entry, dict))
if ("product" in actual and product_overlays != 1) or ("product" not in actual and product_overlays != 0):
    errors.append("top-level-directory-registry: product directory must have exactly one product_overlay")
if errors:
    raise SystemExit("\n".join(errors))
PY
}

all_doc_files=()
while IFS= read -r file; do
  all_doc_files+=("$file")
done < <(find doc -type f -name '*.md' ! -path 'doc/devlog/*' ! -path '*/archive/*' | sort)

# 0) project-ledger retirement is permanent
project_ledger_paths=()
while IFS= read -r file; do
  project_ledger_paths+=("$file")
done < <(
  find doc site tools skills .agents -type f \
    \( -name 'project.md' -o -name '*.project.md' \) \
    ! -path '*/third_party/*' ! -path '*/target/*' ! -path '*/node_modules/*' \
    | sort
)
if [[ ${#project_ledger_paths[@]} -gt 0 ]]; then
  printf 'doc-governance-check: retired project ledger paths:\n%s\n' "${project_ledger_paths[*]}"
  fail "project.md / *.project.md ledgers are retired; use GitHub Issue/Project task truth"
fi

active_governance_docs=()
while IFS= read -r file; do
  active_governance_docs+=("$file")
done < <(
  find doc site skills .agents -type f -name '*.md' \
    ! -path 'doc/devlog/*' ! -path '*/archive/*' ! -path '*/evidence/*' \
    ! -path 'doc/engineering/workflow/source-of-truth.md' \
    | sort
)
if project_reference_hits=$(regex_match_with_line_numbers '(^|[^A-Za-z0-9_])([A-Za-z0-9_./*-]+)?(\.project|/project)\.md([^A-Za-z0-9_]|$)|同名[ `]*(project|项目)|配套[ `]*project( 文档)?|项目管理文档|same[- ]name[ `]*project|same[- ]named[[:space:]]+design[[:space:]]+and[[:space:]]+project|GitHub Issue/Project task truth[:：].*`[^`]+\.md`' "${active_governance_docs[@]}"); then
  echo "doc-governance-check: retired project ledger reference hits:"
  echo "$project_reference_hits"
  fail "active documentation references retired project ledgers or legacy project-management documents"
fi

if [[ ${#all_doc_files[@]} -eq 0 ]]; then
  fail "no markdown files found under doc/"
fi

if ! registry_errors=$(check_top_level_directory_registry 2>&1); then
  echo "$registry_errors"
  fail "top-level directory registry contract failed"
fi

# 1) absolute path check
if abs_hits=$(regex_match_with_line_numbers '/(Users|home)/[^[:space:]]+' "${all_doc_files[@]}"); then
  echo "doc-governance-check: absolute path hits:"
  echo "$abs_hits"
  fail "absolute user-home paths found in non-archive docs"
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

# 3) markdown doc path references must exist (except explicit exemptions)
doc_reference_scan_tmp=$(mktemp)
if ! check_doc_path_references_batch > "$doc_reference_scan_tmp"; then
  rm -f "$doc_reference_scan_tmp"
  fail "markdown doc path reference scan failed"
else
while IFS=$'\t' read -r file ref_path; do
  [[ -n "${file:-}" && -n "${ref_path:-}" ]] || continue
  fail "$file references missing markdown path: $ref_path"
done < "$doc_reference_scan_tmp"
  rm -f "$doc_reference_scan_tmp"
fi

doc_root_actual_tmp=$(mktemp)
module_root_actual_tmp=$(mktemp)

find doc -mindepth 1 -maxdepth 1 -type f -name '*.md' | sort > "$doc_root_actual_tmp"
find doc -mindepth 2 -maxdepth 2 -type f -name '*.md' \
  ! -path 'doc/archive/*' \
  ! -path 'doc/devlog/*' \
  ! -path 'doc/.governance/*' \
  | sort > "$module_root_actual_tmp"

check_allowlist_match "doc root markdown set" "$DOC_ROOT_MD_ALLOWLIST_FILE" "$doc_root_actual_tmp"
check_allowlist_match "module root markdown set" "$MODULE_ROOT_MD_ALLOWLIST_FILE" "$module_root_actual_tmp"

rm -f "$doc_root_actual_tmp" "$module_root_actual_tmp"

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
