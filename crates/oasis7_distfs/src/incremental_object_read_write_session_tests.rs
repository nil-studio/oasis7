use super::*;
use crate::incremental_page_store::tests::{Fixture, logical_io, physical_io};
use oasis7_formal_protocol::incremental_material::{OriginalObjectStreamReader, OriginalObjectStreamWriter};

#[test]
fn real_provisional_original_writer_preserves_original_bytes_pool_and_cold_readback() {
    let fixture = Fixture::new();
    let memory = MemoryBudget::new(24 << 20, 4 << 20).unwrap();
    let pages =
        IncrementalPageStore::create(&fixture.store(), memory.clone(), PageCompression::Raw)
            .unwrap();
    let cancelled = AtomicBool::new(false);
    let mut physical = physical_io();
    let mut logical = logical_io();
    let store = IncrementalObjectStore::create(&pages, &mut physical, &cancelled).unwrap();
    let original = b"real provisional original through the same physical object store";
    let id = *blake3::hash(original).as_bytes();
    let mut io = ObjectIo {
        physical: &mut physical,
        logical: &mut logical,
        class: MemoryClass::Ordinary,
        cancelled: &cancelled,
    };
    {
        let mut readonly = store.original_reader(&mut io);
        assert!(OriginalObjectStreamReader::original_writer(&mut readonly).is_none());
    }
    let foreign = MemoryBudget::new(8 << 20, 0).unwrap();
    {
        let mut session = store.provisional_read_write_session(&mut io).unwrap();
        assert!(std::mem::size_of::<ObjectReadWriteSession<'_, '_, '_, '_>>() < 4096);
        {
            let writer = OriginalObjectStreamReader::original_writer(&mut session).unwrap();
            assert!(
                writer
                    .write_original(&mut original.as_slice(), original.len() as u64, id, &foreign)
                    .is_err()
            );
            assert_eq!(
                writer
                    .write_original(&mut original.as_slice(), original.len() as u64, id, &memory)
                    .unwrap(),
                id
            );
        }
        let mut received = Vec::new();
        assert_eq!(
            session
                .stream_original(&id, original.len() as u64, &memory, &mut received)
                .unwrap(),
            original.len() as u64
        );
        assert_eq!(received.as_slice(), original.as_slice());
        cancelled.store(true, Ordering::Release);
        assert!(
            session
                .stream_original(&id, original.len() as u64, &memory, &mut Vec::new())
                .is_err()
        );
        cancelled.store(false, Ordering::Release);
    }
    drop(io);
    drop(store);
    drop(pages);

    let reopened = IncrementalPageStore::open(
        &fixture.store(),
        memory.clone(),
        PageCompression::Raw,
        MemoryClass::Recovery,
    )
    .unwrap();
    let store =
        IncrementalObjectStore::open(&reopened, MemoryClass::Recovery, &mut physical, &cancelled)
            .unwrap();
    let mut recovery = ObjectIo {
        physical: &mut physical,
        logical: &mut logical,
        class: MemoryClass::Recovery,
        cancelled: &cancelled,
    };
    let read = store
        .read_object_owned(&id, original.len(), &mut recovery)
        .unwrap();
    assert_eq!(read.as_slice(), original);
    assert!(store.provisional_read_write_session(&mut recovery).is_err());
}
