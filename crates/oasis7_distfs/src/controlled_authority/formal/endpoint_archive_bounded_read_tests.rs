use super::super::super::{business_pack, physical};
use super::super::{FileFormalArchiveSourceV1, archive_transfer::bounded_local_for_test};
use super::*;
use std::{
    fs,
    sync::atomic::{AtomicUsize, Ordering},
};
struct Fixture(std::path::PathBuf);
impl Fixture {
    fn new(physical_mode: bool) -> Self {
        let root = std::env::temp_dir().canonicalize().unwrap().join(format!(
            "oasis7-archive-paid-{}",
            super::super::super::super::fencing::fresh_challenge().unwrap()
        ));
        fs::create_dir(&root).unwrap();
        fs::write(root.join("formal-checkpoint-v1.cbor"), b"fixture").unwrap();
        fs::create_dir(root.join("formal-objects-v1")).unwrap();
        if physical_mode {
            physical::initialize(&root.join("formal-objects-v1")).unwrap();
        }
        Self(root)
    }
    fn dir(&self) -> std::path::PathBuf {
        self.0.join("formal-objects-v1")
    }
    fn source(&self) -> FileFormalArchiveSourceV1 {
        FileFormalArchiveSourceV1::new(self.0.clone()).unwrap()
    }
    fn raw(&self, bytes: &[u8]) -> String {
        let id = blake3::hash(bytes).to_hex().to_string();
        fs::write(self.dir().join(&id), bytes).unwrap();
        id
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn memory() -> MemoryBudget {
    MemoryBudget::new(256 << 20, 0).unwrap()
}
fn work() -> FormalArchiveReadWorkV1 {
    FormalArchiveReadWorkV1::new(1_000_000, 128 << 20).unwrap()
}
struct OldOnly(AtomicUsize);
impl FormalArchiveSourceV1 for OldOnly {
    fn read_object(&self, _: &str) -> Result<Option<Vec<u8>>, FormalError> {
        self.0.fetch_add(1, Ordering::SeqCst);
        panic!("bounded default must not call whole-read")
    }
}
#[test]
fn object_safe_default_explicitly_unsupported_without_old_whole_read() {
    let old = OldOnly(AtomicUsize::new(0));
    let source: &dyn FormalArchiveSourceV1 = &old;
    let error = source
        .read_object_bounded(&"00".repeat(32), 100, &memory(), &work(), &mut || false)
        .unwrap_err();
    assert!(error.to_string().contains("provider unsupported"));
    assert_eq!(old.0.load(Ordering::SeqCst), 0);
}
#[test]
fn actual_file_trait_reads_raw_pack_physical_and_generation_paid_owners() {
    for mode in 0..4 {
        let f = Fixture::new(mode >= 2);
        let bytes = vec![71; 4000];
        let id = blake3::hash(&bytes).to_hex().to_string();
        if mode == 0 {
            f.raw(&bytes);
        } else if mode == 1 {
            business_pack::persist(
                &f.dir(),
                &[
                    super::super::super::business_index::BusinessIndexNodeWriteV1 {
                        digest: id.clone(),
                        bytes: bytes.clone(),
                    },
                ],
            )
            .unwrap();
        } else {
            let padding = vec![92; 128 << 10];
            let mut objects = vec![
                (id.clone(), bytes.clone()),
                (blake3::hash(&padding).to_hex().to_string(), padding),
            ];
            objects.sort_by(|a, b| a.0.cmp(&b.0));
            assert!(physical::persist(&f.dir(), &objects).unwrap());
            if mode == 3 {
                physical::roots::retain(&f.dir(), std::slice::from_ref(&id)).unwrap();
                let mut session = physical::GenerationSession::default();
                let mut done = false;
                for _ in 0..100 {
                    let p = physical::advance_with_session(&f.dir(), &mut session).unwrap();
                    if !p.building {
                        done = true;
                        break;
                    }
                }
                assert!(done);
            }
        }
        let file = f.source();
        let source: &dyn FormalArchiveSourceV1 = &file;
        let memory = memory();
        let w = work();
        let owner = source
            .read_object_bounded(&id, bytes.len(), &memory, &w, &mut || false)
            .unwrap()
            .unwrap();
        assert_eq!(owner.as_slice(), bytes);
        assert!(owner.uses_budget(&memory));
        assert_eq!(memory.used(), owner.reserved_bytes());
        drop(owner);
        assert_eq!(memory.used(), 0);
        assert!(
            source
                .read_object_bounded(&id, bytes.len() - 1, &memory, &work(), &mut || false)
                .is_err()
        );
        assert_eq!(memory.used(), 0);
    }
}
#[test]
fn actual_local_archive_fallback_uses_continuing_work_and_stops_on_corruption() {
    let first = Fixture::new(false);
    let next = Fixture::new(false);
    let bytes = b"actual fallback original";
    let id = next.raw(bytes);
    let source = bounded_local_for_test(first.source(), vec![Arc::new(next.source())]);
    let memory = memory();
    let w = work();
    let initial = w.remaining_read_bytes();
    let owner = source
        .read_object_bounded(&id, bytes.len(), &memory, &w, &mut || false)
        .unwrap()
        .unwrap();
    assert_eq!(owner.as_slice(), bytes);
    drop(owner);
    assert_eq!(memory.used(), 0);
    let spent = initial - w.remaining_read_bytes();
    assert!(spent > 0);
    w.consume_read_bytes(w.remaining_read_bytes() - (spent - 1), &mut || false)
        .unwrap();
    assert!(
        source
            .read_object_bounded(&id, bytes.len(), &memory, &w, &mut || false)
            .is_err()
    );
    assert_eq!(w.remaining_read_bytes(), 0);
    assert_eq!(memory.used(), 0);
    // The first source now contains corrupt bytes under the requested identity.
    // Its error must not be hidden by the valid fallback's duplicate original.
    fs::write(first.dir().join(&id), b"bad").unwrap();
    assert!(
        source
            .read_object_bounded(&id, bytes.len(), &memory, &work(), &mut || false)
            .is_err()
    );
    let unsupported: Arc<dyn FormalArchiveSourceV1> = Arc::new(OldOnly(AtomicUsize::new(0)));
    let empty = Fixture::new(false);
    let source = bounded_local_for_test(empty.source(), vec![unsupported, Arc::new(next.source())]);
    let error = source
        .read_object_bounded(&id, bytes.len(), &memory, &work(), &mut || false)
        .unwrap_err();
    assert!(error.to_string().contains("provider unsupported"));
    assert_eq!(memory.used(), 0);
}
// An external provider can only forward a genuine owner. Even such an owner
// must be checked against the caller's particular pool, identity and byte cap.
struct Forward {
    source: FileFormalArchiveSourceV1,
    id: String,
    foreign: Option<MemoryBudget>,
    alternate_meter: bool,
}
impl FormalArchiveSourceV1 for Forward {
    fn read_object(&self, _: &str) -> Result<Option<Vec<u8>>, FormalError> {
        panic!("no old whole-read")
    }
    fn read_object_bounded(
        &self,
        _: &str,
        _: usize,
        memory: &MemoryBudget,
        w: &FormalArchiveReadWorkV1,
        stop: &mut dyn FnMut() -> bool,
    ) -> Result<Option<FormalArchiveReadOwnerV1>, FormalError> {
        let alternate = work();
        let producer_meter = if self.alternate_meter { &alternate } else { w };
        self.source.read_object_bounded(
            &self.id,
            10000,
            self.foreign.as_ref().unwrap_or(memory),
            producer_meter,
            stop,
        )
    }
}
#[test]
fn external_forwarded_genuine_owner_is_rechecked_for_pool_cap_and_hash() {
    let f = Fixture::new(false);
    let id = f.raw(b"forwarded genuine bytes");
    let absent = Fixture::new(false);
    let memory = memory();
    for mode in 0..3 {
        let provider: Arc<dyn FormalArchiveSourceV1> = Arc::new(Forward {
            source: f.source(),
            id: id.clone(),
            foreign: (mode == 0).then(|| MemoryBudget::new(256 << 20, 0).unwrap()),
            alternate_meter: false,
        });
        let source = bounded_local_for_test(absent.source(), vec![provider]);
        let requested = if mode == 2 {
            "00".repeat(32)
        } else {
            id.clone()
        };
        let max = if mode == 1 { 1 } else { 100 };
        assert!(
            source
                .read_object_bounded(&requested, max, &memory, &work(), &mut || false)
                .is_err()
        );
        assert_eq!(memory.used(), 0);
    }
}
#[test]
fn cancellation_steps_io_and_memory_errors_never_return_an_owner() {
    let f = Fixture::new(false);
    let id = f.raw(b"budgeted archive data");
    let source = f.source();
    for mode in 0..4 {
        let memory = if mode == 2 {
            MemoryBudget::new(0, 0).unwrap()
        } else {
            memory()
        };
        let w = work();
        if mode == 0 {
            w.consume_steps(w.remaining_steps(), &mut || false).unwrap();
        }
        if mode == 1 {
            w.consume_read_bytes(w.remaining_read_bytes(), &mut || false)
                .unwrap();
        }
        let mut observations = 0;
        assert!(
            source
                .read_object_bounded(&id, 100, &memory, &w, &mut || {
                    observations += 1;
                    mode == 3 && observations >= 5
                })
                .is_err()
        );
        assert_eq!(memory.used(), 0);
    }
    let empty = Fixture::new(false);
    assert!(
        empty
            .source()
            .read_object_bounded(&id, 100, &memory(), &work(), &mut || false)
            .unwrap()
            .is_none()
    );
}

#[test]
fn actual_file_trait_selected_generation_missing_rejects_stale_overlay() {
    let f = Fixture::new(true);
    let bytes = vec![73; 4000];
    let id = f.raw(&bytes);
    physical::roots::retain(&f.dir(), std::slice::from_ref(&id)).unwrap();
    let mut session = physical::GenerationSession::default();
    let mut selected = None;
    for _ in 0..100 {
        let p = physical::advance_with_session(&f.dir(), &mut session).unwrap();
        if !p.building {
            selected = p.selected;
            break;
        }
    }
    let dir = f
        .dir()
        .join(".generations-v1")
        .join(selected.expect("real generation finished"));
    let memory = memory();
    let source = f.source();
    drop(
        source
            .read_object_bounded(&id, bytes.len(), &memory, &work(), &mut || false)
            .unwrap()
            .unwrap(),
    );
    for path in [dir.join(&id), dir.join(".representation-v1").join(&id)] {
        match fs::remove_file(path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => panic!("{e}"),
        }
    }
    fs::write(f.dir().join(&id), &bytes).unwrap();
    let error = source
        .read_object_bounded(&id, bytes.len(), &memory, &work(), &mut || false)
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("selected generation object missing"),
        "{error}"
    );
    assert_eq!(memory.used(), 0);
}

#[test]
fn forwarded_owner_from_another_genuine_meter_is_rejected() {
    let f = Fixture::new(false);
    let id = f.raw(b"genuine bytes wrong meter");
    let absent = Fixture::new(false);
    let memory = memory();
    let provider: Arc<dyn FormalArchiveSourceV1> = Arc::new(Forward {
        source: f.source(),
        id: id.clone(),
        foreign: None,
        alternate_meter: true,
    });
    let source = bounded_local_for_test(absent.source(), vec![provider]);
    let meter = work();
    let error = source
        .read_object_bounded(&id, 100, &memory, &meter, &mut || false)
        .unwrap_err();
    assert!(
        error.to_string().contains("owner meter mismatch"),
        "{error}"
    );
    assert_eq!(memory.used(), 0);
}
#[test]
fn cancellation_callback_alias_charge_and_nested_read_are_rejected_during_child_lease() {
    let f = Fixture::new(false);
    let id = f.raw(b"genuine lease bytes");
    let source = f.source();
    let memory = memory();
    let baseline = work();
    drop(
        source
            .read_object_bounded(&id, 100, &memory, &baseline, &mut || false)
            .unwrap()
            .unwrap(),
    );
    let meter = work();
    let mut observed = false;
    drop(
        source
            .read_object_bounded(&id, 100, &memory, &meter, &mut || {
                if meter.is_child_active() && !observed {
                    observed = true;
                    let before = (meter.remaining_steps(), meter.remaining_read_bytes());
                    let error = meter.charge(3, 7, &mut || false).unwrap_err();
                    assert!(error.to_string().contains("child read active"));
                    assert_eq!(
                        before,
                        (meter.remaining_steps(), meter.remaining_read_bytes())
                    );
                    let error = source
                        .read_object_bounded(&id, 100, &memory, &meter, &mut || false)
                        .unwrap_err();
                    assert!(error.to_string().contains("child read active"));
                    assert_eq!(
                        before,
                        (meter.remaining_steps(), meter.remaining_read_bytes())
                    );
                }
                false
            })
            .unwrap()
            .unwrap(),
    );
    assert!(observed);
    assert_eq!(meter.remaining_steps(), baseline.remaining_steps());
    assert_eq!(
        meter.remaining_read_bytes(),
        baseline.remaining_read_bytes()
    );
    assert!(!meter.is_child_active());
    assert_eq!(memory.used(), 0);
}
#[test]
fn reconciliation_keeps_first_child_error_and_never_issues_success_after_depletion() {
    let meter = FormalArchiveReadWorkV1::new(10, 10).unwrap();
    meter.charge(9, 9, &mut || false).unwrap();
    let error = meter
        .reconcile(
            (10, 10),
            (8, 8),
            Err::<(), _>(invalid("original child IO sentinel")),
        )
        .unwrap_err();
    assert!(error.to_string().contains("original child IO sentinel"));
    assert_eq!(
        (meter.remaining_steps(), meter.remaining_read_bytes()),
        (0, 0)
    );
    let meter = FormalArchiveReadWorkV1::new(10, 10).unwrap();
    meter.charge(9, 9, &mut || false).unwrap();
    assert!(meter.reconcile((10, 10), (8, 8), Ok(())).is_err());
    assert_eq!(
        (meter.remaining_steps(), meter.remaining_read_bytes()),
        (0, 0)
    );
}
#[test]
fn local_file_child_panic_exhausts_meter_before_caught_retry() {
    let fixture = Fixture::new(false);
    let id = fixture.raw(b"real local file panic evidence");
    let source = fixture.source();
    let memory = memory();
    let meter = work();
    let mut child_polls = 0;
    let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        source.read_object_bounded(&id, 100, &memory, &meter, &mut || {
            if meter.is_child_active() {
                child_polls += 1;
                if child_polls == 3 {
                    panic!("local file child panic sentinel");
                }
            }
            false
        })
    }));
    assert!(caught.is_err());
    assert_eq!(child_polls, 3);
    assert_eq!(memory.used(), 0);
    assert!(!meter.is_child_active());
    assert_eq!(meter.remaining_steps(), 0, "unwound child must fence retry");
    assert_eq!(meter.remaining_read_bytes(), 0);
    assert!(
        source
            .read_object_bounded(&id, 100, &memory, &meter, &mut || false)
            .is_err()
    );
}
#[test]
fn common_context_and_real_file_owner_use_one_continuing_meter() {
    let fixture = Fixture::new(false);
    let id = fixture.raw(b"one common meter across actual local IO");
    let source = fixture.source();
    let memory = memory();
    let meter = work();
    let control = work();
    let mut stop = || false;
    let identity = {
        let mut context = meter.bounded_context(&memory, &mut stop);
        assert!(std::ptr::eq(context.memory(), &memory));
        let identity = context.work().identity();
        context.charge(3, 7).unwrap();
        identity
    };
    let owner = source
        .read_object_bounded(&id, 100, &memory, &meter, &mut || false)
        .unwrap()
        .unwrap();
    assert_eq!(owner.as_slice(), b"one common meter across actual local IO");
    assert!(owner.uses_budget(&memory));
    assert_eq!(memory.used(), owner.reserved_bytes());
    drop(
        source
            .read_object_bounded(&id, 100, &memory, &control, &mut || false)
            .unwrap()
            .unwrap(),
    );
    {
        let mut context = meter.bounded_context(&memory, &mut stop);
        assert_eq!(context.work().identity(), identity);
        context.charge(2, 5).unwrap();
    }
    control.charge(5, 12, &mut || false).unwrap();
    assert_eq!(meter.remaining_steps(), control.remaining_steps());
    assert_eq!(meter.remaining_read_bytes(), control.remaining_read_bytes());
    drop(owner);
    assert_eq!(memory.used(), 0);
}
