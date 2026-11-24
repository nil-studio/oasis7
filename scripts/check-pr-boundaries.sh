#!/usr/bin/env bash
# Enforce PR slice size caps, Allowed-Crates scope-lock, L0..L3 ring span, and branch staleness circuit-breakers.
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
HEAD_REF="${HEAD_REF:-HEAD}"
PR_TITLE="${PR_TITLE:-}"
PR_BODY="${PR_BODY:-}"
PR_LABELS="${PR_LABELS:-}"
PR_AUTHOR="${PR_AUTHOR:-}"
CLI_SLICE=""
CLI_ALLOWED_CRATES=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --base)
      [[ $# -ge 2 ]] || { echo "Missing value for --base" >&2; exit 2; }
      BASE_REF="$2"
      shift 2
      ;;
    --head)
      [[ $# -ge 2 ]] || { echo "Missing value for --head" >&2; exit 2; }
      HEAD_REF="$2"
      shift 2
      ;;
    --slice)
      [[ $# -ge 2 ]] || { echo "Missing value for --slice" >&2; exit 2; }
      CLI_SLICE="$2"
      shift 2
      ;;
    --allowed-crates)
      [[ $# -ge 2 ]] || { echo "Missing value for --allowed-crates" >&2; exit 2; }
      CLI_ALLOWED_CRATES="$2"
      shift 2
      ;;
    -h|--help)
      echo "Usage: $0 [--base origin/main] [--head HEAD] [--slice Dark|Adapter|Cutover|Bugfix|Docs|Chore] [--allowed-crates crate_a,crate_b|none]"
      exit 0
      ;;
    *)
      echo "Unknown argument: $1" >&2
      exit 2
      ;;
  esac
done

normalize_slice_name() {
  local lower
  lower="$(printf '%s' "$1" | tr '[:upper:]' '[:lower:]')"
  case "$lower" in
    dark)    echo "Dark" ;;
    adapter) echo "Adapter" ;;
    cutover) echo "Cutover" ;;
    bugfix)  echo "Bugfix" ;;
    docs)    echo "Docs" ;;
    chore)   echo "Chore" ;;
    *)       printf '%s' "$1" ;;
  esac
}

extract_body_field() {
  local field_name="$1"
  printf '%s\n' "$PR_BODY" | awk -v key="$field_name" '
    BEGIN { key_l = tolower(key) }
    {
      line = $0
      sub(/\r$/, "", line)
      while (match(line, /<!--[^>]*-->/)) {
        line = substr(line, 1, RSTART - 1) substr(line, RSTART + RLENGTH)
      }
      trimmed = line
      sub(/^[-[:space:]*]+/, "", trimmed)
      if (tolower(substr(trimmed, 1, length(key_l))) == key_l) {
        rest = substr(trimmed, length(key_l) + 1)
        if (match(rest, /^[*]*:[[:space:]*]*/)) {
          val = substr(rest, RLENGTH + 1)
          gsub(/`/, "", val)
          sub(/^[[:space:]]+/, "", val)
          sub(/[[:space:]]+$/, "", val)
          print val
          exit
        }
      }
    }
  '
}

# 1. Human Escape Hatch & Bot Exemption
if [[ "$PR_AUTHOR" == "dependabot[bot]" ]]; then
  echo "[PASS] Exempting automated dependency PR from dependabot[bot]."
  exit 0
fi

if [[ ",${PR_LABELS}," == *",pr-boundary-override,"* ]]; then
  echo "[WARN] Label 'pr-boundary-override' detected. Bypassing boundary checks by maintainer override."
  exit 0
fi

if ! git rev-parse --verify "${BASE_REF}^{commit}" >/dev/null 2>&1; then
  if git rev-parse --verify "main^{commit}" >/dev/null 2>&1; then
    BASE_REF="main"
  else
    echo "[ERROR] Cannot resolve base reference '$BASE_REF' or 'main'. Fetch main first." >&2
    exit 1
  fi
fi

if ! git rev-parse --verify "${HEAD_REF}^{commit}" >/dev/null 2>&1; then
  echo "[ERROR] Cannot resolve head reference '$HEAD_REF'." >&2
  exit 1
fi

MERGE_BASE="$(git merge-base "$BASE_REF" "$HEAD_REF")"

# 2. Parse Slice-Type & Allowed-Crates Contract
SLICE_TYPE=""
if [[ -n "$CLI_SLICE" ]]; then
  SLICE_TYPE="$(normalize_slice_name "$CLI_SLICE")"
fi

if [[ -z "$SLICE_TYPE" && -n "$PR_TITLE" ]]; then
  PARSED_TITLE_SLICE="$(printf '%s\n' "$PR_TITLE" | awk '
    {
      line = $0
      sub(/\r$/, "", line)
      if (match(tolower(line), /\[slice:[[:space:]]*(dark|adapter|cutover|bugfix|docs|chore)\]/)) {
        seg = substr(line, RSTART, RLENGTH)
        sub(/^\[[Ss][Ll][Ii][Cc][Ee]:[[:space:]]*/, "", seg)
        sub(/\]$/, "", seg)
        print seg
        exit
      }
    }
  ')"
  if [[ -n "$PARSED_TITLE_SLICE" ]]; then
    SLICE_TYPE="$(normalize_slice_name "$PARSED_TITLE_SLICE")"
  fi
fi

if [[ -z "$SLICE_TYPE" && -n "$PR_BODY" ]]; then
  PARSED_BODY_SLICE="$(extract_body_field "Slice-Type" | awk '{ print $1 }')"
  if [[ -n "$PARSED_BODY_SLICE" ]]; then
    CANDIDATE_SLICE="$(normalize_slice_name "$PARSED_BODY_SLICE")"
    case "$CANDIDATE_SLICE" in
      Dark|Adapter|Cutover|Bugfix|Docs|Chore) SLICE_TYPE="$CANDIDATE_SLICE" ;;
    esac
  fi
fi

if [[ -z "$SLICE_TYPE" ]]; then
  if [[ -n "${GITHUB_ACTIONS:-}" ]]; then
    echo "[FAIL] Missing Slice Type! Add '[Slice: Dark|Adapter|Cutover|Bugfix|Docs|Chore]' to PR title or 'Slice-Type: <Type>' in PR body." >&2
    exit 1
  else
    echo "[INFO] No --slice provided in local mode; defaulting to 'Dark' threshold check."
    SLICE_TYPE="Dark"
  fi
fi

case "$SLICE_TYPE" in
  Dark|Adapter|Cutover|Bugfix|Docs|Chore) ;;
  *)
    echo "[FAIL] Unsupported Slice-Type '$SLICE_TYPE'." >&2
    exit 1
    ;;
esac

ALLOWED_CRATES_RAW="$CLI_ALLOWED_CRATES"
if [[ -z "$ALLOWED_CRATES_RAW" && -n "$PR_BODY" ]]; then
  ALLOWED_CRATES_RAW="$(extract_body_field "Allowed-Crates")"
fi

if [[ -n "${GITHUB_ACTIONS:-}" && "$SLICE_TYPE" =~ ^(Dark|Adapter|Cutover|Bugfix)$ && -z "$ALLOWED_CRATES_RAW" ]]; then
  echo "[FAIL] Missing 'Allowed-Crates:' scope contract in PR body (lock target crates or 'none' at Draft PR creation)." >&2
  exit 1
fi

# 3. Zero-Diff Contract Draft PR Short-Circuit (Empty Commit Scope-Lock Mode)
CHANGED_FILES_COUNT="$(git diff --name-only --no-renames "${MERGE_BASE}...${HEAD_REF}" | wc -l | tr -d '[:space:]')"
if (( CHANGED_FILES_COUNT == 0 )); then
  echo "[PASS] Zero-diff Contract Draft PR verified (Slice: ${SLICE_TYPE}, Allowed-Crates: ${ALLOWED_CRATES_RAW:-unspecified})."
  echo "       Scope is locked; skipping line-churn and staleness checks until code is pushed."
  exit 0
fi

if [[ -n "${GITHUB_ACTIONS:-}" && "$SLICE_TYPE" =~ ^(Dark|Adapter|Cutover|Bugfix)$ ]]; then
  IMPACT_VAL="$(extract_body_field "Active-Path-Impact" | tr -d '[:space:]')"
  if [[ -z "$IMPACT_VAL" ]]; then
    echo "[FAIL] Missing 'Active-Path-Impact:' declaration in PR body." >&2
    exit 1
  fi
fi

# 4. Configure Thresholds per Slice Type
MAX_CRATES="${MAX_CRATES:-3}"
MAX_RING_SPAN="${MAX_RING_SPAN:-1}"
WARN_BEHIND_COMMITS="${WARN_BEHIND_COMMITS:-25}"
MAX_BEHIND_COMMITS="${MAX_BEHIND_COMMITS:-60}"

case "$SLICE_TYPE" in
  Dark)       MAX_LOC="${MAX_LOC_DARK:-800}" ;;
  Adapter)    MAX_LOC="${MAX_LOC_ADAPTER:-600}" ;;
  Cutover)    MAX_LOC="${MAX_LOC_CUTOVER:-150}" ;;
  Bugfix)     MAX_LOC="${MAX_LOC_BUGFIX:-400}" ;;
  Docs|Chore) MAX_LOC="${MAX_LOC_CHORE:-1200}" ;;
esac

# 5. Staleness / Drift Circuit-Breaker Check
BEHIND_COUNT="$(git rev-list --count "${HEAD_REF}..${BASE_REF}")"
if (( BEHIND_COUNT > MAX_BEHIND_COMMITS )); then
  echo "[FAIL] Branch is ${BEHIND_COUNT} commits behind ${BASE_REF} (limit: ${MAX_BEHIND_COMMITS})." >&2
  echo "       Circuit breaker triggered: STOP patching stale branch; follow Donor-Branch Salvage Protocol." >&2
  exit 1
elif (( BEHIND_COUNT > WARN_BEHIND_COMMITS )); then
  echo "[WARN] Branch is ${BEHIND_COUNT} commits behind ${BASE_REF}. Rebase or land slice soon to avoid drift."
fi

# 6. Track Touched Crates (no-renames prevents cross-directory move bypass) & Calculate Effective LOC Churn
TOUCHED_CRATES_LIST=""
while IFS= read -r path; do
  [[ -z "${path:-}" ]] && continue
  if [[ "$SLICE_TYPE" == "Docs" && "$path" == crates/* ]]; then
    echo "[FAIL] [Slice: Docs] cannot modify Rust/Viewer crate files ('$path')." >&2
    exit 1
  fi
  if [[ "$path" =~ ^crates/([^/]+)/ ]]; then
    crate_name="${BASH_REMATCH[1]}"
    if [[ " $TOUCHED_CRATES_LIST " != *" $crate_name "* ]]; then
      TOUCHED_CRATES_LIST="${TOUCHED_CRATES_LIST:+$TOUCHED_CRATES_LIST }$crate_name"
    fi
  fi
done < <(git diff --name-only --no-renames "${MERGE_BASE}...${HEAD_REF}")

if [[ -n "${GITHUB_ACTIONS:-}" && "$SLICE_TYPE" == "Chore" && -n "$TOUCHED_CRATES_LIST" && -z "$ALLOWED_CRATES_RAW" ]]; then
  echo "[FAIL] [Slice: Chore] touches workspace crates ($TOUCHED_CRATES_LIST) without an explicit 'Allowed-Crates:' contract." >&2
  exit 1
fi

TOTAL_ADDED=0
TOTAL_DELETED=0
while IFS=$'\t' read -r added deleted path; do
  [[ -z "${path:-}" ]] && continue
  [[ "$added" == "-" || "$deleted" == "-" ]] && continue

  case "$path" in
    Cargo.lock|*/Cargo.lock|*package-lock.json|*pnpm-lock.yaml|skills-lock.json|*.snap) continue ;;
    fixtures/*|doc/testing/evidence/*|vendor/*|third_party/*) continue ;;
  esac

  TOTAL_ADDED=$((TOTAL_ADDED + added))
  TOTAL_DELETED=$((TOTAL_DELETED + deleted))
done < <(git diff --numstat -M "${MERGE_BASE}...${HEAD_REF}")

TOTAL_CHURN=$((TOTAL_ADDED + TOTAL_DELETED))

# 7. Map Crates to Architecture Rings & Enforce Allowed-Crates Whitelist
crate_to_ring() {
  case "$1" in
    oasis7_proto|oasis7_node|oasis7_net|oasis7_consensus|oasis7_distfs|\
    oasis7_domain_types|oasis7_formal_protocol|oasis7_cas|oasis7_context|\
    oasis7_custody_core|oasis7_local_signer|oasis7_secure_envelope|\
    oasis7_signer_protocol|oasis7_observability)
      echo 0
      ;;
    oasis7|oasis7_simulator|oasis7_runtime|oasis7_wasm_*|\
    oasis7_builtin_wasm_modules|oasis7_executable_image|oasis7_runtime_host)
      echo 1
      ;;
    oasis7_world_state|oasis7_world_projection|oasis7_world_service|\
    oasis7_chain_runtime|oasis7_custody_worker|oasis7_dev_test_worker|\
    oasis7_hosted_access|oasis7_letai_credit_adapter|\
    oasis7_newapi_bridge_service|oasis7_provider_local_bridge)
      echo 2
      ;;
    oasis7_agent_*|oasis7_model_provider|oasis7_client_*|oasis7_game_client|\
    oasis7_browser_game_client|oasis7_launcher_*|oasis7_ops_tools|\
    oasis7_protocol_tools|oasis7_provider_tools|oasis7_viewer*|pixel_world_bridge)
      echo 3
      ;;
    *)
      echo -1
      ;;
  esac
}

CRATE_COUNT=0
MIN_RING=99
MAX_RING=-1
RING_SUMMARY=""
FAILED=0

ALLOWED_NORM=""
if [[ -n "$ALLOWED_CRATES_RAW" ]]; then
  ALLOWED_COMPACT="$(printf '%s' "$ALLOWED_CRATES_RAW" | tr -d '[:space:]')"
  ALLOWED_LOWER="$(printf '%s' "$ALLOWED_COMPACT" | tr '[:upper:]' '[:lower:]')"
  if [[ "$ALLOWED_LOWER" == "none" ]]; then
    ALLOWED_NORM=",__none__,"
  else
    ALLOWED_NORM=",${ALLOWED_COMPACT},"
  fi
fi

for crate in $TOUCHED_CRATES_LIST; do
  CRATE_COUNT=$((CRATE_COUNT + 1))
  ring="$(crate_to_ring "$crate")"
  if (( ring >= 0 )); then
    (( ring < MIN_RING )) && MIN_RING=$ring
    (( ring > MAX_RING )) && MAX_RING=$ring
    RING_SUMMARY="${RING_SUMMARY} ${crate}(L${ring})"
  else
    RING_SUMMARY="${RING_SUMMARY} ${crate}(L?)"
    echo "[WARN] Crate '$crate' is not mapped in scripts/check-pr-boundaries.sh; please assign it to L0..L3."
  fi

  if [[ -n "$ALLOWED_NORM" && "$ALLOWED_NORM" != *",${crate},"* ]]; then
    echo "[FAIL] Scope Drift Detected! Touched crate '$crate' is outside locked Allowed-Crates ($ALLOWED_CRATES_RAW)." >&2
    echo "       Implementation deviated from Draft PR design contract. Revert out-of-scope changes or re-plan." >&2
    FAILED=1
  fi
done

echo "=== PR Boundary Gate Summary ==="
echo "  Slice Type      : ${SLICE_TYPE}"
echo "  Allowed Crates  : ${ALLOWED_CRATES_RAW:-<unconstrained-local>}"
echo "  Commits Behind  : ${BEHIND_COUNT} (warn > ${WARN_BEHIND_COMMITS}, max <= ${MAX_BEHIND_COMMITS})"
echo "  Effective Churn : ${TOTAL_CHURN} lines (+${TOTAL_ADDED} / -${TOTAL_DELETED}, limit <= ${MAX_LOC})"
echo "  Touched Crates  : ${CRATE_COUNT} (limit <= ${MAX_CRATES}) ->${RING_SUMMARY:- none}"

if (( TOTAL_CHURN > MAX_LOC )); then
  echo "[FAIL] Effective line churn (${TOTAL_CHURN}) exceeds '${SLICE_TYPE}' cap (${MAX_LOC})." >&2
  FAILED=1
fi

if (( CRATE_COUNT > MAX_CRATES )); then
  echo "[FAIL] Touched ${CRATE_COUNT} workspace crates (limit: ${MAX_CRATES}). Split across Dark/Adapter/Cutover PRs." >&2
  FAILED=1
fi

if (( MAX_RING >= 0 && MIN_RING <= 3 )); then
  RING_SPAN=$((MAX_RING - MIN_RING))
  if (( RING_SPAN > MAX_RING_SPAN )); then
    echo "[FAIL] Architecture ring jump too wide: L${MIN_RING} -> L${MAX_RING} (span ${RING_SPAN} > ${MAX_RING_SPAN})." >&2
    echo "       Single PR may only touch at most 2 adjacent rings (e.g., L1+L2 or L2+L3)." >&2
    FAILED=1
  fi
fi

if (( FAILED != 0 )); then
  echo "================================"
  echo "[CIRCUIT BREAKER TRIGGERED] Do NOT pile more commits onto this branch." >&2
  echo "See doc/engineering/workflow/pr-slicing-and-circuit-breaker.md for slicing & salvage steps." >&2
  exit 1
fi

echo "[PASS] PR boundaries and scope-lock contract within safe limits."
