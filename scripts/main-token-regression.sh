#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"
source "$ROOT_DIR/scripts/cargo-dev-lib.sh"

TIER="${1:-required}"

usage() {
  cat <<'USAGE'
Usage: scripts/main-token-regression.sh [required|full]

Runs main-token and NodePoints-bridge focused regression suites.
USAGE
}

run() (
  local tier_feature="$1" selector="$2" log
  local -a cmd=(oasis7_cargo_dev test -p oasis7_runtime --lib --features "$tier_feature"
    "$selector" -- --format pretty --color never)
  log="$(mktemp)" || exit 1
  trap 'rm -f -- "$log"' EXIT
  echo "+ ${cmd[*]}"
  "${cmd[@]}" 2>&1 | tee "$log" || exit "$?"
  python3 "$ROOT_DIR/scripts/ci-local-check-command.py" --verify-libtest contains "$selector" "$log"
)

case "${TIER}" in
  required)
    run test_tier_required tests::main_token::
    run test_tier_required tests::reward_asset_settlement_action::
    ;;
  full)
    run test_tier_required tests::main_token::
    run test_tier_required tests::reward_asset_settlement_action::
    run test_tier_full tests::main_token::
    run test_tier_full tests::reward_asset_settlement_action::
    run test_tier_full tests::reward_asset::
    ;;
  *)
    usage
    exit 1
    ;;
esac
