use super::super::invalid;
use super::*;
use crate::incremental_page_store::PageCompression;
use oasis7_formal_protocol::incremental_value::ValueNodeReader;
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Directory(std::path::PathBuf);
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn io() -> PageIoBudget {
    PageIoBudget {
        reads_remaining: 1000,
        read_bytes_remaining: 1 << 20,
        writes_remaining: 1000,
        write_bytes_remaining: 1 << 20,
    }
}
fn counts(io: &PageIoBudget) -> [u64; 4] {
    [
        io.reads_remaining,
        io.read_bytes_remaining,
        io.writes_remaining,
        io.write_bytes_remaining,
    ]
}
fn store() -> (Directory, IncrementalPageStore, MemoryBudget, [u8; 32]) {
    let directory = Directory(std::env::temp_dir().join(format!(
        "oasis7-continuing-source-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed),
    )));
    let memory = MemoryBudget::new(32 << 20, 0).unwrap();
    let pages =
        IncrementalPageStore::create(&directory.0, memory.clone(), PageCompression::Raw).unwrap();
    let bytes = memory
        .reserve(256, MemoryClass::Ordinary)
        .unwrap()
        .retain(b"actual source account original".to_vec());
    let id = *blake3::hash(&bytes).as_bytes();
    pages
        .put_page(id, &bytes, &mut io(), &AtomicBool::new(false))
        .unwrap();
    (directory, pages, memory, id)
}

#[test]
fn original_and_native_loans_keep_debits_on_failure_and_continuation() {
    let (mut occurrences, mut bytes) = (10, 100);
    let result: Result<(), FormalError> = {
        let mut original = OriginalsLoan::new(&mut occurrences, &mut bytes);
        // Both readers borrow the same original work, as Runtime and Cognition do.
        original.work.occurrences_remaining -= 2;
        original.work.bytes_remaining -= 30;
        original.work.occurrences_remaining -= 1;
        original.work.bytes_remaining -= 20;
        Err(invalid("original typed failure"))
    };
    assert!(result.is_err());
    assert_eq!((occurrences, bytes), (7, 50));
    {
        let mut continued = OriginalsLoan::new(&mut occurrences, &mut bytes);
        continued.work.occurrences_remaining -= 1;
        continued.work.bytes_remaining -= 5;
    }
    assert_eq!((occurrences, bytes), (6, 45));
    let mut frozen = 80;
    {
        let mut native = NativeLoan::new(&mut occurrences, &mut bytes, &mut frozen);
        native.work.occurrences_remaining -= 1;
        native.work.bytes_remaining -= 7;
        native.work.frozen_wire_bytes_remaining -= 11;
    }
    assert_eq!((occurrences, bytes, frozen), (5, 38, 69));
}

#[test]
fn unwind_burns_moved_scalar_lanes_and_preserves_original_payload() {
    let (mut original_n, mut original_bytes) = (10, 100);
    let (mut native_n, mut native_bytes, mut frozen_bytes) = (20, 200, 300);
    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _original = OriginalsLoan::new(&mut original_n, &mut original_bytes);
        let _native = NativeLoan::new(&mut native_n, &mut native_bytes, &mut frozen_bytes);
        std::panic::panic_any(73u64);
    }))
    .unwrap_err();
    assert_eq!(*panic.downcast::<u64>().unwrap(), 73);
    assert_eq!(
        (
            original_n,
            original_bytes,
            native_n,
            native_bytes,
            frozen_bytes
        ),
        (0, 0, 0, 0, 0)
    );
}

#[test]
fn physical_writer_returns_actual_remainder_after_read_and_cancellation() {
    let (_directory, pages, memory, id) = store();
    let settlement = Settlement::default();
    let mut physical = io();
    let initial = counts(&physical);
    let observed;
    let outcome = {
        let writer = WriterLoan::new(&pages, &mut physical, &settlement);
        ValueNodeReader::read_node(&*writer, &id, 1024, &memory, MemoryClass::Ordinary).unwrap();
        observed = writer.remaining_io().unwrap();
        writer.cancelled.store(true, Ordering::Release);
        assert!(
            ValueNodeReader::read_node(&*writer, &id, 1024, &memory, MemoryClass::Ordinary)
                .is_err()
        );
        Err::<(), _>(invalid("original cancellation"))
    };
    assert!(observed.reads_remaining < initial[0]);
    assert_eq!(counts(&physical), counts(&observed));
    assert_eq!(
        settlement.finish(outcome).unwrap_err().to_string(),
        invalid("original cancellation").to_string()
    );
}

#[test]
fn publication_returns_both_real_accounts_and_next_loan_uses_the_remainder() {
    let (_directory, pages, memory, id) = store();
    let stop = AtomicBool::new(false);
    let mut physical = io();
    let mut steps = 10000;
    let mut previous_physical = counts(&physical);
    let mut previous_steps = steps;
    for _ in 0..2 {
        let settlement = Settlement::default();
        let observed;
        {
            let sessions = PublicationLoan::new(
                &pages,
                io(),
                io(),
                &mut physical,
                8,
                &mut steps,
                &stop,
                &settlement,
            )
            .unwrap();
            ValueNodeReader::read_node(&sessions.values, &id, 1024, &memory, MemoryClass::Ordinary)
                .unwrap();
            observed = (
                sessions.remaining_publish_io().unwrap(),
                sessions.remaining_memo_steps().unwrap(),
            );
        }
        settlement.finish(Ok(())).unwrap();
        assert_eq!(counts(&physical), counts(&observed.0));
        assert_eq!(steps, observed.1);
        assert!(physical.reads_remaining < previous_physical[0]);
        assert!(steps < previous_steps);
        previous_physical = counts(&physical);
        previous_steps = steps;
    }
}

#[test]
fn physical_panic_burns_only_transferred_lanes_without_replacing_payload() {
    let (_directory, pages, memory, id) = store();
    let stop = AtomicBool::new(false);
    let settlement = Settlement::default();
    let mut writer_io = io();
    let mut publication_io = io();
    let mut steps = 10000;
    let untouched = io();
    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let writer = WriterLoan::new(&pages, &mut writer_io, &settlement);
        let _sessions = PublicationLoan::new(
            &pages,
            io(),
            io(),
            &mut publication_io,
            8,
            &mut steps,
            &stop,
            &settlement,
        )
        .unwrap();
        ValueNodeReader::read_node(&*writer, &id, 1024, &memory, MemoryClass::Ordinary).unwrap();
        std::panic::panic_any(89u64);
    }))
    .unwrap_err();
    assert_eq!(*panic.downcast::<u64>().unwrap(), 89);
    assert_eq!(counts(&writer_io), [0; 4]);
    assert_eq!(counts(&publication_io), [0; 4]);
    assert_eq!(steps, 0);
    assert_eq!(counts(&untouched), counts(&io()));
}

#[test]
fn unobservable_constructor_work_is_burned_and_first_error_wins() {
    let (_directory, pages, _memory, _id) = store();
    let stop = AtomicBool::new(false);
    let settlement = Settlement::default();
    let mut physical = io();
    let mut steps = 10000;
    assert!(
        PublicationLoan::new(
            &pages,
            io(),
            io(),
            &mut physical,
            0,
            &mut steps,
            &stop,
            &settlement
        )
        .is_err()
    );
    assert_eq!(counts(&physical), [0; 4]);
    assert_eq!(steps, 0);
    settlement.failed(invalid("unobservable remaining physical work"));
    assert_eq!(
        settlement
            .finish(Err::<(), _>(invalid("original producer error")))
            .unwrap_err()
            .to_string(),
        invalid("original producer error").to_string()
    );
    let settlement = Settlement::default();
    settlement.failed(invalid("unobservable remaining physical work"));
    assert!(
        settlement.finish(Ok(())).is_err(),
        "successful production cannot hide failed settlement"
    );
}

#[test]
fn poisoned_publisher_burns_only_unobservable_io_and_keeps_original_storage_error() {
    let (_directory, pages, memory, first) = store();
    let stop = AtomicBool::new(false);
    let second_bytes = memory
        .reserve(256, MemoryClass::Ordinary)
        .unwrap()
        .retain(b"second actual source original".to_vec());
    let second = *blake3::hash(&second_bytes).as_bytes();
    pages
        .put_page(second, &second_bytes, &mut io(), &stop)
        .unwrap();
    let settlement = Settlement::default();
    let mut physical = io();
    let initial_reads = physical.reads_remaining;
    let mut steps = 10000;
    let observed_steps;
    let outcome = {
        let sessions = PublicationLoan::new(
            &pages,
            io(),
            io(),
            &mut physical,
            8,
            &mut steps,
            &stop,
            &settlement,
        )
        .unwrap();
        ValueNodeReader::read_node(
            &sessions.values,
            &first,
            1024,
            &memory,
            MemoryClass::Ordinary,
        )
        .unwrap();
        assert!(sessions.remaining_publish_io().unwrap().reads_remaining < initial_reads);
        let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            sessions.panic_while_holding_publish_io_for_test();
        }))
        .unwrap_err();
        assert_eq!(*panic.downcast::<u64>().unwrap(), 211);
        assert!(
            sessions.remaining_publish_io().is_err(),
            "the real publisher mutex is poisoned"
        );
        // An uncached page reaches the original publisher, which preserves its
        // actual typed storage error independently of the later settlement.
        assert!(
            ValueNodeReader::read_node(
                &sessions.values,
                &second,
                1024,
                &memory,
                MemoryClass::Ordinary
            )
            .is_err()
        );
        let error = sessions
            .state
            .take_read_error()
            .expect("original publisher error");
        observed_steps = sessions.remaining_memo_steps().unwrap();
        assert!(observed_steps < 10000);
        Err::<(), _>(page_store_formal_error(error))
    };
    assert_eq!(counts(&physical), [0; 4]);
    assert_eq!(
        steps, observed_steps,
        "the observable memo lane returns its true remainder"
    );
    assert_eq!(
        settlement.finish(outcome).unwrap_err(),
        page_store_formal_error(crate::incremental_page_store::PageStoreError::Corrupt(
            "closure publish lock"
        )),
        "the original typed IO error wins over the settlement lock error",
    );
}
