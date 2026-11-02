use super::super::*;
#[test]
fn session_v2_control_journal_rejects_missing_duplicate_and_causally_reordered_events() {
    let f = claimed();
    let raw = serde_json::to_value(&f.state).unwrap();
    assert!(serde_json::from_value::<AccountProvisionApprovalState>(raw.clone()).is_ok());
    let mut absent = raw.clone();
    absent["test_sessions_v2"][&f.session_id]
        .as_object_mut()
        .unwrap()
        .remove("events")
        .unwrap();
    assert!(serde_json::from_value::<AccountProvisionApprovalState>(absent).is_err());
    let mut missing = raw.clone();
    missing["test_sessions_v2"][&f.session_id]["events"]
        .as_array_mut()
        .unwrap()
        .remove(0);
    assert!(serde_json::from_value::<AccountProvisionApprovalState>(missing).is_err());
    let mut duplicate = raw.clone();
    let events = duplicate["test_sessions_v2"][&f.session_id]["events"]
        .as_array_mut()
        .unwrap();
    events.push(events[0].clone());
    assert!(serde_json::from_value::<AccountProvisionApprovalState>(duplicate).is_err());
    // Same event inventory, original signatures, all map keys and budget values.
    // Only the trusted transaction order is impossible: consumption before claim.
    let mut reversed = raw;
    let events = reversed["test_sessions_v2"][&f.session_id]["events"]
        .as_array_mut()
        .unwrap();
    let claim = events
        .iter()
        .position(|e| e["event"]["kind"] == "approval_verification_claim")
        .unwrap();
    let consume = events
        .iter()
        .position(|e| e["event"]["kind"] == "approval_consumed")
        .unwrap();
    assert!(claim < consume);
    events.swap(claim, consume);
    assert!(serde_json::from_value::<AccountProvisionApprovalState>(reversed).is_err());
}
