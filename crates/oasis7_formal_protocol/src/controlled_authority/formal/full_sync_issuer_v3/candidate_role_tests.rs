//! A distinct explicit V3 purpose; no legacy grant conversion or policy rotation.
use super::*;
const CANDIDATE: FullSyncReadRoleV2 = FullSyncReadRoleV2::BftPendingCandidateV1;
fn candidate() -> FullSyncIssuerTargetV2 {
    FullSyncIssuerTargetV2 {
        node_public_key: public(5),
        role: CANDIDATE,
    }
}
fn candidate_policy(old: &VerifiedFullSyncIssuerPolicyV2) -> VerifiedFullSyncIssuerPolicyV3 {
    let mut body = new_policy(old).body().clone();
    body.allowed_roles = vec![
        FullSyncReadRoleV2::StateSync,
        FullSyncReadRoleV2::FullReadOnly,
        CANDIDATE,
    ];
    let signed = SignedFullSyncIssuerPolicyV3::sign(body, &key(1)).unwrap();
    VerifiedFullSyncIssuerPolicyV3::verify(&signed, &trust(), old).unwrap()
}
fn command(
    state: &VerifiedFullSyncIssuerStateV3,
    operation: FullSyncIssuerOperationV3,
    nonce: u8,
) -> SignedFullSyncIssuerCommandV3 {
    SignedFullSyncIssuerCommandV3::sign(
        FullSyncIssuerCommandBodyV3 {
            schema_version: 3,
            world: world(),
            authority_epoch: 7,
            scope_digest: state.policy().scope_digest().into(),
            policy_digest: state.policy().policy_digest().into(),
            signer_generation: state.policy().body().signer_generation,
            target: candidate(),
            operation,
            expected_head: state.anchor().clone(),
            expected_target_generation: state
                .target_state(&candidate())
                .map_or(0, |s| s.generation),
            request_id: hash(nonce),
            command_nonce: hash(nonce + 1),
        },
        &key(7),
    )
    .unwrap()
}
fn manage(
    state: &VerifiedFullSyncIssuerStateV3,
    operation: FullSyncIssuerOperationV3,
    nonce: u8,
) -> VerifiedFullSyncIssuerStateV3 {
    let observation = v3_observation(state, nonce + 2, 2_000, 2_100);
    let prepared = state
        .prepare_management(command(state, operation, nonce), &observation)
        .unwrap();
    let receipt = SignedFullSyncIssuerReceiptV3::sign(
        FullSyncIssuerReceiptObservationV3::V3(observation),
        prepared.record().unwrap(),
        &key(4),
    )
    .unwrap();
    prepared.commit(receipt).unwrap().state().clone()
}
#[test]
fn v2_rejects_candidate_policy_and_every_management_operation() {
    let old = old_policy();
    let mut body = old.body().clone();
    // A one-role list must still fail, not just lists longer than two.
    body.allowed_roles = vec![CANDIDATE];
    assert!(SignedFullSyncIssuerPolicyV2::sign(body, &key(1)).is_err());
    let state = current_v2_state(&old);
    for operation in [
        FullSyncIssuerOperationV2::Issue {
            not_before_unix_ms: 1_000,
            expires_unix_ms: 10_000,
        },
        FullSyncIssuerOperationV2::Replace {
            not_before_unix_ms: 1_000,
            expires_unix_ms: 10_000,
        },
        FullSyncIssuerOperationV2::Revoke,
    ] {
        let body = FullSyncIssuerCommandBodyV2 {
            schema_version: 2,
            world: world(),
            authority_epoch: 7,
            scope_digest: old.scope_digest().into(),
            policy_digest: old.policy_digest().into(),
            signer_generation: 1,
            transition: None,
            target: candidate(),
            operation,
            expected_head: state.anchor().clone(),
            expected_target_generation: 0,
            request_id: hash(150),
            command_nonce: hash(151),
        };
        // Raw JSON/CBOR may name the enum, but cannot sign or verify V2 authority.
        assert!(SignedFullSyncIssuerCommandV2::sign(body.clone(), &key(2)).is_err());
        assert!(
            SignedFullSyncIssuerCommandV2 {
                body,
                signature_hex: "00".repeat(64)
            }
            .verify(&old)
            .is_err()
        );
    }
    assert_eq!(
        serde_json::to_string(&FullSyncReadRoleV2::StateSync).unwrap(),
        "\"StateSync\""
    );
    assert_eq!(
        serde_json::to_string(&FullSyncReadRoleV2::FullReadOnly).unwrap(),
        "\"FullReadOnly\""
    );
}
#[test]
fn original_v3_policy_does_not_expand_and_migration_never_synthesizes_candidate_targets() {
    let old = old_policy();
    let v2 = current_v2_state(&old);
    let v3 = new_policy(&old);
    let (state, _) = migrated_state(&old, &v3, &v2, 160);
    assert!(state.target_state(&candidate()).is_none());
    let observation = v3_observation(&state, 163, 2_000, 2_100);
    assert!(
        state
            .prepare_management(
                command(
                    &state,
                    FullSyncIssuerOperationV3::Issue {
                        not_before_unix_ms: 1_000,
                        expires_unix_ms: 10_000
                    },
                    164
                ),
                &observation
            )
            .is_err()
    );
    let mut request = provider_lease_request(
        &state,
        &FullSyncIssuerTargetV2 {
            node_public_key: public(5),
            role: FullSyncReadRoleV2::StateSync,
        },
        166,
        167,
        168,
    )
    .body;
    request.target = candidate();
    let request = SignedFullSyncProviderLeaseRequestV3::sign(request, &key(6)).unwrap();
    assert!(request.verify(&v3).is_err());
    assert!(state.prepare_lease(request, &observation, &key(8)).is_err());
    // Even explicit root permission does not manufacture an issuance at migration.
    let enabled = candidate_policy(&old);
    let (enabled_state, _) = migrated_state(&old, &enabled, &v2, 170);
    assert!(enabled_state.target_state(&candidate()).is_none());
}
#[test]
fn explicit_candidate_issue_replace_lease_revoke_are_isolated_and_fully_replayed() {
    let old = old_policy();
    let v2 = current_v2_state(&old);
    let v3 = candidate_policy(&old);
    let (state, _) = migrated_state(&old, &v3, &v2, 180);
    let original = FullSyncIssuerTargetV2 {
        node_public_key: public(5),
        role: FullSyncReadRoleV2::StateSync,
    };
    let original_state = state.target_state(&original).unwrap().clone();
    let issued = manage(
        &state,
        FullSyncIssuerOperationV3::Issue {
            not_before_unix_ms: 1_000,
            expires_unix_ms: 10_000,
        },
        183,
    );
    assert!(issued.target_state(&candidate()).unwrap().active);
    assert_eq!(issued.target_state(&candidate()).unwrap().generation, 1);
    assert_eq!(issued.target_state(&original).unwrap(), &original_state);
    let replaced = manage(
        &issued,
        FullSyncIssuerOperationV3::Replace {
            not_before_unix_ms: 1_000,
            expires_unix_ms: 9_000,
        },
        186,
    );
    assert_eq!(replaced.target_state(&candidate()).unwrap().generation, 2);
    let request = provider_lease_request(&replaced, &candidate(), 189, 190, 191);
    request.verify(&v3).unwrap();
    let observation = v3_observation(&replaced, 192, 2_000, 2_100);
    let prepared = replaced
        .prepare_lease(request.clone(), &observation, &key(8))
        .unwrap();
    let lease_record = prepared.record().unwrap().clone();
    let receipt = SignedFullSyncIssuerReceiptV3::sign(
        FullSyncIssuerReceiptObservationV3::V3(observation.clone()),
        &lease_record,
        &key(4),
    )
    .unwrap();
    let leased = prepared.commit(receipt).unwrap().state().clone();
    let revoked = manage(&leased, FullSyncIssuerOperationV3::Revoke, 193);
    let current = revoked.target_state(&candidate()).unwrap();
    assert!(!current.active);
    assert_eq!(current.generation, 3);
    assert_eq!(revoked.target_state(&original).unwrap(), &original_state);
    assert_eq!(
        revoked.leases().len(),
        1,
        "historical evidence is retained, not current authority"
    );
    let fresh = provider_lease_request(&revoked, &candidate(), 196, 197, 198);
    let fresh_observation = v3_observation(&revoked, 199, 2_000, 2_100);
    assert!(
        revoked
            .prepare_lease(fresh, &fresh_observation, &key(8))
            .is_err()
    );
    let retry = revoked
        .prepare_lease(request, &observation, &key(8))
        .unwrap();
    assert!(retry.record().is_none());
    assert_eq!(
        retry.existing().unwrap().record,
        lease_record,
        "retry is only the original historical result"
    );
    let replayed = VerifiedFullSyncIssuerStateV3::from_witness_observation(
        old,
        v3,
        FullSyncIssuerHistoryV3 {
            v2_entries: v2.records().to_vec(),
            v3_entries: revoked.entries().to_vec(),
        },
        &fresh_observation,
    )
    .unwrap();
    assert_eq!(replayed.anchor(), revoked.anchor());
    assert_eq!(replayed.target_state(&candidate()).unwrap(), current);
    assert_eq!(replayed.leases(), revoked.leases());
}
