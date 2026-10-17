//! Actual World/profile-2 execution capacity fixture. Test signers supply
//! protocol proofs; two local directories do not establish S6 fault domains.
use super::*;
use oasis7::runtime::{Journal, Snapshot, VerifiedFormalRetainedHistoryParentV1};
use oasis7_distfs::{
    controlled_authority::formal::{
        FormalStorageShadowOptions, advance_formal_storage_shadow,
        business_index::BusinessIndexNodeReader,
        endpoint::{FileFormalArchiveSourceV1, FormalArchiveSourceV1},
    },
    storage_inventory::inventory_storage,
};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::Instant,
};
#[path = "formal_storage_benchmark_io.rs"]
mod io_fixture;
use io_fixture::{Corpus, Scratch};
#[derive(Clone)]
struct IndexReader(PathBuf);
impl BusinessIndexNodeReader for IndexReader {
    fn read_node(&self, id: &str) -> Result<Option<Vec<u8>>, String> {
        match fs::read(self.0.join(id)) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }
}
struct Fixture {
    world: World,
    anchor: VerifiedFormalReadAnchorV1,
    candidate: FormalCandidateMaterialV1,
    proof: FormalQualifiedCommitProofV1,
    previous_execution: String,
    previous_node: String,
}
fn initial() -> Fixture {
    let world = wasm_fixture::world_with_formal_history();
    let (_, mut trust, template, _) = fixtures::actual_world_initial_fixture();
    let release = ReleaseSecurityPolicy::default();
    let identity = template.authority().definition().world.clone();
    let bootstrap = super::super::super::formal_initial::InitialNodeBootstrapV1 {
        height: world.state().time,
        consensus_block_hash: fixtures::h(81),
        execution_block_hash: fixtures::h(82),
        execution_state_root: blake3_hex(&to_cbor(world.snapshot()).unwrap()),
    };
    let closure = super::super::super::formal_initial::InitialWorldClosure::from_genesis_world_with_node_bootstrap(
        &world, &identity, &release, bootstrap.clone()).unwrap();
    let mut definition = template.authority().definition().clone();
    definition.profile_version = 2;
    let body = closure
        .activation_body(definition, template.authority().policy().clone())
        .unwrap();
    trust.world = identity;
    trust.initial_state_root = body.initial_state_root.clone();
    trust.governing_manifest_digest = body.governing_manifest_digest.clone();
    trust.recovery_policy_digest = body.recovery_policy.digest().unwrap();
    let certificate = SignedInitialActivationV1 {
        signature_hex: fixtures::sign(initial_activation_signing_bytes(&body).unwrap(), 9),
        body,
    };
    let (authority, candidate) = closure
        .authorize(&certificate, &trust, &fixtures::key(4))
        .unwrap();
    let proof = fixtures::qualified_proof(
        candidate.candidate.record.clone(),
        None,
        &authority,
        None,
        None,
        4,
        5,
        6,
    );
    let anchor = VerifiedFormalReadAnchorV1::from_initial(&certificate, &trust, &proof).unwrap();
    Fixture {
        world,
        anchor,
        candidate,
        proof,
        previous_execution: bootstrap.execution_block_hash,
        previous_node: bootstrap.consensus_block_hash,
    }
}
fn execute_step(
    fixture: &Fixture,
    reader: &super::super::super::formal_business_package::SharedBusinessReader,
    attempts: &Path,
) -> FormalCandidateMaterialV1 {
    let mut original = continuation(&fixture.world, fixture.anchor.head());
    let height = fixture.world.state().time + 1;
    original.context.height = height;
    original.context.slot = height;
    original.context.node_block_hash = blake3_hex(&to_cbor(("capacity-context", height)).unwrap());
    original.context.committed_at_unix_ms = i64::try_from(height).unwrap();
    original.predecessor_committed_height = height - 1;
    original.predecessor_execution_height = height - 1;
    original.predecessor_node_block_hash = Some(fixture.previous_node.clone());
    original.predecessor_execution_block_hash = Some(fixture.previous_execution.clone());
    original.predecessor_execution_state_root =
        Some(fixture.anchor.head().record().after_state_root.clone());
    original.next_height = height + 1;
    original.next_slot = height + 1;
    let tree = super::super::super::formal_business_package::tree(
        &fixture.candidate.material,
        fixture.anchor.head().record(),
        &fixture.world,
        true,
    )
    .unwrap()
    .unwrap();
    let (body, material) = freeze_material_from_parent(
        fixture.anchor.authority(),
        fixture.anchor.head(),
        &original,
        &fixture.world,
        &ReleaseSecurityPolicy::default(),
        tree,
        fixture.proof.clone(),
        None,
        &fixture.candidate.material,
    )
    .unwrap();
    let signed = SignedFormalFrozenV1 {
        signature_hex: fixtures::sign(frozen_signing_bytes(&body).unwrap(), 4),
        body,
    };
    let receiver = |role| FrozenReceiver {
        authority: fixture.anchor.authority().clone(),
        parent: fixture.anchor.head().clone(),
        role,
        retained: None,
    };
    let mut coordinator = FormalCoordinator::new(
        receiver(EndpointRole::Primary),
        receiver(EndpointRole::Replica),
        fixture.anchor.authority().clone(),
        fixture.anchor.authority().clone(),
        None,
        Some(fixture.anchor.head().clone()),
    )
    .unwrap();
    let frozen = coordinator.freeze(signed, material).unwrap();
    super::super::super::formal_executor::execute_normal_with_business_reader(
        &frozen,
        fixture.anchor.authority(),
        fixture.anchor.head(),
        attempts,
        &fixtures::key(4),
        Some(reader),
        Some(&fixture.anchor),
    )
    .unwrap()
}
fn restore(
    fixture: &mut Fixture,
    candidate: FormalCandidateMaterialV1,
    reader: Arc<IndexReader>,
    proof: FormalQualifiedCommitProofV1,
) {
    let verified = verify_formal_qualified(
        &proof,
        fixture.anchor.authority(),
        Some(fixture.anchor.head()),
        None,
    )
    .unwrap();
    let anchor = fixture
        .anchor
        .after_verified_commit(fixture.anchor.authority().clone(), verified)
        .unwrap();
    let snapshot: Snapshot =
        capture::decode_role(&candidate.material, ArtifactRole::Snapshot).unwrap();
    let journal: Journal =
        capture::decode_role(&candidate.material, ArtifactRole::Journal).unwrap();
    let retained =
        VerifiedFormalRetainedHistoryParentV1::verify(&anchor, &to_cbor(&snapshot).unwrap())
            .unwrap();
    let world = World::from_verified_formal_snapshot(snapshot, journal, retained, reader).unwrap();
    let record: super::super::super::ExecutionBridgeRecord =
        super::super::super::formal_business_package::execution_result(&candidate.material, true)
            .unwrap();
    fixture.previous_execution = record.execution_block_hash;
    fixture.previous_node = record.node_block_hash.unwrap();
    fixture.world = world;
    fixture.anchor = anchor;
    fixture.proof = proof;
    fixture.candidate = candidate;
}
fn run(heights: u64) {
    let scratch = Scratch::new();
    let mut corpus = Corpus::new(scratch.path());
    let mut fixture = initial();
    corpus.capture(&fixture.candidate, &fixture.proof);
    let index = Arc::new(IndexReader(corpus.primary_objects()));
    let parent = VerifiedFormalRetainedHistoryParentV1::verify(
        &fixture.anchor,
        &to_cbor(fixture.world.snapshot()).unwrap(),
    )
    .unwrap();
    fixture.world = World::from_verified_formal_snapshot(
        fixture.world.snapshot(),
        fixture.world.journal().clone(),
        parent,
        index.clone(),
    )
    .unwrap();
    let reader = super::super::super::formal_business_package::SharedBusinessReader(index.clone());
    let execution = Instant::now();
    for height in 1..=heights {
        let candidate = execute_step(&fixture, &reader, &scratch.path().join("attempts"));
        let proof = fixtures::qualified_proof(
            candidate.candidate.record.clone(),
            candidate.frozen.clone(),
            fixture.anchor.authority(),
            Some(fixture.anchor.head()),
            None,
            4,
            5,
            6,
        );
        corpus.capture(&candidate, &proof);
        restore(&mut fixture, candidate, index.clone(), proof);
        if height % 100 == 0 {
            eprintln!("storage-capacity actual formal heights={height}/{heights}");
        }
    }
    let execution_ns = execution.elapsed().as_nanos();
    assert_eq!(fixture.world.state().time, heights);
    eprintln!("STORAGE_CAPACITY_PHASE=source_capture_complete heights={heights}");
    let reports = corpus.compare();
    let report = serde_json::json!({
        "schema_version":1,"executed_heights":heights,"head":fixture.anchor.head().head(),
        "scope":"actual World/profile2 artifacts, all five typed phase envelopes and CAS nodes; fixture signers; same-filesystem P/R directories, not S6 fault-domain evidence; source retained; namespace benchmark excludes native executable/environment chunk corpus and native checkpoint/provider/generation activation evidence",
        "execution_and_raw_capture_ns":execution_ns,"domains":reports,
        "unmeasured": { "peak_rss_bytes":null, "device_write_bytes":null,
            "write_amplification":null, "shipping_endpoint_commit_latency":null, "native_environment_archive_bytes":null },
        "attempt_scratch":inventory_storage(&scratch.path().join("attempts")),
    });
    eprintln!(
        "STORAGE_CAPACITY_REPORT={}",
        serde_json::to_string(&report).unwrap()
    );
}
#[test]
#[cfg(unix)]
fn actual_formal_storage_capacity_smoke_10_heights() {
    std::thread::Builder::new()
        .name("storage-capacity".into())
        .stack_size(32 * 1024 * 1024)
        .spawn(|| run(10))
        .unwrap()
        .join()
        .unwrap();
}
#[test]
#[cfg(unix)]
#[ignore = "manual real World capacity run; set OASIS7_STORAGE_BENCHMARK_HEIGHTS=1000 or 10000 and retain emitted report"]
fn actual_formal_storage_capacity_1000_or_10000_heights() {
    let heights: u64 = std::env::var("OASIS7_STORAGE_BENCHMARK_HEIGHTS")
        .unwrap_or_else(|_| "1000".into())
        .parse()
        .unwrap();
    assert!([1000, 10000].contains(&heights));
    std::thread::Builder::new()
        .name("storage-capacity-long".into())
        .stack_size(32 * 1024 * 1024)
        .spawn(move || run(heights))
        .unwrap()
        .join()
        .unwrap();
}
