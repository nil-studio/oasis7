//! Persistent world values, sparse reducers, and concrete codecs.
#[cfg(not(target_arch = "wasm32"))]
pub mod agent_cell;
#[cfg(not(target_arch = "wasm32"))]
pub mod agent_claims;
pub mod agent_state;
#[cfg(not(target_arch = "wasm32"))]
pub mod audit;
#[cfg(not(target_arch = "wasm32"))]
pub mod capability_authorization;
#[cfg(not(target_arch = "wasm32"))]
pub mod consensus;
#[cfg(not(target_arch = "wasm32"))]
pub mod effect;
#[cfg(not(target_arch = "wasm32"))]
pub mod error;
#[cfg(not(target_arch = "wasm32"))]
pub mod events;
#[cfg(not(target_arch = "wasm32"))]
pub mod formal_retained_history;
#[cfg(not(target_arch = "wasm32"))]
pub mod gameplay_capability;
#[cfg(not(target_arch = "wasm32"))]
pub mod gameplay_state;
#[cfg(not(target_arch = "wasm32"))]
pub mod governance;
#[cfg(not(target_arch = "wasm32"))]
pub mod incremental_module_instance_fields;
#[cfg(not(target_arch = "wasm32"))]
pub mod incremental_module_instance_index;
#[cfg(not(target_arch = "wasm32"))]
pub mod incremental_module_marketplace;
#[cfg(not(target_arch = "wasm32"))]
pub mod incremental_owner_closure;
#[cfg(not(target_arch = "wasm32"))]
pub mod main_token;
#[cfg(not(target_arch = "wasm32"))]
pub mod manifest;
#[cfg(not(target_arch = "wasm32"))]
pub mod modules;
#[cfg(not(target_arch = "wasm32"))]
pub mod node_points;
#[cfg(not(target_arch = "wasm32"))]
pub mod policy;
#[cfg(not(target_arch = "wasm32"))]
pub mod reward_asset;
#[cfg(not(target_arch = "wasm32"))]
pub mod rules;
#[cfg(not(target_arch = "wasm32"))]
pub mod snapshot;
#[cfg(not(target_arch = "wasm32"))]
pub mod snapshot_delta;
#[cfg(not(target_arch = "wasm32"))]
pub mod state;

#[cfg(not(target_arch = "wasm32"))]
pub mod builtin_factory_catalog;
#[cfg(not(target_arch = "wasm32"))]
pub mod builtin_recipe_catalog;
#[cfg(not(target_arch = "wasm32"))]
pub mod cognition_feedback_contract;
#[cfg(not(target_arch = "wasm32"))]
pub mod cognition_recovery;
#[cfg(not(target_arch = "wasm32"))]
pub mod cognition_scheduler;
#[cfg(not(target_arch = "wasm32"))]
pub mod cognition_wake;
#[cfg(not(target_arch = "wasm32"))]
pub mod service_admission;
#[cfg(not(target_arch = "wasm32"))]
pub mod types;
#[cfg(not(target_arch = "wasm32"))]
pub mod util;
#[cfg(not(target_arch = "wasm32"))]
pub mod world;
#[cfg(not(target_arch = "wasm32"))]
pub mod world_event;
#[cfg(not(target_arch = "wasm32"))]
pub mod world_rules;
#[cfg(not(target_arch = "wasm32"))]
pub use agent_cell::*;
#[cfg(not(target_arch = "wasm32"))]
pub use agent_claims::*;
#[cfg(not(target_arch = "wasm32"))]
pub use agent_state::*;
#[cfg(not(target_arch = "wasm32"))]
pub use audit::*;
#[cfg(not(target_arch = "wasm32"))]
pub use builtin_factory_catalog::*;
#[cfg(not(target_arch = "wasm32"))]
pub use builtin_recipe_catalog::*;
#[cfg(not(target_arch = "wasm32"))]
pub use capability_authorization::*;
#[cfg(not(target_arch = "wasm32"))]
pub use cognition_feedback_contract::*;
#[cfg(not(target_arch = "wasm32"))]
pub use cognition_recovery::*;
#[cfg(not(target_arch = "wasm32"))]
pub use cognition_scheduler::*;
#[cfg(not(target_arch = "wasm32"))]
pub use cognition_wake::*;
#[cfg(not(target_arch = "wasm32"))]
pub use consensus::*;
#[cfg(not(target_arch = "wasm32"))]
pub use effect::*;
#[cfg(not(target_arch = "wasm32"))]
pub use error::*;
#[cfg(not(target_arch = "wasm32"))]
pub use events::*;
#[cfg(not(target_arch = "wasm32"))]
pub use formal_retained_history::*;
#[cfg(not(target_arch = "wasm32"))]
pub use gameplay_state::*;
#[cfg(not(target_arch = "wasm32"))]
pub use governance::*;
#[cfg(not(target_arch = "wasm32"))]
pub use main_token::*;
#[cfg(not(target_arch = "wasm32"))]
pub use manifest::*;
#[cfg(not(target_arch = "wasm32"))]
pub use modules::*;
#[cfg(not(target_arch = "wasm32"))]
pub use node_points::*;
#[cfg(not(target_arch = "wasm32"))]
pub use policy::*;
#[cfg(not(target_arch = "wasm32"))]
pub use reward_asset::*;
#[cfg(not(target_arch = "wasm32"))]
pub use rules::*;
#[cfg(not(target_arch = "wasm32"))]
pub use snapshot::*;
#[cfg(not(target_arch = "wasm32"))]
pub use snapshot_delta::*;
#[cfg(not(target_arch = "wasm32"))]
pub use state::*;
#[cfg(not(target_arch = "wasm32"))]
pub use types::*;
#[cfg(not(target_arch = "wasm32"))]
pub use world::*;
#[cfg(not(target_arch = "wasm32"))]
pub use world_event::*;
#[cfg(not(target_arch = "wasm32"))]
pub use world_rules::*;
#[cfg(not(target_arch = "wasm32"))]
pub mod codec;
#[cfg(not(target_arch = "wasm32"))]
pub mod cognition;
#[cfg(not(target_arch = "wasm32"))]
mod cognition_receipt_binding;
#[cfg(not(target_arch = "wasm32"))]
pub mod segmentation;

#[cfg(not(target_arch = "wasm32"))]
pub mod incremental_agent_body;
#[cfg(not(target_arch = "wasm32"))]
pub mod incremental_event_fields;

#[cfg(not(target_arch = "wasm32"))]
pub mod incremental_ledger_value;

#[cfg(not(target_arch = "wasm32"))]
pub mod incremental_field_member;
#[cfg(not(target_arch = "wasm32"))]
pub mod incremental_field_presence;
#[cfg(not(target_arch = "wasm32"))]
pub mod incremental_inventory_change;
#[cfg(not(target_arch = "wasm32"))]
pub mod incremental_inventory_genesis;
#[cfg(not(target_arch = "wasm32"))]
pub mod incremental_inventory_overlay_change;
#[cfg(not(target_arch = "wasm32"))]
pub mod incremental_inventory_overlay_read;
#[cfg(not(target_arch = "wasm32"))]
pub mod incremental_inventory_read;
#[cfg(not(target_arch = "wasm32"))]
pub mod incremental_inventory_transaction;
#[cfg(not(target_arch = "wasm32"))]
mod incremental_inventory_value_read;
#[cfg(not(target_arch = "wasm32"))]
mod incremental_key_output;

#[cfg(not(target_arch = "wasm32"))]
pub mod incremental_agent_fields;

#[cfg(not(target_arch = "wasm32"))]
pub mod incremental_projection;
#[cfg(not(target_arch = "wasm32"))]
pub mod incremental_scheduler_fields;

#[cfg(not(target_arch = "wasm32"))]
pub mod incremental_world_field_key;

#[cfg(not(target_arch = "wasm32"))]
pub mod incremental_semantic_key;

#[cfg(not(target_arch = "wasm32"))]
mod task_terminal_v2;
#[cfg(not(target_arch = "wasm32"))]
pub use task_terminal_v2::*;

#[cfg(not(target_arch = "wasm32"))]
pub mod incremental_world_u64;

#[cfg(not(target_arch = "wasm32"))]
pub mod incremental_field_read;

#[cfg(not(target_arch = "wasm32"))]
pub mod incremental_field_overlay;

#[cfg(not(target_arch = "wasm32"))]
pub mod incremental_delivery_cursor;
#[cfg(not(target_arch = "wasm32"))]
mod incremental_field_write;

#[cfg(not(target_arch = "wasm32"))]
pub mod incremental_world_id_sets;

#[cfg(not(target_arch = "wasm32"))]
pub mod incremental_world_number_maps;

#[cfg(not(target_arch = "wasm32"))]
pub mod incremental_authenticated_nonces;

#[cfg(not(target_arch = "wasm32"))]
pub mod incremental_authorization_mapsets;

#[cfg(not(target_arch = "wasm32"))]
mod incremental_expected_bytes;
#[cfg(not(target_arch = "wasm32"))]
pub mod incremental_factory_authority_records;
#[cfg(not(target_arch = "wasm32"))]
pub mod incremental_factory_records;
#[cfg(not(target_arch = "wasm32"))]
mod incremental_identity_amounts;
#[cfg(not(target_arch = "wasm32"))]
pub mod incremental_industry_jobs;
#[cfg(not(target_arch = "wasm32"))]
pub mod incremental_installed_module_targets;
#[cfg(not(target_arch = "wasm32"))]
pub mod incremental_logistics_records;
#[cfg(not(target_arch = "wasm32"))]
pub mod incremental_material_records;
#[cfg(not(target_arch = "wasm32"))]
mod incremental_record_diff;
#[cfg(not(target_arch = "wasm32"))]
mod incremental_record_key_read;
#[cfg(not(target_arch = "wasm32"))]
mod incremental_record_tree;
#[cfg(not(target_arch = "wasm32"))]
mod incremental_record_value_read;
#[cfg(not(target_arch = "wasm32"))]
mod incremental_utf8_output;
#[cfg(not(target_arch = "wasm32"))]
pub mod incremental_world_balances;
#[cfg(not(target_arch = "wasm32"))]
pub mod incremental_world_text_maps;

#[cfg(not(target_arch = "wasm32"))]
mod incremental_family_page_tracking;

#[cfg(not(target_arch = "wasm32"))]
pub mod incremental_fixed_economy_records;

#[cfg(not(target_arch = "wasm32"))]
mod incremental_intent_fields;

#[cfg(not(target_arch = "wasm32"))]
pub mod incremental_agent_authority_records;

#[cfg(not(target_arch = "wasm32"))]
pub mod incremental_reward_budget_records;

#[cfg(not(target_arch = "wasm32"))]
pub mod incremental_governance_records;

#[cfg(not(target_arch = "wasm32"))]
pub mod incremental_main_token_policy_records;
#[cfg(not(target_arch = "wasm32"))]
pub mod incremental_world_rule_effects;
#[cfg(not(target_arch = "wasm32"))]
pub mod incremental_world_rule_lineages;
#[cfg(not(target_arch = "wasm32"))]
mod incremental_world_rule_parent;
#[cfg(not(target_arch = "wasm32"))]
mod incremental_world_rule_record;
#[cfg(not(target_arch = "wasm32"))]
pub mod incremental_world_rule_shipments;

#[cfg(not(target_arch = "wasm32"))]
pub mod incremental_world_rule_organization;

#[cfg(not(target_arch = "wasm32"))]
pub mod incremental_world_rule_supply_root;

#[cfg(not(target_arch = "wasm32"))]
pub mod incremental_diplomacy;

#[cfg(not(target_arch = "wasm32"))]
pub mod incremental_runtime_scheduler_controls;
