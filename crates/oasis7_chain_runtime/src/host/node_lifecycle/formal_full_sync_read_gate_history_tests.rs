use super::*;

fn migration_prefix(fixture: &IssuerFixture) -> FullSyncIssuerHistoryV3 {
    FullSyncIssuerHistoryV3 {
        v2_entries: fixture.history.v2_entries.clone(),
        v3_entries: vec![fixture.history.v3_entries[0].clone()],
    }
}

fn forked_lease_history(
    fixture: &IssuerFixture,
    lease_id: u8,
    request_id: u8,
    nonce: u8,
    time: u64,
) -> FullSyncIssuerHistoryV3 {
    let mut history = migration_prefix(fixture);
    let migration_head = history.v3_entries[0].record.next_anchor.clone();
    let observation = v3_observation_for_test(&fixture.verified_v3, &migration_head, nonce, time);
    let state = VerifiedFullSyncIssuerStateV3::from_witness_observation(
        fixture.verified_v2.clone(),
        fixture.verified_v3.clone(),
        history.clone(),
        &observation,
    )
    .unwrap();
    let original_request = match &fixture.history.v3_entries[1].record.event {
        FullSyncIssuerEventV3::Lease { request, .. } => request.as_ref(),
        _ => panic!("fixture second event must be a committed lease"),
    };
    let mut body = original_request.body.clone();
    body.lease_id = issuer_hash(lease_id);
    body.request_id = issuer_hash(request_id);
    body.request_nonce = issuer_hash(nonce.wrapping_add(1));
    body.expected_head = migration_head;
    let request = SignedFullSyncProviderLeaseRequestV3::sign(body, &key(PROVIDER_KEY)).unwrap();
    let prepared = state
        .prepare_lease(request, &observation, &key(LEASE_SIGNER_KEY))
        .unwrap();
    let record = prepared.record().unwrap().clone();
    let receipt = SignedFullSyncIssuerReceiptV3::sign(
        FullSyncIssuerReceiptObservationV3::V3(observation),
        &record,
        &key(WITNESS_KEY),
    )
    .unwrap();
    history
        .v3_entries
        .push(prepared.commit(receipt).unwrap().entry().clone());
    history
}

fn revoke_target_in_history(
    fixture: &IssuerFixture,
    mut history: FullSyncIssuerHistoryV3,
    nonce: u8,
    time: u64,
) -> FullSyncIssuerHistoryV3 {
    let head = history
        .v3_entries
        .last()
        .unwrap()
        .record
        .next_anchor
        .clone();
    let observation = v3_observation_for_test(&fixture.verified_v3, &head, nonce, time);
    let state = VerifiedFullSyncIssuerStateV3::from_witness_observation(
        fixture.verified_v2.clone(),
        fixture.verified_v3.clone(),
        history.clone(),
        &observation,
    )
    .unwrap();
    let command = SignedFullSyncIssuerCommandV3::sign(
        FullSyncIssuerCommandBodyV3 {
            schema_version: 3,
            world: fixture.verified_v3.body().world.clone(),
            authority_epoch: fixture.verified_v3.body().authority_epoch,
            scope_digest: fixture.verified_v3.scope_digest().into(),
            policy_digest: fixture.verified_v3.policy_digest().into(),
            signer_generation: fixture.verified_v3.body().signer_generation,
            target: fixture.target.clone(),
            operation: FullSyncIssuerOperationV3::Revoke,
            expected_head: head,
            expected_target_generation: 1,
            request_id: issuer_hash(nonce.wrapping_add(1)),
            command_nonce: issuer_hash(nonce.wrapping_add(2)),
        },
        &key(MANAGER_KEY),
    )
    .unwrap();
    let prepared = state.prepare_management(command, &observation).unwrap();
    let record = prepared.record().unwrap().clone();
    let receipt = SignedFullSyncIssuerReceiptV3::sign(
        FullSyncIssuerReceiptObservationV3::V3(observation),
        &record,
        &key(WITNESS_KEY),
    )
    .unwrap();
    history
        .v3_entries
        .push(prepared.commit(receipt).unwrap().entry().clone());
    history
}

fn request_object(
    endpoint: &str,
    reader: &SigningKey,
    base: u64,
    anchor: &VerifiedFormalReadAnchorV1,
    hash: String,
) -> u16 {
    post_object(
        endpoint,
        reader,
        signed_grant(reader, base, 1),
        cut(anchor),
        hash,
    )
    .0
}

#[test]
fn lower_valid_history_is_rejected_without_replacing_installed_head() {
    let (world, trust, anchor, material, proof) = full_sync_fixture();
    let base = now_ms();
    let fixture = issue_lease_fixture(trust.initial_trust.clone(), base);
    let source = Arc::new(SignedEvidenceSource::new(&fixture, base + 2_000));
    let gate = test_gate(&fixture, source.clone(), PROVIDER_INCARNATION);
    let (mut server, endpoint, _, node, _, _) = start_live_service_with_read_gate(
        world,
        trust,
        anchor.clone(),
        material.clone(),
        proof,
        Some(gate),
    );
    let reader = key(READER_KEY);
    let hash = material.roots[&ArtifactRole::Snapshot].clone();

    assert_eq!(
        request_object(&endpoint, &reader, base, &anchor, hash.clone()),
        200
    );
    *source.history.lock().unwrap() = migration_prefix(&fixture);
    assert_ne!(
        request_object(&endpoint, &reader, base, &anchor, hash.clone()),
        200
    );

    *source.history.lock().unwrap() = fixture.history.clone();
    assert_eq!(request_object(&endpoint, &reader, base, &anchor, hash), 200);
    server.stop().unwrap();
    node.lock().unwrap().stop().unwrap();
}

#[test]
fn equal_revision_signed_fork_poison_is_sticky() {
    let (world, trust, anchor, material, proof) = full_sync_fixture();
    let base = now_ms();
    let fixture = issue_lease_fixture(trust.initial_trust.clone(), base);
    let fork = forked_lease_history(&fixture, 71, 72, 73, base + 2_000);
    assert_eq!(
        fork.v3_entries.last().unwrap().record.next_anchor.revision,
        fixture
            .history
            .v3_entries
            .last()
            .unwrap()
            .record
            .next_anchor
            .revision
    );
    assert_ne!(
        fork.v3_entries.last().unwrap().record.next_anchor,
        fixture
            .history
            .v3_entries
            .last()
            .unwrap()
            .record
            .next_anchor
    );
    let source = Arc::new(SignedEvidenceSource::new(&fixture, base + 2_000));
    let gate = test_gate(&fixture, source.clone(), PROVIDER_INCARNATION);
    let (mut server, endpoint, _, node, _, _) = start_live_service_with_read_gate(
        world,
        trust,
        anchor.clone(),
        material.clone(),
        proof,
        Some(gate),
    );
    let reader = key(READER_KEY);
    let hash = material.roots[&ArtifactRole::Snapshot].clone();

    assert_eq!(
        request_object(&endpoint, &reader, base, &anchor, hash.clone()),
        200
    );
    *source.history.lock().unwrap() = fork;
    let (fork_status, fork_body) = post_object(
        &endpoint,
        &reader,
        signed_grant(&reader, base, 1),
        cut(&anchor),
        hash.clone(),
    );
    assert_ne!(fork_status, 200);
    assert!(String::from_utf8_lossy(&fork_body).contains("same-revision anchor conflict"));

    *source.history.lock().unwrap() = fixture.history.clone();
    let (after_reset_status, after_reset_body) = post_object(
        &endpoint,
        &reader,
        signed_grant(&reader, base, 1),
        cut(&anchor),
        hash,
    );
    assert_ne!(after_reset_status, 200);
    assert!(String::from_utf8_lossy(&after_reset_body).contains("gate is poisoned"));
    server.stop().unwrap();
    node.lock().unwrap().stop().unwrap();
}

#[test]
fn higher_complete_signed_nonprefix_fork_poison_is_sticky() {
    let (world, trust, anchor, material, proof) = full_sync_fixture();
    let base = now_ms();
    let fixture = issue_lease_fixture(trust.initial_trust.clone(), base);
    let fork = forked_lease_history(&fixture, 74, 75, 76, base + 2_000);
    let higher_fork = revoke_target_in_history(&fixture, fork, 77, base + 3_000);
    assert!(
        higher_fork
            .v3_entries
            .last()
            .unwrap()
            .record
            .next_anchor
            .revision
            > fixture
                .history
                .v3_entries
                .last()
                .unwrap()
                .record
                .next_anchor
                .revision
    );
    let source = Arc::new(SignedEvidenceSource::new(&fixture, base + 3_000));
    let gate = test_gate(&fixture, source.clone(), PROVIDER_INCARNATION);
    let (mut server, endpoint, _, node, _, _) = start_live_service_with_read_gate(
        world,
        trust,
        anchor.clone(),
        material.clone(),
        proof,
        Some(gate),
    );
    let reader = key(READER_KEY);
    let hash = material.roots[&ArtifactRole::Snapshot].clone();

    assert_eq!(
        request_object(&endpoint, &reader, base, &anchor, hash.clone()),
        200
    );
    *source.history.lock().unwrap() = higher_fork;
    let (fork_status, fork_body) = post_object(
        &endpoint,
        &reader,
        signed_grant(&reader, base, 1),
        cut(&anchor),
        hash.clone(),
    );
    assert_ne!(fork_status, 200);
    assert!(
        String::from_utf8_lossy(&fork_body).contains("does not contain the exact installed anchor")
    );

    *source.history.lock().unwrap() = fixture.history.clone();
    let (after_reset_status, after_reset_body) = post_object(
        &endpoint,
        &reader,
        signed_grant(&reader, base, 1),
        cut(&anchor),
        hash,
    );
    assert_ne!(after_reset_status, 200);
    assert!(String::from_utf8_lossy(&after_reset_body).contains("gate is poisoned"));
    server.stop().unwrap();
    node.lock().unwrap().stop().unwrap();
}

#[test]
fn query_minimum_must_be_exact_prefix_of_the_full_signed_history() {
    let (world, trust, anchor, material, proof) = full_sync_fixture();
    let base = now_ms();
    let fixture = issue_lease_fixture(trust.initial_trust.clone(), base);
    let pinned_minimum = fixture
        .history
        .v3_entries
        .last()
        .unwrap()
        .record
        .next_anchor
        .clone();
    let fork = forked_lease_history(&fixture, 78, 79, 80, base + 2_000);
    let higher_fork = revoke_target_in_history(&fixture, fork, 81, base + 3_000);
    assert!(
        higher_fork
            .v3_entries
            .last()
            .unwrap()
            .record
            .next_anchor
            .revision
            > pinned_minimum.revision
    );
    let source = Arc::new(SignedEvidenceSource::new(&fixture, base + 3_000));
    *source.history.lock().unwrap() = higher_fork;
    let gate = Arc::new(
        FullSyncV3ReadGate::new(
            fixture.trust.clone(),
            fixture.signed_v2.clone(),
            fixture.signed_v3.clone(),
            pinned_minimum,
            public(PROVIDER_KEY),
            issuer_hash(PROVIDER_INCARNATION),
            source.clone(),
        )
        .unwrap(),
    );
    let (mut server, endpoint, _, node, _, _) = start_live_service_with_read_gate(
        world,
        trust,
        anchor.clone(),
        material.clone(),
        proof,
        Some(gate),
    );
    let reader = key(READER_KEY);
    let hash = material.roots[&ArtifactRole::Snapshot].clone();
    let (status, body) = post_object(
        &endpoint,
        &reader,
        signed_grant(&reader, base, 1),
        cut(&anchor),
        hash,
    );
    assert_ne!(status, 200);
    let denial = String::from_utf8_lossy(&body);
    assert!(
        denial.contains("minimum"),
        "signed non-prefix history was rejected for an unexpected reason: {denial}"
    );
    assert_eq!(source.fetch_count(), 1);
    server.stop().unwrap();
    node.lock().unwrap().stop().unwrap();
}

#[test]
fn query_delay_cannot_advance_the_lease_not_before_boundary() {
    let (world, trust, anchor, material, proof) = full_sync_fixture();
    let base = now_ms();
    let fixture = issue_lease_fixture(trust.initial_trust.clone(), base);
    let source = Arc::new(SignedEvidenceSource::new(
        &fixture,
        fixture.lease_not_before.saturating_sub(1),
    ));
    source.delay_by(Duration::from_millis(20));
    let gate = test_gate(&fixture, source.clone(), PROVIDER_INCARNATION);
    let (mut server, endpoint, _, node, _, _) = start_live_service_with_read_gate(
        world,
        trust,
        anchor.clone(),
        material.clone(),
        proof,
        Some(gate),
    );
    let reader = key(READER_KEY);
    let hash = material.roots[&ArtifactRole::Snapshot].clone();
    let (too_early, _) = post_object(
        &endpoint,
        &reader,
        signed_grant(&reader, base, 1),
        cut(&anchor),
        hash.clone(),
    );
    assert_ne!(
        too_early, 200,
        "query delay cannot move earliest protected time forward"
    );

    source.set_time(fixture.lease_not_before);
    let (at_boundary, _) = post_object(
        &endpoint,
        &reader,
        signed_grant(&reader, base, 1),
        cut(&anchor),
        hash,
    );
    assert_eq!(
        at_boundary, 200,
        "the exact signed not-before boundary is accepted"
    );
    server.stop().unwrap();
    node.lock().unwrap().stop().unwrap();
}
