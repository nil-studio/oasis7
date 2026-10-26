//! End-to-end first-block test from a qualified formal H closure.
use super::super::super::super::super::driver::NodeRuntimeExecutionDriver;
use super::FormalBftReadExecutionV1;
use super::formal_storage;
use crate::execution_bridge_real_tests::real_execution_bridge::{
    ExecutionBridgeState, formal_business_package, formal_executor, formal_initial, formal_package,
};
use crate::formal_read_fixtures as fixtures;
use ed25519_dalek::Signer;
use oasis7::world_service::client::FormalReadTrustConfigV1;
use oasis7::world_service::formal_result::FormalBusinessResultTreeV1;
use oasis7_consensus::tendermint_v2::{
    BftProfileTargetV1, BftSchedulePolicyV2, verify_formal_bft_handoff_v1,
};
use oasis7_distfs::controlled_authority::formal::endpoint::{
    FormalPrepareReceiptV1, FormalQueryOutcomeV1,
};
use oasis7_distfs::controlled_authority::formal::{
    FormalError, FormalWorldIdentityV1,
    business_index::BusinessIndexNodeReader,
    coordinator::{FormalCoordinator, FormalDurabilityEndpoint},
    endpoint::FormalCandidateMaterialV1,
    *,
};
use oasis7_distfs::controlled_authority::replicated_protocol::{ClosedRecord, EndpointRole};
use oasis7_game_client::{FormalBftReadClientConfigV2, FormalBftReadClientV2};
use oasis7_node::{BftBlockV2, BftCommitCertificateV2};
use oasis7_node::{NodeExecutionCommitContext, NodeLocalExecutionContinuation};
use oasis7_proto::distributed_bft::v2::{
    BftProposalHeaderV2, BftProposalV2, BftScopeV2, BftValidatorV2, BftVotePhaseV2, BftVoteV2,
    BftVoteValueV2,
};
use oasis7_runtime::{
    Action, BlobStore, GovernanceMainTokenControllerRegistry, GovernanceThresholdSignerPolicy,
    ReleaseSecurityPolicy, RuntimeCommittedTickContext, VerifiedFormalRetainedHistoryParentV1,
    World, World as RuntimeWorld, blake3_hex,
};
use oasis7_wasm_abi::{ModuleCallFailure, ModuleCallRequest, ModuleOutput, ModuleSandbox};
use std::{
    collections::BTreeMap,
    fs,
    net::TcpStream,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

#[path = "formal_wasm_fixture.rs"]
mod wasm_fixture;

#[path = "driver_bft_v2_formal_profile_tests.rs"]
mod profile_tests;

#[derive(Clone, Default)]
struct IndexReader(BTreeMap<String, Vec<u8>>);

impl BusinessIndexNodeReader for IndexReader {
    fn read_node(&self, digest: &str) -> Result<Option<Vec<u8>>, String> {
        Ok(self.0.get(digest).cloned())
    }
}

struct EmptyIndexReader;

impl BusinessIndexNodeReader for EmptyIndexReader {
    fn read_node(&self, _: &str) -> Result<Option<Vec<u8>>, String> {
        Ok(None)
    }
}

struct NoModules;

impl ModuleSandbox for NoModules {
    fn call(&mut self, _: &ModuleCallRequest) -> Result<ModuleOutput, ModuleCallFailure> {
        panic!("governance-only BFT fixture must not invoke modules")
    }
}

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock after epoch")
            .as_nanos();
        let path = std::env::temp_dir()
            .canonicalize()
            .expect("canonical temp directory")
            .join(format!(
                "oasis7-formal-bft-cut-{}-{nonce}",
                std::process::id()
            ));
        fs::create_dir(&path).expect("create private test root");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).expect("protect test root");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct FreezeEndpoint {
    authority: VerifiedAuthorityV1,
    parent: VerifiedFormalCommitV1,
    role: EndpointRole,
}

impl FreezeEndpoint {
    fn unsupported<T>() -> Result<T, FormalError> {
        Err(FormalError::Invalid(
            "formal BFT test freeze endpoint".into(),
        ))
    }
}

impl FormalDurabilityEndpoint for FreezeEndpoint {
    fn install_recovery_frozen(
        &mut self,
        _: FormalFrozenCertificateV1,
        _: ClosedRecord,
    ) -> Result<(), FormalError> {
        Self::unsupported()
    }

    fn head(&mut self) -> Result<FormalHeadRefV1, FormalError> {
        Ok(self.parent.head())
    }

    fn query(&mut self, _: &str) -> Result<FormalQueryOutcomeV1, FormalError> {
        Ok(FormalQueryOutcomeV1::NotRecorded)
    }

    fn freeze(
        &mut self,
        input: SignedFormalFrozenV1,
        material: ClosedRecord,
    ) -> Result<FormalFrozenReceiptV1, FormalError> {
        verify_formal_reservation(&input, &self.authority, &self.parent)?;
        verify_frozen_material(&input.body, &material)?;
        let binding = match self.role {
            EndpointRole::Primary => &self.authority.definition().primary,
            EndpointRole::Replica => &self.authority.definition().replica,
        };
        let body = FormalFrozenReceiptBodyV1 {
            authority_id: self.authority.authority_id()?,
            role: self.role,
            instance_digest: binding
                .instance
                .digest()
                .map_err(|error| FormalError::Invalid(error.to_string()))?,
            reservation_id: input.body.reservation_id()?,
        };
        let key = match self.role {
            EndpointRole::Primary => 5,
            EndpointRole::Replica => 6,
        };
        Ok(FormalFrozenReceiptV1 {
            signature_hex: fixtures::sign(frozen_receipt_signing_bytes(&body)?, key),
            body,
        })
    }

    fn complete_frozen(
        &mut self,
        certificate: FormalFrozenCertificateV1,
    ) -> Result<(), FormalError> {
        certificate
            .verify(self.authority.definition(), self.authority.activation_ref())
            .map(|_| ())
    }

    fn prepare(
        &mut self,
        _: FormalCandidateMaterialV1,
    ) -> Result<FormalPrepareReceiptV1, FormalError> {
        Self::unsupported()
    }

    fn decide(
        &mut self,
        _: &str,
        _: FormalPrepareReceiptV1,
        _: FormalPrepareReceiptV1,
    ) -> Result<FormalFinalReceiptV1, FormalError> {
        Self::unsupported()
    }

    fn install_proof(
        &mut self,
        _: FormalDecisionProofV1,
    ) -> Result<FormalProofCompleteReceiptV1, FormalError> {
        Self::unsupported()
    }

    fn publish_qualified(
        &mut self,
        _: FormalQualifiedCommitProofV1,
    ) -> Result<FormalHeadRefV1, FormalError> {
        Self::unsupported()
    }
}

struct QualifiedH {
    anchor: VerifiedFormalReadAnchorV1,
    trust: FormalReadTrustConfigV1,
    material: ClosedRecord,
    target: BftProfileTargetV1,
    world: RuntimeWorld,
    reader: Arc<IndexReader>,
}

fn qualified_handoff() -> QualifiedH {
    let (_, mut trust, template, _) = fixtures::actual_world_initial_fixture();
    let identity = template.authority().definition().world.clone();
    let mut world = wasm_fixture::world_with_formal_history();
    let controller = fixtures::key(102);
    world
        .set_governance_main_token_controller_registry(GovernanceMainTokenControllerRegistry {
            genesis_controller_account_id: "formal-bft-controller".into(),
            controller_signer_policies: BTreeMap::from([(
                "formal-bft-controller".into(),
                GovernanceThresholdSignerPolicy {
                    threshold: 1,
                    allowed_public_keys: [hex::encode(controller.verifying_key().to_bytes())]
                        .into(),
                },
            )]),
            ..Default::default()
        })
        .expect("set the H-committed governance controller registry");
    world.step().expect("prepare nonzero formal H boundary");
    assert_eq!(world.state().time, 1);

    let release = ReleaseSecurityPolicy::default();
    let bootstrap_snapshot = serde_cbor::to_vec(&world.snapshot()).unwrap();
    let bootstrap = formal_initial::InitialNodeBootstrapV1 {
        height: world.state().time,
        consensus_block_hash: fixtures::h(81),
        execution_block_hash: fixtures::h(82),
        execution_state_root: blake3_hex(&bootstrap_snapshot),
    };
    let closure = formal_initial::InitialWorldClosure::from_existing_world_with_node_bootstrap(
        &world, &identity, &release, bootstrap,
    )
    .expect("prepare profile-v2 activation at tick one");
    let mut definition = template.authority().definition().clone();
    definition.profile_version = 2;
    let policy = template.authority().policy().clone();
    let body = closure
        .activation_body(definition, policy.clone())
        .expect("profile-v2 activation body");
    trust.world = identity.clone();
    trust.initial_state_root = body.initial_state_root.clone();
    trust.governing_manifest_digest = body.governing_manifest_digest.clone();
    trust.recovery_policy_digest = policy.digest().unwrap();
    let certificate = SignedInitialActivationV1 {
        signature_hex: fixtures::sign(initial_activation_signing_bytes(&body).unwrap(), 9),
        body,
    };
    let writer = fixtures::key(4);
    let (authority, initial) = closure
        .authorize(&certificate, &trust, &writer)
        .expect("authorize real profile-v2 initial closure");
    let initial_proof = fixtures::qualified_proof(
        initial.candidate.record.clone(),
        None,
        &authority,
        None,
        None,
        4,
        5,
        6,
    );
    let initial_anchor =
        VerifiedFormalReadAnchorV1::from_initial(&certificate, &trust, &initial_proof).unwrap();
    let initial_tree =
        FormalBusinessResultTreeV1::for_initial_world(&world, &identity, &EmptyIndexReader)
            .expect("initial formal business tree");
    assert_eq!(
        initial_tree.tree.root().unwrap(),
        initial.candidate.record.result_root
    );
    let reader = Arc::new(IndexReader(
        initial_tree
            .delta
            .node_writes
            .iter()
            .map(|write| (write.digest.clone(), write.bytes.clone()))
            .collect(),
    ));
    let snapshot = world.snapshot();
    let snapshot_bytes = serde_cbor::to_vec(&snapshot).unwrap();
    let retained = VerifiedFormalRetainedHistoryParentV1::verify(&initial_anchor, &snapshot_bytes)
        .expect("qualified profile-v2 retained history");
    world = RuntimeWorld::from_verified_formal_snapshot(
        snapshot,
        world.journal().clone(),
        retained,
        reader.clone(),
    )
    .unwrap()
    .with_release_security_policy(release.clone());

    let scope = BftScopeV2 {
        world_id: identity.world_id.clone(),
        chain_id: identity.chain_id.clone(),
        genesis_hash: identity.genesis_digest.clone(),
    };
    let validator_key = fixtures::key(101);
    let target = BftProfileTargetV1::new(
        scope,
        1,
        vec![BftValidatorV2 {
            validator_id: "validator-1".into(),
            public_key_hex: hex::encode(validator_key.verifying_key().to_bytes()),
            voting_power: 1,
        }],
        BftSchedulePolicyV2::new(
            world.governance_execution_policy().epoch_length_ticks,
            "bft",
        )
        .unwrap(),
    )
    .unwrap();
    let handoff = ProfileHandoffV1 {
        next_profile: target.profile().into(),
        next_profile_version: target.profile_version(),
        next_authority_digest: target.target_digest().unwrap(),
        valid_from_position: 3,
    };
    let continuation = continuation(&world, &initial_anchor);
    let (mut frozen_body, frozen_material) = formal_package::freeze_material_from_parent(
        &authority,
        initial_anchor.head(),
        &continuation,
        &world,
        &release,
        initial_tree.tree,
        initial_proof.clone(),
        None,
        &initial.material,
    )
    .expect("freeze H execution closure");
    frozen_body.handoff = Some(handoff);
    let frozen = SignedFormalFrozenV1 {
        signature_hex: fixtures::sign(frozen_signing_bytes(&frozen_body).unwrap(), 4),
        body: frozen_body,
    };
    let endpoint = |role| FreezeEndpoint {
        authority: authority.clone(),
        parent: initial_anchor.head().clone(),
        role,
    };
    let mut coordinator = FormalCoordinator::new(
        endpoint(EndpointRole::Primary),
        endpoint(EndpointRole::Replica),
        authority.clone(),
        authority.clone(),
        None,
        Some(initial_anchor.head().clone()),
    )
    .unwrap();
    let executable = coordinator.freeze(frozen, frozen_material).unwrap();
    let scratch = Scratch::new();
    let shared_reader = formal_business_package::SharedBusinessReader(reader.clone());
    let candidate = formal_executor::execute_normal_with_business_reader(
        &executable,
        &authority,
        initial_anchor.head(),
        scratch.path(),
        &writer,
        Some(&shared_reader),
        Some(&initial_anchor),
    )
    .expect("execute H against the actual World");
    verify_formal_candidate(
        &candidate.candidate,
        candidate.frozen.as_ref(),
        &authority,
        Some(initial_anchor.head()),
        None,
        FormalCompletionBranchV1::Normal,
    )
    .unwrap();
    let h_proof = fixtures::qualified_proof(
        candidate.candidate.record.clone(),
        candidate.frozen.clone(),
        &authority,
        Some(initial_anchor.head()),
        None,
        4,
        5,
        6,
    );
    let committed =
        verify_formal_qualified(&h_proof, &authority, Some(initial_anchor.head()), None).unwrap();
    let anchor = initial_anchor
        .after_verified_commit(authority, committed)
        .unwrap();
    assert_eq!(anchor.head().record().position, 2);
    assert!(anchor.head().record().handoff.is_some());

    let envelope = formal_business_package::envelope(&candidate.material, true)
        .unwrap()
        .unwrap();
    let mut nodes = reader.0.clone();
    nodes.extend(
        envelope
            .delta
            .node_writes
            .iter()
            .map(|write| (write.digest.clone(), write.bytes.clone())),
    );
    let reader = Arc::new(IndexReader(nodes));
    let checkpoint = anchor.checkpoint();
    let read_trust = FormalReadTrustConfigV1 {
        initial_trust: trust,
        checkpoint_digest: checkpoint.digest().unwrap(),
        checkpoint,
    };
    QualifiedH {
        anchor,
        trust: read_trust,
        material: candidate.material,
        target,
        world,
        reader,
    }
}

fn continuation(
    world: &RuntimeWorld,
    parent: &VerifiedFormalReadAnchorV1,
) -> NodeLocalExecutionContinuation {
    let snapshot_root = blake3_hex(&serde_cbor::to_vec(&world.snapshot()).unwrap());
    let node_hash = fixtures::h(83);
    NodeLocalExecutionContinuation {
        schema_version: 1,
        context: NodeExecutionCommitContext {
            world_id: parent.head().record().world.world_id.clone(),
            node_id: "formal-bft-test-node".into(),
            proposer_id: "formal-bft-test-node".into(),
            height: parent.head().record().position + 1,
            slot: parent.head().record().position + 1,
            epoch: 1,
            node_block_hash: node_hash.clone(),
            action_root: oasis7_node::compute_consensus_action_root(&[]).unwrap(),
            committed_actions: Vec::new(),
            committed_at_unix_ms: 2,
        },
        predecessor_committed_height: parent.head().record().position,
        predecessor_node_block_hash: Some(fixtures::h(81)),
        predecessor_execution_height: parent.head().record().position,
        predecessor_execution_block_hash: Some(fixtures::h(82)),
        predecessor_execution_state_root: Some(snapshot_root),
        next_height: parent.head().record().position + 1,
        next_slot: parent.head().record().position + 2,
        reserved_action_bytes: 0,
        approved_stake: 1,
        rejected_stake: 0,
        required_stake: 1,
        total_stake: 1,
    }
}

fn driver(root: &Path, world: RuntimeWorld) -> NodeRuntimeExecutionDriver {
    let policy = crate::controlled_execution_policy::GuardedExecutionPolicy::current();
    NodeRuntimeExecutionDriver::new_with_sandbox(
        root.join("state.json"),
        root.join("world"),
        root.join("records"),
        root.join("cas"),
        ExecutionBridgeState::default(),
        world,
        Box::new(policy.executor().unwrap()),
        policy.hot_head_heights,
        policy.checkpoint_interval,
        policy.checkpoint_keep,
    )
}

fn certificate(
    block: BftBlockV2,
    schedule: &oasis7_consensus::tendermint_v2::BftHeightProposerScheduleV2,
) -> BftCommitCertificateV2 {
    let key = fixtures::key(101);
    let proposer_id = schedule.expected_proposer_for_round(0).unwrap();
    let header = BftProposalHeaderV2::sign(
        block.scope.clone(),
        block.height,
        0,
        schedule.active_set().set_hash().to_owned(),
        schedule.policy_hash().to_owned(),
        proposer_id.clone(),
        block.block_id().unwrap(),
        None,
        &key,
    )
    .unwrap();
    let proposal = BftProposalV2 {
        header,
        block,
        valid_round_evidence: None,
    };
    let vote = BftVoteV2::sign(
        schedule.scope().clone(),
        schedule.height(),
        0,
        BftVotePhaseV2::Precommit,
        schedule.active_set().set_hash().to_owned(),
        schedule.policy_hash().to_owned(),
        BftVoteValueV2::Block {
            block_id: proposal.header.block_id.clone(),
        },
        proposer_id,
        &key,
    )
    .unwrap();
    BftCommitCertificateV2 {
        proposal: Box::new(proposal),
        precommits: vec![vote],
    }
}

#[test]
#[cfg(unix)]
fn qualified_handoff_executes_and_reopens_one_durable_bft_successor_cut() {
    std::thread::Builder::new()
        .name("formal-bft-handoff-cut".into())
        .stack_size(32 * 1024 * 1024)
        .spawn(formal_handoff_cut_case)
        .expect("spawn large-stack formal BFT fixture")
        .join()
        .expect("formal handoff cut case");
}

#[cfg(unix)]
fn formal_handoff_cut_case() {
    let h = qualified_handoff();
    let scratch = Scratch::new();
    let reader: Arc<dyn BusinessIndexNodeReader + Send + Sync> = h.reader.clone();
    let mut initial_driver = driver(scratch.path(), h.world.clone());
    let mut execution = initial_driver
        .open_formal_bft_read_v1(
            h.anchor.clone(),
            h.target.clone(),
            h.material.clone(),
            reader.clone(),
            true,
        )
        .expect("open from complete qualified H closure");
    let block = execution
        .build_candidate(Vec::new())
        .expect("execute the first formal BFT height");
    execution.preview_candidate(&block).unwrap();
    let certificate = certificate(block, execution.schedule());
    let cut = execution
        .commit_certificate(certificate.clone())
        .expect("persist verified H+1 certificate and World");
    assert_eq!(cut.certificate(), &certificate);
    assert_eq!(cut.commitment().height, 3);
    assert_eq!(cut.world().state().time, 3);
    let durable_digest = cut.durable_record_digest().to_owned();
    let retained_record = execution.ledger.record().unwrap().clone();
    let store = execution.store.clone();
    drop(cut);
    drop(execution);
    drop(initial_driver);

    // CAS GC must not remove any H+1 material or H replay dependencies while
    // the immutable formal read record is the durable owner of those refs.
    store.prune_unpinned(0).unwrap();
    assert!(store.has(&retained_record.snapshot_ref).unwrap());
    assert!(store.has(&retained_record.journal_ref).unwrap());
    for digest in &retained_record.business_index_refs {
        assert!(store.has(digest).unwrap());
    }

    let mut reopened_driver = driver(scratch.path(), h.world.clone());
    let reopened = reopened_driver
        .open_formal_bft_read_v1(
            h.anchor.clone(),
            h.target.clone(),
            h.material.clone(),
            Arc::new(EmptyIndexReader),
            false,
        )
        .expect("reopen and replay using the pinned business-index overlay");
    assert!(
        !reopened
            .ledger
            .record()
            .unwrap()
            .business_index_refs
            .is_empty(),
        "the real H result-tree metadata node is retained with the cut"
    );
    let replayed = reopened.verified_cut(3).unwrap();
    assert_eq!(replayed.certificate(), &certificate);
    assert_eq!(replayed.durable_record_digest(), durable_digest);
    assert_eq!(replayed.world().state().time, 3);
    assert_eq!(
        replayed.world().current_manifest_hash().unwrap(),
        replayed.commitment().post_manifest_hash
    );
}

#[test]
#[cfg(unix)]
fn qualified_handoff_rejects_a_non_aligned_world_without_rewriting_it() {
    std::thread::Builder::new()
        .name("formal-bft-handoff-tick-mapping".into())
        .stack_size(32 * 1024 * 1024)
        .spawn(|| {
            let h = qualified_handoff();
            // `qualified_handoff` retains the pre-H World supplied to the
            // formal writer. It is deliberately not the authenticated
            // post-H World, so this exercises rejection of a mismatched
            // caller World without mutating it or pretending to create a
            // valid, non-aligned H record.
            let before_tick = h.world.state().time;
            assert_ne!(before_tick, h.anchor.head().record().position);
            let error = super::validate_handoff_world(&h.anchor, &h.world).unwrap_err();
            assert!(error.contains("position/tick mapping is unsupported"));
            assert!(error.contains("H position"));
            assert_eq!(
                h.world.state().time,
                before_tick,
                "validation must reject, never rewrite World time"
            );
        })
        .expect("spawn large-stack H tick mapping fixture")
        .join()
        .expect("H tick mapping mismatch remains fail-closed");
}

#[test]
#[cfg(unix)]
fn interrupted_formal_read_initialization_recovers_only_the_exact_existing_pin() {
    std::thread::Builder::new()
        .name("formal-bft-interrupted-init".into())
        .stack_size(32 * 1024 * 1024)
        .spawn(interrupted_initialization_case)
        .expect("spawn large-stack formal BFT fixture")
        .join()
        .expect("interrupted formal BFT initialization case");
}

#[cfg(unix)]
fn interrupted_initialization_case() {
    let h = qualified_handoff();
    let scratch = Scratch::new();
    let reader: Arc<dyn BusinessIndexNodeReader + Send + Sync> = h.reader.clone();

    // This leaves the same durable files as a crash after pin.cbor was
    // fsynced but before the required marker was created.
    let initial = open_formal_read(&h, &scratch, reader.clone(), true)
        .expect("initialize formal BFT read pin and marker");
    drop(initial);

    let records = scratch.path().join("records");
    let marker = records.join("formal-bft-read-required-v1");
    let ledger = records.join("formal-bft-read-v1");
    let pin = ledger.join("pin.cbor");
    std::fs::remove_file(&marker).expect("simulate crash before required marker");
    assert!(pin.is_file());

    let refused_reopen = open_formal_read(&h, &scratch, reader.clone(), false);
    assert!(
        refused_reopen.is_err(),
        "ordinary reopen must not finalize initialization"
    );
    assert!(
        !marker.exists(),
        "ordinary reopen must not create the marker"
    );

    let recovered = open_formal_read(&h, &scratch, reader.clone(), true)
        .expect("resume exact pin after interrupted initialization");
    assert!(
        recovered.verified_cut(3).is_err(),
        "initialization has no committed record"
    );
    drop(recovered);
    assert_eq!(
        std::fs::read(&marker).expect("recovered required marker"),
        b"oasis7.formal.bft.read.required/v1\n"
    );

    std::fs::remove_file(&marker).expect("recreate missing-marker crash state");
    std::fs::remove_file(&pin).expect("remove exact pin to test fail-closed behavior");
    let missing_pin = open_formal_read(&h, &scratch, reader, true);
    assert!(
        missing_pin.is_err(),
        "recovery must not invent a missing pin"
    );
    assert!(!pin.exists());
    assert!(!marker.exists());
}

#[test]
#[cfg(unix)]
fn formal_bft_cas_material_is_pinned_before_record_publication() {
    std::thread::Builder::new()
        .name("formal-bft-pin-before-record".into())
        .stack_size(32 * 1024 * 1024)
        .spawn(formal_bft_pin_before_record_case)
        .expect("spawn large-stack CAS pin ordering fixture")
        .join()
        .expect("formal BFT pin-before-record case");
}

#[cfg(unix)]
fn formal_bft_pin_before_record_case() {
    let h = qualified_handoff();
    let scratch = Scratch::new();
    let reader: Arc<dyn BusinessIndexNodeReader + Send + Sync> = h.reader.clone();
    let mut runtime = driver(scratch.path(), h.world.clone());
    let mut execution = runtime
        .open_formal_bft_read_v1(
            h.anchor.clone(),
            h.target.clone(),
            h.material.clone(),
            reader.clone(),
            true,
        )
        .unwrap();
    let block = execution.build_candidate(Vec::new()).unwrap();
    let executed = execution.execute_candidate(&block).unwrap();
    let certificate = certificate(block.clone(), execution.schedule());
    let snapshot_ref = blake3_hex(executed.snapshot_bytes.as_slice());
    let journal_ref = blake3_hex(executed.journal_bytes.as_slice());
    let business_index_nodes = execution
        .retained_nodes
        .recorded_nodes()
        .unwrap()
        .into_iter()
        .collect::<BTreeMap<_, _>>();
    let candidate = formal_storage::FormalBftReadCandidateV1 {
        certificate,
        commitment: executed.commitment,
        block: &block,
        block_id: block.block_id().unwrap(),
        snapshot_bytes: executed.snapshot_bytes.as_slice(),
        journal_bytes: executed.journal_bytes.as_slice(),
        business_index_nodes: &business_index_nodes,
        new_results: executed.new_results.as_slice(),
    };
    let failed =
        execution
            .ledger
            .commit_with_before_record_hook(candidate, &execution.store, || {
                Err("simulated crash after CAS pin, before immutable record".into())
            });
    assert!(failed.is_err());
    assert!(execution.ledger.record().is_none());
    let record_path = scratch
        .path()
        .join("records/formal-bft-read-v1/height-00000000000000000001.cbor");
    assert!(!record_path.exists(), "record publication did not occur");
    assert!(
        execution
            .store
            .is_effectively_pinned(&snapshot_ref)
            .unwrap()
    );
    assert!(execution.store.is_effectively_pinned(&journal_ref).unwrap());
    assert!(
        !business_index_nodes.is_empty(),
        "qualified H replay reads actual business-index nodes"
    );
    for digest in business_index_nodes.keys() {
        assert!(execution.store.is_effectively_pinned(digest).unwrap());
    }

    execution.store.prune_unpinned(0).unwrap();
    assert!(execution.store.has(&snapshot_ref).unwrap());
    assert!(execution.store.has(&journal_ref).unwrap());
    for digest in business_index_nodes.keys() {
        assert!(execution.store.has(digest).unwrap());
    }
    drop(execution);
    drop(runtime);

    let reopened = open_formal_read(&h, &scratch, reader, false).unwrap();
    assert!(reopened.verified_cut(3).is_err());
    assert!(reopened.ledger.record().is_none());
}

#[cfg(unix)]
fn open_formal_read(
    h: &QualifiedH,
    scratch: &Scratch,
    reader: Arc<dyn BusinessIndexNodeReader + Send + Sync>,
    allow_initialize: bool,
) -> Result<FormalBftReadExecutionV1, String> {
    let mut runtime = driver(scratch.path(), h.world.clone());
    runtime.open_formal_bft_read_v1(
        h.anchor.clone(),
        h.target.clone(),
        h.material.clone(),
        reader,
        allow_initialize,
    )
}

#[test]
#[cfg(unix)]
fn formal_bft_attach_preserves_the_exact_retained_h_reader() {
    std::thread::Builder::new()
        .name("formal-bft-retained-h-attach".into())
        .stack_size(32 * 1024 * 1024)
        .spawn(formal_bft_retained_h_attach_case)
        .expect("spawn large-stack retained H attachment fixture")
        .join()
        .expect("formal BFT retained H attachment case");
}

#[cfg(unix)]
fn formal_bft_retained_h_attach_case() {
    let h = qualified_handoff();
    let capability = verify_formal_bft_handoff_v1(&h.anchor, h.target.clone()).unwrap();
    let scratch = Scratch::new();
    let reader: Arc<dyn BusinessIndexNodeReader + Send + Sync> = h.reader.clone();
    let mut runtime = driver(scratch.path(), h.world.clone());
    let execution = runtime
        .open_formal_bft_read_v1(
            h.anchor.clone(),
            h.target.clone(),
            h.material.clone(),
            reader.clone(),
            true,
        )
        .expect("derive H+1 schedule from the qualified H closure");
    let schedule = execution.schedule().clone();
    let parent_world = execution.parent_world.clone();
    drop(execution);
    drop(runtime);

    let wrong_schedule = capability
        .initial_schedule_after_handoff_replay(
            &"12".repeat(32),
            &parent_world.current_manifest_hash().unwrap(),
        )
        .unwrap();
    assert_ne!(wrong_schedule, schedule);
    let mut wrong_root_world = parent_world.clone();
    assert!(
        wrong_root_world
            .attach_formal_bft_handoff_parent_v1(&capability, &wrong_schedule)
            .is_err(),
        "a schedule with a different H semantic root cannot attach"
    );

    let mut world = parent_world;
    let original_snapshot = serde_cbor::to_vec(&world.snapshot()).unwrap();
    assert!(
        world.attach_bft_governance_parent_v2(&schedule).is_err(),
        "the engineering attach path remains closed for qualified H"
    );
    world
        .attach_formal_bft_handoff_parent_v1(&capability, &schedule)
        .unwrap();
    assert_eq!(
        serde_cbor::to_vec(&world.snapshot()).unwrap(),
        original_snapshot
    );

    let retained = VerifiedFormalRetainedHistoryParentV1::verify(&h.anchor, &original_snapshot)
        .expect("reconstruct exact qualified H parent");
    assert!(
        world
            .attach_verified_formal_history(retained, Arc::new(EmptyIndexReader))
            .is_err(),
        "the attached H reader cannot be replaced after formal BFT attach"
    );
    assert!(
        world.inherit_formal_history_reader(&h.world).is_err(),
        "the H reader cannot be swapped through inheritance either"
    );
    assert_eq!(
        world
            .read_retained_world_service_result(&format!("blake3:{}", "00".repeat(32)))
            .unwrap(),
        None,
        "the original authenticated reader remains attached for retained reads"
    );
    assert_eq!(
        serde_cbor::to_vec(&world.snapshot()).unwrap(),
        original_snapshot
    );
}

#[test]
#[cfg(unix)]
fn formal_handoff_authorizes_multiple_actions_against_unchanged_h_policy() {
    std::thread::Builder::new()
        .name("formal-bft-multi-governance".into())
        .stack_size(32 * 1024 * 1024)
        .spawn(formal_bft_multi_governance_case)
        .expect("spawn large-stack formal BFT governance fixture")
        .join()
        .expect("formal BFT multi-governance case");
}

#[cfg(unix)]
fn formal_bft_multi_governance_case() {
    let h = qualified_handoff();
    let scratch = Scratch::new();
    let reader: Arc<dyn BusinessIndexNodeReader + Send + Sync> = h.reader.clone();
    let mut runtime = driver(scratch.path(), h.world.clone());
    let execution = runtime
        .open_formal_bft_read_v1(
            h.anchor.clone(),
            h.target.clone(),
            h.material.clone(),
            reader,
            true,
        )
        .expect("replay exact H and derive its BFT schedule");
    let mut world = execution.parent_world.clone();
    world
        .attach_formal_bft_handoff_parent_v1(&execution.handoff, &execution.schedule)
        .expect("attach exact retained H capability");
    let context = RuntimeCommittedTickContext {
        height: execution.height(),
        slot: execution.height().saturating_sub(1),
        epoch: execution.target.authority_epoch(),
        node_block_hash: "21".repeat(32),
        action_root: "22".repeat(32),
        authority_node_id: world.tick_consensus_authority_source().to_owned(),
        committed_at_unix_ms: 0,
    };
    let controller = fixtures::key(102);
    let actions = [
        Action::SubmitGovernanceValidatorAdmission {
            controller_account_id: "formal-bft-controller".into(),
            candidate_id: "bft-admission-one".into(),
            node_id: "bft-node-one".into(),
            finality_signer_public_key: hex::encode(fixtures::key(201).verifying_key().to_bytes()),
            stake: 25,
            operator_owner: "operator-one".into(),
            public_manifest_hash: "31".repeat(32),
        },
        Action::SubmitGovernanceValidatorAdmission {
            controller_account_id: "formal-bft-controller".into(),
            candidate_id: "bft-admission-two".into(),
            node_id: "bft-node-two".into(),
            finality_signer_public_key: hex::encode(fixtures::key(202).verifying_key().to_bytes()),
            stake: 27,
            operator_owner: "operator-two".into(),
            public_manifest_hash: "32".repeat(32),
        },
    ];
    let mut proofs = Vec::new();
    for (index, action) in actions.iter().enumerate() {
        let mut proof = world
            .governance_authorization_for_next_commit(index as u64 + 1)
            .expect("derive authorization from immutable H policy");
        let signature = controller.sign(&proof.signing_bytes(action).unwrap());
        proof.signatures.insert(
            hex::encode(controller.verifying_key().to_bytes()),
            hex::encode(signature.to_bytes()),
        );
        proofs.push(proof);
    }

    world.begin_bft_execution_tick(&context).unwrap();
    for (action, proof) in actions.into_iter().zip(proofs) {
        world.submit_governance_action(action, proof);
    }
    world
        .step_with_modules_for_committed_context(&mut NoModules, &context)
        .expect("both actions remain authorized after the first mutates World state");
    world.finish_bft_execution_tick(&context).unwrap();

    assert_eq!(world.state().time, execution.height());
    assert_eq!(
        world
            .state()
            .governance_authorization_nonces
            .get("formal-bft-controller"),
        Some(&2)
    );
    assert!(
        world
            .state()
            .governance_validator_admissions
            .contains_key("bft-admission-one")
    );
    assert!(
        world
            .state()
            .governance_validator_admissions
            .contains_key("bft-admission-two")
    );
}

struct RunningFormalBftReadCli {
    stop: Arc<AtomicBool>,
    join: Option<JoinHandle<Result<(), String>>>,
}

impl RunningFormalBftReadCli {
    fn start(config_path: PathBuf) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&stop);
        let join = std::thread::Builder::new()
            .name("formal-bft-read-cli-http".into())
            .stack_size(32 * 1024 * 1024)
            .spawn(move || {
                let config = config_path
                    .to_str()
                    .expect("formal BFT CLI fixture path is UTF-8")
                    .to_owned();
                crate::run_formal_bft_read_cli_until_shutdown(
                    ["serve", "--config", config.as_str()].into_iter(),
                    thread_stop,
                )
            })
            .expect("start actual formal BFT read CLI service");
        Self {
            stop,
            join: Some(join),
        }
    }

    fn stop_and_join(&mut self) -> Result<(), String> {
        self.stop.store(true, Ordering::Release);
        self.join
            .take()
            .expect("CLI join handle is present")
            .join()
            .map_err(|_| "formal BFT CLI service thread panicked".to_owned())?
    }
}

impl Drop for RunningFormalBftReadCli {
    fn drop(&mut self) {
        if self.join.is_some() {
            self.stop.store(true, Ordering::Release);
            if let Some(join) = self.join.take() {
                let _ = join.join();
            }
        }
    }
}

#[test]
#[cfg(unix)]
fn qualified_handoff_cli_install_and_http_client_verify_the_real_h_plus_one_cut() {
    std::thread::Builder::new()
        .name("formal-bft-read-cli-client-test".into())
        .stack_size(32 * 1024 * 1024)
        .spawn(formal_bft_cli_http_case)
        .expect("spawn large-stack formal BFT CLI fixture")
        .join()
        .expect("formal BFT CLI HTTP case");
}

#[cfg(unix)]
fn formal_bft_cli_http_case() {
    let h = qualified_handoff();
    let scratch = Scratch::new();
    let reader: Arc<dyn BusinessIndexNodeReader + Send + Sync> = h.reader.clone();
    let mut runtime = driver(scratch.path(), h.world.clone());
    let execution = runtime
        .open_formal_bft_read_v1(
            h.anchor.clone(),
            h.target.clone(),
            h.material.clone(),
            reader,
            true,
        )
        .expect("open actual qualified H closure");
    let block = execution.build_candidate(Vec::new()).unwrap();
    let certificate = certificate(block.clone(), execution.schedule());
    let exact_target = oasis7_client_api::world_service::FormalBftReadTargetV2 {
        height: block.height,
        block_id: block.block_id().unwrap(),
    };
    let index_nodes = h
        .reader
        .0
        .iter()
        .map(|(digest, bytes)| {
            oasis7_distfs::controlled_authority::formal::business_index::BusinessIndexNodeWriteV1 {
                digest: digest.clone(),
                bytes: bytes.clone(),
            }
        })
        .collect();
    drop(execution);
    drop(runtime);

    let fixture = crate::install_handoff_for_test(
        h.trust.clone(),
        h.target.clone(),
        exact_target.clone(),
        h.material.clone(),
        certificate,
        index_nodes,
    )
    .expect("shipping install dispatcher commits the verified H+1 cut");
    let mut server = RunningFormalBftReadCli::start(fixture.config_path.clone());
    let address = fixture.bind;
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if TcpStream::connect_timeout(&address, Duration::from_millis(50)).is_ok() {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "shipping formal BFT read service bound its configured loopback address"
        );
        assert!(
            !server.join.as_ref().expect("CLI join handle").is_finished(),
            "CLI service remains running while the client connects"
        );
        std::thread::sleep(Duration::from_millis(10));
    }

    let client = FormalBftReadClientV2::new(
        FormalBftReadClientConfigV2 {
            endpoint: format!("http://{address}"),
            trusted_service_public_key: fixture.service_public_key_hex.clone(),
            read_private_key_hex: fixture.read_private_key_hex.clone(),
            timeout: Duration::from_secs(2),
            scope_id: "public".into(),
            exact_target: exact_target.clone(),
        },
        h.anchor,
        h.target,
    )
    .expect("construct independently pinned real client");
    let page = client
        .read::<oasis7::world_service::projection::WorldServiceProjection>()
        .expect("real HTTP response passes signature, QC, commitment and inclusion checks");
    assert_eq!(page.response().head.height, exact_target.height);
    assert_eq!(page.response().head.block_id, exact_target.block_id);
    assert_eq!(page.view().state.time, exact_target.height);
    assert!(page.view().formal_commit_header.is_some());
    server
        .stop_and_join()
        .expect("stop and join shipping CLI service");
}
