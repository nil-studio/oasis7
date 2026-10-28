//! One signed issuer history for three readers and three provider incarnations.
//! This is an explicitly simulated witness/clock, never a production backend.
use super::*;
pub(crate) struct NetworkGates {
    pub gates: Vec<Arc<FullSyncV3ReadGate>>,
    pub policy_digest: String,
    pub committed_grants: Vec<FormalNodeReadGrantV1>,
    pub revoke_committed: Box<dyn Fn(u8) + Send + Sync>,
}
pub(crate) fn network_gates(trust: InitialActivationTrustV1) -> NetworkGates {
    let base = now_ms();
    let providers = [200, 201, 202];
    let mut f = issue_lease_fixture_keys(
        trust,
        base,
        FullSyncReadRoleV2::BftPendingCandidateV1,
        120,
        200,
        220,
        &providers,
    );
    let observation = v3_observation_for_test(
        &f.verified_v3,
        &f.history.v3_entries.last().unwrap().record.next_anchor,
        80,
        base + 2000,
    );
    let mut state = VerifiedFullSyncIssuerStateV3::from_witness_observation(
        f.verified_v2.clone(),
        f.verified_v3.clone(),
        f.history.clone(),
        &observation,
    )
    .unwrap();
    let mut nonce = 81u8;
    for reader in 120..=122 {
        for role in [
            FullSyncReadRoleV2::BftPendingCandidateV1,
            FullSyncReadRoleV2::FullReadOnly,
        ] {
            let target = FullSyncIssuerTargetV2 {
                node_public_key: public(if role == FullSyncReadRoleV2::FullReadOnly {
                    reader + 10
                } else {
                    reader
                }),
                role,
            };
            if !(reader == 120 && role == FullSyncReadRoleV2::BftPendingCandidateV1) {
                let observed =
                    v3_observation_for_test(&f.verified_v3, state.anchor(), nonce, base + 2000);
                nonce += 1;
                let command = SignedFullSyncIssuerCommandV3::sign(
                    FullSyncIssuerCommandBodyV3 {
                        schema_version: 3,
                        world: f.verified_v3.body().world.clone(),
                        authority_epoch: 1,
                        scope_digest: f.verified_v3.scope_digest().into(),
                        policy_digest: f.verified_v3.policy_digest().into(),
                        signer_generation: 2,
                        target: target.clone(),
                        operation: FullSyncIssuerOperationV3::Issue {
                            not_before_unix_ms: base.saturating_sub(1000),
                            expires_unix_ms: base + 60000,
                        },
                        expected_head: state.anchor().clone(),
                        expected_target_generation: 0,
                        request_id: issuer_hash(nonce),
                        command_nonce: issuer_hash(nonce + 1),
                    },
                    &key(MANAGER_KEY),
                )
                .unwrap();
                nonce += 2;
                let prepared = state.prepare_management(command, &observed).unwrap();
                let record = prepared.record().unwrap().clone();
                let receipt = SignedFullSyncIssuerReceiptV3::sign(
                    FullSyncIssuerReceiptObservationV3::V3(observed),
                    &record,
                    &key(WITNESS_KEY),
                )
                .unwrap();
                let decision = prepared.commit(receipt).unwrap();
                state = decision.state().clone();
                f.history.v3_entries.push(decision.entry().clone());
            }
            for (index, provider) in providers.iter().enumerate() {
                let observed =
                    v3_observation_for_test(&f.verified_v3, state.anchor(), nonce, base + 2000);
                nonce += 1;
                let request = SignedFullSyncProviderLeaseRequestV3::sign(
                    FullSyncProviderLeaseRequestBodyV3 {
                        schema_version: 3,
                        world: f.verified_v3.body().world.clone(),
                        authority_epoch: 1,
                        scope_digest: f.verified_v3.scope_digest().into(),
                        policy_digest: f.verified_v3.policy_digest().into(),
                        signer_generation: 2,
                        provider_public_key: public(*provider),
                        provider_incarnation: issuer_hash(220 + index as u8),
                        lease_id: issuer_hash(nonce),
                        request_id: issuer_hash(nonce + 1),
                        request_nonce: issuer_hash(nonce + 2),
                        expected_head: state.anchor().clone(),
                        target: target.clone(),
                        target_generation: 1,
                    },
                    &key(*provider),
                )
                .unwrap();
                nonce += 3;
                let prepared = state
                    .prepare_lease(request, &observed, &key(LEASE_SIGNER_KEY))
                    .unwrap();
                let record = prepared.record().unwrap().clone();
                let receipt = SignedFullSyncIssuerReceiptV3::sign(
                    FullSyncIssuerReceiptObservationV3::V3(observed),
                    &record,
                    &key(WITNESS_KEY),
                )
                .unwrap();
                let decision = prepared.commit(receipt).unwrap();
                state = decision.state().clone();
                f.history.v3_entries.push(decision.entry().clone());
            }
        }
    }
    let source = Arc::new(SignedEvidenceSource::new(&f, base + 2000));
    let gates = providers
        .iter()
        .enumerate()
        .map(|(index, provider)| {
            Arc::new(
                FullSyncV3ReadGate::new(
                    f.trust.clone(),
                    f.signed_v2.clone(),
                    f.signed_v3.clone(),
                    f.minimum_v3.clone(),
                    public(*provider),
                    issuer_hash(220 + index as u8),
                    source.clone(),
                )
                .unwrap(),
            )
        })
        .collect();
    let committed_grants = (120..=122)
        .map(|reader| {
            FormalNodeReadGrantV1::sign(
                FormalNodeReadGrantBodyV1 {
                    schema_version: 1,
                    world: WorldIdentity {
                        world_id: f.trust.world.world_id.clone(),
                        genesis_digest: f.trust.world.genesis_digest.clone(),
                    },
                    chain_id: f.trust.world.chain_id.clone(),
                    node_public_key: public(reader + 10),
                    role: FormalNodeReadRoleV1::FullReadOnly,
                    grant_epoch: 1,
                    authority_epoch: 1,
                    not_before_unix_ms: base.saturating_sub(1000),
                    expires_unix_ms: base + 60000,
                },
                &hex::encode([9; 32]),
            )
            .unwrap()
        })
        .collect();
    let revoke_source = source.clone();
    let revoke_fixture = f.clone();
    NetworkGates {
        gates,
        policy_digest: f.verified_v3.policy_digest().into(),
        committed_grants,
        revoke_committed: Box::new(move |index| {
            let mut fixture = revoke_fixture.clone();
            fixture.target.node_public_key = public(130 + index);
            fixture.target.role = FullSyncReadRoleV2::FullReadOnly;
            revoke_source.revoke_target(&fixture);
        }),
    }
}
