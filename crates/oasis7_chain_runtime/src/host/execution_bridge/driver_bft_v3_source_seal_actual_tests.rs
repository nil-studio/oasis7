//! Real World/Noise Some-to-Seal path. Backend and issuer remain explicit memory doubles.
use super::*;
use std::cell::Cell;
thread_local! { static ENABLED: Cell<bool> = const { Cell::new(false) }; }
pub(super) fn requested() -> bool {
    ENABLED.with(Cell::get)
}
#[test]
fn wire3_actual_source_seal_preserves_original_some_and_t_plus_one() {
    thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(|| {
            ENABLED.with(|value| value.set(true));
            install_case_with_retention(
                false,
                false,
                66,
                CandidateRetentionProfile::PublicationReserved,
            );
        })
        .unwrap()
        .join()
        .unwrap();
}
pub(super) fn exercise(
    lease: &mut dyn BftSignerLeaseV3,
    original: &BftSignerStateV3,
    cut: oasis7_distfs::protected_bft_v3::BftSuccessorWindowCutV1,
) {
    use oasis7_distfs::protected_bft_v3::BftPrivateRetentionLifecycleV1;
    let deadline = || Instant::now() + Duration::from_secs(120);
    let before = lease.observe_private_retention(deadline()).unwrap();
    assert_eq!(&before.checkpoint().state, original);
    assert!(before.latest_seal().is_none());
    let entries = &before.ledger().map.entries;
    assert!(
        entries.iter().any(|e| e.key.height <= cut.height),
        "real committed Some absent"
    );
    let carry = entries
        .iter()
        .filter(|e| e.key.height == cut.height + 1)
        .cloned()
        .collect::<Vec<_>>();
    assert!(!carry.is_empty(), "real original T+1 promise absent");
    assert!(
        entries
            .iter()
            .all(|e| e.lifecycle == BftPrivateRetentionLifecycleV1::Ready)
    );
    let sealed = lease
        .seal_private_retention(
            original.revision,
            original.state_digest.clone(),
            cut.clone(),
            deadline(),
        )
        .unwrap();
    let after = &sealed.checkpoint().state;
    assert_eq!(after.history, original.history);
    assert_eq!(after.journal, original.journal);
    assert_eq!(after.applied_installs, original.applied_installs);
    assert_eq!(after.completed_journals, original.completed_journals);
    assert_eq!(after.revision, original.revision + 1);
    assert_ne!(after.state_digest, original.state_digest);
    assert_eq!(sealed.ledger().map.coverage_start_height, cut.height + 1);
    assert_eq!(sealed.ledger().map.entries, carry);
    let anchor = sealed
        .latest_seal()
        .expect("sealed Current must authenticate original cut");
    assert_eq!(anchor.body.cut, cut);
    assert_eq!(anchor.body.carry_entries, carry);
    let again = lease.observe_private_retention(deadline()).unwrap();
    assert_eq!(again.checkpoint().state, *after);
    assert_eq!(again.ledger(), sealed.ledger());
    assert_eq!(again.latest_seal(), sealed.latest_seal());
    // A lost caller observation may repeat the exact original pre-Seal request.
    // It must return the same protected operation, without a second transition.
    let retry = lease
        .seal_private_retention(
            original.revision,
            original.state_digest.clone(),
            anchor.body.cut.clone(),
            deadline(),
        )
        .unwrap();
    assert_eq!(retry.checkpoint().state, *after);
    assert_eq!(retry.ledger(), sealed.ledger());
    assert_eq!(retry.latest_seal(), sealed.latest_seal());
    let proof = lease
        .read_private_retention_seal_proof(anchor.body.seal_body_digest.clone(), deadline())
        .unwrap();
    assert_eq!(proof.body.original, *before.ledger());
    assert_eq!(proof.body.cut, anchor.body.cut);
    assert_eq!(proof.body.carry_map().unwrap(), sealed.ledger().map);
    assert_eq!(proof.anchor().unwrap(), *anchor);
    assert_eq!(lease.checkpoint().unwrap().state, *after);
}
