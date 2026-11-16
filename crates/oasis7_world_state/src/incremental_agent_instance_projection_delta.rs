//! Selected actual Current AgentCell/ModuleInstance JSON, using their original
//! dedicated scalar/body/mailbox formats. This is DATA, never qualification.
use crate::{
    AgentCell, AgentIntentV2, AgentState, WorldEventBody,
    agent_cell::AgentActivityV1,
    agent_state::{AgentBodyState, BodyModuleSlot, CargoEntityEntry},
    incremental_agent_body::{AGENT_BODY_KEY_DOMAIN_V1, AgentBodyKeyV1, AgentBodyValueV1},
    incremental_agent_fields::{
        AGENT_FIELD_KEY_DOMAIN_V1, AgentFieldKeyV1, AgentFieldV1 as F, AgentScalarRefV1,
        decode_agent_activity_status_v1, decode_agent_cargo_kind_v1, decode_agent_slot_type_v1,
    },
    incremental_agent_mutation::BodyMutationError,
    incremental_agent_subtree::AgentSubtreeStructureQualificationV1,
    incremental_event_fields::{
        MAILBOX_KEY_DOMAIN_V1, MailboxFrontierV1, MailboxKeyV1, mailbox_event_id_v1,
    },
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
    incremental_module_instance_family::ModuleInstanceFamilyQualificationV1,
    incremental_module_instance_fields::{
        ModuleInstallTargetRefV1, ModuleInstanceFieldV1 as I, ModuleInstanceKeyV1,
        ModuleInstanceScalarRefV1,
    },
    incremental_runtime_revocation::BusinessDiffProcessingWorkV1,
    incremental_world_event_decode::{WorldEventDecodeWorkV1, decode_world_event_original_v1},
    incremental_world_rule_projection_delta::{Processing, json, spend},
};
use oasis7_agent_api::{
    world_action::{GeoPos, ModuleInstallTarget, ResourceKind},
    world_entity::RobotBodySpec,
    world_resource::ResourceStock,
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFence, ExecutionTransaction, PreparedGuard, RootSetV1},
    incremental_material::{FactBudget, OriginalObjectStreamReader},
    incremental_state::{MerkleNodeReader, StateNamespaceV1 as Ns},
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation, ReservedValue};
use std::{
    collections::{BTreeMap, VecDeque},
    io::{self, Write},
    mem::size_of,
};
fn bad() -> ValueTreeError {
    ValueTreeError::Invalid("Current Agent/Instance projection")
}
fn limit() -> ValueTreeError {
    ValueTreeError::ResourceLimited("Current Agent/Instance projection")
}
#[derive(Debug)]
pub enum AgentInstanceProjectionErrorV1<E> {
    Value(ValueTreeError),
    Overlay(WorldFieldOverlayErrorV1),
    Body(BodyMutationError),
    Original(E),
}
impl<E> From<ValueTreeError> for AgentInstanceProjectionErrorV1<E> {
    fn from(e: ValueTreeError) -> Self {
        Self::Value(e)
    }
}
impl<E> From<WorldFieldOverlayErrorV1> for AgentInstanceProjectionErrorV1<E> {
    fn from(e: WorldFieldOverlayErrorV1) -> Self {
        Self::Overlay(e)
    }
}
struct Row {
    field: &'static str,
    semantic: ReservedValue<Vec<u8>>,
    key: ReservedValue<Vec<u8>>,
    value: Option<ReservedValue<Vec<u8>>>,
    electricity: Option<i64>,
    intent: Option<ReservedValue<Vec<u8>>>,
}
pub struct CurrentAgentInstanceProjectionDeltaV1 {
    rows: Vec<Row>,
    guard: PreparedGuard,
    parent: [u8; 32],
    memory: MemoryBudget,
    fee: MemoryReservation,
}
impl CurrentAgentInstanceProjectionDeltaV1 {
    pub fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        live: &ExecutionFence,
    ) -> Result<(), WorldFieldOverlayErrorV1> {
        tx.validate_read_source_guard(&self.guard, live)?;
        if !self.memory.same_pool(memory)
            || !self.fee.uses_budget(memory)
            || !tx.uses_budget(memory)
            || tx.parent_fence().parent_roots != self.parent
        {
            return Err(bad().into());
        }
        Ok(())
    }
    /// Small source fact captured while the actual AgentCell was already
    /// decoded. Missing Agent is None; a present Agent without the resource is
    /// Some(0), exactly ResourceStock::get. No mailbox is decoded by this visit.
    pub fn visit_agent_electricity_with_work_v1<E>(
        &self,
        mut work: impl FnMut(u64, u64) -> Result<(), E>,
        mut emit: impl FnMut(&[u8], &[u8], Option<i64>) -> Result<(), E>,
    ) -> Result<(), E> {
        for row in &self.rows {
            work(1, 0)?;
            if row.field == "agents" {
                emit(&row.semantic, &row.key, row.electricity)?;
            }
        }
        Ok(())
    }
    /// Actual optional intent captured from the same decoded AgentCell. An
    /// existing agent with no intent emits JSON null; deletion emits None.
    pub fn visit_agent_intents_with_work_v1<E>(
        &self,
        mut work: impl FnMut(u64, u64) -> Result<(), E>,
        mut emit: impl FnMut(&[u8], &[u8], Option<&[u8]>) -> Result<(), E>,
    ) -> Result<(), E> {
        for row in &self.rows {
            work(1, 0)?;
            if row.field == "agents" {
                emit(
                    &row.semantic,
                    &row.key,
                    row.intent.as_deref().map(Vec::as_slice),
                )?
            }
        }
        Ok(())
    }
    /// Work-only traversal of the same retained typed rows. Qualification,
    /// decoding and actual Original IO remain in the fixed source producer.
    pub fn visit_with_work_v1<E>(
        &self,
        mut work: impl FnMut(u64, u64) -> Result<(), E>,
        mut visit: impl FnMut(&'static str, &[u8], &[u8], Option<&[u8]>) -> Result<(), E>,
    ) -> Result<(), E> {
        for row in &self.rows {
            work(1, 0)?;
            visit(
                row.field,
                &row.semantic,
                &row.key,
                row.value.as_deref().map(Vec::as_slice),
            )?;
        }
        Ok(())
    }
    pub fn visit<E: From<ValueTreeError>>(
        &self,
        processing: &mut BusinessDiffProcessingWorkV1,
        mut stop: impl FnMut() -> bool,
        mut visit: impl FnMut(&'static str, &[u8], &[u8], Option<&[u8]>) -> Result<(), E>,
    ) -> Result<(), E> {
        let p = Processing {
            bank: std::cell::RefCell::new(processing),
        };
        self.visit_with_work_v1(|_, _| spend(&p, 0, &mut stop).map_err(E::from), visit)
    }
}
struct Pending {
    ns: Ns,
    key: Vec<u8>,
    before: Option<[u8; 32]>,
    after: Option<[u8; 32]>,
}
fn relevant(key: &[u8]) -> bool {
    key.starts_with(AGENT_BODY_KEY_DOMAIN_V1)
        || key.starts_with(AGENT_FIELD_KEY_DOMAIN_V1)
        || key.starts_with(MAILBOX_KEY_DOMAIN_V1)
        || ModuleInstanceKeyV1::is_instance_key(key)
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Agent,
    Instance,
}
struct Target {
    kind: Kind,
    key: String,
}
enum Scalar {
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
    I64(i64),
    Bool(bool),
    Text(String),
    OptionalText(Option<String>),
    OptionalU64(Option<u64>),
}
struct OriginalDebit<'a> {
    remaining: &'a mut u64,
    before: u64,
    settled: bool,
}
impl OriginalDebit<'_> {
    fn settle(&mut self, actual: u64) -> Result<(), ValueTreeError> {
        *self.remaining = self.before.checked_sub(actual).ok_or_else(limit)?;
        self.settled = true;
        Ok(())
    }
}
impl Drop for OriginalDebit<'_> {
    fn drop(&mut self) {
        if !self.settled {
            *self.remaining = 0;
        }
    }
}
struct Reader<'a, 'p, 't, 'v, 'm, M, R, E, C> {
    ctx: &'a mut WorldFieldOverlayContextV1<'t, 'v, 'm, M, R, E, C>,
    p: &'a Processing<'p>,
    fee: MemoryReservation,
}
impl<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
> Reader<'_, '_, '_, '_, '_, M, R, E, C>
{
    fn pay(&mut self, n: usize) -> Result<(), ValueTreeError> {
        spend(self.p, n, &mut self.ctx.cancelled)?;
        let mut fee = self.ctx.values.memory.reserve(n, MemoryClass::Ordinary)?;
        self.fee.merge(&mut fee).map_err(ValueTreeError::Invalid)
    }
    fn own(&mut self, s: &str) -> Result<String, ValueTreeError> {
        self.pay(s.len().checked_add(size_of::<String>()).ok_or_else(limit)?)?;
        Ok(s.to_owned())
    }
    fn raw(&mut self, id: &[u8; 32]) -> Result<ReservedValue<Vec<u8>>, ValueTreeError> {
        let root = self.ctx.values.load_root(id)?;
        let n = usize::try_from(root.0.logical_len).map_err(|_| limit())?;
        if root.0.logical_len > self.ctx.record_bytes {
            return Err(limit());
        }
        spend(
            self.p,
            n.checked_mul(8)
                .and_then(|v| v.checked_add(128))
                .ok_or_else(limit)?,
            &mut self.ctx.cancelled,
        )?;
        let fee = self.ctx.values.memory.reserve(
            n.checked_add(size_of::<Vec<u8>>()).ok_or_else(limit)?,
            MemoryClass::Ordinary,
        )?;
        let mut bytes = Vec::new();
        bytes.try_reserve_exact(n).map_err(|_| limit())?;
        if bytes.capacity() > n {
            return Err(limit());
        }
        self.ctx
            .values
            .read_range(&root, 0, n as u64, &mut bytes, &mut self.ctx.cancelled)?;
        if bytes.len() != n {
            return Err(bad());
        }
        Ok(fee.retain(bytes))
    }
    fn point(&mut self, ns: Ns, key: &[u8]) -> Result<Option<[u8; 32]>, WorldFieldOverlayErrorV1> {
        spend(self.p, key.len(), &mut self.ctx.cancelled)?;
        self.ctx.transaction.read(ns, key).map_err(Into::into)
    }
    fn scalar(
        &mut self,
        agent: &str,
        field: F,
        ordinal: u64,
    ) -> Result<Option<Scalar>, WorldFieldOverlayErrorV1> {
        spend(
            self.p,
            agent.len().checked_add(128).ok_or_else(limit)?,
            &mut self.ctx.cancelled,
        )?;
        let key = AgentFieldKeyV1::new(agent, field, ordinal).map_err(ValueTreeError::Invalid)?;
        let Some(id) = self.point(field.namespace(), key.as_bytes())? else {
            return Ok(None);
        };
        let raw = self.raw(&id)?;
        let value =
            AgentScalarRefV1::from_canonical_bytes(&raw, field).map_err(ValueTreeError::Invalid)?;
        Ok(Some(match value {
            AgentScalarRefV1::U8(v) => Scalar::U8(v),
            AgentScalarRefV1::U16(v) => Scalar::U16(v),
            AgentScalarRefV1::U32(v) => Scalar::U32(v),
            AgentScalarRefV1::U64(v) => Scalar::U64(v),
            AgentScalarRefV1::I64(v) => Scalar::I64(v),
            AgentScalarRefV1::Bool(v) => Scalar::Bool(v),
            AgentScalarRefV1::Text(v) => Scalar::Text(self.own(v)?),
            AgentScalarRefV1::OptionalText(v) => {
                Scalar::OptionalText(v.map(|v| self.own(v)).transpose()?)
            }
            AgentScalarRefV1::OptionalU64(v) => Scalar::OptionalU64(v),
        }))
    }
    fn required(&mut self, a: &str, f: F, o: u64) -> Result<Scalar, WorldFieldOverlayErrorV1> {
        self.scalar(a, f, o)?.ok_or_else(|| bad().into())
    }
    fn text(&mut self, a: &str, f: F, o: u64) -> Result<String, WorldFieldOverlayErrorV1> {
        match self.required(a, f, o)? {
            Scalar::Text(v) => Ok(v),
            _ => Err(bad().into()),
        }
    }
    fn optional_text(&mut self, a: &str, f: F) -> Result<Option<String>, WorldFieldOverlayErrorV1> {
        match self.required(a, f, 0)? {
            Scalar::OptionalText(v) => Ok(v),
            _ => Err(bad().into()),
        }
    }
    fn optional_u64(&mut self, a: &str, f: F) -> Result<Option<u64>, WorldFieldOverlayErrorV1> {
        match self.required(a, f, 0)? {
            Scalar::OptionalU64(v) => Ok(v),
            _ => Err(bad().into()),
        }
    }
    fn u64(&mut self, a: &str, f: F) -> Result<u64, WorldFieldOverlayErrorV1> {
        match self.required(a, f, 0)? {
            Scalar::U64(v) => Ok(v),
            _ => Err(bad().into()),
        }
    }
    fn i64(&mut self, a: &str, f: F, o: u64) -> Result<i64, WorldFieldOverlayErrorV1> {
        match self.required(a, f, o)? {
            Scalar::I64(v) => Ok(v),
            _ => Err(bad().into()),
        }
    }
    fn u16(&mut self, a: &str, f: F) -> Result<u16, WorldFieldOverlayErrorV1> {
        match self.required(a, f, 0)? {
            Scalar::U16(v) => Ok(v),
            _ => Err(bad().into()),
        }
    }
    fn u8(&mut self, a: &str, f: F, o: u64) -> Result<u8, WorldFieldOverlayErrorV1> {
        match self.required(a, f, o)? {
            Scalar::U8(v) => Ok(v),
            _ => Err(bad().into()),
        }
    }
    fn boolean(&mut self, a: &str, f: F, o: u64) -> Result<bool, WorldFieldOverlayErrorV1> {
        match self.required(a, f, o)? {
            Scalar::Bool(v) => Ok(v),
            _ => Err(bad().into()),
        }
    }
    fn absent_group(&mut self, a: &str, first: F, last: F) -> Result<(), WorldFieldOverlayErrorV1> {
        for field in F::ALL {
            spend(self.p, 0, &mut self.ctx.cancelled)?;
            if (*field as u16) >= (first as u16)
                && (*field as u16) <= (last as u16)
                && self.scalar(a, *field, 0)?.is_some()
            {
                return Err(bad().into());
            }
        }
        Ok(())
    }
    fn activity(&mut self, a: &str) -> Result<Option<AgentActivityV1>, WorldFieldOverlayErrorV1> {
        if !self.boolean(a, F::ActivityPresent, 0)? {
            self.absent_group(a, F::ActivityStatus, F::ActivityReasonSummary)?;
            return Ok(None);
        }
        let status = decode_agent_activity_status_v1(self.u8(a, F::ActivityStatus, 0)?)
            .map_err(ValueTreeError::Invalid)?;
        Ok(Some(AgentActivityV1 {
            status,
            updated_at: self.u64(a, F::ActivityUpdated)?,
            operation_kind: self.optional_text(a, F::ActivityOperationKind)?,
            operation_id: self.optional_u64(a, F::ActivityOperationId)?,
            target_id: self.optional_text(a, F::ActivityTarget)?,
            reason_code: self.optional_text(a, F::ActivityReasonCode)?,
            reason_summary: self.optional_text(a, F::ActivityReasonSummary)?,
        }))
    }
    fn intent(&mut self, a: &str) -> Result<Option<AgentIntentV2>, WorldFieldOverlayErrorV1> {
        if !self.boolean(a, F::IntentPresent, 0)? {
            self.absent_group(a, F::IntentSchema, F::IntentRequestDigest)?;
            return Ok(None);
        }
        use crate::incremental_intent_fields::{
            IntentFieldV1 as I, OriginalIntentProjectionScalarV1 as V,
            assemble_original_intent_projection_v1,
        };
        let intent = assemble_original_intent_projection_v1(
            |field| {
                Ok(match field {
                    I::SchemaVersion => match self.required(a, F::IntentSchema, 0)? {
                        Scalar::U32(value) => V::U32(value),
                        _ => return Err(WorldFieldOverlayErrorV1::from(bad())),
                    },
                    I::AgentId => V::Text(self.text(a, F::IntentAgent, 0)?),
                    I::IntentId => V::Text(self.text(a, F::IntentId, 0)?),
                    I::Kind => V::Text(self.text(a, F::IntentKind, 0)?),
                    I::Summary => V::Text(self.text(a, F::IntentSummary, 0)?),
                    I::TargetId => self
                        .optional_text(a, F::IntentTarget)?
                        .map_or(V::Unit, V::Text),
                    I::EffectIntentId => self
                        .optional_text(a, F::IntentEffect)?
                        .map_or(V::Unit, V::Text),
                    I::IntentTick => self.optional_u64(a, F::IntentTick)?.map_or(V::Unit, V::U64),
                    I::WorldId => self
                        .optional_text(a, F::IntentWorld)?
                        .map_or(V::Unit, V::Text),
                    I::ReorgEpoch => self
                        .optional_u64(a, F::IntentReorg)?
                        .map_or(V::Unit, V::U64),
                    I::AuthorityScope => self
                        .optional_text(a, F::IntentAuthorityScope)?
                        .map_or(V::Unit, V::Text),
                    I::Status => V::Text(self.text(a, F::IntentStatus, 0)?),
                    I::Source => V::Text(self.text(a, F::IntentSource, 0)?),
                    I::LogicalTime => V::U64(self.u64(a, F::IntentLogicalTime)?),
                    I::EventSeq => V::U64(self.u64(a, F::IntentEventSeq)?),
                    I::UpdatedAt => V::U64(self.u64(a, F::IntentUpdated)?),
                    I::ReceiptRef => self
                        .optional_text(a, F::IntentReceipt)?
                        .map_or(V::Unit, V::Text),
                    I::ReasonCode => self
                        .optional_text(a, F::IntentReasonCode)?
                        .map_or(V::Unit, V::Text),
                    I::ReasonSummary => self
                        .optional_text(a, F::IntentReasonSummary)?
                        .map_or(V::Unit, V::Text),
                    I::ReplacedBy => self
                        .optional_text(a, F::IntentReplacedBy)?
                        .map_or(V::Unit, V::Text),
                    I::ActorId => V::Text(self.text(a, F::IntentActor, 0)?),
                    I::RequestDigest => V::Text(self.text(a, F::IntentRequestDigest, 0)?),
                })
            },
            "Current Agent/Instance projection",
        )?;
        if intent.agent_id != a {
            return Err(bad().into());
        }
        Ok(Some(intent))
    }
    fn instance(
        &mut self,
        a: &str,
    ) -> Result<Option<crate::state::ModuleInstanceState>, WorldFieldOverlayErrorV1> {
        let mut raw = Vec::new();
        self.pay(8 * size_of::<ReservedValue<Vec<u8>>>())?;
        raw.try_reserve_exact(8).map_err(|_| limit())?;
        if raw.capacity() > 8 {
            return Err(limit().into());
        }
        for f in I::ALL {
            spend(self.p, 0, &mut self.ctx.cancelled)?;
            let key = ModuleInstanceKeyV1::new(a, f).map_err(ValueTreeError::Invalid)?;
            let Some(id) = self.point(Ns::Business, key.as_bytes())? else {
                if !raw.is_empty() {
                    return Err(bad().into());
                }
                for other in I::ALL {
                    spend(self.p, 0, &mut self.ctx.cancelled)?;
                    let k = ModuleInstanceKeyV1::new(a, other).map_err(ValueTreeError::Invalid)?;
                    if self.point(Ns::Business, k.as_bytes())?.is_some() {
                        return Err(bad().into());
                    }
                }
                return Ok(None);
            };
            raw.push(self.raw(&id)?);
        }
        let text =
            |i: usize, f: I| match ModuleInstanceScalarRefV1::from_canonical_bytes(&raw[i], f)
                .map_err(ValueTreeError::Invalid)?
            {
                ModuleInstanceScalarRefV1::Text(v) => Ok(v),
                _ => Err(bad()),
            };
        let instance_id = self.own(text(0, I::InstanceId)?)?;
        let module_id = self.own(text(1, I::ModuleId)?)?;
        let module_version = self.own(text(2, I::Version)?)?;
        let wasm_hash = self.own(text(3, I::WasmHash)?)?;
        let owner_agent_id = self.own(text(4, I::Owner)?)?;
        let install_target =
            match ModuleInstanceScalarRefV1::from_canonical_bytes(&raw[5], I::Target)
                .map_err(ValueTreeError::Invalid)?
            {
                ModuleInstanceScalarRefV1::Target(ModuleInstallTargetRefV1::SelfAgent) => {
                    ModuleInstallTarget::SelfAgent
                }
                ModuleInstanceScalarRefV1::Target(
                    ModuleInstallTargetRefV1::LocationInfrastructure(v),
                ) => ModuleInstallTarget::LocationInfrastructure {
                    location_id: self.own(v)?,
                },
                _ => return Err(bad().into()),
            };
        let active = match ModuleInstanceScalarRefV1::from_canonical_bytes(&raw[6], I::Active)
            .map_err(ValueTreeError::Invalid)?
        {
            ModuleInstanceScalarRefV1::Bool(v) => v,
            _ => return Err(bad().into()),
        };
        let installed_at =
            match ModuleInstanceScalarRefV1::from_canonical_bytes(&raw[7], I::InstalledAt)
                .map_err(ValueTreeError::Invalid)?
            {
                ModuleInstanceScalarRefV1::U64(v) => v,
                _ => return Err(bad().into()),
            };
        Ok(Some(crate::state::ModuleInstanceState {
            instance_id,
            module_id,
            module_version,
            wasm_hash,
            owner_agent_id,
            install_target,
            active,
            installed_at,
        }))
    }
    fn agent<O: OriginalObjectStreamReader>(
        &mut self,
        a: &str,
        objects: &mut O,
        facts: &mut FactBudget,
        decode: &mut WorldEventDecodeWorkV1,
    ) -> Result<Option<AgentCell>, AgentInstanceProjectionErrorV1<O::Error>> {
        self.pay(size_of::<AgentCell>())?;
        let body_key = AgentBodyKeyV1::new(a).map_err(ValueTreeError::Invalid)?;
        let Some(body_id) = self.point(Ns::Business, body_key.as_bytes())? else {
            // A real removed body is absence only if the ordinary identity is
            // also gone. A present scalar cannot silently become a deletion.
            if self.scalar(a, F::Identity, 0)?.is_some() {
                return Err(bad().into());
            }
            return Ok(None);
        };
        let raw = self.raw(&body_id)?;
        let body_value =
            AgentBodyValueV1::from_canonical_bytes(&raw).map_err(ValueTreeError::Invalid)?;
        let agent_id = self.text(a, F::Identity, 0)?;
        if agent_id != a {
            return Err(bad().into());
        }
        let pos = GeoPos {
            x_cm: self.i64(a, F::PositionX, 0)?,
            y_cm: self.i64(a, F::PositionY, 0)?,
            z_cm: self.i64(a, F::PositionZ, 0)?,
        };
        let body = RobotBodySpec {
            kind: self.text(a, F::BodyKind, 0)?,
            height_cm: self.i64(a, F::BodyHeight, 0)?,
        };
        let mut amounts = BTreeMap::new();
        for (ordinal, kind) in [(0, ResourceKind::Electricity), (1, ResourceKind::Data)] {
            spend(self.p, 0, &mut self.ctx.cancelled)?;
            match self.scalar(a, F::ResourceAmount, ordinal)? {
                Some(Scalar::I64(n)) => {
                    self.pay(512)?;
                    amounts.insert(kind, n);
                }
                None => {}
                _ => return Err(bad().into()),
            }
        }
        let frame_kind = self.text(a, F::FrameKind, 0)?;
        let slot_capacity = self.u16(a, F::SlotCapacity)?;
        let expansion_level = self.u16(a, F::ExpansionLevel)?;
        let count = self.u64(a, F::SlotCount)?;
        let next = self.u64(a, F::SlotNext)?;
        if count > next {
            return Err(bad().into());
        }
        let cap = usize::try_from(count).map_err(|_| limit())?;
        self.pay(
            cap.checked_mul(size_of::<BodyModuleSlot>())
                .ok_or_else(limit)?,
        )?;
        let mut slots = Vec::new();
        slots.try_reserve_exact(cap).map_err(|_| limit())?;
        if slots.capacity() > cap {
            return Err(limit().into());
        }
        for ordinal in 0..next {
            spend(self.p, 0, &mut self.ctx.cancelled)?;
            let Some(first) = self.scalar(a, F::SlotId, ordinal)? else {
                for f in [F::SlotType, F::SlotInstalled, F::SlotLocked] {
                    if self.scalar(a, f, ordinal)?.is_some() {
                        return Err(bad().into());
                    }
                }
                continue;
            };
            if slots.len() == cap {
                return Err(bad().into());
            }
            let slot_id = match first {
                Scalar::Text(v) => v,
                _ => return Err(bad().into()),
            };
            let slot_type = decode_agent_slot_type_v1(self.u8(a, F::SlotType, ordinal)?)
                .map_err(ValueTreeError::Invalid)?;
            let installed_module = match self.required(a, F::SlotInstalled, ordinal)? {
                Scalar::OptionalText(v) => v,
                _ => return Err(bad().into()),
            };
            let locked = self.boolean(a, F::SlotLocked, ordinal)?;
            slots.push(BodyModuleSlot {
                slot_id,
                slot_type,
                installed_module,
                locked,
            });
        }
        if slots.len() != cap {
            return Err(bad().into());
        }
        let count = self.u64(a, F::CargoCount)?;
        let next = self.u64(a, F::CargoNext)?;
        if count > next {
            return Err(bad().into());
        }
        let cap = usize::try_from(count).map_err(|_| limit())?;
        self.pay(
            cap.checked_mul(size_of::<CargoEntityEntry>())
                .ok_or_else(limit)?,
        )?;
        let mut cargo_entries = Vec::new();
        cargo_entries.try_reserve_exact(cap).map_err(|_| limit())?;
        if cargo_entries.capacity() > cap {
            return Err(limit().into());
        }
        for ordinal in 0..next {
            spend(self.p, 0, &mut self.ctx.cancelled)?;
            let Some(first) = self.scalar(a, F::CargoId, ordinal)? else {
                for f in [F::CargoKind, F::CargoQuantity, F::CargoUnitSize] {
                    if self.scalar(a, f, ordinal)?.is_some() {
                        return Err(bad().into());
                    }
                }
                continue;
            };
            if cargo_entries.len() == cap {
                return Err(bad().into());
            }
            let entity_id = match first {
                Scalar::Text(v) => v,
                _ => return Err(bad().into()),
            };
            let entity_kind = decode_agent_cargo_kind_v1(self.u8(a, F::CargoKind, ordinal)?)
                .map_err(ValueTreeError::Invalid)?;
            let quantity = self.i64(a, F::CargoQuantity, ordinal)?;
            let size_per_unit = self.i64(a, F::CargoUnitSize, ordinal)?;
            cargo_entries.push(CargoEntityEntry {
                entity_id,
                entity_kind,
                quantity,
                size_per_unit,
            });
        }
        if cargo_entries.len() != cap {
            return Err(bad().into());
        }
        let activity = self.activity(a)?;
        let intent = self.intent(a)?;
        let key = MailboxKeyV1::new(a, None).map_err(ValueTreeError::Invalid)?;
        let frontier_id = self.point(Ns::Scheduler, key.as_bytes())?.ok_or_else(bad)?;
        let frontier = MailboxFrontierV1::from_canonical_bytes(&self.raw(&frontier_id)?)
            .map_err(ValueTreeError::Invalid)?;
        let n = usize::try_from(frontier.tail - frontier.head).map_err(|_| limit())?;
        self.pay(
            n.checked_mul(size_of::<crate::DomainEvent>())
                .ok_or_else(limit)?,
        )?;
        let mut mailbox = VecDeque::new();
        mailbox.try_reserve_exact(n).map_err(|_| limit())?;
        if mailbox.capacity() > n {
            return Err(limit().into());
        }
        for ordinal in frontier.head..frontier.tail {
            spend(self.p, 0, &mut self.ctx.cancelled)?;
            let key = MailboxKeyV1::new(a, Some(ordinal)).map_err(ValueTreeError::Invalid)?;
            let reference = self.point(Ns::Scheduler, key.as_bytes())?.ok_or_else(bad)?;
            let id =
                mailbox_event_id_v1(&self.raw(&reference)?).map_err(ValueTreeError::Invalid)?;
            facts.facts_remaining = facts.facts_remaining.checked_sub(1).ok_or_else(limit)?;
            let max = facts
                .max_fact_bytes
                .min(facts.original_bytes_remaining)
                .min(decode.max_original_bytes as u64);
            if max == 0 {
                return Err(limit().into());
            }
            if !objects
                .allocation_budget()
                .same_pool(self.ctx.values.memory)
                || objects.allocation_class() != MemoryClass::Ordinary
            {
                return Err(bad().into());
            }
            let before = facts.original_bytes_remaining;
            facts.original_bytes_remaining = before.checked_sub(max).ok_or_else(limit)?;
            let mut debit = OriginalDebit {
                remaining: &mut facts.original_bytes_remaining,
                before,
                settled: false,
            };
            let raw = objects
                .read_original(
                    &id,
                    usize::try_from(max).map_err(|_| limit())?,
                    self.ctx.values.memory,
                )
                .map_err(AgentInstanceProjectionErrorV1::Original)?;
            if !raw.uses_budget(self.ctx.values.memory)
                || raw.len() as u64 > max
                || raw.reserved_bytes() < raw.len()
            {
                return Err(bad().into());
            }
            debit.settle(raw.len() as u64)?;
            drop(debit);
            let mut hash = blake3::Hasher::new();
            for chunk in raw.chunks(8192) {
                spend(self.p, chunk.len(), &mut self.ctx.cancelled)?;
                hash.update(chunk);
            }
            if hash.finalize().as_bytes() != &id {
                return Err(bad().into());
            }
            let event = decode_world_event_original_v1(
                &raw,
                self.ctx.values.memory,
                MemoryClass::Ordinary,
                decode,
                &mut self.ctx.cancelled,
            )?;
            // The decoded owned graph is the exact original codec's paid bound;
            // admit a second graph before this one selected event is cloned.
            self.pay(event.reserved_bytes())?;
            let cloned = event.event().clone();
            let WorldEventBody::Domain(domain) = cloned.body else {
                return Err(bad().into());
            };
            mailbox.push_back(domain);
        }
        Ok(Some(AgentCell {
            state: AgentState {
                agent_id,
                pos,
                body,
                resources: ResourceStock { amounts },
                body_view: body_value.body_view,
                body_state: AgentBodyState {
                    frame_kind,
                    slot_capacity,
                    slots,
                    expansion_level,
                    cargo_entries,
                },
            },
            mailbox,
            last_active: body_value.last_active,
            activity,
            intent,
        }))
    }
    fn before(
        &mut self,
        pending: &[Pending],
        ns: Ns,
        key: &[u8],
    ) -> Result<[u8; 32], ValueTreeError> {
        let mut before = None;
        for row in pending {
            spend(
                self.p,
                row.key.len().checked_add(key.len()).ok_or_else(limit)?,
                &mut self.ctx.cancelled,
            )?;
            if row.ns == ns && row.key == key {
                if before.is_some() {
                    return Err(bad());
                }
                before = row.before;
            }
        }
        before.ok_or_else(bad)
    }
    fn verify_removed_agent(
        &mut self,
        a: &str,
        pending: &[Pending],
    ) -> Result<(), WorldFieldOverlayErrorV1> {
        let body = AgentBodyKeyV1::new(a).map_err(ValueTreeError::Invalid)?;
        self.before(pending, Ns::Business, body.as_bytes())?;
        for field in F::ALL {
            spend(self.p, 0, &mut self.ctx.cancelled)?;
            if self.scalar(a, *field, 0)?.is_some() {
                return Err(bad().into());
            }
        }
        if self.scalar(a, F::ResourceAmount, 1)?.is_some() {
            return Err(bad().into());
        }
        for (next_field, fields) in [
            (
                F::SlotNext,
                &[F::SlotId, F::SlotType, F::SlotInstalled, F::SlotLocked][..],
            ),
            (
                F::CargoNext,
                &[F::CargoId, F::CargoKind, F::CargoQuantity, F::CargoUnitSize][..],
            ),
        ] {
            let key = AgentFieldKeyV1::new(a, next_field, 0).map_err(ValueTreeError::Invalid)?;
            let id = self.before(pending, Ns::Business, key.as_bytes())?;
            let raw = self.raw(&id)?;
            let AgentScalarRefV1::U64(next) =
                AgentScalarRefV1::from_canonical_bytes(&raw, next_field)
                    .map_err(ValueTreeError::Invalid)?
            else {
                return Err(bad().into());
            };
            for ordinal in 0..next {
                spend(self.p, 0, &mut self.ctx.cancelled)?;
                for field in fields {
                    if self.scalar(a, *field, ordinal)?.is_some() {
                        return Err(bad().into());
                    }
                }
            }
        }
        let key = MailboxKeyV1::new(a, None).map_err(ValueTreeError::Invalid)?;
        if self.point(Ns::Scheduler, key.as_bytes())?.is_some() {
            return Err(bad().into());
        }
        let id = self.before(pending, Ns::Scheduler, key.as_bytes())?;
        let frontier = MailboxFrontierV1::from_canonical_bytes(&self.raw(&id)?)
            .map_err(ValueTreeError::Invalid)?;
        for ordinal in frontier.head..frontier.tail {
            spend(self.p, 0, &mut self.ctx.cancelled)?;
            let key = MailboxKeyV1::new(a, Some(ordinal)).map_err(ValueTreeError::Invalid)?;
            if self.point(Ns::Scheduler, key.as_bytes())?.is_some() {
                return Err(bad().into());
            }
        }
        Ok(())
    }
}

struct KeyCount {
    n: usize,
}
impl Write for KeyCount {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        self.n = self.n.checked_add(b.len()).ok_or(io::ErrorKind::Other)?;
        Ok(b.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn semantic<C: FnMut() -> bool>(
    key: &str,
    memory: &MemoryBudget,
    p: &Processing<'_>,
    stop: &mut C,
) -> Result<ReservedValue<Vec<u8>>, ValueTreeError> {
    // The original sealed Text key writer runs twice, with both traversals
    // prepaid from the real key length before either copy/encode occurs.
    spend(
        p,
        key.len()
            .checked_add(128)
            .and_then(|n| n.checked_mul(2))
            .ok_or_else(limit)?,
        stop,
    )?;
    let mut count = KeyCount { n: 0 };
    crate::incremental_semantic_key::write_semantic_key_v1(key, &mut count).map_err(|_| bad())?;
    let fee = memory.reserve(
        count
            .n
            .checked_add(size_of::<Vec<u8>>())
            .ok_or_else(limit)?,
        MemoryClass::Ordinary,
    )?;
    let mut raw = Vec::new();
    raw.try_reserve_exact(count.n).map_err(|_| limit())?;
    if raw.capacity() > count.n {
        return Err(limit());
    }
    crate::incremental_semantic_key::write_semantic_key_v1(key, &mut raw).map_err(|_| bad())?;
    if raw.len() != count.n {
        return Err(bad());
    }
    Ok(fee.retain(raw))
}

/// Capture each actually changed Agent/Instance once. Hashed agent-field keys
/// are joined to the actual Identity scalar and original body key; hashes are
/// never interpreted as names. All work inputs are continuous caller banks.
#[allow(clippy::too_many_arguments)]
fn read_current_agent_instance_projection_delta_impl<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    O: OriginalObjectStreamReader,
>(
    agents: &ReservedValue<AgentSubtreeStructureQualificationV1>,
    instances: &ReservedValue<ModuleInstanceFamilyQualificationV1>,
    roots: &RootSetV1,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    pending_entries: &mut usize,
    processing: &mut BusinessDiffProcessingWorkV1,
    objects: &mut O,
    originals: &mut FactBudget,
    decode: &mut WorldEventDecodeWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<CurrentAgentInstanceProjectionDeltaV1, AgentInstanceProjectionErrorV1<O::Error>> {
    ctx.transaction
        .validate_read_source_guard(guard, live)
        .map_err(WorldFieldOverlayErrorV1::from)?;
    let p = Processing {
        bank: std::cell::RefCell::new(processing),
    };
    let memory = ctx.values.memory.clone();
    let _phase = memory
        .reserve(8192 + size_of::<Processing<'_>>(), MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    if !agents.uses_budget(&memory)
        || !instances.uses_budget(&memory)
        || !ctx.transaction.uses_budget(&memory)
        || ctx.values.class != MemoryClass::Ordinary
        || agents.allocation_class() != MemoryClass::Ordinary
        || instances.allocation_class() != MemoryClass::Ordinary
    {
        return Err(bad().into());
    }
    if roots.world_id.len() > 256 || roots.state.iter().any(|r| r.node_root.len() != 64) {
        return Err(bad().into());
    }
    let root_bytes = roots
        .state
        .iter()
        .try_fold(roots.world_id.len() + 512, |n, r| {
            spend(&p, 0, &mut ctx.cancelled)?;
            n.checked_add(r.node_root.len() + 128).ok_or_else(limit)
        })?;
    spend(
        &p,
        root_bytes.checked_mul(4).ok_or_else(limit)?,
        &mut ctx.cancelled,
    )?;
    agents.validate_binding(roots)?;
    instances
        .validate_binding(roots)
        .map_err(AgentInstanceProjectionErrorV1::Body)?;
    if roots.digest().map_err(|_| bad())? != ctx.transaction.parent_fence().parent_roots {
        return Err(bad().into());
    }
    let mut count = 0usize;
    let mut bytes = 0usize;
    for root in &roots.state {
        spend(&p, 0, &mut ctx.cancelled)?;
        let stop = std::cell::RefCell::new(&mut ctx.cancelled);
        ctx.transaction.visit_pending_writes(
            root.namespace,
            pending_entries,
            || spend(&p, 0, &mut || (stop.borrow_mut())()).map_err(WorldFieldOverlayErrorV1::from),
            |key, before, after| {
                spend(&p, key.len(), &mut || (stop.borrow_mut())())?;
                if before != after && relevant(key) {
                    count = count.checked_add(1).ok_or_else(limit)?;
                    bytes = bytes.checked_add(key.len()).ok_or_else(limit)?;
                }
                Ok::<_, WorldFieldOverlayErrorV1>(())
            },
        )?;
    }
    let pending_fee = memory
        .reserve(
            count
                .checked_mul(size_of::<Pending>())
                .and_then(|n| n.checked_add(bytes))
                .ok_or_else(limit)?,
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?;
    let mut pending = Vec::new();
    pending.try_reserve_exact(count).map_err(|_| limit())?;
    if pending.capacity() > count {
        return Err(limit().into());
    }
    for root in &roots.state {
        spend(&p, 0, &mut ctx.cancelled)?;
        let stop = std::cell::RefCell::new(&mut ctx.cancelled);
        ctx.transaction.visit_pending_writes(
            root.namespace,
            pending_entries,
            || spend(&p, 0, &mut || (stop.borrow_mut())()).map_err(WorldFieldOverlayErrorV1::from),
            |key, before, after| {
                spend(&p, key.len(), &mut || (stop.borrow_mut())())?;
                if before != after && relevant(key) {
                    if pending.len() == count {
                        return Err(bad().into());
                    }
                    pending.push(Pending {
                        ns: root.namespace,
                        key: key.to_vec(),
                        before,
                        after,
                    });
                }
                Ok::<_, WorldFieldOverlayErrorV1>(())
            },
        )?;
    }
    if pending.len() != count {
        return Err(bad().into());
    }
    let fee = memory
        .reserve(
            count
                .checked_mul(size_of::<Row>())
                .and_then(|n| n.checked_add(size_of::<CurrentAgentInstanceProjectionDeltaV1>()))
                .ok_or_else(limit)?,
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?;
    let mut rows = Vec::new();
    rows.try_reserve_exact(count).map_err(|_| limit())?;
    if rows.capacity() > count {
        return Err(limit().into());
    }
    let mut reader = Reader {
        ctx: &mut *ctx,
        p: &p,
        fee: memory
            .reserve(0, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?,
    };
    reader.pay(
        count
            .checked_mul(size_of::<Target>() + size_of::<usize>())
            .ok_or_else(limit)?,
    )?;
    let mut targets: Vec<Target> = Vec::new();
    targets.try_reserve_exact(count).map_err(|_| limit())?;
    if targets.capacity() > count {
        return Err(limit().into());
    }
    let mut owners = Vec::new();
    owners.try_reserve_exact(count).map_err(|_| limit())?;
    if owners.capacity() > count {
        return Err(limit().into());
    }
    for row in &pending {
        if reader.point(row.ns, &row.key)? != row.after {
            return Err(bad().into());
        }
        let (kind, key) = if row.key.starts_with(AGENT_BODY_KEY_DOMAIN_V1) {
            if row.ns != Ns::Business {
                return Err(bad().into());
            }
            (
                Kind::Agent,
                reader
                    .own(AgentBodyKeyV1::from_bytes(&row.key).map_err(ValueTreeError::Invalid)?)?,
            )
        } else if row.key.starts_with(MAILBOX_KEY_DOMAIN_V1) {
            if row.ns != Ns::Scheduler {
                return Err(bad().into());
            }
            let (id, ordinal) =
                MailboxKeyV1::from_bytes(&row.key).map_err(ValueTreeError::Invalid)?;
            if let Some(ordinal) = ordinal {
                if row.after.is_some() {
                    let key = MailboxKeyV1::new(id, None).map_err(ValueTreeError::Invalid)?;
                    let original = reader
                        .point(Ns::Scheduler, key.as_bytes())?
                        .ok_or_else(bad)?;
                    let frontier = MailboxFrontierV1::from_canonical_bytes(&reader.raw(&original)?)
                        .map_err(ValueTreeError::Invalid)?;
                    if ordinal < frontier.head || ordinal >= frontier.tail {
                        return Err(bad().into());
                    }
                }
            }
            (Kind::Agent, reader.own(id)?)
        } else if ModuleInstanceKeyV1::is_instance_key(&row.key) {
            if row.ns != Ns::Business {
                return Err(bad().into());
            }
            let (id, _) =
                ModuleInstanceKeyV1::from_bytes(&row.key).map_err(ValueTreeError::Invalid)?;
            (Kind::Instance, reader.own(id)?)
        } else {
            let (owner, field, ordinal) =
                AgentFieldKeyV1::from_bytes(&row.key).map_err(ValueTreeError::Invalid)?;
            if row.ns != field.namespace() {
                return Err(bad().into());
            }
            let identity = AgentFieldKeyV1::from_owner_digest(owner, F::Identity, 0)
                .map_err(ValueTreeError::Invalid)?;
            let identity_id = if let Some(id) = reader.point(Ns::Business, identity.as_bytes())? {
                id
            } else {
                let mut found = None;
                for original in &pending {
                    spend(
                        &p,
                        identity
                            .as_bytes()
                            .len()
                            .checked_add(original.key.len())
                            .ok_or_else(limit)?,
                        &mut reader.ctx.cancelled,
                    )?;
                    if original.ns == Ns::Business && original.key == identity.as_bytes() {
                        if found.is_some() {
                            return Err(bad().into());
                        }
                        found = original.before;
                    }
                }
                found.ok_or_else(bad)?
            };
            let raw = reader.raw(&identity_id)?;
            let AgentScalarRefV1::Text(id) =
                AgentScalarRefV1::from_canonical_bytes(&raw, F::Identity)
                    .map_err(ValueTreeError::Invalid)?
            else {
                return Err(bad().into());
            };
            spend(
                &p,
                id.len().checked_add(row.key.len()).ok_or_else(limit)?,
                &mut reader.ctx.cancelled,
            )?;
            if AgentFieldKeyV1::new(id, field, ordinal)
                .map_err(ValueTreeError::Invalid)?
                .as_bytes()
                != row.key
            {
                return Err(bad().into());
            }
            if row.after.is_some() {
                let next = match field {
                    F::SlotId | F::SlotType | F::SlotInstalled | F::SlotLocked => {
                        Some(reader.u64(id, F::SlotNext)?)
                    }
                    F::CargoId | F::CargoKind | F::CargoQuantity | F::CargoUnitSize => {
                        Some(reader.u64(id, F::CargoNext)?)
                    }
                    _ => None,
                };
                if next.is_some_and(|next| ordinal >= next) {
                    return Err(bad().into());
                }
            }
            (Kind::Agent, reader.own(id)?)
        };
        let mut seen = None;
        for (index, old) in targets.iter().enumerate() {
            spend(
                &p,
                old.key.len().checked_add(key.len()).ok_or_else(limit)?,
                &mut reader.ctx.cancelled,
            )?;
            if old.kind == kind && old.key == key {
                seen = Some(index);
                break;
            }
        }
        let index = if let Some(index) = seen {
            index
        } else {
            let index = targets.len();
            targets.push(Target { kind, key });
            index
        };
        owners.push(index);
    }
    for (index, target) in targets.iter().enumerate() {
        spend(&p, 0, &mut reader.ctx.cancelled)?;
        let semantic = semantic(&target.key, &memory, &p, &mut reader.ctx.cancelled)?;
        let key = json(&target.key, &memory, &p, &mut reader.ctx.cancelled)?;
        let (field, value, electricity, intent) = match target.kind {
            Kind::Agent => {
                let value = reader.agent(&target.key, objects, originals, decode)?;
                let electricity = value
                    .as_ref()
                    .map(|agent| agent.state.resources.get(ResourceKind::Electricity));
                (
                    "agents",
                    value
                        .as_ref()
                        .map(|v| json(v, &memory, &p, &mut reader.ctx.cancelled))
                        .transpose()?,
                    electricity,
                    value
                        .as_ref()
                        .map(|agent| json(&agent.intent, &memory, &p, &mut reader.ctx.cancelled))
                        .transpose()?,
                )
            }
            Kind::Instance => {
                let value = reader.instance(&target.key)?;
                (
                    "module_instances",
                    value
                        .as_ref()
                        .map(|v| json(v, &memory, &p, &mut reader.ctx.cancelled))
                        .transpose()?,
                    None,
                    None,
                )
            }
        };
        if value.is_none() {
            for (owner, row) in owners.iter().zip(&pending) {
                spend(&p, 0, &mut reader.ctx.cancelled)?;
                if *owner == index && row.after.is_some() {
                    return Err(bad().into());
                }
            }
            if target.kind == Kind::Agent {
                reader.verify_removed_agent(&target.key, &pending)?;
            }
        }
        rows.push(Row {
            field,
            semantic,
            key,
            value,
            electricity,
            intent,
        });
    }
    drop(owners);
    drop(targets);
    drop(reader);
    drop(pending);
    drop(pending_fee);
    if let Some(error) = (ctx.take_state_error)() {
        return Err(error.into());
    }
    Ok(CurrentAgentInstanceProjectionDeltaV1 {
        rows,
        guard: ctx.transaction.guard(),
        parent: ctx.transaction.parent_fence().parent_roots,
        memory,
        fee,
    })
}

#[allow(clippy::too_many_arguments)]
pub fn read_current_agent_instance_projection_delta_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    O: OriginalObjectStreamReader,
>(
    agents: &ReservedValue<AgentSubtreeStructureQualificationV1>,
    instances: &ReservedValue<ModuleInstanceFamilyQualificationV1>,
    roots: &RootSetV1,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    pending_entries: &mut usize,
    processing: &mut BusinessDiffProcessingWorkV1,
    objects: &mut O,
    originals: &mut FactBudget,
    decode: &mut WorldEventDecodeWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<CurrentAgentInstanceProjectionDeltaV1, AgentInstanceProjectionErrorV1<O::Error>> {
    let result = read_current_agent_instance_projection_delta_impl(
        agents,
        instances,
        roots,
        ctx,
        pending_entries,
        processing,
        objects,
        originals,
        decode,
        guard,
        live,
    );
    // MerkleReader's typed first cause takes precedence over its String
    // transport sentinel, on both success and refusal paths.
    if let Some(error) = (ctx.take_state_error)() {
        return Err(error.into());
    }
    result
}

#[path = "incremental_agent_instance_projection_delta_cold.rs"]
mod cold;
pub use cold::{
    QualifiedAgentInstanceProjectionColumnsV1, read_qualified_agent_instance_projection_columns_v1,
};
