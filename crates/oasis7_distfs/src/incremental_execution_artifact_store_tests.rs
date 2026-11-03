//! Real artifact producers/readers; no executable or policy is executed here.
use super::*;
use oasis7_formal_protocol::{
    controlled_authority::formal::test_fixtures::incremental::{
        genesis_config, native_fixture, setup_with_execution_artifacts,
    },
    incremental_artifacts::*,
    incremental_material::{
        ExecutionArtifactBudget, ExecutionArtifactReadContext, OriginalArtifactBudget,
        verify_execution_artifacts, verify_native_environment,
    },
};
use std::io::Read;
fn wasm() -> impl Read {
    // A core WASM module containing one large custom section with empty name.
    let mut bytes = b"\0asm\x01\0\0\0".to_vec();
    bytes.push(0);
    let mut length = 64u32 << 20;
    loop {
        let mut byte = (length & 127) as u8;
        length >>= 7;
        if length != 0 {
            byte |= 128;
        }
        bytes.push(byte);
        if length == 0 {
            break;
        }
    }
    bytes.push(0);
    std::io::Cursor::new(bytes).chain(std::io::repeat(0x71).take((64 << 20) - 1))
}
#[test]
fn iws_manifest_and_large_wasm_catalog_reopen_with_exact_streaming_dependencies() {
    let dir = Fixture::new();
    let memory = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
    let _fixture = memory.reserve(512 * 1024, MemoryClass::Ordinary).unwrap();
    let (native, raw) = native_fixture();
    let descriptor = native.encode().unwrap();
    let native_id = *blake3::hash(&descriptor).as_bytes();
    let mut hash = blake3::Hasher::new();
    let mut input = wasm();
    let mut buffer = vec![0; 65536];
    let mut wasm_len = 0;
    loop {
        let count = input.read(&mut buffer).unwrap();
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
        wasm_len += count as u64;
    }
    let wasm_id = *hash.finalize().as_bytes();
    assert!(wasm_len > 64 << 20);
    let policy = b"original policy";
    let policy_id = *blake3::hash(policy).as_bytes();
    let cancelled = AtomicBool::new(false);
    let mut physical = physical_io();
    let mut logical = logical_io();
    let store =
        IncrementalPageStore::create(&dir.store(), memory.clone(), PageCompression::Raw).unwrap();
    let mut pages = store
        .state_session(physical_io(), MemoryClass::Ordinary)
        .unwrap();
    let wd = ArtifactCatalogV1::empty(ArtifactKindV1::Wasm)
        .unwrap()
        .append(
            &[ArtifactEntryV1 {
                kind: ArtifactKindV1::Wasm,
                original: OriginalArtifactRefV1 {
                    id: wasm_id,
                    byte_len: wasm_len,
                },
            }],
            &pages,
            &memory,
            4 << 20,
        )
        .unwrap();
    let rd = ArtifactCatalogV1::empty(ArtifactKindV1::Rules)
        .unwrap()
        .append(
            &[ArtifactEntryV1 {
                kind: ArtifactKindV1::Rules,
                original: OriginalArtifactRefV1 {
                    id: policy_id,
                    byte_len: policy.len() as u64,
                },
            }],
            &pages,
            &memory,
            4 << 20,
        )
        .unwrap();
    pages.persist_artifact_pages(&wd).unwrap();
    pages.persist_artifact_pages(&rd).unwrap();
    let config = genesis_config(native_id);
    let manifest = IwsExecutionManifestV1 {
        scope: config.scope().unwrap(),
        format_profile: config.format_profile,
        native: native_id,
        executable: hex::decode(&native.components[0].content_digest)
            .unwrap()
            .try_into()
            .unwrap(),
        wasm: wd.after.digest().unwrap(),
        rules: rd.after.digest().unwrap(),
        policy: OriginalArtifactRefV1 {
            id: policy_id,
            byte_len: policy.len() as u64,
        },
        mode: ExecutionManifestModeV1::Genesis,
    };
    let s = setup_with_execution_artifacts(
        memory.clone(),
        native_id,
        manifest.digest().unwrap(),
        manifest.wasm,
        manifest.rules,
        policy_id,
    );
    let (blobs, catalog) = core_objects(&s, &s.history);
    pages.persist_catalog_pages(&catalog).unwrap();
    let objects = IncrementalObjectStore::create(&store, &mut physical, &cancelled).unwrap();
    let mut io = ObjectIo {
        physical: &mut physical,
        logical: &mut logical,
        class: MemoryClass::Ordinary,
        cancelled: &cancelled,
    };
    objects
        .write_object(&mut wasm(), wasm_len, Some(wasm_id), &mut io)
        .unwrap();
    let manifest_bytes = manifest.canonical_bytes().unwrap();
    let wasm_root = wd.after.canonical_bytes().unwrap();
    let rules_root = rd.after.canonical_bytes().unwrap();
    for bytes in raw
        .iter()
        .map(Vec::as_slice)
        .chain([
            descriptor.as_slice(),
            policy.as_slice(),
            manifest_bytes.as_slice(),
            wasm_root.as_slice(),
            rules_root.as_slice(),
        ])
        .chain(blobs.iter().map(|bytes| bytes.as_slice()))
    {
        objects
            .write_object(
                &mut &bytes[..],
                bytes.len() as u64,
                Some(*blake3::hash(bytes).as_bytes()),
                &mut io,
            )
            .unwrap();
    }
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
    let native =
        verify_native_environment(&core, &mut reader, &memory, &mut budget, || false).unwrap();
    let mut budget = ExecutionArtifactBudget {
        structural: CoreBudget {
            objects_remaining: 3,
            original_bytes_remaining: (manifest_bytes.len() + wasm_root.len() + rules_root.len())
                as u64,
            new_pages_remaining: 0,
            merkle_reads_remaining: 771,
        },
        originals: OriginalArtifactBudget {
            objects_remaining: 2,
            bytes_remaining: wasm_len + policy.len() as u64,
            max_object_bytes: wasm_len,
        },
    };
    let before = memory.used();
    let checked = verify_execution_artifacts(
        &core,
        &native,
        ExecutionArtifactReadContext {
            objects: &mut reader,
            pages: &pages,
            memory: &memory,
        },
        &mut budget,
        || false,
    )
    .unwrap();
    assert_eq!(checked.manifest_id(), manifest.digest().unwrap());
    assert_eq!(checked.body_id(), s.history.body.digest().unwrap());
    assert_eq!(checked.catalog_counts(), [1, 1]);
    assert_eq!(checked.original_bytes(), wasm_len + policy.len() as u64);
    assert_eq!(
        (
            budget.structural.objects_remaining,
            budget.structural.original_bytes_remaining,
            budget.structural.merkle_reads_remaining,
            budget.originals.objects_remaining,
            budget.originals.bytes_remaining
        ),
        (0, 0, 0, 0, 0)
    );
    assert_eq!(memory.used(), before + checked.reserved_bytes());
    drop(checked);
    assert_eq!(memory.used(), before);
}
