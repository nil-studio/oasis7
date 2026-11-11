use super::*;
#[allow(clippy::too_many_arguments)]
pub(super) fn prepare<
    'a,
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeWriter,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
    D: FnMut() -> bool,
>(
    action: &'a crate::Action,
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    sources: &FactoryLifecycleSourcesV1<'_>,
    roots: &oasis7_formal_protocol::incremental_execution::RootSetV1,
    read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    parent_read: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    work: &mut FactoryLifecycleWorkV1,
    live: &ExecutionFence,
) -> Result<LifecycleDecisionV1<'a>, BodyMutationError> {
    let (operator, factory, parts) = match action {
        crate::Action::MaintainFactory {
            operator_agent_id,
            factory_id,
            parts,
        } => (
            operator_agent_id.as_str(),
            factory_id.as_str(),
            Some(*parts),
        ),
        crate::Action::RecycleFactory {
            operator_agent_id,
            factory_id,
        } => (operator_agent_id.as_str(), factory_id.as_str(), None),
        _ => {
            return Err(BodyMutationError::Invalid(
                "factory lifecycle effective action",
            ));
        }
    };
    if let Some(parts) = parts {
        if parts <= 0 {
            return Ok(LifecycleDecisionV1::Rejected(
                crate::RejectReason::InvalidAmount { amount: parts },
            ));
        }
    }
    if !read::agent(
        step,
        operator,
        sources,
        roots,
        read,
        parent_read,
        work,
        live,
    )? {
        return Ok(LifecycleDecisionV1::Rejected(
            crate::RejectReason::AgentNotFound {
                agent_id: operator.to_owned(),
            },
        ));
    }
    // A true Current absent outer record is the old business FactoryNotFound.
    let exists = read::text(step, factory, FactoryColumnV1::FactoryFactoryId, read, work)?;
    if exists.is_none() {
        return Ok(LifecycleDecisionV1::Rejected(
            crate::RejectReason::FactoryNotFound {
                factory_id: factory.to_owned(),
            },
        ));
    }
    // Close the actual Current child before using selected business fields. The
    // storage preparation is not consumed as deletion permission on Maintain.
    {
        let guard = step.transaction.guard();
        prepare_current_factory_delete_v1(
            factory,
            sources.factories,
            roots,
            step.transaction,
            read,
            &mut work.factory,
            work.max_scalar_bytes as u64,
            &guard,
            live,
        )
        .map_err(overlay_error)?;
    }
    let builder = read::text(
        step,
        factory,
        FactoryColumnV1::FactoryBuilderAgentId,
        read,
        work,
    )?
    .ok_or(BodyMutationError::Invalid("Current factory builder absent"))?;
    if builder.as_str() != operator {
        return Ok(LifecycleDecisionV1::Rejected(
            crate::RejectReason::RuleDenied {
                notes: vec![format!(
                    "{} factory denied: operator {} is not builder {}",
                    if parts.is_some() {
                        "maintain"
                    } else {
                        "recycle"
                    },
                    operator,
                    builder.as_str()
                )],
            },
        ));
    }
    let before = read::number(
        step,
        factory,
        FactoryColumnV1::FactoryDurabilityPpm,
        read,
        work,
    )?;
    let effect = if let Some(requested) = parts {
        if before.clamp(0, 1_000_000) >= 1_000_000 {
            return Ok(LifecycleDecisionV1::Rejected(
                crate::RejectReason::RuleDenied {
                    notes: vec![format!("factory {} already at full durability", factory)],
                },
            ));
        }
        let preferred = read::ledger(
            step,
            factory,
            FactoryColumnV1::FactoryInputLedger,
            read,
            work,
        )?;
        let available = read::amount(read::quantity(
            step,
            &preferred,
            "hardware_part",
            read,
            work,
        )?);
        let ledger = if available >= requested {
            preferred
        } else {
            step.mutation
                .memory
                .reserve(
                    std::mem::size_of::<MaterialLedgerId>(),
                    MemoryClass::Ordinary,
                )
                .map_err(ValueTreeError::Memory)?
                .retain(MaterialLedgerId::World)
        };
        let available = read::amount(read::quantity(step, &ledger, "hardware_part", read, work)?);
        if available < requested {
            return Ok(LifecycleDecisionV1::Rejected(
                crate::RejectReason::InsufficientMaterial {
                    material_kind: "hardware_part".into(),
                    requested,
                    available,
                },
            ));
        }
        let Some(repair) = math::maintenance_repair_v1(before, requested) else {
            return Ok(LifecycleDecisionV1::Rejected(
                crate::RejectReason::RuleDenied {
                    notes: vec![format!("factory {} maintenance has no effect", factory)],
                },
            ));
        };
        LifecycleEffectV1::Maintained {
            ledger,
            consumed: repair.consumed_parts,
            before_durability: before,
            durability: repair.durability_ppm,
        }
    } else {
        let guard = step.transaction.guard();
        let snapshot = observe_current_factory_lifecycle_v1(
            factory,
            sources.index,
            sources.factories,
            sources.industry,
            roots,
            step.transaction,
            read,
            &mut work.factory,
            &mut work.industry,
            &mut work.lifecycle,
            work.max_scalar_bytes as u64,
            &guard,
            live,
        )
        .map_err(overlay_error)?;
        if snapshot.active_jobs() > 0 {
            let slots = match read::cell(
                step,
                factory,
                FactoryCellV1::Column(FactoryColumnV1::SpecRecipeSlots),
                read,
                work,
                &mut std::io::sink(),
            )? {
                FactoryCellReadV1::Value(FactoryValueV1::Unsigned(n)) => u16::try_from(n)
                    .map_err(|_| BodyMutationError::Invalid("factory recipe slots"))?,
                _ => return Err(BodyMutationError::Invalid("factory recipe slots shape")),
            };
            return Ok(LifecycleDecisionV1::Rejected(
                crate::RejectReason::FactoryBusy {
                    factory_id: factory.to_owned(),
                    active_jobs: snapshot.active_jobs(),
                    recipe_slots: slots,
                },
            ));
        }
        let ledger = read::ledger(
            step,
            factory,
            FactoryColumnV1::FactoryOutputLedger,
            read,
            work,
        )?;
        let n = match read::cell(
            step,
            factory,
            FactoryCellV1::Collection(FactoryCollectionV1::SpecBuildCost),
            read,
            work,
            &mut std::io::sink(),
        )? {
            FactoryCellReadV1::Value(FactoryValueV1::Unsigned(n)) => usize::try_from(n)
                .map_err(|_| ValueTreeError::ResourceLimited("factory build cost count"))?,
            _ => return Err(BodyMutationError::Invalid("factory build cost length")),
        };
        if n > work.max_material_entries {
            return Err(ValueTreeError::ResourceLimited("factory recovered materials").into());
        }
        let cap = work.max_scalar_bytes.max(8);
        let bytes = n
            .checked_mul(
                std::mem::size_of::<MaterialStack>()
                    .checked_add(cap)
                    .ok_or(ValueTreeError::ResourceLimited("factory salvage width"))?,
            )
            .ok_or(ValueTreeError::ResourceLimited(
                "factory salvage allocation",
            ))?;
        let fee = step
            .mutation
            .memory
            .reserve(bytes, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let mut recovered = Vec::with_capacity(n);
        for i in 0..n {
            if (read.cancelled)() || (step.mutation.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            work.spend(cap)?;
            let mut kind = Vec::with_capacity(cap);
            let cell = read::cell(
                step,
                factory,
                FactoryCellV1::StackItem(
                    FactoryCollectionV1::SpecBuildCost,
                    i as u64,
                    FactoryStackColumnV1::Kind,
                ),
                read,
                work,
                &mut kind,
            )?;
            if !matches!(cell,FactoryCellReadV1::Value(FactoryValueV1::Text{utf8_bytes})if usize::try_from(utf8_bytes).ok()==Some(kind.len()))
            {
                return Err(BodyMutationError::Invalid("factory cost material kind"));
            }
            let amount = match read::cell(
                step,
                factory,
                FactoryCellV1::StackItem(
                    FactoryCollectionV1::SpecBuildCost,
                    i as u64,
                    FactoryStackColumnV1::Amount,
                ),
                read,
                work,
                &mut std::io::sink(),
            )? {
                FactoryCellReadV1::Value(FactoryValueV1::Signed(n)) => n,
                _ => return Err(BodyMutationError::Invalid("factory cost material amount")),
            };
            if let Some(amount) = math::recycle_stack_amount_v1(amount, before) {
                recovered.push(MaterialStack::new(
                    String::from_utf8(kind)
                        .map_err(|_| BodyMutationError::Invalid("factory cost UTF-8"))?,
                    amount,
                ));
            }
        }
        LifecycleEffectV1::Recycled {
            ledger,
            recovered,
            durability: before.clamp(0, 1_000_000),
            _fee: fee,
        }
    };
    read.check()?;
    parent_read.check()?;
    let identity = step.transaction.guard();
    step.transaction.validate_guard(&identity, live)?;
    Ok(LifecycleDecisionV1::Effect(LifecycleIntentV1 {
        operator,
        factory,
        effect,
        identity,
        time: step.time(),
    }))
}
