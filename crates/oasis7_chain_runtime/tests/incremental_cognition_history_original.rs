#![cfg(all(unix, not(target_arch = "wasm32")))]
use oasis7_chain_runtime::incremental_cognition_history_store::CognitionHistoryObjectSinkV1;
use oasis7_distfs::incremental_page_store::{
    IncrementalPageStore, PageCompression, PageIoBudget, PageStoreError,
    objects::{IncrementalObjectStore, ObjectIo},
};
use oasis7_formal_protocol::{
    incremental_material::OriginalObjectStreamReader,
    incremental_value::{ValueIoBudget, ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass};
use oasis7_wasm_abi::CanonicalCborWorkV1;
use oasis7_world_state::incremental_cognition_history::*;
use oasis7_world_state::{
    incremental_field_member::*, incremental_field_read::WorldFieldReadContextV1,
};
use std::{
    io::Write,
    path::PathBuf,
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(0);
#[path = "support/cognition_history_enabled.rs"]
mod enabled;
#[path = "support/cognition_history_normal.rs"]
mod normal;
#[path = "support/cognition_history_readmission.rs"]
mod readmission;
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "oasis7-cognition-original-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
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
        reads_remaining: 4_000_000,
        read_bytes_remaining: 512 << 20,
        writes_remaining: 1_000_000,
        write_bytes_remaining: 512 << 20,
    }
}
fn logical() -> ValueIoBudget {
    ValueIoBudget {
        nodes_remaining: 1_000_000,
        read_bytes_remaining: 512 << 20,
        write_bytes_remaining: 512 << 20,
    }
}
struct HashOutput(blake3::Hasher, u64);
impl Write for HashOutput {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.update(bytes);
        self.1 += bytes.len() as u64;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn actual_cognition_history_original_finishes_then_reopens_without_dto_or_old_pool() {
    for (compression, length) in [
        (PageCompression::Raw, 32 << 20),
        (PageCompression::Zstd, 4096),
    ] {
        let directory = Directory::new();
        let path = directory.0.join("pages");
        let (id, encoded) = {
            // Original borrowed DTO is caller-owned, charged before allocation.
            // The independent engine pool remains smaller than the large row.
            let input = MemoryBudget::new(64 << 20, 4 << 20).unwrap();
            let _input = input
                .reserve(length + (1 << 20), MemoryClass::Ordinary)
                .unwrap();
            let value = serde_json::json!({"response_digest":"", "response_artifact":{"reason":"r".repeat(length)},"envelope_digest":"原 envelope", "journal_head":"原 head"});
            let memory = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
            let store = IncrementalPageStore::create(&path, memory.clone(), compression).unwrap();
            let cancelled = AtomicBool::new(false);
            let mut physical = physical();
            let mut logical = logical();
            let objects =
                IncrementalObjectStore::create(&store, &mut physical, &cancelled).unwrap();
            let record = CognitionHistoryRecordRefV1::from_original(
                CognitionHistoryFieldV1::Responses,
                9,
                &value,
            )
            .unwrap();
            let mut context = CognitionHistoryWriteContextV1::new(
                &memory,
                CanonicalCborWorkV1 {
                    visits_remaining: 1_000_000,
                    bytes_remaining: 128 << 20,
                    max_depth: 32,
                    max_map_keys: 128,
                    max_key_bytes: 4096,
                },
                1,
                || false,
            );
            let stored = store_cognition_history_original_v1(
                record,
                &mut context,
                &mut CognitionHistoryObjectSinkV1 {
                    objects: &objects,
                    io: &mut ObjectIo {
                        physical: &mut physical,
                        logical: &mut logical,
                        class: MemoryClass::Ordinary,
                        cancelled: &cancelled,
                    },
                    cancelled: || false,
                },
            )
            .unwrap();
            assert_eq!(stored.ordinal(), 9);
            assert!(!context.is_poisoned());
            (*stored.id(), stored.encoded_bytes())
        };
        // All old pools, DTOs and handles have left scope. Only flat original ID
        // and length survive; cold read verifies the actual stored original.
        let memory = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
        let store =
            IncrementalPageStore::open(&path, memory.clone(), compression, MemoryClass::Recovery)
                .unwrap();
        let cancelled = AtomicBool::new(false);
        let mut physical = physical();
        let mut logical = logical();
        let objects =
            IncrementalObjectStore::open(&store, MemoryClass::Recovery, &mut physical, &cancelled)
                .unwrap();
        let mut io = ObjectIo {
            physical: &mut physical,
            logical: &mut logical,
            class: MemoryClass::Recovery,
            cancelled: &cancelled,
        };
        let mut output = HashOutput(blake3::Hasher::new(), 0);
        objects
            .original_reader(&mut io)
            .stream_original(&id, encoded, &memory, &mut output)
            .unwrap();
        assert_eq!(output.1, encoded);
        assert_eq!(output.0.finalize().as_bytes(), &id);
    }
}

#[path = "support/cognition_history_index.rs"]
mod index;

#[path = "support/cognition_history_signed.rs"]
mod signed;

fn state_error(error: PageStoreError) -> ValueTreeError {
    match error {
        PageStoreError::Memory(e) => ValueTreeError::Memory(e),
        PageStoreError::ResourceLimited(s) => ValueTreeError::ResourceLimited(s),
        PageStoreError::Cancelled => ValueTreeError::Cancelled,
        PageStoreError::Io(e) => ValueTreeError::Io(e),
        PageStoreError::Missing(_) => ValueTreeError::Io(std::io::ErrorKind::NotFound.into()),
        PageStoreError::Corrupt(s) => ValueTreeError::Invalid(s),
        PageStoreError::UnsupportedPlatform => ValueTreeError::Invalid("History fixture platform"),
    }
}

#[path = "support/cognition_history_absence.rs"]
mod absence;

#[path = "support/cognition_history_cache_physical.rs"]
mod cache_physical;

#[path = "support/cognition_history_v2_signed.rs"]
mod v2_signed;

#[path = "support/cognition_history_normal_accept.rs"]
mod normal_accept;

#[path = "support/cognition_history_lifecycle_physical.rs"]
mod lifecycle_physical;

#[path = "support/cognition_history_journal_normal.rs"]
mod journal_normal;
