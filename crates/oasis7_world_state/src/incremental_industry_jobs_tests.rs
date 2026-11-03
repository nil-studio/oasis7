use super::*;
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};
fn recipe() -> RecipeJobState {
    serde_json::from_value(json!({
 "industrial_root_ref":{"world_binding":" w/界 ","domain":"d","schema":"s","id":"embedded-ref","revision":u64::MAX,"digest":"original-NON-normalized"},
 "emergency_power_world_binding":" raw/Ω ",
 "committed_recipe_origin":{"submission":{"verified_player_id":" p ","public_key":"RawKey","auth_nonce":u64::MAX,"hosted_registration_nonce":" raw nonce ","requester_agent_id":"submission-agent","factory_id":"submission-factory","recipe_id":"submission-recipe"},"consensus_action_id":u64::MAX,"consensus_submitter_player_id":"consensus-player","action_payload_hash":"RawHash","committed_height":u64::MAX,"action_root":"RawRoot"},
 "job_id":17,"requester_agent_id":"agent","factory_id":"factory","recipe_id":"recipe","accepted_batches":u32::MAX,
 "consume":[{"kind":"ore","amount":i64::MIN}],"produce":[],"byproducts":[],"power_required":i64::MIN,"power_owner_agent_id":" payer ","duration_ticks":u32::MAX,"consume_ledger":"world","output_ledger":"world","bottleneck_tags":["dup","dup"],"logistics_route_ids":[],"logistics_path_ids":[],"ready_at":u64::MAX
})).unwrap()
}
fn rendered(payload: MemberPayloadRefV1<'_>) -> String {
    match payload {
        MemberPayloadRefV1::Unit => "unit".into(),
        MemberPayloadRefV1::U64(n) => format!("u:{n}"),
        MemberPayloadRefV1::I64(n) => format!("i:{n}"),
        MemberPayloadRefV1::Bool(v) => format!("b:{v}"),
        MemberPayloadRefV1::Text(v) => format!("t:{v}"),
        MemberPayloadRefV1::Ledger(v) => format!("ledger:{v:?}"),
        _ => panic!("unexpected aggregated payload"),
    }
}
fn collected(record: IndustryRecordRefV1<'_>) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    visit_industry_record_v1(record, |cell, payload| -> Result<(), MemberRecordError> {
        let path = match cell {
            IndustryCellV1::Record => "unit".into(),
            IndustryCellV1::Column(c) => {
                let (section, name) = c.path();
                format!("{}.{}", section.name(), name)
            }
            IndustryCellV1::Collection(c) => format!("record.{}[]", c.name()),
            IndustryCellV1::TextItem(c, i) => format!("record.{}[{i}]", c.name()),
            IndustryCellV1::StackItem(c, i, p) => format!(
                "record.{}[{i}].{}",
                c.name(),
                match p {
                    IndustryStackColumnV1::Kind => "kind",
                    IndustryStackColumnV1::Amount => "amount",
                }
            ),
            IndustryCellV1::Balance(c, key) => format!("record.{}[{key}]", c.name()),
        };
        assert!(out.insert(path, rendered(payload)).is_none());
        Ok(())
    })
    .unwrap();
    out
}
#[test]
fn industry_borrowed_identity_and_collection_oracle_is_lossless() {
    let recipe = recipe();
    let actual = collected(IndustryRecordRefV1::Recipe(&recipe));
    // 39 fixed/empty markers plus two stack columns and two distinct Vec positions.
    assert_eq!(actual.len(), 43);
    for (key, value) in [
        ("record.job_id", "u:17"),
        ("record.ready_at", "u:18446744073709551615"),
        ("record.accepted_batches", "u:4294967295"),
        ("record.power_required", "i:-9223372036854775808"),
        ("industrial_root_ref.world_binding", "t: w/界 "),
        ("industrial_root_ref.id", "t:embedded-ref"),
        ("industrial_root_ref.revision", "u:18446744073709551615"),
        ("industrial_root_ref.digest", "t:original-NON-normalized"),
        ("committed_recipe_origin.action_payload_hash", "t:RawHash"),
        ("committed_recipe_origin.action_root", "t:RawRoot"),
        ("submission.public_key", "t:RawKey"),
        ("submission.requester_agent_id", "t:submission-agent"),
        ("submission.hosted_registration_nonce", "t: raw nonce "),
        ("record.consume[]", "u:1"),
        ("record.consume[0].kind", "t:ore"),
        ("record.consume[0].amount", "i:-9223372036854775808"),
        ("record.produce[]", "u:0"),
        ("record.bottleneck_tags[0]", "t:dup"),
        ("record.bottleneck_tags[1]", "t:dup"),
    ] {
        assert_eq!(actual.get(key).map(String::as_str), Some(value), "{key}");
    }
    let mut absent = recipe;
    absent.industrial_root_ref = None;
    absent.committed_recipe_origin = None;
    absent.emergency_power_world_binding = None;
    let absent = collected(IndustryRecordRefV1::Recipe(&absent));
    assert_eq!(absent["record.industrial_root_ref"], "b:false");
    assert_eq!(absent["record.committed_recipe_origin"], "b:false");
    for (key, value) in &absent {
        if key.starts_with("industrial_root_ref.")
            || key.starts_with("committed_recipe_origin.")
            || key.starts_with("submission.")
        {
            assert_eq!(value, "unit", "{key}");
        }
    }
}
#[test]
fn industry_five_families_use_original_registered_names_and_distinct_schemas() {
    let expected = [
        (
            "pending_recipe_jobs",
            oasis7_formal_protocol::incremental_state::StateNamespaceV1::Scheduler,
        ),
        (
            "pending_material_transits",
            oasis7_formal_protocol::incremental_state::StateNamespaceV1::Scheduler,
        ),
        (
            "recipe_completion_receipts",
            oasis7_formal_protocol::incremental_state::StateNamespaceV1::Authorization,
        ),
        (
            "factory_production_failure_dispositions",
            oasis7_formal_protocol::incremental_state::StateNamespaceV1::Authorization,
        ),
        (
            "logistics_settlement_receipts",
            oasis7_formal_protocol::incremental_state::StateNamespaceV1::Authorization,
        ),
    ];
    let mut schemas = BTreeSet::new();
    for (kind, (name, ns)) in IndustryRecordKindV1::ALL.into_iter().zip(expected) {
        let key = IndustryRecordKeyV1 { kind, id: u64::MAX };
        let field = key.descriptor().unwrap();
        assert_eq!(field.name, name);
        assert_eq!(key.namespace(), ns);
        assert!(schemas.insert(kind.schema()));
    }
}
