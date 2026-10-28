# Validator Finality Proof Evidence V1 Template

Purpose: record one bounded `WorldFinalityProofV1` external verifier sample for
validator-set hash, stake threshold, finality-vote target binding,
fork/misbehavior evidence, live Ed25519 finality vote verification, and bounded
validator-set transition execution plus governance-certificate semantics. This
is optional, non-promotional evidence.

It does not claim full light-client security, public validator onboarding,
multi-client equivalence, or mainnet-grade finality.

| Field | Expected value |
| --- | --- |
| evidence_schema | `oasis7.validator_finality_proof.v1` |
| status | `pass` |
| verifier_mode | `validator_set_finality` |
| independent_process | `true` |
| external_verifier.schema_version | `oasis7.world_finality_proof_verifier.v1` |
| external_verifier.proof_contract | `WorldFinalityProofV1` |
| external_verifier.claim_boundary | `validator_set_finality_evidence_only_not_full_light_client_or_mainnet_readiness` |

```json
{
  "evidence_schema": "oasis7.validator_finality_proof.v1",
  "status": "pass",
  "verifier_mode": "validator_set_finality",
  "independent_process": true,
  "implementation_ref": "crates/oasis7_proto/src/bin/oasis7_world_head_proof_verify.rs",
  "command_ref": "cargo run -p oasis7_proto --bin oasis7_world_head_proof_verify -- --finality-proof proof.json --format json --expect-world-id world-a --expect-height 42 --json",
  "network_tier": {
    "tier": "public_testnet",
    "status": "rehearsal",
    "chain_id": "oasis7-public-testnet-example",
    "network_id": "oasis7-public-testnet-example",
    "world_id": "world-a"
  },
  "manifest_ref": "doc/testing/templates/network-tier-public-testnet.example.json",
  "genesis_ref": "doc/testing/templates/public-testnet-genesis.example.json",
  "bootstrap_peer_ref": "doc/testing/templates/public-testnet-bootstrap.example.txt",
  "rpc_ref": "https://public-testnet.example.invalid/rpc",
  "status_endpoint_ref": "",
  "finality_proof_ref": "proof.json",
  "finality_proof_hash": "replace-with-verifier-proof-hash",
  "sample_window": {
    "started_at": "2026-07-03T00:00:00Z",
    "ended_at": "2026-07-03T00:01:00Z"
  },
  "trusted_anchor": {
    "height": 39,
    "block_hash": "replace-with-anchor-block-hash"
  },
  "observed_head": {
    "height": 42,
    "hash": "replace-with-observed-head-hash",
    "state_root": "replace-with-observed-state-root"
  },
  "verified_range": {
    "from_height": 40,
    "to_height": 42
  },
  "validator_set": {
    "validator_set_id": "replace-with-set-id",
    "activation_height": 1,
    "validator_set_hash": "replace-with-validator-set-hash",
    "validator_count": 3,
    "quorum_threshold_bps": 6667
  },
  "finality_sample": {
    "commitment_count": 3,
    "vote_count": 6,
    "misbehavior_evidence_count": 1,
    "stake_threshold_checked": true,
    "validator_set_hash_checked": true,
    "consensus_approver_subset_checked": true,
    "ed25519_signature_verification_checked": true,
    "validator_set_transition_execution_checked": true,
    "validator_set_transition_governance_checked": true,
    "trusted_governance_anchor_checked": false,
    "governance_set_hash": "",
    "governance_certificate_count": 0,
    "validator_set_transition_count": 0
  },
  "transition_sample": {
    "transition_result": "no_transition_in_sample",
    "transition_count": 0,
    "reason": "bounded transition governance certificate, execution, and finality semantics are verified when present"
  },
  "fork_or_reorg_cases": [
    "conflicting_head_rejected_or_recorded",
    "unknown_signer_rejected",
    "insufficient_signed_stake_rejected"
  ],
  "misbehavior_result": "evidence_recorded",
  "external_verifier": {
    "schema_version": "oasis7.world_finality_proof_verifier.v1",
    "status": "pass",
    "verifier_mode": "validator_set_finality",
    "proof_contract": "WorldFinalityProofV1",
    "hash_domain": "oasis7.world_finality_proof.v2",
    "claim_boundary": "validator_set_finality_evidence_only_not_full_light_client_or_mainnet_readiness",
    "proof_ref": "proof.json",
    "proof_hash": "replace-with-verifier-proof-hash",
    "world_id": "world-a",
    "from_height": 40,
    "to_height": 42,
    "validator_set": {
      "validator_set_hash": "replace-with-validator-set-hash"
    },
    "finality": {
      "commitment_count": 3,
      "vote_count": 6,
      "stake_threshold_checked": true,
      "validator_set_hash_checked": true,
      "consensus_approver_subset_checked": true,
      "ed25519_signature_verification_checked": true,
      "validator_set_transition_execution_checked": true,
      "validator_set_transition_governance_checked": true,
      "trusted_governance_anchor_checked": false,
      "governance_set_hash": "",
      "governance_certificate_count": 0,
      "validator_set_transition_count": 0
    },
    "head": {
      "height": 42,
      "block_hash": "replace-with-observed-head-hash",
      "state_root": "replace-with-observed-state-root"
    },
    "does_not_claim": [
      "mainnet-grade finality",
      "full light client",
      "DA sampling",
      "multi-client consensus equivalence",
      "public validator onboarding open"
    ]
  },
  "node_db_access_used": false,
  "manual_checkpoint_or_data_copy_used": false,
  "privileged_internal_api_used": false,
  "does_not_claim": [
    "full light client security",
    "mainnet-grade finality",
    "public validator onboarding open",
    "permissionless validator onboarding",
    "DA sampling",
    "multi-client consensus equivalence",
    "ready_for_live_candidate",
    "live public launch"
  ],
  "residual_risk": [
    "same implementation family verifier; no independent client parity",
    "bounded transition governance remains same-family verifier evidence, not full light-client or multi-client equivalence"
  ]
}
```
