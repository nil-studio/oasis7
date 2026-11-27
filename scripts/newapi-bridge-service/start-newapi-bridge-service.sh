#!/usr/bin/env bash
set -euo pipefail

require_env() {
  local name="$1"
  if [[ -z "${!name:-}" ]]; then
    echo "missing required environment variable: $name" >&2
    exit 1
  fi
}

require_env OASIS7_NEWAPI_BRIDGE_ROOT
require_env OASIS7_NEWAPI_BRIDGE_STATE_PATH
require_env OASIS7_NEWAPI_BRIDGE_LETAI_BASE_URL
require_env OASIS7_NEWAPI_BRIDGE_LETAI_PLATFORM_KEY

ROOT_DIR="$OASIS7_NEWAPI_BRIDGE_ROOT"
BRIDGE_BIN="${OASIS7_NEWAPI_BRIDGE_BIN:-$ROOT_DIR/oasis7_newapi_bridge_service}"
BIND_ADDR="${OASIS7_NEWAPI_BRIDGE_BIND_ADDR:-127.0.0.1:5852}"
STATE_PATH="$OASIS7_NEWAPI_BRIDGE_STATE_PATH"
ROUTE_TTL_SECONDS="${OASIS7_NEWAPI_BRIDGE_ROUTE_TTL_SECONDS:-900}"
DEPOSIT_ACCOUNT_PREFIX="${OASIS7_NEWAPI_BRIDGE_DEPOSIT_ACCOUNT_PREFIX:-oc:bridge:}"
LETAI_BASE_URL="$OASIS7_NEWAPI_BRIDGE_LETAI_BASE_URL"
LETAI_PLATFORM_KEY="$OASIS7_NEWAPI_BRIDGE_LETAI_PLATFORM_KEY"
LETAI_PARENT_CHANNEL_ID="${OASIS7_NEWAPI_BRIDGE_LETAI_PARENT_CHANNEL_ID:-}"
LETAI_TIMEOUT_MS="${OASIS7_NEWAPI_BRIDGE_LETAI_TIMEOUT_MS:-5000}"
RECONCILE_INTERVAL_SECONDS="${OASIS7_NEWAPI_BRIDGE_RECONCILE_INTERVAL_SECONDS:-0}"
MAX_CREDIT_ATTEMPTS="${OASIS7_NEWAPI_BRIDGE_MAX_CREDIT_ATTEMPTS:-3}"
CHAIN_BASE_URL="${OASIS7_NEWAPI_BRIDGE_CHAIN_BASE_URL:-}"
CHAIN_TIMEOUT_MS="${OASIS7_NEWAPI_BRIDGE_CHAIN_TIMEOUT_MS:-5000}"
CHAIN_CONFIRMATIONS_REQUIRED="${OASIS7_NEWAPI_BRIDGE_CHAIN_CONFIRMATIONS_REQUIRED:-1}"
PRICING_RULES_FILE="${OASIS7_NEWAPI_BRIDGE_PRICING_RULES_FILE:-$ROOT_DIR/scripts/newapi-bridge-service/pricing-rules.example.env}"
PRICING_RULES="${OASIS7_NEWAPI_BRIDGE_PRICING_RULES:-}"

if [[ -z "$PRICING_RULES" ]]; then
  if [[ ! -f "$PRICING_RULES_FILE" ]]; then
    echo "pricing rules file does not exist: $PRICING_RULES_FILE" >&2
    exit 1
  fi
  PRICING_RULES="$(
    sed -n 's/^OASIS7_NEWAPI_BRIDGE_PRICING_RULES="\([^"]*\)"$/\1/p' "$PRICING_RULES_FILE"
  )"
fi

if [[ ! -x "$BRIDGE_BIN" ]]; then
  echo "bridge binary is not executable: $BRIDGE_BIN" >&2
  exit 1
fi

cmd=(
  "$BRIDGE_BIN"
  --bind-addr "$BIND_ADDR"
  --state-path "$STATE_PATH"
  --route-ttl-seconds "$ROUTE_TTL_SECONDS"
  --deposit-account-prefix "$DEPOSIT_ACCOUNT_PREFIX"
  --letai-base-url "$LETAI_BASE_URL"
  --letai-platform-key-env OASIS7_NEWAPI_BRIDGE_LETAI_PLATFORM_KEY
  --letai-timeout-ms "$LETAI_TIMEOUT_MS"
  --max-credit-attempts "$MAX_CREDIT_ATTEMPTS"
  --chain-timeout-ms "$CHAIN_TIMEOUT_MS"
  --chain-confirmations-required "$CHAIN_CONFIRMATIONS_REQUIRED"
)

if [[ -n "$LETAI_PARENT_CHANNEL_ID" ]]; then
  cmd+=(--letai-parent-channel-id "$LETAI_PARENT_CHANNEL_ID")
fi

if [[ "$RECONCILE_INTERVAL_SECONDS" != "0" ]]; then
  cmd+=(--reconcile-interval-seconds "$RECONCILE_INTERVAL_SECONDS")
fi

if [[ -n "$CHAIN_BASE_URL" ]]; then
  cmd+=(--chain-base-url "$CHAIN_BASE_URL")
fi

IFS=',' read -r -a pricing_rule_array <<< "$PRICING_RULES"
pricing_rule_count=0
for pricing_rule in "${pricing_rule_array[@]}"; do
  pricing_rule="${pricing_rule//[[:space:]]/}"
  if [[ -z "$pricing_rule" ]]; then
    continue
  fi
  cmd+=(--pricing-rule "$pricing_rule")
  pricing_rule_count=$((pricing_rule_count + 1))
done

if [[ "$pricing_rule_count" -eq 0 ]]; then
  echo "no usable pricing rules found in OASIS7_NEWAPI_BRIDGE_PRICING_RULES" >&2
  exit 1
fi

exec "${cmd[@]}"
