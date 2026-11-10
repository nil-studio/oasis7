//! Genuine signed CollectData owner: real Current auth/nonce, resources and original publication.
use super::*;
#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn execute_selected_authenticated_collect_data_v1<
    M,
    P,
    AP,
    V,
    W,
    E,
    C,
    PE,
    PC,
    AE,
    AC,
    D,
    F,
>(
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    materials: &mut crate::world::incremental_body_event::materials::CorePolicyMaterialContextV1<
        '_,
        M,
        impl FnMut() -> Option<ValueTreeError>,
    >,
    selected: &SelectedPendingActionV1,
    outcome: &ReservedValue<super::super::incremental_module_invocation::RootRulePhaseOutcomeV1>,
    agents: &ReservedValue<AgentSubtreeStructureQualificationV1>,
    roots: &RootSetV1,
    read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    parent_read: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    index: &ReservedValue<oasis7_world_state::incremental_world_native::StarterBindingIndexV1>,
    controls: &ReservedValue<oasis7_world_state::incremental_world_collection_controls::WorldCollectionControlsQualificationV1>,
    auth_read: &mut WorldFieldReadContextV1<'_, AP, V, AE, AC>,
    auth_work: &mut auth_current::CollectDataAuthorizationWorkV1,
    work: &mut CollectDataWorkV1,
    audit_work: &mut WorldEventDecodeWorkV1,
    max_event_bytes: usize,
    guard: &PreparedGuard,
    live: &ExecutionFence,
    mut persist_original: F,
) -> Result<CompletedCollectDataV1, BodyMutationError>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeWriter,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
    AP: MerkleNodeReader,
    AE: FnMut() -> Option<ValueTreeError>,
    AC: FnMut() -> bool,
    D: FnMut() -> bool,
    F: FnMut(&mut dyn AgentEventOriginalV1) -> Result<(), BodyMutationError>,
{
    let result = (|| {
        step.transaction.validate_guard(guard, live)?;
        selected.validate(step.transaction)?;
        if !outcome.uses_budget(step.mutation.memory)
            || read.values.class != MemoryClass::Ordinary
            || parent_read.values.class != MemoryClass::Ordinary
            || !read.values.memory.same_pool(step.mutation.memory)
            || !parent_read.values.memory.same_pool(step.mutation.memory)
            || !step.transaction.uses_budget(step.mutation.memory)
            || !agents.uses_budget(step.mutation.memory)
        {
            return Err(BodyMutationError::Invalid(
                "CollectData selected-action pool/class",
            ));
        }
        read.check()?;
        parent_read.check()?;

        if !matches!(
            &selected.envelope().action,
            crate::Action::CollectDataAuthenticated { .. }
        ) {
            return Err(BodyMutationError::Invalid(
                "authenticated CollectData original family",
            ));
        }
        let replacement = outcome.override_action_for_selected_v1(selected, step.transaction)?;
        if outcome.decision.verdict == crate::RuleVerdict::Deny {
            return Err(BodyMutationError::Invalid(
                "CollectData denied by Rule outcome",
            ));
        }
        let action = replacement.unwrap_or(&selected.envelope().action);
        let crate::Action::CollectDataAuthenticated {
            collector_agent_id,
            electricity_cost,
            data_amount,
            player_id,
            public_key,
            nonce,
            signature,
        } = action
        else {
            return Err(BodyMutationError::Invalid(
                "selected CollectData action kind",
            ));
        };
        work.spend(collector_agent_id.len())?;
        match auth_current::authorize_selected_collect_data_v1(
            selected,
            outcome,
            index,
            controls,
            roots,
            step.transaction,
            auth_read,
            auth_work,
            live,
        )? {
            auth_current::SelectedCollectDataAuthorizationV1::Authorized(_proof) => {}
            auth_current::SelectedCollectDataAuthorizationV1::Rejected(reason) => {
                let owner_fee =
                    reserve_completed_fee(step.mutation.memory, collector_agent_id.len())?;
                let note_bytes = match &*reason {
                    crate::RejectReason::RuleDenied { notes } => {
                        notes.iter().try_fold(512usize, |n, note| {
                            n.checked_add(note.len())
                                .and_then(|n| n.checked_add(std::mem::size_of::<String>()))
                        })
                    }
                    _ => None,
                }
                .ok_or(ValueTreeError::ResourceLimited(
                    "CollectData auth rejection allocation",
                ))?;
                let _reason_copy = step
                    .mutation
                    .memory
                    .reserve(note_bytes, MemoryClass::Ordinary)
                    .map_err(ValueTreeError::Memory)?;
                let audit = step.append_selected_rejection_owned(
                    materials,
                    live,
                    selected,
                    (*reason).clone(),
                    max_event_bytes,
                    audit_work,
                    &mut persist_original,
                )?;
                return Ok(CompletedCollectDataV1 {
                    owner: CompletedCollectDataOwnerV1::Rejection(audit),
                    _fee: owner_fee,
                });
            }
        }

        let presence = {
            let mut overlay = WorldFieldOverlayContextV1 {
                transaction: step.transaction,
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
                cancelled: || (read.cancelled)() || (step.mutation.cancelled)(),
            };
            let guard = overlay.transaction.guard();
            read_current_agent_presence_v1(
                collector_agent_id,
                agents,
                roots,
                &mut overlay,
                parent_read,
                &mut work.pending_entries_remaining,
                &guard,
                live,
            )
            .map_err(overlay_error)?
        };
        if presence != CurrentAgentPresenceV1::Existing {
            let owner_fee = reserve_completed_fee(step.mutation.memory, collector_agent_id.len())?;
            let audit = step.append_selected_rejection_owned(
                materials,
                live,
                selected,
                crate::RejectReason::AgentNotFound {
                    agent_id: collector_agent_id.clone(),
                },
                max_event_bytes,
                audit_work,
                &mut persist_original,
            )?;
            return Ok(CompletedCollectDataV1 {
                owner: CompletedCollectDataOwnerV1::Rejection(audit),
                _fee: owner_fee,
            });
        }
        if *electricity_cost <= 0 {
            return reject_invalid_amount(
                step,
                materials,
                live,
                selected,
                *electricity_cost,
                max_event_bytes,
                audit_work,
                &mut persist_original,
            );
        }
        if *data_amount <= 0 {
            return reject_invalid_amount(
                step,
                materials,
                live,
                selected,
                *data_amount,
                max_event_bytes,
                audit_work,
                &mut persist_original,
            );
        }

        let electricity_before =
            read_resource(step, collector_agent_id, agents, read, work, 0, live)?;
        let electricity = electricity_before.unwrap_or(0);
        if electricity < *electricity_cost {
            let owner_fee = reserve_completed_fee(step.mutation.memory, collector_agent_id.len())?;
            let audit = step.append_selected_rejection_owned(
                materials,
                live,
                selected,
                crate::RejectReason::InsufficientResource {
                    agent_id: collector_agent_id.clone(),
                    kind: oasis7_agent_api::world_action::ResourceKind::Electricity,
                    requested: *electricity_cost,
                    available: electricity,
                },
                max_event_bytes,
                audit_work,
                &mut persist_original,
            )?;
            return Ok(CompletedCollectDataV1 {
                owner: CompletedCollectDataOwnerV1::Rejection(audit),
                _fee: owner_fee,
            });
        }

        let data_before = read_resource(step, collector_agent_id, agents, read, work, 1, live)?;
        let (electricity_after, data_after) = collection_balances(
            electricity,
            *electricity_cost,
            data_before.unwrap_or(0),
            *data_amount,
            step.mutation.memory,
        )?;

        let owner_fee = reserve_completed_fee(step.mutation.memory, collector_agent_id.len())?;
        let sequence =
            step.read_allocator(super::super::incremental_allocators::RuntimeAllocatorV1::Event)?;
        let (event_id, _, _) = World::preview_next_event_id(sequence.next, sequence.era);
        let event_bytes = std::mem::size_of::<crate::WorldEvent>()
            .checked_add(collector_agent_id.len())
            .and_then(|n| n.checked_add(player_id.len()))
            .and_then(|n| n.checked_add(public_key.len()))
            .and_then(|n| n.checked_add(signature.len()))
            .ok_or(ValueTreeError::ResourceLimited(
                "CollectData event allocation",
            ))?;
        let event_build_fee = step
            .mutation
            .memory
            .reserve(event_bytes, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        // Reauthenticate the actual Current claim and nonce after all business
        // preflight reads, before any business write; this mirrors the legacy
        // publication owner's independent auth check and preserves strict guards.
        let proof = match auth_current::authorize_selected_collect_data_v1(
            selected,
            outcome,
            index,
            controls,
            roots,
            step.transaction,
            auth_read,
            auth_work,
            live,
        )? {
            auth_current::SelectedCollectDataAuthorizationV1::Authorized(proof) => proof,
            auth_current::SelectedCollectDataAuthorizationV1::Rejected(_) => {
                return Err(BodyMutationError::Invalid(
                    "CollectData auth changed during preflight",
                ));
            }
        };
        {
            let mut overlay = WorldFieldOverlayContextV1 {
                transaction: step.transaction,
                values: ValueReadContext {
                    source: auth_read.values.source,
                    memory: auth_read.values.memory,
                    class: auth_read.values.class,
                    io: &mut *auth_read.values.io,
                },
                key_work: &mut *auth_read.key_work,
                semantic_bytes: auth_read.semantic_bytes,
                record_bytes: auth_read.record_bytes,
                take_state_error: &mut auth_read.take_state_error,
                cancelled: &mut auth_read.cancelled,
            };
            let mut writer =
                oasis7_world_state::incremental_field_member::MemberRecordWriteContext {
                    sink: &mut *step.mutation.sink,
                    memory: step.mutation.memory,
                    io: &mut *step.mutation.io,
                    key_work: &mut *read.key_work,
                    limits: oasis7_world_state::incremental_field_member::MemberRecordLimitsV1 {
                        semantic_bytes: auth_read.semantic_bytes,
                        payload_bytes: 16,
                        record_bytes: auth_read.record_bytes,
                    },
                    cancelled: &mut step.mutation.cancelled,
                };
            proof.consume_nonce(selected, outcome, &mut overlay, &mut writer, live)?;
        }
        let collector_copy = collector_agent_id.clone();
        if collector_copy.capacity() > collector_agent_id.len() {
            return Err(ValueTreeError::ResourceLimited("CollectData collector capacity").into());
        }
        let player_copy = player_id.clone();
        let key_copy = public_key.clone();
        let signature_copy = signature.clone();
        if player_copy.capacity() > player_id.len()
            || key_copy.capacity() > public_key.len()
            || signature_copy.capacity() > signature.len()
        {
            return Err(ValueTreeError::ResourceLimited("CollectData auth field capacity").into());
        }
        let event = crate::WorldEvent {
            id: event_id,
            time: step.time(),
            caused_by: Some(crate::CausedBy::Action(selected.envelope().id)),
            body: crate::WorldEventBody::Domain(crate::DomainEvent::DataCollectedAuthenticated {
                collector_agent_id: collector_copy,
                electricity_cost: *electricity_cost,
                data_amount: *data_amount,
                player_id: player_copy,
                public_key: key_copy,
                nonce: *nonce,
                signature: signature_copy,
            }),
        };
        let event = oasis7_world_state::incremental_world_event_owned::retain_world_event_owned_v1(
            event,
            step.mutation.memory,
            MemoryClass::Ordinary,
            audit_work,
            &mut step.mutation.cancelled,
        )
        .map_err(BodyMutationError::Value)?;
        drop(event_build_fee);
        let prepared = PreparedCollectDataV1 {
            event,
            action_id: selected.envelope().id,
            now: step.time(),
            electricity_before,
            data_before,
            electricity_after,
            data_after,
            identity: step.transaction.guard(),
        };
        prepared.validate(step.transaction, step.mutation.memory, step.time())?;
        let before = step.begin_world_event()?;
        event::sink_prepared_collect_data_v1(
            step,
            materials,
            &prepared,
            agents,
            read,
            work,
            max_event_bytes,
            live,
            &mut persist_original,
        )?;
        step.finish_world_event(before)?;
        if (step.mutation.cancelled)()
            || (read.cancelled)()
            || (parent_read.cancelled)()
            || (auth_read.cancelled)()
        {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(CompletedCollectDataV1 {
            owner: CompletedCollectDataOwnerV1::Collected {
                event: prepared.event,
                identity: step.transaction.guard(),
            },
            _fee: owner_fee,
        })
    })();
    let result = result.and_then(|completed| {
        if (step.mutation.cancelled)()
            || (read.cancelled)()
            || (parent_read.cancelled)()
            || (auth_read.cancelled)()
        {
            Err(ValueTreeError::Cancelled.into())
        } else {
            Ok(completed)
        }
    });
    if result.is_err() {
        let _: Result<(), ExecutionError> = step
            .transaction
            .contribute(|_| Err(ExecutionError::Aborted));
    }
    result
}
