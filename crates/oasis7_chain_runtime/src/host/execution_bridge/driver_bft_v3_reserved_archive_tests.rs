//! Actual native-provider archive propagation of original Some reservations.
use super::*;
pub(super) fn assert_original_promises_exported(
    root: &Scratch,
    store: &LocalCasStore,
    manifest: &BftSuccessorWindowArchiveManifestV1,
) {
    let directory = root
        .path()
        .join("ledger/formal-bft-header-prefix-v1/node-private-v1");
    let mut count = 0;
    for file in fs::read_dir(directory).unwrap() {
        let path = file.unwrap().path();
        if !path
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("candidate-")
        {
            continue;
        }
        let value: serde_cbor::Value = serde_cbor::from_slice(&fs::read(path).unwrap()).unwrap();
        let serde_cbor::Value::Map(fields) = value else {
            panic!("original Data map missing");
        };
        let reference = fields.get(&serde_cbor::Value::Text(
            "publication_reservation_ref".into(),
        ));
        let Some(serde_cbor::Value::Text(reference)) = reference else {
            panic!("native original candidate must retain Some reservation");
        };
        assert!(
            store
                .root()
                .join("blobs")
                .join(format!("{reference}.blob"))
                .exists()
        );
        assert!(
            manifest
                .entries
                .iter()
                .any(|entry| entry.object.digest == *reference),
            "actual successor archive omitted original publication metadata for committed T or signed T+1"
        );
        count += 1;
    }
    assert!(
        count >= 2,
        "fixture must contain both committed T and original signed T+1 Some reservations"
    );
}

pub(super) fn assert_archive_material_failure_and_exact_restore(
    root: &Scratch,
    archive: &LocalBftSuccessorArchiveStoreV1,
    manifest: &BftSuccessorWindowArchiveManifestV1,
    mut verify: impl FnMut() -> Result<(), String>,
) {
    let mut originals = Vec::new();
    for file in fs::read_dir(
        root.path()
            .join("ledger/formal-bft-header-prefix-v1/node-private-v1"),
    )
    .unwrap()
    {
        let path = file.unwrap().path();
        if !path
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("candidate-")
        {
            continue;
        }
        let value: serde_cbor::Value = serde_cbor::from_slice(&fs::read(path).unwrap()).unwrap();
        let serde_cbor::Value::Map(fields) = value else {
            panic!("original Data map missing");
        };
        let Some(serde_cbor::Value::Text(reference)) = fields.get(&serde_cbor::Value::Text(
            "publication_reservation_ref".into(),
        )) else {
            panic!("Some original missing");
        };
        let object = manifest
            .entries
            .iter()
            .find(|entry| entry.object.digest == *reference)
            .expect("original metadata must be archived");
        let bytes = archive.load_object(&object.object, 16 * 1024).unwrap();
        let path = archive
            .root()
            .join("blobs")
            .join(format!("{reference}.blob"));
        let permissions = fs::metadata(&path).unwrap().permissions();
        originals.push((
            archive
                .root()
                .join("blobs")
                .join(format!("{reference}.blob")),
            bytes,
            permissions,
        ));
    }
    assert!(originals.len() >= 2);
    verify().expect("unaltered actual archive must independently replay");
    for (path, bytes, permissions) in &originals {
        fs::remove_file(path).unwrap();
        assert!(
            verify().is_err(),
            "missing archived Some metadata cannot become legacy or another promise"
        );
        fs::write(path, bytes).unwrap();
        fs::set_permissions(path, permissions.clone()).unwrap();
        verify().expect("exact original metadata restoration must recover same private base");
    }
    assert_ne!(originals[0].1, originals[1].1);
    fs::write(&originals[0].0, &originals[1].1).unwrap();
    assert!(
        verify().is_err(),
        "another original T+1 promise cannot satisfy committed T ref"
    );
    fs::write(&originals[0].0, &originals[0].1).unwrap();
    verify().expect("restoring original T ref must recover same authenticated base");
}

/// Join while all original archive roots still exist, including an assertion unwind.
pub(super) struct ScopedArchiveServerV1 {
    stop: Arc<std::sync::atomic::AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}
impl ScopedArchiveServerV1 {
    pub(super) fn new(
        stop: Arc<std::sync::atomic::AtomicBool>,
        handle: thread::JoinHandle<()>,
    ) -> Self {
        Self {
            stop,
            handle: Some(handle),
        }
    }
    pub(super) fn join(mut self) -> thread::Result<()> {
        self.stop.store(true, std::sync::atomic::Ordering::SeqCst);
        self.handle.take().unwrap().join()
    }
}
impl Drop for ScopedArchiveServerV1 {
    fn drop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::SeqCst);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}
