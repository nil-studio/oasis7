use super::*;
use serde_json::{Value, json};
fn rows(value: FactoryAuthorityRecordRefV1<'_>) -> Vec<Value> {
    let mut rows = Vec::new();
    visit_factory_authority_record_v1(value, |cell, payload| -> Result<(), ()> {
        let data = match payload {
            MemberPayloadRefV1::Unit => json!({"unit":true}),
            MemberPayloadRefV1::Text(v) => json!({"text":v}),
            MemberPayloadRefV1::U64(v) => json!({"u64":v}),
            MemberPayloadRefV1::I64(v) => json!({"i64":v}),
            MemberPayloadRefV1::Bool(v) => json!({"bool":v}),
            _ => panic!("unexpected field codec"),
        };
        let index = match cell {
            FactoryAuthorityCellV1::AuthorizedAgent(index) => Some(index),
            _ => None,
        };
        rows.push(json!([cell.name(), index, data]));
        Ok(())
    })
    .unwrap();
    rows
}
#[test]
fn original_site_fields_preserve_allowlist_order_duplicates_and_empty_ids() {
    let source = FactorySiteAuthorityV1 {
        site_id: " inner-site\0界 ".into(),
        location_id: String::new(),
        owner_agent_id: "owner".into(),
        authorized_agent_ids: vec!["z".into(), String::new(), "z".into()],
        chunk_ready: false,
        active: true,
        authority_revision: u64::MAX,
        registered_at: 17,
    };
    assert_eq!(
        rows(FactoryAuthorityRecordRefV1::Site(&source)),
        vec![
            json!(["record",null,{"unit":true}]),
            json!(["site_id",null,{"text":" inner-site\0界 "}]),
            json!(["location_id",null,{"text":""}]),
            json!(["owner_agent_id",null,{"text":"owner"}]),
            json!(["authorized_agent_count",null,{"u64":3}]),
            json!(["authorized_agent_ids",0,{"text":"z"}]),
            json!(["authorized_agent_ids",1,{"text":""}]),
            json!(["authorized_agent_ids",2,{"text":"z"}]),
            json!(["chunk_ready",null,{"bool":false}]),
            json!(["active",null,{"bool":true}]),
            json!(["authority_revision",null,{"u64":u64::MAX}]),
            json!(["registered_at",null,{"u64":17}]),
        ]
    );
    visit_factory_authority_record_v1(
        FactoryAuthorityRecordRefV1::Site(&source),
        |cell, payload| -> Result<(), ()> {
            if cell == FactoryAuthorityCellV1::SiteId {
                let MemberPayloadRefV1::Text(v) = payload else {
                    panic!("borrowed text")
                };
                assert_eq!(v.as_ptr(), source.site_id.as_ptr());
            }
            Ok(())
        },
    )
    .unwrap();
}
#[test]
fn original_power_option_and_signed_extremes_have_literal_oracle() {
    for (module, flag) in [(None, false), (Some(""), true), (Some(" raw\0界 "), true)] {
        let source = FactoryConstructionPowerProfileV1 {
            factory_id: "inner-factory".into(),
            factory_kind: String::new(),
            source_module_id: module.map(str::to_owned),
            electricity_amount: i64::MIN,
            mode: FactoryConstructionPowerMode::StartOnlySink,
            authority_revision: u64::MAX,
            active: false,
        };
        let mut expected = vec![
            json!(["record",null,{"unit":true}]),
            json!(["factory_id",null,{"text":"inner-factory"}]),
            json!(["factory_kind",null,{"text":""}]),
            json!(["source_module_present",null,{"bool":flag}]),
        ];
        if let Some(module) = module {
            expected.push(json!(["source_module_id",null,{"text":module}]));
        }
        expected.extend([
            json!(["electricity_amount",null,{"i64":i64::MIN}]),
            json!(["mode",null,{"u64":0}]),
            json!(["authority_revision",null,{"u64":u64::MAX}]),
            json!(["active",null,{"bool":false}]),
        ]);
        assert_eq!(rows(FactoryAuthorityRecordRefV1::Power(&source)), expected);
    }
}
