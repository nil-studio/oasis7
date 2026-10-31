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

#[derive(schemars::JsonSchema)]
#[serde(untagged)]
pub enum ActionSelectionSchemaV2 {
    ExactMove0(super::ExactMoveSelectionV2),
    Resource(ResourceSelectionSchemaV1),
}
