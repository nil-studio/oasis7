use super::*;
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let root = std::env::temp_dir().canonicalize().unwrap().join(format!(
            "oasis7-local-maintenance-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).unwrap();
        Self(root)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
#[test]
fn same_native_generation_accepts_reopen_and_changed_generation_requires_rebuild() {
    use crate::controlled_authority::formal::{
        business_index::BusinessIndexNodeWriteV1, business_pack, physical,
    };
    let fixture = Directory::new();
    physical::initialize(&fixture.0).unwrap();
    let memory = MemoryBudget::new(256 << 20, 16 << 20).unwrap();
    let original = physical::current_generation_digest_uncached(
        &fixture.0,
        &memory,
        MemoryClass::Ordinary,
        &mut || false,
    )
    .unwrap();
    assert_eq!(original, None);
    let record = LocalDataClosureRecordV1 {
        version: 1,
        instance: [1; 32],
        authority: [2; 32],
        activation: [3; 32],
        epoch: 1,
        record: [4; 32],
        body: [5; 32],
        roots: [6; 32],
        manifest: [7; 32],
        page_lease: (8, 9),
        native_lease: (10, 11),
        native_generation: original,
        cognition: LocalFactLocationV1 {
            height: 0,
            ordinal: 0,
            original: [12; 32],
        },
        runtime: LocalFactLocationV1 {
            height: 0,
            ordinal: 1,
            original: [13; 32],
        },
    };
    assert_eq!(
        check_maintenance_generation(&record, &fixture.0, &memory, &mut || false).unwrap(),
        original
    );
    let bytes = b"a real retained native object".to_vec();
    business_pack::persist(
        &fixture.0,
        &[BusinessIndexNodeWriteV1 {
            digest: blake3::hash(&bytes).to_hex().to_string(),
            bytes,
        }],
    )
    .unwrap();
    let mut complete = false;
    let mut session = physical::GenerationSession::default();
    for _ in 0..64 {
        let progress = physical::advance_with_budget(
            &fixture.0,
            &mut session,
            physical::GenerationBudget::explicit(),
        )
        .unwrap();
        if !progress.building {
            complete = true;
            break;
        }
    }
    assert!(complete, "actual generation maintenance did not finish");
    assert!(
        physical::current_generation_digest_uncached(
            &fixture.0,
            &memory,
            MemoryClass::Ordinary,
            &mut || false
        )
        .unwrap()
        .is_some()
    );
    let error =
        check_maintenance_generation(&record, &fixture.0, &memory, &mut || false).unwrap_err();
    assert!(
        matches!(error,FormalError::Invalid(ref reason) if reason.contains("generation changed"))
    );
    assert_eq!(memory.used(), 0);
}
