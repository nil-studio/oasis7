use super::*;
use oasis7_formal_protocol::controlled_authority::replicated_protocol::ArtifactObject;
use oasis7_formal_protocol::incremental_value::ValueIoBudget;
use std::io::{self, Write};

struct PartialSink {
    bytes: Vec<u8>,
    stop: Option<usize>,
}
impl Write for PartialSink {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.stop.is_some_and(|limit| self.bytes.len() >= limit) {
            return Err(io::Error::other("injected downstream cancellation"));
        }
        let count = bytes.len().min(7);
        self.bytes.extend_from_slice(&bytes[..count]);
        Ok(count)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
#[test]
fn iws_closed_original_stream_honors_partial_writes_errors_bounds_and_pool() {
    let bytes = (0..100_003).map(|n| (n % 251) as u8).collect::<Vec<_>>();
    let id = *blake3::hash(&bytes).as_bytes();
    // Only the adapter is under test. This record is deliberately not protocol
    // material and cannot obtain a receipt or any typed source qualification.
    let mut record = ClosedRecord {
        payload_digest: String::new(),
        before_state_root: String::new(),
        after_state_root: String::new(),
        roots: Default::default(),
        objects: vec![ArtifactObject {
            content_hash: hex::encode(id),
            bytes: bytes.clone(),
            references: vec![],
        }],
    };
    let memory = MemoryBudget::new(1 << 20, 1 << 19).unwrap();
    let baseline = memory.used();
    let mut sink = PartialSink {
        bytes: vec![],
        stop: None,
    };
    let mut reader = ClosedRecordOriginalReader {
        record: &record,
        memory: &memory,
    };
    assert_eq!(
        reader
            .stream_original(&id, bytes.len() as u64, &memory, &mut sink)
            .unwrap(),
        bytes.len() as u64
    );
    assert_eq!(sink.bytes, bytes);
    assert_eq!(memory.used(), baseline);
    let mut stopped = PartialSink {
        bytes: vec![],
        stop: Some(31),
    };
    assert!(
        reader
            .stream_original(&id, bytes.len() as u64, &memory, &mut stopped)
            .is_err()
    );
    assert!(stopped.bytes.len() < bytes.len());
    let mut untouched = PartialSink {
        bytes: vec![],
        stop: None,
    };
    assert!(
        reader
            .stream_original(&id, 1, &memory, &mut untouched)
            .is_err()
    );
    let other = MemoryBudget::new(1 << 20, 1 << 19).unwrap();
    assert!(
        reader
            .stream_original(&id, bytes.len() as u64, &other, &mut untouched)
            .is_err()
    );
    assert!(untouched.bytes.is_empty());
    assert_eq!(memory.used(), baseline);
    record.objects[0].bytes[0] ^= 1;
    let mut reader = ClosedRecordOriginalReader {
        record: &record,
        memory: &memory,
    };
    assert!(
        reader
            .stream_original(&id, bytes.len() as u64, &memory, &mut untouched)
            .is_err()
    );
    assert_eq!(memory.used(), baseline);
}

#[test]
fn iws_endpoint_original_reader_joins_candidate_and_reopened_retained_objects() {
    use crate::incremental_page_store::{
        PageCompression,
        objects::{IncrementalObjectStore, ObjectIo},
    };
    use std::sync::atomic::AtomicBool;
    let path = std::env::temp_dir().join(format!(
        "oasis7-endpoint-original-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let retained_bytes = vec![71u8; 200_003];
    let retained_id = *blake3::hash(&retained_bytes).as_bytes();
    let new_bytes = b"actual candidate original".to_vec();
    let new_id = *blake3::hash(&new_bytes).as_bytes();
    let quota = || PageIoBudget {
        reads_remaining: 10000,
        read_bytes_remaining: 16 << 20,
        writes_remaining: 10000,
        write_bytes_remaining: 16 << 20,
    };
    let logical = || ValueIoBudget {
        nodes_remaining: 10000,
        read_bytes_remaining: 16 << 20,
        write_bytes_remaining: 16 << 20,
    };
    let cancelled = AtomicBool::new(false);
    {
        // Zstd encoding admits its fixed 16MiB workspace before page buffers.
        let memory = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
        let pages =
            IncrementalPageStore::create(&path, memory.clone(), PageCompression::Zstd).unwrap();
        let mut physical = quota();
        let objects = IncrementalObjectStore::create(&pages, &mut physical, &cancelled).unwrap();
        let mut values = logical();
        let mut io = ObjectIo {
            physical: &mut physical,
            logical: &mut values,
            class: MemoryClass::Ordinary,
            cancelled: &cancelled,
        };
        assert_eq!(
            objects
                .write_object(
                    &mut retained_bytes.as_slice(),
                    retained_bytes.len() as u64,
                    Some(retained_id),
                    &mut io
                )
                .unwrap(),
            retained_id
        );
    }
    {
        // All prior stores, readers, proofs and pool have gone away.
        let memory = MemoryBudget::new(16 << 20, 4 << 20).unwrap();
        let pages = IncrementalPageStore::open(
            &path,
            memory.clone(),
            PageCompression::Zstd,
            MemoryClass::Ordinary,
        )
        .unwrap();
        let mut physical = quota();
        let objects =
            IncrementalObjectStore::open(&pages, MemoryClass::Ordinary, &mut physical, &cancelled)
                .unwrap();
        let mut values = logical();
        let mut io = ObjectIo {
            physical: &mut physical,
            logical: &mut values,
            class: MemoryClass::Ordinary,
            cancelled: &cancelled,
        };
        let mut retained = objects.original_reader(&mut io);
        let mut record = ClosedRecord {
            payload_digest: String::new(),
            before_state_root: String::new(),
            after_state_root: String::new(),
            roots: Default::default(),
            objects: vec![ArtifactObject {
                content_hash: hex::encode(new_id),
                bytes: new_bytes.clone(),
                references: vec![],
            }],
        };
        {
            let mut reader = EndpointOriginalReader {
                candidate: ClosedRecordOriginalReader {
                    record: &record,
                    memory: &memory,
                },
                retained: &mut retained,
            };
            for (id, expected) in [
                (retained_id, retained_bytes.as_slice()),
                (new_id, new_bytes.as_slice()),
            ] {
                let mut output = PartialSink {
                    bytes: vec![],
                    stop: None,
                };
                assert_eq!(
                    reader
                        .stream_original(&id, expected.len() as u64, &memory, &mut output)
                        .unwrap(),
                    expected.len() as u64
                );
                assert_eq!(output.bytes, expected);
                let owned = reader.read_original(&id, expected.len(), &memory).unwrap();
                assert_eq!(owned.as_slice(), expected);
            }
        }
        // A corrupt candidate occurrence shadows storage and must reject; it
        // cannot silently fall back to a valid retained copy of the same ID.
        record.objects[0].content_hash = hex::encode(retained_id);
        let mut reader = EndpointOriginalReader {
            candidate: ClosedRecordOriginalReader {
                record: &record,
                memory: &memory,
            },
            retained: &mut retained,
        };
        assert!(
            reader
                .read_original(&retained_id, retained_bytes.len(), &memory)
                .is_err()
        );
        let mut output = PartialSink {
            bytes: vec![],
            stop: None,
        };
        assert!(
            reader
                .stream_original(
                    &retained_id,
                    retained_bytes.len() as u64,
                    &memory,
                    &mut output
                )
                .is_err()
        );
    }
    std::fs::remove_dir_all(path).unwrap();
}
