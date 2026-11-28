#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"
source "$repo_root/scripts/cargo-dev-lib.sh"

fixture_dir="fixtures/llm_baseline/state_01"
snapshot_path="$fixture_dir/snapshot.json"
journal_path="$fixture_dir/journal.json"

if [[ ! -f "$snapshot_path" ]]; then
  echo "missing baseline snapshot fixture: $snapshot_path" >&2
  exit 2
fi

if [[ ! -f "$journal_path" ]]; then
  echo "missing baseline journal fixture: $journal_path" >&2
  exit 2
fi

echo "+ baseline fixture: $fixture_dir"
echo "+ oasis7_cargo_dev test -p oasis7_runtime_host --lib --features test_tier_full simulator_tests::persist::kernel_loads_tracked_llm_baseline_fixture_state -- --exact --nocapture"
oasis7_cargo_dev test -p oasis7_runtime_host --lib --features test_tier_full simulator_tests::persist::kernel_loads_tracked_llm_baseline_fixture_state -- --exact --nocapture
echo "+ oasis7_cargo_dev test -p oasis7_provider_tools --bin oasis7_llm_agent_demo --features test_tier_full runtime_bridge_continues_governance_from_tracked_baseline_fixture -- --nocapture"
oasis7_cargo_dev test -p oasis7_provider_tools --bin oasis7_llm_agent_demo --features test_tier_full runtime_bridge_continues_governance_from_tracked_baseline_fixture -- --nocapture
echo "+ oasis7_cargo_dev test -p oasis7_provider_tools --bin oasis7_llm_agent_demo --features test_tier_full runtime_bridge_civic_hotspot_preset_seeds_followup_handles -- --nocapture"
oasis7_cargo_dev test -p oasis7_provider_tools --bin oasis7_llm_agent_demo --features test_tier_full runtime_bridge_civic_hotspot_preset_seeds_followup_handles -- --nocapture
