//! Narrow actual Current Product selector used by the due dispatcher. This is
//! DATA selection through the original active registry/builtin fallback kernel.
use super::*;
use current::CurrentFactoryScheduleFactsV1;
#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn read_due_product_module_v1<M, P, R, V, E, C, PE, PC>(
    kind: &str,
    sources: &FactoryScheduleSourcesV1<'_>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    work: &mut FactoryScheduleWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    now: u64,
    live: &ExecutionFence,
) -> Result<Option<ReservedValue<String>>, BodyMutationError>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    V: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
{
    source_binding(sources, context.transaction, common)?;
    let mut fees = current::SourceFeesV1::new(work.max_source_records, common)?;
    let result = {
        let mut facts = CurrentFactoryScheduleFactsV1 {
            sources,
            context,
            read: parent,
            work,
            common,
            now,
            live,
            fees: &mut fees,
        };
        facts.product_module_id(kind)
    };
    let value = match result {
        Ok(value) => value,
        Err(error) => return Err(evaluation_error(error, fees)),
    };
    // Double-admit the actual output before releasing any selector scratch fees.
    // The value itself is moved; its string is never cloned here.
    let bytes = size_of::<ReservedValue<Option<String>>>()
        .checked_add(value.as_ref().map_or(0, String::capacity))
        .ok_or_else(limited)?;
    charge(common, bytes)?;
    let fee = common
        .memory()
        .reserve(bytes, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    Ok(value.map(|value| fee.retain(value)))
}

pub(in crate::world) enum DueTerminalCurrentErrorV1 {
    Body(BodyMutationError),
    World {
        cause: crate::WorldError,
        _fees: Vec<MemoryReservation>,
        _slots: MemoryReservation,
    },
}
impl From<BodyMutationError> for DueTerminalCurrentErrorV1 {
    fn from(e: BodyMutationError) -> Self {
        Self::Body(e)
    }
}
/// Same original ledger/destination order over genuine selected Current rows.
#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn check_due_ledger_destination_v1<M, P, R, V, E, C, PE, PC>(
    root: &oasis7_agent_api::world_action::RootOperationV1,
    sources: &FactoryScheduleSourcesV1<'_>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    work: &mut FactoryScheduleWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    now: u64,
    live: &ExecutionFence,
) -> Result<(), DueTerminalCurrentErrorV1>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    V: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
{
    use crate::world::event_processing::factory_schedule_fixed::FactoryBuildFactsV1;
    source_binding(sources, context.transaction, common)?;
    let mut fees = current::SourceFeesV1::new(work.max_source_records, common)?;
    let mut facts = CurrentFactoryScheduleFactsV1 {
        sources,
        context,
        read: parent,
        work,
        common,
        now,
        live,
        fees: &mut fees,
    };
    let result = (|| {
        facts.charge(root.owner_id.len())?;
        let mut factory = None;
        let mut site = None;
        let mut retired = false;
        match &root.output_ledger {
            oasis7_world_state::MaterialLedgerId::World
            | oasis7_world_state::MaterialLedgerId::Agent(_) => {}
            oasis7_world_state::MaterialLedgerId::Site(id) => {
                site = FactoryBuildFactsV1::site(&mut facts, id)?;
            }
            oasis7_world_state::MaterialLedgerId::Factory(id) => {
                factory = facts.complete_factory(id)?;
                // Preserve the original short circuit before reading site authority.
                if let Some(f) = &factory {
                    facts.charge(f.builder_agent_id.len() + root.owner_id.len())?;
                    if f.builder_agent_id == root.owner_id {
                        retired = FactoryBuildFactsV1::retired(&mut facts, id)?;
                        if !retired && f.durability_ppm > 0 {
                            site = FactoryBuildFactsV1::site(&mut facts, &f.site_id)?;
                        }
                    }
                }
            }
        }
        if let Some(site) = &site {
            facts.charge(site.owner_agent_id.len())?;
            for id in &site.authorized_agent_ids {
                facts.charge(id.len() + root.owner_id.len())?;
            }
        }
        facts
            .fees
            .hold(128 + size_of::<crate::WorldError>(), facts.common)?;
        oasis7_world_state::world_rules::industrial_ledger_access_from_records_v1(
            &root.owner_id,
            &root.output_ledger,
            now,
            factory.as_deref(),
            retired,
            site.as_deref(),
        )?;
        let destination = facts.complete_factory(&root.factory_id)?;
        let ledger_len = |v: &crate::MaterialLedgerId| match v {
            crate::MaterialLedgerId::World => 0,
            crate::MaterialLedgerId::Agent(v)
            | crate::MaterialLedgerId::Site(v)
            | crate::MaterialLedgerId::Factory(v) => v.len(),
        };
        let mut retired = false;
        if let Some(destination) = &destination {
            facts.charge(
                destination.builder_agent_id.len()
                    + root.owner_id.len()
                    + ledger_len(&destination.output_ledger)
                    + ledger_len(&root.output_ledger),
            )?;
            if destination.builder_agent_id == root.owner_id
                && destination.output_ledger == root.output_ledger
            {
                retired = FactoryBuildFactsV1::retired(&mut facts, &root.factory_id)?;
            }
        }
        oasis7_world_state::world_rules::validate_industrial_completion_destination_v1(
            root,
            destination.as_deref(),
            retired,
        )?;
        Ok(())
    })();
    drop(facts);
    match result {
        Ok(()) => Ok(()),
        Err(error) => Err(match error {
            current::EvaluationErrorV1::Body(error) => DueTerminalCurrentErrorV1::Body(error),
            current::EvaluationErrorV1::World(cause) => DueTerminalCurrentErrorV1::World {
                cause,
                _fees: fees.fees,
                _slots: fees.slots,
            },
        }),
    }
}
