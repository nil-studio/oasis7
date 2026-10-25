use super::*;
use crate::node_bft_protected_test_provider::*;
use oasis7_distfs::protected_bft::*;
use std::time::Duration;

struct ProviderFixture {
    bft: crate::NodeBftConfigV1,
    binding: BftSignerBindingV1,
    backend: MemoryBackend,
    provider: Arc<RemoteBftSignerProviderV1>,
    _server: TcpServiceServer,
}

impl ProviderFixture {
    fn new() -> Self {
        let validator = SigningKey::from_bytes(&[71; 32]);
        let requester = SigningKey::from_bytes(&[72; 32]);
        let provider_key = SigningKey::from_bytes(&[73; 32]);
        let bft = bft_config(&validator);
        let binding = BftSignerBindingV1 {
            schema_version: 1,
            scope: bft.scope.clone(),
            authority_epoch: bft.authority_epoch,
            validators: bft.validators.clone(),
            local_validator_id: bft.local_validator_id.clone(),
            local_signing_public_key: hex::encode(validator.verifying_key().to_bytes()),
            timeouts: bft.timeouts,
            operator_instance_id: "node-tcp-provider-fixture".into(),
            requester_auth_public_key: hex::encode(requester.verifying_key().to_bytes()),
        };
        let backend = MemoryBackend::provision(&binding).expect("provision test storage");
        let trust = BftSignerTrustV1 {
            schema_version: 1,
            binding_digest: binding.digest().unwrap(),
            provider_public_key: hex::encode(provider_key.verifying_key().to_bytes()),
        };
        let service = Arc::new(
            BftSignerServiceV1::open(
                binding.clone(),
                Box::new(SoftwareKey(provider_key)),
                Box::new(SoftwareKey(validator)),
                Box::new(backend.clone()),
            )
            .expect("open provisioned test service"),
        );
        let server = TcpServiceServer::start(service);
        let provider = Arc::new(
            RemoteBftSignerProviderV1::new(
                server.address,
                Duration::from_secs(2),
                binding.clone(),
                requester,
                trust,
            )
            .expect("authenticated TCP provider"),
        );
        Self {
            bft,
            binding,
            backend,
            provider,
            _server: server,
        }
    }

    fn worker(&self, root: &TempRoot) -> Worker {
        let set = self.binding.validate().unwrap();
        let schedule =
            BftHeightProposerScheduleV1::genesis(&set, self.bft.authority_epoch).unwrap();
        let wal = BftWalStore::open_protected_for_recovery(
            &root.path().join("replication/tendermint-v1"),
            &schedule,
            &self.binding,
            false,
            self.provider.as_ref(),
            &[],
        )
        .expect("recover authenticated provider journal");
        Worker::restore(WorkerSetup {
            config: node_config(),
            bft: self.bft.clone(),
            set,
            schedule,
            key: None,
            hook: Arc::new(Mutex::new(Box::new(ProposalHook {
                governance_authorized: Arc::new(AtomicBool::new(true)),
                history_page_calls: Arc::new(AtomicUsize::new(0)),
                history_page_error: None,
            }))),
            wal,
            pending: Arc::new(Mutex::new(Vec::new())),
            pending_bytes: Arc::new(AtomicUsize::new(0)),
            state: Arc::new(Mutex::new(crate::node_runtime_core::RuntimeState::default())),
            tip: None,
        })
        .expect("restore keyless protected worker")
    }
}

#[test]
fn real_worker_proposal_and_votes_use_tcp_signer_without_local_key() {
    let root = TempRoot::new();
    let fixture = ProviderFixture::new();
    let mut worker = fixture.worker(&root);
    assert!(worker.setup.key.is_none());
    let origin = worker.setup.wal.state().height_started_at_ms.unwrap();
    assert!(
        origin < 10_000,
        "provider clock, not Unix wall time, owns the origin"
    );
    worker
        .maybe_propose(None)
        .expect("provider signs real worker proposal");
    let result = worker.finish_effects(None);
    assert!(
        result.is_err(),
        "fixture deliberately stops canonical installation"
    );
    let journal = worker.setup.wal.state();
    let mut proposal_count = 0;
    let mut phases = Vec::new();
    for event in &journal.events {
        match event {
            BftWalEventV1::Proposal {
                proposal,
                local: true,
                now_ms,
                ..
            } => {
                proposal.header.verify_signature().unwrap();
                assert!(*now_ms >= origin && *now_ms < 10_000);
                proposal_count += 1;
            }
            BftWalEventV1::Vote {
                vote,
                local: true,
                now_ms,
            } => {
                vote.verify_signature().unwrap();
                assert!(*now_ms >= origin && *now_ms < 10_000);
                phases.push(vote.phase);
            }
            _ => {}
        }
    }
    assert_eq!(proposal_count, 1);
    assert_eq!(
        phases,
        vec![BftVotePhaseV1::Prevote, BftVotePhaseV1::Precommit]
    );
    assert!(journal.pending_commit.is_some());
    let ledger = fixture.backend.0.lock().unwrap();
    assert_eq!(
        ledger.state.journal, *journal,
        "provider journal matches durable mirror"
    );
    drop(ledger);
    drop(worker);
}

#[test]
fn old_local_mirror_recovers_provider_lock_and_refuses_local_downgrade() {
    let root = TempRoot::new();
    let fixture = ProviderFixture::new();
    let mut worker = fixture.worker(&root);
    let directory = root.path().join("replication/tendermint-v1");
    let mirror = directory.join("tendermint-v1.protected.cbor");
    let old = fs::read(&mirror).expect("initial protected mirror");
    worker.maybe_propose(None).unwrap();
    assert!(worker.finish_effects(None).is_err());
    let original = worker.setup.wal.state().clone();
    assert!(worker.machine.locked().is_some());
    drop(worker);
    fs::write(&mirror, old).unwrap();
    let mut recovered = fixture.worker(&root);
    assert_eq!(recovered.setup.wal.state(), &original);
    assert!(recovered.machine.locked().is_some());
    let before = recovered.setup.wal.state().events.len();
    assert!(
        recovered.finish_effects(None).is_err(),
        "fixture still refuses canonical installation"
    );
    assert_eq!(recovered.setup.wal.state().events.len(), before);
    drop(recovered);
    let set = fixture.binding.validate().unwrap();
    let schedule = BftHeightProposerScheduleV1::genesis(&set, fixture.bft.authority_epoch).unwrap();
    assert!(
        BftWalStore::open_for_recovery(
            &directory,
            &schedule,
            VALIDATOR_ID,
            &fixture.binding.local_signing_public_key,
            false,
        )
        .is_err(),
        "protected state cannot open with a local signer"
    );
}

#[test]
fn replacement_provider_lease_stops_the_original_worker_before_more_signatures() {
    let root = TempRoot::new();
    let fixture = ProviderFixture::new();
    let mut worker = fixture.worker(&root);
    let original_revision = fixture.backend.0.lock().unwrap().state.revision;
    let replacement = fixture.provider.acquire(&fixture.binding).unwrap();
    assert!(
        worker.now().is_err(),
        "fenced worker cannot obtain a signing clock"
    );
    assert!(
        worker.maybe_propose(None).is_err(),
        "fenced worker cannot reserve or sign"
    );
    assert_eq!(
        fixture.backend.0.lock().unwrap().state.revision,
        original_revision
    );
    drop(replacement);
    drop(worker);
}

#[test]
fn worker_recovers_original_signed_proposal_after_tcp_reply_loss() {
    let root = TempRoot::new();
    let fixture = ProviderFixture::new();
    let mut worker = fixture.worker(&root);
    let context = NodeBftExecutionContextV1 {
        scope: fixture.bft.scope.clone(),
        height: 1,
        parent_commit_ref: GENESIS_HASH.into(),
        authority_epoch: 1,
        manifest_hash: MANIFEST_HASH.into(),
    };
    let block = ProposalHook::block(&context, &[]).unwrap();
    let intent = worker.machine.proposal_intent(block, VALIDATOR_ID).unwrap();
    worker
        .setup
        .wal
        .reserve_proposal_intent(intent.clone())
        .unwrap();
    fixture._server.drop_next_reply();
    assert!(worker.sign_proposal(intent, None).is_err());
    let original = {
        let ledger = fixture.backend.0.lock().unwrap();
        assert_eq!(ledger.state.journal.events.len(), 1);
        ledger.state.journal.events[0].clone()
    };
    assert!(
        worker.setup.wal.state().events.is_empty(),
        "lost reply precedes mirror publication"
    );
    drop(worker);
    let recovered = fixture.worker(&root);
    assert_eq!(recovered.setup.wal.state().events[0], original);
    assert!(
        recovered
            .setup
            .wal
            .state()
            .pending_proposal_intent
            .is_none()
    );
    assert_eq!(recovered.setup.wal.state().events.len(), 1);
    drop(recovered);
}

#[test]
fn protected_runtime_rejects_local_signer_before_contacting_provider() {
    let root = TempRoot::new();
    let fixture = ProviderFixture::new();
    let key = SigningKey::from_bytes(&[71; 32]);
    let mut config = node_config();
    config.replication = Some(
        crate::NodeReplicationConfig::new(root.path().join("replication"))
            .unwrap()
            .with_signing_keypair(
                hex::encode(key.to_bytes()),
                hex::encode(key.verifying_key().to_bytes()),
            )
            .unwrap(),
    );
    let mut runtime = NodeRuntime::new(config)
        .with_replication_network_consensus_enabled(false)
        .with_tendermint_v1(fixture.bft.clone())
        .unwrap()
        .with_protected_bft_signer(fixture.binding.clone(), fixture.provider.clone())
        .unwrap();
    let error = runtime
        .start()
        .expect_err("mixed local/protected signer must stop");
    assert!(
        error
            .to_string()
            .contains("rejects a local consensus signing key"),
        "unexpected earlier startup rejection: {error}"
    );
    assert_eq!(fixture.backend.0.lock().unwrap().lease_generation, 0);
    assert!(!root.path().join("replication/tendermint-v1").exists());
}

#[test]
fn protected_builder_rejects_scope_or_timeout_mismatch_and_requires_bft() {
    let fixture = ProviderFixture::new();
    assert!(
        NodeRuntime::new(node_config())
            .with_protected_bft_signer(fixture.binding.clone(), fixture.provider.clone())
            .is_err()
    );
    let mut wrong_scope = fixture.binding.clone();
    wrong_scope.scope.chain_id.push_str("-other");
    assert!(
        NodeRuntime::new(node_config())
            .with_tendermint_v1(fixture.bft.clone())
            .unwrap()
            .with_protected_bft_signer(wrong_scope, fixture.provider.clone())
            .is_err()
    );
    let mut wrong_timeout = fixture.binding.clone();
    wrong_timeout.timeouts.propose_ms += 1;
    assert!(
        NodeRuntime::new(node_config())
            .with_tendermint_v1(fixture.bft.clone())
            .unwrap()
            .with_protected_bft_signer(wrong_timeout, fixture.provider.clone())
            .is_err()
    );
    assert_eq!(fixture.backend.0.lock().unwrap().lease_generation, 0);
}

#[test]
fn duplicate_wal_open_does_not_acquire_or_fence_the_live_worker() {
    let root = TempRoot::new();
    let fixture = ProviderFixture::new();
    let mut worker = fixture.worker(&root);
    let generation = fixture.backend.0.lock().unwrap().lease_generation;
    let set = fixture.binding.validate().unwrap();
    let genesis = BftHeightProposerScheduleV1::genesis(&set, 1).unwrap();
    let duplicate = BftWalStore::open_protected_for_recovery(
        &root.path().join("replication/tendermint-v1"),
        &genesis,
        &fixture.binding,
        false,
        fixture.provider.as_ref(),
        &[],
    );
    assert!(
        duplicate.is_err(),
        "live worker must retain its directory lock"
    );
    assert_eq!(
        fixture.backend.0.lock().unwrap().lease_generation,
        generation
    );
    assert!(worker.now().is_ok(), "existing lease remains usable");
    worker
        .maybe_propose(None)
        .expect("existing worker can still sign a new proposal");
    assert_eq!(worker.setup.wal.state().events.len(), 1);
    drop(worker);
}

#[test]
fn provider_journal_rollback_cannot_be_repaired_from_a_newer_local_mirror() {
    let root = TempRoot::new();
    let fixture = ProviderFixture::new();
    let mut worker = fixture.worker(&root);
    let old_provider = fixture.backend.0.lock().unwrap().state.clone();
    worker.maybe_propose(None).unwrap();
    assert!(worker.setup.wal.state().events.len() > old_provider.journal.events.len());
    drop(worker);
    // Deliberately violate the test backend nonrollback obligation. Node must
    // stop at the independently newer mirror, never "repair" provider authority.
    fixture.backend.0.lock().unwrap().state = old_provider.clone();
    let set = fixture.binding.validate().unwrap();
    let genesis = BftHeightProposerScheduleV1::genesis(&set, 1).unwrap();
    assert!(
        BftWalStore::open_protected_for_recovery(
            &root.path().join("replication/tendermint-v1"),
            &genesis,
            &fixture.binding,
            false,
            fixture.provider.as_ref(),
            &[],
        )
        .is_err()
    );
    assert_eq!(fixture.backend.0.lock().unwrap().state, old_provider);
}
