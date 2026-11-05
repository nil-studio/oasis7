//! Real signed IWS originals reach the page-backed owner gate, never a receipt.
use super::*;
use crate::incremental_page_store::{IncrementalPageStore, PageCompression, PageIoBudget};
use oasis7_formal_protocol::controlled_authority::formal::{
    initial_activation_input_bytes, test_fixtures::incremental as iws,
};
use oasis7_formal_protocol::native_environment::*;
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass};
#[test]
fn signed_iws_genesis_and_cancelled_prepare_never_write_incomplete_owner_receipts() {
    let fixture = Fixture::new();
    let chunk = b"original endpoint executable metadata".to_vec();
    let chunk_id = blake3::hash(&chunk).to_hex().to_string();
    let descriptor = NativeExecutionEnvironmentV1 {
        schema_version: 1,
        target: "fixture-linux".into(),
        os: "linux".into(),
        arch: "fixture".into(),
        components: [
            NativeEnvironmentRole::Executable,
            NativeEnvironmentRole::BuildMetadata,
        ]
        .into_iter()
        .enumerate()
        .map(|(i, role)| NativeEnvironmentComponentV1 {
            role,
            name: format!("part-{i}"),
            byte_len: chunk.len() as u64,
            content_digest: chunk_id.clone(),
            chunks: vec![NativeEnvironmentChunkV1 {
                digest: chunk_id.clone(),
                byte_len: chunk.len() as u32,
            }],
        })
        .collect(),
    };
    let native = descriptor.encode().unwrap();
    let manifest = b"manifest original";
    let wasm = b"wasm original";
    let rules = b"rules original";
    let digest = |b: &[u8]| *blake3::hash(b).as_bytes();
    let memory = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
    let mut setup = iws::setup_with_execution_artifacts(
        memory.clone(),
        digest(&native),
        digest(manifest),
        digest(wasm),
        digest(rules),
        [81; 32],
    );
    // Bind the real endpoint directories before signing the activation and
    // preparing its original history; a synthetic fingerprint cannot open it.
    use oasis7_formal_protocol::controlled_authority::formal::{
        self as formal, test_fixtures as signed_fixture,
    };
    use oasis7_formal_protocol::incremental_execution::{ExecutionFactKindV1, ExecutionFactRefV1};
    use oasis7_formal_protocol::incremental_history::{CatalogRootV1, HeaderPreparation};
    let mut certificate = setup.certificate.clone();
    certificate
        .body
        .recovery_policy
        .authorized_fencing_resource_bindings = vec![fixture.provider.resource_binding().into()];
    certificate.body.authority.recovery_policy_digest =
        certificate.body.recovery_policy.digest().unwrap();
    certificate
        .body
        .authority
        .primary
        .instance
        .storage_fingerprint = fencing::directory_fingerprint(&fixture.pdir).unwrap();
    certificate
        .body
        .authority
        .replica
        .instance
        .storage_fingerprint = fencing::directory_fingerprint(&fixture.rdir).unwrap();
    certificate.signature_hex = signed_fixture::sign(
        formal::initial_activation_signing_bytes(&certificate.body).unwrap(),
        9,
    );
    let trust = formal::InitialActivationTrustV1 {
        issuer_public_key: signed_fixture::public(9),
        world: certificate.body.authority.world.clone(),
        initial_state_root: certificate.body.initial_state_root.clone(),
        governing_manifest_digest: certificate.body.governing_manifest_digest.clone(),
        recovery_policy_digest: certificate.body.recovery_policy.digest().unwrap(),
    };
    let authority = formal::verify_initial_activation(&certificate, &trust).unwrap();
    let input = initial_activation_input_bytes(&certificate).unwrap();
    let input_id = digest(&input);
    let history = HeaderPreparation {
        config: &setup.config,
        catalog: &CatalogRootV1::empty(setup.config.scope().unwrap()).unwrap(),
        roots: &setup.roots,
        parent: None,
        parent_history: None,
        transition: None,
        genesis_facts: &[ExecutionFactRefV1 {
            kind: ExecutionFactKindV1::Input,
            id: input_id,
        }],
        operation: [51; 32],
        authority: formal::incremental::authority_binding(&authority, &memory).unwrap(),
        execution_environment: setup.config.execution_environment,
        checkpoint: true,
    }
    .prepare(&memory)
    .unwrap();
    let mut record = setup.proof.decision.record.clone();
    record.original_authority_id = authority.authority_id().unwrap();
    record.activation_ref = authority.activation_ref().into();
    record.payload_digest = hex::encode(input_id);
    record
        .artifact_roots
        .insert(ArtifactRole::Input, hex::encode(input_id));
    record.artifact_roots.insert(
        ArtifactRole::IncrementalCommitBody,
        hex::encode(history.body.digest().unwrap()),
    );
    record.artifact_roots.insert(
        ArtifactRole::IncrementalHistoryPrefix,
        hex::encode(history.prefix.digest().unwrap()),
    );
    let proof = signed_fixture::proof(
        record,
        &authority,
        FormalCompletionBranchV1::Normal,
        None,
        None,
        4,
        5,
        6,
    );
    formal::verify_formal_qualified(&proof, &authority, None, None).unwrap();
    let record = &proof.decision.record;

    let mut objects = BTreeMap::new();
    let mut put = |bytes: Vec<u8>, mut references: Vec<String>| {
        references.sort();
        references.dedup();
        let content_hash = blake3::hash(&bytes).to_hex().to_string();
        objects.insert(
            content_hash.clone(),
            ArtifactObject {
                content_hash,
                bytes,
                references,
            },
        );
    };
    setup.certificate = certificate.clone();
    let (originals, catalog) = iws::core_objects(&setup, &history);
    let body_id = history.body.digest().unwrap();
    let dependencies = originals
        .iter()
        .map(|bytes| hex::encode(digest(bytes)))
        .filter(|id| id != &hex::encode(body_id))
        .collect::<Vec<_>>();
    for bytes in originals.iter() {
        put(
            bytes.to_vec(),
            if digest(bytes) == body_id {
                dependencies.clone()
            } else {
                vec![]
            },
        );
    }
    for bytes in [manifest.to_vec(), wasm.to_vec(), rules.to_vec()] {
        put(bytes, vec![]);
    }
    // Native chunk originals use the existing external source contract.
    // This rejected endpoint candidate never grants their availability.
    put(native, vec![]);
    let material = ClosedRecord {
        payload_digest: record.payload_digest.clone(),
        before_state_root: record.before_state_root.clone(),
        after_state_root: record.after_state_root.clone(),
        roots: record.artifact_roots.clone(),
        objects: objects.into_values().collect(),
    };
    material.validate_iws().unwrap();
    let candidate = FormalCandidateMaterialV1 {
        candidate: SignedCanonicalCandidateV1 {
            record: record.clone(),
            writer_signature_hex: proof.decision.writer_signature_hex.clone(),
        },
        material,
        frozen: None,
        frozen_material: None,
        branch: FormalCompletionBranchV1::Normal,
    };
    let mut endpoint = FileFormalEndpoint::open_inner(
        &fixture.pdir,
        FormalEndpointAuthorizationV1 {
            authority: authority.clone(),
            original: authority.clone(),
            recovery: None,
            parent: None,
        },
        EndpointRole::Primary,
        key(5),
        fixture.provider.clone(),
        true,
        None,
    )
    .unwrap();
    let request = candidate.candidate.record.request_digest.clone();
    assert!(
        endpoint
            .prepare_with_cancel(candidate.clone(), || true)
            .unwrap_err()
            .to_string()
            .contains("cancelled before verification")
    );
    assert!(matches!(
        endpoint.query(&request).unwrap(),
        FormalQueryOutcomeV1::NotRecorded
    ));
    assert!(
        endpoint
            .prepare(candidate.clone())
            .unwrap_err()
            .to_string()
            .contains("page-store capability missing")
    );
    let pages = IncrementalPageStore::create(
        &fixture.path.join("iws-pages"),
        memory,
        PageCompression::Raw,
    )
    .unwrap();
    let io = || PageIoBudget {
        reads_remaining: 10000,
        read_bytes_remaining: 16 << 20,
        writes_remaining: 10000,
        write_bytes_remaining: 16 << 20,
    };
    let mut page_io = io();
    let no_cancel = std::sync::atomic::AtomicBool::new(false);
    for node in &catalog.new_nodes {
        let charge = pages
            .memory_budget()
            .reserve(node.bytes.capacity() + 128, MemoryClass::Ordinary)
            .unwrap();
        pages
            .put_page(
                digest(&node.bytes),
                &charge.retain(node.bytes.clone()),
                &mut page_io,
                &no_cancel,
            )
            .unwrap();
    }
    let budgets = IwsEndpointVerificationBudgetsV1 {
        original_objects: 100,
        original_bytes: 16 << 20,
        new_pages: 10000,
        core_merkle_reads: 10000,
        state_scan_nodes: 10000,
        state_scan_bytes: 16 << 20,
        state_scan_leaves: 10000,
        full_value_occurrences: 10000,
        full_value_bytes: 16 << 20,
        max_full_value_bytes: 1 << 20,
        transition_value_occurrences: 10000,
        transition_value_bytes: 16 << 20,
        max_transition_value_bytes: 1 << 20,
        value_nodes: 10000,
        value_read_bytes: 16 << 20,
        max_pending_node_bytes: 4 << 20,
        state_page_io: io(),
        value_page_io: io(),
    };
    for (name, pending, succeeds) in [("zero", 0, false), ("largest", usize::MAX, true)] {
        let boundary_pages = IncrementalPageStore::create(
            &fixture.path.join(name),
            pages.memory_budget().clone(),
            PageCompression::Raw,
        )
        .unwrap();
        let mut boundary = budgets.clone();
        boundary.max_pending_node_bytes = pending;
        assert_eq!(
            IwsPageStoreCapabilityV1::new(boundary_pages, boundary).is_ok(),
            succeeds
        );
    }
    endpoint
        .install_iws_page_store(IwsPageStoreCapabilityV1::new(pages, budgets).unwrap())
        .unwrap();
    let rejected = endpoint.prepare(candidate).unwrap_err();
    assert!(
        rejected.to_string().contains("IWS typed owner coverage"),
        "{rejected}"
    );
    assert!(matches!(
        endpoint.query(&request).unwrap(),
        FormalQueryOutcomeV1::NotRecorded
    ));
}
