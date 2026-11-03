//! Native original-byte closure only; the fixture executable is never run.
use super::*;
use oasis7_formal_protocol::{
    controlled_authority::formal::test_fixtures::incremental::setup_with_native,
    incremental_material::verify_native_environment,
    native_environment::{
        NativeEnvironmentChunkV1, NativeEnvironmentComponentV1, NativeEnvironmentRole,
        NativeExecutionEnvironmentV1,
    },
};
#[test]
fn native_larger_than_pool_is_verified_from_original_chunks_after_reopen() {
    let dir = Fixture::new();
    let memory = MemoryBudget::new(64 << 20, 4 << 20).unwrap();
    let _fixture_charge = memory.reserve(4 << 20, MemoryClass::Ordinary).unwrap();
    let full = vec![0x5a; 1 << 20];
    let tail = vec![0x37; 17];
    let metadata = b"exact original build";
    let chunk = |bytes: &[u8]| NativeEnvironmentChunkV1 {
        digest: blake3::hash(bytes).to_hex().to_string(),
        byte_len: bytes.len() as u32,
    };
    let mut hash = blake3::Hasher::new();
    for _ in 0..64 {
        hash.update(&full);
    }
    hash.update(&tail);
    let executable = *hash.finalize().as_bytes();
    let native = NativeExecutionEnvironmentV1 {
        schema_version: 1,
        target: "fixture-native".into(),
        os: "fixture".into(),
        arch: "fixture".into(),
        components: vec![
            NativeEnvironmentComponentV1 {
                role: NativeEnvironmentRole::Executable,
                name: "runtime".into(),
                byte_len: (64 << 20) + 17,
                content_digest: hex::encode(executable),
                chunks: (0..64)
                    .map(|_| chunk(&full))
                    .chain(std::iter::once(chunk(&tail)))
                    .collect(),
            },
            NativeEnvironmentComponentV1 {
                role: NativeEnvironmentRole::BuildMetadata,
                name: "build".into(),
                byte_len: metadata.len() as u64,
                content_digest: blake3::hash(metadata).to_hex().to_string(),
                chunks: vec![chunk(metadata)],
            },
        ],
    };
    let descriptor = native.encode().unwrap();
    let id = *blake3::hash(&descriptor).as_bytes();
    let s = setup_with_native(memory.clone(), id);
    let (blobs, catalog) = core_objects(&s, &s.history);
    let cancelled = AtomicBool::new(false);
    let mut physical = physical_io();
    let mut logical = logical_io();
    let store =
        IncrementalPageStore::create(&dir.store(), memory.clone(), PageCompression::Raw).unwrap();
    let objects = IncrementalObjectStore::create(&store, &mut physical, &cancelled).unwrap();
    for bytes in [
        full.as_slice(),
        tail.as_slice(),
        metadata.as_slice(),
        descriptor.as_slice(),
    ]
    .into_iter()
    .chain(blobs.iter().map(|bytes| bytes.as_slice()))
    {
        objects
            .write_object(
                &mut &bytes[..],
                bytes.len() as u64,
                Some(*blake3::hash(bytes).as_bytes()),
                &mut ObjectIo {
                    physical: &mut physical,
                    logical: &mut logical,
                    class: MemoryClass::Ordinary,
                    cancelled: &cancelled,
                },
            )
            .unwrap();
    }
    let mut pages = store
        .state_session(physical_io(), MemoryClass::Ordinary)
        .unwrap();
    pages.persist_catalog_pages(&catalog).unwrap();
    drop(pages);
    drop(objects);
    drop(store);
    let store = IncrementalPageStore::open(
        &dir.store(),
        memory.clone(),
        PageCompression::Raw,
        MemoryClass::Ordinary,
    )
    .unwrap();
    let objects =
        IncrementalObjectStore::open(&store, MemoryClass::Ordinary, &mut physical, &cancelled)
            .unwrap();
    let pages = store
        .state_session(physical_io(), MemoryClass::Ordinary)
        .unwrap();
    let signed = formal::SignedCanonicalCandidateV1 {
        record: s.commit.record().clone(),
        writer_signature_hex: s.proof.decision.writer_signature_hex.clone(),
    };
    let candidate = formal::verify_formal_candidate(
        &signed,
        None,
        &s.authority,
        None,
        None,
        formal::FormalCompletionBranchV1::Normal,
    )
    .unwrap();
    let mut io = ObjectIo {
        physical: &mut physical,
        logical: &mut logical,
        class: MemoryClass::Ordinary,
        cancelled: &cancelled,
    };
    let mut reader = objects.original_reader(&mut io);
    let mut budget = CoreBudget {
        objects_remaining: 32,
        original_bytes_remaining: 1 << 20,
        new_pages_remaining: 2048,
        merkle_reads_remaining: 10000,
    };
    let core = verify_commit_core(
        &CoreRequest {
            candidate: &candidate,
            original_authority: &s.authority,
            parent: None,
            max_pending_node_bytes: 4 << 20,
        },
        &mut reader,
        &pages,
        &memory,
        &mut budget,
        || false,
    )
    .unwrap();
    let bytes = descriptor.len() as u64
        + native
            .components
            .iter()
            .map(|component| component.byte_len)
            .sum::<u64>();
    let mut budget = CoreBudget {
        objects_remaining: 67,
        original_bytes_remaining: bytes,
        new_pages_remaining: 0,
        merkle_reads_remaining: 0,
    };
    let before = memory.used();
    let checked =
        verify_native_environment(&core, &mut reader, &memory, &mut budget, || false).unwrap();
    assert_eq!(checked.body_id(), s.history.body.digest().unwrap());
    assert_eq!(checked.environment_id(), id);
    assert_eq!(checked.executable_id(), executable);
    assert_eq!(checked.chunk_occurrences(), 66);
    assert_eq!(
        checked.component_bytes(),
        (64 << 20) + 17 + metadata.len() as u64
    );
    assert_eq!(
        (budget.objects_remaining, budget.original_bytes_remaining),
        (0, 0)
    );
    assert_eq!(memory.used(), before + checked.reserved_bytes());
    drop(checked);
    assert_eq!(memory.used(), before);
}
