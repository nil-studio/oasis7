use std::fs;
use std::path::Path;

use ed25519_dalek::{Signer, SigningKey};
use oasis7_proto::distributed::{
    BlobRef, CheckpointClosureEvidenceV1, ExecutionBindingEvidenceV1, HeadConsensusEvidenceV1,
    WIRE_ENCODING_CBOR, WORLD_FINALITY_PROOF_CLAIM_BOUNDARY_V1,
    WORLD_FINALITY_PROOF_HASH_DOMAIN_V1, WORLD_FINALITY_PROOF_V1_SCHEMA,
    WORLD_HEAD_PROOF_CLAIM_BOUNDARY_V1, WORLD_HEAD_PROOF_V1_SCHEMA, WorldBlock,
    WorldFinalityCommitmentV1, WorldFinalityMisbehaviorEvidenceV1, WorldFinalityProofV1,
    WorldFinalityValidatorV1, WorldFinalityVoteV1, WorldHeadAnnounce, WorldHeadProofV1,
    compute_world_finality_validator_set_hash, world_finality_vote_signing_payload,
};
#[cfg(test)]
use oasis7_proto::distributed::{
    WorldFinalityGovernanceSignerV1, WorldFinalityValidatorSetTransitionApprovalV1,
    WorldFinalityValidatorSetTransitionGovernanceApprovalV1,
    WorldFinalityValidatorSetTransitionGovernanceCertificateV1,
    WorldFinalityValidatorSetTransitionV1, compute_world_finality_governance_set_hash,
    world_finality_validator_set_transition_governance_signing_payload,
    world_finality_validator_set_transition_signing_payload,
};
use serde::Serialize;
use serde_json::json;

fn canonical_blake3_hex<T: Serialize>(value: &T) -> String {
    let payload = serde_cbor::to_vec(value).expect("encode canonical cbor");
    blake3::hash(payload.as_slice()).to_hex().to_string()
}

fn sample_validator_keys() -> Vec<(&'static str, SigningKey)> {
    vec![
        ("validator-a", SigningKey::from_bytes(&[1_u8; 32])),
        ("validator-b", SigningKey::from_bytes(&[2_u8; 32])),
        ("validator-c", SigningKey::from_bytes(&[3_u8; 32])),
    ]
}

#[cfg(test)]
fn sample_governance_keys() -> Vec<(&'static str, SigningKey)> {
    vec![
        ("governance-a", SigningKey::from_bytes(&[11_u8; 32])),
        ("governance-b", SigningKey::from_bytes(&[12_u8; 32])),
        ("governance-c", SigningKey::from_bytes(&[13_u8; 32])),
    ]
}

fn sample_public_key_hex(keys: &[(&'static str, SigningKey)], validator_id: &str) -> String {
    let key = keys
        .iter()
        .find(|(id, _)| *id == validator_id)
        .map(|(_, key)| key)
        .expect("sample validator key");
    hex::encode(key.verifying_key().to_bytes())
}

#[cfg(test)]
fn sample_governance_public_key_hex(
    keys: &[(&'static str, SigningKey)],
    signer_id: &str,
) -> String {
    let key = keys
        .iter()
        .find(|(id, _)| *id == signer_id)
        .map(|(_, key)| key)
        .expect("sample governance key");
    hex::encode(key.verifying_key().to_bytes())
}

fn sample_signed_finality_vote(
    keys: &[(&'static str, SigningKey)],
    validator_id: &str,
    commitment: &WorldFinalityCommitmentV1,
) -> WorldFinalityVoteV1 {
    let key = keys
        .iter()
        .find(|(id, _)| *id == validator_id)
        .map(|(_, key)| key)
        .expect("sample validator key");
    let payload = world_finality_vote_signing_payload(
        validator_id,
        commitment.height,
        commitment.block_hash.as_str(),
        commitment.state_root.as_str(),
        commitment.validator_set_hash.as_str(),
        commitment.round_id.as_str(),
    )
    .expect("sample finality vote payload");
    WorldFinalityVoteV1 {
        validator_id: validator_id.to_string(),
        height: commitment.height,
        block_hash: commitment.block_hash.clone(),
        state_root: commitment.state_root.clone(),
        signature_scheme: "ed25519".to_string(),
        signature_evidence_hash: canonical_blake3_hex(&payload),
        signature_hex: hex::encode(key.sign(payload.as_slice()).to_bytes()),
    }
}

pub(crate) fn verify_finality_proof_path(
    path: &Path,
    format: &str,
    expect_hash: Option<&str>,
    expect_world_id: Option<&str>,
    expect_height: Option<u64>,
    expect_governance_set_hash: Option<&str>,
    proof_ref: Option<String>,
) -> Result<serde_json::Value, String> {
    let bytes =
        fs::read(path).map_err(|err| format!("read finality proof {}: {err}", path.display()))?;
    let proof: WorldFinalityProofV1 = match format {
        "cbor" => serde_cbor::from_slice(bytes.as_slice())
            .map_err(|err| format!("decode WorldFinalityProofV1 cbor: {err}"))?,
        "json" => serde_json::from_slice(bytes.as_slice())
            .map_err(|err| format!("decode WorldFinalityProofV1 json: {err}"))?,
        raw => return Err(format!("unsupported finality proof format: {raw}")),
    };
    proof.validate_contract()?;
    let proof_hash = proof.proof_hash()?;
    if let Some(expected) = expect_hash
        && proof_hash != expected
    {
        return Err(format!(
            "finality proof hash mismatch: expected={expected} actual={proof_hash}"
        ));
    }
    if let Some(expected) = expect_world_id
        && proof.world_id != expected
    {
        return Err(format!(
            "finality proof world_id mismatch: expected={expected} actual={}",
            proof.world_id
        ));
    }
    if let Some(expected) = expect_height
        && proof.to_height != expected
    {
        return Err(format!(
            "finality proof to_height mismatch: expected={expected} actual={}",
            proof.to_height
        ));
    }
    let transition_governance_hashes = proof
        .validator_set_transitions
        .iter()
        .map(|transition| {
            transition
                .governance_certificate
                .as_ref()
                .map(|certificate| certificate.governance_set_hash.clone())
                .ok_or_else(|| {
                    "finality proof transition missing governance certificate after validation"
                        .to_string()
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let transition_governance_anchor_checked = if transition_governance_hashes.is_empty() {
        false
    } else {
        let expected = expect_governance_set_hash.ok_or_else(|| {
            "finality proof transition requires --expect-governance-set-hash trusted anchor"
                .to_string()
        })?;
        for governance_set_hash in &transition_governance_hashes {
            if governance_set_hash != expected {
                return Err(format!(
                    "finality proof governance set hash mismatch: expected={expected} actual={governance_set_hash}"
                ));
            }
        }
        true
    };
    let head = proof
        .head_proofs
        .last()
        .ok_or_else(|| "finality proof head_proofs empty after validation".to_string())?;
    let total_commitments = proof.finality_commitments.len();
    let total_votes = proof
        .finality_commitments
        .iter()
        .map(|commitment| commitment.votes.len())
        .sum::<usize>();
    Ok(json!({
        "schema_version": "oasis7.world_finality_proof_verifier.v1",
        "status": "pass",
        "verifier_mode": "validator_set_finality",
        "proof_contract": "WorldFinalityProofV1",
        "hash_domain": WORLD_FINALITY_PROOF_HASH_DOMAIN_V1,
        "claim_boundary": WORLD_FINALITY_PROOF_CLAIM_BOUNDARY_V1,
        "proof_ref": proof_ref.unwrap_or_default(),
        "proof_hash": proof_hash,
        "world_id": proof.world_id,
        "from_height": proof.from_height,
        "to_height": proof.to_height,
        "trusted_anchor": {
            "height": proof.trusted_anchor_height,
            "block_hash": proof.trusted_anchor_block_hash
        },
        "validator_set": {
            "validator_set_id": proof.validator_set_id,
            "activation_height": proof.validator_set_activation_height,
            "validator_set_hash": proof.validator_set_hash,
            "validator_count": proof.validators.len(),
            "quorum_threshold_bps": proof.quorum_threshold_bps
        },
        "finality": {
            "commitment_count": total_commitments,
            "vote_count": total_votes,
            "misbehavior_evidence_count": proof.misbehavior_evidence.len(),
            "stake_threshold_checked": true,
            "validator_set_hash_checked": true,
            "consensus_approver_subset_checked": true,
            "ed25519_signature_verification_checked": true,
            "validator_set_transition_execution_checked": true,
            "validator_set_transition_governance_checked": true,
            "trusted_governance_anchor_checked": transition_governance_anchor_checked,
            "governance_set_hash": transition_governance_hashes.last().cloned().unwrap_or_default(),
            "governance_certificate_count": transition_governance_hashes.len(),
            "validator_set_transition_count": proof.validator_set_transitions.len()
        },
        "head": {
            "height": head.height,
            "block_hash": head.head.block_hash,
            "state_root": head.head.state_root
        },
        "does_not_claim": [
            "mainnet-grade finality",
            "full light client",
            "DA sampling",
            "multi-client consensus equivalence",
            "public validator onboarding open"
        ]
    }))
}

pub(crate) fn write_sample_finality_json(path: &Path) -> Result<serde_json::Value, String> {
    let proof = sample_world_finality_proof();
    fs::write(
        path,
        serde_json::to_vec_pretty(&proof)
            .map_err(|err| format!("encode sample finality proof json: {err}"))?,
    )
    .map_err(|err| format!("write sample finality proof {}: {err}", path.display()))?;
    Ok(json!({
        "schema_version": "oasis7.world_finality_proof_verifier_fixture.v1",
        "status": "sample_finality_written",
        "proof_path": path,
        "proof_hash": proof.proof_hash()?,
        "world_id": proof.world_id,
        "from_height": proof.from_height,
        "to_height": proof.to_height
    }))
}

pub(crate) fn sample_world_finality_proof() -> WorldFinalityProofV1 {
    let first = sample_world_head_proof_at(40, "prev-block-39");
    let second = sample_world_head_proof_at(41, first.head.block_hash.as_str());
    let third = sample_world_head_proof_at(42, second.head.block_hash.as_str());
    let keys = sample_validator_keys();
    let validators = vec![
        WorldFinalityValidatorV1 {
            validator_id: "validator-a".to_string(),
            stake_weight: 34,
            finality_signer_public_key: sample_public_key_hex(&keys, "validator-a"),
            activation_height: 1,
            exit_height: None,
        },
        WorldFinalityValidatorV1 {
            validator_id: "validator-b".to_string(),
            stake_weight: 33,
            finality_signer_public_key: sample_public_key_hex(&keys, "validator-b"),
            activation_height: 1,
            exit_height: None,
        },
        WorldFinalityValidatorV1 {
            validator_id: "validator-c".to_string(),
            stake_weight: 33,
            finality_signer_public_key: sample_public_key_hex(&keys, "validator-c"),
            activation_height: 1,
            exit_height: None,
        },
    ];
    let validator_set_hash =
        compute_world_finality_validator_set_hash("sample-set-1", 1, 6_667, &validators)
            .expect("validator set hash");
    let head_proofs = vec![first, second, third];
    let finality_commitments = head_proofs
        .iter()
        .map(|proof| {
            let mut commitment = WorldFinalityCommitmentV1 {
                height: proof.height,
                round_id: format!("round-{}", proof.height),
                block_hash: proof.head.block_hash.clone(),
                state_root: proof.head.state_root.clone(),
                validator_set_hash: validator_set_hash.clone(),
                quorum_threshold_bps: 6_667,
                votes: vec![],
            };
            commitment.votes = vec![
                sample_signed_finality_vote(&keys, "validator-a", &commitment),
                sample_signed_finality_vote(&keys, "validator-b", &commitment),
            ];
            commitment
        })
        .collect();
    WorldFinalityProofV1 {
        schema_version: WORLD_FINALITY_PROOF_V1_SCHEMA,
        world_id: "world-a".to_string(),
        from_height: 40,
        to_height: 42,
        trusted_anchor_height: 39,
        trusted_anchor_block_hash: "prev-block-39".to_string(),
        validator_set_id: "sample-set-1".to_string(),
        validator_set_activation_height: 1,
        validator_set_hash,
        validators,
        quorum_threshold_bps: 6_667,
        head_proofs,
        finality_commitments,
        validator_set_transitions: vec![],
        misbehavior_evidence: vec![WorldFinalityMisbehaviorEvidenceV1 {
            height: 42,
            validator_id: "validator-c".to_string(),
            evidence_kind: "conflicting_head".to_string(),
            conflicting_block_hash: "conflicting-block-42".to_string(),
            conflicting_proof_hash: "conflicting-proof-42".to_string(),
            disposition: "rejected".to_string(),
        }],
        claim_boundary: WORLD_FINALITY_PROOF_CLAIM_BOUNDARY_V1.to_string(),
    }
}

#[cfg(test)]
pub(crate) fn sample_world_finality_transition_proof() -> WorldFinalityProofV1 {
    let mut proof = sample_world_finality_proof();
    let validator_keys = sample_validator_keys();
    let to_validators = vec![
        WorldFinalityValidatorV1 {
            validator_id: "validator-a".to_string(),
            stake_weight: 34,
            finality_signer_public_key: sample_public_key_hex(&validator_keys, "validator-a"),
            activation_height: 42,
            exit_height: None,
        },
        WorldFinalityValidatorV1 {
            validator_id: "validator-b".to_string(),
            stake_weight: 33,
            finality_signer_public_key: sample_public_key_hex(&validator_keys, "validator-b"),
            activation_height: 42,
            exit_height: None,
        },
        WorldFinalityValidatorV1 {
            validator_id: "validator-c".to_string(),
            stake_weight: 33,
            finality_signer_public_key: sample_public_key_hex(&validator_keys, "validator-c"),
            activation_height: 42,
            exit_height: None,
        },
    ];
    let to_validator_set_hash =
        compute_world_finality_validator_set_hash("sample-set-2", 42, 6_667, &to_validators)
            .expect("to validator set hash");
    let mut transition = WorldFinalityValidatorSetTransitionV1 {
        from_validator_set_id: proof.validator_set_id.clone(),
        from_validator_set_hash: proof.validator_set_hash.clone(),
        to_validator_set_id: "sample-set-2".to_string(),
        to_validator_set_activation_height: 42,
        to_quorum_threshold_bps: 6_667,
        to_validator_set_hash: to_validator_set_hash.clone(),
        to_validators,
        transition_height: 41,
        transition_block_hash: proof.head_proofs[1].head.block_hash.clone(),
        approvals: vec![],
        governance_certificate: None,
    };
    transition.approvals = ["validator-a", "validator-b"]
        .into_iter()
        .map(|validator_id| {
            let key = validator_keys
                .iter()
                .find(|(id, _)| *id == validator_id)
                .map(|(_, key)| key)
                .expect("sample validator key");
            let payload = world_finality_validator_set_transition_signing_payload(
                validator_id,
                transition.from_validator_set_hash.as_str(),
                transition.to_validator_set_hash.as_str(),
                transition.to_validator_set_activation_height,
                transition.transition_height,
                transition.transition_block_hash.as_str(),
            )
            .expect("transition signing payload");
            WorldFinalityValidatorSetTransitionApprovalV1 {
                validator_id: validator_id.to_string(),
                from_validator_set_hash: transition.from_validator_set_hash.clone(),
                to_validator_set_hash: transition.to_validator_set_hash.clone(),
                to_validator_set_activation_height: transition.to_validator_set_activation_height,
                transition_height: transition.transition_height,
                transition_block_hash: transition.transition_block_hash.clone(),
                signature_scheme: "ed25519".to_string(),
                signature_hex: hex::encode(key.sign(payload.as_slice()).to_bytes()),
            }
        })
        .collect();
    let governance_keys = sample_governance_keys();
    let governance_signers = vec![
        WorldFinalityGovernanceSignerV1 {
            signer_id: "governance-a".to_string(),
            stake_weight: 34,
            governance_public_key: sample_governance_public_key_hex(
                &governance_keys,
                "governance-a",
            ),
            activation_height: 1,
            exit_height: None,
        },
        WorldFinalityGovernanceSignerV1 {
            signer_id: "governance-b".to_string(),
            stake_weight: 33,
            governance_public_key: sample_governance_public_key_hex(
                &governance_keys,
                "governance-b",
            ),
            activation_height: 1,
            exit_height: None,
        },
        WorldFinalityGovernanceSignerV1 {
            signer_id: "governance-c".to_string(),
            stake_weight: 33,
            governance_public_key: sample_governance_public_key_hex(
                &governance_keys,
                "governance-c",
            ),
            activation_height: 1,
            exit_height: None,
        },
    ];
    let governance_set_hash = compute_world_finality_governance_set_hash(
        "sample-governance-set-1",
        1,
        6_667,
        &governance_signers,
    )
    .expect("governance set hash");
    let governance_approvals = ["governance-a", "governance-b"]
        .into_iter()
        .map(|signer_id| {
            let key = governance_keys
                .iter()
                .find(|(id, _)| *id == signer_id)
                .map(|(_, key)| key)
                .expect("sample governance key");
            let payload = world_finality_validator_set_transition_governance_signing_payload(
                signer_id,
                proof.world_id.as_str(),
                governance_set_hash.as_str(),
                transition.from_validator_set_hash.as_str(),
                transition.to_validator_set_hash.as_str(),
                transition.to_validator_set_activation_height,
                transition.transition_height,
                transition.transition_block_hash.as_str(),
            )
            .expect("transition governance signing payload");
            WorldFinalityValidatorSetTransitionGovernanceApprovalV1 {
                signer_id: signer_id.to_string(),
                governance_set_hash: governance_set_hash.clone(),
                from_validator_set_hash: transition.from_validator_set_hash.clone(),
                to_validator_set_hash: transition.to_validator_set_hash.clone(),
                to_validator_set_activation_height: transition.to_validator_set_activation_height,
                transition_height: transition.transition_height,
                transition_block_hash: transition.transition_block_hash.clone(),
                signature_scheme: "ed25519".to_string(),
                signature_hex: hex::encode(key.sign(payload.as_slice()).to_bytes()),
            }
        })
        .collect();
    transition.governance_certificate =
        Some(WorldFinalityValidatorSetTransitionGovernanceCertificateV1 {
            governance_set_id: "sample-governance-set-1".to_string(),
            governance_set_activation_height: 1,
            governance_threshold_bps: 6_667,
            governance_set_hash,
            governance_signers,
            governance_approvals,
        });
    proof.validator_set_transitions = vec![transition];
    proof.finality_commitments[2].validator_set_hash = to_validator_set_hash;
    proof.finality_commitments[2].votes = vec![
        sample_signed_finality_vote(
            &validator_keys,
            "validator-a",
            &proof.finality_commitments[2],
        ),
        sample_signed_finality_vote(
            &validator_keys,
            "validator-b",
            &proof.finality_commitments[2],
        ),
    ];
    proof.misbehavior_evidence[0].height = 40;
    proof
}

fn sample_world_head_proof_at(height: u64, prev_block_hash: &str) -> WorldHeadProofV1 {
    let state_root = format!("state-root-{height}");
    let action_root = format!("action-root-{height}");
    let journal_ref = format!("journal-ref-{height}");
    let snapshot_ref = format!("snapshot-ref-{height}");
    let block = WorldBlock {
        world_id: "world-a".to_string(),
        height,
        prev_block_hash: prev_block_hash.to_string(),
        action_root: action_root.clone(),
        event_root: format!("event-root-{height}"),
        state_root: state_root.clone(),
        journal_ref: journal_ref.clone(),
        snapshot_ref: snapshot_ref.clone(),
        receipts_root: format!("receipts-root-{height}"),
        proposer_id: "validator-a".to_string(),
        timestamp_ms: 1_772_467_200_000 + height as i64,
        signature: "block-signature-evidence-only".to_string(),
    };
    let block_hash = canonical_blake3_hex(&block);
    WorldHeadProofV1 {
        schema_version: WORLD_HEAD_PROOF_V1_SCHEMA,
        world_id: "world-a".to_string(),
        height,
        timestamp_ms: 1_772_467_200_000 + height as i64,
        head: WorldHeadAnnounce {
            world_id: "world-a".to_string(),
            height,
            block_hash: block_hash.clone(),
            state_root: state_root.clone(),
            timestamp_ms: 1_772_467_200_000 + height as i64,
            signature: "head-signature-evidence-only".to_string(),
        },
        block,
        snapshot_manifest_ref: BlobRef {
            content_hash: snapshot_ref.clone(),
            size_bytes: 120,
            codec: WIRE_ENCODING_CBOR.to_string(),
            links: vec!["snapshot-chunk-1".to_string()],
        },
        journal_segments_ref: BlobRef {
            content_hash: journal_ref.clone(),
            size_bytes: 80,
            codec: WIRE_ENCODING_CBOR.to_string(),
            links: vec!["journal-segment-1".to_string()],
        },
        consensus: HeadConsensusEvidenceV1 {
            consensus_status: "committed".to_string(),
            proposer_id: "validator-a".to_string(),
            quorum_threshold: 2,
            validator_count: 3,
            vote_count: 2,
            approver_ids: vec!["validator-a".to_string(), "validator-b".to_string()],
            evidence_hash: format!("consensus-evidence-{height}"),
        },
        execution: ExecutionBindingEvidenceV1 {
            execution_height: height,
            node_block_hash: block_hash,
            execution_block_hash: format!("execution-block-{height}"),
            execution_state_root: state_root.clone(),
            action_root,
        },
        checkpoint: Some(CheckpointClosureEvidenceV1 {
            checkpoint_height: height,
            execution_block_hash: format!("execution-block-{height}"),
            execution_state_root: state_root.clone(),
            manifest_ref: format!("checkpoint-manifest-{height}"),
            manifest_hash: format!("checkpoint-manifest-hash-{height}"),
            pinned_refs: vec![snapshot_ref, journal_ref, state_root],
        }),
        claim_boundary: WORLD_HEAD_PROOF_CLAIM_BOUNDARY_V1.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_json(path: &Path, proof: &WorldFinalityProofV1) {
        fs::write(
            path,
            serde_json::to_vec_pretty(proof).expect("encode finality proof json"),
        )
        .expect("write finality proof json");
    }

    #[test]
    fn verifies_transition_against_trusted_governance_anchor() {
        let proof = sample_world_finality_transition_proof();
        let governance_set_hash = proof.validator_set_transitions[0]
            .governance_certificate
            .as_ref()
            .expect("governance certificate")
            .governance_set_hash
            .clone();
        let proof_path = std::env::temp_dir().join(format!(
            "oasis7-finality-proof-{}-transition-governance.json",
            std::process::id()
        ));
        write_json(&proof_path, &proof);
        let summary = verify_finality_proof_path(
            &proof_path,
            "json",
            None,
            Some("world-a"),
            Some(42),
            Some(governance_set_hash.as_str()),
            Some("finality-transition-proof-ref-42".to_string()),
        )
        .expect("verify transition finality proof");
        let _ = fs::remove_file(&proof_path);
        assert_eq!(summary["status"], "pass");
        assert_eq!(summary["proof_ref"], "finality-transition-proof-ref-42");
        assert_eq!(summary["finality"]["validator_set_transition_count"], 1);
        assert_eq!(
            summary["finality"]["trusted_governance_anchor_checked"],
            true
        );
        assert_eq!(
            summary["finality"]["governance_set_hash"],
            governance_set_hash
        );
        assert_eq!(summary["finality"]["governance_certificate_count"], 1);
    }

    #[test]
    fn rejects_transition_without_trusted_governance_anchor() {
        let proof = sample_world_finality_transition_proof();
        let proof_path = std::env::temp_dir().join(format!(
            "oasis7-finality-proof-{}-transition-missing-governance-anchor.json",
            std::process::id()
        ));
        write_json(&proof_path, &proof);
        let err = verify_finality_proof_path(
            &proof_path,
            "json",
            None,
            Some("world-a"),
            Some(42),
            None,
            None,
        )
        .expect_err("missing governance anchor rejected");
        let _ = fs::remove_file(&proof_path);
        assert!(
            err.contains("requires --expect-governance-set-hash trusted anchor"),
            "{err}"
        );
    }

    #[test]
    fn rejects_transition_governance_anchor_mismatch() {
        let proof = sample_world_finality_transition_proof();
        let proof_path = std::env::temp_dir().join(format!(
            "oasis7-finality-proof-{}-transition-governance-anchor-mismatch.json",
            std::process::id()
        ));
        write_json(&proof_path, &proof);
        let err = verify_finality_proof_path(
            &proof_path,
            "json",
            None,
            Some("world-a"),
            Some(42),
            Some("wrong-governance-set-hash"),
            None,
        )
        .expect_err("governance anchor mismatch rejected");
        let _ = fs::remove_file(&proof_path);
        assert!(err.contains("governance set hash mismatch"), "{err}");
    }
}
