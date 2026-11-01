use super::*;
use oasis7_agent_api::world_action::MaterialLedgerId;
use serde_json::{Value, json};
use std::collections::BTreeMap;
fn rows(value: MaterialRecordRefV1<'_>) -> BTreeMap<&'static str, Value> {
    let mut rows = BTreeMap::new();
    visit_material_record_v1(value, |cell, payload| -> Result<(), ()> {
        let value = match payload {
            MemberPayloadRefV1::Unit => json!({"unit":true}),
            MemberPayloadRefV1::Text(v) => json!({"text":v}),
            MemberPayloadRefV1::U64(v) => json!({"u64":v}),
            MemberPayloadRefV1::I64(v) => json!({"i64":v}),
            MemberPayloadRefV1::Bool(v) => json!({"bool":v}),
            MemberPayloadRefV1::Ledger(v) => match v {
                MaterialLedgerId::World => json!({"ledger":"world"}),
                MaterialLedgerId::Agent(id) => json!({"ledger":"agent","id":id}),
                MaterialLedgerId::Site(id) => json!({"ledger":"site","id":id}),
                MaterialLedgerId::Factory(id) => json!({"ledger":"factory","id":id}),
            },
            _ => panic!("unexpected profile codec"),
        };
        assert!(rows.insert(cell.name(), value).is_none());
        Ok(())
    })
    .unwrap();
    rows
}
#[test]
fn profile_original_columns_and_closed_enums_have_independent_literal_oracle() {
    for (loss, encoded_loss) in [
        (MaterialTransportLossClass::Low, 0),
        (MaterialTransportLossClass::Medium, 1),
        (MaterialTransportLossClass::High, 2),
    ] {
        for (priority, encoded_priority) in [
            (MaterialDefaultPriority::Urgent, 0),
            (MaterialDefaultPriority::Standard, 1),
        ] {
            let source = MaterialProfileV1 {
                kind: " raw-kind\0界 ".into(),
                tier: u8::MAX,
                category: String::new(),
                stack_limit: i64::MIN,
                transport_loss_class: loss,
                decay_bps_per_tick: i64::MAX,
                default_priority: priority,
            };
            let actual = rows(MaterialRecordRefV1::Profile(&source));
            let expected: BTreeMap<&str, Value> = BTreeMap::from([
                ("record", json!({"unit":true})),
                ("kind", json!({"text":" raw-kind\0界 "})),
                ("tier", json!({"u64":255})),
                ("category", json!({"text":""})),
                ("stack_limit", json!({"i64":i64::MIN})),
                ("transport_loss_class", json!({"u64":encoded_loss})),
                ("decay_bps_per_tick", json!({"i64":i64::MAX})),
                ("default_priority", json!({"u64":encoded_priority})),
            ]);
            assert_eq!(actual, expected);
            visit_material_record_v1(
                MaterialRecordRefV1::Profile(&source),
                |cell, payload| -> Result<(), ()> {
                    if cell == MaterialCellV1::Kind {
                        let MemberPayloadRefV1::Text(text) = payload else {
                            panic!("original text")
                        };
                        assert_eq!(text.as_ptr(), source.kind.as_ptr());
                    }
                    Ok(())
                },
            )
            .unwrap();
        }
    }
}
#[test]
fn transfer_original_ledger_identity_and_option_are_not_normalized() {
    for route in [None, Some(String::new()), Some(" raw-route\0界 ".into())] {
        for (priority, encoded_priority) in [
            (MaterialTransitPriority::Urgent, 0),
            (MaterialTransitPriority::Standard, 1),
        ] {
            let source = MaterialTransferReceiptV1 {
                transfer_id: u64::MAX,
                requester_agent_id: "requester\0界".into(),
                from_ledger: MaterialLedgerId::Agent(String::new()),
                to_ledger: MaterialLedgerId::Factory(" factory\0界 ".into()),
                kind: String::new(),
                amount: i64::MIN,
                distance_km: i64::MAX,
                priority,
                route_id: route.clone(),
            };
            let actual = rows(MaterialRecordRefV1::Transfer(&source));
            let mut expected = BTreeMap::from([
                ("record", json!({"unit":true})),
                ("transfer_id", json!({"u64":u64::MAX})),
                ("requester_agent_id", json!({"text":"requester\0界"})),
                ("from_ledger", json!({"ledger":"agent","id":""})),
                (
                    "to_ledger",
                    json!({"ledger":"factory","id":" factory\0界 "}),
                ),
                ("kind", json!({"text":""})),
                ("amount", json!({"i64":i64::MIN})),
                ("distance_km", json!({"i64":i64::MAX})),
                ("priority", json!({"u64":encoded_priority})),
                ("route_present", json!({"bool":route.is_some()})),
            ]);
            if let Some(text) = &route {
                expected.insert("route_id", json!({"text":text}));
            }
            assert_eq!(actual, expected);
        }
    }
}
