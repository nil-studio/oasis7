//! Actual private World progression with real protected Prepare/minimum/Adopt RPCs.
//! QCs use the explicit one-validator fixture, not a Node scheduling claim.
use super::*;

pub(super) fn nonempty_actions(offset: u64) -> Vec<u8> {
    let name = format!("private-window-nonempty-{offset}");
    let action = oasis7_node::NodeConsensusAction::from_payload(
        90_000 + offset,
        validator_id(0),
        payload(&name),
    )
    .unwrap();
    serde_cbor::to_vec(&vec![action]).unwrap()
}
pub(super) fn reserve_certificate(
    lease: &mut dyn BftSignerLeaseV3,
    certificate: &FormalBftCertifiedHeaderV3,
) {
    let mut checkpoint = lease.checkpoint().unwrap();
    if let Some(intent) = checkpoint
        .state
        .journal
        .as_ref()
        .and_then(|journal| journal.pending_vote_intent())
        .cloned()
    {
        checkpoint = lease
            .sign_vote(checkpoint.state.revision, &intent)
            .unwrap()
            .1;
    }
    if checkpoint.state.journal.is_some() {
        checkpoint = lease.start_height_timer(checkpoint.state.revision).unwrap();
        let mut candidate = checkpoint.state.candidate();
        candidate.journal.as_mut().unwrap().pending_commit = Some(certificate.clone());
        checkpoint = lease
            .advance(
                checkpoint.state.revision,
                &checkpoint.state.state_digest,
                candidate,
            )
            .unwrap();
    }
    lease
        .append_committed_certificate(
            checkpoint.state.revision,
            &checkpoint.state.state_digest,
            certificate.clone(),
        )
        .unwrap();
}
pub(super) fn confirm_applied(
    lease: &mut dyn BftSignerLeaseV3,
    window: &FormalBftHeaderActiveExecutionV1,
    certificate: &FormalBftCertifiedHeaderV3,
) {
    let checkpoint = lease.checkpoint().unwrap();
    let applied = lease
        .confirm_applied(
            checkpoint.state.revision,
            &checkpoint.state.state_digest,
            certificate.execution_commitment.height,
            &certificate.header.block_id().unwrap(),
            &window.current_install_digest().unwrap(),
        )
        .unwrap();
    assert!(applied.state.pending_driver_apply.is_none());
    assert_eq!(
        window.current_result_root().unwrap(),
        certificate.execution_commitment.result_root
    );
}
pub(super) fn assert_nonempty(
    window: &FormalBftHeaderActiveExecutionV1,
    offset: u64,
    certificate: &FormalBftCertifiedHeaderV3,
) {
    assert!(
        window
            .world()
            .state()
            .agents
            .contains_key(&format!("private-window-nonempty-{offset}"))
    );
    assert_eq!(
        window.current_result_root().unwrap(),
        certificate.execution_commitment.result_root
    );
    assert_eq!(
        window
            .world()
            .execution_state_identity()
            .unwrap()
            .state_root_hash,
        certificate.execution_commitment.state_root
    );
}

#[allow(clippy::too_many_arguments)]
pub(super) fn continue_to_height65(
    mut window: FormalBftHeaderActiveExecutionV1,
    records: &std::path::Path,
    store: LocalCasStore,
    binding: &BftSignerBindingV3,
    pin: &str,
    verifier: &ActualBftSuccessorWindowDriverVerifierV1,
    mut record: BftSuccessorWindowStoredRequestV1,
    lease: &mut dyn BftSignerLeaseV3,
    archive: Arc<LocalBftSuccessorArchiveStoreV1>,
    target_offset: u64,
) {
    let mut immutable_segments = Vec::new();
    let mut last_certificate = None;
    // The first frozen T+1 may be empty. Offsets 2..=66 provide 65 distinct
    // nonempty actions, with a real successor install every four records.
    for offset in 5..=target_offset {
        if (offset - 1) % 4 == 0 {
            let segment = records
                .join("successor-active-v1")
                .join(record.request.intent.body.checkpoint.digest().unwrap());
            let exported = window
                .export_successor_private_closure(
                    &archive,
                    &BftSuccessorWindowSourceV1::from_binding(binding).unwrap(),
                    &lease.checkpoint().unwrap().state,
                    deadline(),
                )
                .unwrap();
            let current = lease.checkpoint().unwrap();
            let source = BftSuccessorWindowSourceV1::from_binding(binding).unwrap();
            let previous = &record.request.intent.body.checkpoint;
            let mut manifest = BftSuccessorWindowArchiveManifestV1 {
                schema_version: 1,
                public_index: None,
                source: source.clone(),
                kind: BftSuccessorWindowArchiveKindV1::Public,
                segment_sequence: exported.manifest().segment_sequence,
                predecessor_root: Some(previous.public_archive_root.clone()),
                first_height: exported.manifest().first_height,
                last_height: exported.manifest().last_height,
                entries: vec![BftSuccessorWindowArchiveEntryV1 {
                    identity_digest: current.state.state_digest.clone(),
                    object: exported.public_state_ref().clone(),
                }],
                usage: exported.manifest().usage,
            };
            oasis7_distfs::protected_bft_v3::index_whole_state_signed_results_v1(
                &mut manifest,
                &current.state,
            )
            .unwrap();
            let journal_ref = current.state.journal.as_ref().map(|journal| {
                archive
                    .persist_object(
                        &oasis7_cas::to_canonical_cbor(journal).unwrap(),
                        512 * 1024 * 1024,
                    )
                    .unwrap()
            });
            let body = BftSuccessorWindowCheckpointBodyV1 {
                schema_version: 1,
                source,
                request_id: format!(
                    "actual-private-window-{}",
                    exported.manifest().segment_sequence
                ),
                sequence: exported.manifest().segment_sequence,
                predecessor_checkpoint_digest: previous.digest().unwrap(),
                cut: exported.cut().clone(),
                successor_schedule_cbor: oasis7_cas::to_canonical_cbor(window.schedule()).unwrap(),
                successor_schedule_digest: window.schedule().digest().unwrap(),
                current_journal_ref: journal_ref,
                current_journal_digest: current
                    .state
                    .journal
                    .as_ref()
                    .map(|journal| journal.digest().unwrap()),
                public_archive_root: manifest.digest().unwrap(),
                execution_archive_root: exported.manifest().digest().unwrap(),
                execution_manifest_ref: exported.execution_manifest_ref().clone(),
                evidence_refs: vec![exported.metadata_ref().clone()],
                usage: exported.manifest().usage,
            };
            let digest = body.digest().unwrap();
            let id = body.request_id.clone();
            let (_, prepared) = lease
                .prepare_successor_window(
                    BftSuccessorWindowInstallIntentBodyV1 {
                        schema_version: 1,
                        checkpoint: body.clone(),
                        expected_lease_generation: current.lease_generation,
                        expected_revision: current.state.revision,
                        expected_state_digest: current.state.state_digest.clone(),
                        driver_evidence_digest: body.cut.driver_install_digest.clone(),
                    },
                    manifest,
                )
                .unwrap();
            assert_eq!(prepared.state.active_window, current.state.active_window);
            let (_, committed) = lease.commit_successor_window(&id, &digest).unwrap();
            assert_eq!(
                committed.state.successor_window.as_ref().unwrap().minimum,
                Some(body.minimum().unwrap())
            );
            let evidence = window
                .install_successor_window(
                    binding,
                    pin,
                    lease,
                    archive.clone(),
                    &id,
                    &digest,
                    deadline(),
                )
                .unwrap();
            assert!(evidence.active_segment_origin.is_some());
            let (adopted, rebased) = lease
                .confirm_successor_window_adopted(&id, &digest, evidence)
                .unwrap();
            assert!(rebased.state.history.is_empty());
            assert_eq!(
                rebased
                    .state
                    .active_window
                    .as_ref()
                    .unwrap()
                    .checkpoint_digest,
                digest
            );
            immutable_segments.push((segment.clone(), inventory(&segment)));
            let before = window.world().execution_state_identity().unwrap();
            drop(window);
            let original = lease
                .read_successor_original_state(&id, &digest, deadline())
                .unwrap();
            let installed = original
                .reconstruct_installed_pending(binding, pin, &adopted)
                .unwrap();
            let active = lease.verified_active_window().unwrap().unwrap();
            // A corrupted older original State must block newest private replay.
            // Restoring exactly those bytes makes the same installation usable.
            if offset == 5 {
                let path = archive
                    .root()
                    .join("blobs")
                    .join(format!("{}.blob", record.original_public_state.digest));
                let original_bytes = fs::read(&path).unwrap();
                let mut changed = original_bytes.clone();
                changed.push(0);
                fs::write(&path, changed).unwrap();
                assert!(
                    verifier
                        .recover_private_base(&adopted, &original, deadline())
                        .is_err()
                );
                assert_eq!(fs::read(&path).unwrap().len(), original_bytes.len() + 1);
                fs::write(&path, original_bytes).unwrap();
            }
            let base = verifier
                .recover_private_base(&adopted, &original, deadline())
                .unwrap();
            window = FormalBftHeaderActiveExecutionV1::open(
                records,
                store.clone(),
                base,
                &active,
                binding,
                pin,
                &installed,
                deadline(),
            )
            .unwrap();
            assert_eq!(window.world().execution_state_identity().unwrap(), before);
            for (path, bytes) in &immutable_segments {
                assert_eq!(&inventory(path), bytes);
            }
            record = adopted;
        }
        let candidate = window
            .build_and_retain(nonempty_actions(offset), deadline())
            .unwrap();
        let certificate = certify_one(window.schedule(), &candidate.public_value);
        reserve_certificate(lease, &certificate);
        window
            .append_certified(certificate.clone(), deadline())
            .unwrap();
        confirm_applied(lease, &window, &certificate);
        assert_nonempty(&window, offset, &certificate);
        last_certificate = Some(certificate);
    }
    let last = last_certificate.unwrap();
    assert!(
        last.execution_commitment.height >= binding.handoff().handoff_position() + target_offset
    );
    assert!(record.request.intent.body.checkpoint.sequence >= 2);
    let before = window.world().execution_state_identity().unwrap();
    drop(window);
    let original = lease
        .read_successor_original_state(
            &record.request.intent.body.checkpoint.request_id,
            &record.request.intent.body.checkpoint.digest().unwrap(),
            deadline(),
        )
        .unwrap();
    let installed = original
        .reconstruct_installed_pending(binding, pin, &record)
        .unwrap();
    let active = lease.verified_active_window().unwrap().unwrap();
    let base = verifier
        .recover_private_base(&record, &original, deadline())
        .unwrap();
    let reopened = FormalBftHeaderActiveExecutionV1::open(
        records,
        store,
        base,
        &active,
        binding,
        pin,
        &installed,
        deadline(),
    )
    .unwrap();
    assert_eq!(reopened.world().execution_state_identity().unwrap(), before);
    for offset in 2..=target_offset {
        assert!(
            reopened
                .world()
                .state()
                .agents
                .contains_key(&format!("private-window-nonempty-{offset}"))
        );
    }
    for (path, bytes) in immutable_segments {
        assert_eq!(inventory(&path), bytes);
    }
}
