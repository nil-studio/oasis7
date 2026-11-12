//! Fixed original registration selection/integrity and externality predicates.
//! Only resident metadata selects pages; only selected Current rows are decoded.
use super::*;
use oasis7_formal_protocol::{
    incremental_state::StatePageWriter, incremental_value::ValueNodeWriter,
};
use oasis7_world_state::{
    incremental_field_member::MemberRecordWriteContext,
    incremental_world_rule_composite_family::{
        CurrentContinuityContractSourcesV1, observe_current_continuity_contract_sources_v1,
    },
    incremental_world_rule_continuity::{
        apply_prepared_continuity_settlement_v1, prepare_continuity_settlement_pause_v1,
        prepare_continuity_settlement_receipt_v1, read_current_continuity_settlement_row_v1,
    },
};
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Skip,
    Pause,
    Receipt,
}
struct Assessment {
    origin: String,
    target: String,
    kind: Kind,
}
pub(super) struct ContinuityPlanV1 {
    _selected: ReservedValue<CurrentContinuityContractSourcesV1>,
    assessments: ReservedValue<Vec<Assessment>>,
    identity: PreparedGuard,
    core: WorkIdentityV1,
    memory: MemoryBudget,
}
fn error(
    e: oasis7_world_state::incremental_world_rule_continuity::ContinuityMembershipErrorV1,
) -> BodyMutationError {
    BodyMutationError::Original(Box::new(e))
}
fn equal(
    a: &str,
    b: &str,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<bool, BodyMutationError> {
    charge(common, a.len().checked_add(b.len()).ok_or_else(limited)?)?;
    Ok(a == b)
}
fn binding_equal(
    a: &oasis7_world_state::world_rules::OrganizationControllerBindingV1,
    b: &oasis7_world_state::world_rules::OrganizationControllerBindingV1,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<bool, BodyMutationError> {
    for (a, b) in [
        (&a.root_agent_id, &b.root_agent_id),
        (&a.player_id, &b.player_id),
        (&a.source_id, &b.source_id),
    ] {
        if !equal(a, b, common)? {
            return Ok(false);
        }
    }
    charge(common, 2 * size_of::<u64>())?;
    Ok(a.generation == b.generation)
}
#[allow(clippy::too_many_arguments)]
fn assess<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    V: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
>(
    origin: &str,
    target: &str,
    record: &EconomicContractState,
    world_binding: Option<&str>,
    sources: &ContractSourcesV1<'_, '_>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    read: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    work: &mut ContractWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    now: u64,
    live: &ExecutionFence,
) -> Result<Kind, BodyMutationError> {
    let guard = context.transaction.guard();
    let original = read_current_continuity_settlement_row_v1(
        sources.shared.continuity,
        sources.shared.roots,
        origin,
        read.state,
        context,
        &mut work.shared.scope_scan,
        &mut work.shared.continuity_read,
        &guard,
        live,
    )
    .map_err(overlay)?;
    let Some(original) = original else {
        return Ok(Kind::Skip);
    };
    charge(
        common,
        original.registration().world_binding.len() + world_binding.map(str::len).unwrap_or(0),
    )?;
    if world_binding != Some(original.registration().world_binding.as_str()) {
        return Ok(Kind::Skip);
    }
    // The original selection uses registration values' embedded ID, then performs
    // a second map lookup using that ID. Do not substitute the outer map key.
    for id in original.registration().activity_sources.keys() {
        charge(
            common,
            id.len()
                .checked_add(record.contract_id.len())
                .ok_or_else(limited)?,
        )?;
    }
    if !original
        .registration()
        .activity_sources
        .contains_key(&record.contract_id)
    {
        return Ok(Kind::Skip);
    }
    drop(original);
    let guard = context.transaction.guard();
    let current = read_current_continuity_settlement_row_v1(
        sources.shared.continuity,
        sources.shared.roots,
        target,
        read.state,
        context,
        &mut work.shared.scope_scan,
        &mut work.shared.continuity_read,
        &guard,
        live,
    )
    .map_err(overlay)?;
    let Some(current) = current else {
        return Ok(Kind::Skip);
    };
    let registration = current.registration();
    for id in registration.activity_sources.keys() {
        charge(
            common,
            id.len()
                .checked_add(record.contract_id.len())
                .ok_or_else(limited)?,
        )?;
    }
    let Some(source) = registration.activity_sources.get(&record.contract_id) else {
        return Ok(Kind::Pause);
    };
    let terms = hash::fixed_json_digest_v1(
        &(
            "oasis7.organization-continuity.atomic-exchange-terms.v1",
            &record.contract_id,
            &record.creator_agent_id,
            &record.counterparty_agent_id,
            record.fulfillment_kind,
            record.settlement_kind,
            record.settlement_amount,
            record.reputation_stake,
            record.expires_at,
            &record.description,
        ),
        common,
    )?;
    let terms_match = equal(&terms, &source.terms_hash, common)?;
    let controller_sources = controller::ControllerSourcesV1 {
        roots: sources.shared.roots,
        orders: sources.shared.orders,
        agents: sources.shared.agents,
        native: sources.shared.native,
    };
    let creator = controller::resolve_current_controller_v1(
        &record.creator_agent_id,
        &controller_sources,
        context,
        read,
        &mut work.shared.controller,
        common,
        live,
    )?;
    let creator_matches = if let Some(owner) = creator {
        owner.validate(context.transaction, common)?;
        let value = owner.value();
        !value.root_agent_id.is_empty()
            && !value.player_id.is_empty()
            && !value.source_id.is_empty()
            && value.generation > 0
            && binding_equal(value, &source.creator_binding, common)?
    } else {
        false
    };
    let counterparty = controller::resolve_current_controller_v1(
        &record.counterparty_agent_id,
        &controller_sources,
        context,
        read,
        &mut work.shared.controller,
        common,
        live,
    )?;
    let counterparty_matches = if let Some(owner) = counterparty {
        owner.validate(context.transaction, common)?;
        let value = owner.value();
        !value.root_agent_id.is_empty()
            && !value.player_id.is_empty()
            && !value.source_id.is_empty()
            && value.generation > 0
            && binding_equal(value, &source.counterparty_binding, common)?
    } else {
        false
    };
    // This lookup intentionally uses registration.organization_id, independently
    // of both registration outer and the ID used for its second map lookup.
    let guard = context.transaction.guard();
    let alliance = open_current_diplomacy_relation_record_v1(
        DiplomacyFieldV1::Alliances,
        &registration.organization_id,
        sources.shared.diplomacy,
        sources.shared.roots,
        context.transaction,
        read,
        &mut work.shared.index,
        &guard,
        live,
    )
    .map_err(overlay)?;
    let external = if let Some(proof) = alliance {
        let alliance = super::super::read::read_alliance_v1(
            &registration.organization_id,
            &proof,
            read,
            work.shared.max_text_bytes,
            common,
        )?;
        let counterparty = controller::resolve_current_controller_v1(
            &record.counterparty_agent_id,
            &controller_sources,
            context,
            read,
            &mut work.shared.controller,
            common,
            live,
        )?;
        if let Some(counterparty) = counterparty {
            let mut external = true;
            for member in &alliance.members {
                charge(common, member.len())?;
                let member = controller::resolve_current_controller_v1(
                    member,
                    &controller_sources,
                    context,
                    read,
                    &mut work.shared.controller,
                    common,
                    live,
                )?;
                if let Some(member) = member {
                    if !(!equal(
                        &member.value().player_id,
                        &counterparty.value().player_id,
                        common,
                    )? && !equal(
                        &member.value().root_agent_id,
                        &counterparty.value().root_agent_id,
                        common,
                    )?) {
                        external = false;
                        break;
                    }
                } else {
                    external = false;
                    break;
                }
            }
            external
        } else {
            false
        }
    } else {
        false
    };
    let same_world = equal(&source.world_binding, &registration.world_binding, common)?;
    let distinct = !equal(
        &record.creator_agent_id,
        &record.counterparty_agent_id,
        common,
    )?;
    charge(common, 5 * size_of::<u64>())?;
    Ok(
        if same_world
            && source.registered_at_world_tick <= now
            && distinct
            && record.expires_at >= now
            && record.transfer_amount == record.settlement_amount
            && terms_match
            && creator_matches
            && counterparty_matches
            && external
        {
            Kind::Receipt
        } else {
            Kind::Pause
        },
    )
}
#[allow(clippy::too_many_arguments)]
pub(super) fn prepare_continuity_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    V: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
>(
    record: &EconomicContractState,
    body: &crate::WorldEventBody,
    sources: &ContractSourcesV1<'_, '_>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    read: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    work: &mut ContractWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    now: u64,
    live: &ExecutionFence,
) -> Result<ContinuityPlanV1, BodyMutationError> {
    let crate::WorldEventBody::Domain(crate::DomainEvent::EconomicContractSettled {
        world_binding,
        ..
    }) = body
    else {
        return Err(bad());
    };
    let guard = context.transaction.guard();
    let selected = observe_current_continuity_contract_sources_v1(
        &record.contract_id,
        sources.shared.continuity,
        sources.shared.roots,
        read.state,
        context,
        &mut work.shared.scope_scan,
        &mut work.shared.continuity_read,
        &guard,
        live,
    )
    .map_err(overlay)?;
    let mut n = 0usize;
    let mut bytes = size_of::<Vec<Assessment>>();
    for (origin, target) in selected.registrations() {
        charge(
            common,
            origin.len().checked_add(target.len()).ok_or_else(limited)?,
        )?;
        n = n.checked_add(1).ok_or_else(limited)?;
        bytes = bytes
            .checked_add(size_of::<Assessment>())
            .and_then(|n| n.checked_add(origin.len()))
            .and_then(|n| n.checked_add(target.len()))
            .ok_or_else(limited)?;
    }
    let fee = common
        .memory()
        .reserve(bytes, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let mut assessments = Vec::with_capacity(n);
    if assessments.capacity() > n {
        return Err(limited());
    }
    for (origin, target) in selected.registrations() {
        assessments.push(Assessment {
            origin: copy(origin, common)?,
            target: copy(target, common)?,
            kind: Kind::Skip,
        });
    }
    // Original BTreeMap values are visited in original outer-key order. Resident
    // insertion history is not that order; use a charged in-place stable sort.
    for i in 1..assessments.len() {
        let mut j = i;
        while j > 0 {
            charge(
                common,
                assessments[j - 1]
                    .origin
                    .len()
                    .checked_add(assessments[j].origin.len())
                    .ok_or_else(limited)?,
            )?;
            if assessments[j - 1].origin <= assessments[j].origin {
                break;
            }
            assessments.swap(j - 1, j);
            j -= 1;
        }
    }
    for row in &mut assessments {
        row.kind = assess(
            &row.origin,
            &row.target,
            record,
            world_binding.as_deref(),
            sources,
            context,
            read,
            work,
            common,
            now,
            live,
        )?;
    }
    Ok(ContinuityPlanV1 {
        _selected: selected,
        assessments: fee.retain(assessments),
        identity: context.transaction.guard(),
        core: common.work().identity(),
        memory: common.memory().clone(),
    })
}
#[allow(clippy::too_many_arguments)]
pub(super) fn apply_continuity_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    V: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    CW: FnMut() -> bool,
>(
    plan: &ContinuityPlanV1,
    record: &EconomicContractState,
    body: &crate::WorldEventBody,
    sources: &ContractSourcesV1<'_, '_>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    read: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    work: &mut ContractWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    now: u64,
    live: &ExecutionFence,
) -> Result<(), BodyMutationError> {
    context
        .transaction
        .validate_transaction_identity(&plan.identity)?;
    if plan.core != common.work().identity() || !plan.memory.same_pool(common.memory()) {
        return Err(bad());
    }
    let crate::WorldEventBody::Domain(crate::DomainEvent::EconomicContractSettled {
        world_binding,
        ..
    }) = body
    else {
        return Err(bad());
    };
    // Each mutation is prepared from the fresh selected branch after the preceding
    // private mutation, preserving duplicate embedded IDs and receipt dedup.
    for assessment in plan.assessments.iter() {
        let actual = assess(
            &assessment.origin,
            &assessment.target,
            record,
            world_binding.as_deref(),
            sources,
            context,
            read,
            work,
            common,
            now,
            live,
        )?;
        if actual != assessment.kind {
            return Err(bad());
        }
        if actual == Kind::Skip {
            continue;
        }
        let guard = context.transaction.guard();
        let source = read_current_continuity_settlement_row_v1(
            sources.shared.continuity,
            sources.shared.roots,
            &assessment.target,
            read.state,
            context,
            &mut work.shared.scope_scan,
            &mut work.shared.continuity_read,
            &guard,
            live,
        )
        .map_err(overlay)?
        .ok_or_else(bad)?;
        let prepared = if actual == Kind::Pause {
            prepare_continuity_settlement_pause_v1(source, context.transaction, common, live)
                .map_err(error)?
        } else {
            prepare_continuity_settlement_receipt_v1(
                source,
                &record.contract_id,
                now,
                context,
                common,
                live,
            )
            .map_err(error)?
        };
        let guard = context.transaction.guard();
        apply_prepared_continuity_settlement_v1(
            prepared,
            sources.shared.continuity,
            sources.shared.roots,
            read.state,
            context,
            write,
            &mut work.shared.scope_scan,
            &mut work.shared.continuity_read,
            &mut work.shared.continuity_write,
            common,
            &guard,
            live,
        )
        .map_err(error)?;
    }
    Ok(())
}
