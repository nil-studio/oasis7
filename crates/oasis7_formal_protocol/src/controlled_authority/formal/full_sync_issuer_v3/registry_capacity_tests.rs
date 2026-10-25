use super::*;

#[test]
fn provider_lease_accepts_a_valid_registry_larger_than_one_event() {
    let old = old_policy();
    let mut state =
        VerifiedFullSyncIssuerStateV2::from_committed_records(old.clone(), vec![]).unwrap();
    let mut first_target = None;
    for sequence in 1..=1_000 {
        let node = SigningKey::from_bytes(&capacity_bytes(80, sequence));
        let target = FullSyncIssuerTargetV2 {
            node_public_key: hex::encode(node.verifying_key().as_bytes()),
            role: FullSyncReadRoleV2::StateSync,
        };
        first_target.get_or_insert_with(|| target.clone());
        let command = SignedFullSyncIssuerCommandV2::sign(
            FullSyncIssuerCommandBodyV2 {
                schema_version: 2,
                world: old.body().world.clone(),
                authority_epoch: old.body().authority_epoch,
                scope_digest: old.scope_digest().into(),
                policy_digest: old.policy_digest().into(),
                signer_generation: old.body().signer_generation,
                transition: None,
                target,
                operation: FullSyncIssuerOperationV2::Issue {
                    not_before_unix_ms: 1_000,
                    expires_unix_ms: 1_000_000,
                },
                expected_head: state.anchor().clone(),
                expected_target_generation: 0,
                request_id: capacity_id(81, sequence),
                command_nonce: capacity_id(82, sequence),
            },
            &key(2),
        )
        .unwrap();
        let prepared = state.prepare(command).unwrap();
        let record = prepared.record().unwrap().clone();
        let query = SignedFullSyncIssuerQueryV2::sign(
            &old,
            record.previous_anchor.clone(),
            capacity_bytes(83, sequence),
            &key(3),
        )
        .unwrap();
        let observation = SignedFullSyncIssuerObservationV2::sign(
            FullSyncIssuerObservationBodyV2 {
                schema_version: 2,
                query,
                current_anchor: record.previous_anchor.clone(),
            },
            &key(4),
        )
        .unwrap();
        let receipt = SignedFullSyncIssuerReceiptV2::sign(observation, &record, &key(4)).unwrap();
        state = prepared.commit(receipt).unwrap().state().clone();
    }
    let policy = new_policy(&old);
    let (state, _) = migrated_state(&old, &policy, &state, 205);
    assert_eq!(state.target_states.len(), 1_000);
    let registry_bytes = serde_cbor::to_vec(&state.target_states).unwrap().len();
    assert!(registry_bytes > super::super::types::MAX_ENTRY_BYTES_V3);
    assert!(registry_bytes < super::super::types::MAX_LEDGER_BYTES_V3);
    let request = provider_lease_request(&state, &first_target.unwrap(), 234, 235, 236);
    let observation = v3_observation(&state, 208, 2_000, 2_010);
    let prepared = state
        .prepare_lease(request, &observation, &key(8))
        .expect("a valid accumulated registry must not be limited to one event's byte budget");
    let receipt = SignedFullSyncIssuerReceiptV3::sign(
        FullSyncIssuerReceiptObservationV3::V3(observation),
        prepared.record().unwrap(),
        &key(4),
    )
    .unwrap();
    let decision = prepared.commit(receipt).unwrap();
    assert_eq!(decision.state().anchor().revision, 1_002);
    assert_eq!(decision.state().leases().len(), 1);
}
