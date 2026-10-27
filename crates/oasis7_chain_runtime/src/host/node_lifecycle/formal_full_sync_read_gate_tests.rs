use super::*;
use crate::formal_live_service::full_sync_read_gate::{
    FullSyncIssuerReadEvidenceRequestV3, FullSyncIssuerReadEvidenceSource,
    FullSyncIssuerReadEvidenceV3, FullSyncV3ReadGate,
};
use crate::formal_live_service::{FormalReadError, FormalReadResult};
use oasis7_distfs::controlled_authority::formal::full_sync_issuer::*;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

#[test]
fn provider_read_mode_requires_exactly_matching_gate_dependencies() {
    use crate::formal_live_config::FullSyncProviderReadAuthorizationModeV1 as Mode;
    use crate::formal_live_service::full_sync_read_gate::validate_full_sync_read_mode_gate_pair;

    assert!(validate_full_sync_read_mode_gate_pair(Mode::LegacyV1, false).is_ok());
    assert!(validate_full_sync_read_mode_gate_pair(Mode::V3Required, true).is_ok());
    assert!(validate_full_sync_read_mode_gate_pair(Mode::LegacyV1, true).is_err());
    let error = validate_full_sync_read_mode_gate_pair(Mode::V3Required, false).unwrap_err();
    assert!(error.contains("no production witness/history and protected-clock adapter"));
}

const PROVIDER_KEY: u8 = 91;
const READER_KEY: u8 = 92;
const MANAGER_KEY: u8 = 15;
const V2_ISSUER_KEY: u8 = 12;
const LEASE_SIGNER_KEY: u8 = 14;
const WITNESS_KEY: u8 = 13;
const PROVIDER_INCARNATION: u8 = 23;

fn key(seed: u8) -> SigningKey {
    test_key(seed)
}

fn public(seed: u8) -> String {
    hex::encode(key(seed).verifying_key().to_bytes())
}

fn issuer_hash(seed: u8) -> String {
    test_hash(seed)
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

struct IssuerFixture {
    trust: InitialActivationTrustV1,
    signed_v2: SignedFullSyncIssuerPolicyV2,
    verified_v2: VerifiedFullSyncIssuerPolicyV2,
    signed_v3: SignedFullSyncIssuerPolicyV3,
    verified_v3: VerifiedFullSyncIssuerPolicyV3,
    minimum_v3: FullSyncIssuerAnchorV3,
    history: FullSyncIssuerHistoryV3,
    target: FullSyncIssuerTargetV2,
    lease_not_before: u64,
    lease_expires: u64,
}

fn issue_lease_fixture(trust: InitialActivationTrustV1, base: u64) -> IssuerFixture {
    issue_lease_fixture_role(trust, base, FullSyncReadRoleV2::StateSync)
}
fn issue_lease_fixture_role(
    trust: InitialActivationTrustV1,
    base: u64,
    role: FullSyncReadRoleV2,
) -> IssuerFixture {
    let signed_v2 = SignedFullSyncIssuerPolicyV2::sign(
        FullSyncIssuerPolicyBodyV2 {
            schema_version: 2,
            root_public_key: trust.issuer_public_key.clone(),
            activation_trust_digest: full_sync_activation_trust_digest_v2(&trust).unwrap(),
            world: trust.world.clone(),
            authority_epoch: 1,
            allowed_roles: vec![FullSyncReadRoleV2::StateSync],
            management_public_key: public(MANAGER_KEY),
            issuer_public_key: public(V2_ISSUER_KEY),
            witness_public_key: public(WITNESS_KEY),
            signer_generation: 1,
        },
        &key(9),
    )
    .unwrap();
    let verified_v2 = VerifiedFullSyncIssuerPolicyV2::verify(&signed_v2, &trust).unwrap();
    let signed_v3 = SignedFullSyncIssuerPolicyV3::sign(
        FullSyncIssuerPolicyBodyV3 {
            schema_version: 3,
            root_public_key: trust.issuer_public_key.clone(),
            activation_trust_digest: full_sync_activation_trust_digest_v3(&trust).unwrap(),
            world: trust.world.clone(),
            authority_epoch: 1,
            scope_digest: verified_v2.scope_digest().into(),
            allowed_roles: if role == FullSyncReadRoleV2::StateSync {
                vec![role]
            } else {
                vec![FullSyncReadRoleV2::StateSync, role]
            },
            provider_public_keys: vec![public(PROVIDER_KEY)],
            management_public_key: public(MANAGER_KEY),
            lease_signer_public_key: public(LEASE_SIGNER_KEY),
            witness_public_key: public(WITNESS_KEY),
            signer_generation: 2,
            max_authorization_staleness_ms: 30_000,
            max_clock_uncertainty_ms: 100,
            clock_profile_digest: issuer_hash(35),
        },
        &key(9),
    )
    .unwrap();
    let verified_v3 =
        VerifiedFullSyncIssuerPolicyV3::verify(&signed_v3, &trust, &verified_v2).unwrap();
    let mut target = FullSyncIssuerTargetV2 {
        node_public_key: public(READER_KEY),
        role: FullSyncReadRoleV2::StateSync,
    };
    let target_not_before = base.saturating_sub(1_000);
    let target_expires = base + 60_000;

    let empty =
        VerifiedFullSyncIssuerStateV2::from_committed_records(verified_v2.clone(), vec![]).unwrap();
    let command = SignedFullSyncIssuerCommandV2::sign(
        FullSyncIssuerCommandBodyV2 {
            schema_version: 2,
            world: verified_v2.body().world.clone(),
            authority_epoch: verified_v2.body().authority_epoch,
            scope_digest: verified_v2.scope_digest().into(),
            policy_digest: verified_v2.policy_digest().into(),
            signer_generation: verified_v2.body().signer_generation,
            transition: None,
            target: target.clone(),
            operation: FullSyncIssuerOperationV2::Issue {
                not_before_unix_ms: target_not_before,
                expires_unix_ms: target_expires,
            },
            expected_head: empty.anchor().clone(),
            expected_target_generation: 0,
            request_id: issuer_hash(40),
            command_nonce: issuer_hash(41),
        },
        &key(MANAGER_KEY),
    )
    .unwrap();
    let prepared_v2 = empty.prepare(command).unwrap();
    let v2_record = prepared_v2.record().unwrap().clone();
    let v2_query = SignedFullSyncIssuerQueryV2::sign(
        &verified_v2,
        v2_record.previous_anchor.clone(),
        [42; 32],
        &key(V2_ISSUER_KEY),
    )
    .unwrap();
    let v2_observation = SignedFullSyncIssuerObservationV2::sign(
        FullSyncIssuerObservationBodyV2 {
            schema_version: 2,
            query: v2_query,
            current_anchor: v2_record.previous_anchor.clone(),
        },
        &key(WITNESS_KEY),
    )
    .unwrap();
    let v2_receipt =
        SignedFullSyncIssuerReceiptV2::sign(v2_observation.clone(), &v2_record, &key(WITNESS_KEY))
            .unwrap();
    let v2_state = prepared_v2.commit(v2_receipt).unwrap().state().clone();

    let migration = SignedFullSyncIssuerMigrationV3::sign(
        FullSyncIssuerMigrationBodyV3 {
            schema_version: 3,
            previous_v2_policy_digest: verified_v2.policy_digest().into(),
            previous_v2_signer_generation: verified_v2.body().signer_generation,
            expected_v2_head: v2_state.anchor().clone(),
            new_policy: signed_v3.clone(),
            request_id: issuer_hash(43),
            manager_nonce: issuer_hash(44),
        },
        &key(MANAGER_KEY),
    )
    .unwrap();
    let migration_observation = v2_observation_for_test(&verified_v2, v2_state.anchor(), 45);
    let prepared_migration = v2_state
        .prepare_migration_v3(verified_v3.clone(), migration, &migration_observation)
        .unwrap();
    let migration_record = prepared_migration.record().unwrap().clone();
    let migration_receipt = SignedFullSyncIssuerReceiptV3::sign(
        FullSyncIssuerReceiptObservationV3::Migration(migration_observation),
        &migration_record,
        &key(WITNESS_KEY),
    )
    .unwrap();
    let migration_decision = prepared_migration.commit(migration_receipt).unwrap();
    let minimum_v3 = migration_decision.record().next_anchor.clone();
    let mut migrated = migration_decision.state().clone();
    let mut v3_entries = vec![migration_decision.entry().clone()];
    if role != FullSyncReadRoleV2::StateSync {
        target.role = role;
        let observed = v3_observation_for_test(&verified_v3, migrated.anchor(), 61, base);
        let command = SignedFullSyncIssuerCommandV3::sign(
            FullSyncIssuerCommandBodyV3 {
                schema_version: 3,
                world: verified_v3.body().world.clone(),
                authority_epoch: verified_v3.body().authority_epoch,
                scope_digest: verified_v3.scope_digest().into(),
                policy_digest: verified_v3.policy_digest().into(),
                signer_generation: verified_v3.body().signer_generation,
                target: target.clone(),
                operation: FullSyncIssuerOperationV3::Issue {
                    not_before_unix_ms: target_not_before,
                    expires_unix_ms: target_expires,
                },
                expected_head: migrated.anchor().clone(),
                expected_target_generation: 0,
                request_id: issuer_hash(62),
                command_nonce: issuer_hash(63),
            },
            &key(MANAGER_KEY),
        )
        .unwrap();
        let prepared = migrated.prepare_management(command, &observed).unwrap();
        let record = prepared.record().unwrap().clone();
        let receipt = SignedFullSyncIssuerReceiptV3::sign(
            FullSyncIssuerReceiptObservationV3::V3(observed),
            &record,
            &key(WITNESS_KEY),
        )
        .unwrap();
        let decision = prepared.commit(receipt).unwrap();
        migrated = decision.state().clone();
        v3_entries.push(decision.entry().clone());
    }

    let lease_creation_time = base + 1_000;
    let lease_observation =
        v3_observation_for_test(&verified_v3, migrated.anchor(), 46, lease_creation_time);
    let request = SignedFullSyncProviderLeaseRequestV3::sign(
        FullSyncProviderLeaseRequestBodyV3 {
            schema_version: 3,
            world: verified_v3.body().world.clone(),
            authority_epoch: verified_v3.body().authority_epoch,
            scope_digest: verified_v3.scope_digest().into(),
            policy_digest: verified_v3.policy_digest().into(),
            signer_generation: verified_v3.body().signer_generation,
            provider_public_key: public(PROVIDER_KEY),
            provider_incarnation: issuer_hash(PROVIDER_INCARNATION),
            lease_id: issuer_hash(47),
            request_id: issuer_hash(48),
            request_nonce: issuer_hash(49),
            expected_head: migrated.anchor().clone(),
            target: target.clone(),
            target_generation: 1,
        },
        &key(PROVIDER_KEY),
    )
    .unwrap();
    let prepared_lease = migrated
        .prepare_lease(request, &lease_observation, &key(LEASE_SIGNER_KEY))
        .unwrap();
    let lease_record = prepared_lease.record().unwrap().clone();
    let lease_not_before = transition_payload_not_before(&lease_record);
    let lease_receipt = SignedFullSyncIssuerReceiptV3::sign(
        FullSyncIssuerReceiptObservationV3::V3(lease_observation),
        &lease_record,
        &key(WITNESS_KEY),
    )
    .unwrap();
    let lease_decision = prepared_lease.commit(lease_receipt).unwrap();
    let lease_expires = match &lease_decision.record().event {
        FullSyncIssuerEventV3::Lease { lease, .. } => lease.body.expires_unix_ms,
        _ => unreachable!(),
    };
    let history = FullSyncIssuerHistoryV3 {
        v2_entries: v2_state.records().to_vec(),
        v3_entries: {
            v3_entries.push(lease_decision.entry().clone());
            v3_entries
        },
    };
    IssuerFixture {
        trust,
        signed_v2,
        verified_v2,
        signed_v3,
        verified_v3,
        minimum_v3,
        history,
        target,
        lease_not_before,
        lease_expires,
    }
}

fn v2_observation_for_test(
    policy: &VerifiedFullSyncIssuerPolicyV2,
    anchor: &FullSyncIssuerAnchorV2,
    nonce: u8,
) -> SignedFullSyncIssuerObservationV2 {
    let query =
        SignedFullSyncIssuerQueryV2::sign(policy, anchor.clone(), [nonce; 32], &key(V2_ISSUER_KEY))
            .unwrap();
    SignedFullSyncIssuerObservationV2::sign(
        FullSyncIssuerObservationBodyV2 {
            schema_version: 2,
            query,
            current_anchor: anchor.clone(),
        },
        &key(WITNESS_KEY),
    )
    .unwrap()
}

fn v3_observation_for_test(
    policy: &VerifiedFullSyncIssuerPolicyV3,
    anchor: &FullSyncIssuerAnchorV3,
    nonce: u8,
    time: u64,
) -> SignedFullSyncIssuerObservationV3 {
    let query = SignedFullSyncIssuerQueryV3::sign(
        policy,
        anchor.clone(),
        [nonce; 32],
        &key(LEASE_SIGNER_KEY),
    )
    .unwrap();
    SignedFullSyncIssuerObservationV3::sign(
        FullSyncIssuerObservationBodyV3 {
            schema_version: 3,
            query,
            current_anchor: anchor.clone(),
            protected_time: FullSyncIssuerProtectedTimeIntervalV3 {
                earliest_unix_ms: time,
                latest_unix_ms: time,
                clock_profile_digest: policy.body().clock_profile_digest.clone(),
            },
        },
        &key(WITNESS_KEY),
    )
    .unwrap()
}

struct SignedEvidenceSource {
    policy: VerifiedFullSyncIssuerPolicyV3,
    history: Mutex<FullSyncIssuerHistoryV3>,
    fetches: AtomicU64,
    protected_time: AtomicU64,
    delay: Mutex<Duration>,
    bad_nonce: Mutex<bool>,
    unavailable: Mutex<bool>,
}

impl SignedEvidenceSource {
    fn new(fixture: &IssuerFixture, protected_time: u64) -> Self {
        Self {
            policy: fixture.verified_v3.clone(),
            history: Mutex::new(fixture.history.clone()),
            fetches: AtomicU64::new(0),
            protected_time: AtomicU64::new(protected_time),
            delay: Mutex::new(Duration::ZERO),
            bad_nonce: Mutex::new(false),
            unavailable: Mutex::new(false),
        }
    }

    fn set_time(&self, time: u64) {
        self.protected_time.store(time, Ordering::SeqCst);
    }

    fn fetch_count(&self) -> u64 {
        self.fetches.load(Ordering::SeqCst)
    }

    fn delay_by(&self, delay: Duration) {
        *self.delay.lock().unwrap() = delay;
    }

    fn return_bad_nonce(&self, yes: bool) {
        *self.bad_nonce.lock().unwrap() = yes;
    }

    fn set_unavailable(&self, yes: bool) {
        *self.unavailable.lock().unwrap() = yes;
    }

    fn remove_committed_lease(&self) {
        self.history
            .lock()
            .unwrap()
            .v3_entries
            .retain(|entry| !matches!(&entry.record.event, FullSyncIssuerEventV3::Lease { .. }));
    }

    fn revoke_target(&self, fixture: &IssuerFixture) {
        let history = self.history.lock().unwrap().clone();
        let current_anchor = history
            .v3_entries
            .last()
            .unwrap()
            .record
            .next_anchor
            .clone();
        let protected_time = self.protected_time.load(Ordering::SeqCst);
        let observation =
            v3_observation_for_test(&fixture.verified_v3, &current_anchor, 51, protected_time);
        let current = VerifiedFullSyncIssuerStateV3::from_witness_observation(
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
                expected_head: current.anchor().clone(),
                expected_target_generation: 1,
                request_id: issuer_hash(52),
                command_nonce: issuer_hash(53),
            },
            &key(MANAGER_KEY),
        )
        .unwrap();
        let prepared = current.prepare_management(command, &observation).unwrap();
        let record = prepared.record().unwrap().clone();
        let receipt = SignedFullSyncIssuerReceiptV3::sign(
            FullSyncIssuerReceiptObservationV3::V3(observation),
            &record,
            &key(WITNESS_KEY),
        )
        .unwrap();
        let decision = prepared.commit(receipt).unwrap();
        self.history
            .lock()
            .unwrap()
            .v3_entries
            .push(decision.entry().clone());
    }
}

impl FullSyncIssuerReadEvidenceSource for SignedEvidenceSource {
    fn fetch(
        &self,
        request: &FullSyncIssuerReadEvidenceRequestV3,
    ) -> FormalReadResult<FullSyncIssuerReadEvidenceV3> {
        self.fetches.fetch_add(1, Ordering::SeqCst);
        if *self.unavailable.lock().unwrap() {
            return Err(FormalReadError::NotReady("test witness unavailable".into()));
        }
        let delay = *self.delay.lock().unwrap();
        if !delay.is_zero() {
            std::thread::sleep(delay);
        }
        if Instant::now() > request.deadline {
            return Err(FormalReadError::NotReady(
                "test evidence deadline expired".into(),
            ));
        }
        let history = self.history.lock().unwrap().clone();
        let current_anchor = history
            .v3_entries
            .last()
            .expect("fixture has migration and lease records")
            .record
            .next_anchor
            .clone();
        let nonce = if *self.bad_nonce.lock().unwrap() {
            let mut wrong = request.observation_nonce;
            wrong[0] ^= 1;
            wrong
        } else {
            request.observation_nonce
        };
        let query = SignedFullSyncIssuerQueryV3::sign(
            &self.policy,
            request.minimum_anchor.clone(),
            nonce,
            &key(LEASE_SIGNER_KEY),
        )
        .map_err(|error| FormalReadError::Integrity(error.to_string()))?;
        let time = self.protected_time.load(Ordering::SeqCst);
        let observation = SignedFullSyncIssuerObservationV3::sign(
            FullSyncIssuerObservationBodyV3 {
                schema_version: 3,
                query: query.clone(),
                current_anchor,
                protected_time: FullSyncIssuerProtectedTimeIntervalV3 {
                    earliest_unix_ms: time,
                    latest_unix_ms: time,
                    clock_profile_digest: self.policy.body().clock_profile_digest.clone(),
                },
            },
            &key(WITNESS_KEY),
        )
        .map_err(|error| FormalReadError::Integrity(error.to_string()))?;
        Ok(FullSyncIssuerReadEvidenceV3 {
            query,
            observation,
            history,
        })
    }
}

fn test_gate(
    fixture: &IssuerFixture,
    source: Arc<SignedEvidenceSource>,
    incarnation: u8,
) -> Arc<FullSyncV3ReadGate> {
    Arc::new(
        FullSyncV3ReadGate::new(
            fixture.trust.clone(),
            fixture.signed_v2.clone(),
            fixture.signed_v3.clone(),
            fixture.minimum_v3.clone(),
            public(PROVIDER_KEY),
            issuer_hash(incarnation),
            source,
        )
        .unwrap(),
    )
}

#[test]
fn service_gate_binding_requires_the_original_trust_world_and_service_signer() {
    let (_, trust, _, _, _) = full_sync_fixture();
    let fixture = issue_lease_fixture(trust.initial_trust.clone(), now_ms());
    let source = Arc::new(SignedEvidenceSource::new(&fixture, now_ms()));
    let gate = test_gate(&fixture, source, PROVIDER_INCARNATION);
    let world = oasis7_client_api::world_service::WorldIdentity {
        world_id: fixture.verified_v3.body().world.world_id.clone(),
        genesis_digest: fixture.verified_v3.body().world.genesis_digest.clone(),
    };
    let provider = public(PROVIDER_KEY);
    gate.validate_service_binding(&fixture.trust, &world, &provider)
        .unwrap();

    assert!(
        gate.validate_service_binding(&fixture.trust, &world, &public(88))
            .is_err()
    );
    let mut wrong_trust = fixture.trust.clone();
    wrong_trust.issuer_public_key = public(87);
    assert!(
        gate.validate_service_binding(&wrong_trust, &world, &provider)
            .is_err()
    );
    let wrong_world = oasis7_client_api::world_service::WorldIdentity {
        world_id: "different-world".into(),
        genesis_digest: world.genesis_digest.clone(),
    };
    assert!(
        gate.validate_service_binding(&fixture.trust, &wrong_world, &provider)
            .is_err()
    );
}

#[test]
fn invalid_v1_grant_is_rejected_before_full_history_fetch() {
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
    let mut forged_grant = signed_grant(&reader, base, 1);
    forged_grant.signature_hex = hex::encode([0; 64]);
    let (forged_status, _) =
        post_object(&endpoint, &reader, forged_grant, cut(&anchor), hash.clone());
    assert_eq!(forged_status, 403);
    assert_eq!(
        source.fetch_count(),
        0,
        "bad grant must not trigger witness/history work"
    );

    let (valid_status, _) = post_object(
        &endpoint,
        &reader,
        signed_grant(&reader, base, 1),
        cut(&anchor),
        hash,
    );
    assert_eq!(valid_status, 200);
    assert_eq!(
        source.fetch_count(),
        2,
        "manifest material path checks before and after reads"
    );

    server.stop().unwrap();
    node.lock().unwrap().stop().unwrap();
}

fn transition_payload_not_before(record: &FullSyncIssuerRecordV3) -> u64 {
    match &record.event {
        FullSyncIssuerEventV3::Lease { lease, .. } => lease.body.not_before_unix_ms,
        _ => panic!("expected lease record"),
    }
}

#[test]
fn real_http_full_sync_uses_committed_lease_and_refuses_the_next_chunk_after_revoke() {
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
    let grant = signed_grant(&reader, base, 1);
    let requested_cut = cut(&anchor);
    let (manifest_status, _) =
        post_manifest(&endpoint, &reader, grant.clone(), requested_cut.clone());
    assert_eq!(manifest_status, 200);

    let snapshot_hash = material.roots[&ArtifactRole::Snapshot].clone();
    let (first_chunk_status, _) = post_object_range(
        &endpoint,
        &reader,
        grant.clone(),
        requested_cut.clone(),
        snapshot_hash.clone(),
        0,
        1,
    );
    assert_eq!(first_chunk_status, 200);

    let (second_chunk_status, _) = post_object_range(
        &endpoint,
        &reader,
        grant.clone(),
        requested_cut.clone(),
        snapshot_hash.clone(),
        1,
        1,
    );
    assert_eq!(second_chunk_status, 200);

    source.revoke_target(&fixture);
    let (revoked_chunk_status, body) = post_object_range(
        &endpoint,
        &reader,
        grant,
        requested_cut,
        snapshot_hash,
        2,
        1,
    );
    assert_eq!(revoked_chunk_status, 403);
    let revoked_message = String::from_utf8_lossy(&body);
    assert!(
        revoked_message.contains("V3 target is revoked or inactive"),
        "unexpected denial after committed revoke: {revoked_message}"
    );

    server.stop().unwrap();
    node.lock().unwrap().stop().unwrap();
}

#[test]
fn query_elapsed_is_not_added_to_earliest_but_is_added_to_latest_for_expiry() {
    let (world, trust, anchor, material, proof) = full_sync_fixture();
    let base = now_ms();
    let fixture = issue_lease_fixture(trust.initial_trust.clone(), base);
    let source = Arc::new(SignedEvidenceSource::new(
        &fixture,
        fixture.lease_not_before,
    ));
    source.delay_by(Duration::from_millis(10));
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
    let grant = signed_grant(&reader, base, 1);
    let snapshot_hash = material.roots[&ArtifactRole::Snapshot].clone();
    let (at_not_before, _) = post_object(
        &endpoint,
        &reader,
        grant.clone(),
        cut(&anchor),
        snapshot_hash.clone(),
    );
    assert_eq!(at_not_before, 200, "elapsed must not be added to earliest");

    source.set_time(fixture.lease_expires.saturating_sub(1));
    source.delay_by(Duration::from_millis(10));
    let (past_expiry, _) = post_object(&endpoint, &reader, grant, cut(&anchor), snapshot_hash);
    assert_ne!(
        past_expiry, 200,
        "elapsed must be added to latest for expiry"
    );

    server.stop().unwrap();
    node.lock().unwrap().stop().unwrap();
}

#[test]
fn gate_rejects_wrong_query_nonce_wrong_incarnation_and_unavailable_history() {
    let (world, trust, anchor, material, proof) = full_sync_fixture();
    let base = now_ms();
    let fixture = issue_lease_fixture(trust.initial_trust.clone(), base);
    let source = Arc::new(SignedEvidenceSource::new(&fixture, base + 2_000));
    source.return_bad_nonce(true);
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
    let grant = signed_grant(&reader, base, 1);
    let hash = material.roots[&ArtifactRole::Snapshot].clone();
    let (bad_nonce, _) = post_object(
        &endpoint,
        &reader,
        grant.clone(),
        cut(&anchor),
        hash.clone(),
    );
    assert_ne!(bad_nonce, 200);

    source.return_bad_nonce(false);
    source.set_unavailable(true);
    let (unavailable, _) = post_object(
        &endpoint,
        &reader,
        grant.clone(),
        cut(&anchor),
        hash.clone(),
    );
    assert_ne!(unavailable, 200);
    source.set_unavailable(false);

    // The gate is constructed with an incarnation that has no committed lease.
    // Use a fresh service fixture because the first one was moved into server.
    let (world2, trust2, anchor2, material2, proof2) = full_sync_fixture();
    let fixture2 = issue_lease_fixture(trust2.initial_trust.clone(), base);
    let source2 = Arc::new(SignedEvidenceSource::new(&fixture2, base + 2_000));
    let wrong_incarnation_gate = test_gate(&fixture2, source2, 24);
    let (mut server2, endpoint2, _, node2, _, _) = start_live_service_with_read_gate(
        world2,
        oasis7::world_service::client::FormalReadTrustConfigV1 {
            initial_trust: trust2.initial_trust.clone(),
            checkpoint: anchor2.checkpoint(),
            checkpoint_digest: anchor2.checkpoint().digest().unwrap(),
        },
        anchor2.clone(),
        material2,
        proof2,
        Some(wrong_incarnation_gate),
    );
    let (wrong_incarnation, _) = post_object(
        &endpoint2,
        &reader,
        signed_grant(&reader, base, 1),
        cut(&anchor2),
        hash,
    );
    assert_ne!(wrong_incarnation, 200);

    server.stop().unwrap();
    node.lock().unwrap().stop().unwrap();
    server2.stop().unwrap();
    node2.lock().unwrap().stop().unwrap();
}

#[test]
fn gate_rejects_missing_committed_lease_and_wrong_target_generation() {
    let (world, trust, anchor, material, proof) = full_sync_fixture();
    let base = now_ms();
    let fixture = issue_lease_fixture(trust.initial_trust.clone(), base);
    let source = Arc::new(SignedEvidenceSource::new(&fixture, base + 2_000));
    source.remove_committed_lease();
    let gate = test_gate(&fixture, source, PROVIDER_INCARNATION);
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
    let (missing_lease, _) = post_object(
        &endpoint,
        &reader,
        signed_grant(&reader, base, 1),
        cut(&anchor),
        hash.clone(),
    );
    assert_ne!(missing_lease, 200);
    let (wrong_generation, _) = post_object(
        &endpoint,
        &reader,
        signed_grant(&reader, base, 2),
        cut(&anchor),
        hash,
    );
    assert_ne!(wrong_generation, 200);

    server.stop().unwrap();
    node.lock().unwrap().stop().unwrap();
}

#[path = "formal_full_sync_read_gate_history_tests.rs"]
mod signed_history_tests;

struct FailingEvidenceSource(Mutex<FormalReadError>);

impl FullSyncIssuerReadEvidenceSource for FailingEvidenceSource {
    fn fetch(
        &self,
        _request: &FullSyncIssuerReadEvidenceRequestV3,
    ) -> FormalReadResult<FullSyncIssuerReadEvidenceV3> {
        Err(self.0.lock().unwrap().clone())
    }
}

#[test]
fn full_sync_source_failure_wire_category_ignores_diagnostic_words() {
    let (world, trust, anchor, material, proof) = full_sync_fixture();
    let base = now_ms();
    let fixture = issue_lease_fixture(trust.initial_trust.clone(), base);
    let source = Arc::new(FailingEvidenceSource(Mutex::new(
        FormalReadError::Internal(String::new()),
    )));
    let gate = Arc::new(
        FullSyncV3ReadGate::new(
            fixture.trust.clone(),
            fixture.signed_v2.clone(),
            fixture.signed_v3.clone(),
            fixture.minimum_v3.clone(),
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
    let grant = signed_grant(&reader, base, 1);
    let snapshot_hash = material.roots[&ArtifactRole::Snapshot].clone();
    for diagnostic in [
        "opaque failure",
        "HistoryUnavailable not available not ready grant authorized node",
    ] {
        for (error, expected_status, expected_code) in [
            (
                FormalReadError::Internal(diagnostic.into()),
                400,
                "full_sync_rejected",
            ),
            (
                FormalReadError::Integrity(diagnostic.into()),
                400,
                "full_sync_rejected",
            ),
            (
                FormalReadError::RequestRejected(diagnostic.into()),
                400,
                "full_sync_rejected",
            ),
            (
                FormalReadError::GrantRejected(diagnostic.into()),
                403,
                "read_grant_rejected",
            ),
            (
                FormalReadError::NotReady(diagnostic.into()),
                503,
                "not_ready",
            ),
            (
                FormalReadError::HistoryUnavailable(diagnostic.into()),
                410,
                "history_unavailable",
            ),
        ] {
            *source.0.lock().unwrap() = error;
            for (status, body) in [
                post_manifest(&endpoint, &reader, grant.clone(), cut(&anchor)),
                post_object(
                    &endpoint,
                    &reader,
                    grant.clone(),
                    cut(&anchor),
                    snapshot_hash.clone(),
                ),
            ] {
                assert_eq!(status, expected_status, "{diagnostic}");
                let response: serde_json::Value = serde_json::from_slice(&body).unwrap();
                assert_eq!(response["payload"]["code"], expected_code);
            }
        }
    }
    server.stop().unwrap();
    node.lock().unwrap().stop().unwrap();
}

#[test]
fn full_sync_gate_preserves_response_closure_failure_category() {
    let (_, trust, anchor, _, _) = full_sync_fixture();
    let base = now_ms();
    let fixture = issue_lease_fixture(trust.initial_trust.clone(), base);
    let source = Arc::new(SignedEvidenceSource::new(&fixture, base + 2_000));
    let gate = test_gate(&fixture, source, PROVIDER_INCARNATION);
    let reader = key(READER_KEY);
    let grant = signed_grant(&reader, base, 1);
    for diagnostic in [
        "opaque failure",
        "HistoryUnavailable not ready authorized grant node",
    ] {
        for expected in [
            FormalReadError::Internal(diagnostic.into()),
            FormalReadError::Integrity(diagnostic.into()),
            FormalReadError::HistoryUnavailable(diagnostic.into()),
            FormalReadError::NotReady(diagnostic.into()),
        ] {
            let result = gate.authorize_with_response::<()>(
                &grant,
                &cut(&anchor),
                &public(READER_KEY),
                || Err(expected.clone()),
            );
            assert_eq!(result.unwrap_err(), expected);
        }
    }
}

#[path = "formal_bft_candidate_read_gate_tests.rs"]
mod candidate_read_gate;
