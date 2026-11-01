use super::*;
use crate::{MaterialLedgerId, MaterialTransitPriority};
use std::collections::BTreeMap;
fn collect(record: LogisticsRecordRefV1<'_>) -> BTreeMap<String, String> {
    let mut rows = BTreeMap::new();
    visit_logistics_record_v1(record, |cell, payload| -> Result<(), MemberRecordError> {
        let key = match cell {
            LogisticsCellV1::Record => "unit".into(),
            LogisticsCellV1::Column(c) => c.path().1.into(),
            LogisticsCellV1::Collection(c) => format!("{}[]", c.name()),
            LogisticsCellV1::TextItem(c, i) => format!("{}[{i}]", c.name()),
        };
        let value = match payload {
            MemberPayloadRefV1::Unit => "unit".into(),
            MemberPayloadRefV1::Text(s) => format!("text:{s}"),
            MemberPayloadRefV1::I64(n) => format!("signed:{n}"),
            MemberPayloadRefV1::U64(n) => format!("unsigned:{n}"),
            MemberPayloadRefV1::Bool(b) => format!("bool:{b}"),
            MemberPayloadRefV1::Ledger(v) => format!("ledger:{v:?}"),
            _ => panic!("unexpected aggregate"),
        };
        assert!(rows.insert(key, value).is_none());
        Ok(())
    })
    .unwrap();
    rows
}
#[test]
fn original_route_and_path_fields_are_borrowed_without_normalization() {
    let route = LogisticsRouteV1 {
        route_id: " embedded/路线 ".into(),
        from_ledger: MaterialLedgerId::World,
        to_ledger: MaterialLedgerId::Factory(" ID/厂 ".into()),
        kind: " Ore ".into(),
        distance_km: i64::MIN,
        priority: MaterialTransitPriority::Urgent,
        owner_agent_id: " owner ".into(),
        available: false,
        capacity_units: i64::MAX,
        reserved_capacity_units: i64::MIN,
        tariff_electricity_per_unit: -1,
    };
    let rows = collect(LogisticsRecordRefV1::Route(&route));
    assert_eq!(rows.len(), 12);
    for (key, value) in [
        ("route_id", "text: embedded/路线 "),
        ("kind", "text: Ore "),
        ("priority", "unsigned:1"),
        ("distance_km", "signed:-9223372036854775808"),
        ("capacity_units", "signed:9223372036854775807"),
        ("available", "bool:false"),
    ] {
        assert_eq!(rows[key], value);
    }
    let mut path = LogisticsPathAuthorityV1 {
        path_id: " path/Ω ".into(),
        route_ids: vec!["dup".into(), "dup".into(), "".into()],
        from_ledger: MaterialLedgerId::World,
        to_ledger: MaterialLedgerId::World,
        kind: " original ".into(),
        settled_amount: i64::MIN,
        remaining_recipe_amount: i64::MAX,
    };
    let rows = collect(LogisticsRecordRefV1::Path(&path));
    assert_eq!(rows.len(), 11);
    assert_eq!(rows["route_ids[]"], "unsigned:3");
    assert_eq!(rows["route_ids[0]"], "text:dup");
    assert_eq!(rows["route_ids[1]"], "text:dup");
    assert_eq!(rows["route_ids[2]"], "text:");
    assert_eq!(rows["path_id"], "text: path/Ω ");
    path.route_ids.clear();
    assert_eq!(
        collect(LogisticsRecordRefV1::Path(&path))["route_ids[]"],
        "unsigned:0"
    );
    assert_ne!(
        LogisticsRecordKindV1::Route.schema(),
        LogisticsRecordKindV1::Path.schema()
    );
    assert_eq!(
        LogisticsRecordKindV1::Route.descriptor().unwrap().name,
        "logistics_routes"
    );
    assert_eq!(
        LogisticsRecordKindV1::Path.descriptor().unwrap().name,
        "completed_logistics_paths"
    );
}

#[test]
fn relative_key_whitelist_preserves_original_vector_indices_and_rejects_other_shapes() {
    use std::io::Write;
    fn text(bytes: &mut Vec<u8>, name: &str) {
        bytes.push(1);
        bytes.extend_from_slice(&(name.len() as u64).to_be_bytes());
        bytes.extend_from_slice(name.as_bytes());
    }
    // Independent literal tuple encoding: (((Unit,"record"),"route_ids"),U64).
    let mut bytes = b"oasis7.iws.semantic-key.v1\0".to_vec();
    bytes.extend_from_slice(&[5, 5, 5, 0]);
    text(&mut bytes, "record");
    text(&mut bytes, "route_ids");
    bytes.push(2);
    bytes.extend_from_slice(&u64::MAX.to_be_bytes());
    let mut decoder = key_read::CellKeyDecoder::new();
    decoder.write_all(&bytes).unwrap();
    let parsed = decoder.finish().unwrap();
    assert!(matches!(parsed, key_read::ParsedCell::Text(u64::MAX)));
    assert!(parsed.validate(LogisticsRecordKindV1::Path).is_ok());
    assert!(parsed.validate(LogisticsRecordKindV1::Route).is_err());
    bytes.push(0);
    let mut decoder = key_read::CellKeyDecoder::new();
    decoder.write_all(&bytes).unwrap();
    assert!(decoder.finish().is_err());
    let mut unknown = b"oasis7.iws.semantic-key.v1\0".to_vec();
    unknown.extend_from_slice(&[5, 5, 0]);
    text(&mut unknown, "record");
    text(&mut unknown, "future_unregistered_column");
    let mut decoder = key_read::CellKeyDecoder::new();
    decoder.write_all(&unknown).unwrap();
    assert!(decoder.finish().is_err());
}
