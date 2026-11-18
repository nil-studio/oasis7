//! Original owner/delegate audience selection over actual readonly columns.
//! This private DATA carrier grants no read, execution or accepted authority.
use super::*;
use crate::service_admission::agent_authority::{
    OriginalDelegationBindingDataV1, original_agent_signer_is_owner_v1,
    owner_public_key_from_original_owner_v1, validate_original_delegation_fields_v1,
};
use oasis7_client_api::formal_projection::{MAX_PROJECTION_SCOPES, ProjectionAudienceV1};
use oasis7_world_state::observation_projection::current_commitment::{
    ProjectionColumnDataV1, ProjectionColumnErrorV1, ProjectionColumnRefV1,
};
use std::{cell::RefCell, collections::BTreeSet, mem::size_of};
pub(super) struct ActualProjectionAudiencesDataV1 {
    pub(super) values: BTreeSet<ProjectionAudienceV1>,
    fees: Vec<MemoryReservation>,
    slots: MemoryReservation,
    _fee: MemoryReservation,
}
fn error(error: ProjectionColumnErrorV1<BodyMutationError>) -> BodyMutationError {
    match error {
        ProjectionColumnErrorV1::Work(error) => error,
        ProjectionColumnErrorV1::Memory(error) => ValueTreeError::Memory(error).into(),
        ProjectionColumnErrorV1::Invalid(note) => invalid(note),
    }
}
fn pay(
    common: &mut FormalBoundedContextV1<'_>,
    n: usize,
) -> Result<MemoryReservation, BodyMutationError> {
    common.charge(1, n as u64).map_err(paid)?;
    common
        .memory()
        .reserve(n, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)
        .map_err(Into::into)
}
impl ActualProjectionAudiencesDataV1 {
    pub(super) fn push_fee(
        &mut self,
        fee: MemoryReservation,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), BodyMutationError> {
        if self.fees.len() == self.fees.capacity() {
            let capacity = self
                .fees
                .capacity()
                .checked_mul(2)
                .unwrap_or(usize::MAX)
                .max(4);
            let n = capacity
                .checked_mul(size_of::<MemoryReservation>())
                .ok_or_else(|| invalid("audience fee slots layout"))?;
            let slots = pay(common, n)?;
            self.fees
                .try_reserve_exact(capacity - self.fees.len())
                .map_err(|_| invalid("audience fee slots allocation"))?;
            if self.fees.capacity() != capacity {
                return Err(invalid("audience fee slots capacity"));
            }
            self.slots = slots
        }
        self.fees.push(fee);
        Ok(())
    }
    fn add(
        &mut self,
        agent: &str,
        key: &str,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), BodyMutationError> {
        let strings = agent
            .len()
            .checked_add(key.len())
            .and_then(|n| n.checked_add(64))
            .ok_or_else(|| invalid("audience original fields layout"))?;
        // At the original 16384-scope cap an std B-tree insertion has fewer than
        // 256 comparisons and at most eight new 11-key nodes, including root splits.
        common
            .charge(
                256,
                strings
                    .checked_mul(512)
                    .ok_or_else(|| invalid("audience comparison work"))? as u64,
            )
            .map_err(paid)?;
        let n = (11 * size_of::<ProjectionAudienceV1>())
            .checked_mul(8)
            .and_then(|n| n.checked_add(2048))
            .and_then(|n| {
                strings
                    .checked_mul(2)
                    .and_then(|strings| n.checked_add(strings))
            })
            .ok_or_else(|| invalid("audience B-tree insertion layout"))?;
        let fee = pay(common, n)?;
        let phase = pay(common, 32768)?;
        let constructed = ProjectionAudienceV1::agent(agent, key.to_owned());
        common.consume_steps(0).map_err(paid)?;
        let audience = match constructed {
            Ok(value) => value,
            Err(note) => return Err(paid(AudienceRefusal { note, _fee: phase })),
        };
        if self.values.insert(audience) {
            self.push_fee(fee, common)?
        }
        Ok(())
    }
}
#[derive(Debug)]
struct AudienceRefusal {
    note: String,
    _fee: MemoryReservation,
}
impl std::fmt::Display for AudienceRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.note)
    }
}
impl std::error::Error for AudienceRefusal {}
fn member<'a>(
    node: &ProjectionColumnRefV1<'a>,
    name: &str,
    memory: &MemoryBudget,
    common: &RefCell<&mut FormalBoundedContextV1<'_>>,
) -> Result<ProjectionColumnRefV1<'a>, BodyMutationError> {
    node.object_member_data(name, memory, &mut |s, b| {
        common.borrow_mut().charge(s, b).map_err(paid)
    })
    .map_err(error)?
    .ok_or_else(|| invalid("actual audience source field missing"))
}
fn text<'a>(
    node: &ProjectionColumnRefV1<'a>,
    name: &str,
    memory: &MemoryBudget,
    common: &RefCell<&mut FormalBoundedContextV1<'_>>,
) -> Result<&'a str, BodyMutationError> {
    member(node, name, memory, common)?
        .text_data()
        .ok_or_else(|| invalid("actual audience text source"))
}
fn number(
    node: &ProjectionColumnRefV1<'_>,
    name: &str,
    memory: &MemoryBudget,
    common: &RefCell<&mut FormalBoundedContextV1<'_>>,
) -> Result<u64, BodyMutationError> {
    member(node, name, memory, common)?
        .unsigned_data()
        .ok_or_else(|| invalid("actual audience number source"))
}
pub(super) fn collect_actual_projection_audiences_v1(
    state: &super::state_columns::OriginalStateColumnsDataV1,
    electricity: &super::original_rows::OriginalProjectionRowsDataV1,
    aux: &ProjectionColumnDataV1,
    metadata:&crate::world::incremental_normal_business_commitment::OriginalProjectionReadMetadataV1<'_>,
    binding: Option<&ReservedValue<oasis7_agent_api::RuntimeBindingV1>>,
    origin: &dyn crate::world::incremental_normal_projection_commitment::facts::ProjectionReadSourceV1,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<ActualProjectionAudiencesDataV1, BodyMutationError> {
    origin.validate_metadata(metadata, common)?;
    state.validate_context(common)?;
    electricity.validate(common)?;
    if !aux.uses_budget(common.memory())
        || binding.is_some_and(|binding| !binding.uses_budget(common.memory()))
    {
        return Err(invalid("actual audience source pool"));
    }
    let fee = pay(common, size_of::<ActualProjectionAudiencesDataV1>())?;
    let slots = pay(common, 0)?;
    let mut result = ActualProjectionAudiencesDataV1 {
        values: BTreeSet::new(),
        fees: Vec::new(),
        slots,
        _fee: fee,
    };
    let memory = common.memory().clone();
    let shared = RefCell::new(&mut *common);
    let revocation = member(
        &aux.root_data(),
        "capability_revocation_state",
        &memory,
        &shared,
    )?;
    let identities = member(&revocation, "agent_identities", &memory, &shared)?;
    // Omission is the original skip-empty delegation-map wire, not an unknown
    // source or permission default. The surrounding actual source is mandatory.
    let delegations = revocation
        .object_member_data("agent_signer_delegations", &memory, &mut |s, b| {
            shared.borrow_mut().charge(s, b).map_err(paid)
        })
        .map_err(error)?;
    identities
        .object_members_data(
            &memory,
            &mut |s, b| shared.borrow_mut().charge(s, b).map_err(paid),
            &mut |agent, identity, _work| {
                let owner = super::facts::resolve_projection_owner_data_v1(
                    state,
                    electricity,
                    metadata,
                    agent,
                    origin,
                    &mut shared.borrow_mut(),
                )
                .map_err(ProjectionColumnErrorV1::Work)?;
                let Some(owner_value) = owner.owner() else {
                    return Ok(());
                };
                let identity_binding = text(&identity, "owner_binding", &memory, &shared)
                    .map_err(ProjectionColumnErrorV1::Work)?;
                shared
                    .borrow_mut()
                    .charge(
                        2,
                        (identity_binding.len() + owner_value.player_id.len()) as u64,
                    )
                    .map_err(paid)
                    .map_err(ProjectionColumnErrorV1::Work)?;
                let Ok(owner_key) =
                    owner_public_key_from_original_owner_v1(identity_binding, owner_value)
                else {
                    return Ok(());
                };
                result
                    .add(agent, owner_key, &mut shared.borrow_mut())
                    .map_err(ProjectionColumnErrorV1::Work)?;
                let delegate = match delegations.as_ref() {
                    Some(delegations) => {
                        delegations.object_member_data(agent, &memory, &mut |s, b| {
                            shared.borrow_mut().charge(s, b).map_err(paid)
                        })?
                    }
                    None => None,
                };
                if let Some(delegate) = delegate {
                    let key = text(&delegate, "delegate_public_key", &memory, &shared)
                        .map_err(ProjectionColumnErrorV1::Work)?;
                    let generation = number(&delegate, "generation", &memory, &shared)
                        .map_err(ProjectionColumnErrorV1::Work)?;
                    shared
                        .borrow_mut()
                        .charge(2, (owner_key.len() + key.len()) as u64)
                        .map_err(paid)
                        .map_err(ProjectionColumnErrorV1::Work)?;
                    let valid = match original_agent_signer_is_owner_v1(owner_key, key, generation)
                    {
                        Ok(true) => true,
                        Err(_) => false,
                        Ok(false) => {
                            if let Some(binding) = binding {
                                let identity_generation =
                                    number(&identity, "generation", &memory, &shared)
                                        .map_err(ProjectionColumnErrorV1::Work)?;
                                let revoked = member(&delegate, "revoked", &memory, &shared)
                                    .map_err(ProjectionColumnErrorV1::Work)?
                                    .bool_data()
                                    .ok_or_else(|| {
                                        ProjectionColumnErrorV1::Work(invalid(
                                            "actual delegation revoked source",
                                        ))
                                    })?;
                                let source = OriginalDelegationBindingDataV1 {
                                    revoked,
                                    delegate_public_key: key,
                                    generation,
                                    owner_binding: text(
                                        &delegate,
                                        "owner_binding",
                                        &memory,
                                        &shared,
                                    )
                                    .map_err(ProjectionColumnErrorV1::Work)?,
                                    agent_identity_generation: number(
                                        &delegate,
                                        "agent_identity_generation",
                                        &memory,
                                        &shared,
                                    )
                                    .map_err(ProjectionColumnErrorV1::Work)?,
                                    world_id: text(&delegate, "world_id", &memory, &shared)
                                        .map_err(ProjectionColumnErrorV1::Work)?,
                                    branch_id: text(&delegate, "branch_id", &memory, &shared)
                                        .map_err(ProjectionColumnErrorV1::Work)?,
                                };
                                shared
                                    .borrow_mut()
                                    .charge(
                                        8,
                                        (source.delegate_public_key.len()
                                            + key.len()
                                            + source.owner_binding.len()
                                            + identity_binding.len()
                                            + source.world_id.len()
                                            + binding.world_id.len()
                                            + source.branch_id.len()
                                            + binding.branch_id.len())
                                            as u64,
                                    )
                                    .map_err(paid)
                                    .map_err(ProjectionColumnErrorV1::Work)?;
                                validate_original_delegation_fields_v1(
                                    source,
                                    key,
                                    generation,
                                    identity_binding,
                                    identity_generation,
                                    &binding.world_id,
                                    &binding.branch_id,
                                )
                                .is_ok()
                            } else {
                                false
                            }
                        }
                    };
                    if valid {
                        result
                            .add(agent, key, &mut shared.borrow_mut())
                            .map_err(ProjectionColumnErrorV1::Work)?
                    }
                }
                if result.values.len() + 1 > MAX_PROJECTION_SCOPES {
                    return Err(ProjectionColumnErrorV1::Work(invalid(
                        "formal projection audience budget",
                    )));
                }
                Ok(())
            },
        )
        .map_err(error)?;
    let common = shared.into_inner();
    origin.validate_metadata(metadata, common)?;
    Ok(result)
}
