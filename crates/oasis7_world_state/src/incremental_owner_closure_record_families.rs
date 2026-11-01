use super::{SeenField, WorldStateOwnerError};
use crate::incremental_semantic_key::SemanticKeyBudget;
use crate::{
    incremental_factory_authority_records::{
        FactoryAuthorityRecordKindV1, qualify_factory_authority_family_v1,
    },
    incremental_field_read::WorldFieldReadContextV1,
    incremental_governance_records::{GovernanceRecordKindV1, qualify_governance_family_v1},
    incremental_main_token_policy_records::{
        MainTokenPolicyKindV1, qualify_main_token_policy_family_v1,
    },
    incremental_projection::{FieldDescriptor, FieldDestination, FieldOwner, FieldShape},
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RecordFamily {
    Governance,
    FactoryAuthority,
    MainTokenPolicy,
}

#[derive(Debug, Default, Clone, Copy)]
pub(super) struct FamilyCoverage {
    pub governance: bool,
    pub factory_authority: bool,
    pub main_token_policy: bool,
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
    MainTokenPolicyKindV1::ALL
        .into_iter()
        .any(|kind| matches(field, kind.name(), kind.shape(), kind.namespace()))
        .then_some(RecordFamily::MainTokenPolicy)
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

    if family_has_entries(RecordFamily::Governance, seen, descriptors) {
        qualify!(qualify_governance_family_v1, validate_binding);
        coverage.governance = true;
    }
    if family_has_entries(RecordFamily::FactoryAuthority, seen, descriptors) {
        qualify!(qualify_factory_authority_family_v1, validate_binding);
        coverage.factory_authority = true;
    }
    if family_has_entries(RecordFamily::MainTokenPolicy, seen, descriptors) {
        qualify!(qualify_main_token_policy_family_v1, validate_binding);
        coverage.main_token_policy = true;
    }
    Ok(coverage)
}
