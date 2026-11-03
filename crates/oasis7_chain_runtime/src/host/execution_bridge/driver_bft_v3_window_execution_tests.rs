//! Actual private World execution; provider/backend remain explicit memory fixtures.
use super::*;
use oasis7_proto::distributed_bft::v3::{
    BftHeaderCommitCertificateV3, BftProposalHeaderV3, BftVotePhaseV3, BftVoteV3, BftVoteValueV3,
};
use std::collections::BTreeMap;

pub(super) struct ActiveExerciseV1<'a> {
    pub(super) h: &'a QualifiedH<BftProfileTargetV2>,
    pub(super) root: &'a Scratch,
    pub(super) store: LocalCasStore,
    pub(super) binding: &'a BftSignerBindingV3,
    pub(super) pin: &'a str,
    pub(super) verifier: &'a ActualBftSuccessorWindowDriverVerifierV1,
    pub(super) original: &'a ValidatedBftSuccessorOriginalStateV1,
    pub(super) record: &'a BftSuccessorWindowStoredRequestV1,
    pub(super) lease: &'a mut dyn BftSignerLeaseV3,
    pub(super) pre_pin: bool,
    pub(super) archive: Arc<LocalBftSuccessorArchiveStoreV1>,
    pub(super) target_offset: u64,
    pub(super) reserved_publication: bool,
}
#[path = "driver_bft_v3_reserved_active_tests.rs"]
mod reserved;
/// Mutually exclusive test profiles do not retain the legacy fault/alias
/// exercise's large World frames while independently replaying a fresh base.
pub(super) fn exercise(mut case: ActiveExerciseV1<'_>) {
    if case.reserved_publication {
        reserved::run(&mut case);
    } else {
        exercise_legacy(
            case.h,
            case.root,
            case.store,
            case.binding,
            case.pin,
            case.verifier,
            case.original,
            case.record,
            case.lease,
            case.pre_pin,
            case.archive,
            case.target_offset,
        );
    }
}
#[allow(clippy::too_many_arguments)]
fn exercise_legacy(
    h: &QualifiedH<BftProfileTargetV2>,
    root: &Scratch,
    store: LocalCasStore,
    binding: &BftSignerBindingV3,
    pin: &str,
    verifier: &ActualBftSuccessorWindowDriverVerifierV1,
    original: &ValidatedBftSuccessorOriginalStateV1,
    record: &BftSuccessorWindowStoredRequestV1,
    lease: &mut dyn BftSignerLeaseV3,
    pre_pin: bool,
    archive: Arc<LocalBftSuccessorArchiveStoreV1>,
    target_offset: u64,
) {
    let records = root.path().join("ledger");
    let frozen = inventory(&records.join("formal-bft-header-prefix-v1"));
    let readonly = OriginalHeaderIdentityV1::open(
        &records,
        store.clone(),
        h.anchor.clone(),
        h.target.clone(),
        h.material.clone(),
        h.reader.clone(),
    )
    .unwrap();
    assert_eq!(readonly.material_identity(), binding.material_identity);
    readonly.verify_unchanged().unwrap();
    drop(readonly);
    let active = lease.verified_active_window().unwrap().unwrap();
    let installed = original
        .reconstruct_installed_pending(binding, pin, record)
        .unwrap();
    let base = verifier
        .recover_private_base(record, original, deadline())
        .unwrap();
    let mut window = FormalBftHeaderActiveExecutionV1::open(
        &records,
        store.clone(),
        base,
        &active,
        binding,
        pin,
        &installed,
        deadline(),
    )
    .unwrap();
    assert_eq!(window.material_identity(), binding.material_identity);
    let original_pending = window
        .pending_candidate()
        .unwrap()
        .expect("same original T+1 proposal must survive window installation");
    let pending_directory = records
        .join("successor-active-v1")
        .join(record.request.intent.body.checkpoint.digest().unwrap())
        .join("formal-bft-header-window-v1/node-private-v1");
    let pending_bytes = inventory(&pending_directory);
    assert_eq!(
        window
            .build_and_retain(original_pending.actions_cbor.clone(), deadline())
            .unwrap(),
        original_pending
    );
    assert_eq!(
        inventory(&pending_directory),
        pending_bytes,
        "reproducing original T+1 preserves complete candidate WAL bytes and dependency refs"
    );
    let mut other = original_pending.actions_cbor.clone();
    other.push(0);
    assert!(window.build_and_retain(other, deadline()).is_err());
    let base_height = active.minimum().height;
    for offset in 1..=4 {
        let actions = if offset == 1 {
            original_pending.actions_cbor.clone()
        } else {
            rolling::nonempty_actions(offset)
        };
        let candidate = window.build_and_retain(actions, deadline()).unwrap();
        assert_eq!(
            candidate.public_value.execution_commitment.height,
            base_height + offset
        );
        let certificate = certify_one(window.schedule(), &candidate.public_value);
        rolling::reserve_certificate(lease, &certificate);
        if offset == 1 {
            if pre_pin {
                window.inject_pre_pin_failure(candidate.public_value.execution_commitment.height);
            } else {
                window
                    .inject_publication_failure(candidate.public_value.execution_commitment.height);
            }
            assert_eq!(
                window
                    .append_certified(certificate.clone(), deadline())
                    .unwrap_err(),
                "injected original prefix publication interruption",
                "the failure must reach PinsPublished before testing cold WAL recovery"
            );
        } else {
            window
                .append_certified(certificate.clone(), deadline())
                .unwrap();
        }
        if offset == 1 {
            drop(window);
            if pre_pin {
                let reservation = records
                    .join("successor-active-v1")
                    .join(record.request.intent.body.checkpoint.digest().unwrap())
                    .join("formal-bft-header-window-v1")
                    .join(format!(
                        "reservation-{:020}.cbor",
                        certificate.execution_commitment.height
                    ));
                let original_bytes = fs::read(&reservation).unwrap();
                let mut changed = original_bytes.clone();
                changed.push(0);
                fs::write(&reservation, changed).unwrap();
                let base = verifier
                    .recover_private_base(record, original, deadline())
                    .unwrap();
                assert!(
                    FormalBftHeaderActiveExecutionV1::open(
                        &records,
                        store.clone(),
                        base,
                        &active,
                        binding,
                        pin,
                        &installed,
                        deadline()
                    )
                    .is_err(),
                    "changed original pre-pin reservation must remain blocked"
                );
                assert_eq!(
                    fs::read(&reservation).unwrap().len(),
                    original_bytes.len() + 1
                );
                fs::write(&reservation, original_bytes).unwrap();
            }
            let base = verifier
                .recover_private_base(record, original, deadline())
                .unwrap();
            window = FormalBftHeaderActiveExecutionV1::open(
                &records,
                store.clone(),
                base,
                &active,
                binding,
                pin,
                &installed,
                deadline(),
            )
            .unwrap();
            assert_eq!(
                window
                    .world()
                    .execution_state_identity()
                    .unwrap()
                    .state_root_hash,
                certificate.execution_commitment.state_root
            );
            if pre_pin {
                rolling::confirm_applied(lease, &window, &certificate);
                assert_eq!(
                    inventory(&records.join("formal-bft-header-prefix-v1")),
                    frozen
                );
                return;
            }
            let segment = records
                .join("successor-active-v1")
                .join(record.request.intent.body.checkpoint.digest().unwrap());
            for directory in [
                records.clone(),
                records.join("successor-active-v1"),
                segment.clone(),
                segment.join("formal-bft-header-window-v1"),
                segment.join("formal-bft-header-window-v1/node-private-v1"),
            ] {
                let retained = replace_parent_preserving_children(&directory);
                assert!(
                    window.build_and_retain(vec![0x80], deadline()).is_err(),
                    "online parent alias must fail despite original leaf inodes"
                );
                drop(window);
                let base = verifier
                    .recover_private_base(record, original, deadline())
                    .unwrap();
                assert!(
                    FormalBftHeaderActiveExecutionV1::open(
                        &records,
                        store.clone(),
                        base,
                        &active,
                        binding,
                        pin,
                        &installed,
                        deadline()
                    )
                    .is_err(),
                    "cold parent alias must fail against protected Origin"
                );
                restore_parent(&directory, &retained);
                let base = verifier
                    .recover_private_base(record, original, deadline())
                    .unwrap();
                window = FormalBftHeaderActiveExecutionV1::open(
                    &records,
                    store.clone(),
                    base,
                    &active,
                    binding,
                    pin,
                    &installed,
                    deadline(),
                )
                .unwrap();
            }
        }
        rolling::confirm_applied(lease, &window, &certificate);
        if offset > 1 {
            rolling::assert_nonempty(&window, offset, &certificate);
        }
    }
    rolling::continue_to_height65(
        window,
        &records,
        store,
        binding,
        pin,
        verifier,
        record.clone(),
        lease,
        archive,
        target_offset,
    );
    assert_eq!(
        inventory(&records.join("formal-bft-header-prefix-v1")),
        frozen
    );
}
#[path = "driver_bft_v3_window_rolling_tests.rs"]
mod rolling;
fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(120)
}
fn certify_one(
    schedule: &BftHeaderHeightScheduleV3,
    value: &oasis7_consensus::tendermint_v3::BftHeaderValueV3,
) -> FormalBftCertifiedHeaderV3 {
    let id = schedule.expected_proposer_for_round(0).unwrap();
    let block = value.block_id().unwrap();
    let proposal = BftProposalHeaderV3::sign(
        schedule.scope().clone(),
        schedule.height(),
        0,
        schedule.active_set().set_hash().into(),
        schedule.policy_hash().into(),
        id.clone(),
        block.clone(),
        None,
        &fixtures::key(120),
    )
    .unwrap();
    let vote = BftVoteV3::sign(
        schedule.scope().clone(),
        schedule.height(),
        0,
        BftVotePhaseV3::Precommit,
        schedule.active_set().set_hash().into(),
        schedule.policy_hash().into(),
        BftVoteValueV3::Block { block_id: block },
        id,
        &fixtures::key(120),
    )
    .unwrap();
    FormalBftCertifiedHeaderV3::new(
        BftHeaderCommitCertificateV3 {
            proposal,
            valid_round_evidence: None,
            precommits: vec![vote],
        },
        value.header.clone(),
        value.execution_commitment.clone(),
    )
    .unwrap()
}
fn inventory(root: &std::path::Path) -> BTreeMap<std::path::PathBuf, Vec<u8>> {
    fn visit(
        base: &std::path::Path,
        path: &std::path::Path,
        out: &mut BTreeMap<std::path::PathBuf, Vec<u8>>,
    ) {
        for entry in fs::read_dir(path).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                visit(base, &path, out);
            } else {
                out.insert(
                    path.strip_prefix(base).unwrap().into(),
                    fs::read(path).unwrap(),
                );
            }
        }
    }
    let mut out = BTreeMap::new();
    visit(root, root, &mut out);
    out
}

fn replace_parent_preserving_children(path: &std::path::Path) -> std::path::PathBuf {
    let retained = path.with_extension("retained-directory");
    fs::rename(path, &retained).unwrap();
    fs::create_dir(path).unwrap();
    fs::set_permissions(path, fs::metadata(&retained).unwrap().permissions()).unwrap();
    move_children_preserving_file_inodes(&retained, path);
    retained
}
fn restore_parent(path: &std::path::Path, retained: &std::path::Path) {
    move_children_preserving_file_inodes(path, retained);
    fs::remove_dir(path).unwrap();
    fs::rename(retained, path).unwrap();
}
fn move_children_preserving_file_inodes(from: &std::path::Path, to: &std::path::Path) {
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let destination = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            fs::rename(entry.path(), destination).unwrap();
        } else {
            fs::hard_link(entry.path(), &destination).unwrap();
            fs::remove_file(entry.path()).unwrap();
        }
    }
}

struct ReservedNodeOutcomeV1 {
    original: oasis7_consensus::tendermint_v3::VerifiedBftHeaderCertificateV3,
    fresh: oasis7_consensus::tendermint_v3::VerifiedBftHeaderCertificateV3,
    accounting: (usize, usize, usize, usize),
    result: NodeBftExecutionCommitResultV3,
}
fn exercise_reserved_node_consumer(
    window: Box<FormalBftHeaderActiveExecutionV1>,
    binding: &BftSignerBindingV3,
    lease: &mut dyn BftSignerLeaseV3,
    original_t1: &FormalBftCertifiedHeaderV3,
    base_schedule: &BftHeaderHeightScheduleV3,
) -> ReservedNodeOutcomeV1 {
    let schedule = window.schedule().clone();
    let mut driver = window.into_node_hook_for_test(binding).unwrap();
    driver.hook().configure_bft_v3_execution(binding).unwrap();
    let token = oasis7_consensus::tendermint_v3::verify_commit_certificate_token_v3(
        base_schedule,
        original_t1,
    )
    .unwrap();
    // Real Node cold replay preserves the original decided T+1; the new
    // publication reservation belongs only to the following fresh operation.
    driver
        .hook()
        .commit_bft_v3_certificate(token.clone())
        .unwrap();
    let original = token;
    let expected = driver
        .hook()
        .current_bft_v3_execution_context(&schedule)
        .unwrap();
    let action = oasis7_node::NodeConsensusAction::from_payload(
        93_002,
        validator_id(0),
        payload("active-node-reserved-business"),
    )
    .unwrap();
    let candidate = driver
        .hook()
        .build_bft_v3_candidate(NodeBftBuildContextV3 {
            execution: expected.clone(),
            actions: vec![action],
        })
        .unwrap();
    let receipt = driver.hook().retain_bft_v3_reserved_candidate(&expected, &candidate)
        .expect("actual ActiveNodeDriver must durably reserve fresh publication after authenticated original T+1 replay");
    assert_eq!(receipt.len(), 64);
    let reserved = driver.accounting().unwrap();
    let certificate = certify_one(&schedule, &candidate.public_value);
    rolling::reserve_certificate(lease, &certificate);
    let token = oasis7_consensus::tendermint_v3::verify_commit_certificate_token_v3(
        &schedule,
        &certificate,
    )
    .unwrap();
    let result = driver
        .hook()
        .commit_bft_v3_certificate(token.clone())
        .unwrap();
    let fresh = token;
    let before = lease.checkpoint().unwrap();
    let after = lease
        .confirm_applied(
            before.state.revision,
            &before.state.state_digest,
            result.height,
            &result.header_id,
            &result.driver_install_digest,
        )
        .unwrap();
    assert!(after.state.pending_driver_apply.is_none());
    assert_eq!(
        after
            .state
            .applied_installs
            .last()
            .unwrap()
            .driver_install_digest,
        result.driver_install_digest
    );
    let accounting = driver.accounting().unwrap();
    assert_eq!(
        reserved.0, accounting.0,
        "Active actual consumes original promise without refund or double charge"
    );
    ReservedNodeOutcomeV1 {
        original,
        fresh,
        accounting,
        result,
    }
}

/// Advance the real inherited candidate through the existing one-validator QC
/// fixture and protected append/readback path. This is setup for cold tests;
/// it does not claim service Local proposal signing coverage.
pub(super) fn commit_original_carry_for_test(
    window: &mut FormalBftHeaderActiveExecutionV1,
    lease: &mut dyn BftSignerLeaseV3,
    deadline: Instant,
) {
    let original = window
        .pending_candidate()
        .unwrap()
        .expect("original carry must be selected");
    let certificate = certify_one(window.schedule(), &original.public_value);
    rolling::reserve_certificate(lease, &certificate);
    window
        .append_certified(certificate.clone(), deadline)
        .unwrap();
    rolling::confirm_applied(lease, window, &certificate);
}
