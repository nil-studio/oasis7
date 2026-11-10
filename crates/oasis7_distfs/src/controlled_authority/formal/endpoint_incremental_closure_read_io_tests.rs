use super::*;
use crate::incremental_page_store::{IncrementalPageStore, PageCompression, PageIoBudget};
use oasis7_formal_protocol::incremental_value::write_value;
use oasis7_proto::memory_budget::MemoryBudget;
use std::sync::atomic::AtomicBool;
#[test]
fn current_value_ranges_copy_with_storage_writes_disabled() {
    let path = std::env::temp_dir().join(format!(
        "oasis7-closure-read-io-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(path.clone());
    let memory = MemoryBudget::new(32 << 20, 0).unwrap();
    let pages = IncrementalPageStore::create(&path, memory.clone(), PageCompression::Raw).unwrap();
    let physical = PageIoBudget {
        reads_remaining: 128,
        read_bytes_remaining: 1 << 20,
        writes_remaining: 128,
        write_bytes_remaining: 1 << 20,
    };
    let payload = b"actual Current native field bytes";
    let mut sink = pages.value_session(physical.clone());
    let root = write_value(
        &mut &payload[..],
        payload.len() as u64,
        &mut sink,
        &memory,
        &mut read_value_io(128, 1 << 20),
        || false,
    )
    .unwrap();
    drop(sink);
    let stop = AtomicBool::new(false);
    let source = pages.value_session_with_cancelled(
        PageIoBudget {
            writes_remaining: 0,
            write_bytes_remaining: 0,
            ..physical
        },
        &stop,
    );
    let mut io = read_value_io(128, 1 << 20);
    let mut read = ValueReadContext {
        source: &source,
        memory: &memory,
        class: MemoryClass::Ordinary,
        io: &mut io,
    };
    let checked = read.load_root(&root.digest().unwrap()).unwrap();
    let mut output = Vec::new();
    read.read_range(&checked, 0, payload.len() as u64, &mut output, || false)
        .unwrap();
    assert_eq!(output, payload);
    assert_eq!(io.write_bytes_remaining, (1 << 20) - payload.len() as u64);
}
