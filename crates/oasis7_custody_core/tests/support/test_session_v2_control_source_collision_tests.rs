use super::super::*;
#[test]
fn session_v2_control_restore_source_id_cannot_alias_a_real_control_native_lifetime_nonce() {
    let mut f = claimed();
    let request = q(&f, "collision-control-A", SessionControlKindV1::Initialize);
    challenge(&mut f, &request, SessionPurposeV1::Execution, 70, 13_000);
    let raw = serde_json::to_value(&f.state).unwrap();
    let collision = raw["test_sessions_v2"][&f.session_id]["control_records"]["ctrl-challenge-70"]
        ["observation"]["lifetime"]["lifetime"]["status"]["nonce"]
        .as_str()
        .unwrap()
        .to_owned();
    let nonce = oasis7_custody_core::approval::bytes_digest(b"source-B-independent-proof-nonce");
    let before = f.state.clone();
    assert!(
        f.state
            .reserve_test_session_v2_source(
                &f.session_id,
                SessionSourceReservation {
                    operation_id: &collision,
                    nonce: &nonce,
                    funding: SessionSourceFundingV1::Control
                },
                UnixMillis::new(14_000)
            )
            .is_err()
    );
    assert_eq!(f.state, before);
    f.state
        .reserve_test_session_v2_source(
            &f.session_id,
            SessionSourceReservation {
                operation_id: "source-B-reserved",
                nonce: &nonce,
                funding: SessionSourceFundingV1::Control,
            },
            UnixMillis::new(14_000),
        )
        .unwrap();
    let mut raw = serde_json::to_value(&f.state).unwrap();
    assert!(serde_json::from_value::<AccountProvisionApprovalState>(raw.clone()).is_ok());
    let s = &mut raw["test_sessions_v2"][&f.session_id];
    let mut moved = s["sources"]
        .as_object_mut()
        .unwrap()
        .remove("source-B-reserved")
        .unwrap();
    moved["source"]["operation_id"] = serde_json::json!(collision);
    s["sources"][&collision] = moved;
    for map in ["source_funding", "source_oracle_reads"] {
        let value = s[map]
            .as_object_mut()
            .unwrap()
            .remove("source-B-reserved")
            .unwrap();
        s[map][&collision] = value;
    }
    let mut renamed = 0;
    for event in s["events"].as_array_mut().unwrap() {
        if event["event"]["kind"] == "source_reserved"
            && event["event"]["source_operation_id"] == "source-B-reserved"
        {
            event["event"]["source_operation_id"] = serde_json::json!(collision);
            renamed += 1;
        }
    }
    assert_eq!(renamed, 1);
    // A's real fixed-Root LF signature and all source/transport charges stay exact.
    assert!(
        serde_json::from_value::<AccountProvisionApprovalState>(raw).is_err(),
        "source key must preserve the live cross-namespace guard on cold restore"
    );
}
