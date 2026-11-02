//! Test-only schema projection of SDK wire. No authority or runtime conversion.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
#[derive(JsonSchema, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum ResourceLedgerSchemaV1 {
    World,
    Agent { agent_id: String },
    Site { site_id: String },
    Factory { factory_id: String },
}
#[derive(JsonSchema, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum ResourceRoutingSchemaV1 {
    Direct,
    Routes { route_ids: Vec<String> },
}
#[derive(JsonSchema, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum ResourceSelectionSchemaV1 {
    Transfer {
        from: ResourceLedgerSchemaV1,
        to: ResourceLedgerSchemaV1,
        material: String,
        #[schemars(regex(pattern = "^(0|[1-9][0-9]*|-[1-9][0-9]*)$"))]
        amount: String,
        #[schemars(regex(pattern = "^(0|[1-9][0-9]*|-[1-9][0-9]*)$"))]
        distance_km: String,
        priority: oasis7_agent_api::world_action::MaterialTransitPriority,
        routing: ResourceRoutingSchemaV1,
    },
    Build {
        site: String,
        factory: String,
        factory_kind: String,
    },
    Schedule {
        factory: String,
        recipe: String,
        batches: u32,
    },
}

#[test]
fn resource_v2_schema_projection_preserves_actual_sdk_selection_wire() {
    use oasis7_client_api::world_service::resource_basis as w;
    for selection in [
        w::ResourceBasisSelectionV1::Transfer {
            from: w::ResourceLedgerV1::Agent {
                agent_id: "a".into(),
            },
            to: w::ResourceLedgerV1::Site {
                site_id: "s".into(),
            },
            material: "ore".into(),
            amount: w::ResourceDecimalI64V1(9_007_199_254_740_993),
            distance_km: w::ResourceDecimalI64V1(0),
            priority: oasis7_agent_api::world_action::MaterialTransitPriority::Standard,
            routing: w::ResourceRoutingV1::Routes {
                route_ids: vec!["route".into()],
            },
        },
        w::ResourceBasisSelectionV1::Build {
            site: "s".into(),
            factory: "f".into(),
            factory_kind: "factory.smelter.mk1".into(),
        },
        w::ResourceBasisSelectionV1::Schedule {
            factory: "f".into(),
            recipe: "recipe.smelter.iron_ingot".into(),
            batches: 1,
        },
    ] {
        let bytes = serde_json::to_vec(&selection).unwrap();
        let projection: ResourceSelectionSchemaV1 = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(serde_json::to_vec(&projection).unwrap(), bytes);
    }
}

#[derive(schemars::JsonSchema, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ActionSelectionSchemaV2 {
    ExactMove0(super::ExactMoveSelectionV2),
    Resource(ResourceSelectionSchemaV1),
}

#[test]
fn resource_v2_action_schema_projection_preserves_both_actual_selection_json_branches() {
    use super::{ApprovedActionSelectionV2, ExactMoveSelectionV2};
    use oasis7_client_api::world_service::resource_basis as wire;
    // The real production wrapper owns JSON/CBOR behavior. This test-only
    // schema facade is checked only for exact JSON parity and closedness.
    for selected in [
        ApprovedActionSelectionV2::ExactMove0(ExactMoveSelectionV2::Move {
            position_cm: [
                crate::approval::DecimalI64(i64::MIN),
                crate::approval::DecimalI64(0),
                crate::approval::DecimalI64(i64::MAX),
            ],
        }),
        ApprovedActionSelectionV2::Resource(wire::ResourceBasisSelectionV1::Build {
            site: "site".into(),
            factory: "factory".into(),
            factory_kind: "factory.smelter.mk1".into(),
        }),
    ] {
        selected.validate().unwrap();
        let bytes = serde_json::to_vec(&selected).unwrap();
        let projection: ActionSelectionSchemaV2 = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(serde_json::to_vec(&projection).unwrap(), bytes);
        let actual: ApprovedActionSelectionV2 =
            serde_json::from_slice(&serde_json::to_vec(&projection).unwrap()).unwrap();
        assert_eq!(actual, selected);
        let mut unknown = serde_json::to_value(&selected).unwrap();
        unknown
            .as_object_mut()
            .unwrap()
            .insert("verified".into(), serde_json::json!(true));
        assert!(serde_json::from_value::<ActionSelectionSchemaV2>(unknown.clone()).is_err());
        assert!(serde_json::from_value::<ApprovedActionSelectionV2>(unknown).is_err());
    }
}
