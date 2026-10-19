//! Real signed closure copies count source reads, then require an independent
//! local readback. The fixture source never grants authority or supplies a memo.
use super::*;
use std::sync::Mutex;
use std::time::Duration;

struct CountedSource {
    source: FileFormalArchiveSourceV1,
    reads: Mutex<BTreeMap<String, usize>>,
}
impl CountedSource {
    fn new(directory: PathBuf) -> Self {
        Self {
            source: FileFormalArchiveSourceV1::new(directory).unwrap(),
            reads: Mutex::new(BTreeMap::new()),
        }
    }
    fn reads(&self, id: &str) -> usize {
        self.reads.lock().unwrap().get(id).copied().unwrap_or(0)
    }
}
impl FormalArchiveSourceV1 for CountedSource {
    fn read_object(&self, id: &str) -> Result<Option<Vec<u8>>, FormalError> {
        *self.reads.lock().unwrap().entry(id.into()).or_default() += 1;
        self.source.read_object(id)
    }
}
fn target(fixture: &Fixture) -> PathBuf {
    let target = fixture.path.join("independent-copy-target");
    fs::create_dir(&target).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&target, fs::Permissions::from_mode(0o700)).unwrap();
    }
    target
}

#[test]
fn healthy_copy_source_fetches_shared_history_index_once() {
    let fixture = Fixture::new();
    let mut coordinator = fixture.coordinator();
    coordinator.submit(fixture.initial.clone()).unwrap();
    let primary = coordinator.endpoints_mut().0;
    let checkpoint = primary.checkpoint().unwrap();
    primary.seal_epoch(&h(210), 2, &checkpoint).unwrap();
    let index = primary.store.body.index.clone().unwrap();
    let native = Store::read_native_checkpoint(&fixture.pdir).unwrap();
    let source = CountedSource::new(fixture.pdir.clone());
    let target = target(&fixture);
    let copied = super::super::store::durable_set::copy_authenticated_closure(
        &target,
        &native,
        &public(5),
        &source,
        1,
        64 * 1024 * 1024,
        true,
    )
    .unwrap();
    let local = FileFormalArchiveSourceV1::new(target.clone()).unwrap();
    let verified = super::super::store::durable_set::copy_authenticated_closure(
        &target,
        &native,
        &public(5),
        &local,
        1,
        64 * 1024 * 1024,
        false,
    )
    .unwrap();
    assert_eq!(copied, verified);
    assert_eq!(
        source.reads(&index),
        1,
        "history lookup and index walk must reuse the same hash-verified source bytes"
    );
}

#[test]
fn healthy_copy_source_reuses_authenticated_inline_business_nodes() {
    let snapshot = serde_cbor::to_vec(&serde_json::json!({
        "state": { "retained_history_root": null }
    }))
    .unwrap();
    let (fixture, envelope) = super::business_tests::fixture_with_snapshot(snapshot);
    let mut coordinator = fixture.coordinator();
    coordinator.submit(fixture.initial.clone()).unwrap();
    let primary = coordinator.endpoints_mut().0;
    let checkpoint = primary.checkpoint().unwrap();
    primary.seal_epoch(&h(210), 2, &checkpoint).unwrap();
    let native = Store::read_native_checkpoint(&fixture.pdir).unwrap();
    let source = CountedSource::new(fixture.pdir.clone());
    let target = target(&fixture);
    let copied = super::super::store::durable_set::copy_authenticated_closure(
        &target,
        &native,
        &public(5),
        &source,
        2,
        64 * 1024 * 1024,
        true,
    )
    .unwrap();
    let local = FileFormalArchiveSourceV1::new(target.clone()).unwrap();
    let verified = super::super::store::durable_set::copy_authenticated_closure(
        &target,
        &native,
        &public(5),
        &local,
        2,
        64 * 1024 * 1024,
        false,
    )
    .unwrap();
    assert_eq!(copied, verified);
    assert_eq!(envelope.delta.node_writes.len(), 257);
    let repeated_reads: usize = envelope
        .delta
        .node_writes
        .iter()
        .map(|write| source.reads(&write.digest))
        .sum();
    assert_eq!(
        repeated_reads, 0,
        "the authenticated Result already carries every node; copying must not refetch them"
    );
    // A verified inline copy is not an install verdict. Independent readback
    // must still detect damage to the newly written physical pack.
    fs::write(
        target
            .join("formal-objects-v1")
            .join(&envelope.delta.node_writes[0].digest),
        b"corrupt independently stored business pack",
    )
    .unwrap();
    assert!(
        super::super::store::durable_set::copy_authenticated_closure(
            &target,
            &native,
            &public(5),
            &local,
            2,
            64 * 1024 * 1024,
            false,
        )
        .is_err()
    );
}

#[test]
fn healthy_copy_source_rechecks_new_manifest_and_stale_source_on_each_walk() {
    let fixture = Fixture::new();
    let mut coordinator = fixture.coordinator();
    coordinator.submit(fixture.initial.clone()).unwrap();
    let primary = coordinator.endpoints_mut().0;
    let checkpoint = primary.checkpoint().unwrap();
    primary.seal_epoch(&h(210), 2, &checkpoint).unwrap();
    let mut body = primary.store.body.clone();
    let index = body.index.clone().unwrap();
    let native = Store::read_native_checkpoint(&fixture.pdir).unwrap();
    let source = CountedSource::new(fixture.pdir.clone());
    let target = target(&fixture);
    let copy = |checkpoint: &[u8]| {
        super::super::store::durable_set::copy_authenticated_closure(
            &target,
            checkpoint,
            &public(5),
            &source,
            1,
            64 * 1024 * 1024,
            true,
        )
    };
    copy(&native).unwrap();
    // Even an otherwise correctly signed new checkpoint cannot borrow the
    // prior walk's index roots, bytes or membership verdict.
    body.index = Some(h(211));
    let changed = Store::encode_provider_checkpoint(
        &body,
        &signed(Store::checkpoint_signing_bytes(&body).unwrap(), 5),
        &public(5),
    )
    .unwrap();
    assert!(copy(&changed).is_err());
    let before = source.reads(&index);
    fs::write(
        fixture.pdir.join("formal-objects-v1").join(&index),
        b"corrupt source after prior successful walk",
    )
    .unwrap();
    assert!(copy(&native).is_err());
    assert_eq!(source.reads(&index), before + 1);
}

#[test]
fn healthy_copy_concurrent_walk_preserves_paused_run_and_exact_inventory() {
    use std::sync::mpsc;
    struct PausedSource {
        source: FileFormalArchiveSourceV1,
        target: PathBuf,
        paused_once: std::sync::atomic::AtomicBool,
        paused: mpsc::SyncSender<()>,
        resume: Mutex<mpsc::Receiver<()>>,
    }
    impl FormalArchiveSourceV1 for PausedSource {
        fn read_object(&self, id: &str) -> Result<Option<Vec<u8>>, FormalError> {
            // A History visit can perform additional index-source reads before
            // publishing its first marker. Observe actual completed walk work,
            // rather than guessing which source-read ordinal is a safe pause.
            let mut has_marker = false;
            if !self.paused_once.load(std::sync::atomic::Ordering::SeqCst) {
                for run in fs::read_dir(self.target.join(".formal-durable-set-staging"))
                    .map_err(|error| FormalError::Io(error.to_string()))?
                {
                    let run = run.map_err(|error| FormalError::Io(error.to_string()))?;
                    if !run.file_name().to_string_lossy().starts_with("run-") {
                        continue;
                    }
                    for marker in fs::read_dir(run.path())
                        .map_err(|error| FormalError::Io(error.to_string()))?
                    {
                        let marker = marker.map_err(|error| FormalError::Io(error.to_string()))?;
                        let name = marker.file_name();
                        let name = name.to_string_lossy();
                        if ["edge-", "object-", "entry-"]
                            .iter()
                            .any(|prefix| name.starts_with(*prefix))
                        {
                            has_marker = true;
                            break;
                        }
                    }
                    if has_marker {
                        break;
                    }
                }
            }
            if has_marker
                && !self
                    .paused_once
                    .swap(true, std::sync::atomic::Ordering::SeqCst)
            {
                self.paused
                    .send(())
                    .map_err(|error| FormalError::Io(error.to_string()))?;
                self.resume
                    .lock()
                    .unwrap()
                    .recv_timeout(Duration::from_secs(15))
                    .map_err(|error| FormalError::Io(error.to_string()))?;
            }
            self.source.read_object(id)
        }
    }
    let fixture = Fixture::new();
    let mut coordinator = fixture.coordinator();
    coordinator.submit(fixture.initial.clone()).unwrap();
    let primary = coordinator.endpoints_mut().0;
    let checkpoint = primary.checkpoint().unwrap();
    primary.seal_epoch(&h(210), 2, &checkpoint).unwrap();
    let native = Store::read_native_checkpoint(&fixture.pdir).unwrap();
    let target = target(&fixture);
    let source = FileFormalArchiveSourceV1::new(fixture.pdir.clone()).unwrap();
    let copied = super::super::store::durable_set::copy_authenticated_closure(
        &target,
        &native,
        &public(5),
        &source,
        1,
        64 * 1024 * 1024,
        true,
    )
    .unwrap();
    let (paused_tx, paused_rx) = mpsc::sync_channel(1);
    let (resume_tx, resume_rx) = mpsc::sync_channel(1);
    let paused = PausedSource {
        source,
        target: target.clone(),
        paused_once: std::sync::atomic::AtomicBool::new(false),
        paused: paused_tx,
        resume: Mutex::new(resume_rx),
    };
    let first_target = target.clone();
    let first_native = native.clone();
    let first = std::thread::spawn(move || {
        super::super::store::durable_set::copy_authenticated_closure(
            &first_target,
            &first_native,
            &public(5),
            &paused,
            1,
            64 * 1024 * 1024,
            true,
        )
    });
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        paused_rx.recv_timeout(Duration::from_secs(10)).unwrap();
        let staging = target.join(".formal-durable-set-staging");
        let runs = || {
            fs::read_dir(&staging)
                .unwrap()
                .map(|entry| entry.unwrap().path())
                .filter(|path| {
                    path.file_name()
                        .unwrap()
                        .to_string_lossy()
                        .starts_with("run-")
                })
                .collect::<Vec<_>>()
        };
        let paths = runs();
        assert_eq!(paths.len(), 1);
        let live_run = &paths[0];
        assert!(
            fs::read_dir(live_run).unwrap().count() > 1,
            "pause after actual walk markers were written"
        );
        let local = FileFormalArchiveSourceV1::new(target.clone()).unwrap();
        let second = super::super::store::durable_set::copy_authenticated_closure(
            &target,
            &native,
            &public(5),
            &local,
            1,
            64 * 1024 * 1024,
            false,
        )
        .unwrap();
        assert_eq!(second, copied);
        assert!(
            live_run.exists(),
            "another complete walk must never reclaim a live run"
        );
        assert_eq!(runs(), paths);
    }));
    let _ = resume_tx.send(());
    let first = first.join().unwrap();
    if let Err(error) = outcome {
        std::panic::resume_unwind(error);
    }
    assert_eq!(first.unwrap(), copied);
}
