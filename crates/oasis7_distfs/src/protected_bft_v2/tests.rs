use std::io::{Cursor, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use ed25519_dalek::{Signer, SigningKey};
use oasis7_consensus::tendermint_v1::BftTimeoutsV1;
use oasis7_consensus::tendermint_v2::{
    BftHeightProposerScheduleV2, BftSchedulePolicyV2, BftSigningJournalV2,
    verify_commit_certificate_token_v2,
};
use oasis7_proto::distributed_bft::v2::{
    BFT_PROTOCOL_VERSION_V2, BftBlockV2, BftCommitCertificateV2, BftProposalHeaderV2,
    BftProposalV2, BftScopeV2, BftValidatorTransitionRefV2, BftValidatorTransitionV2,
    BftValidatorV2, BftVotePhaseV2, BftVoteV2, BftVoteValueV2,
};

use super::backend::{BftSignerAcquireOutcomeV2, BftSignerBackendSnapshotV2, BftSignerBackendV2};
use super::binding::{
    BftSignerBindingV2, BftSignerErrorV2, BftSignerKeyIdentityV2, BftSignerTrustV2,
    BftSigningKeyHandleV2,
};
use super::client::{BftSignerProviderV2, RemoteBftSignerProviderV2};
use super::install::{
    BftLocalInstallEvidenceV2, BftSignedTransitionInstallAckV2, BftTransitionInstallIntentBodyV2,
    BftTransitionInstallIntentV2, BftTransitionInstallPreparationV2, BftTransitionInstallReceiptV2,
};
use super::service::BftSignerServiceV2;
use super::state::BftSignerStateV2;
use super::wire::{
    BftSignerCommandV2, BftSignerLeaseCredentialV2, BftSignerReplyKindV2, SignedBftSignerReplyV2,
    SignedBftSignerRequestV2, encode_frame, read_frame, validate_reply,
};

mod security;
mod transition_recovery_tests;

struct TestKey(SigningKey);

impl BftSigningKeyHandleV2 for TestKey {
    fn public_key_hex(&self) -> String {
        hex::encode(self.0.verifying_key().to_bytes())
    }

    fn sign_preimage(&self, preimage: &[u8]) -> Result<[u8; 64], BftSignerErrorV2> {
        Ok(self.0.sign(preimage).to_bytes())
    }
}

#[derive(Clone)]
struct SharedBackend(Arc<Mutex<BackendState>>);

struct BackendState {
    binding_digest: String,
    snapshot: BftSignerBackendSnapshotV2,
    return_stale_snapshot_after_next_cas: bool,
    regress_time_on_next_acquire: bool,
    install_attempt_token_but_report_conflict: bool,
}

impl SharedBackend {
    fn snapshot(&self) -> BftSignerBackendSnapshotV2 {
        self.0.lock().expect("backend lock").snapshot.clone()
    }

    fn replace_state(&self, state: BftSignerStateV2) {
        self.0.lock().expect("backend lock").snapshot.state = state;
    }

    fn return_stale_snapshot_after_next_cas(&self) {
        self.0
            .lock()
            .expect("backend lock")
            .return_stale_snapshot_after_next_cas = true;
    }

    fn with_active_lease<T>(
        &self,
        generation: u64,
        token_digest: &str,
        f: impl FnOnce(&mut BackendState) -> Result<T, BftSignerErrorV2>,
    ) -> Result<T, BftSignerErrorV2> {
        let mut state = self
            .0
            .lock()
            .map_err(|_| BftSignerErrorV2("test backend lock poisoned".into()))?;
        if state.snapshot.lease_generation != generation
            || state.snapshot.lease_token_digest != token_digest
            || token_digest.is_empty()
        {
            return Err(BftSignerErrorV2("test lease is fenced".into()));
        }
        f(&mut state)
    }
}

impl BftSignerBackendV2 for SharedBackend {
    fn load_provisioned(
        &mut self,
        binding_digest: &str,
    ) -> Result<BftSignerBackendSnapshotV2, BftSignerErrorV2> {
        let state = self
            .0
            .lock()
            .map_err(|_| BftSignerErrorV2("test backend lock poisoned".into()))?;
        if binding_digest != state.binding_digest {
            return Err(BftSignerErrorV2("wrong binding".into()));
        }
        Ok(state.snapshot.clone())
    }

    fn acquire_lease(
        &mut self,
        binding_digest: &str,
        expected_generation: u64,
        new_lease_token_digest: &str,
    ) -> Result<BftSignerAcquireOutcomeV2, BftSignerErrorV2> {
        let mut state = self
            .0
            .lock()
            .map_err(|_| BftSignerErrorV2("test backend lock poisoned".into()))?;
        if binding_digest != state.binding_digest {
            return Err(BftSignerErrorV2("wrong binding".into()));
        }
        if expected_generation != state.snapshot.lease_generation
            || !state.snapshot.lease_token_digest.is_empty()
        {
            return Ok(BftSignerAcquireOutcomeV2::Conflict(state.snapshot.clone()));
        }
        state.snapshot.lease_generation = state
            .snapshot
            .lease_generation
            .checked_add(1)
            .ok_or_else(|| BftSignerErrorV2("generation overflow".into()))?;
        state.snapshot.lease_token_digest = new_lease_token_digest.to_string();
        if state.install_attempt_token_but_report_conflict {
            state.install_attempt_token_but_report_conflict = false;
            return Ok(BftSignerAcquireOutcomeV2::Conflict(state.snapshot.clone()));
        }
        let mut acquired = state.snapshot.clone();
        if state.regress_time_on_next_acquire {
            state.regress_time_on_next_acquire = false;
            acquired.protected_time_floor_ms = acquired.protected_time_floor_ms.saturating_sub(1);
        }
        Ok(BftSignerAcquireOutcomeV2::Acquired(acquired))
    }

    fn release_lease(
        &mut self,
        expected_generation: u64,
        expected_lease_token_digest: &str,
    ) -> Result<BftSignerBackendSnapshotV2, BftSignerErrorV2> {
        let mut state = self
            .0
            .lock()
            .map_err(|_| BftSignerErrorV2("test backend lock poisoned".into()))?;
        if state.snapshot.lease_generation == expected_generation
            && state.snapshot.lease_token_digest == expected_lease_token_digest
        {
            state.snapshot.lease_generation = state
                .snapshot
                .lease_generation
                .checked_add(1)
                .ok_or_else(|| BftSignerErrorV2("generation overflow".into()))?;
            state.snapshot.lease_token_digest.clear();
        }
        Ok(state.snapshot.clone())
    }

    fn read_for_lease(
        &mut self,
        lease_generation: u64,
        lease_token_digest: &str,
    ) -> Result<BftSignerBackendSnapshotV2, BftSignerErrorV2> {
        self.with_active_lease(lease_generation, lease_token_digest, |state| {
            Ok(state.snapshot.clone())
        })
    }

    fn protected_time_ms(
        &mut self,
        lease_generation: u64,
        lease_token_digest: &str,
    ) -> Result<i64, BftSignerErrorV2> {
        self.with_active_lease(lease_generation, lease_token_digest, |state| {
            state.snapshot.protected_time_floor_ms = state
                .snapshot
                .protected_time_floor_ms
                .checked_add(10)
                .ok_or_else(|| BftSignerErrorV2("clock overflow".into()))?;
            Ok(state.snapshot.protected_time_floor_ms)
        })
    }

    fn compare_and_swap(
        &mut self,
        lease_generation: u64,
        lease_token_digest: &str,
        expected_revision: u64,
        expected_state_digest: &str,
        minimum_event_time_ms: Option<i64>,
        next_state: BftSignerStateV2,
    ) -> Result<BftSignerBackendSnapshotV2, BftSignerErrorV2> {
        self.with_active_lease(lease_generation, lease_token_digest, |state| {
            if state.snapshot.state.revision != expected_revision
                || state.snapshot.state.state_digest != expected_state_digest
                || next_state.revision != expected_revision + 1
            {
                return Err(BftSignerErrorV2("CAS mismatch".into()));
            }
            if let Some(event_time) = minimum_event_time_ms {
                state.snapshot.protected_time_floor_ms =
                    state.snapshot.protected_time_floor_ms.max(event_time);
            }
            let old_snapshot = state.snapshot.clone();
            state.snapshot.state = next_state;
            if state.return_stale_snapshot_after_next_cas {
                state.return_stale_snapshot_after_next_cas = false;
                let mut stale = old_snapshot;
                stale.lease_generation = state.snapshot.lease_generation;
                stale.lease_token_digest = state.snapshot.lease_token_digest.clone();
                stale.protected_time_floor_ms = state.snapshot.protected_time_floor_ms;
                return Ok(stale);
            }
            Ok(state.snapshot.clone())
        })
    }

    fn complete_transition_install(
        &mut self,
        lease_generation: u64,
        lease_token_digest: &str,
        expected_revision: u64,
        expected_state_digest: &str,
        minimum_event_time_ms: i64,
        next_lease_token_digest: &str,
        next_state: BftSignerStateV2,
    ) -> Result<BftSignerBackendSnapshotV2, BftSignerErrorV2> {
        self.with_active_lease(lease_generation, lease_token_digest, |state| {
            if state.snapshot.state.revision != expected_revision
                || state.snapshot.state.state_digest != expected_state_digest
                || next_state.revision != expected_revision + 1
            {
                return Err(BftSignerErrorV2("install CAS mismatch".into()));
            }
            state.snapshot.protected_time_floor_ms = state
                .snapshot
                .protected_time_floor_ms
                .max(minimum_event_time_ms);
            state.snapshot.state = next_state;
            state.snapshot.lease_generation = lease_generation + 1;
            state.snapshot.lease_token_digest = next_lease_token_digest.to_string();
            Ok(state.snapshot.clone())
        })
    }
}

struct Harness {
    binding: BftSignerBindingV2,
    requester: SigningKey,
    provider: SigningKey,
    validator: SigningKey,
    trust: BftSignerTrustV2,
    service: Arc<BftSignerServiceV2>,
    backend: SharedBackend,
}

fn harness() -> Harness {
    let requester = key(1);
    let provider = key(2);
    let validator = key(3);
    let scope = BftScopeV2 {
        world_id: "world-test".into(),
        chain_id: "chain-test".into(),
        genesis_hash: hash(4),
    };
    let binding = BftSignerBindingV2 {
        schema_version: 2,
        scope: scope.clone(),
        genesis_authority_epoch: 1,
        genesis_validators: vec![BftValidatorV2 {
            validator_id: "validator-a".into(),
            public_key_hex: hex::encode(validator.verifying_key().to_bytes()),
            voting_power: 1,
        }],
        genesis_state_root: hash(5),
        genesis_manifest_hash: hash(6),
        schedule_policy: BftSchedulePolicyV2::new(10, "profile-test").unwrap(),
        local_validator_id: "validator-a".into(),
        provisioned_signing_keys: vec![BftSignerKeyIdentityV2 {
            validator_id: "validator-a".into(),
            public_key_hex: hex::encode(validator.verifying_key().to_bytes()),
        }],
        timeouts: BftTimeoutsV1 {
            propose_ms: 10,
            prevote_ms: 10,
            precommit_ms: 10,
            round_delta_ms: 10,
        },
        operator_instance_id: "runtime-instance-a".into(),
        requester_auth_public_key: hex::encode(requester.verifying_key().to_bytes()),
    };
    let binding_digest = binding.digest().unwrap();
    let state = BftSignerStateV2::genesis(&binding).unwrap();
    let backend = SharedBackend(Arc::new(Mutex::new(BackendState {
        binding_digest: binding_digest.clone(),
        snapshot: BftSignerBackendSnapshotV2 {
            state,
            lease_generation: 1,
            lease_token_digest: String::new(),
            protected_time_floor_ms: 100,
        },
        return_stale_snapshot_after_next_cas: false,
        regress_time_on_next_acquire: false,
        install_attempt_token_but_report_conflict: false,
    })));
    let provider_handle = TestKey(provider.clone());
    let validator_handle = TestKey(validator.clone());
    let service = Arc::new(
        BftSignerServiceV2::open(
            binding.clone(),
            Box::new(provider_handle),
            vec![Box::new(validator_handle)],
            Box::new(backend.clone()),
        )
        .unwrap(),
    );
    let trust = BftSignerTrustV2 {
        schema_version: 2,
        provider_public_key: hex::encode(provider.verifying_key().to_bytes()),
        binding_digest,
    };
    Harness {
        binding,
        requester,
        provider,
        validator,
        trust,
        service,
        backend,
    }
}

fn installable_harness() -> Harness {
    let mut fixture = harness();
    fixture.binding.schedule_policy = BftSchedulePolicyV2::new(4, "profile-test").unwrap();
    let (history, schedule) = transition_history(&fixture.binding, &fixture.validator);
    let journal = BftSigningJournalV2::for_schedule(
        &schedule,
        fixture.binding.local_validator_id.clone(),
        hex::encode(fixture.validator.verifying_key().to_bytes()),
    )
    .unwrap();
    let state = BftSignerStateV2::from_candidate(
        &fixture.binding,
        0,
        history,
        Some(journal),
        None,
        Vec::new(),
        Some(&hex::encode(fixture.provider.verifying_key().to_bytes())),
    )
    .unwrap();
    let binding_digest = fixture.binding.digest().unwrap();
    fixture.backend.0.lock().unwrap().binding_digest = binding_digest.clone();
    fixture.backend.0.lock().unwrap().snapshot.state = state;
    fixture.trust.binding_digest = binding_digest;
    fixture.service = Arc::new(
        BftSignerServiceV2::open(
            fixture.binding.clone(),
            Box::new(TestKey(fixture.provider.clone())),
            vec![Box::new(TestKey(fixture.validator.clone()))],
            Box::new(fixture.backend.clone()),
        )
        .unwrap(),
    );
    fixture
}

fn transition_history(
    binding: &BftSignerBindingV2,
    validator_key: &SigningKey,
) -> (Vec<BftCommitCertificateV2>, BftHeightProposerScheduleV2) {
    let mut schedule = BftHeightProposerScheduleV2::genesis(
        binding.validate().unwrap(),
        binding.schedule_policy.clone(),
        binding.genesis_authority_epoch,
        binding.genesis_state_root.clone(),
        binding.genesis_manifest_hash.clone(),
    )
    .unwrap();
    let mut history = Vec::new();
    let mut confirmation_ref: Option<BftValidatorTransitionRefV2> = None;
    while schedule.height() <= 7 {
        let transition = (schedule.height() == 6).then(|| {
            let validator = BftValidatorV2 {
                validator_id: binding.local_validator_id.clone(),
                public_key_hex: hex::encode(validator_key.verifying_key().to_bytes()),
                voting_power: 2,
            };
            BftValidatorTransitionV2 {
                source_commit_ref: schedule.parent_commit_ref().to_string(),
                source_state_root: schedule.parent_state_root().to_string(),
                source_profile: schedule.policy().profile_id.clone(),
                source_epoch: schedule.parent_tick() / schedule.policy().epoch_length_ticks,
                target_epoch: 2,
                epoch_length_ticks: schedule.policy().epoch_length_ticks,
                mapping_ruleset_hash: schedule.policy_hash().to_string(),
                scheduled_height: 6,
                activation_height: 8,
                old_validator_set_hash: schedule.active_set().set_hash().to_string(),
                new_validators: vec![validator.clone()],
                new_validator_set_hash: oasis7_consensus::tendermint_v2::BftValidatorSetV2::new(
                    schedule.scope().clone(),
                    vec![validator],
                )
                .unwrap()
                .set_hash()
                .to_string(),
                governance_action_digest: hash(20),
                projected_registry_digest: hash(21),
            }
        });
        let transition_ref = if schedule.height() == 7 {
            confirmation_ref.clone()
        } else {
            None
        };
        let block = BftBlockV2 {
            protocol_version: BFT_PROTOCOL_VERSION_V2,
            scope: schedule.scope().clone(),
            height: schedule.height(),
            parent_commit_ref: schedule.parent_commit_ref().to_string(),
            authority_epoch: schedule.authority_epoch(),
            active_validator_set_hash: schedule.active_set().set_hash().to_string(),
            schedule_policy_hash: schedule.policy_hash().to_string(),
            manifest_hash: schedule.parent_manifest_hash().to_string(),
            post_manifest_hash: hash(22),
            actions_cbor: vec![0x80],
            action_root: hash(schedule.height() as u8),
            event_root: hash(schedule.height() as u8 + 1),
            execution_block_hash: hash(schedule.height() as u8 + 2),
            execution_state_root: hash(schedule.height() as u8 + 3),
            state_root: hash(schedule.height() as u8 + 4),
            transition: transition.clone(),
            transition_ref,
        };
        if let Some(transition) = transition {
            confirmation_ref = Some(
                transition
                    .committed_reference(block.block_id().unwrap())
                    .unwrap(),
            );
        }
        let proposer_id = schedule.expected_proposer_for_round(0).unwrap();
        assert_eq!(proposer_id, binding.local_validator_id);
        let block_id = block.block_id().unwrap();
        let header = BftProposalHeaderV2::sign(
            block.scope.clone(),
            block.height,
            0,
            schedule.active_set().set_hash().to_string(),
            schedule.policy_hash().to_string(),
            proposer_id,
            block_id.clone(),
            None,
            validator_key,
        )
        .unwrap();
        let proposal = BftProposalV2 {
            header,
            block,
            valid_round_evidence: None,
        };
        let value = BftVoteValueV2::Block { block_id };
        let precommit = BftVoteV2::sign(
            schedule.scope().clone(),
            schedule.height(),
            0,
            BftVotePhaseV2::Precommit,
            schedule.active_set().set_hash().to_string(),
            schedule.policy_hash().to_string(),
            value,
            binding.local_validator_id.clone(),
            validator_key,
        )
        .unwrap();
        let certificate = BftCommitCertificateV2 {
            proposal: Box::new(proposal),
            precommits: vec![precommit],
        };
        let verified = verify_commit_certificate_token_v2(&schedule, &certificate).unwrap();
        history.push(certificate);
        schedule = verified.next_height_schedule().clone();
    }
    assert_eq!(schedule.height(), 8);
    (history, schedule)
}

fn key(seed: u8) -> SigningKey {
    SigningKey::from_bytes(&[seed; 32])
}

fn hash(byte: u8) -> String {
    hex::encode([byte; 32])
}

fn spawn_tcp_server(
    service: Arc<BftSignerServiceV2>,
    connections: usize,
) -> (SocketAddr, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let join = thread::spawn(move || {
        let mut workers = Vec::with_capacity(connections);
        for _ in 0..connections {
            let (mut stream, _) = listener.accept().unwrap();
            let service = service.clone();
            workers.push(thread::spawn(move || {
                service
                    .serve_connection(&mut stream)
                    .expect("test signer TCP connection should close cleanly");
            }));
        }
        for worker in workers {
            worker.join().unwrap();
        }
    });
    (address, join)
}

#[test]
fn canonical_cbor_frame_round_trips_without_changing_payload_bytes() {
    let fixture = harness();
    let request = SignedBftSignerRequestV2::sign(
        &fixture.binding,
        &fixture.requester,
        None,
        BftSignerCommandV2::PrepareAcquire {
            binding: Box::new(fixture.binding.clone()),
        },
        hex::encode([25; 32]),
    )
    .unwrap();

    let frame = encode_frame(&request).unwrap();
    let payload_len = u32::from_be_bytes(frame[..4].try_into().unwrap()) as usize;
    let canonical_payload = oasis7_cas::to_canonical_cbor(&request).unwrap();
    assert_eq!(payload_len, frame.len() - 4);
    assert_eq!(&frame[4..], canonical_payload.as_slice());
    assert_eq!(&frame[4..7], &[0xd9, 0xd9, 0xf7]);

    let decoded: SignedBftSignerRequestV2 = read_frame(&mut Cursor::new(frame.clone())).unwrap();
    assert_eq!(decoded, request);
    assert_eq!(encode_frame(&decoded).unwrap(), frame);
}

#[test]
fn frame_decoder_rejects_trailing_bytes_inside_declared_payload() {
    let fixture = harness();
    let request = SignedBftSignerRequestV2::sign(
        &fixture.binding,
        &fixture.requester,
        None,
        BftSignerCommandV2::PrepareAcquire {
            binding: Box::new(fixture.binding.clone()),
        },
        hex::encode([26; 32]),
    )
    .unwrap();
    let mut payload = oasis7_cas::to_canonical_cbor(&request).unwrap();
    payload.push(0xf6);
    let payload_len = u32::try_from(payload.len()).unwrap();
    let mut frame = payload_len.to_be_bytes().to_vec();
    frame.extend_from_slice(&payload);

    let error = read_frame::<SignedBftSignerRequestV2>(&mut Cursor::new(frame)).unwrap_err();
    assert!(error.0.contains("trailing bytes in BFT V2 CBOR"), "{error}");
}

#[test]
fn remote_provider_acquires_polls_and_releases_a_real_tcp_session() {
    let fixture = harness();
    let (address, server) = spawn_tcp_server(fixture.service.clone(), 1);
    let provider = RemoteBftSignerProviderV2::new(
        address,
        Duration::from_secs(3),
        fixture.binding.clone(),
        fixture.requester.clone(),
        fixture.trust.clone(),
    )
    .unwrap();
    let mut lease = provider.acquire(&fixture.binding).unwrap();
    let initial = lease.checkpoint().unwrap();
    assert_eq!(initial.lease_generation, 2);
    assert_eq!(initial.state.revision, 0);
    assert!(lease.protected_time_ms().unwrap() > 100);
    let timed = lease.start_height_timer(initial.state.revision).unwrap();
    assert_eq!(timed.state.revision, 1);
    assert!(
        timed
            .state
            .journal
            .as_ref()
            .and_then(|journal| journal.height_started_at_ms())
            .is_some()
    );
    drop(lease);
    server.join().unwrap();

    let stored = fixture.backend.snapshot();
    assert_eq!(stored.lease_generation, 3);
    assert!(stored.lease_token_digest.is_empty());
    assert_eq!(stored.state.revision, 1);
}

#[test]
fn pending_transition_install_survives_disconnect_reacquire_and_exact_retry() {
    let fixture = installable_harness();
    let (address, server) = spawn_tcp_server(fixture.service.clone(), 3);
    let provider = RemoteBftSignerProviderV2::new(
        address,
        Duration::from_secs(3),
        fixture.binding.clone(),
        fixture.requester.clone(),
        fixture.trust.clone(),
    )
    .unwrap();

    let mut first_lease = provider.acquire(&fixture.binding).unwrap();
    let before = first_lease.checkpoint().unwrap();
    let request_id = "install-request-recovery".to_string();
    let first_preparation = first_lease
        .prepare_transition_install(
            before.state.revision,
            &before.state.state_digest,
            request_id.clone(),
            6,
            fixture.binding.operator_instance_id.clone(),
        )
        .unwrap();
    let BftTransitionInstallPreparationV2::Pending {
        intent: original_intent,
        checkpoint: pending_checkpoint,
    } = first_preparation
    else {
        panic!("first install preparation must create a pending intent");
    };
    assert_eq!(pending_checkpoint.state.revision, before.state.revision + 1);
    drop(first_lease);
    // Closing the client socket does not synchronously release the server lease.
    security::wait_for_released_generation(&fixture, pending_checkpoint.lease_generation + 1);

    let mut recovered_lease = provider.acquire(&fixture.binding).unwrap();
    let recovered_checkpoint = recovered_lease.checkpoint().unwrap();
    assert_eq!(
        recovered_checkpoint.state.pending_install,
        Some(original_intent.clone())
    );
    assert!(recovered_checkpoint.lease_generation > original_intent.body.lease_generation);
    let retried_preparation = recovered_lease
        .prepare_transition_install(
            before.state.revision,
            &before.state.state_digest,
            request_id,
            6,
            fixture.binding.operator_instance_id.clone(),
        )
        .unwrap();
    let BftTransitionInstallPreparationV2::Pending {
        intent: retried_intent,
        checkpoint: retry_checkpoint,
    } = retried_preparation
    else {
        panic!("recovered pending request must return its original intent");
    };
    assert_eq!(retried_intent, original_intent);
    assert_eq!(
        retry_checkpoint.state.revision,
        recovered_checkpoint.state.revision
    );

    let evidence = install_evidence(&original_intent);
    let completed = recovered_lease
        .complete_transition_install(original_intent.clone(), evidence.clone())
        .unwrap();
    let acknowledgement = BftSignedTransitionInstallAckV2::sign(
        &original_intent,
        evidence.clone(),
        &fixture.binding,
        &fixture.trust.provider_public_key,
        &fixture.requester,
    )
    .unwrap();
    let intent_digest = original_intent.digest().unwrap();
    let acknowledgement_digest = acknowledgement.digest().unwrap();
    super::client::validate_install_completion_receipt(
        &completed.receipt,
        &original_intent,
        &acknowledgement,
        &evidence,
        &completed.checkpoint,
        &fixture.binding,
        &fixture.trust.provider_public_key,
        &intent_digest,
        &acknowledgement_digest,
        true,
    )
    .unwrap();
    let mut substituted_body = completed.receipt.body.clone();
    substituted_body.request_id = "another-valid-install-request".into();
    substituted_body.acknowledgement_digest = hash(99);
    let substituted_receipt =
        BftTransitionInstallReceiptV2::sign(substituted_body, &TestKey(fixture.provider.clone()))
            .unwrap();
    let mut substituted_checkpoint = completed.checkpoint.clone();
    substituted_checkpoint
        .state
        .install_receipts
        .push(substituted_receipt.clone());
    assert!(
        super::client::validate_install_completion_receipt(
            &substituted_receipt,
            &original_intent,
            &acknowledgement,
            &evidence,
            &substituted_checkpoint,
            &fixture.binding,
            &fixture.trust.provider_public_key,
            &intent_digest,
            &acknowledgement_digest,
            true,
        )
        .is_err()
    );
    assert_eq!(
        completed.checkpoint.state.revision,
        retry_checkpoint.state.revision + 1
    );
    assert!(completed.checkpoint.state.pending_install.is_none());
    assert_eq!(
        completed.checkpoint.state.install_receipts,
        vec![completed.receipt.clone()]
    );
    drop(recovered_lease);

    let mut final_lease = provider.acquire(&fixture.binding).unwrap();
    let final_checkpoint = final_lease.checkpoint().unwrap();
    let completed_prepare = final_lease
        .prepare_transition_install(
            before.state.revision,
            &before.state.state_digest,
            "install-request-recovery".into(),
            6,
            fixture.binding.operator_instance_id.clone(),
        )
        .unwrap();
    let BftTransitionInstallPreparationV2::Completed {
        receipt: recovered_receipt,
        checkpoint: receipt_checkpoint,
    } = completed_prepare
    else {
        panic!("completed request retry must return its original receipt");
    };
    assert_eq!(recovered_receipt, completed.receipt);
    assert_eq!(receipt_checkpoint.state, final_checkpoint.state);
    assert_eq!(
        receipt_checkpoint.lease_generation,
        final_checkpoint.lease_generation
    );
    let exact_completion_retry = final_lease
        .complete_transition_install(original_intent, evidence)
        .unwrap();
    assert_eq!(exact_completion_retry.receipt, completed.receipt);
    assert_eq!(
        exact_completion_retry.checkpoint.state,
        final_checkpoint.state
    );
    assert_eq!(
        exact_completion_retry.checkpoint.lease_generation,
        final_checkpoint.lease_generation
    );
    drop(final_lease);
    server.join().unwrap();

    let stored = fixture.backend.snapshot();
    assert_eq!(stored.state.install_receipts, vec![completed.receipt]);
    assert!(stored.state.pending_install.is_none());
    assert!(stored.lease_token_digest.is_empty());
}

fn install_evidence(intent: &BftTransitionInstallIntentV2) -> BftLocalInstallEvidenceV2 {
    BftLocalInstallEvidenceV2 {
        schema_version: 2,
        runtime_instance_id: intent.body.runtime_instance_id.clone(),
        installed_head_height: intent.body.install_head_height,
        installed_head_commit_ref: intent.body.install_parent_commit_ref.clone(),
        installed_state_root: intent.body.install_state_root.clone(),
        installed_manifest_hash: intent.body.install_manifest_hash.clone(),
        installed_active_set_hash: intent.body.install_active_set_hash.clone(),
        installed_authority_epoch: intent.body.target_authority_epoch,
        local_validator_id: intent.body.local_validator_id.clone(),
        local_signing_public_key: intent.body.local_signing_public_key.clone(),
        wal_install_digest: hash(23),
        driver_install_digest: hash(24),
    }
}

#[test]
fn a_lease_frame_cannot_move_to_another_real_tcp_connection() {
    let fixture = harness();
    let (address, server) = spawn_tcp_server(fixture.service.clone(), 2);
    let mut first = TcpStream::connect(address).unwrap();
    first
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    first
        .set_write_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let (_, prepare_reply) = direct_exchange(
        &mut first,
        &fixture.binding,
        &fixture.requester,
        None,
        BftSignerCommandV2::PrepareAcquire {
            binding: Box::new(fixture.binding.clone()),
        },
        11,
        &fixture.trust,
    );
    assert!(matches!(
        &prepare_reply.body.result,
        BftSignerReplyKindV2::Prepared { .. }
    ));
    let BftSignerReplyKindV2::Prepared {
        expected_generation,
        prepare_nonce,
    } = prepare_reply.body.result
    else {
        unreachable!();
    };
    let (_acquire, acquire_reply) = direct_exchange(
        &mut first,
        &fixture.binding,
        &fixture.requester,
        None,
        BftSignerCommandV2::Acquire {
            binding: Box::new(fixture.binding.clone()),
            expected_generation,
            prepare_nonce,
        },
        12,
        &fixture.trust,
    );
    assert!(matches!(
        acquire_reply.body.result,
        BftSignerReplyKindV2::Acquired
    ));
    let token = acquire_reply.body.lease_token.unwrap();
    let credential = BftSignerLeaseCredentialV2 {
        generation: acquire_reply.body.lease_generation,
        token,
    };

    let mut second = TcpStream::connect(address).unwrap();
    second
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    second
        .set_write_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let (_, moved_reply) = direct_exchange(
        &mut second,
        &fixture.binding,
        &fixture.requester,
        Some(credential.clone()),
        BftSignerCommandV2::Checkpoint,
        13,
        &fixture.trust,
    );
    assert!(matches!(
        moved_reply.body.result,
        BftSignerReplyKindV2::Rejected { .. }
    ));
    let (_, original_reply) = direct_exchange(
        &mut first,
        &fixture.binding,
        &fixture.requester,
        Some(credential),
        BftSignerCommandV2::ProtectedTime,
        14,
        &fixture.trust,
    );
    assert!(matches!(
        original_reply.body.result,
        BftSignerReplyKindV2::ProtectedTime { .. }
    ));
    drop(second);
    drop(first);
    server.join().unwrap();
    let stored = fixture.backend.snapshot();
    assert_eq!(stored.lease_generation, 3);
    assert!(stored.lease_token_digest.is_empty());
}

fn direct_exchange(
    stream: &mut TcpStream,
    binding: &BftSignerBindingV2,
    requester: &SigningKey,
    lease: Option<BftSignerLeaseCredentialV2>,
    command: BftSignerCommandV2,
    challenge_byte: u8,
    trust: &BftSignerTrustV2,
) -> (SignedBftSignerRequestV2, SignedBftSignerReplyV2) {
    let request = SignedBftSignerRequestV2::sign(
        binding,
        requester,
        lease,
        command,
        hex::encode([challenge_byte; 32]),
    )
    .unwrap();
    stream.write_all(&encode_frame(&request).unwrap()).unwrap();
    let reply: SignedBftSignerReplyV2 = read_frame(stream).unwrap();
    validate_reply(binding, trust, &request, &reply).unwrap();
    (request, reply)
}

#[test]
fn install_evidence_rejects_another_provisioned_rotation_key() {
    let mut fixture = harness();
    let other_validator_key = key(7);
    let other_public_key = hex::encode(other_validator_key.verifying_key().to_bytes());
    fixture
        .binding
        .provisioned_signing_keys
        .push(BftSignerKeyIdentityV2 {
            validator_id: fixture.binding.local_validator_id.clone(),
            public_key_hex: other_public_key.clone(),
        });
    fixture
        .binding
        .provisioned_signing_keys
        .sort_by(|left, right| left.public_key_hex.cmp(&right.public_key_hex));
    let body = BftTransitionInstallIntentBodyV2 {
        schema_version: 2,
        binding_digest: fixture.binding.digest().unwrap(),
        expected_revision: 1,
        expected_state_digest: hash(8),
        lease_generation: 2,
        request_id: "rotation-install-1".into(),
        intent_nonce: hash(9),
        runtime_instance_id: fixture.binding.operator_instance_id.clone(),
        transition_source_height: 1,
        transition_source_parent_commit_ref: hash(10),
        transition_block_id: hash(11),
        transition_digest: hash(12),
        confirmation_height: 2,
        confirmation_block_id: hash(13),
        activation_height: 3,
        install_head_height: 2,
        old_validator_set_hash: hash(14),
        new_validator_set_hash: hash(15),
        install_active_set_hash: hash(15),
        target_authority_epoch: 2,
        install_parent_commit_ref: hash(13),
        install_state_root: hash(16),
        install_manifest_hash: hash(17),
        local_validator_id: fixture.binding.local_validator_id.clone(),
        local_signing_public_key: hex::encode(fixture.validator.verifying_key().to_bytes()),
    };
    let intent = BftTransitionInstallIntentV2 {
        body,
        provider_public_key: hex::encode(fixture.provider.verifying_key().to_bytes()),
        provider_signature_hex: String::new(),
    };
    let evidence = |key: String| BftLocalInstallEvidenceV2 {
        schema_version: 2,
        runtime_instance_id: fixture.binding.operator_instance_id.clone(),
        installed_head_height: intent.body.install_head_height,
        installed_head_commit_ref: hash(13),
        installed_state_root: hash(16),
        installed_manifest_hash: hash(17),
        installed_active_set_hash: intent.body.install_active_set_hash.clone(),
        installed_authority_epoch: 2,
        local_validator_id: fixture.binding.local_validator_id.clone(),
        local_signing_public_key: key,
        wal_install_digest: hash(18),
        driver_install_digest: hash(19),
    };
    let target_key = intent.body.local_signing_public_key.clone();
    assert!(
        evidence(target_key)
            .validate_for(&intent, &fixture.binding)
            .is_ok()
    );
    assert!(
        evidence(other_public_key)
            .validate_for(&intent, &fixture.binding)
            .is_err()
    );
}
