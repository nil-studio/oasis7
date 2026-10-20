use super::*;
use std::sync::Mutex;

use crate::formal_read_fixtures as fixtures;

#[test]
fn historical_native_restore_selects_only_the_exact_unique_recovery_permission() {
    let (_, _, _, mut proof) = fixtures::actual_world_initial_fixture();
    let first = fixtures::h(70);
    let selected = fixtures::h(71);
    assert_eq!(
        historical_recovery_permission_index(&proof, [first.as_str()].into_iter()).unwrap(),
        None
    );
    proof.decision.branch = FormalCompletionBranchV1::ReplayFrozen;
    assert!(historical_recovery_permission_index(&proof, [].into_iter()).is_err());
    proof.decision.primary.body.recovery_certificate_digest = Some(selected.clone());
    proof.decision.replica.body.recovery_certificate_digest = Some(first.clone());
    assert!(historical_recovery_permission_index(&proof, [selected.as_str()].into_iter()).is_err());
    proof.decision.replica.body.recovery_certificate_digest = Some(selected.clone());
    assert!(historical_recovery_permission_index(&proof, [first.as_str()].into_iter()).is_err());
    assert_eq!(
        historical_recovery_permission_index(
            &proof,
            [first.as_str(), selected.as_str()].into_iter()
        )
        .unwrap(),
        Some(1)
    );
    assert!(
        historical_recovery_permission_index(
            &proof,
            [selected.as_str(), selected.as_str()].into_iter()
        )
        .is_err()
    );
    proof.decision.branch = FormalCompletionBranchV1::Normal;
    assert!(historical_recovery_permission_index(&proof, [selected.as_str()].into_iter()).is_err());
}

#[test]
fn native_resolver_preserves_content_cache_hooks_without_endpoint_io() {
    use crate::formal_live_runtime::RemoteBusinessIndexReader;
    let (_, _, anchor, _) = fixtures::actual_world_initial_fixture();
    let address = "127.0.0.1:1".parse().unwrap();
    let endpoint = |role| {
        RemoteFormalEndpoint::new(
            address,
            std::time::Duration::from_secs(1),
            anchor.authority().clone(),
            role,
            fixtures::key(4),
            anchor.head().head(),
        )
        .unwrap()
    };
    let reader = Arc::new(RemoteBusinessIndexReader::new(
        endpoint(EndpointRole::Primary),
        endpoint(EndpointRole::Replica),
        1024 * 1024,
        16,
    ));
    let resolver = NativeHistoryResolver {
        addresses: [address; 2],
        timeout: std::time::Duration::from_secs(1),
        authentication: fixtures::key(4),
        external_origin: anchor.clone(),
        trusted: anchor.clone(),
        permission: None,
        active: anchor.authority().clone(),
        permissions: Vec::new(),
        release: ReleaseSecurityPolicy::default(),
        business_reader: reader,
        writer_directory: PathBuf::new(),
        origin_history_directory: None,
        activation_markers:
            super::super::durable_set_activation::DurableSetActivationMarkerStoreV1::new(
                PathBuf::new(),
            ),
        prewarm_scope: Arc::new(()),
    };
    let bytes = BusinessIndexNodeV1::branch(0, "0".repeat(64), "1".repeat(64))
        .unwrap()
        .encode()
        .unwrap();
    let digest = hex::encode(blake3::hash(&bytes).as_bytes());
    assert_eq!(resolver.read_cached_node(&digest).unwrap(), None);
    resolver.cache_content_node(&digest, &bytes).unwrap();
    assert_eq!(
        resolver.read_cached_node(&digest).unwrap(),
        Some(bytes.clone())
    );
    assert_eq!(resolver.read_node(&digest).unwrap(), Some(bytes.clone()));
    assert!(resolver.cache_content_node(&digest, b"corrupt").is_err());
    assert_eq!(resolver.read_cached_node(&digest).unwrap(), Some(bytes));
}

#[derive(Default)]
struct CacheRecorder {
    writes: Mutex<Vec<BusinessIndexNodeWriteV1>>,
}

impl BusinessIndexNodeReader for CacheRecorder {
    fn read_node(&self, _digest: &str) -> Result<Option<Vec<u8>>, String> {
        Ok(None)
    }

    fn cache_content_node(&self, digest: &str, bytes: &[u8]) -> Result<(), String> {
        self.writes
            .lock()
            .map_err(|_| "cache recorder poisoned")?
            .push(BusinessIndexNodeWriteV1 {
                digest: digest.to_owned(),
                bytes: bytes.to_vec(),
            });
        Ok(())
    }
}

fn staged_node() -> StartupBusinessNodeStage {
    let bytes = BusinessIndexNodeV1::branch(0, "0".repeat(64), "1".repeat(64))
        .unwrap()
        .encode()
        .unwrap();
    let digest = hex::encode(blake3::hash(&bytes).as_bytes());
    let mut staged = StartupBusinessNodeStage::default();
    staged
        .insert(BusinessIndexNodeWriteV1 { digest, bytes })
        .unwrap();
    staged
}

fn finish_hint(
    replayed: Result<super::super::ReplayedHistoryCut, super::super::ReplayFailure>,
    reader: &CacheRecorder,
) -> Result<Option<VerifiedFormalReadAnchorV1>, String> {
    finish_prewarm_result(
        replayed,
        staged_node(),
        None,
        true,
        |_| Ok(true),
        std::time::Instant::now() + std::time::Duration::from_secs(1),
        reader,
    )
}

#[test]
fn missing_partial_prefix_discards_already_staged_business_bytes() {
    let reader = CacheRecorder::default();
    let completed = finish_hint(
        Err(super::super::ReplayFailure::LocalPublicationMissing),
        &reader,
    )
    .unwrap();
    assert!(
        completed.is_none(),
        "partial replay must not issue a reusable completion"
    );
    assert!(reader.writes.lock().unwrap().is_empty());
}

#[test]
fn mismatching_protected_head_discards_staged_bytes_as_optional_warmup_miss() {
    let reader = CacheRecorder::default();
    assert!(
        finish_hint(Err(super::super::ReplayFailure::TargetMismatch), &reader)
            .unwrap()
            .is_none()
    );
    assert!(reader.writes.lock().unwrap().is_empty());
}

#[test]
fn corrupt_proof_error_is_not_converted_to_an_optional_warmup_miss() {
    let reader = CacheRecorder::default();
    let result = finish_hint(
        Err(super::super::ReplayFailure::Invalid(
            "HistoryUnavailable: invalid protected proof".into(),
        )),
        &reader,
    );
    assert!(result.is_err());
    assert!(reader.writes.lock().unwrap().is_empty());
}

#[test]
fn exact_head_with_different_verified_context_does_not_seed() {
    let (_world, _trust, anchor, _) = fixtures::actual_world_initial_fixture();
    let replayed = super::super::ReplayedHistoryCut {
        anchor,
        target_proof: None,
        target_recovery: None,
        activation: None,
        activation_predecessor: None,
        activation_step: None,
    };
    let reader = CacheRecorder::default();
    let completed = finish_prewarm_result(
        Ok(replayed),
        staged_node(),
        None,
        true,
        |_| Ok(false),
        std::time::Instant::now() + std::time::Duration::from_secs(1),
        &reader,
    )
    .unwrap();
    assert!(completed.is_none());
    assert!(reader.writes.lock().unwrap().is_empty());
}

fn prewarm_resolver(
    anchor: &VerifiedFormalReadAnchorV1,
    reader: Arc<CacheRecorder>,
) -> NativeHistoryResolver {
    NativeHistoryResolver {
        addresses: ["127.0.0.1:1".parse().unwrap(); 2],
        timeout: Duration::from_secs(1),
        authentication: fixtures::key(4),
        external_origin: anchor.clone(),
        trusted: anchor.clone(),
        permission: None,
        active: anchor.authority().clone(),
        permissions: Vec::new(),
        release: ReleaseSecurityPolicy::default(),
        business_reader: reader,
        writer_directory: PathBuf::new(),
        origin_history_directory: None,
        activation_markers:
            super::super::durable_set_activation::DurableSetActivationMarkerStoreV1::new(
                PathBuf::new(),
            ),
        prewarm_scope: Arc::new(()),
    }
}

#[test]
fn completed_prewarm_receipt_reuses_only_the_exact_same_open() {
    let (world, _, origin, _) = fixtures::actual_world_initial_fixture();
    let (_, target, proof) = fixtures::actual_world_successor_fixture(&origin, world);
    let reader = Arc::new(CacheRecorder::default());
    let resolver = prewarm_resolver(&origin, reader.clone());
    let target_ref = FormalReadAnchorRefV1 {
        position: target.head().record().position,
        record_id: target.head().head().record_id,
        active_authority_id: target.authority().authority_id().unwrap(),
    };
    let mut proofs_read = 0;
    let deadline = Instant::now() + Duration::from_secs(2);
    let replayed = resolver.replay_exact_origin_for_prewarm(
        &target_ref,
        super::super::ReplayTargetPolicy::ProtectedHeadHint,
        deadline,
        |_, _, _, _| {
            proofs_read += 1;
            Ok(super::super::ReplayStepProof {
                proof: proof.clone(),
                candidate: None,
            })
        },
        |_, _, _, _| Ok(()),
    );
    let completed = finish_prewarm_result(
        replayed,
        staged_node(),
        None,
        true,
        |anchor| replayed_context_matches_hint(&resolver, anchor, &target_ref),
        deadline,
        reader.as_ref(),
    )
    .unwrap()
    .expect("the complete external-origin replay must be installable");
    assert_eq!(proofs_read, 1);
    assert_eq!(reader.writes.lock().unwrap().len(), 1);
    let receipt =
        VerifiedBusinessPrewarmReceipt::from_completed(&resolver, &completed, reader.as_ref())
            .unwrap();
    assert!(
        resolver
            .prewarm_qualified_business_history_after_hint(&target, reader.as_ref(), receipt)
            .unwrap(),
        "the exact restored writer must reuse its completed same-open warmup"
    );
    assert_eq!(reader.writes.lock().unwrap().len(), 1);
}

#[test]
fn prewarm_receipt_binds_full_checkpoint_context_resolver_and_reader() {
    let (world, _, origin, _) = fixtures::actual_world_initial_fixture();
    let (_, target, _) = fixtures::actual_world_successor_fixture(&origin, world);
    let reader = Arc::new(CacheRecorder::default());
    let resolver = prewarm_resolver(&origin, reader.clone());
    let receipt = || {
        VerifiedBusinessPrewarmReceipt::from_completed(&resolver, &target, reader.as_ref())
            .unwrap()
            .unwrap()
    };
    assert!(
        receipt()
            .matches(&resolver, &target, reader.as_ref())
            .unwrap()
    );
    assert!(
        !receipt()
            .matches(&resolver, &origin, reader.as_ref())
            .unwrap()
    );

    assert!(
        !resolver
            .prewarm_qualified_business_history_after_hint(
                &origin,
                reader.as_ref(),
                Some(receipt()),
            )
            .unwrap(),
        "a different exact writer target must use the ordinary replay path"
    );
    let another_open = prewarm_resolver(&origin, reader.clone());
    assert!(
        !receipt()
            .matches(&another_open, &target, reader.as_ref())
            .unwrap()
    );
    assert!(
        !another_open
            .prewarm_qualified_business_history_after_hint(
                &target,
                reader.as_ref(),
                Some(receipt()),
            )
            .unwrap(),
        "a fresh constructor cannot inherit a prior constructor's completion"
    );
    let another_reader = CacheRecorder::default();
    assert!(
        !receipt()
            .matches(&resolver, &target, &another_reader)
            .unwrap()
    );
    assert!(
        VerifiedBusinessPrewarmReceipt::from_completed(&resolver, &target, &another_reader)
            .unwrap()
            .is_none()
    );

    // These preserve the short (position, record ID, authority ID) selector.
    // They must still miss: those three fields are not the opaque trust context.
    let mut changed = receipt();
    changed.target.checkpoint.retained_record_id = Some(fixtures::h(90));
    assert!(
        !changed
            .matches(&resolver, &target, reader.as_ref())
            .unwrap()
    );
    let mut changed = receipt();
    changed.target.authority_context = fixtures::h(91);
    assert!(
        !changed
            .matches(&resolver, &target, reader.as_ref())
            .unwrap()
    );
    let mut changed = receipt();
    changed.external_origin.authority_context = fixtures::h(92);
    assert!(
        !changed
            .matches(&resolver, &target, reader.as_ref())
            .unwrap()
    );
    let mut changed = receipt();
    changed.trusted.checkpoint.first_normal_position = Some(99);
    assert!(
        !changed
            .matches(&resolver, &target, reader.as_ref())
            .unwrap()
    );
    let mut changed = receipt();
    changed.active_context = fixtures::h(93);
    assert!(
        !changed
            .matches(&resolver, &target, reader.as_ref())
            .unwrap()
    );
}

#[test]
fn expired_or_failed_cache_installation_cannot_complete_prewarm() {
    let (_, _, anchor, _) = fixtures::actual_world_initial_fixture();
    let replayed = || super::super::ReplayedHistoryCut {
        anchor: anchor.clone(),
        target_proof: None,
        target_recovery: None,
        activation: None,
        activation_predecessor: None,
        activation_step: None,
    };
    let reader = CacheRecorder::default();
    assert!(
        finish_prewarm_result(
            Ok(replayed()),
            staged_node(),
            None,
            true,
            |_| Ok(true),
            Instant::now(),
            &reader,
        )
        .unwrap()
        .is_none()
    );
    assert!(reader.writes.lock().unwrap().is_empty());

    struct FailingCache;
    impl BusinessIndexNodeReader for FailingCache {
        fn read_node(&self, _: &str) -> Result<Option<Vec<u8>>, String> {
            Ok(None)
        }
        fn cache_content_node(&self, _: &str, _: &[u8]) -> Result<(), String> {
            Err("injected cache installation failure".into())
        }
    }
    assert!(
        finish_prewarm_result(
            Ok(replayed()),
            staged_node(),
            None,
            true,
            |_| Ok(true),
            Instant::now() + Duration::from_secs(1),
            &FailingCache,
        )
        .unwrap_err()
        .contains("injected cache installation failure")
    );
}

#[test]
fn cache_installation_crossing_deadline_cannot_issue_prewarm_receipt() {
    use std::sync::atomic::{AtomicBool, Ordering};

    struct SlowCache {
        deadline: Instant,
        installed: AtomicBool,
    }
    impl BusinessIndexNodeReader for SlowCache {
        fn read_node(&self, _: &str) -> Result<Option<Vec<u8>>, String> {
            Ok(None)
        }
        fn cache_content_node(&self, _: &str, _: &[u8]) -> Result<(), String> {
            self.installed.store(true, Ordering::SeqCst);
            std::thread::sleep(
                self.deadline.saturating_duration_since(Instant::now()) + Duration::from_millis(1),
            );
            Ok(())
        }
    }
    let (_, _, anchor, _) = fixtures::actual_world_initial_fixture();
    let replayed = super::super::ReplayedHistoryCut {
        anchor,
        target_proof: None,
        target_recovery: None,
        activation: None,
        activation_predecessor: None,
        activation_step: None,
    };
    let deadline = Instant::now() + Duration::from_secs(1);
    let reader = SlowCache {
        deadline,
        installed: AtomicBool::new(false),
    };
    assert!(
        finish_prewarm_result(
            Ok(replayed),
            staged_node(),
            None,
            true,
            |_| Ok(true),
            deadline,
            &reader,
        )
        .unwrap()
        .is_none(),
        "cache installation must finish inside the original prewarm budget"
    );
    assert!(
        reader.installed.load(Ordering::SeqCst),
        "the regression must cross its deadline during actual cache installation"
    );
}
