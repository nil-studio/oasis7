use super::*;
fn body() -> FinalityKeyRotationBodyV1 {
    serde_json::from_value(serde_json::json!({
            "schema_version":1,"authorization":{"schema_version":1,"world_id":"w","chain_id":"c",
            "genesis_digest":"0".repeat(64),"parent_position":1,"parent_record_id":"1".repeat(64),
            "parent_state_root":"2".repeat(64),"policy_digest":"3".repeat(64),"controller_account_id":"a",
            "nonce":1,"valid_from_position":2,"valid_through_position":3,"signatures":{}},
            "rotation_id":"r","candidate_id":"c","node_id":"n","expected_generation":1,
            "old_public_key_hex":"4".repeat(64),"new_public_key_hex":"5".repeat(64),
            "decision_height":2,"activation_height":4,"target_epoch":2
        })).unwrap()
}
#[test]
fn typed_rotation_queue_bounds_include_inflight_and_stop_cleans_replies() {
    let queue = Arc::new(RotationQueue::default());
    queue.open().unwrap();
    let deadline = Instant::now() + Duration::from_secs(60);
    let mut replies = Vec::new();
    for _ in 0..MAX_PENDING {
        let (tx, rx) = mpsc::channel();
        queue
            .enqueue(body(), deadline, Arc::new(AtomicBool::new(false)), tx)
            .unwrap();
        replies.push(rx);
    }
    let inflight = queue.inner.lock().unwrap().jobs.pop_front().unwrap();
    assert_eq!(inflight.body, RotationBody::Admitted(body()));
    let (tx, _) = mpsc::channel();
    assert!(
        queue
            .enqueue(body(), deadline, Arc::new(AtomicBool::new(false)), tx)
            .is_err()
    );
    queue.close();
    assert!(!inflight.live());
    assert_eq!(queue.outstanding.load(Ordering::Acquire), 1);
    drop(inflight);
    for rx in replies {
        assert!(rx.recv().unwrap().is_err());
    }
    assert_eq!(queue.outstanding.load(Ordering::Acquire), 0);
    queue.open().unwrap();
    let (tx, _) = mpsc::channel();
    assert!(
        queue
            .enqueue(body(), Instant::now(), Arc::new(AtomicBool::new(false)), tx)
            .is_err()
    );
    queue.close();
}
#[test]
fn conflicting_original_rotation_is_rejected_before_provider_without_stopping_queue() {
    use oasis7_distfs::protected_bft_v3::{
        BftFinalityRotationProofArchiveV3, BftFinalityRotationProofRecordV3,
    };
    let queue = Arc::new(RotationQueue::default());
    queue.open().unwrap();
    let original = body();
    for role in [
        FinalityKeyRotationRoleV1::OldKeyAuthorization,
        FinalityKeyRotationRoleV1::NewKeyPossession,
    ] {
        // This unit exercises routing only, not signature validity or custody.
        let archive = BftFinalityRotationProofArchiveV3 {
            schema_version: 1,
            records: vec![BftFinalityRotationProofRecordV3 {
                body: original.clone(),
                public_key_hex: original.old_public_key_hex.clone(),
                proof: FinalityKeyRotationProofV1 {
                    schema_version: 1,
                    role,
                    signature_hex: "0".repeat(128),
                },
                recorded_revision: 1,
            }],
        };
        let mut changed = original.clone();
        changed.candidate_id = "different-original".into();
        changed.validate().unwrap();
        let (tx, rx) = mpsc::channel();
        queue
            .enqueue(
                changed,
                Instant::now() + Duration::from_secs(60),
                Arc::new(AtomicBool::new(false)),
                tx,
            )
            .unwrap();
        let mut rejected = queue.inner.lock().unwrap().jobs.pop_front().unwrap();
        assert!(rejected.reject_conflicting_archive(Some(&archive), None));
        assert!(rx.recv().unwrap().is_err());
        drop(rejected);
        // The same production rejection seam keeps admission open and the
        // exact original request eligible. No provider call is made here.
        let (tx, _rx) = mpsc::channel();
        queue
            .enqueue(
                original.clone(),
                Instant::now() + Duration::from_secs(60),
                Arc::new(AtomicBool::new(false)),
                tx,
            )
            .unwrap();
        let mut accepted = queue.inner.lock().unwrap().jobs.pop_front().unwrap();
        assert!(!accepted.reject_conflicting_archive(Some(&archive), None));
        assert!(accepted.live());
        drop(accepted);
        assert_eq!(queue.outstanding.load(Ordering::Acquire), 0);
    }
    queue.close();
}

fn bootstrap_body() -> BootstrapFinalityKeyRotationBodyV1 {
    let mut value = serde_json::to_value(body()).unwrap();
    let object = value.as_object_mut().unwrap();
    object.remove("candidate_id");
    object.insert(
        "bootstrap_registry_digest_hex".into(),
        serde_json::json!("a".repeat(64)),
    );
    object.insert(
        "bootstrap_public_key_hex".into(),
        serde_json::json!("b".repeat(64)),
    );
    serde_json::from_value(value).unwrap()
}
#[test]
fn mixed_source_requests_share_capacity_and_inflight_stop_cancellation() {
    let queue = Arc::new(RotationQueue::default());
    queue.open().unwrap();
    let deadline = Instant::now() + Duration::from_secs(60);
    let (bootstrap_tx, bootstrap_rx) = mpsc::channel();
    queue
        .enqueue_job(
            RotationBody::Bootstrap(bootstrap_body()),
            deadline,
            Arc::new(AtomicBool::new(false)),
            RotationReply::Bootstrap(bootstrap_tx),
        )
        .unwrap();
    let inflight = queue.inner.lock().unwrap().jobs.pop_front().unwrap();
    let mut replies = Vec::new();
    for _ in 1..MAX_PENDING {
        let (tx, rx) = mpsc::channel();
        queue
            .enqueue(body(), deadline, Arc::new(AtomicBool::new(false)), tx)
            .unwrap();
        replies.push(rx);
    }
    let (tx, _) = mpsc::channel();
    assert!(
        queue
            .enqueue_job(
                RotationBody::Bootstrap(bootstrap_body()),
                deadline,
                Arc::new(AtomicBool::new(false)),
                RotationReply::Bootstrap(tx)
            )
            .is_err()
    );
    queue.close();
    assert!(!inflight.live());
    assert_eq!(queue.outstanding.load(Ordering::Acquire), 1);
    drop(inflight);
    assert!(bootstrap_rx.recv().unwrap().is_err());
    for rx in replies {
        assert!(rx.recv().unwrap().is_err());
    }
    assert_eq!(queue.outstanding.load(Ordering::Acquire), 0);
}
#[test]
fn bootstrap_rotation_id_conflicts_with_admitted_archive_before_signing() {
    use oasis7_distfs::protected_bft_v3::{
        BftFinalityRotationProofArchiveV3, BftFinalityRotationProofRecordV3,
    };
    let queue = Arc::new(RotationQueue::default());
    queue.open().unwrap();
    let b = body();
    let archive = BftFinalityRotationProofArchiveV3 {
        schema_version: 1,
        records: vec![BftFinalityRotationProofRecordV3 {
            body: b.clone(),
            public_key_hex: b.old_public_key_hex.clone(),
            proof: FinalityKeyRotationProofV1 {
                schema_version: 1,
                role: FinalityKeyRotationRoleV1::OldKeyAuthorization,
                signature_hex: "0".repeat(128),
            },
            recorded_revision: 1,
        }],
    };
    let (tx, rx) = mpsc::channel();
    queue
        .enqueue_job(
            RotationBody::Bootstrap(bootstrap_body()),
            Instant::now() + Duration::from_secs(60),
            Arc::new(AtomicBool::new(false)),
            RotationReply::Bootstrap(tx),
        )
        .unwrap();
    let mut job = queue.inner.lock().unwrap().jobs.pop_front().unwrap();
    assert!(job.reject_conflicting_archive(Some(&archive), None));
    assert!(rx.recv().unwrap().is_err());
    assert!(job.live());
    drop(job);
    queue.close();
}

#[test]
fn bootstrap_archive_preserves_original_source_and_rejects_cross_source_id() {
    use oasis7_distfs::protected_bft_v3::{
        BftBootstrapFinalityRotationProofArchiveV3, BftBootstrapFinalityRotationProofRecordV3,
    };
    let original = bootstrap_body();
    let archive = BftBootstrapFinalityRotationProofArchiveV3 {
        schema_version: 1,
        records: vec![BftBootstrapFinalityRotationProofRecordV3 {
            body: original.clone(),
            public_key_hex: original.old_public_key_hex.clone(),
            proof: BootstrapFinalityKeyRotationProofV1 {
                schema_version: 1,
                role: BootstrapFinalityKeyRotationRoleV1::OldKeyAuthorization,
                signature_hex: "0".repeat(128),
            },
            recorded_revision: 1,
        }],
    };
    let queue = Arc::new(RotationQueue::default());
    queue.open().unwrap();
    let deadline = Instant::now() + Duration::from_secs(60);
    let (tx, rx) = mpsc::channel();
    queue
        .enqueue(body(), deadline, Arc::new(AtomicBool::new(false)), tx)
        .unwrap();
    let mut admitted = queue.inner.lock().unwrap().jobs.pop_front().unwrap();
    assert!(admitted.reject_conflicting_archive(None, Some(&archive)));
    assert!(rx.recv().unwrap().is_err());
    drop(admitted);
    for changed in [false, true] {
        let mut b = original.clone();
        if changed {
            b.bootstrap_registry_digest_hex = "c".repeat(64);
        }
        let (tx, rx) = mpsc::channel();
        queue
            .enqueue_job(
                RotationBody::Bootstrap(b),
                deadline,
                Arc::new(AtomicBool::new(false)),
                RotationReply::Bootstrap(tx),
            )
            .unwrap();
        let mut job = queue.inner.lock().unwrap().jobs.pop_front().unwrap();
        assert_eq!(
            job.reject_conflicting_archive(None, Some(&archive)),
            changed
        );
        if changed {
            assert!(rx.recv().unwrap().is_err());
        }
        assert!(job.live());
        drop(job);
    }
    queue.close();
}
