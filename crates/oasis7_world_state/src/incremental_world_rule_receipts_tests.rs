use super::*;
use crate::incremental_typed_record_test_support::{roundtrip, source_admission};
use oasis7_agent_api::world_action::*;
use std::collections::BTreeMap;
#[path = "incremental_world_rule_receipts_fixture.rs"]
mod data;
#[test]
fn requests_all_outer_intents_preserve_original_map_keys_and_typed_fields() {
    let _source = source_admission();
    for (index, command) in data::variants_WorldRuleCommandV1().into_iter().enumerate() {
        let mut receipt = data::fixture_WorldRuleReceiptV1();
        receipt.command = command;
        receipt.request_id = "inner-request".into();
        let key = if index == 0 {
            "_length".into()
        } else {
            format!("outside-key-{index}")
        };
        roundtrip(
            &BTreeMap::from([(key, receipt)]),
            WorldRuleCompositeBranchV1::Requests.profile(),
        );
    }
}
#[test]
fn requests_every_nested_command_variant_cold_and_streaming_roundtrip() {
    let _source = source_admission();
    let profile = WorldRuleCompositeBranchV1::Requests.profile();
    for command in data::variants_IndustrialWorkCommandV1() {
        roundtrip(&command, profile);
    }
    for command in data::variants_RecipeSubstitutionCommandV1() {
        roundtrip(&command, profile);
    }
    for command in data::variants_MinimumServiceCommandV1() {
        roundtrip(&command, profile);
    }
    for command in data::variants_OrganizationContinuityCommandV1() {
        roundtrip(&command, profile);
    }
    for command in data::variants_EmergencySupplyCommandV1() {
        roundtrip(&command, profile);
    }
}
#[test]
fn orders_original_full_quote_all_phases_and_optional_absence_roundtrip() {
    let _source = source_admission();
    use crate::world_rules::AgentIndustrialPhaseV1 as Phase;
    for phase in [
        Phase::Reserved,
        Phase::Producing,
        Phase::ProductionBlocked,
        Phase::AwaitingDelivery,
        Phase::Delivered,
        Phase::Cancelled,
        Phase::Disposed,
    ] {
        let mut order = data::fixture_AgentIndustrialOrderV1();
        order.phase = phase;
        roundtrip(
            &BTreeMap::from([("_length".to_owned(), order)]),
            WorldRuleCompositeBranchV1::AgentOrders.profile(),
        );
    }
    let mut absent = data::fixture_AgentIndustrialOrderV1();
    absent.delivered_at = None;
    absent.started_at = None;
    absent.produced_at = None;
    absent.blocked_at = None;
    absent.last_progress_tick = None;
    absent.terminal_reason = None;
    absent.refunded_at = None;
    absent.quote.world_binding = None;
    roundtrip(
        &BTreeMap::from([("map-id-independent".to_owned(), absent)]),
        WorldRuleCompositeBranchV1::AgentOrders.profile(),
    );
}
#[test]
fn estate_scope_borrowed_keys_preserve_all_rights_and_reserved_object_id() {
    let _source = source_admission();
    use OrganizationEstateRightV1 as Right;
    for right in [
        Right::Preserve,
        Right::ReturnTrust,
        Right::DistributeResidual,
        Right::FinalizeCase,
        Right::AdvanceCase,
    ] {
        let set = std::collections::BTreeSet::from([OrganizationEstateScopeV1 {
            object_id: "_length".into(),
            right,
        }]);
        roundtrip(&set, WorldRuleCompositeBranchV1::Requests.profile());
    }
}
