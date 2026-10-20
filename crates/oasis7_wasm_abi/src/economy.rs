use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Category of economy module in M4 industrial pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EconomyModuleKind {
    Recipe,
    Product,
    Factory,
}

/// Generic stack of materials/resources used by economy modules.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MaterialStack {
    pub kind: String,
    pub amount: i64,
}

impl MaterialStack {
    pub fn new(kind: impl Into<String>, amount: i64) -> Self {
        Self {
            kind: kind.into(),
            amount,
        }
    }
}

/// Transport loss class used for material logistics weighting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MaterialTransportLossClass {
    Low,
    #[default]
    Medium,
    High,
}

/// Default logistics priority for one material kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MaterialDefaultPriority {
    Urgent,
    #[default]
    Standard,
}

/// Runtime material profile used by layered economy tuning.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MaterialProfileV1 {
    pub kind: String,
    pub tier: u8,
    pub category: String,
    pub stack_limit: i64,
    #[serde(default)]
    pub transport_loss_class: MaterialTransportLossClass,
    #[serde(default)]
    pub decay_bps_per_tick: i64,
    #[serde(default)]
    pub default_priority: MaterialDefaultPriority,
}

/// Runtime product profile used by progression and role tuning.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProductProfileV1 {
    pub product_id: String,
    pub role_tag: String,
    #[serde(default)]
    pub maintenance_sink: Vec<MaterialStack>,
    pub tradable: bool,
    #[serde(default)]
    pub unlock_stage: String,
}

/// Runtime recipe profile used by bottleneck and stage tuning.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecipeProfileV1 {
    pub recipe_id: String,
    /// Empty legacy profiles never authorize material substitution.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub substitution_rules: Vec<crate::SubstitutionRuleV1>,
    #[serde(default)]
    pub bottleneck_tags: Vec<String>,
    #[serde(default)]
    pub stage_gate: String,
    #[serde(default)]
    pub preferred_factory_tags: Vec<String>,
}

impl RecipeProfileV1 {
    pub fn validate_substitution_rules(&self) -> Result<(), crate::SubstitutionRuleError> {
        crate::validate_substitution_rules(&self.substitution_rules, &self.recipe_id)
    }
}

/// Runtime factory profile used by progression and capacity tuning.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FactoryProfileV1 {
    pub factory_id: String,
    pub tier: u8,
    pub recipe_slots: u16,
    #[serde(default)]
    pub tags: Vec<String>,
}

/// Static recipe definition emitted by a recipe module.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecipeModuleSpec {
    pub recipe_id: String,
    pub display_name: String,
    #[serde(default)]
    pub inputs: Vec<MaterialStack>,
    #[serde(default)]
    pub outputs: Vec<MaterialStack>,
    #[serde(default)]
    pub byproducts: Vec<MaterialStack>,
    pub cycle_ticks: u32,
    pub power_per_cycle: i64,
    #[serde(default)]
    pub allowed_factory_tags: Vec<String>,
    pub min_factory_tier: u8,
}

/// Static product metadata emitted by a product module.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProductModuleSpec {
    pub product_id: String,
    pub display_name: String,
    pub category: String,
    pub stack_limit: u32,
    pub decay_per_tick_bps: u32,
    #[serde(default)]
    pub quality_levels: Vec<String>,
    pub tradable: bool,
}

/// Runtime request for validating a product stack by product module.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProductValidationRequest {
    pub product_id: String,
    pub stack: MaterialStack,
    pub deterministic_seed: u64,
}

/// Runtime decision returned by product module.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProductValidationDecision {
    pub product_id: String,
    pub accepted: bool,
    #[serde(default)]
    pub notes: Vec<String>,
    pub stack_limit: u32,
    pub tradable: bool,
    #[serde(default)]
    pub quality_levels: Vec<String>,
}

impl ProductValidationDecision {
    pub fn accepted(
        product_id: impl Into<String>,
        stack_limit: u32,
        tradable: bool,
        quality_levels: Vec<String>,
    ) -> Self {
        Self {
            product_id: product_id.into(),
            accepted: true,
            notes: Vec::new(),
            stack_limit,
            tradable,
            quality_levels,
        }
    }

    pub fn rejected(
        product_id: impl Into<String>,
        stack_limit: u32,
        tradable: bool,
        quality_levels: Vec<String>,
        notes: Vec<String>,
    ) -> Self {
        Self {
            product_id: product_id.into(),
            accepted: false,
            notes,
            stack_limit,
            tradable,
            quality_levels,
        }
    }
}

/// Static factory metadata emitted by a factory module.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FactoryModuleSpec {
    pub factory_id: String,
    pub display_name: String,
    pub tier: u8,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub build_cost: Vec<MaterialStack>,
    pub build_time_ticks: u32,
    pub base_power_draw: i64,
    pub recipe_slots: u16,
    pub throughput_bps: u32,
    pub maintenance_per_tick: i64,
}

/// Runtime request for evaluating a recipe batch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecipeExecutionRequest {
    pub recipe_id: String,
    pub factory_id: String,
    pub desired_batches: u32,
    #[serde(default)]
    pub available_inputs: Vec<MaterialStack>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub available_inputs_by_ledger: Option<BTreeMap<String, Vec<MaterialStack>>>,
    pub available_power: i64,
    pub deterministic_seed: u64,
}

/// Runtime plan returned by recipe module.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecipeExecutionPlan {
    pub accepted_batches: u32,
    #[serde(default)]
    pub consume: Vec<MaterialStack>,
    #[serde(default)]
    pub produce: Vec<MaterialStack>,
    #[serde(default)]
    pub byproducts: Vec<MaterialStack>,
    pub power_required: i64,
    pub duration_ticks: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reject_reason: Option<String>,
}

impl RecipeExecutionPlan {
    pub fn accepted(
        accepted_batches: u32,
        consume: Vec<MaterialStack>,
        produce: Vec<MaterialStack>,
        byproducts: Vec<MaterialStack>,
        power_required: i64,
        duration_ticks: u32,
    ) -> Self {
        Self {
            accepted_batches,
            consume,
            produce,
            byproducts,
            power_required,
            duration_ticks,
            reject_reason: None,
        }
    }

    pub fn rejected(reason: impl Into<String>) -> Self {
        Self {
            accepted_batches: 0,
            consume: Vec::new(),
            produce: Vec::new(),
            byproducts: Vec::new(),
            power_required: 0,
            duration_ticks: 0,
            reject_reason: Some(reason.into()),
        }
    }

    pub fn is_rejected(&self) -> bool {
        self.reject_reason.is_some()
    }
}

/// Runtime request for validating/estimating factory build.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FactoryBuildRequest {
    pub factory_id: String,
    pub site_id: String,
    pub builder: String,
    #[serde(default)]
    pub available_inputs: Vec<MaterialStack>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub available_inputs_by_ledger: Option<BTreeMap<String, Vec<MaterialStack>>>,
    pub available_power: i64,
}

/// Runtime decision returned by factory module for build action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FactoryBuildDecision {
    pub accepted: bool,
    #[serde(default)]
    pub consume: Vec<MaterialStack>,
    pub duration_ticks: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reject_reason: Option<String>,
}

impl FactoryBuildDecision {
    pub fn accepted(consume: Vec<MaterialStack>, duration_ticks: u32) -> Self {
        Self {
            accepted: true,
            consume,
            duration_ticks,
            reject_reason: None,
        }
    }

    pub fn rejected(reason: impl Into<String>) -> Self {
        Self {
            accepted: false,
            consume: Vec::new(),
            duration_ticks: 0,
            reject_reason: Some(reason.into()),
        }
    }
}

/// Author-facing interface contract for recipe modules.
pub trait RecipeModuleApi {
    fn describe_recipe(&self) -> RecipeModuleSpec;
    fn evaluate_recipe(&self, req: RecipeExecutionRequest) -> RecipeExecutionPlan;
}

/// Author-facing interface contract for product modules.
pub trait ProductModuleApi {
    fn describe_product(&self) -> ProductModuleSpec;
    fn evaluate_product(&self, req: ProductValidationRequest) -> ProductValidationDecision;
}

/// Author-facing interface contract for factory modules.
pub trait FactoryModuleApi {
    fn describe_factory(&self) -> FactoryModuleSpec;
    fn evaluate_build(&self, req: FactoryBuildRequest) -> FactoryBuildDecision;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn economy_module_kind_serializes_snake_case() {
        let value = serde_json::to_string(&EconomyModuleKind::Recipe)
            .expect("serialize economy module kind");
        assert_eq!(value, "\"recipe\"");
    }

    #[test]
    fn material_profile_round_trips_with_defaults() {
        let profile = MaterialProfileV1 {
            kind: "iron_ingot".to_string(),
            tier: 2,
            category: "intermediate".to_string(),
            stack_limit: 500,
            transport_loss_class: MaterialTransportLossClass::Medium,
            decay_bps_per_tick: 10,
            default_priority: MaterialDefaultPriority::Standard,
        };
        let json = serde_json::to_vec(&profile).expect("serialize material profile");
        let decoded: MaterialProfileV1 =
            serde_json::from_slice(&json).expect("deserialize material profile");
        assert_eq!(decoded, profile);

        let legacy = serde_json::json!({
            "kind": "copper_ore",
            "tier": 1,
            "category": "ore",
            "stack_limit": 800
        });
        let legacy_decoded: MaterialProfileV1 =
            serde_json::from_value(legacy).expect("deserialize legacy material profile");
        assert_eq!(
            legacy_decoded.transport_loss_class,
            MaterialTransportLossClass::Medium
        );
        assert_eq!(
            legacy_decoded.default_priority,
            MaterialDefaultPriority::Standard
        );
    }

    #[test]
    fn product_recipe_and_factory_profiles_round_trip_json() {
        let product = ProductProfileV1 {
            product_id: "module_rack".to_string(),
            role_tag: "scale".to_string(),
            maintenance_sink: vec![MaterialStack::new("hardware_part", 1)],
            tradable: true,
            unlock_stage: "scale_out".to_string(),
        };
        let recipe = RecipeProfileV1 {
            substitution_rules: Vec::new(),
            recipe_id: "recipe.assembler.module_rack".to_string(),
            bottleneck_tags: vec!["control_chip".to_string(), "sensor_pack".to_string()],
            stage_gate: "scale_out".to_string(),
            preferred_factory_tags: vec!["assembler".to_string(), "precision".to_string()],
        };
        let factory = FactoryProfileV1 {
            factory_id: "factory.assembler.mk1".to_string(),
            tier: 2,
            recipe_slots: 4,
            tags: vec!["assembler".to_string(), "precision".to_string()],
        };
        let product_json = serde_json::to_vec(&product).expect("serialize product profile");
        let recipe_json = serde_json::to_vec(&recipe).expect("serialize recipe profile");
        let factory_json = serde_json::to_vec(&factory).expect("serialize factory profile");
        let product_decoded: ProductProfileV1 =
            serde_json::from_slice(&product_json).expect("deserialize product profile");
        let recipe_decoded: RecipeProfileV1 =
            serde_json::from_slice(&recipe_json).expect("deserialize recipe profile");
        let factory_decoded: FactoryProfileV1 =
            serde_json::from_slice(&factory_json).expect("deserialize factory profile");
        assert_eq!(product_decoded, product);
        assert_eq!(recipe_decoded, recipe);
        assert_eq!(factory_decoded, factory);
    }

    #[test]
    fn recipe_module_spec_round_trips_json() {
        let spec = RecipeModuleSpec {
            recipe_id: "recipe.motor.mk1".to_string(),
            display_name: "Motor MK1".to_string(),
            inputs: vec![MaterialStack::new("gear", 2), MaterialStack::new("wire", 4)],
            outputs: vec![MaterialStack::new("motor", 1)],
            byproducts: vec![MaterialStack::new("scrap", 1)],
            cycle_ticks: 12,
            power_per_cycle: 30,
            allowed_factory_tags: vec!["assembly".to_string()],
            min_factory_tier: 2,
        };

        let json = serde_json::to_vec(&spec).expect("serialize recipe module spec");
        let decoded: RecipeModuleSpec =
            serde_json::from_slice(&json).expect("deserialize recipe module spec");
        assert_eq!(decoded, spec);
    }

    #[test]
    fn rejected_recipe_plan_has_empty_effective_output() {
        let plan = RecipeExecutionPlan::rejected("missing power");
        assert!(plan.is_rejected());
        assert_eq!(plan.accepted_batches, 0);
        assert!(plan.consume.is_empty());
        assert!(plan.produce.is_empty());
        assert_eq!(plan.reject_reason.as_deref(), Some("missing power"));
    }

    #[test]
    fn accepted_factory_build_decision_omits_reject_reason() {
        let decision = FactoryBuildDecision::accepted(vec![MaterialStack::new("steel", 20)], 60);
        assert!(decision.accepted);
        assert!(decision.reject_reason.is_none());

        let json = serde_json::to_value(&decision).expect("serialize factory build decision");
        assert!(json.get("reject_reason").is_none());
    }

    #[test]
    fn rejected_product_validation_keeps_notes() {
        let decision = ProductValidationDecision::rejected(
            "motor_mk1",
            128,
            true,
            vec!["industrial".to_string()],
            vec!["stack exceeds limit".to_string()],
        );
        assert!(!decision.accepted);
        assert_eq!(decision.notes.len(), 1);
        assert_eq!(decision.notes[0], "stack exceeds limit");
    }

    #[test]
    fn recipe_execution_request_supports_optional_ledger_inputs() {
        let mut by_ledger = BTreeMap::new();
        by_ledger.insert(
            "site:alpha".to_string(),
            vec![MaterialStack::new("iron_ingot", 12)],
        );
        let request = RecipeExecutionRequest {
            recipe_id: "recipe.motor.mk1".to_string(),
            factory_id: "factory.alpha".to_string(),
            desired_batches: 2,
            available_inputs: vec![MaterialStack::new("iron_ingot", 12)],
            available_inputs_by_ledger: Some(by_ledger.clone()),
            available_power: 80,
            deterministic_seed: 42,
        };

        let json = serde_json::to_vec(&request).expect("serialize recipe request");
        let decoded: RecipeExecutionRequest =
            serde_json::from_slice(&json).expect("deserialize recipe request");
        assert_eq!(decoded.available_inputs_by_ledger, Some(by_ledger));

        let legacy_json = serde_json::json!({
            "recipe_id": "recipe.motor.mk1",
            "factory_id": "factory.alpha",
            "desired_batches": 2,
            "available_inputs": [{"kind": "iron_ingot", "amount": 12}],
            "available_power": 80,
            "deterministic_seed": 42
        });
        let legacy: RecipeExecutionRequest =
            serde_json::from_value(legacy_json).expect("deserialize legacy recipe request");
        assert!(legacy.available_inputs_by_ledger.is_none());
    }

    #[test]
    fn factory_build_request_supports_optional_ledger_inputs() {
        let mut by_ledger = BTreeMap::new();
        by_ledger.insert(
            "agent:builder-a".to_string(),
            vec![
                MaterialStack::new("steel_plate", 20),
                MaterialStack::new("circuit_board", 4),
            ],
        );
        let request = FactoryBuildRequest {
            factory_id: "factory.alpha".to_string(),
            site_id: "site-1".to_string(),
            builder: "builder-a".to_string(),
            available_inputs: vec![
                MaterialStack::new("steel_plate", 20),
                MaterialStack::new("circuit_board", 4),
            ],
            available_inputs_by_ledger: Some(by_ledger.clone()),
            available_power: 100,
        };

        let json = serde_json::to_vec(&request).expect("serialize factory build request");
        let decoded: FactoryBuildRequest =
            serde_json::from_slice(&json).expect("deserialize factory build request");
        assert_eq!(decoded.available_inputs_by_ledger, Some(by_ledger));

        let legacy_json = serde_json::json!({
            "factory_id": "factory.alpha",
            "site_id": "site-1",
            "builder": "builder-a",
            "available_inputs": [{"kind": "steel_plate", "amount": 20}],
            "available_power": 100
        });
        let legacy: FactoryBuildRequest =
            serde_json::from_value(legacy_json).expect("deserialize legacy factory build request");
        assert!(legacy.available_inputs_by_ledger.is_none());
    }
}
