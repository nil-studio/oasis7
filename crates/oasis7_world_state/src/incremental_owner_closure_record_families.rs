use super::{SeenField, WorldStateOwnerError};
use crate::incremental_semantic_key::SemanticKeyBudget;
use crate::{
    incremental_agent_authority_records::{
        AgentAuthorityRecordKindV1, qualify_agent_authority_family_v1,
    },
    incremental_diplomacy::{DiplomacyFieldV1, qualify_diplomacy_family_v1},
    incremental_factory_authority_records::{
        FactoryAuthorityRecordKindV1, qualify_factory_authority_family_v1,
    },
    incremental_field_read::WorldFieldReadContextV1,
    incremental_fixed_economy_records::{FixedEconomyFieldV1, qualify_fixed_economy_family_v1},
    incremental_governance_records::{GovernanceRecordKindV1, qualify_governance_family_v1},
    incremental_logistics_records::{
        LogisticsFamilyReadContextV1, LogisticsRecordKindV1, LogisticsWorkBudgetV1,
        qualify_logistics_family_v1,
    },
    incremental_main_token_policy_records::{
        MainTokenPolicyKindV1, qualify_main_token_policy_family_v1,
    },
    incremental_material_records::{MaterialRecordKindV1, qualify_material_family_v1},
    incremental_projection::{FieldDescriptor, FieldDestination, FieldOwner, FieldShape},
    incremental_reward_budget_records::{RewardRecordKindV1, qualify_reward_family_v1},
    incremental_world_rule_organization::root::qualify_organization_family_v1,
    incremental_world_rule_supply_root::qualify_supply_family_v1,
};
use oasis7_formal_protocol::incremental_value::ValueTreeError;
use oasis7_formal_protocol::{
    incremental_execution::RootSetV1,
    incremental_state::{MerkleNodeReader, StateNamespaceV1, StateScanBudget},
    incremental_value::{ValueNodeReader, ValueReadContext},
};
use std::cell::RefCell;

pub(super) const GOVERNANCE_FIELD_COUNT: u8 = GovernanceRecordKindV1::ALL.len() as u8;
pub(super) const FACTORY_AUTHORITY_FIELD_COUNT: u8 = FactoryAuthorityRecordKindV1::ALL.len() as u8;
pub(super) const MAIN_TOKEN_POLICY_FIELD_COUNT: u8 = MainTokenPolicyKindV1::ALL.len() as u8;
pub(super) const MATERIAL_FIELD_COUNT: u8 = MaterialRecordKindV1::ALL.len() as u8;
pub(super) const LOGISTICS_FIELD_COUNT: u8 = LogisticsRecordKindV1::ALL.len() as u8;
pub(super) const FIXED_ECONOMY_FIELD_COUNT: u8 = FixedEconomyFieldV1::ALL.len() as u8;
pub(super) const AGENT_AUTHORITY_FIELD_COUNT: u8 = AgentAuthorityRecordKindV1::ALL.len() as u8;
pub(super) const REWARD_FIELD_COUNT: u8 = RewardRecordKindV1::ALL.len() as u8;
pub(super) const ORGANIZATION_FIELD_COUNT: u8 = 2;
pub(super) const SUPPLY_FIELD_COUNT: u8 = 2;
pub(super) const DIPLOMACY_FIELD_COUNT: u8 = DiplomacyFieldV1::ALL.len() as u8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RecordFamily {
    Governance,
    FactoryAuthority,
    MainTokenPolicy,
    Material,
    Logistics,
    FixedEconomy,
    AgentAuthority,
    Reward,
    Diplomacy,
}

#[derive(Debug, Default, Clone, Copy)]
pub(super) struct FamilyCoverage {
    pub governance: bool,
    pub factory_authority: bool,
    pub main_token_policy: bool,
    pub material: bool,
    pub logistics: bool,
    pub fixed_economy: bool,
    pub agent_authority: bool,
    pub reward: bool,
    pub organization: bool,
    pub supply: bool,
    pub diplomacy: bool,
}

fn matches(
    field: FieldDescriptor,
    name: &str,
    shape: FieldShape,
    namespace: StateNamespaceV1,
) -> bool {
    field.owner == FieldOwner::WorldState
        && field.name == name
        && field.shape == shape
        && field.destination == FieldDestination::State(namespace)
}

pub(super) fn family_for_descriptor(field: FieldDescriptor) -> Option<RecordFamily> {
    if GovernanceRecordKindV1::ALL
        .into_iter()
        .any(|kind| matches(field, kind.field_name(), FieldShape::Map, kind.namespace()))
    {
        return Some(RecordFamily::Governance);
    }
    if FactoryAuthorityRecordKindV1::ALL
        .into_iter()
        .any(|kind| matches(field, kind.field_name(), FieldShape::Map, kind.namespace()))
    {
        return Some(RecordFamily::FactoryAuthority);
    }
    if MainTokenPolicyKindV1::ALL
        .into_iter()
        .any(|kind| matches(field, kind.name(), kind.shape(), kind.namespace()))
    {
        return Some(RecordFamily::MainTokenPolicy);
    }
    if MaterialRecordKindV1::ALL
        .into_iter()
        .any(|kind| matches(field, kind.field_name(), FieldShape::Map, kind.namespace()))
    {
        return Some(RecordFamily::Material);
    }
    if LogisticsRecordKindV1::ALL
        .into_iter()
        .any(|kind| matches(field, kind.field_name(), FieldShape::Map, kind.namespace()))
    {
        return Some(RecordFamily::Logistics);
    }
    if FixedEconomyFieldV1::ALL
        .into_iter()
        .any(|kind| matches(field, kind.name(), FieldShape::Value, kind.namespace()))
    {
        return Some(RecordFamily::FixedEconomy);
    }
    if AgentAuthorityRecordKindV1::ALL
        .into_iter()
        .any(|kind| matches(field, kind.field_name(), FieldShape::Map, kind.namespace()))
    {
        return Some(RecordFamily::AgentAuthority);
    }
    if RewardRecordKindV1::ALL
        .into_iter()
        .any(|kind| matches(field, kind.name(), kind.shape(), kind.namespace()))
    {
        return Some(RecordFamily::Reward);
    }
    DiplomacyFieldV1::ALL
        .into_iter()
        .any(|kind| {
            matches(
                field,
                kind.name(),
                FieldShape::Map,
                StateNamespaceV1::Business,
            )
        })
        .then_some(RecordFamily::Diplomacy)
}

pub(super) fn is_world_rules_parent(field: FieldDescriptor) -> bool {
    field.owner == FieldOwner::WorldState
        && field.name == "world_rules"
        && field.shape == FieldShape::Value
        && field.destination == FieldDestination::State(StateNamespaceV1::Authorization)
}

fn world_rules_entries_present(seen: &[SeenField], descriptors: &[FieldDescriptor]) -> bool {
    descriptors
        .iter()
        .copied()
        .enumerate()
        .find(|(_, field)| is_world_rules_parent(*field))
        .is_some_and(|(index, _)| seen[index].entries != 0)
}

fn family_has_entries(
    family: RecordFamily,
    seen: &[SeenField],
    descriptors: &[FieldDescriptor],
) -> bool {
    descriptors
        .iter()
        .copied()
        .enumerate()
        .any(|(index, field)| {
            family_for_descriptor(field) == Some(family) && seen[index].entries != 0
        })
}

/// Split a caller's remaining budget into disjoint outer/child allowances.
/// The unused portions are returned after qualification, so each family
/// shares the same finite candidate budget instead of receiving a reset.
fn split_budget(remaining: &StateScanBudget) -> (StateScanBudget, StateScanBudget) {
    let outer = StateScanBudget {
        nodes_remaining: remaining.nodes_remaining / 2,
        read_bytes_remaining: remaining.read_bytes_remaining / 2,
        leaves_remaining: remaining.leaves_remaining / 2,
    };
    let inner = StateScanBudget {
        nodes_remaining: remaining.nodes_remaining - outer.nodes_remaining,
        read_bytes_remaining: remaining.read_bytes_remaining - outer.read_bytes_remaining,
        leaves_remaining: remaining.leaves_remaining - outer.leaves_remaining,
    };
    (outer, inner)
}

fn zero_budget() -> StateScanBudget {
    StateScanBudget {
        nodes_remaining: 0,
        read_bytes_remaining: 0,
        leaves_remaining: 0,
    }
}

/// Lease the caller's remaining scan allowance before invoking the family
/// visitor. If it unwinds, the caller conservatively keeps the whole lease
/// spent; normal errors return only the amount the visitor did not consume.
fn lease_budget(shared: &mut StateScanBudget) -> (StateScanBudget, StateScanBudget) {
    let (outer, inner) = split_budget(shared);
    *shared = zero_budget();
    (outer, inner)
}

fn return_unused_budget(
    shared: &mut StateScanBudget,
    outer: &StateScanBudget,
    inner: &StateScanBudget,
) -> Result<(), WorldStateOwnerError> {
    shared.nodes_remaining = outer
        .nodes_remaining
        .checked_add(inner.nodes_remaining)
        .ok_or(WorldStateOwnerError::Invalid("family node budget overflow"))?;
    shared.read_bytes_remaining = outer
        .read_bytes_remaining
        .checked_add(inner.read_bytes_remaining)
        .ok_or(WorldStateOwnerError::Invalid("family byte budget overflow"))?;
    shared.leaves_remaining = outer
        .leaves_remaining
        .checked_add(inner.leaves_remaining)
        .ok_or(WorldStateOwnerError::Invalid("family leaf budget overflow"))?;
    Ok(())
}

#[expect(
    clippy::too_many_arguments,
    reason = "the verifier keeps the bound roots, shared readers, scan budget, limits, cancellation, and seen schema explicit"
)]
pub(super) fn verify_present_families<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    C: FnMut() -> bool,
>(
    roots: &RootSetV1,
    root_digest: &[u8; 32],
    state: &M,
    values: &mut ValueReadContext<'_, R>,
    state_io: &mut StateScanBudget,
    max_value_bytes: u64,
    cancelled: &RefCell<C>,
    seen: &[SeenField],
    descriptors: &[FieldDescriptor],
) -> Result<FamilyCoverage, WorldStateOwnerError> {
    let mut key_work = SemanticKeyBudget {
        // This is one aggregate finite ceiling for the candidate, shared by
        // all family qualifiers. Per-record readers retain their tighter cap.
        bytes_remaining: max_value_bytes,
    };
    let mut cells = state_io.leaves_remaining;
    let mut coverage = FamilyCoverage::default();

    macro_rules! qualify {
        ($namespace:expr, $qualifier:path, $binding:ident) => {{
            let (mut outer, mut inner) = lease_budget(state_io);
            let result = (|| {
                let root = roots
                    .state
                    .iter()
                    .find(|root| root.namespace == $namespace)
                    .ok_or(ValueTreeError::Invalid("record family state root missing"))?;
                let mut context = WorldFieldReadContextV1 {
                    root,
                    state,
                    values: ValueReadContext {
                        source: values.source,
                        memory: values.memory,
                        class: values.class,
                        io: &mut *values.io,
                    },
                    key_work: &mut key_work,
                    semantic_bytes: max_value_bytes,
                    record_bytes: max_value_bytes,
                    take_state_error: || None,
                    cancelled: || (cancelled.borrow_mut())(),
                };
                let qualification = $qualifier(
                    roots,
                    &mut context,
                    &mut outer,
                    &mut inner,
                    &mut cells,
                    max_value_bytes,
                )?;
                qualification.$binding(root_digest)?;
                drop(qualification);
                Ok::<(), ValueTreeError>(())
            })();
            return_unused_budget(state_io, &outer, &inner)?;
            result.map_err(WorldStateOwnerError::Value)?;
        }};
    }

    macro_rules! qualify_without_payload_limit {
        ($qualifier:path, $binding:ident) => {{
            let (mut outer, mut inner) = lease_budget(state_io);
            let result = (|| {
                let mut context = WorldFieldReadContextV1 {
                    root: &roots.state[0],
                    state,
                    values: ValueReadContext {
                        source: values.source,
                        memory: values.memory,
                        class: values.class,
                        io: &mut *values.io,
                    },
                    key_work: &mut key_work,
                    semantic_bytes: max_value_bytes,
                    record_bytes: max_value_bytes,
                    take_state_error: || None,
                    cancelled: || (cancelled.borrow_mut())(),
                };
                let qualification =
                    $qualifier(roots, &mut context, &mut outer, &mut inner, &mut cells)?;
                qualification.$binding(root_digest)?;
                drop(qualification);
                Ok::<(), ValueTreeError>(())
            })();
            return_unused_budget(state_io, &outer, &inner)?;
            result.map_err(WorldStateOwnerError::Value)?;
        }};
    }

    if family_has_entries(RecordFamily::Governance, seen, descriptors) {
        qualify!(
            StateNamespaceV1::Business,
            qualify_governance_family_v1,
            validate_binding
        );
        coverage.governance = true;
    }
    if family_has_entries(RecordFamily::FactoryAuthority, seen, descriptors) {
        qualify!(
            StateNamespaceV1::Business,
            qualify_factory_authority_family_v1,
            validate_binding
        );
        coverage.factory_authority = true;
    }
    if family_has_entries(RecordFamily::MainTokenPolicy, seen, descriptors) {
        qualify!(
            StateNamespaceV1::Business,
            qualify_main_token_policy_family_v1,
            validate_binding
        );
        coverage.main_token_policy = true;
    }
    if family_has_entries(RecordFamily::Material, seen, descriptors) {
        qualify!(
            StateNamespaceV1::Business,
            qualify_material_family_v1,
            validate_binding
        );
        coverage.material = true;
    }
    if family_has_entries(RecordFamily::Logistics, seen, descriptors) {
        let max_pending_node_bytes = usize::try_from(max_value_bytes)
            .map_err(|_| WorldStateOwnerError::Invalid("logistics payload limit"))?;
        let (mut outer, inner) = lease_budget(state_io);
        let cells_remaining = cells;
        let mut work = LogisticsWorkBudgetV1 {
            cells_remaining,
            max_pending_node_bytes,
            scan: inner,
        };
        let result = (|| {
            let mut context = LogisticsFamilyReadContextV1 {
                state,
                values: ValueReadContext {
                    source: values.source,
                    memory: values.memory,
                    class: values.class,
                    io: &mut *values.io,
                },
                key_work: &mut key_work,
                semantic_bytes: max_value_bytes,
                record_bytes: max_value_bytes,
                max_payload_bytes: max_value_bytes,
                outer_scan: &mut outer,
                take_state_error: || None,
                cancelled: || (cancelled.borrow_mut())(),
            };
            let qualification = qualify_logistics_family_v1(roots, &mut context, &mut work)?;
            qualification.validate_binding(root_digest)?;
            drop(qualification);
            Ok::<(), ValueTreeError>(())
        })();
        let inner = work.scan;
        cells = work.cells_remaining;
        return_unused_budget(state_io, &outer, &inner)?;
        result.map_err(WorldStateOwnerError::Value)?;
        coverage.logistics = true;
    }
    if family_has_entries(RecordFamily::FixedEconomy, seen, descriptors) {
        qualify_without_payload_limit!(qualify_fixed_economy_family_v1, validate_binding);
        coverage.fixed_economy = true;
    }
    if family_has_entries(RecordFamily::AgentAuthority, seen, descriptors) {
        qualify!(
            StateNamespaceV1::Business,
            qualify_agent_authority_family_v1,
            validate_binding
        );
        coverage.agent_authority = true;
    }
    if family_has_entries(RecordFamily::Reward, seen, descriptors) {
        qualify!(
            StateNamespaceV1::Business,
            qualify_reward_family_v1,
            validate_binding
        );
        coverage.reward = true;
    }
    let world_rules_present = world_rules_entries_present(seen, descriptors);
    if world_rules_present {
        qualify!(
            StateNamespaceV1::Authorization,
            qualify_organization_family_v1,
            validate_binding
        );
        coverage.organization = true;
    }
    if world_rules_present {
        qualify!(
            StateNamespaceV1::Authorization,
            qualify_supply_family_v1,
            validate_binding
        );
        coverage.supply = true;
    }
    if descriptors
        .iter()
        .copied()
        .enumerate()
        .any(|(index, field)| {
            family_for_descriptor(field) == Some(RecordFamily::Diplomacy)
                && (seen[index].metadata != 0 || seen[index].entries != 0)
        })
    {
        qualify!(
            StateNamespaceV1::Business,
            qualify_diplomacy_family_v1,
            validate_binding
        );
        coverage.diplomacy = true;
    }
    Ok(coverage)
}
