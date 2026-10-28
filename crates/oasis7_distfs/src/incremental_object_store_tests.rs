use super::super::tests::{Fixture, charged, logical_io, physical_io};
use super::*;
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFactKindV1, ExecutionFactRefV1},
    incremental_history::{FactSetV1, WorldScopeV1},
};

#[test]
fn canonical_history_fact_set_larger_than_a_page_reopens_with_its_original_identity() {
    let fixture = Fixture::new();
    let memory = MemoryBudget::new(12 << 20, 3 << 20).unwrap();
    let pages =
        IncrementalPageStore::create(&fixture.store(), memory.clone(), PageCompression::Raw)
            .unwrap();
    let cancelled = AtomicBool::new(false);
    let mut physical = physical_io();
    let mut logical = logical_io();
    let objects = IncrementalObjectStore::create(&pages, &mut physical, &cancelled).unwrap();
    let _fact_charge = memory.reserve(4096 * 64, MemoryClass::Ordinary).unwrap();
    let facts = FactSetV1 {
        scope: WorldScopeV1 {
            world_id: "original-history".into(),
            genesis_config: [3; 32],
        },
        operation: [7; 32],
        facts: (0..4096)
            .map(|index| ExecutionFactRefV1 {
                kind: ExecutionFactKindV1::Event,
                id: *blake3::hash(&(index as u32).to_be_bytes()).as_bytes(),
            })
            .collect(),
    };
    let canonical = facts.canonical_bytes(&memory).unwrap();
    assert!(canonical.len() > VALUE_MAX_NODE_BYTES);
    let expected = *blake3::hash(&canonical).as_bytes();
    let mut io = ObjectIo {
        physical: &mut physical,
        logical: &mut logical,
        class: MemoryClass::Ordinary,
        cancelled: &cancelled,
    };
    assert_eq!(
        objects
            .write_object(
                &mut canonical.as_slice(),
                canonical.len() as u64,
                Some(expected),
                &mut io
            )
            .unwrap(),
        expected
    );
    drop(objects);
    drop(pages);
    let pages = IncrementalPageStore::open(
        &fixture.store(),
        memory.clone(),
        PageCompression::Raw,
        MemoryClass::Recovery,
    )
    .unwrap();
    let objects =
        IncrementalObjectStore::open(&pages, MemoryClass::Recovery, &mut physical, &cancelled)
            .unwrap();
    let mut io = ObjectIo {
        physical: &mut physical,
        logical: &mut logical,
        class: MemoryClass::Recovery,
        cancelled: &cancelled,
    };
    let read = objects
        .read_object_owned(&expected, canonical.len(), &mut io)
        .unwrap();
    assert_eq!(read.as_slice(), canonical.as_slice());
    assert_eq!(
        *FactSetV1::from_canonical_bytes(&read, &memory).unwrap(),
        facts
    );
    let before = fs::read_dir(fixture.store().join("objects"))
        .unwrap()
        .count();
    objects
        .write_object(
            &mut canonical.as_slice(),
            canonical.len() as u64,
            Some(expected),
            &mut io,
        )
        .unwrap();
    assert_eq!(
        fs::read_dir(fixture.store().join("objects"))
            .unwrap()
            .count(),
        before
    );
    assert!(matches!(
        objects.write_object(&mut &b"short"[..], 6, None, &mut io),
        Err(PageStoreError::Io(_))
    ));
    io.logical.nodes_remaining = 0;
    assert!(matches!(
        objects.write_object(&mut &b"later"[..], 5, None, &mut io),
        Err(PageStoreError::ResourceLimited(_))
    ));
    assert_eq!(
        fs::read_dir(fixture.store().join("objects"))
            .unwrap()
            .count(),
        before
    );
}

#[test]
fn locator_substitution_with_a_valid_checksum_cannot_return_an_unverified_owned_object() {
    let fixture = Fixture::new();
    let memory = MemoryBudget::new(8 << 20, 0).unwrap();
    let pages =
        IncrementalPageStore::create(&fixture.store(), memory.clone(), PageCompression::Raw)
            .unwrap();
    let cancelled = AtomicBool::new(false);
    let mut physical = physical_io();
    let mut logical = logical_io();
    let objects = IncrementalObjectStore::create(&pages, &mut physical, &cancelled).unwrap();
    let mut io = ObjectIo {
        physical: &mut physical,
        logical: &mut logical,
        class: MemoryClass::Ordinary,
        cancelled: &cancelled,
    };
    let first = objects
        .write_object(&mut &b"original"[..], 8, None, &mut io)
        .unwrap();
    let second = objects
        .write_object(&mut &b"replaced"[..], 8, None, &mut io)
        .unwrap();
    let aliases = fixture.store().join("objects");
    let other = decode_locator(
        &second,
        &fs::read(aliases.join(hex::encode(second))).unwrap(),
    )
    .unwrap();
    fs::write(
        aliases.join(hex::encode(first)),
        encode_locator(&first, &other),
    )
    .unwrap();
    let baseline = memory.used();
    assert!(matches!(
        objects.read_object_owned(&first, 8, &mut io),
        Err(PageStoreError::Corrupt(
            "original object reconstruction hash"
        ))
    ));
    assert_eq!(memory.used(), baseline);
    let mut provisional = Vec::new();
    assert!(
        objects
            .read_object_into(&first, 8, &mut provisional, &mut io)
            .is_err()
    );
    // A streaming sink is explicitly provisional, not an authoritative value.
    assert_eq!(provisional, b"replaced");
}

#[test]
fn empty_objects_and_exact_length_imports_do_not_consume_the_next_object() {
    let fixture = Fixture::new();
    let memory = MemoryBudget::new(8 << 20, 0).unwrap();
    let pages =
        IncrementalPageStore::create(&fixture.store(), memory, PageCompression::Raw).unwrap();
    let cancelled = AtomicBool::new(false);
    let mut physical = physical_io();
    let mut logical = logical_io();
    let objects = IncrementalObjectStore::create(&pages, &mut physical, &cancelled).unwrap();
    let mut io = ObjectIo {
        physical: &mut physical,
        logical: &mut logical,
        class: MemoryClass::Ordinary,
        cancelled: &cancelled,
    };
    let mut input = &b"firstsecond"[..];
    let first = objects.write_object(&mut input, 5, None, &mut io).unwrap();
    assert_eq!(input, b"second");
    assert_eq!(
        objects
            .read_object_owned(&first, 5, &mut io)
            .unwrap()
            .as_slice(),
        b"first"
    );
    let empty = objects.write_object(&mut input, 0, None, &mut io).unwrap();
    assert_eq!(empty, *blake3::hash(b"").as_bytes());
    assert_eq!(input, b"second");
    assert!(
        objects
            .read_object_owned(&empty, 0, &mut io)
            .unwrap()
            .is_empty()
    );
    let before = fs::read_dir(fixture.store().join("objects"))
        .unwrap()
        .count();
    assert!(
        objects
            .write_object(&mut input, 6, Some([99; 32]), &mut io)
            .is_err()
    );
    assert_eq!(
        fs::read_dir(fixture.store().join("objects"))
            .unwrap()
            .count(),
        before
    );
}

#[test]
fn missing_format_bounds_cancellation_and_retained_output_do_not_bypass_the_shared_pool() {
    let fixture = Fixture::new();
    let memory = MemoryBudget::new(8 << 20, 3 << 20).unwrap();
    let pages =
        IncrementalPageStore::create(&fixture.store(), memory.clone(), PageCompression::Raw)
            .unwrap();
    let cancelled = AtomicBool::new(false);
    let mut physical = physical_io();
    let mut logical = logical_io();
    assert!(
        IncrementalObjectStore::open(&pages, MemoryClass::Ordinary, &mut physical, &cancelled)
            .is_err()
    );
    assert!(!fixture.store().join("objects").exists());
    let objects = IncrementalObjectStore::create(&pages, &mut physical, &cancelled).unwrap();
    let input = charged(&memory, vec![51; 100000]);
    let mut io = ObjectIo {
        physical: &mut physical,
        logical: &mut logical,
        class: MemoryClass::Ordinary,
        cancelled: &cancelled,
    };
    let id = objects
        .write_object(&mut input.as_slice(), input.len() as u64, None, &mut io)
        .unwrap();
    assert!(matches!(
        objects.read_object_owned(&[99; 32], 100000, &mut io),
        Err(PageStoreError::Missing(_))
    ));
    let baseline = memory.used();
    assert!(matches!(
        objects.read_object_owned(&id, input.len() - 1, &mut io),
        Err(PageStoreError::ResourceLimited(_))
    ));
    assert_eq!(memory.used(), baseline);
    let output = objects
        .read_object_owned(&id, input.len(), &mut io)
        .unwrap();
    let reader = output.clone();
    drop(output);
    assert!(memory.used() >= baseline + input.len());
    drop(reader);
    assert_eq!(memory.used(), baseline);
    cancelled.store(true, Ordering::Release);
    assert!(matches!(
        objects.read_object_owned(&id, input.len(), &mut io),
        Err(PageStoreError::Cancelled)
    ));
    cancelled.store(false, Ordering::Release);
    let ordinary = memory
        .reserve((5 << 20) - baseline, MemoryClass::Ordinary)
        .unwrap();
    assert!(matches!(
        objects.read_object_owned(&id, input.len(), &mut io),
        Err(PageStoreError::Memory(_))
    ));
    io.class = MemoryClass::Recovery;
    assert_eq!(
        objects
            .read_object_owned(&id, input.len(), &mut io)
            .unwrap()
            .as_slice(),
        input.as_slice()
    );
    drop(ordinary);
    fs::write(fixture.store().join("objects-format"), b"unknown").unwrap();
    assert!(matches!(
        objects.read_object_owned(&id, input.len(), &mut io),
        Err(PageStoreError::Corrupt(_))
    ));
}

#[test]
fn actual_header_producer_material_is_persisted_and_read_back_without_a_head() {
    use oasis7_formal_protocol::{
        incremental_execution::{AuthorityBinding, RootSetV1},
        incremental_history::{CatalogRootV1, CommitBodyV1, GenesisConfigV1, HeaderPreparation},
    };
    let fixture = Fixture::new();
    let memory = MemoryBudget::new(12 << 20, 0).unwrap();
    let pages =
        IncrementalPageStore::create(&fixture.store(), memory.clone(), PageCompression::Raw)
            .unwrap();
    let cancelled = AtomicBool::new(false);
    let mut physical = physical_io();
    let mut logical = logical_io();
    let objects = IncrementalObjectStore::create(&pages, &mut physical, &cancelled).unwrap();
    let config = GenesisConfigV1 {
        world_id: "persisted-header".into(),
        format_profile: [1; 32],
        initial_content: [2; 32],
        activation_config: [3; 32],
        execution_environment: [4; 32],
    };
    let scope = config.scope().unwrap();
    let roots = RootSetV1::empty(scope.world_id.clone(), scope.genesis_config).unwrap();
    let prepared = HeaderPreparation {
        config: &config,
        catalog: &CatalogRootV1::empty(scope).unwrap(),
        roots: &roots,
        parent: None,
        parent_history: None,
        transition: None,
        genesis_facts: &[],
        operation: [6; 32],
        authority: AuthorityBinding {
            token: [5; 32],
            epoch: 1,
        },
        execution_environment: config.execution_environment,
        checkpoint: true,
    }
    .prepare(&memory)
    .unwrap();
    let mut io = ObjectIo {
        physical: &mut physical,
        logical: &mut logical,
        class: MemoryClass::Ordinary,
        cancelled: &cancelled,
    };
    let id = objects
        .persist_history_material(&prepared, &mut io)
        .unwrap();
    assert_eq!(id, prepared.body.digest().unwrap());
    let body = objects.read_object_owned(&id, 4096, &mut io).unwrap();
    assert_eq!(
        CommitBodyV1::from_canonical_bytes(&body).unwrap(),
        prepared.body
    );
    for id in [
        prepared.body.facts,
        prepared.segment.digest().unwrap(),
        prepared.body.history,
        prepared.body.checkpoint.unwrap(),
    ] {
        let bytes = objects.read_object_owned(&id, 4096, &mut io).unwrap();
        assert_eq!(blake3::hash(&bytes).as_bytes(), &id);
    }
    assert!(!fixture.store().join("head").exists());
    assert!(!fixture.store().join("accepted").exists());
}

#[test]
fn original_artifact_above_sixty_four_mib_streams_through_real_pages_under_shared_admission() {
    let fixture = Fixture::new();
    let memory = MemoryBudget::new(8 << 20, 3 << 20).unwrap();
    let pages =
        IncrementalPageStore::create(&fixture.store(), memory.clone(), PageCompression::Raw)
            .unwrap();
    let cancelled = AtomicBool::new(false);
    let mut physical = physical_io();
    let mut logical = logical_io();
    let objects = IncrementalObjectStore::create(&pages, &mut physical, &cancelled).unwrap();
    let length = (64 * 1024 * 1024 + 17) as u64;
    let charge = memory.reserve(65536, MemoryClass::Ordinary).unwrap();
    let block = charge.retain(vec![59; 65536]);
    let mut hash = blake3::Hasher::new();
    for _ in 0..1024 {
        hash.update(&block);
    }
    hash.update(&block[..17]);
    let expected = *hash.finalize().as_bytes();
    let mut io = ObjectIo {
        physical: &mut physical,
        logical: &mut logical,
        class: MemoryClass::Ordinary,
        cancelled: &cancelled,
    };
    assert_eq!(
        objects
            .write_object(&mut std::io::repeat(59), length, Some(expected), &mut io)
            .unwrap(),
        expected
    );
    let baseline = memory.used();
    assert!(matches!(
        objects.read_object_owned(&expected, length as usize, &mut io),
        Err(PageStoreError::Memory(_))
    ));
    assert_eq!(memory.used(), baseline);
    assert_eq!(
        objects
            .read_object_into(&expected, length, &mut std::io::sink(), &mut io)
            .unwrap(),
        length
    );
    assert_eq!(memory.used(), baseline);
    // The repeating fixture exercises original size beyond the old limit;
    // it is not a capacity, RSS, random-data or fault-domain benchmark.
}

#[test]
#[ignore = "exact worker for original object publication kill test"]
fn object_publication_crash_worker() {
    let path = PathBuf::from(std::env::var_os("OASIS7_IWS_PAGE_CRASH_ROOT").expect("worker root"));
    let memory = MemoryBudget::new(8 << 20, 0).unwrap();
    let pages =
        IncrementalPageStore::open(&path, memory, PageCompression::Raw, MemoryClass::Ordinary)
            .unwrap();
    let cancelled = AtomicBool::new(false);
    let mut physical = physical_io();
    let mut logical = logical_io();
    let objects =
        IncrementalObjectStore::open(&pages, MemoryClass::Ordinary, &mut physical, &cancelled)
            .unwrap();
    objects
        .write_object(
            &mut &b"original-history"[..],
            16,
            None,
            &mut ObjectIo {
                physical: &mut physical,
                logical: &mut logical,
                class: MemoryClass::Ordinary,
                cancelled: &cancelled,
            },
        )
        .unwrap();
    panic!("object publisher did not reach barrier");
}

#[test]
fn killed_original_object_publishers_keep_the_same_identity_on_reopen_and_retry() {
    use std::{process::Command, time::Duration};
    for step in 1..=3 {
        let fixture = Fixture::new();
        let memory = MemoryBudget::new(8 << 20, 0).unwrap();
        let cancelled = AtomicBool::new(false);
        let mut physical = physical_io();
        let mut logical = logical_io();
        {
            let pages = IncrementalPageStore::create(
                &fixture.store(),
                memory.clone(),
                PageCompression::Raw,
            )
            .unwrap();
            IncrementalObjectStore::create(&pages, &mut physical, &cancelled).unwrap();
        }
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "--exact",
                "incremental_page_store::objects::tests::object_publication_crash_worker",
                "--ignored",
                "--nocapture",
                "--test-threads=1",
            ])
            .env("OASIS7_IWS_PAGE_CRASH_ROOT", fixture.store())
            .env("OASIS7_IWS_PAGE_CRASH_POINT", format!("objects-{step}"));
        assert!(
            !crate::test_process::kill_at_marker(
                &mut command,
                &fixture.0.join("paused"),
                Duration::from_secs(15)
            )
            .unwrap()
            .success()
        );
        let pages = IncrementalPageStore::open(
            &fixture.store(),
            memory,
            PageCompression::Raw,
            MemoryClass::Recovery,
        )
        .unwrap();
        let objects =
            IncrementalObjectStore::open(&pages, MemoryClass::Recovery, &mut physical, &cancelled)
                .unwrap();
        let mut io = ObjectIo {
            physical: &mut physical,
            logical: &mut logical,
            class: MemoryClass::Ordinary,
            cancelled: &cancelled,
        };
        let id = *blake3::hash(b"original-history").as_bytes();
        let read = objects.read_object_owned(&id, 16, &mut io);
        if step == 1 {
            assert!(matches!(read, Err(PageStoreError::Missing(_))));
        } else {
            assert_eq!(read.unwrap().as_slice(), b"original-history");
        }
        assert_eq!(
            objects
                .write_object(&mut &b"original-history"[..], 16, Some(id), &mut io)
                .unwrap(),
            id
        );
        assert_eq!(
            fs::read_dir(fixture.store().join("objects"))
                .unwrap()
                .count(),
            1
        );
        assert_eq!(
            objects
                .read_object_owned(&id, 16, &mut io)
                .unwrap()
                .as_slice(),
            b"original-history"
        );
    }
}

#[path = "incremental_material_store_tests.rs"]
mod material;

#[test]
fn empty_original_cannot_skip_missing_typed_data_leaf() {
    let fixture = Fixture::new();
    let memory = MemoryBudget::new(8 << 20, 0).unwrap();
    let pages =
        IncrementalPageStore::create(&fixture.store(), memory.clone(), PageCompression::Raw)
            .unwrap();
    let cancelled = AtomicBool::new(false);
    let mut physical = physical_io();
    let mut logical = logical_io();
    let objects = IncrementalObjectStore::create(&pages, &mut physical, &cancelled).unwrap();
    let mut io = ObjectIo {
        physical: &mut physical,
        logical: &mut logical,
        class: MemoryClass::Ordinary,
        cancelled: &cancelled,
    };
    let id = objects
        .write_object(&mut &b""[..], 0, None, &mut io)
        .unwrap();
    let locator = objects.locator(&id, 0, &mut io).unwrap();
    let source = pages.value_session(physical_io());
    let root = oasis7_formal_protocol::incremental_value::ValueReadContext {
        source: &source,
        memory: &memory,
        class: MemoryClass::Ordinary,
        io: &mut logical_io(),
    }
    .load_root(&locator.value_root)
    .unwrap();
    fs::remove_file(fixture.store().join("recipes").join(hex::encode(root.0.id))).unwrap();
    assert!(
        objects.read_object_owned(&id, 0, &mut io).is_err(),
        "empty flat hash cannot replace its required typed leaf"
    );
}

#[test]
fn empty_locator_with_nonempty_root_remains_corruption_not_budget_exhaustion() {
    let fixture = Fixture::new();
    let memory = MemoryBudget::new(8 << 20, 0).unwrap();
    let pages =
        IncrementalPageStore::create(&fixture.store(), memory, PageCompression::Raw).unwrap();
    let cancelled = AtomicBool::new(false);
    let mut physical = physical_io();
    let mut logical = logical_io();
    let objects = IncrementalObjectStore::create(&pages, &mut physical, &cancelled).unwrap();
    let mut io = ObjectIo {
        physical: &mut physical,
        logical: &mut logical,
        class: MemoryClass::Ordinary,
        cancelled: &cancelled,
    };
    let empty = objects
        .write_object(&mut &b""[..], 0, None, &mut io)
        .unwrap();
    let other = objects
        .write_object(&mut &b"bytes"[..], 5, None, &mut io)
        .unwrap();
    let mut locator = objects.locator(&other, 5, &mut io).unwrap();
    locator.length = 0;
    fs::write(
        fixture.store().join("objects").join(hex::encode(empty)),
        encode_locator(&empty, &locator),
    )
    .unwrap();
    assert!(matches!(
        objects.read_object_owned(&empty, 0, &mut io),
        Err(PageStoreError::Corrupt("object root length"))
    ));
}
