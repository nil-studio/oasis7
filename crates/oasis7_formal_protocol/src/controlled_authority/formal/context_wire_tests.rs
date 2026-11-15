use super::*;
use crate::controlled_authority::formal::bounded_context::FormalBoundedWorkV1;
use oasis7_proto::memory_budget::MemoryBudget;

#[test]
fn bridge_preserves_wire_and_debits_the_callers_existing_meter() {
    let memory = MemoryBudget::new(4096, 0).unwrap();
    let work = FormalBoundedWorkV1::new(1000, 4096).unwrap();
    let mut stop = || false;
    let mut ctx = FormalBoundedContextV1::new(&memory, &work, &mut stop);
    let value = ("wire", vec![1u64, 2, 3]);
    let expected = serde_cbor::to_vec(&value).unwrap();
    let output = encode_paid(&value, 4096, &mut ctx, "wire limit", "encoding").unwrap();
    assert_eq!(&**output, &expected);
    assert!(output.uses_budget(&memory));
    assert_eq!(
        work.remaining_read_bytes(),
        4096 - 2 * expected.len() as u64
    );
    assert!(work.remaining_steps() < 999);
    assert!(!work.is_child_active());
    assert!(memory.used() > 0);
    drop(output);
    assert_eq!(memory.used(), 0);
}

#[test]
fn bridge_preserves_first_failure_and_releases_lease_without_refund() {
    let memory = MemoryBudget::new(4096, 0).unwrap();
    let work = FormalBoundedWorkV1::new(1000, 4096).unwrap();
    let mut stop = || false;
    let mut ctx = FormalBoundedContextV1::new(&memory, &work, &mut stop);
    let error = encode_paid(&42u64, 0, &mut ctx, "wire limit", "encoding").unwrap_err();
    assert!(matches!(error, FormalError::Invalid(message) if message == "wire limit"));
    assert!(!work.is_child_active());
    assert!(work.remaining_steps() < 1000);
    assert_eq!(memory.used(), 0);
    ctx.consume_steps(1).unwrap();
}

#[test]
fn bridge_cancellation_callback_cannot_reenter_parent_meter() {
    let memory = MemoryBudget::new(4096, 0).unwrap();
    let work = FormalBoundedWorkV1::new(1000, 4096).unwrap();
    let mut calls = 0;
    let mut stop = || {
        calls += 1;
        if work.is_child_active() {
            assert!(work.charge(1, 1, &mut || false).is_err());
            true
        } else {
            false
        }
    };
    let mut ctx = FormalBoundedContextV1::new(&memory, &work, &mut stop);
    let error = encode_paid(&42u64, 4096, &mut ctx, "wire limit", "encoding").unwrap_err();
    assert!(
        matches!(error, FormalError::Invalid(message) if message == "bounded archive read cancelled")
    );
    assert!(!work.is_child_active());
    assert_eq!(
        (work.remaining_steps(), work.remaining_read_bytes()),
        (999, 4096)
    );
    assert_eq!(memory.used(), 0);
    assert!(calls >= 2);
}
