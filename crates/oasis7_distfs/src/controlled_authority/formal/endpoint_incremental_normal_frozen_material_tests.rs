//! Exercise the actual normal Frozen content verifier and its paid reader.
//! Signed content is not durable acceptance, a Node decision, or a live Host.
use super::*;
use crate::controlled_authority::formal::endpoint::incremental::closure::with_frozen_material_fixture;
use crate::controlled_authority::replicated_protocol::ArtifactRole;
use std::sync::Arc;

fn archive_work() -> FormalArchiveReadWorkV1 {
    FormalArchiveReadWorkV1::new(1_000_000_000, 16 << 30).unwrap()
}

fn material_work() -> IncrementalFrozenMaterialWorkV1 {
    IncrementalFrozenMaterialWorkV1 {
        metadata_entries_remaining: 1_000_000,
        comparison_bytes_remaining: 64 << 20,
        hash_bytes_remaining: 32 << 20,
        frozen_wire_bytes_remaining: 64 << 20,
        original_calls_remaining: 16,
        original_bytes_remaining: 16 << 20,
        max_original_bytes: 8 << 20,
    }
}

#[test]
fn normal_frozen_verifier_reads_metadata_and_streams_originals() {
    with_frozen_material_fixture(|input, material, authority, parent, memory| {
        let archive = archive_work();
        let paid =
            state::state(input, material, None, None, memory, &archive, &mut || false).unwrap();
        let reservation = verify_formal_reservation_bounded(
            input,
            authority,
            parent,
            &mut archive.bounded_context(memory, &mut || false),
        )
        .unwrap();
        let before = memory.used();
        let mut work = material_work();
        let checked = check_material(
            &paid,
            &reservation,
            parent,
            memory,
            &archive,
            &mut work,
            &mut || false,
        )
        .unwrap();
        assert!(checked.uses_budget(memory));
        assert_eq!(checked.parent_record_id(), parent.verified_record_id());
        assert_eq!(
            hex::encode(checked.reservation_id()),
            input.body.reservation_id().unwrap()
        );
        assert_eq!(
            hex::encode(checked.material_id()),
            input.body.frozen_material_digest
        );
        assert_eq!(
            hex::encode(checked.manifest_id()),
            input.body.runtime_manifest_digest
        );
        assert_eq!(checked.input().id, *blake3::hash(b"next-input").as_bytes());
        assert_eq!(checked.input().byte_len, b"next-input".len() as u64);
        assert_eq!(
            checked.continuation().id,
            *blake3::hash(b"next-continuation").as_bytes()
        );
        assert_eq!(
            checked.continuation().byte_len,
            b"next-continuation".len() as u64
        );
        // Config/manifest/native metadata plus input/policy/continuation streams.
        assert_eq!(work.original_calls_remaining, 16 - 3 - 3);
        assert_eq!(memory.used(), before + checked.reserved_bytes());
        drop(checked);
        assert_eq!(memory.used(), before);
    });
}

fn with_paid_original(
    run: impl FnOnce(&PaidRequestStateV1, &MemoryBudget, &FormalArchiveReadWorkV1, [u8; 32], &[u8]),
) {
    with_frozen_material_fixture(|input, material, _, _, memory| {
        let archive = archive_work();
        let paid =
            state::state(input, material, None, None, memory, &archive, &mut || false).unwrap();
        let text = &material.roots[&ArtifactRole::IncrementalGenesisConfig];
        let id = hex::decode(text).unwrap().try_into().unwrap();
        let original = material
            .objects
            .iter()
            .find(|object| &object.content_hash == text)
            .unwrap();
        run(&paid, memory, &archive, id, &original.bytes);
    });
}

#[test]
fn normal_frozen_metadata_reader_retains_fee_until_last_arc_drops() {
    with_paid_original(|paid, memory, work, id, expected| {
        let mut keep_running = || false;
        let stop = RefCell::new(&mut keep_running as &mut dyn FnMut() -> bool);
        let mut source = FrozenOriginals {
            paid,
            memory,
            work,
            stop: &stop,
        };
        let before = memory.used();
        let bytes = source.read_original(&id, expected.len(), memory).unwrap();
        assert_eq!(bytes.as_slice(), expected);
        assert!(bytes.uses_budget(memory));
        assert!(
            bytes.reserved_bytes()
                >= bytes.capacity() + std::mem::size_of::<ReservedValue<Vec<u8>>>()
        );
        let held = memory.used();
        assert_eq!(held, before + bytes.reserved_bytes());
        let other = Arc::clone(&bytes);
        drop(bytes);
        assert_eq!(memory.used(), held);
        drop(other);
        assert_eq!(memory.used(), before);
    });
}

#[test]
fn normal_frozen_metadata_reader_rejects_bound_and_foreign_pool_without_output() {
    with_paid_original(|paid, memory, work, id, expected| {
        let mut keep_running = || false;
        let stop = RefCell::new(&mut keep_running as &mut dyn FnMut() -> bool);
        let mut source = FrozenOriginals {
            paid,
            memory,
            work,
            stop: &stop,
        };
        let before = memory.used();
        let error = source
            .read_original(&id, expected.len() - 1, memory)
            .unwrap_err();
        assert!(error.to_string().contains("normal Frozen original bound"));
        assert_eq!(memory.used(), before);
        let foreign = MemoryBudget::new(1 << 20, 0).unwrap();
        let error = source
            .read_original(&id, expected.len(), &foreign)
            .unwrap_err();
        assert!(error.to_string().contains("normal Frozen original pool"));
        assert_eq!(foreign.used(), 0);
        assert_eq!(memory.used(), before);
        let bytes = source.read_original(&id, expected.len(), memory).unwrap();
        assert_eq!(bytes.as_slice(), expected);
    });
}

#[test]
fn normal_frozen_metadata_reader_cancellation_releases_partial_allocation() {
    with_paid_original(|paid, memory, work, id, expected| {
        let before = memory.used();
        // Cancel only after this reader's admission; original fees stay alive.
        let mut cancel_after_admission = || memory.used() > before;
        let stop = RefCell::new(&mut cancel_after_admission as &mut dyn FnMut() -> bool);
        let mut source = FrozenOriginals {
            paid,
            memory,
            work,
            stop: &stop,
        };
        let error = source
            .read_original(&id, expected.len(), memory)
            .unwrap_err();
        assert!(error.to_string().contains("bounded archive read cancelled"));
        assert_eq!(memory.used(), before);
        assert!(paid.uses_budget(memory));
    });
}

#[test]
fn normal_frozen_metadata_reader_checks_cancel_after_copy_charge() {
    with_paid_original(|paid, memory, work, id, expected| {
        let FormalRequestStateV1::Frozen { material, .. } = paid.state() else {
            panic!("actual Frozen original")
        };
        let text = hex::encode(id);
        let mut lookup_bytes = 0;
        for object in &material.objects {
            lookup_bytes += object.content_hash.len() as u64;
            if object.content_hash == text {
                break;
            }
        }
        assert!(!expected.is_empty());
        let after_copy = work.remaining_read_bytes() - lookup_bytes - expected.len() as u64;
        let before = memory.used();
        // The lookup and every copy chunk are allowed. Cancellation becomes
        // visible only after the original meter has charged the entire copy.
        let mut cancel_after_copy = || work.remaining_read_bytes() == after_copy;
        let stop = RefCell::new(&mut cancel_after_copy as &mut dyn FnMut() -> bool);
        let mut source = FrozenOriginals {
            paid,
            memory,
            work,
            stop: &stop,
        };
        let error = source
            .read_original(&id, expected.len(), memory)
            .unwrap_err();
        assert!(error.to_string().contains("bounded archive read cancelled"));
        assert_eq!(work.remaining_read_bytes(), after_copy);
        assert_eq!(memory.used(), before);
    });
}

#[test]
fn normal_frozen_metadata_reader_does_not_refill_exhausted_steps() {
    with_paid_original(|paid, memory, work, id, expected| {
        work.consume_steps(work.remaining_steps(), &mut || false)
            .unwrap();
        let before = memory.used();
        let mut keep_running = || false;
        let stop = RefCell::new(&mut keep_running as &mut dyn FnMut() -> bool);
        let mut source = FrozenOriginals {
            paid,
            memory,
            work,
            stop: &stop,
        };
        for _ in 0..2 {
            let error = source
                .read_original(&id, expected.len(), memory)
                .unwrap_err();
            assert!(error.to_string().contains("bounded archive read steps"));
            assert_eq!(work.remaining_steps(), 0);
            assert_eq!(memory.used(), before);
        }
    });
}

#[test]
fn normal_frozen_metadata_reader_does_not_refill_exhausted_bytes() {
    with_paid_original(|paid, memory, work, id, expected| {
        work.consume_read_bytes(work.remaining_read_bytes(), &mut || false)
            .unwrap();
        let before = memory.used();
        let mut keep_running = || false;
        let stop = RefCell::new(&mut keep_running as &mut dyn FnMut() -> bool);
        let mut source = FrozenOriginals {
            paid,
            memory,
            work,
            stop: &stop,
        };
        for _ in 0..2 {
            let error = source
                .read_original(&id, expected.len(), memory)
                .unwrap_err();
            assert!(error.to_string().contains("bounded archive read bytes"));
            assert_eq!(work.remaining_read_bytes(), 0);
            assert_eq!(memory.used(), before);
        }
    });
}

#[test]
fn normal_frozen_metadata_reader_honors_ordinary_memory_limit() {
    with_paid_original(|paid, memory, work, id, expected| {
        let before = memory.used();
        let limit = memory
            .reserve(usize::MAX, MemoryClass::Ordinary)
            .unwrap_err()
            .admission_limit;
        let full = memory
            .reserve(limit - before, MemoryClass::Ordinary)
            .unwrap();
        let steps = work.remaining_steps();
        let mut keep_running = || false;
        let stop = RefCell::new(&mut keep_running as &mut dyn FnMut() -> bool);
        let mut source = FrozenOriginals {
            paid,
            memory,
            work,
            stop: &stop,
        };
        let error = source
            .read_original(&id, expected.len(), memory)
            .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("normal Frozen original admission")
        );
        assert_eq!(memory.used(), limit);
        assert!(work.remaining_steps() < steps);
        drop(full);
        assert_eq!(memory.used(), before);
        let bytes = source.read_original(&id, expected.len(), memory).unwrap();
        assert_eq!(bytes.as_slice(), expected);
        drop(bytes);
        assert_eq!(memory.used(), before);
    });
}
