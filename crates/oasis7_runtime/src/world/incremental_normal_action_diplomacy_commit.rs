//! Genuine Accepted -> Current CAS -> private Alliance event capability.
use super::*;
use crate::world::{
    incremental_allocators::RuntimeAllocatorV1,
    incremental_body::write_body_value,
    incremental_body_event::{
        AgentEventOriginalV1, materials::CorePolicyMaterialContextV1, read_fixed,
    },
    incremental_step::IwsStepContext,
};
use oasis7_formal_protocol::{
    incremental_state::{StateNamespaceV1, StatePageWriter},
    incremental_value::{ValueNodeWriter, ValueReadContext},
};
use oasis7_world_state::{
    incremental_agent_body::{AGENT_BODY_VALUE_BYTES, AgentBodyKeyV1, AgentBodyValueV1},
    incremental_agent_subtree::{CurrentAgentPresenceV1, read_current_agent_presence_v1},
    incremental_diplomacy::*,
    incremental_field_member::MemberRecordWriteContext,
    incremental_field_overlay::WorldFieldOverlayContextV1,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_world_event_decode::WorldEventDecodeWorkV1,
    incremental_world_rule_continuity::apply_prepared_continuity_membership_v1,
};
fn text(a: &str, b: &str, c: &mut FormalBoundedContextV1<'_>) -> Result<bool, BodyMutationError> {
    charge(c, a.len().checked_add(b.len()).ok_or_else(limited)?)?;
    Ok(a == b)
}
fn list(
    a: &[String],
    b: &[String],
    c: &mut FormalBoundedContextV1<'_>,
) -> Result<bool, BodyMutationError> {
    charge(c, 2 * size_of::<usize>())?;
    if a.len() != b.len() {
        return Ok(false);
    }
    for (a, b) in a.iter().zip(b) {
        if !text(a, b, c)? {
            return Ok(false);
        }
    }
    Ok(true)
}
fn same(
    a: &crate::WorldEventBody,
    b: &crate::WorldEventBody,
    c: &mut FormalBoundedContextV1<'_>,
) -> Result<bool, BodyMutationError> {
    use crate::DomainEvent as D;
    match (a, b) {
        (
            crate::WorldEventBody::Domain(D::AllianceFormed {
                proposer_agent_id: a,
                alliance_id: ai,
                members: am,
                charter: ac,
            }),
            crate::WorldEventBody::Domain(D::AllianceFormed {
                proposer_agent_id: b,
                alliance_id: bi,
                members: bm,
                charter: bc,
            }),
        ) => Ok(text(a, b, c)? && text(ai, bi, c)? && list(am, bm, c)? && text(ac, bc, c)?),
        (
            crate::WorldEventBody::Domain(D::AllianceJoined {
                operator_agent_id: a,
                alliance_id: ai,
                member_agent_id: am,
            }),
            crate::WorldEventBody::Domain(D::AllianceJoined {
                operator_agent_id: b,
                alliance_id: bi,
                member_agent_id: bm,
            }),
        )
        | (
            crate::WorldEventBody::Domain(D::AllianceLeft {
                operator_agent_id: a,
                alliance_id: ai,
                member_agent_id: am,
            }),
            crate::WorldEventBody::Domain(D::AllianceLeft {
                operator_agent_id: b,
                alliance_id: bi,
                member_agent_id: bm,
            }),
        ) => Ok(text(a, b, c)? && text(ai, bi, c)? && text(am, bm, c)?),
        (
            crate::WorldEventBody::Domain(D::AllianceDissolved {
                operator_agent_id: a,
                alliance_id: ai,
                reason: ar,
                former_members: am,
            }),
            crate::WorldEventBody::Domain(D::AllianceDissolved {
                operator_agent_id: b,
                alliance_id: bi,
                reason: br,
                former_members: bm,
            }),
        ) => Ok(text(a, b, c)? && text(ai, bi, c)? && text(ar, br, c)? && list(am, bm, c)?),
        _ => Ok(false),
    }
}
#[allow(clippy::too_many_arguments)]
fn touch<M, P, V, R, W, D, E, C, PE, PC>(
    id: &str,
    required: bool,
    sources: &AllianceSourcesV1<'_, '_>,
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    read: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, R, PE, PC>,
    work: &mut AllianceWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<(), BodyMutationError>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    R: ValueNodeReader,
    W: ValueNodeReader + ValueNodeWriter,
    D: FnMut() -> bool,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
{
    charge(common, id.len())?;
    let exists = {
        let mut context = WorldFieldOverlayContextV1 {
            transaction: &mut *step.transaction,
            values: ValueReadContext {
                source: read.values.source,
                memory: read.values.memory,
                class: read.values.class,
                io: &mut *read.values.io,
            },
            key_work: &mut *read.key_work,
            semantic_bytes: read.semantic_bytes,
            record_bytes: read.record_bytes,
            take_state_error: &mut read.take_state_error,
            cancelled: &mut read.cancelled,
        };
        let guard = context.transaction.guard();
        read_current_agent_presence_v1(
            id,
            sources.agents,
            sources.roots,
            &mut context,
            parent,
            &mut work.pending_entries_remaining,
            &guard,
            live,
        )
        .map_err(overlay)?
            == CurrentAgentPresenceV1::Existing
    };
    if !exists {
        return if required {
            Err(original(crate::WorldError::AgentNotFound {
                agent_id: id.to_owned(),
            }))
        } else {
            Ok(())
        };
    }
    let key = AgentBodyKeyV1::new(id).map_err(BodyMutationError::Invalid)?;
    let before =
        AgentBodyValueV1::from_canonical_bytes(&read_fixed::<AGENT_BODY_VALUE_BYTES, _, _, _, _>(
            step.transaction,
            StateNamespaceV1::Business,
            key.as_bytes(),
            step.values,
            step.mutation,
        )?)
        .map_err(BodyMutationError::Invalid)?;
    let after = write_body_value(
        &AgentBodyValueV1 {
            body_view: before.body_view,
            last_active: step.time(),
        },
        step.mutation,
    )?;
    step.transaction
        .write(StateNamespaceV1::Business, key.as_bytes(), Some(after))?;
    Ok(())
}
#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn commit_prepared_alliance_action_v1<
    M,
    P,
    V,
    R,
    W,
    D,
    E,
    C,
    PE,
    PC,
    CW,
    ME,
    F,
>(
    prepared: PreparedAllianceActionV1<'_, '_, '_>,
    accepted: SelectedAcceptedDispositionV1,
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    read: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, R, PE, PC>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    work: &mut AllianceWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    materials: &mut CorePolicyMaterialContextV1<'_, M, ME>,
    max_event_bytes: usize,
    audit: &mut WorldEventDecodeWorkV1,
    live: &ExecutionFence,
    persist: &mut F,
) -> Result<CompletedAllianceActionV1, BodyMutationError>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    R: ValueNodeReader,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    D: FnMut() -> bool,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
    CW: FnMut() -> bool,
    ME: FnMut() -> Option<ValueTreeError>,
    F: FnMut(&mut dyn AgentEventOriginalV1) -> Result<(), BodyMutationError>,
{
    let result = (|| {
        charge(common, 0)?;
        step.transaction
            .validate_transaction_identity(&prepared.identity)?;
        accepted.validate(prepared.selected, step.transaction, step.mutation.memory)?;
        if prepared.core != common.work().identity()
            || !prepared.memory.same_pool(common.memory())
            || !prepared.memory.same_pool(step.mutation.memory)
            || prepared.time != step.time()
        {
            return Err(bad());
        }
        let action = prepared
            .outcome
            .override_action_for_selected_v1(prepared.selected, step.transaction)?
            .unwrap_or(&prepared.selected.envelope().action);
        let PreparedAllianceActionV1 {
            selected,
            sources,
            body,
            before,
            proof,
            created,
            after_members,
            membership,
            fees,
            verification,
            event_fee,
            layout,
            ..
        } = prepared;
        if let crate::WorldEventBody::Domain(crate::DomainEvent::ActionRejected {
            reason, ..
        }) = body
        {
            return step
                .append_selected_rejection_owned(
                    materials,
                    live,
                    selected,
                    reason,
                    max_event_bytes,
                    audit,
                    &mut *persist,
                )
                .map(|event| CompletedAllianceActionV1::Rejected {
                    accepted: accepted.into_audit(),
                    event,
                    _fees: fees,
                    _layout: layout,
                });
        }
        {
            let mut context = WorldFieldOverlayContextV1 {
                transaction: &mut *step.transaction,
                values: ValueReadContext {
                    source: read.values.source,
                    memory: read.values.memory,
                    class: read.values.class,
                    io: &mut *read.values.io,
                },
                key_work: &mut *read.key_work,
                semantic_bytes: read.semantic_bytes,
                record_bytes: read.record_bytes,
                take_state_error: &mut read.take_state_error,
                cancelled: &mut read.cancelled,
            };
            // The Accepted event may change the actor/materials. Re-evaluate actual
            // Current predicates once, using the already-admitted output workspaces.
            // Its separate reader borrows the same original source and live counters.
            let mut business = WorldFieldReadContextV1 {
                root: sources.roots.state_root(StateNamespaceV1::Business),
                state: parent.state,
                values: ValueReadContext {
                    source: parent.values.source,
                    memory: parent.values.memory,
                    class: parent.values.class,
                    io: &mut *parent.values.io,
                },
                key_work: &mut *parent.key_work,
                semantic_bytes: parent.semantic_bytes,
                record_bytes: parent.record_bytes,
                take_state_error: &mut parent.take_state_error,
                cancelled: &mut parent.cancelled,
            };
            // A second metadata reader must not duplicate counters. The already-existing
            // caller read is used for parent point dependencies while business uses its
            // own actual borrowed IO above; source type/counters remain the caller's.
            let checked = prepare::evaluate(
                selected.envelope().id,
                action,
                sources,
                &mut context,
                &mut business,
                work,
                common,
                live,
                verification,
            )?;
            if !same(&body, &checked.body, common)? {
                return Err(bad());
            }
            drop(checked);
            let id = match &body {
                crate::WorldEventBody::Domain(
                    crate::DomainEvent::AllianceFormed { alliance_id, .. }
                    | crate::DomainEvent::AllianceJoined { alliance_id, .. }
                    | crate::DomainEvent::AllianceLeft { alliance_id, .. }
                    | crate::DomainEvent::AllianceDissolved { alliance_id, .. },
                ) => alliance_id.as_str(),
                _ => return Err(bad()),
            };
            let guard = context.transaction.guard();
            let actual = open_current_diplomacy_relation_record_v1(
                DiplomacyFieldV1::Alliances,
                id,
                sources.diplomacy,
                sources.roots,
                context.transaction,
                &mut business,
                &mut work.index,
                &guard,
                live,
            )
            .map_err(overlay)?;
            match (&proof, &actual) {
                (None, None) => {}
                (Some(before), Some(after)) => {
                    charge(
                        common,
                        before
                            .root()
                            .node_root
                            .len()
                            .checked_add(after.root().node_root.len())
                            .ok_or_else(limited)?,
                    )?;
                    if before.root() != after.root() {
                        return Err(bad());
                    }
                }
                _ => return Err(bad()),
            }
            if let Some(membership) = membership {
                let guard = context.transaction.guard();
                apply_prepared_continuity_membership_v1(
                    membership,
                    sources.continuity,
                    sources.roots,
                    business.state,
                    &mut context,
                    write,
                    &mut work.scope_scan,
                    &mut work.continuity_read,
                    &mut work.continuity_write,
                    common,
                    &guard,
                    live,
                )
                .map_err(membership_error)?;
            }
            match &body {
                crate::WorldEventBody::Domain(crate::DomainEvent::AllianceFormed { .. }) => {
                    let source = created.as_ref().ok_or_else(bad)?;
                    let guard = context.transaction.guard();
                    create_qualified_alliance_v1(
                        AllianceCreateV1 {
                            id,
                            source,
                            family: sources.diplomacy,
                            max_pending_node_bytes: work.max_pending_node_bytes,
                        },
                        &mut context,
                        write,
                        &guard,
                        live,
                        &mut work.write_cells,
                    )
                    .map_err(overlay)?;
                }
                crate::WorldEventBody::Domain(
                    crate::DomainEvent::AllianceJoined { .. }
                    | crate::DomainEvent::AllianceLeft { .. },
                ) => {
                    publish::replace_members_v1(
                        id,
                        before.as_ref().ok_or_else(bad)?,
                        after_members.as_ref().ok_or_else(bad)?,
                        actual.ok_or_else(bad)?,
                        &mut context,
                        write,
                        work.max_text_bytes as u64,
                        work.max_pending_node_bytes,
                        common,
                        live,
                    )?;
                }
                crate::WorldEventBody::Domain(crate::DomainEvent::AllianceDissolved { .. }) => {
                    let proof = actual.as_ref().ok_or_else(bad)?;
                    let old = before.as_ref().ok_or_else(bad)?;
                    let guard = context.transaction.guard();
                    delete_qualified_alliance_v1(
                        DiplomacyClosedDeleteV1 {
                            id,
                            qualification: proof,
                            expected: DiplomacySourceV1::Alliance(old),
                            max_current_payload_bytes: work.max_text_bytes as u64,
                        },
                        &mut context,
                        &guard,
                        live,
                        &mut work.write_cells,
                    )
                    .map_err(overlay)?;
                }
                _ => return Err(bad()),
            }
        }
        match &body {
            crate::WorldEventBody::Domain(crate::DomainEvent::AllianceFormed {
                proposer_agent_id,
                members,
                ..
            }) => {
                touch(
                    proposer_agent_id,
                    true,
                    sources,
                    step,
                    read,
                    parent,
                    work,
                    common,
                    live,
                )?;
                for member in members {
                    touch(
                        member, false, sources, step, read, parent, work, common, live,
                    )?;
                }
            }
            crate::WorldEventBody::Domain(
                crate::DomainEvent::AllianceJoined {
                    operator_agent_id,
                    member_agent_id,
                    ..
                }
                | crate::DomainEvent::AllianceLeft {
                    operator_agent_id,
                    member_agent_id,
                    ..
                },
            ) => {
                touch(
                    operator_agent_id,
                    true,
                    sources,
                    step,
                    read,
                    parent,
                    work,
                    common,
                    live,
                )?;
                touch(
                    member_agent_id,
                    true,
                    sources,
                    step,
                    read,
                    parent,
                    work,
                    common,
                    live,
                )?;
            }
            crate::WorldEventBody::Domain(crate::DomainEvent::AllianceDissolved {
                operator_agent_id,
                former_members,
                ..
            }) => {
                touch(
                    operator_agent_id,
                    true,
                    sources,
                    step,
                    read,
                    parent,
                    work,
                    common,
                    live,
                )?;
                let members = if former_members.is_empty() {
                    &before.as_ref().ok_or_else(bad)?.members
                } else {
                    former_members
                };
                for member in members {
                    touch(
                        member, false, sources, step, read, parent, work, common, live,
                    )?;
                }
            }
            _ => return Err(bad()),
        }
        let sequence = step.read_allocator(RuntimeAllocatorV1::Event)?;
        let (id, _, _) = crate::world::World::preview_next_event_id(sequence.next, sequence.era);
        let event = crate::WorldEvent {
            id,
            time: step.time(),
            caused_by: Some(crate::CausedBy::Action(selected.envelope().id)),
            body,
        };
        let cap = PreparedAllianceEventV1 {
            event: event_fee.retain(event),
            accepted: accepted.into_audit(),
            identity: step.transaction.guard(),
            _fees: fees,
            _layout: layout,
        };
        step.append_prepared_alliance_event_v1(
            &cap,
            materials,
            live,
            max_event_bytes,
            &mut *persist,
        )?;
        Ok(CompletedAllianceActionV1::Applied(cap))
    })();
    if result.is_err() {
        let _: Result<(), ExecutionError> = step
            .transaction
            .contribute(|_| Err(ExecutionError::Aborted));
    }
    result
}
