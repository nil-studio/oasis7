#![cfg(all(unix, not(target_arch = "wasm32")))]

use oasis7_distfs::incremental_page_store::{IncrementalPageStore, PageCompression, PageIoBudget};
use oasis7_formal_protocol::incremental_state::{
    STATE_ROOT_BYTES, StateDeltaV1, StateNamespaceV1, StatePageWriter, StateRootV1, StateWriteV1,
};
use oasis7_formal_protocol::incremental_value::{ValueIoBudget, ValueReadContext};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass};
use oasis7_runtime::{incremental_field_member::*, types::MaterialLedgerId};
use std::{
    io::Write,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);
struct Directory(std::path::PathBuf);
impl Directory {
    fn new() -> Self {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let sequence = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "oasis7-field-member-{}-{nonce}-{sequence}",
            std::process::id()
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}
fn physical() -> PageIoBudget {
    PageIoBudget {
        reads_remaining: 100_000,
        read_bytes_remaining: 256 << 20,
        writes_remaining: 100_000,
        write_bytes_remaining: 256 << 20,
    }
}
fn logical() -> ValueIoBudget {
    ValueIoBudget {
        nodes_remaining: 100_000,
        read_bytes_remaining: 256 << 20,
        write_bytes_remaining: 256 << 20,
    }
}
struct HashOutput {
    hash: blake3::Hasher,
    bytes: u64,
}
impl Write for HashOutput {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.hash.update(bytes);
        self.bytes += bytes.len() as u64;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn original_large_key_and_ledger_payload_reopen_as_separate_authenticated_ranges() {
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        let directory = Directory::new();
        let path = directory.0.join("pages");
        let memory = MemoryBudget::new(96 << 20, 4 << 20).unwrap();
        let repetitions = (65 << 20) / 5 + 1;
        let length = repetitions * 5;
        let source = memory
            .reserve(length + 256, MemoryClass::Ordinary)
            .unwrap()
            .retain(MaterialLedgerId::Factory("ab甲".repeat(repetitions)));
        let MaterialLedgerId::Factory(id) = &*source else {
            unreachable!()
        };
        let (record, state_root_bytes, expected_key, expected_payload) = {
            let _output = memory.reserve(8192, MemoryClass::Ordinary).unwrap();
            // Derive both oracles from frozen wire bytes, not the member writer.
            let mut key_hash = blake3::Hasher::new();
            key_hash.update(b"oasis7.iws.semantic-key.v1\0");
            key_hash.update(&[1]);
            key_hash.update(&(length as u64).to_be_bytes());
            key_hash.update(id.as_bytes());
            let mut payload_hash = blake3::Hasher::new();
            payload_hash.update(b"oasis7.iws.ledger-value.v1\0");
            payload_hash.update(&[3]);
            payload_hash.update(&(length as u64).to_be_bytes());
            payload_hash.update(id.as_bytes());
            let store = IncrementalPageStore::create(&path, memory.clone(), compression).unwrap();
            let mut sink = store.value_session(physical());
            let record = write_member_record_v1(
                FieldDescriptor {
                    owner: FieldOwner::WorldState,
                    destination: FieldDestination::State(
                        oasis7_formal_protocol::incremental_state::StateNamespaceV1::Business,
                    ),
                    name: "fixture.ledger",
                    shape: FieldShape::Map,
                },
                WorldFieldRecordKindV1::Entry,
                id.as_str(),
                MemberPayloadRefV1::Ledger(&source),
                &mut MemberRecordWriteContext {
                    sink: &mut sink,
                    memory: &memory,
                    io: &mut logical(),
                    key_work: &mut SemanticKeyBudget {
                        bytes_remaining: 128 << 20,
                    },
                    limits: MemberRecordLimitsV1 {
                        semantic_bytes: 128 << 20,
                        payload_bytes: 128 << 20,
                        record_bytes: 192 << 20,
                    },
                    cancelled: || false,
                },
            )
            .unwrap();
            // Attach the bounded field key to its member value in the actual
            // state tree; this is provisional content, not an authority head.
            let state_root_bytes: [u8; STATE_ROOT_BYTES] = {
                let state_source = store
                    .state_session(physical(), MemoryClass::Ordinary)
                    .unwrap();
                let charge = memory.reserve(4 << 20, MemoryClass::Ordinary).unwrap();
                let delta = charge.retain(
                    StateDeltaV1::build(
                        StateRootV1::empty(StateNamespaceV1::Business).unwrap(),
                        vec![StateWriteV1 {
                            key: record.field_key.as_bytes().to_vec(),
                            before: None,
                            after: Some(record.value_root),
                        }],
                        &state_source,
                        2 << 20,
                    )
                    .unwrap(),
                );
                sink.put_state_pages(&delta, &memory).unwrap();
                delta.after.canonical_bytes().unwrap().try_into().unwrap()
            };
            (
                record,
                state_root_bytes,
                key_hash.finalize(),
                payload_hash.finalize(),
            )
        };
        drop(source);
        assert_eq!(memory.used(), 0);
        for class in [MemoryClass::Ordinary, MemoryClass::Recovery] {
            let memory = MemoryBudget::new(
                4 << 20,
                if class == MemoryClass::Recovery {
                    3 << 20
                } else {
                    1 << 20
                },
            )
            .unwrap();
            let _output = memory.reserve(8192, class).unwrap();
            let store =
                IncrementalPageStore::open(&path, memory.clone(), compression, class).unwrap();
            let state_source = store.state_session(physical(), class).unwrap();
            let _filler = if class == MemoryClass::Recovery {
                let filler = memory
                    .reserve((1 << 20) - memory.used(), MemoryClass::Ordinary)
                    .unwrap();
                assert!(memory.reserve(1, MemoryClass::Ordinary).is_err());
                Some(filler)
            } else {
                None
            };
            let value_root = {
                let _query = memory.reserve(512 << 10, class).unwrap();
                let state_root = StateRootV1::from_canonical_bytes(&state_root_bytes).unwrap();
                state_root
                    .get(record.field_key.as_bytes(), &state_source)
                    .unwrap()
                    .unwrap()
            };
            assert_eq!(value_root, record.value_root);
            let source = store.value_session(physical());
            let mut io = logical();
            let mut reader = ValueReadContext {
                source: &source,
                memory: &memory,
                class,
                io: &mut io,
            };
            let mut key = HashOutput {
                hash: blake3::Hasher::new(),
                bytes: 0,
            };
            let mut payload = HashOutput {
                hash: blake3::Hasher::new(),
                bytes: 0,
            };
            let info = read_member_record_v1(
                &value_root,
                &record.field_key,
                &mut reader,
                MemberRecordLimitsV1 {
                    semantic_bytes: 128 << 20,
                    payload_bytes: 128 << 20,
                    record_bytes: 192 << 20,
                },
                &mut key,
                &mut payload,
                || false,
            )
            .unwrap();
            assert_eq!(info, record.info);
            assert_eq!(info.codec, MemberPayloadCodecV1::Ledger);
            assert_eq!(key.bytes, info.semantic_bytes);
            assert_eq!(payload.bytes, info.payload_bytes);
            assert_eq!(key.hash.finalize(), expected_key);
            assert_eq!(payload.hash.finalize(), expected_payload);
        }
    }
}
