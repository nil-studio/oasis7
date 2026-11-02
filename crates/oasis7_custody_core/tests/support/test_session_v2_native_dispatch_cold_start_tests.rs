//! Paid history is not the live first-dispatch handle.
use super::*;
#[test]
fn session_v2_native45_cold_paid_history_cannot_rebuild_first_dispatch() {
    let (mut ready, request, dispatch) = gated();
    drop(dispatch);
    let cold: AccountProvisionApprovalState =
        serde_json::from_value(serde_json::to_value(&ready.f.state).unwrap()).unwrap();
    assert_eq!(cold, ready.f.state);
    ready.f.state = cold;
    let before = ready.f.state.clone();
    assert!(
        rejected(
            &mut ready,
            &request,
            ExecutionPreparePhysicalEdgeV2::AccountToIdentityService,
            oasis7_custody_core::http::Role::Owner
        ),
        "paid reservation and restored metadata cannot reconstruct the first non-Serde dispatch handle"
    );
    assert_eq!(before, ready.f.state);
}
