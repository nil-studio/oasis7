#![allow(non_snake_case)]
use crate::AgentClaimCostQuote;
use crate::world_rules::{
    AgentIndustrialOrderV1, AgentIndustrialPhaseV1, AgentManufacturingQuoteV1, WorldRuleReceiptV1,
};
use oasis7_agent_api::world_action::*;
use oasis7_wasm_abi::*;
use std::collections::{BTreeMap, BTreeSet};
#[path = "incremental_world_rule_receipts_fixture_0.rs"]
mod part0;
pub(super) use part0::*;
#[path = "incremental_world_rule_receipts_fixture_1.rs"]
mod part1;
pub(super) use part1::*;
#[path = "incremental_world_rule_receipts_fixture_2.rs"]
mod part2;
pub(super) use part2::*;
