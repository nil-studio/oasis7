use super::*;
use crate::incremental_typed_record_test_support::{roundtrip, source_admission};
#[path = "incremental_world_rule_industrial_test_data.rs"]
mod data;

#[test]
fn industrial_full_original_graph_roundtrips_reserved_keys_and_integer_extremes() {
    let _source_charge = source_admission();
    let source = data::industrial();
    roundtrip(
        &source,
        WorldRuleCompositeBranchV1::IndustrialWork.profile(),
    );
}

#[test]
fn substitution_full_original_graph_roundtrips_all_nested_records() {
    let _source_charge = source_admission();
    let source = data::substitution();
    roundtrip(&source, WorldRuleCompositeBranchV1::Substitution.profile());
}
