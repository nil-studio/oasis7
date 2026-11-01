//! Core private2 paid originals only; no Native/Actor I/O qualification.
use super::*;
#[test]
fn session_v2_native_prepare_restore_source_key_cannot_alias_paid_private_current_port() {
    let (mut r, paid) = ready_new();
    let observed = private(&mut r, &paid);
    assert_eq!(
        observed.source_operation_id(),
        "actual-prepare-factory-source"
    );
    let original_value = serde_json::to_value(&r.f.state).unwrap();
    let factory: oasis7_custody_core::provisioning::ExecutionPrepareFactoryTransportV2 =
        serde_json::from_value(
            original_value["test_sessions_v2"][&r.f.session_id]["purpose_transports"][&paid]
                .clone(),
        )
        .unwrap();
    let read = factory.private_current.as_ref().unwrap();
    let collision = read.ports[0].physical_id.clone();
    let ns = scope(&r.f.pin);
    let (_, _, pin, floor) = issuer(
        AccountMachinePurposeV2::ExecutionPrivateCurrentV2,
        ns.http_origin,
    );
    read.proof
        .as_ref()
        .unwrap()
        .verify_issuer(&pin, &floor, read.claimed_at.get())
        .unwrap();
    r.f.state
        .test_session_v2_prepare_private_issuer_history(&r.f.session_id, &pin, &floor)
        .unwrap();
    let marker: oasis7_custody_core::provisioning::ExecutionPrivateCurrentTransportV2 =
        serde_json::from_value(
            original_value["test_sessions_v2"][&r.f.session_id]["purpose_transports"][&collision]
                .clone(),
        )
        .unwrap();
    assert_eq!(marker.factory_key, paid);
    assert_eq!(marker.hop, read.ports[0].hop);
    let honest: AccountProvisionApprovalState = serde_json::from_value(original_value).unwrap();
    assert_eq!(
        honest, r.f.state,
        "real private2 signed proof/paid marker must cold restore"
    );
    let nonce = oasis7_custody_core::approval::bytes_digest(b"private2-independent-source-B-nonce");
    let before = r.f.state.clone();
    assert!(
        r.f.state
            .reserve_test_session_v2_source(
                &r.f.session_id,
                SessionSourceReservation {
                    operation_id: &collision,
                    nonce: &nonce,
                    funding: SessionSourceFundingV1::Control
                },
                UnixMillis::new(14_012)
            )
            .is_err()
    );
    assert_eq!(
        before, r.f.state,
        "live occupied private2 physical ID rejects with wholeStateEq"
    );
    let original = "private2-source-B-reserved";
    r.f.state
        .reserve_test_session_v2_source(
            &r.f.session_id,
            SessionSourceReservation {
                operation_id: original,
                nonce: &nonce,
                funding: SessionSourceFundingV1::Control,
            },
            UnixMillis::new(14_012),
        )
        .unwrap();
    let mut raw = serde_json::to_value(&r.f.state).unwrap();
    let honest: AccountProvisionApprovalState = serde_json::from_value(raw.clone()).unwrap();
    assert_eq!(
        honest, r.f.state,
        "the independent actual Source B is valid and paid"
    );
    let s = &mut raw["test_sessions_v2"][&r.f.session_id];
    let book = s["execution_book"].clone();
    let transports = s["purpose_transports"].clone();
    let roots = s["roots"].clone();
    let mut source = s["sources"]
        .as_object_mut()
        .unwrap()
        .remove(original)
        .unwrap();
    source["source"]["operation_id"] = serde_json::json!(collision.as_str());
    s["sources"][&collision] = source;
    for map in ["source_funding", "source_oracle_reads"] {
        let value = s[map].as_object_mut().unwrap().remove(original).unwrap();
        s[map][&collision] = value;
    }
    let mut renamed = 0;
    for event in s["events"].as_array_mut().unwrap() {
        if event["event"]["kind"] == "source_reserved"
            && event["event"]["source_operation_id"] == original
        {
            event["event"]["source_operation_id"] = serde_json::json!(collision.as_str());
            renamed += 1;
        }
    }
    assert_eq!(
        renamed, 1,
        "the original paid Source event and ordinal remain fully linked"
    );
    assert_eq!(s["execution_book"], book);
    assert_eq!(
        s["purpose_transports"], transports,
        "the actual signed private2 proof/physical leaves do not change"
    );
    assert_eq!(
        s["roots"], roots,
        "budgets, generations and revocation state remain equal"
    );
    assert!(
        serde_json::from_value::<AccountProvisionApprovalState>(raw).is_err(),
        "cold Source key must preserve the live paid private2 physical ID namespace"
    );
}
