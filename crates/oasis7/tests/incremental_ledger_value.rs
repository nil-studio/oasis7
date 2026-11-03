#![cfg(all(unix, not(target_arch = "wasm32")))]

use oasis7_distfs::incremental_page_store::{IncrementalPageStore, PageCompression, PageIoBudget};
use oasis7_formal_protocol::incremental_value::{
    ValueIoBudget, ValueReadContext, ValueStreamWriter,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass};
use oasis7_runtime::{incremental_ledger_value::*, types::MaterialLedgerId};
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
            "oasis7-ledger-value-{}-{nonce}-{sequence}",
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
fn large_unicode_ledger_reopens_raw_and_zstd_with_bounded_ordinary_and_recovery_reads() {
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
        let expected = blake3::hash(match &*source {
            MaterialLedgerId::Factory(id) => id.as_bytes(),
            _ => unreachable!(),
        });
        let root = {
            let store = IncrementalPageStore::create(&path, memory.clone(), compression).unwrap();
            let mut sink = store.value_session(physical());
            let mut quota = logical();
            let mut writer = ValueStreamWriter::new(
                &mut sink,
                &memory,
                &mut quota,
                ledger_value_len_v1(&source).unwrap(),
                || false,
            )
            .unwrap();
            assert_eq!(
                write_ledger_value_v1(&source, &mut writer).unwrap(),
                ledger_value_len_v1(&source).unwrap()
            );
            writer.finish().unwrap().digest().unwrap()
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
            let store =
                IncrementalPageStore::open(&path, memory.clone(), compression, class).unwrap();
            let _ordinary_filler = if class == MemoryClass::Recovery {
                let filler = memory
                    .reserve((1 << 20) - memory.used(), MemoryClass::Ordinary)
                    .unwrap();
                assert!(memory.reserve(1, MemoryClass::Ordinary).is_err());
                Some(filler)
            } else {
                None
            };
            let source = store.value_session(physical());
            let mut quota = logical();
            let mut context = ValueReadContext {
                source: &source,
                memory: &memory,
                class,
                io: &mut quota,
            };
            let mut output = HashOutput {
                hash: blake3::Hasher::new(),
                bytes: 0,
            };
            assert_eq!(
                read_ledger_value_tree_v1(&root, &mut context, length as u64, &mut output, || {
                    false
                })
                .unwrap(),
                LedgerValueInfoV1 {
                    kind: LedgerValueKindV1::Factory,
                    id_bytes: length as u64
                }
            );
            assert_eq!(output.bytes, length as u64);
            assert_eq!(output.hash.finalize(), expected);
        }
    }
}
