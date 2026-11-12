//! Sealed Current industrial observations. No World snapshot is reconstructed.
use super::*;
use crate::world::event_processing::factory_schedule_fixed::{
    FactoryBuildFactsV1, FactoryScheduleRecordV1, sealed,
};
use oasis7_world_state::MaterialLedgerId;
use oasis7_world_state::{
    incremental_agent_fields::AgentFieldV1,
    incremental_agent_subtree::{
        AgentSubtreeQualifiedRequestV1, AgentSubtreeScalarRequestV1, AgentSubtreeValueV1,
        CurrentAgentPresenceV1, read_agent_subtree_scalar_overlay_v1,
        read_current_agent_presence_v1,
    },
    incremental_factory_records::*,
    incremental_factory_schedule_authority::*,
    incremental_factory_schedule_index::any_current_pending_factory_build_v1,
    incremental_world_id_sets::{
        WorldIdSetFieldV1, WorldIdSetKeyRefV1, world_id_set_contains_overlay_v1,
    },
    incremental_world_native::{
        recover_current_schedule_factory_profile_v1, recover_current_schedule_product_profile_v1,
        recover_current_schedule_recipe_profile_v1,
    },
};

pub(in crate::world) enum EvaluationErrorV1 {
    Body(BodyMutationError),
    World(crate::WorldError),
}
impl From<BodyMutationError> for EvaluationErrorV1 {
    fn from(e: BodyMutationError) -> Self {
        Self::Body(e)
    }
}
impl From<crate::WorldError> for EvaluationErrorV1 {
    fn from(e: crate::WorldError) -> Self {
        Self::World(e)
    }
}
impl From<ValueTreeError> for EvaluationErrorV1 {
    fn from(e: ValueTreeError) -> Self {
        Self::Body(e.into())
    }
}
impl From<WorldFieldOverlayErrorV1> for EvaluationErrorV1 {
    fn from(e: WorldFieldOverlayErrorV1) -> Self {
        Self::Body(overlay(e))
    }
}

pub(super) struct SourceFeesV1 {
    pub fees: Vec<MemoryReservation>,
    pub slots: MemoryReservation,
}
impl SourceFeesV1 {
    pub fn new(
        max: usize,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<Self, BodyMutationError> {
        charge(common, 0)?;
        let n = max
            .checked_mul(size_of::<MemoryReservation>())
            .ok_or_else(limited)?;
        let slots = common
            .memory()
            .reserve(n, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let fees = Vec::with_capacity(max);
        if fees.capacity() != max {
            return Err(limited());
        }
        Ok(Self { fees, slots })
    }
    pub(super) fn hold(
        &mut self,
        n: usize,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), BodyMutationError> {
        charge(common, n)?;
        if self.fees.len() == self.fees.capacity() {
            return Err(limited());
        }
        let fee = common
            .memory()
            .reserve(n, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        self.fees.push(fee);
        Ok(())
    }
}
pub(super) struct CurrentFactoryScheduleFactsV1<
    'a,
    'source,
    'tx,
    'state,
    'values,
    'read,
    'core,
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    V: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
> {
    pub sources: &'a FactoryScheduleSourcesV1<'source>,
    pub context: &'a mut WorldFieldOverlayContextV1<'tx, 'state, 'values, M, R, E, C>,
    pub read: &'a mut WorldFieldReadContextV1<'read, P, V, PE, PC>,
    pub work: &'a mut FactoryScheduleWorkV1,
    pub common: &'a mut FormalBoundedContextV1<'core>,
    pub now: u64,
    pub live: &'a ExecutionFence,
    pub fees: &'a mut SourceFeesV1,
}
impl<M, P, R, V, E, C, PE, PC>
    CurrentFactoryScheduleFactsV1<'_, '_, '_, '_, '_, '_, '_, M, P, R, V, E, C, PE, PC>
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
    pub(super) fn charge(&mut self, n: usize) -> Result<(), EvaluationErrorV1> {
        charge(self.common, n).map_err(Into::into)
    }
    pub(super) fn diagnostics<T: serde::Serialize>(
        &mut self,
        value: &T,
    ) -> Result<(), EvaluationErrorV1> {
        // Closed actual selected DTOs only. Canonical wire length dominates
        // their UTF-8 payload. Rust Debug escaping is <=6 bytes per input byte;
        // String geometric growth is <2x; inner and enclosing note buffers may
        // coexist, hence 2 * 2 * 6. The known literal inventory is paid
        // separately by reserve_action_output/reserve_recipe_output.
        let wire = oasis7_formal_protocol::controlled_authority::formal::context_wire::encode_paid(
            value,
            self.work.max_source_encoding_bytes,
            self.common,
            "factory schedule source encoding allowance",
            "factory schedule source",
        )
        .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        let n = wire.len().checked_mul(24).ok_or_else(limited)?;
        self.fees.hold(n, self.common)?;
        Ok(())
    }
    pub(super) fn presence(&mut self, id: &str) -> Result<bool, EvaluationErrorV1> {
        self.charge(id.len())?;
        let sources = self.sources;
        let live = self.live;
        super::io::paid(
            sources.roots,
            self.context,
            self.read,
            self.work,
            self.common,
            |ctx, read, work| {
                let guard = ctx.transaction.guard();
                Ok(read_current_agent_presence_v1(
                    id,
                    sources.lifecycle.agents,
                    sources.roots,
                    ctx,
                    read,
                    &mut work.lifecycle.pending_entries_remaining,
                    &guard,
                    live,
                )? == CurrentAgentPresenceV1::Existing)
            },
        )
    }
    pub(super) fn electricity_value(&mut self, id: &str) -> Result<i64, EvaluationErrorV1> {
        self.charge(id.len())?;
        let sources = self.sources;
        let live = self.live;
        super::io::paid(
            sources.roots,
            self.context,
            self.read,
            self.work,
            self.common,
            |ctx, _read, _work| {
                let guard = ctx.transaction.guard();
                match read_agent_subtree_scalar_overlay_v1(
                    AgentSubtreeQualifiedRequestV1 {
                        cell: AgentSubtreeScalarRequestV1 {
                            agent: id,
                            field: AgentFieldV1::ResourceAmount,
                            ordinal: 0,
                        },
                        qualification: sources.lifecycle.agents,
                    },
                    ctx,
                    64,
                    &mut std::io::sink(),
                    &guard,
                    live,
                )? {
                    None => Ok(0),
                    Some(AgentSubtreeValueV1::I64(n)) => Ok(n),
                    _ => Err(invalid().into()),
                }
            },
        )
    }
    pub(super) fn quantity(
        &mut self,
        ledger: &MaterialLedgerId,
        kind: &str,
    ) -> Result<i64, EvaluationErrorV1> {
        self.charge(kind.len())?;
        let roots = self.sources.roots;
        super::io::paid(
            roots,
            self.context,
            self.read,
            self.work,
            self.common,
            |ctx, _read, _work| {
                let q = ctx
                    .ledger(ledger, kind)
                    .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
                Ok(match q {
                    oasis7_world_state::incremental_inventory_read::LedgerQuantityV1::Value(n) => n,
                    _ => 0,
                })
            },
        )
    }
}
impl<M, P, R, V, E, C, PE, PC>
    CurrentFactoryScheduleFactsV1<'_, '_, '_, '_, '_, '_, '_, M, P, R, V, E, C, PE, PC>
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
    pub(super) fn complete_factory(
        &mut self,
        id: &str,
    ) -> Result<Option<ReservedValue<oasis7_world_state::FactoryState>>, EvaluationErrorV1> {
        self.charge(id.len())?;
        let sources = self.sources;
        let live = self.live;
        super::io::paid(
            sources.roots,
            self.context,
            self.read,
            self.work,
            self.common,
            |ctx, _read, work| {
                let guard = ctx.transaction.guard();
                Ok(oasis7_world_state::incremental_factory_schedule_factory::recover_current_schedule_complete_factory_v1(id,sources.lifecycle.factories,ctx,&mut work.factory,&guard,live)?)
            },
        )
    }
    pub(super) fn inventory_ledger(
        &mut self,
        id: &MaterialLedgerId,
    ) -> Result<ReservedValue<Option<std::collections::BTreeMap<String, i64>>>, EvaluationErrorV1>
    {
        let sources = self.sources;
        let live = self.live;
        super::io::paid(
            sources.roots,
            self.context,
            self.read,
            self.work,
            self.common,
            |ctx, read, work| {
                let guard = ctx.transaction.guard();
                Ok(
                    oasis7_world_state::incremental_inventory::recover_current_inventory_ledger_v1(
                        id,
                        sources.inventory_directory,
                        sources.lifecycle.inventory,
                        sources.roots,
                        ctx,
                        read,
                        &mut work.inventory_family,
                        &mut work.inventory_directory,
                        &guard,
                        live,
                    )?,
                )
            },
        )
    }
    pub(super) fn receipt_rows(&mut self)->Result<ReservedValue<oasis7_world_state::incremental_factory_schedule_receipts::CurrentFactoryScheduleReceiptsV1>,EvaluationErrorV1>{
        let sources = self.sources;
        let live = self.live;
        super::io::paid(
            sources.roots,
            self.context,
            self.read,
            self.work,
            self.common,
            |ctx, _read, work| {
                let guard = ctx.transaction.guard();
                Ok(oasis7_world_state::incremental_factory_schedule_receipts::observe_current_factory_schedule_receipts_v1(sources.receipts,sources.lifecycle.industry,sources.roots,ctx,&mut work.lifecycle.industry,&mut work.receipts,&guard,live)?)
            },
        )
    }
    pub(super) fn progress(
        &mut self,
    ) -> Result<
        ReservedValue<
            oasis7_world_state::incremental_world_native::CurrentFactoryNativeRecordV1<
                oasis7_world_state::IndustryProgressState,
            >,
        >,
        EvaluationErrorV1,
    > {
        let sources = self.sources;
        let live = self.live;
        super::io::paid(
            sources.roots,
            self.context,
            self.read,
            self.work,
            self.common,
            |ctx, parent, work| {
                let guard = ctx.transaction.guard();
                let mut read = WorldFieldReadContextV1 {
                    root: sources.roots.state_root(
                        oasis7_formal_protocol::incremental_state::StateNamespaceV1::Business,
                    ),
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
                Ok(oasis7_world_state::incremental_world_native::recover_current_factory_industry_progress_v1(sources.lifecycle.native,sources.roots,ctx.transaction,&mut read,&mut work.lifecycle.native_scan,&mut work.lifecycle.native_read,&guard,live)?)
            },
        )
    }
}

impl<M, P, R, V, E, C, PE, PC> sealed::Sealed
    for CurrentFactoryScheduleFactsV1<'_, '_, '_, '_, '_, '_, '_, M, P, R, V, E, C, PE, PC>
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
}

impl<'source, M, P, R, V, E, C, PE, PC> FactoryBuildFactsV1<'source>
    for CurrentFactoryScheduleFactsV1<'_, 'source, '_, '_, '_, '_, '_, M, P, R, V, E, C, PE, PC>
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
    type Error = EvaluationErrorV1;
    fn charge(&mut self, n: usize) -> Result<(), Self::Error> {
        self.charge(n)
    }
    fn reserve_action_output(
        &mut self,
        builder: &str,
        site: &str,
        spec: &oasis7_wasm_abi::FactoryModuleSpec,
    ) -> Result<(), Self::Error> {
        let mut n = 0usize;
        for text in [builder, site, &spec.factory_id, &spec.display_name] {
            self.charge(text.len())?;
            n = n.checked_add(text.len()).ok_or_else(limited)?;
        }
        for text in &spec.tags {
            self.charge(text.len())?;
            n = n.checked_add(text.len()).ok_or_else(limited)?;
        }
        for stack in &spec.build_cost {
            self.charge(stack.kind.len())?;
            n = n.checked_add(stack.kind.len()).ok_or_else(limited)?;
        }
        let bytes = n
            .checked_mul(24)
            .and_then(|n| n.checked_add(super::bounds::build_output_bytes(spec)?))
            .ok_or_else(limited)?;
        self.fees.hold(bytes, self.common)?;
        Ok(())
    }
    fn reserve_material_balances(
        &mut self,
        stacks: &[crate::MaterialStack],
    ) -> Result<(), Self::Error> {
        let mut bytes = super::bounds::material_map_bytes(stacks.len())?;
        for stack in stacks {
            self.charge(stack.kind.len())?;
            bytes = bytes
                .checked_add(stack.kind.len().checked_mul(2).ok_or_else(limited)?)
                .ok_or_else(limited)?;
        }
        self.fees.hold(bytes, self.common)?;
        Ok(())
    }
    fn now(&self) -> u64 {
        self.now
    }
    fn agent_exists(&mut self, id: &str) -> Result<bool, Self::Error> {
        self.presence(id)
    }
    fn retired(&mut self, id: &str) -> Result<bool, Self::Error> {
        self.charge(id.len())?;
        let sources = self.sources;
        let live = self.live;
        super::io::paid(
            sources.roots,
            self.context,
            self.read,
            self.work,
            self.common,
            |ctx, _read, _work| {
                Ok(world_id_set_contains_overlay_v1(
                    WorldIdSetFieldV1::RetiredFactories,
                    WorldIdSetKeyRefV1::Text(id),
                    ctx,
                )?)
            },
        )
    }
    fn factory_exists(&mut self, id: &str) -> Result<bool, Self::Error> {
        self.charge(id.len())?;
        let sources = self.sources;
        let live = self.live;
        super::io::paid(
            sources.roots,
            self.context,
            self.read,
            self.work,
            self.common,
            |ctx, _read, _work| match read_factory_cell_overlay_v1(
                FactoryRecordKeyV1::Factory(id),
                FactoryCellV1::Record,
                ctx,
                0,
                &mut std::io::sink(),
            )? {
                FactoryCellReadV1::RecordAbsent => Ok(false),
                FactoryCellReadV1::Value(FactoryValueV1::Unit) => Ok(true),
                _ => Err(invalid().into()),
            },
        )
    }
    fn any_pending_factory(&mut self, id: &str) -> Result<bool, Self::Error> {
        self.charge(id.len())?;
        let sources = self.sources;
        let live = self.live;
        super::io::paid(
            sources.roots,
            self.context,
            self.read,
            self.work,
            self.common,
            |ctx, _read, work| {
                let guard = ctx.transaction.guard();
                Ok(any_current_pending_factory_build_v1(
                    id,
                    sources.builds,
                    sources.lifecycle.factories,
                    sources.roots,
                    ctx,
                    &mut work.lifecycle.factory,
                    &mut work.builds,
                    &guard,
                    live,
                )?)
            },
        )
    }
    fn starter_assembler_blocker(&mut self, id: &str) -> Result<Option<String>, Self::Error> {
        if id != crate::STARTER_ASSEMBLER_FACTORY_ID {
            return Ok(None);
        }
        super::starter::read_current_starter_blocker_v1(self)
    }
    fn factory_profile(
        &mut self,
        id: &str,
    ) -> Result<Option<FactoryScheduleRecordV1<'source, crate::FactoryProfileV1>>, Self::Error>
    {
        self.charge(id.len())?;
        let sources = self.sources;
        let live = self.live;
        let record = super::io::paid(
            sources.roots,
            self.context,
            self.read,
            self.work,
            self.common,
            |ctx, parent, work| {
                let guard = ctx.transaction.guard();
                let mut read = WorldFieldReadContextV1 {
                    root: sources.roots.state_root(
                        oasis7_formal_protocol::incremental_state::StateNamespaceV1::Business,
                    ),
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
                Ok(recover_current_schedule_factory_profile_v1(
                    id,
                    sources.lifecycle.native,
                    sources.roots,
                    ctx.transaction,
                    &mut read,
                    &mut work.lifecycle.native_scan,
                    &mut work.lifecycle.native_read,
                    &guard,
                    live,
                )?)
            },
        )?;
        if let Some(record) = &record {
            self.diagnostics(record.value().value())?;
            let n = super::bounds::tag_normalization_bytes(&record.value().value().tags)?;
            self.fees.hold(n, self.common)?;
        }
        Ok(record.map(FactoryScheduleRecordV1::Native))
    }
    fn site(
        &mut self,
        id: &str,
    ) -> Result<Option<FactoryScheduleRecordV1<'source, crate::FactorySiteAuthorityV1>>, Self::Error>
    {
        self.charge(id.len())?;
        let sources = self.sources;
        let live = self.live;
        let value = super::io::paid(
            sources.roots,
            self.context,
            self.read,
            self.work,
            self.common,
            |ctx, read, work| {
                let guard = ctx.transaction.guard();
                Ok(recover_current_schedule_site_v1(
                    id,
                    sources.factory_authority,
                    read.state,
                    ctx,
                    &mut work.authority_scan,
                    &mut work.authority_cells,
                    &mut work.authority,
                    &guard,
                    live,
                )?)
            },
        )?;
        if let Some(value) = &value {
            self.diagnostics(&**value)?;
        }
        Ok(value.map(FactoryScheduleRecordV1::Current))
    }
    fn location(
        &mut self,
        id: &str,
    ) -> Result<
        Option<FactoryScheduleRecordV1<'source, crate::AgentLocationAuthorityV1>>,
        Self::Error,
    > {
        self.charge(id.len())?;
        let sources = self.sources;
        let live = self.live;
        let value = super::io::paid(
            sources.roots,
            self.context,
            self.read,
            self.work,
            self.common,
            |ctx, read, work| {
                let guard = ctx.transaction.guard();
                Ok(recover_current_schedule_agent_location_v1(
                    id,
                    sources.agent_authority,
                    read.state,
                    ctx,
                    &mut work.authority_scan,
                    &mut work.authority_cells,
                    &mut work.authority,
                    &guard,
                    live,
                )?)
            },
        )?;
        if let Some(value) = &value {
            self.diagnostics(&**value)?;
        }
        Ok(value.map(FactoryScheduleRecordV1::Current))
    }
    fn anchor_revision(&mut self, id: &str) -> Result<Result<u64, crate::WorldError>, Self::Error> {
        self.charge(id.len())?;
        let sources = self.sources;
        let live = self.live;
        let value = super::io::paid(
            sources.roots,
            self.context,
            self.read,
            self.work,
            self.common,
            |ctx, read, work| {
                let guard = ctx.transaction.guard();
                Ok(recover_current_schedule_location_anchor_v1(
                    id,
                    sources.agent_authority,
                    read.state,
                    ctx,
                    &mut work.authority_scan,
                    &mut work.authority_cells,
                    &mut work.authority,
                    &guard,
                    live,
                )?)
            },
        )?;
        if let Some(value) = &value {
            self.diagnostics(&**value)?;
        }
        let fee=id.len().checked_mul(12).and_then(|n|n.checked_add(2*("location anchor unknown: location anchor inactive_or_stale: location_id= revision= active=location anchor not yet effective: effective_at= now=".len()+3*20+5))).ok_or_else(limited)?;
        self.fees.hold(fee, self.common)?;
        Ok(oasis7_world_state::require_active_location_anchor_point_v1(
            value.as_deref(),
            id,
            self.now,
        )
        .map(|()| value.as_ref().map_or(0, |v| v.authority_revision)))
    }
    fn power_profile(
        &mut self,
        id: &str,
    ) -> Result<
        Option<FactoryScheduleRecordV1<'source, crate::FactoryConstructionPowerProfileV1>>,
        Self::Error,
    > {
        self.charge(id.len())?;
        let sources = self.sources;
        let live = self.live;
        let value = super::io::paid(
            sources.roots,
            self.context,
            self.read,
            self.work,
            self.common,
            |ctx, read, work| {
                let guard = ctx.transaction.guard();
                Ok(recover_current_schedule_power_profile_v1(
                    id,
                    sources.factory_authority,
                    read.state,
                    ctx,
                    &mut work.authority_scan,
                    &mut work.authority_cells,
                    &mut work.authority,
                    &guard,
                    live,
                )?)
            },
        )?;
        if let Some(value) = &value {
            self.diagnostics(&**value)?;
        }
        Ok(value.map(FactoryScheduleRecordV1::Current))
    }
    fn electricity(&mut self, id: &str) -> Result<i64, Self::Error> {
        self.electricity_value(id)
    }
    fn material(&mut self, ledger: &MaterialLedgerId, kind: &str) -> Result<i64, Self::Error> {
        self.quantity(ledger, kind)
    }
}
