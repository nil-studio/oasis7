//! Shared original profile fixtures for industry and release record graphs.
use oasis7_wasm_abi::*;
fn reference(id: &str) -> SubstitutionProfileRefV1 {
    SubstitutionProfileRefV1 {
        id: id.into(),
        version: "original-version".into(),
        digest: format!("original-{id}-digest"),
    }
}
pub(crate) fn product_profile() -> ProductProfileV1 {
    ProductProfileV1 {
        product_id: "inner-product-id".into(),
        role_tag: "role".into(),
        maintenance_sink: vec![
            MaterialStack::new("material", i64::MAX),
            MaterialStack::new("material", i64::MAX),
        ],
        tradable: false,
        unlock_stage: "unlock".into(),
    }
}
pub(crate) fn recipe_profile() -> RecipeProfileV1 {
    RecipeProfileV1 {
        recipe_id: "inner-recipe-id".into(),
        substitution_rules: vec![rule()],
        bottleneck_tags: vec!["ordered".into(), "ordered".into()],
        stage_gate: "stage".into(),
        preferred_factory_tags: vec!["factory-tag".into()],
    }
}
pub(crate) fn factory_profile() -> FactoryProfileV1 {
    FactoryProfileV1 {
        factory_id: "inner-factory-id".into(),
        tier: u8::MAX,
        recipe_slots: u16::MAX,
        tags: vec!["tag".into(), "tag".into()],
    }
}
fn rule() -> SubstitutionRuleV1 {
    let impacts = SubstitutionImpactV1 {
        outputs: reference("outputs"),
        quality: reference("quality"),
        power: reference("power"),
        time: reference("time"),
        byproducts: reference("byproducts"),
        terminal: reference("terminal"),
        value_class: reference("value"),
        logistics: reference("logistics"),
        buffer: reference("buffer"),
    };
    SubstitutionRuleV1 {
        rule: reference("rule"),
        recipe: reference("recipe"),
        input_edge: "input".into(),
        required_material: reference("required"),
        allowed_materials: vec![reference("allowed"), reference("allowed")],
        accepted_specs: vec!["spec".into()],
        accepted_quality: vec![],
        accepted_custody: vec!["custody".into()],
        owner_ledger_path_policy: reference("ledger"),
        source_unit: "source".into(),
        target_unit: "target".into(),
        ratio_numerator: u64::MAX,
        ratio_denominator: 1,
        executable_target_quantum: 1,
        rounding: SubstitutionRoundingV1::Ceil,
        residual: SubstitutionResidualV1::Retain,
        aggregation_domain: reference("aggregation"),
        source_loss_numerator: 0,
        source_loss_denominator: 1,
        impacts,
        conversion: SubstitutionConversionV1::Required {
            converter: reference("converter"),
        },
        applicable_input_states: vec!["active".into(), "active".into()],
        disposition_policy: reference("disposition"),
        recheck_policy: reference("recheck"),
    }
}
