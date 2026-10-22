#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

run_test() {
  env -u RUSTC_WRAPPER cargo test -p oasis7 --lib "$1" -- --nocapture
}

env -u RUSTC_WRAPPER cargo test -p oasis7_proto viewer_v2_hello_ack_advertises_signed_rollback_capability -- --nocapture
env -u RUSTC_WRAPPER cargo test -p oasis7_protocol_tools --bin oasis7_rollback_strict_audit_evidence -- --nocapture
run_test runtime_authoritative_recovery_requires_v2_negotiated_signed_rollback_capability
run_test rollback_receipt_freezes_full_signed_identity_and_evidence
run_test runtime_authoritative_recovery_receipt_is_immutable_after_later_progress
run_test complete_receipt_requires_governance_auth_and_replay_stays_rejected_after_restart
run_test rollback_nonce_is_durable_and_cannot_be_replayed_through_an_old_snapshot
run_test authoritative_recovery_generation_contains_restartable_viewer_state
run_test recovery_generation_round_trip_preserves_revoked_session_policy_and_metadata

echo "governed rollback v2 drill: PASS"
