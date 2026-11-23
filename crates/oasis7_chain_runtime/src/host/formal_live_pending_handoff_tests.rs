use super::*;

struct Directory(PathBuf);

impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().canonicalize().unwrap().join(format!(
            "oasis7-formal-pending-{}",
            crate::node_lifecycle::local_io::random_id().unwrap()
        ));
        crate::node_lifecycle::local_io::private_dir(&path).unwrap();
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn identity(nonce: &str) -> LocalRequestIdentity {
    LocalRequestIdentity {
        verified_subject: "verified-test-owner".into(),
        operation_domain: "world-service".into(),
        nonce_scope: "original-owner-scope".into(),
        request_id: nonce.into(),
    }
}

#[test]
fn freeze_retains_only_successful_enqueue_and_reopens_the_exact_immutable_set() {
    let directory = Directory::new();
    let queue = FormalPendingHandoffQueue::open(directory.path(), "authority-a").unwrap();
    let original = vec![7, 11, 19];
    let accepted = identity("17");
    queue
        .admit(accepted.clone(), original.clone(), || Ok(()))
        .unwrap();
    queue
        .admit(accepted.clone(), original.clone(), || {
            panic!("duplicate enqueue")
        })
        .unwrap();
    assert!(
        queue
            .admit(accepted.clone(), vec![9], || panic!("conflicting enqueue"))
            .is_err()
    );
    assert!(
        queue
            .admit(identity("18"), vec![5], || Err(
                "actual Node enqueue failed".into()
            ))
            .is_err()
    );
    let expected = RecoveryPendingHandoffV1::from_requests(vec![RecoveryPendingRequestV1 {
        identity: accepted,
        signed_request: original,
    }])
    .unwrap();
    assert_eq!(queue.freeze().unwrap(), expected);
    assert_eq!(queue.freeze().unwrap(), expected);
    assert!(
        queue
            .admit(identity("19"), vec![8], || panic!("frozen owner enqueue"))
            .is_err()
    );
    assert!(queue.ensure_new_frozen_allowed().is_err());
    drop(queue);
    let reopened = FormalPendingHandoffQueue::open(directory.path(), "authority-a").unwrap();
    assert!(reopened.is_frozen().unwrap());
    assert_eq!(reopened.freeze().unwrap(), expected);
    assert!(FormalPendingHandoffQueue::open(directory.path(), "authority-b").is_err());
    let path = reopened.export_path().to_path_buf();
    let saved = fs::read(&path).unwrap();
    fs::remove_file(&path).unwrap();
    assert!(
        FormalPendingHandoffQueue::open(directory.path(), "authority-a")
            .err()
            .unwrap()
            .contains("missing its complete original")
    );
    publish_immutable_bytes(&path, b"damaged original", MAX_EXPORT_BYTES).unwrap();
    assert!(FormalPendingHandoffQueue::open(directory.path(), "authority-a").is_err());
    fs::remove_file(&path).unwrap();
    publish_immutable_bytes(&path, &saved, MAX_EXPORT_BYTES).unwrap();
    assert_eq!(
        FormalPendingHandoffQueue::open(directory.path(), "authority-a")
            .unwrap()
            .freeze()
            .unwrap(),
        expected
    );
}

#[test]
fn capacity_exhaustion_keeps_complete_originals_and_blocks_new_admission() {
    let directory = Directory::new();
    let queue = FormalPendingHandoffQueue::open(directory.path(), "authority-a").unwrap();
    let original = RecoveryPendingRequestV1 {
        identity: identity("1"),
        signed_request: vec![42],
    };
    queue
        .admit(
            original.identity.clone(),
            original.signed_request.clone(),
            || Ok(()),
        )
        .unwrap();
    let too_large = vec![1; 16 * 1024 * 1024];
    assert!(
        queue
            .admit(identity("2"), too_large, || panic!("over-bound enqueue"))
            .is_err()
    );
    assert!(
        queue
            .admit(identity("3"), vec![1], || panic!("halted enqueue"))
            .is_err()
    );
    assert!(queue.ensure_new_frozen_allowed().is_err());
    assert_eq!(queue.freeze().unwrap().requests, vec![original]);
}
