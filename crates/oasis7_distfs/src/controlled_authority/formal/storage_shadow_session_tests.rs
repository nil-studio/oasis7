use super::*;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture {
    root: PathBuf,
    source: PathBuf,
    target: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "o7-shadow-session-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let source = root.join("source");
        fs::create_dir_all(source.join("formal-objects-v1")).unwrap();
        File::create(source.join(".formal-endpoint.lock")).unwrap();
        for byte in 0..8u8 {
            let bytes = vec![byte; 10000];
            fs::write(
                source
                    .join("formal-objects-v1")
                    .join(blake3::hash(&bytes).to_hex().as_str()),
                bytes,
            )
            .unwrap();
        }
        let target = root.join("shadow");
        Self {
            root,
            source,
            target,
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
#[test]
fn live_shadow_session_recovers_once_fences_other_writers_and_recovers_after_error_or_reopen() {
    let fixture = Fixture::new();
    let options = FormalStorageShadowOptions {
        optimized: true,
        max_logical_bytes: 1,
    };
    let mut session = FormalStorageShadowSession::default();
    super::super::physical::publication_recovery_count_reset();
    for _ in 0..2 {
        assert!(
            !advance_formal_storage_shadow_with_session(
                &fixture.source,
                &fixture.target,
                &options,
                &mut session
            )
            .unwrap()
            .complete
        );
    }
    assert_eq!(super::super::physical::publication_recovery_count(), 1);
    let error =
        advance_formal_storage_shadow(&fixture.source, &fixture.target, &options).unwrap_err();
    assert!(
        format!("{error:?}").contains("shadow destination busy"),
        "live owner must fence even a stateless competing writer"
    );
    assert!(
        !advance_formal_storage_shadow_with_session(
            &fixture.source,
            &fixture.target,
            &options,
            &mut session
        )
        .unwrap()
        .complete
    );
    assert_eq!(super::super::physical::publication_recovery_count(), 1);
    let invalid_options = FormalStorageShadowOptions {
        optimized: true,
        max_logical_bytes: 0,
    };
    assert!(
        advance_formal_storage_shadow_with_session(
            &fixture.source,
            &fixture.target,
            &invalid_options,
            &mut session
        )
        .is_err()
    );
    assert!(
        !advance_formal_storage_shadow_with_session(
            &fixture.source,
            &fixture.target,
            &options,
            &mut session
        )
        .unwrap()
        .complete
    );
    assert_eq!(super::super::physical::publication_recovery_count(), 2);
    drop(session);
    let mut reopened = FormalStorageShadowSession::default();
    assert!(
        !advance_formal_storage_shadow_with_session(
            &fixture.source,
            &fixture.target,
            &options,
            &mut reopened
        )
        .unwrap()
        .complete
    );
    assert_eq!(super::super::physical::publication_recovery_count(), 3);
}
