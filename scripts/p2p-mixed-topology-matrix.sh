#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

usage() {
  cat <<'USAGE'
Usage: ./scripts/p2p-mixed-topology-matrix.sh [options]

Build or execute the P2PARCH-6 mixed-topology validation matrix.

Options:
  --tier <tier>                    required | full (default: required)
  --out-dir <path>                 output root (default: .tmp/p2p_mixed_topology)
  --shared-window-evidence-ref <path>
                                   attach same-window network-rehearsal evidence ref
                                   (repeatable; summary only, does not execute it)
  --dedicated-lab-evidence-ref <path>
                                   attach dedicated sentry/NAT/live-lab evidence ref
                                   (repeatable; summary only, does not execute it)
  --pass-uplift-decision-ref <ref> attach producer/QA pass-uplift decision ref
  --dry-run                        render commands and summary only
  -h, --help                       show help

Notes:
  - `required` runs deterministic exact-coverage cases only.
  - `full` runs the required set plus proxy longrun cases that exercise
    distributed recovery without claiming a dedicated sentry/NAT lab.
  - Case metadata distinguishes executable exact/proxy coverage from
    manual-lab-only and unsupported claim classes. Proxy coverage is never
    physical NAT/CGNAT truth without a dedicated lab or real-env evidence ref.
  - External evidence refs are copied into the summary so downstream gate
    tooling can see which shared-window / dedicated-lab / pass-uplift inputs
    were present for this run.
  - Artifacts are written to:
      <out-dir>/<timestamp>-<tier>/{summary.json,summary.md,cases/*}
USAGE
}

tier="required"
out_root=".tmp/p2p_mixed_topology"
dry_run=0
shared_window_refs=()
dedicated_lab_refs=()
pass_uplift_decision_ref=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --tier)
      tier=${2:-}
      shift 2
      ;;
    --out-dir)
      out_root=${2:-}
      shift 2
      ;;
    --shared-window-evidence-ref)
      shared_window_refs+=("${2:-}")
      shift 2
      ;;
    --dedicated-lab-evidence-ref)
      dedicated_lab_refs+=("${2:-}")
      shift 2
      ;;
    --pass-uplift-decision-ref)
      pass_uplift_decision_ref=${2:-}
      shift 2
      ;;
    --dry-run)
      dry_run=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "unknown option: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

case "$tier" in
  required|full) ;;
  *)
    echo "invalid --tier: $tier (expected required|full)" >&2
    exit 2
    ;;
esac

run_id=$(date +"%Y%m%d-%H%M%S")
run_dir="$out_root/${run_id}-${tier}"
cases_root="$run_dir/cases"
case_records="$run_dir/cases.ndjson"
summary_json="$run_dir/summary.json"
summary_md="$run_dir/summary.md"

mkdir -p "$cases_root"
: > "$case_records"

find_longrun_bash() {
  local candidate
  if [[ -n "${P2P_LONGRUN_BASH:-}" ]]; then
    printf '%s\n' "$P2P_LONGRUN_BASH"
    return 0
  fi
  for candidate in bash /opt/homebrew/bin/bash /usr/local/bin/bash; do
    if command -v "$candidate" >/dev/null 2>&1 && "$candidate" -c '(( BASH_VERSINFO[0] >= 4 ))' >/dev/null 2>&1; then
      command -v "$candidate"
      return 0
    fi
  done
  printf 'bash\n'
}

longrun_bash=$(find_longrun_bash)

write_json_array() {
  local out_file=$1
  shift
  if [[ $# -eq 0 ]]; then
    printf '[]\n' > "$out_file"
  else
    printf '%s\n' "$@" | jq -R . | jq -s . > "$out_file"
  fi
}

shared_window_refs_json="$run_dir/shared_window_refs.json"
dedicated_lab_refs_json="$run_dir/dedicated_lab_refs.json"
dedicated_lab_refs_ready=1
if ((${#shared_window_refs[@]})); then
  write_json_array "$shared_window_refs_json" "${shared_window_refs[@]}"
else
  write_json_array "$shared_window_refs_json"
fi
if ((${#dedicated_lab_refs[@]})); then
  for dedicated_lab_ref in "${dedicated_lab_refs[@]}"; do
    if [[ -z "$dedicated_lab_ref" || ! -f "$dedicated_lab_ref" ]]; then
      dedicated_lab_refs_ready=0
    fi
  done
  write_json_array "$dedicated_lab_refs_json" "${dedicated_lab_refs[@]}"
else
  dedicated_lab_refs_ready=0
  write_json_array "$dedicated_lab_refs_json"
fi

select_case() {
  local min_tier=$1
  if [[ "$tier" == "required" ]]; then
    [[ "$min_tier" == "required" ]]
  else
    return 0
  fi
}

case_table=$(cat <<'EOF'
nat_private_role_policy|required|exact|substrate_exact|home_nat|home_nat_private|none|may_direct_must_recover|may_direct_must_recover|policy_supported|exact_contract_not_physical_nat_truth|Validate private/home-NAT deployment-mode and role override plumbing.|Maps private validators/full nodes onto explicit P2P deployment policy instead of implicit public assumptions.|source ./scripts/cargo-dev-lib.sh; oasis7_cargo_dev test -p oasis7_chain_runtime --lib parse_options_reads_explicit_p2p_policy_overrides -- --nocapture
validator_hidden_boundary|required|exact|substrate_exact|validator_hidden|validator_hidden_to_public_network|none|must_not_publish_public_direct|must_not_publish_public_direct|policy_supported|exact_policy_boundary_not_reachability_proof|Validate validator_hidden remains bound to validator_core semantics.|Prevents observer/runtime role drift from claiming validator-hidden transport semantics.|source ./scripts/cargo-dev-lib.sh; oasis7_cargo_dev test -p oasis7_chain_runtime --lib node_network_policy_rejects_incompatible_runtime_role_combo -- --nocapture
relay_only_lane_budget|required|exact|substrate_exact|relay_only|relay_only_to_full_node|none|must_relay|must_relay_control_only|policy_supported|exact_lane_policy_not_relay_capacity_proof|Validate relay role stays confined to control-lane service surface.|Ensures relay/public ingress does not silently regain sync/blob request or serve rights.|source ./scripts/cargo-dev-lib.sh; oasis7_cargo_dev test -p oasis7_node network_policy_limits_relay_to_control_lane -- --nocapture
cgnat_relay_path_ranking|required|exact|substrate_exact|cgnat|cgnat_to_public_peer|none|may_direct_must_recover|prefer_direct_then_hole_punch_then_relay|policy_supported|exact_path_ranking_not_physical_cgnat_truth|Validate CGNAT peers rank direct before hole-punch before relay fallback.|Uses signed peer-record transport ordering as deterministic contract coverage; physical CGNAT claims still require dedicated lab or real-env refs.|source ./scripts/cargo-dev-lib.sh; oasis7_cargo_dev test -p oasis7_net --features libp2p peer_record_transport_paths_rank_direct_before_hole_punch_before_relay -- --nocapture
bootstrap_poisoning_dedupe|required|exact|substrate_exact|bootstrap_poisoning|bootstrap_peer_to_suspect_record|bootstrap_poisoning|may_direct_must_recover|must_recover_after_clean_record|policy_supported|exact_discovery_contract_not_public_network_truth|Validate poisoned bootstrap discovery does not permanently consume dial dedupe.|Covers discovery ingress quarantine against suspect records that later refresh healthy metadata.|source ./scripts/cargo-dev-lib.sh; oasis7_cargo_dev test -p oasis7_net --features libp2p process_discovered_peer_record_does_not_poison_dial_dedupe_for_suspect_peer -- --nocapture
relay_budget_detection|required|exact|substrate_exact|relay_exhaustion|many_peers_to_relay_domain|relay_exhaustion|may_direct_must_recover|must_detect_and_downgrade|policy_supported|exact_budget_policy_not_live_capacity_truth|Validate relay budget overflow is detected and downgraded before quarantine consumption.|Exact peer-manager coverage for relay-budget and relay-domain concentration detection during health recompute.|source ./scripts/cargo-dev-lib.sh; oasis7_cargo_dev test -p oasis7_net --features libp2p recompute_marks_relay_budget_and_domain_concentration -- --nocapture
path_failover_selection|required|exact|substrate_exact|path_failover|direct_peer_to_ranked_fallbacks|restart_pause_disconnect|may_direct_must_recover|direct_to_hole_punch_to_relay|policy_supported|exact_failover_contract_not_live_path_churn_truth|Validate direct-path failure falls back to hole-punch before relay.|Exact transport failover coverage for direct -> punched -> relay ordering.|source ./scripts/cargo-dev-lib.sh; oasis7_cargo_dev test -p oasis7_net --features libp2p preferred_transport_path_skips_direct_and_falls_back_to_hole_punch_before_relay -- --nocapture
sentry_loss_proxy_longrun|full|proxy|executable_proxy|sentry_loss|validator_sentry_proxy|sentry_loss|may_direct_must_recover|must_recover_via_remaining_paths|proxy_supported|proxy_not_dedicated_sentry_or_nat_lab_truth|Run triad_distributed ingress-loss proxy with disconnect/restart chaos.|Dedicated sentry live harness is not wired yet; triad_distributed ingress loss is the current executable proxy for sentry/anchor loss.|__LONGRUN_BASH__ ./scripts/p2p-longrun-soak.sh --profile soak_release --topologies triad_distributed --base-port 16610 --duration-secs 300 --max-stall-secs 240 --max-lag-p95 50 --max-distfs-failure-ratio 0.1 --chaos-continuous-enable --chaos-continuous-interval-secs 30 --chaos-continuous-start-sec 30 --chaos-continuous-max-events 8 --chaos-continuous-actions disconnect,restart --chaos-continuous-seed 20260403 --chaos-continuous-restart-down-secs 1 --chaos-continuous-pause-duration-secs 2 --out-dir __RUN_DIR__/sentry-loss-proxy
mixed_topology_release_proxy|full|proxy|executable_proxy|mixed_topology|public_private_proxy_mix|restart_pause_disconnect|may_direct_must_recover|must_recover_without_physical_nat_claim|proxy_supported|proxy_not_physical_nat_or_cgnat_truth|Run triad + triad_distributed release-profile proxy under mixed chaos.|Current runtime harness has no physical NAT/CGNAT lab; this proxy leaves a real distributed recovery command in the evidence bundle without overstating coverage.|__LONGRUN_BASH__ ./scripts/p2p-longrun-soak.sh --profile soak_release --topologies triad,triad_distributed --base-port 17610 --duration-secs 300 --max-stall-secs 240 --max-lag-p95 50 --max-distfs-failure-ratio 0.1 --chaos-continuous-enable --chaos-continuous-interval-secs 30 --chaos-continuous-start-sec 30 --chaos-continuous-max-events 8 --chaos-continuous-actions restart,pause,disconnect --chaos-continuous-seed 20260403 --chaos-continuous-restart-down-secs 1 --chaos-continuous-pause-duration-secs 2 --out-dir __RUN_DIR__/mixed-topology-release-proxy
EOF
)

echo "p2p mixed-topology matrix"
echo "- tier: $tier"
echo "- dry_run: $dry_run"
echo "- output: $run_dir"

while IFS='|' read -r case_id min_tier evidence_class execution_class scenario reachability_pair degradation_class path_expectation expected_route supported_status claim_boundary description note command_template; do
  [[ -z "$case_id" ]] && continue
  if ! select_case "$min_tier"; then
    continue
  fi

  case_dir="$cases_root/$case_id"
  mkdir -p "$case_dir"
  command=${command_template//__RUN_DIR__/$run_dir}
  command=${command//__LONGRUN_BASH__/$longrun_bash}
  printf '%s\n' "$command" > "$case_dir/command.txt"

  started_at=$(date -Iseconds)
  status="dry_run"
  exit_code=0

  if [[ "$dry_run" -eq 1 ]]; then
    printf 'dry-run only\n' > "$case_dir/stdout.log"
    : > "$case_dir/stderr.log"
    echo "+ dry-run [$case_id]: $command"
  else
    echo "+ [$case_id] $command"
    if bash -lc "$command" >"$case_dir/stdout.log" 2>"$case_dir/stderr.log"; then
      status="ok"
      exit_code=0
    else
      exit_code=$?
      status="failed"
    fi
  fi
  ended_at=$(date -Iseconds)

  jq -n \
    --arg case_id "$case_id" \
    --arg min_tier "$min_tier" \
    --arg coverage "$evidence_class" \
    --arg evidence_class "$evidence_class" \
    --arg execution_class "$execution_class" \
    --arg scenario "$scenario" \
    --arg reachability_pair "$reachability_pair" \
    --arg degradation_class "$degradation_class" \
    --arg path_expectation "$path_expectation" \
    --arg expected_route "$expected_route" \
    --arg supported_status "$supported_status" \
    --arg claim_boundary "$claim_boundary" \
    --arg description "$description" \
    --arg note "$note" \
    --arg command "$command" \
    --arg started_at "$started_at" \
    --arg ended_at "$ended_at" \
    --arg status "$status" \
    --arg stdout_log "$case_dir/stdout.log" \
    --arg stderr_log "$case_dir/stderr.log" \
    --argjson exit_code "$exit_code" \
    '{
      case_id: $case_id,
      min_tier: $min_tier,
      coverage: $coverage,
      evidence_class: $evidence_class,
      execution_class: $execution_class,
      scenario: $scenario,
      reachability_pair: $reachability_pair,
      degradation_class: $degradation_class,
      path_expectation: $path_expectation,
      expected_route: $expected_route,
      supported_status: $supported_status,
      claim_boundary: $claim_boundary,
      description: $description,
      note: $note,
      command: $command,
      started_at: $started_at,
      ended_at: $ended_at,
      status: $status,
      exit_code: $exit_code,
      stdout_log: $stdout_log,
      stderr_log: $stderr_log
    }' >> "$case_records"
done <<< "$case_table"

generated_at=$(date -Iseconds)

jq -s \
  --arg generated_at "$generated_at" \
  --arg tier "$tier" \
  --arg run_dir "$run_dir" \
  --arg summary_md "$summary_md" \
  --arg pass_uplift_decision_ref "$pass_uplift_decision_ref" \
  --argjson dry_run "$dry_run" \
  --argjson dedicated_lab_refs_ready "$dedicated_lab_refs_ready" \
  --slurpfile shared_window_refs "$shared_window_refs_json" \
  --slurpfile dedicated_lab_refs "$dedicated_lab_refs_json" \
  '{
    generated_at: $generated_at,
    tier: $tier,
    dry_run: ($dry_run == 1),
    run_dir: $run_dir,
    summary_md: $summary_md,
    cases: .,
    totals: {
      case_count: length,
      exact_case_count: (map(select(.coverage == "exact")) | length),
      proxy_case_count: (map(select(.coverage == "proxy")) | length),
      manual_lab_case_count: (map(select(.coverage == "manual_lab")) | length),
      unsupported_case_count: (map(select(.coverage == "unsupported")) | length),
      passed_count: (map(select(.status == "ok")) | length),
      failed_count: (map(select(.status == "failed")) | length),
      dry_run_count: (map(select(.status == "dry_run")) | length)
    },
    path_behavior_taxonomy: {
      evidence_classes: {
        exact: {
          execution: "deterministic_repo_test",
          claim_scope: "current substrate and policy contract only"
        },
        proxy: {
          execution: "executable_longrun_or_chaos_proxy",
          claim_scope: "mixed-topology recovery proxy; not physical NAT, CGNAT, or dedicated sentry lab truth"
        },
        manual_lab: {
          execution: "external_evidence_ref_required",
          claim_scope: "physical NAT, CGNAT, dedicated sentry, or public internet topology truth only when attached as dedicated lab or real-env evidence"
        },
        unsupported: {
          execution: "not_executed_by_repo_matrix",
          claim_scope: "explicitly unsupported or out-of-scope topology; must not be reported as pass"
        }
      },
      path_expectation_values: (map(.path_expectation) | unique),
      expected_route_values: (map(.expected_route) | unique),
      degradation_class_values: (map(.degradation_class) | unique),
      supported_status_values: (map(.supported_status) | unique),
      claim_boundaries: (map(.claim_boundary) | unique),
      physical_nat_truth_requires: [
        "coverage=manual_lab",
        "dedicated_lab_evidence_ref_or_real_env_ref",
        "producer_qa_pass_uplift_decision_ref_for_release_claims"
      ],
      proxy_claim_guard: (
        ((map(select(.coverage == "proxy" and (.claim_boundary | contains("proxy_not_")))) | length)
          == (map(select(.coverage == "proxy")) | length))
      )
    },
    external_evidence: {
      shared_window_evidence_refs: $shared_window_refs[0],
      dedicated_lab_evidence_refs: $dedicated_lab_refs[0],
      pass_uplift_decision_ref: (
        if $pass_uplift_decision_ref == "" then
          null
        else
          $pass_uplift_decision_ref
        end
      )
    },
    evidence_contract: {
      executable_boundary: {
        required_exact_ready: (
          ($dry_run == 0)
          and ((map(select(.coverage == "exact")) | length) > 0)
          and ((map(select(.coverage == "exact" and .status == "ok")) | length) == (map(select(.coverage == "exact")) | length))
        ),
        full_proxy_ready: (
          ($tier == "full")
          and ($dry_run == 0)
          and ((map(select(.coverage == "proxy")) | length) > 0)
          and ((map(select(.coverage == "proxy" and .status == "ok")) | length) == (map(select(.coverage == "proxy")) | length))
        ),
        stronger_full_tier_truth_ready: (
          ($tier == "full")
          and ($dry_run == 0)
          and ($dedicated_lab_refs_ready == 1)
          and ((map(select(.status == "failed")) | length) == 0)
          and ((map(select(.coverage == "proxy")) | length) > 0)
          and ((map(select(.coverage == "proxy" and .status == "ok")) | length) == (map(select(.coverage == "proxy")) | length))
        )
      },
      claim_readiness: {
        mixed_topology_full_tier_status: (
          if $tier == "required" then
            if $dry_run == 1 then
              "required_plan"
            elif any(.[]; .status == "failed") then
              "required_failed"
            else
              "required_exact_executed"
            end
          else
            if $dry_run == 1 then
              "full_proxy_plan"
            elif any(.[]; .status == "failed") then
              "full_failed"
            elif ((map(select(.coverage == "proxy" and .status == "ok")) | length) == (map(select(.coverage == "proxy")) | length)) then
              if (($dedicated_lab_refs[0] | length) > 0 and $dedicated_lab_refs_ready == 1) then
                "full_proxy_executed_plus_dedicated_refs"
              else
                "full_proxy_executed"
              end
            else
              "required_exact_executed"
            end
          end
        ),
        shared_network_pass_inputs_ready: (
          ($tier == "full")
          and ($dry_run == 0)
          and ((map(select(.coverage == "proxy")) | length) > 0)
          and ((map(select(.coverage == "proxy" and .status == "ok")) | length) == (map(select(.coverage == "proxy")) | length))
          and (($shared_window_refs[0] | length) > 0)
          and ($pass_uplift_decision_ref != "")
        ),
        stronger_full_tier_truth_blockers: (
          []
          + (if $tier != "full" then ["run_full_tier_proxy_execution"] else [] end)
          + (if $dry_run == 1 then ["execute_full_tier_live_run"] else [] end)
          + (if any(.[]; .status == "failed") then ["fix_failed_matrix_cases"] else [] end)
          + (if (($dedicated_lab_refs[0] | length) == 0) then ["dedicated_sentry_or_nat_lab_evidence_ref"] else [] end)
          + (if (($dedicated_lab_refs[0] | length) > 0 and $dedicated_lab_refs_ready != 1) then ["dedicated_sentry_or_nat_lab_evidence_ref_must_exist"] else [] end)
        ),
        shared_network_pass_blockers: (
          []
          + (if $tier != "full" then ["run_full_tier_proxy_execution"] else [] end)
          + (if $dry_run == 1 then ["execute_full_tier_live_run"] else [] end)
          + (if any(.[]; .status == "failed") then ["fix_failed_matrix_cases"] else [] end)
          + (if (($shared_window_refs[0] | length) == 0) then ["same_window_shared_network_evidence_ref"] else [] end)
          + (if $pass_uplift_decision_ref == "" then ["producer_qa_pass_uplift_decision_ref"] else [] end)
        )
      }
    },
    overall_status: (
      if $dry_run == 1 then
        "dry_run"
      elif any(.[]; .status == "failed") then
        "failed"
      else
        "ok"
      end
    )
  }' "$case_records" > "$summary_json"

{
  echo "# P2P Mixed Topology Validation Matrix"
  echo
  echo "- generated_at: \`$generated_at\`"
  echo "- tier: \`$tier\`"
  echo "- dry_run: \`$dry_run\`"
  echo "- run_dir: \`$run_dir\`"
  echo "- overall_status: \`$(jq -r '.overall_status' "$summary_json")\`"
  echo
  echo "| case | scenario | reachability_pair | degradation_class | path_expectation | expected_route | coverage | supported_status | claim_boundary | status |"
  echo "| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |"
  while IFS=$'\t' read -r case_id scenario reachability_pair degradation_class path_expectation expected_route coverage supported_status claim_boundary status; do
    echo "| $case_id | $scenario | $reachability_pair | $degradation_class | $path_expectation | $expected_route | $coverage | $supported_status | $claim_boundary | $status |"
  done < <(jq -r '.cases[] | [ .case_id, .scenario, .reachability_pair, .degradation_class, .path_expectation, .expected_route, .coverage, .supported_status, .claim_boundary, .status ] | @tsv' "$summary_json")
  echo
  echo "## Coverage Notes"
  echo "- \`exact\`: deterministic cargo tests that directly cover the current substrate contracts."
  echo "- \`proxy\`: executable longrun drills that approximate mixed-topology recovery; they are not physical NAT/CGNAT or dedicated sentry lab truth."
  echo "- \`manual_lab\`: external dedicated-lab or real-env evidence refs required for physical NAT/CGNAT/public-internet claims."
  echo "- \`unsupported\`: explicitly out-of-scope topology claims that the matrix must not report as pass."
  echo "- \`proxy_claim_guard\`: \`$(jq -r '.path_behavior_taxonomy.proxy_claim_guard' "$summary_json")\`"
  echo
  echo "## Evidence Contract"
  echo "- \`mixed_topology_full_tier_status\`: \`$(jq -r '.evidence_contract.claim_readiness.mixed_topology_full_tier_status' "$summary_json")\`"
  echo "- \`required_exact_ready\`: \`$(jq -r '.evidence_contract.executable_boundary.required_exact_ready' "$summary_json")\`"
  echo "- \`full_proxy_ready\`: \`$(jq -r '.evidence_contract.executable_boundary.full_proxy_ready' "$summary_json")\`"
  echo "- \`stronger_full_tier_truth_ready\`: \`$(jq -r '.evidence_contract.executable_boundary.stronger_full_tier_truth_ready' "$summary_json")\`"
  echo "- \`shared_network_pass_inputs_ready\`: \`$(jq -r '.evidence_contract.claim_readiness.shared_network_pass_inputs_ready' "$summary_json")\`"
  echo "- \`stronger_full_tier_truth_blockers\`: \`$(jq -r '.evidence_contract.claim_readiness.stronger_full_tier_truth_blockers | if length == 0 then "(none)" else join(", ") end' "$summary_json")\`"
  echo "- \`shared_network_pass_blockers\`: \`$(jq -r '.evidence_contract.claim_readiness.shared_network_pass_blockers | if length == 0 then "(none)" else join(", ") end' "$summary_json")\`"
  echo
  echo "## External Evidence Refs"
  echo "- \`shared_window_evidence_refs\`: \`$(jq -r '.external_evidence.shared_window_evidence_refs | if length == 0 then "(none)" else join(", ") end' "$summary_json")\`"
  echo "- \`dedicated_lab_evidence_refs\`: \`$(jq -r '.external_evidence.dedicated_lab_evidence_refs | if length == 0 then "(none)" else join(", ") end' "$summary_json")\`"
  echo "- \`pass_uplift_decision_ref\`: \`$(jq -r '.external_evidence.pass_uplift_decision_ref // "(none)"' "$summary_json")\`"
} > "$summary_md"

echo "matrix summary:"
echo "  summary_json: $summary_json"
echo "  summary_md: $summary_md"

if [[ "$dry_run" -eq 1 ]]; then
  exit 0
fi

if [[ "$(jq -r '.overall_status' "$summary_json")" != "ok" ]]; then
  exit 1
fi
