#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

tmp_root="$(mktemp -d)"
trap 'rm -rf "$tmp_root"' EXIT

bin_dir="$tmp_root/bin"
mkdir -p "$bin_dir"

cat >"$bin_dir/systemctl" <<'SH'
#!/usr/bin/env bash
if [[ "${1:-}" == "is-active" ]]; then
  if [[ " $* " == *" oasis7-inactive.service "* ]]; then
    echo inactive
    exit 3
  fi
  echo active
  exit 0
fi
exit 1
SH

cat >"$bin_dir/ssh" <<'SH'
#!/usr/bin/env bash
echo "forced ssh failure for fallback test" >&2
exit 255
SH

cat >"$bin_dir/sshpass" <<'SH'
#!/usr/bin/env bash
if [[ "${1:-}" == "-e" ]]; then
  shift
fi
exec "$@"
SH

cat >"$bin_dir/curl" <<'SH'
#!/usr/bin/env bash
if [[ " $* " != *" --max-time "* ]]; then
  echo "curl missing --max-time: $*" >&2
  exit 2
fi
url="${@: -1}"
case "$url" in
  *local/healthz|*sequencer/healthz|*storage/healthz)
    printf '{"ok":true}\n'
    ;;
  *local/status)
    cat <<'JSON'
{"node_id":"local","world_id":"fallback-test","role":"sequencer","running":true,"liveness":{"status":"ok","running":true},"readiness":{"status":"ready","ready":true,"policy":{"tier":"public_testnet","role":"sequencer","quorum_mode":"count","relay_policy":"public_direct_or_relay","slashing_policy":"disabled_for_non_mainnet","slashing_enforced":false}},"sync":{"status":"ok","network_head_source":"local","network_height_lag":0,"fresh_peer_count":1,"stale_peer_count":0,"conflicting_peer_count":0},"observability":{"status":"ok","ready":true,"network_head_available":true,"transport_stable":true,"reachability_policy_ok":true},"consensus":{"committed_height":10,"network_committed_height":10,"known_peer_heads":1,"network_head":{"source":"local","decision":"accepted","required_peer_count":1,"quorum_mode":"count","fresh_peer_count":1,"stale_peer_count":0,"conflicting_peer_count":0,"observed_stake":100,"required_stake":100,"total_stake":300,"stake_quorum_met":true}}}
JSON
    ;;
  *sequencer/status)
    cat <<'JSON'
{"node_id":"sequencer","world_id":"fallback-test","role":"sequencer","running":true,"liveness":{"status":"ok","running":true},"readiness":{"status":"ready","ready":true,"policy":{"tier":"public_testnet","role":"sequencer","quorum_mode":"count","relay_policy":"public_direct_or_relay","slashing_policy":"disabled_for_non_mainnet","slashing_enforced":false}},"sync":{"status":"ok","network_head_source":"peer","network_height_lag":0,"fresh_peer_count":1,"stale_peer_count":0,"conflicting_peer_count":0},"observability":{"status":"ok","ready":true,"network_head_available":true,"transport_stable":true,"reachability_policy_ok":true},"consensus":{"committed_height":20,"network_committed_height":20,"known_peer_heads":1,"network_head":{"source":"peer","decision":"accepted","required_peer_count":1,"quorum_mode":"count","fresh_peer_count":1,"stale_peer_count":0,"conflicting_peer_count":0,"observed_stake":100,"required_stake":100,"total_stake":300,"stake_quorum_met":true}}}
JSON
    ;;
  *storage/status)
    cat <<'JSON'
{"node_id":"storage","world_id":"fallback-test","role":"sequencer","running":true,"liveness":{"status":"ok","running":true},"readiness":{"status":"ready","ready":true,"policy":{"tier":"public_testnet","role":"sequencer","quorum_mode":"count","relay_policy":"public_direct_or_relay","slashing_policy":"disabled_for_non_mainnet","slashing_enforced":false}},"sync":{"status":"ok","network_head_source":"peer","network_height_lag":0,"fresh_peer_count":1,"stale_peer_count":0,"conflicting_peer_count":0},"observability":{"status":"ok","ready":true,"network_head_available":true,"transport_stable":true,"reachability_policy_ok":true},"consensus":{"committed_height":30,"network_committed_height":30,"known_peer_heads":1,"network_head":{"source":"peer","decision":"accepted","required_peer_count":1,"quorum_mode":"count","fresh_peer_count":1,"stale_peer_count":0,"conflicting_peer_count":0,"observed_stake":100,"required_stake":100,"total_stake":300,"stake_quorum_met":true}}}
JSON
    ;;
  *)
    echo "unexpected curl url: $url" >&2
    exit 22
    ;;
esac
SH

chmod +x "$bin_dir/systemctl" "$bin_dir/ssh" "$bin_dir/sshpass" "$bin_dir/curl"

PATH="$bin_dir:$PATH" ./scripts/p2p-real-env-triad-snapshot.sh \
  --samples 1 \
  --interval-secs 1 \
  --ssh-timeout-secs 1 \
  --out-dir "$tmp_root/out" \
  --world-id fallback-test \
  --local-status-url http://local/status \
  --local-health-url http://local/healthz \
  --local-env-file "$tmp_root/missing-local.env" \
  --sequencer-status-url http://127.0.0.1:1/status \
  --sequencer-health-url http://127.0.0.1:1/healthz \
  --sequencer-public-status-url http://sequencer/status \
  --sequencer-public-health-url http://sequencer/healthz \
  --storage-status-url http://127.0.0.1:2/status \
  --storage-health-url http://127.0.0.1:2/healthz \
  --storage-public-status-url http://storage/status \
  --storage-public-health-url http://storage/healthz

run_dir="$tmp_root/out/$(jq -r '.latest_attempt.run_directory' "$tmp_root/out/attempt_state.json")"
summary_json="$run_dir/summary.json"

jq -e '
  .nodes.sequencer_ecs.status_fetch_all_ok == true
  and .nodes.storage_ecs.status_fetch_all_ok == true
  and .nodes.sequencer_ecs.heights.max_committed_height == 20
  and .nodes.storage_ecs.heights.max_committed_height == 30
  and .nodes.sequencer_ecs.node_ids == ["sequencer"]
  and .nodes.storage_ecs.node_ids == ["storage"]
' "$summary_json" >/dev/null

test -f "$run_dir/nodes/sequencer_ecs/samples/sample-001/status.fallback.txt"
test -f "$run_dir/nodes/storage_ecs/samples/sample-001/status.fallback.txt"

PATH="$bin_dir:$PATH" ./scripts/p2p-real-env-triad-snapshot.sh \
  --samples 1 \
  --interval-secs 1 \
  --ssh-timeout-secs 1 \
  --out-dir "$tmp_root/out-explicit" \
  --world-id fallback-test \
  --node "label=local_node,mode=local,service=oasis7-triad-observer.service,status_url=http://local/status,health_url=http://local/healthz,env_file=$tmp_root/missing-local.env" \
  --node "label=sequencer_ecs,mode=remote,target=root@127.0.0.1,service=oasis7-triad-sequencer.service,status_url=http://127.0.0.1:1/status,health_url=http://127.0.0.1:1/healthz,public_status_url=http://sequencer/status,public_health_url=http://sequencer/healthz,password_env=P2PARCH6_SEQ_SSH_PASSWORD,env_file=/tmp/sequencer.env" \
  --node "label=storage_ecs,mode=remote,target=root@127.0.0.2,service=oasis7-triad-storage.service,status_url=http://127.0.0.1:2/status,health_url=http://127.0.0.1:2/healthz,public_status_url=http://storage/status,public_health_url=http://storage/healthz,password_env=P2PARCH6_STORAGE_SSH_PASSWORD,env_file=/tmp/storage.env" \
  --node "label=extra_alpha,mode=local,service=oasis7-extra-alpha.service,status_url=http://local/status,health_url=http://local/healthz,env_file=$tmp_root/missing-alpha.env" \
  --node "label=extra_beta,mode=remote,target=root@127.0.0.3,service=oasis7-extra-beta.service,status_url=http://127.0.0.1:3/status,health_url=http://127.0.0.1:3/healthz,public_status_url=http://storage/status,public_health_url=http://storage/healthz,env_file=/tmp/beta.env"

explicit_run_dir="$tmp_root/out-explicit/$(jq -r '.latest_attempt.run_directory' "$tmp_root/out-explicit/attempt_state.json")"
explicit_summary_json="$explicit_run_dir/summary.json"

jq -e '
	  .totals.node_count == 5
	  and .analysis.claim_mode == "explicit_nodes"
	  and ((.failure_signatures | index("local_service_unhealthy")) | not)
	  and .nodes.extra_alpha.status_fetch_all_ok == true
	  and .nodes.extra_beta.status_fetch_all_ok == true
  and .nodes.extra_alpha.node_ids == ["local"]
  and .nodes.extra_beta.node_ids == ["storage"]
' "$explicit_summary_json" >/dev/null

grep -q '### `extra_alpha`' "$explicit_run_dir/summary.md"
grep -q '### `extra_beta`' "$explicit_run_dir/summary.md"
grep -q 'claim_mode: `explicit_nodes`' "$explicit_run_dir/summary.md"
grep -q 'storage_challenge_network_degraded_any' "$explicit_run_dir/summary.md"

PATH="$bin_dir:$PATH" ./scripts/p2p-real-env-triad-snapshot.sh \
  --samples 1 \
  --interval-secs 1 \
  --ssh-timeout-secs 1 \
  --out-dir "$tmp_root/out-single-explicit" \
  --world-id fallback-test \
  --node "label=custom_only,mode=local,service=oasis7-inactive.service,status_url=http://local/status,health_url=http://local/healthz,env_file=$tmp_root/missing-custom.env"

single_explicit_run_dir="$tmp_root/out-single-explicit/$(jq -r '.latest_attempt.run_directory' "$tmp_root/out-single-explicit/attempt_state.json")"
single_explicit_summary_json="$single_explicit_run_dir/summary.json"

jq -e '
  .totals.node_count == 1
  and .analysis.claim_mode == "explicit_nodes"
  and .nodes.custom_only.service_active == false
  and (.failure_signatures | index("node_service_inactive")) != null
  and .claim_status != "pass_candidate"
' "$single_explicit_summary_json" >/dev/null

if PATH="$bin_dir:$PATH" ./scripts/p2p-real-env-triad-snapshot.sh \
  --samples 1 \
  --interval-secs 1 \
  --ssh-timeout-secs 1 \
  --out-dir "$tmp_root/out-duplicate" \
  --world-id fallback-test \
  --node "label=dup_node,mode=local,service=oasis7-one.service,status_url=http://local/status,health_url=http://local/healthz,env_file=$tmp_root/missing-one.env" \
  --node "label=dup_node,mode=local,service=oasis7-two.service,status_url=http://local/status,health_url=http://local/healthz,env_file=$tmp_root/missing-two.env" \
  >"$tmp_root/duplicate.out" 2>"$tmp_root/duplicate.err"; then
  echo "duplicate --node labels should fail" >&2
  exit 1
fi
grep -q 'duplicate --node label' "$tmp_root/duplicate.err"

python3 ./scripts/p2p-real-env-snapshot-durable.test.py
