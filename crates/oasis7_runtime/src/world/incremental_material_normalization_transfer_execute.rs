//! Selected immediate-lane execution, using the parent private Current/event owners.
use super::*;
pub(in crate::world) fn execute_selected_material_transfer_v1<M, P, V, W, E, C, PE, PC, ME, D, F>(
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    materials: &mut CorePolicyMaterialContextV1<'_, M, ME>,
    selected: &SelectedPendingActionV1,
    outcome: &ReservedValue<crate::world::incremental_module_invocation::RootRulePhaseOutcomeV1>,
    agents: &ReservedValue<AgentSubtreeStructureQualificationV1>,
    material_family: &ReservedValue<MaterialFamilyQualificationV1>,
    read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    parent_read: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    work: &mut MaterialTransferWorkV1,
    audit_work: &mut WorldEventDecodeWorkV1,
    max_event_bytes: usize,
    guard: &PreparedGuard,
    live: &ExecutionFence,
    mut persist_original: F,
) -> Result<CompletedMaterialTransferV1, BodyMutationError>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeWriter + ValueNodeReader + StatePageWriter,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
    ME: FnMut() -> Option<ValueTreeError>,
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
            || agents.allocation_class() != MemoryClass::Ordinary
            || !material_family.uses_budget(step.mutation.memory)
            || material_family.allocation_class() != MemoryClass::Ordinary
        {
            return Err(BodyMutationError::Invalid(
                "MaterialTransfer selected-action pool/class",
            ));
        }
        read.check()?;
        parent_read.check()?;

        let replacement = outcome.override_action_for_selected_v1(selected, step.transaction)?;
        if outcome.decision.verdict == crate::RuleVerdict::Deny {
            return Err(BodyMutationError::Invalid(
                "TransferMaterial denied by Rule outcome",
            ));
        }
        let action = replacement.unwrap_or(&selected.envelope().action);
        let crate::Action::TransferMaterial {
            requester_agent_id,
            from_ledger,
            to_ledger,
            kind,
            amount,
            distance_km,
            priority,
            route_id,
            route_ids,
            auto_reroute,
        } = action
        else {
            return Err(BodyMutationError::Invalid(
                "selected TransferMaterial action kind",
            ));
        };

        work.spend(
            requester_agent_id
                .len()
                .checked_add(kind.len())
                .and_then(|n| n.checked_add(ledger_text_bytes(from_ledger)))
                .and_then(|n| n.checked_add(ledger_text_bytes(to_ledger)))
                .ok_or(ValueTreeError::ResourceLimited(
                    "material transfer identifiers",
                ))?,
        )?;
        let text_bytes = requester_agent_id
            .len()
            .checked_add(kind.len())
            .and_then(|n| n.checked_add(ledger_text_bytes(from_ledger)))
            .and_then(|n| n.checked_add(ledger_text_bytes(to_ledger)))
            .ok_or(ValueTreeError::ResourceLimited(
                "material transfer allocation bound",
            ))?;
        let _phase_fee = step
            .mutation
            .memory
            .reserve(
                text_bytes
                    .checked_mul(8)
                    .and_then(|n| n.checked_add(16 << 10))
                    .ok_or(ValueTreeError::ResourceLimited(
                        "material transfer allocation",
                    ))?,
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;

        // Preserve the original World evaluator's requester-first check and
        // rejection ordering against the actual Current Agent subtree.
        if read_agent_presence(
            step,
            requester_agent_id,
            agents,
            materials.roots,
            read,
            parent_read,
            &mut work.inventory.pending_entries_remaining,
            live,
        )? != CurrentAgentPresenceV1::Existing
        {
            return reject(
                step,
                materials,
                live,
                selected,
                crate::RejectReason::AgentNotFound {
                    agent_id: requester_agent_id.clone(),
                },
                max_event_bytes,
                audit_work,
                &mut persist_original,
            );
        }
        if from_ledger == to_ledger {
            return reject(
                step,
                materials,
                live,
                selected,
                crate::RejectReason::RuleDenied {
                    notes: vec!["from_ledger and to_ledger cannot be the same".into()],
                },
                max_event_bytes,
                audit_work,
                &mut persist_original,
            );
        }
        if kind.trim().is_empty() {
            return reject(
                step,
                materials,
                live,
                selected,
                crate::RejectReason::RuleDenied {
                    notes: vec!["material kind cannot be empty".into()],
                },
                max_event_bytes,
                audit_work,
                &mut persist_original,
            );
        }
        if *amount <= 0 {
            return reject(
                step,
                materials,
                live,
                selected,
                crate::RejectReason::InvalidAmount { amount: *amount },
                max_event_bytes,
                audit_work,
                &mut persist_original,
            );
        }
        if *distance_km < 0 {
            return reject(
                step,
                materials,
                live,
                selected,
                crate::RejectReason::RuleDenied {
                    notes: vec!["distance_km must be >= 0".into()],
                },
                max_event_bytes,
                audit_work,
                &mut persist_original,
            );
        }
        if *distance_km > crate::world::logistics::MATERIAL_TRANSFER_MAX_DISTANCE_KM {
            return reject(
                step,
                materials,
                live,
                selected,
                crate::RejectReason::MaterialTransferDistanceExceeded {
                    distance_km: *distance_km,
                    max_distance_km: crate::world::logistics::MATERIAL_TRANSFER_MAX_DISTANCE_KM,
                },
                max_event_bytes,
                audit_work,
                &mut persist_original,
            );
        }
        if *distance_km != 0 || route_id.is_some() || !route_ids.is_empty() || *auto_reroute {
            return Err(unsupported());
        }
        if !matches!(from_ledger, MaterialLedgerId::Agent(id) if id == requester_agent_id)
            || !matches!(to_ledger, MaterialLedgerId::Agent(_))
        {
            return Err(unsupported());
        }

        let inventory = open_current_material_scope(step, materials, read, live)?;
        let outcome = transfer_current_material(
            step,
            materials,
            &inventory,
            agents,
            from_ledger,
            to_ledger,
            requester_agent_id,
            kind,
            *amount,
            read,
            parent_read,
            &mut work.inventory,
            live,
        )?;
        let destination_overflow = match outcome {
            AgentLedgerTransferOutcomeV1::Applied => None,
            AgentLedgerTransferOutcomeV1::DestinationOverflow { current } => Some(current),
            AgentLedgerTransferOutcomeV1::Insufficient { available } => {
                return reject(
                    step,
                    materials,
                    live,
                    selected,
                    crate::RejectReason::InsufficientMaterial {
                        material_kind: kind.clone(),
                        requested: *amount,
                        available,
                    },
                    max_event_bytes,
                    audit_work,
                    &mut persist_original,
                );
            }
            AgentLedgerTransferOutcomeV1::RequesterAbsent => {
                return reject(
                    step,
                    materials,
                    live,
                    selected,
                    crate::RejectReason::AgentNotFound {
                        agent_id: requester_agent_id.clone(),
                    },
                    max_event_bytes,
                    audit_work,
                    &mut persist_original,
                );
            }
            AgentLedgerTransferOutcomeV1::RecipientAbsent => {
                return Err(unsupported());
            }
        };

        // Legacy ordering checks the source balance before consulting the
        // profile. The Current transfer owner has already returned a genuine
        // insufficiency without reading it; successful balances can proceed to
        // the Current profile lookup before any destination overflow is raised.
        let transit_priority = match priority {
            Some(priority) => *priority,
            None => {
                let profile = read_priority(
                    step,
                    materials,
                    material_family,
                    kind,
                    read,
                    &mut work.profile,
                    live,
                )?;
                match profile {
                    Some(MaterialDefaultPriority::Urgent) => MaterialTransitPriority::Urgent,
                    Some(MaterialDefaultPriority::Standard) => MaterialTransitPriority::Standard,
                    None if urgent_kind(kind) => MaterialTransitPriority::Urgent,
                    None => MaterialTransitPriority::Standard,
                }
            }
        };
        if let Some(current) = destination_overflow {
            return Err(material_transfer_overflow_error(
                kind,
                current,
                *amount,
                step.mutation.memory,
            )?);
        }

        // CorePolicy material normalization changes only the legacy and World
        // ledgers. Run it after the Agent-ledger point owner has finished so a
        // business rejection can use the shared rejection path's one required
        // normalization contribution rather than scanning the family twice.
        let material_guard = step.transaction.guard();
        materials.contribute(
            step.transaction,
            step.values,
            step.mutation,
            &material_guard,
            live,
        )?;

        let receipt = MaterialTransferReceiptV1 {
            transfer_id: selected.envelope().id,
            requester_agent_id: requester_agent_id.clone(),
            from_ledger: from_ledger.clone(),
            to_ledger: to_ledger.clone(),
            kind: kind.clone(),
            amount: *amount,
            distance_km: 0,
            priority: transit_priority,
            route_id: None,
        };
        let receipt_guard = step.transaction.guard();
        let read_stop = RefCell::new(&mut read.cancelled);
        let mutation_stop = RefCell::new(&mut step.mutation.cancelled);
        let mut record_context = WorldFieldOverlayContextV1 {
            transaction: step.transaction,
            values: ValueReadContext {
                source: read.values.source,
                memory: step.mutation.memory,
                class: MemoryClass::Ordinary,
                io: &mut *materials.read_io,
            },
            key_work: &mut *materials.read_keys,
            semantic_bytes: materials.semantic_bytes,
            record_bytes: materials.record_bytes,
            take_state_error: &mut materials.take_state_error,
            cancelled: || (read_stop.borrow_mut())() || (mutation_stop.borrow_mut())(),
        };
        let mut record_write = MemberRecordWriteContext {
            sink: &mut *step.mutation.sink,
            memory: step.mutation.memory,
            io: &mut *step.mutation.io,
            key_work: &mut *materials.write_keys,
            limits: materials.write_limits,
            cancelled: || (read_stop.borrow_mut())() || (mutation_stop.borrow_mut())(),
        };
        let receipt_proof = create_qualified_material_record_v1(
            MaterialRecordCreateV1 {
                key: MaterialRecordKeyV1::Transfer(selected.envelope().id),
                source: MaterialRecordRefV1::Transfer(&receipt),
                family: material_family,
            },
            &mut record_context,
            &mut record_write,
            &mut work.receipt,
            &receipt_guard,
            live,
        )
        .map_err(overlay_error)?;

        let sequence =
            step.read_allocator(crate::world::incremental_allocators::RuntimeAllocatorV1::Event)?;
        let (event_id, _, _) = World::preview_next_event_id(sequence.next, sequence.era);
        let event = crate::WorldEvent {
            id: event_id,
            time: step.time(),
            caused_by: Some(crate::CausedBy::Action(selected.envelope().id)),
            body: crate::WorldEventBody::Domain(crate::DomainEvent::MaterialTransferred {
                transfer_id: Some(selected.envelope().id),
                requester_agent_id: requester_agent_id.clone(),
                from_ledger: from_ledger.clone(),
                to_ledger: to_ledger.clone(),
                kind: kind.clone(),
                amount: *amount,
                distance_km: 0,
                priority: transit_priority,
                route_id: None,
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
        let prepared = PreparedMaterialTransferEventV1 {
            event,
            receipt: receipt_proof,
            action_id: selected.envelope().id,
            now: step.time(),
            identity: step.transaction.guard(),
        };
        prepared.validate(step.transaction, step.mutation.memory, step.time())?;
        let before = step.begin_world_event()?;
        event::sink_prepared_material_transfer_v1(
            step,
            &prepared,
            agents,
            work,
            max_event_bytes,
            live,
            &mut persist_original,
        )?;
        step.finish_world_event(before)?;
        if (step.mutation.cancelled)() || (read.cancelled)() || (parent_read.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(CompletedMaterialTransferV1 {
            owner: CompletedMaterialTransferOwnerV1::Transferred {
                event: prepared.event,
                identity: step.transaction.guard(),
            },
            _fee: reserve_completed_fee(step.mutation.memory, text_bytes)?,
        })
    })();
    let result = result.and_then(|completed| {
        if (step.mutation.cancelled)() || (read.cancelled)() || (parent_read.cancelled)() {
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
