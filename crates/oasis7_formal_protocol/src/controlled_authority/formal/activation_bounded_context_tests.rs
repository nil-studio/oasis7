//! Context DATA/domain equivalence, not acceptance or a live namespace fixture.
use super::*;
use crate::controlled_authority::formal::{
    bounded_context::FormalBoundedWorkV1, test_fixtures::incremental,
};
use oasis7_proto::memory_budget::MemoryBudget;

fn original_tuple_digest(authority: &VerifiedAuthorityV1) -> String {
    let old = hash(
        "oasis7.formal.verified_authority_context.v1",
        &(
            &authority.definition,
            &authority.policy,
            &authority.activation_ref,
            authority.first_normal_position,
            &authority.recovery_cut,
            &authority.recovery_retained_record,
            &authority.initial_input_digest,
        ),
    )
    .unwrap();
    match &authority.durable_set_activation_digest {
        None => old,
        Some(digest) => hash(
            "oasis7.formal.verified_authority_context.v2",
            &(old, digest),
        )
        .unwrap(),
    }
}

#[test]
fn paid_context_matches_original_tuple_v1_and_v2_data_and_releases_fee() {
    let setup = incremental::setup();
    for v2 in [false, true] {
        let mut authority = setup.authority.clone();
        // Only a digest data variant; not a reverified durable activation.
        if v2 {
            authority.durable_set_activation_digest = Some("de".repeat(32));
        }
        let memory = MemoryBudget::new(4 << 20, 0).unwrap();
        let work = FormalBoundedWorkV1::new(100_000, 4 << 20).unwrap();
        let mut no = || false;
        let mut ctx = FormalBoundedContextV1::new(&memory, &work, &mut no);
        let paid = authority.context_digest_bounded(&mut ctx).unwrap();
        let expected = original_tuple_digest(&authority);
        assert_eq!(*paid, expected);
        assert_eq!(authority.context_digest().unwrap(), expected);
        assert_eq!(memory.used(), paid.reserved_bytes());
        let binding = crate::controlled_authority::formal::incremental::authority_binding_bounded(
            &authority, &mut ctx,
        )
        .unwrap();
        assert_eq!(hex::encode(binding.token), expected);
        assert_eq!(binding.epoch, authority.definition.epoch);
        drop(paid);
        assert_eq!(memory.used(), 0);
    }
}

#[test]
fn context_hash_cancel_and_zero_memory_return_no_owned_digest() {
    let setup = incremental::setup();
    for stopped in [false, true] {
        let memory = MemoryBudget::new(if stopped { 4 << 20 } else { 0 }, 0).unwrap();
        let work = FormalBoundedWorkV1::new(100_000, 4 << 20).unwrap();
        let mut stop = || stopped;
        let mut ctx = FormalBoundedContextV1::new(&memory, &work, &mut stop);
        assert!(setup.authority.context_digest_bounded(&mut ctx).is_err());
        assert_eq!(memory.used(), 0);
    }
}
