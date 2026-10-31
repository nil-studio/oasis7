use super::*;
use std::collections::BTreeMap;
#[test]
fn original_intent_and_location_fields_are_complete_and_lossless() {
    let intent = AgentIntentV2 {
        schema_version: u32::MAX,
        agent_id: " embedded agent ".into(),
        intent_id: " embedded intent ".into(),
        kind: " Original Kind ".into(),
        summary: " 原summary ".into(),
        target_id: None,
        effect_intent_id: Some(String::new()),
        intent_tick: Some(0),
        world_id: Some(" raw world ".into()),
        reorg_epoch: Some(u64::MAX),
        authority_scope: None,
        status: " arbitrary terminal text ".into(),
        source: " legacy source ".into(),
        logical_time: u64::MAX,
        event_seq: 0,
        updated_at: u64::MAX,
        receipt_ref: Some(String::new()),
        reason_code: None,
        reason_summary: Some(" raw reason ".into()),
        replaced_by: None,
        actor_id: " actor ".into(),
        request_digest: " OriginalDigest ".into(),
    };
    let mut rows = BTreeMap::new();
    visit_agent_authority_record_v1(
        AgentAuthorityRecordRefV1::IntentLedger(&intent),
        |cell, payload| -> Result<(), MemberRecordError> {
            let name = match cell {
                AgentAuthorityCellV1::Record => "unit",
                AgentAuthorityCellV1::Column(c) => c.path().1,
            };
            let value = match payload {
                MemberPayloadRefV1::Unit => "none/unit".to_owned(),
                MemberPayloadRefV1::Text(v) => format!("text:{v}"),
                MemberPayloadRefV1::U64(v) => format!("unsigned:{v}"),
                _ => panic!("unexpected profile"),
            };
            assert!(rows.insert(name, value).is_none());
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(rows.len(), 23);
    for (key, value) in [
        ("schema_version", "unsigned:4294967295"),
        ("target_id", "none/unit"),
        ("effect_intent_id", "text:"),
        ("intent_tick", "unsigned:0"),
        ("reorg_epoch", "unsigned:18446744073709551615"),
        ("receipt_ref", "text:"),
        ("agent_id", "text: embedded agent "),
        ("intent_id", "text: embedded intent "),
        ("status", "text: arbitrary terminal text "),
        ("request_digest", "text: OriginalDigest "),
    ] {
        assert_eq!(rows[key], value);
    }
    let location = AgentLocationAuthorityV1 {
        agent_id: " raw agent ".into(),
        location_id: " raw location ".into(),
        active: false,
        authority_revision: 0,
        effective_at: u64::MAX,
    };
    let anchor = LocationAnchorV1 {
        location_id: " different location ".into(),
        active: false,
        authority_revision: 0,
        effective_at: 0,
    };
    for (record, count) in [
        (AgentAuthorityRecordRefV1::AgentLocation(&location), 6),
        (AgentAuthorityRecordRefV1::LocationAnchor(&anchor), 5),
    ] {
        let mut seen = 0;
        visit_agent_authority_record_v1(record, |cell, _| -> Result<(), MemberRecordError> {
            valid_cell(record.kind().sample(), cell)?;
            seen += 1;
            Ok(())
        })
        .unwrap();
        assert_eq!(seen, count);
    }
    assert_eq!(
        AgentAuthorityRecordKindV1::IntentLedger.namespace(),
        oasis7_formal_protocol::incremental_state::StateNamespaceV1::Authorization
    );
    assert!(
        valid_cell(
            AgentAuthorityRecordKindV1::LocationAnchor.sample(),
            AgentAuthorityCellV1::Column(AgentAuthorityColumnV1::AgentId)
        )
        .is_err()
    );
    assert_ne!(
        AgentAuthorityRecordKindV1::AgentLocation.schema(),
        AgentAuthorityRecordKindV1::LocationAnchor.schema()
    );
}
