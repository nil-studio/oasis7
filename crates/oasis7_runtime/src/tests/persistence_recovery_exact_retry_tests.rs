use super::*;
use ed25519_dalek::{Signer, SigningKey};

const NOW_MS: u64 = 1_720_000_000_000;

fn signed_approval(
    snapshot: &Snapshot,
    journal_len: usize,
    state_root: &str,
    reason: &str,
    nonce: &str,
    on_call: &SigningKey,
    governance: &SigningKey,
) -> RollbackAuthorizationEnvelope {
    let intent = RollbackIntent {
        schema_version: 1,
        rollback_ticket: "ROLLBACK-EXACT-RETRY".to_string(),
        snapshot_hash: util::hash_json(snapshot).expect("snapshot hash"),
        snapshot_journal_len: snapshot.journal_len,
        target_journal_len: journal_len,
        target_journal_commitment: None,
        expected_target_state_root: state_root.to_string(),
        target_batch_id: Some("batch-signed-target".to_string()),
        reason: reason.to_string(),
        issued_at_ms: NOW_MS - 1_000,
        expires_at_ms: NOW_MS + 60_000,
        nonce: nonce.to_string(),
    };
    let payload = intent
        .canonical_signing_payload()
        .expect("canonical payload");
    RollbackAuthorizationEnvelope {
        intent,
        signatures: vec![
            RollbackApprovalSignature {
                authority_id: "on-call-alice".to_string(),
                role: RollbackAuthorityRole::OnCall,
                signature_scheme: "ed25519".to_string(),
                signature_hex: hex::encode(on_call.sign(&payload).to_bytes()),
            },
            RollbackApprovalSignature {
                authority_id: "governance-bob".to_string(),
                role: RollbackAuthorityRole::Governance,
                signature_scheme: "ed25519".to_string(),
                signature_hex: hex::encode(governance.sign(&payload).to_bytes()),
            },
        ],
    }
}

#[test]
fn committed_exact_retry_rejects_mismatched_supplied_snapshot_reason_and_target_atomically() {
    let on_call = SigningKey::from_bytes(&[7; 32]);
    let governance = SigningKey::from_bytes(&[9; 32]);
    let mut world = World::new();
    world
        .set_rollback_authority_registry(
            RollbackAuthorityRegistry::new([
                RollbackAuthorityRecord {
                    authority_id: "on-call-alice".to_string(),
                    role: RollbackAuthorityRole::OnCall,
                    public_key_hex: hex::encode(on_call.verifying_key().to_bytes()),
                    active: true,
                },
                RollbackAuthorityRecord {
                    authority_id: "governance-bob".to_string(),
                    role: RollbackAuthorityRole::Governance,
                    public_key_hex: hex::encode(governance.verifying_key().to_bytes()),
                    active: true,
                },
            ])
            .expect("authority registry"),
        )
        .expect("configure authorities");
    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-exact-retry".to_string(),
        pos: pos(0, 0),
    });
    world.step().expect("stable target");
    let snapshot = world.snapshot();
    let journal = world.journal().clone();
    let state_root = world.current_state_root_hash().expect("target root");
    let approval = signed_approval(
        &snapshot,
        journal.len(),
        &state_root,
        "signed-reason",
        "nonce-exact-retry-parameters",
        &on_call,
        &governance,
    );
    world
        .rollback_to_snapshot(
            snapshot.clone(),
            journal.clone(),
            "signed-reason",
            Some("batch-signed-target"),
            approval.clone(),
            NOW_MS,
        )
        .expect("initial rollback commits");

    let observe = |world: &World| {
        (
            world.snapshot(),
            world.journal().clone(),
            world.rollback_nonce_outcome("nonce-exact-retry-parameters"),
            world.rollback_readiness_without_evidence("nonce-exact-retry-parameters"),
            world.rollback_receipt_projection("nonce-exact-retry-parameters"),
        )
    };
    let mut mismatched_snapshot = snapshot.clone();
    mismatched_snapshot.state.time = mismatched_snapshot.state.time.saturating_add(1);
    let mut shorter_journal = journal.clone();
    shorter_journal
        .events
        .pop()
        .expect("non-empty target journal");
    let mut tampered_journal = journal.clone();
    tampered_journal.events[0].time = tampered_journal.events[0].time.saturating_add(1);
    let cases = [
        (
            mismatched_snapshot,
            journal.clone(),
            "signed-reason",
            "batch-signed-target",
        ),
        (
            snapshot.clone(),
            journal.clone(),
            "outer-reason-does-not-match",
            "batch-signed-target",
        ),
        (
            snapshot.clone(),
            journal.clone(),
            "signed-reason",
            "batch-other-target",
        ),
        (
            snapshot.clone(),
            shorter_journal,
            "signed-reason",
            "batch-signed-target",
        ),
        (
            snapshot.clone(),
            tampered_journal,
            "signed-reason",
            "batch-signed-target",
        ),
    ];
    for (supplied_snapshot, supplied_journal, supplied_reason, supplied_target) in cases {
        let before = observe(&world);
        world
            .rollback_to_snapshot(
                supplied_snapshot,
                supplied_journal,
                supplied_reason,
                Some(supplied_target),
                approval.clone(),
                NOW_MS,
            )
            .expect_err("committed retry must reject execution parameters outside signed intent");
        assert_eq!(
            observe(&world),
            before,
            "rejected retry mutated durable state"
        );
    }
}
