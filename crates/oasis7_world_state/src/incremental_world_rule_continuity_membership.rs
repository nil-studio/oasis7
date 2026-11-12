//! Selected organization membership data and its fixed typed publication.
//! No World, arbitrary patch list or controller-authorization capability is accepted.
use super::*;
use crate::{
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1, lookup},
    incremental_inventory_value_read::InventoryValueReader,
    incremental_record_columns_read::{ColumnCellV1, ColumnReadProfileV1, read_column_row_v1},
    incremental_typed_record_codec::{Codec, Encoded, btree_output_heap},
    incremental_world_rule_composite_family::CompositeBranchCodecV1,
    incremental_world_rule_composite_overlay::{
        CompositePathPatchV1, WorldRuleCompositeOverlayV1, bind_current_world_rule_branch_v1,
        patch_world_rule_branch_v1, prepare_semantic_path_key_v1,
    },
};
use oasis7_formal_protocol::{
    controlled_authority::formal::{
        FormalError,
        bounded_context::{FormalBoundedContextV1, WorkIdentityV1},
    },
    incremental_execution::{
        ExecutionError, ExecutionFence, ExecutionTransaction, PreparedGuard, RootSetV1,
    },
    incremental_state::StateRootV1,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation};
use std::{collections::BTreeMap, mem::size_of};
type State = crate::world_rules::OrganizationContinuityStateV1;
type Binding = crate::world_rules::OrganizationControllerBindingV1;
type Row = (u64, Option<u64>, String);
type Histories = BTreeMap<String, Vec<Row>>;
type Bindings = BTreeMap<String, Binding>;
fn bad() -> ValueTreeError {
    ValueTreeError::Invalid("selected Current organization membership")
}
fn limited() -> ValueTreeError {
    ValueTreeError::ResourceLimited("organization membership layout")
}
#[derive(Clone, Copy)]
enum Key<'a> {
    Field(&'static str),
    Entry(&'a str),
}
fn key<R: ValueNodeReader, C: FnMut() -> bool>(
    key: Key<'_>,
    records: &mut InventoryValueReader<'_, '_, R, C>,
) -> Result<(WorldFieldKeyV1, u64), ValueTreeError> {
    let field = State::BRANCH.profile().field;
    match key {
        Key::Field(name) => records.key(field, WorldFieldRecordKindV1::Entry, name),
        Key::Entry(id) => records.key(field, WorldFieldRecordKindV1::Entry, &("entry", id)),
    }
}
/// One original typed path, with the exact member value ID retained separately
/// from any embedded organization/member identifier.
fn point<R, E, C>(
    root: &StateRootV1,
    path: &[Key<'_>],
    values: &mut oasis7_formal_protocol::incremental_value::ValueReadContext<'_, R>,
    key_work: &mut crate::incremental_semantic_key::SemanticKeyBudget,
    semantic_bytes: u64,
    record_bytes: u64,
    error: &mut E,
    cancelled: &mut C,
) -> Result<Option<([u8; 32], ColumnCellV1)>, WorldFieldOverlayErrorV1>
where
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let p = State::BRANCH.profile();
    let mut body = root.clone();
    for (index, part) in path.iter().enumerate() {
        if cancelled() {
            return Err(ValueTreeError::Cancelled.into());
        }
        let _frame = values
            .memory
            .reserve(
                size_of::<StateRootV1>()
                    + crate::incremental_world_field_key::WORLD_FIELD_KEY_MAX_BYTES
                    + 8192,
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        let mut records = InventoryValueReader {
            values: &mut *values,
            key_work: &mut *key_work,
            semantic_bytes,
            record_bytes,
            cancelled: &mut *cancelled,
        };
        let (key, _) = key(*part, &mut records)?;
        let Some(value) =
            crate::incremental_record_tree::lookup(&body, &key, &mut records, p.labels)?
        else {
            return Ok(None);
        };
        let entry = oasis7_formal_protocol::incremental_state::StateEntryV1 {
            namespace: p.namespace()?,
            key: key.as_bytes().to_vec(),
            value,
        };
        let row = read_column_row_v1(
            ColumnReadProfileV1 {
                field: p.field,
                schema: p.schema,
                groups: &[],
                reference_schema: p.reference_schema,
            },
            &entry,
            &mut records,
            usize::try_from(record_bytes).map_err(|_| bad())?,
        )?;
        if let Some(error) = error() {
            return Err(error.into());
        }
        if index + 1 == path.len() {
            let cell = match &row.cell {
                ColumnCellV1::Root(root) => ColumnCellV1::Root(root.clone()),
                ColumnCellV1::U64(n) => ColumnCellV1::U64(*n),
                _ => return Err(bad().into()),
            };
            return Ok(Some((value, cell)));
        }
        match &row.cell {
            ColumnCellV1::Root(next) => body = next.clone(),
            _ => return Err(bad().into()),
        }
    }
    Err(bad().into())
}
/// Only the selected registration's controller map and selected organization's
/// history map are materialized. Cases, assets and other organizations stay in
/// their original typed roots.
pub struct CurrentOrganizationMembershipV1 {
    source: Option<ReservedValue<WorldRuleCompositeOverlayV1<State>>>,
    organization: String,
    member: String,
    registered: bool,
    history: Option<RecoveredWorldRuleCompositeV1<Histories>>,
    history_value: Option<[u8; 32]>,
    history_length: u64,
    history_length_value: Option<[u8; 32]>,
    bindings: Option<RecoveredWorldRuleCompositeV1<Bindings>>,
    bindings_value: Option<[u8; 32]>,
    identity: PreparedGuard,
    memory: MemoryBudget,
    _header: MemoryReservation,
}
impl CurrentOrganizationMembershipV1 {
    pub fn registered(&self) -> bool {
        self.registered
    }
    pub fn history(&self) -> Option<&[(u64, Option<u64>, String)]> {
        self.history
            .as_ref()
            .and_then(|h| h.value().get(&self.member))
            .map(Vec::as_slice)
    }
    pub fn validate<M: MerkleNodeReader>(
        &self,
        tx: &mut ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        live: &ExecutionFence,
    ) -> Result<(), WorldFieldOverlayErrorV1> {
        tx.contribute(|tx| {
            tx.validate_guard(&self.identity, live)?;
            if !self.memory.same_pool(memory) || !tx.uses_budget(memory) {
                return Err(bad().into());
            }
            Ok(())
        })
    }
}
#[allow(clippy::too_many_arguments)]
pub fn read_current_organization_membership_v1<M, S, R, E, C>(
    parent: &ReservedValue<OrganizationContinuityQualificationV1>,
    roots: &RootSetV1,
    organization: &str,
    member: &str,
    state: &S,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut WorldRuleCompositeReadWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<CurrentOrganizationMembershipV1, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    S: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let result = (|| {
        context.transaction.validate_guard(guard, live)?;
        parent.check_budget(context.values.memory, MemoryClass::Ordinary)?;
        if !parent.uses_budget(context.values.memory)
            || parent.reserved_bytes() < 512
            || context.values.class != MemoryClass::Ordinary
            || !context.transaction.uses_budget(context.values.memory)
        {
            return Err(bad().into());
        }
        let _phase = context
            .values
            .memory
            .reserve(4096, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        roots.validate()?;
        parent.validate_binding(roots)?;
        if context.transaction.parent_fence().parent_roots != roots.digest()? {
            return Err(bad().into());
        }
        let layout = size_of::<CurrentOrganizationMembershipV1>()
            .checked_add(organization.len())
            .and_then(|n| n.checked_add(member.len()))
            .ok_or_else(limited)?;
        let header = context
            .values
            .memory
            .reserve(layout, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let present = {
            let mut records = InventoryValueReader {
                values: &mut context.values,
                key_work: &mut *context.key_work,
                semantic_bytes: context.semantic_bytes,
                record_bytes: context.record_bytes,
                cancelled: &mut context.cancelled,
            };
            let (field, present) = crate::incremental_world_rule_parent::read_parent_overlay(
                context.transaction,
                &mut records,
                &mut context.take_state_error,
            )?;
            if !present {
                let key = records
                    .key(
                        field,
                        WorldFieldRecordKindV1::Entry,
                        &(State::BRANCH.name(), ()),
                    )?
                    .0;
                if lookup(
                    context.transaction,
                    &key,
                    &mut records,
                    &mut context.take_state_error,
                )?
                .is_some()
                {
                    return Err(bad().into());
                }
            }
            present
        };
        let mut source = None;
        let mut registered = false;
        let mut history = None;
        let mut history_value = None;
        let mut history_length = 0;
        let mut history_length_value = None;
        let mut bindings = None;
        let mut bindings_value = None;
        if present {
            let guard = context.transaction.guard();
            let proof = bind_current_world_rule_branch_v1(
                parent, roots, state, context, scan, work, &guard, live,
            )?;
            let mut get = |path: &[Key<'_>]| {
                point(
                    proof.body(),
                    path,
                    &mut context.values,
                    &mut *context.key_work,
                    context.semantic_bytes,
                    context.record_bytes,
                    &mut context.take_state_error,
                    &mut context.cancelled,
                )
            };
            registered = match get(&[Key::Field("registrations"), Key::Entry(organization)])? {
                None => false,
                Some((_, ColumnCellV1::Root(_))) => true,
                _ => return Err(bad().into()),
            };
            if registered {
                let (value, length) =
                    get(&[Key::Field("membership_history"), Key::Field("_length")])?
                        .ok_or_else(bad)?;
                history_length = match length {
                    ColumnCellV1::U64(n) => n,
                    _ => return Err(bad().into()),
                };
                history_length_value = Some(value);
                let h = get(&[Key::Field("membership_history"), Key::Entry(organization)])?;
                let b = get(&[
                    Key::Field("registrations"),
                    Key::Entry(organization),
                    Key::Field("membership_controller_bindings"),
                ])?
                .ok_or_else(bad)?;
                drop(get);
                match h {
                    Some((value, ColumnCellV1::Root(body))) => {
                        let mut read = WorldFieldReadContextV1 {
                            root: &body,
                            state,
                            values: oasis7_formal_protocol::incremental_value::ValueReadContext {
                                source: context.values.source,
                                memory: context.values.memory,
                                class: context.values.class,
                                io: &mut *context.values.io,
                            },
                            key_work: &mut *context.key_work,
                            semantic_bytes: context.semantic_bytes,
                            record_bytes: context.record_bytes,
                            take_state_error: &mut context.take_state_error,
                            cancelled: &mut context.cancelled,
                        };
                        history = Some(recover_typed_record_body_v1::<Histories, _, _, _, _>(
                            &body,
                            &State::BRANCH.profile(),
                            &mut read,
                            scan,
                            work,
                        )?);
                        history_value = Some(value);
                    }
                    None => {}
                    _ => return Err(bad().into()),
                }
                let (value, ColumnCellV1::Root(body)) = b else {
                    return Err(bad().into());
                };
                let mut read = WorldFieldReadContextV1 {
                    root: &body,
                    state,
                    values: oasis7_formal_protocol::incremental_value::ValueReadContext {
                        source: context.values.source,
                        memory: context.values.memory,
                        class: context.values.class,
                        io: &mut *context.values.io,
                    },
                    key_work: &mut *context.key_work,
                    semantic_bytes: context.semantic_bytes,
                    record_bytes: context.record_bytes,
                    take_state_error: &mut context.take_state_error,
                    cancelled: &mut context.cancelled,
                };
                bindings = Some(recover_typed_record_body_v1::<Bindings, _, _, _, _>(
                    &body,
                    &State::BRANCH.profile(),
                    &mut read,
                    scan,
                    work,
                )?);
                bindings_value = Some(value);
            }
            source = Some(proof);
        }
        if (context.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(CurrentOrganizationMembershipV1 {
            source,
            organization: organization.to_owned(),
            member: member.to_owned(),
            registered,
            history,
            history_value,
            history_length,
            history_length_value,
            bindings,
            bindings_value,
            identity: context.transaction.guard(),
            memory: context.values.memory.clone(),
            _header: header,
        })
    })();
    if result.is_err() {
        let _: Result<(), ExecutionError> = context
            .transaction
            .contribute(|_| Err(ExecutionError::Aborted));
    }
    result
}

#[derive(Debug)]
pub enum ContinuityMembershipErrorV1 {
    World(crate::WorldError),
    Overlay(WorldFieldOverlayErrorV1),
    Work(FormalError),
}
impl From<crate::WorldError> for ContinuityMembershipErrorV1 {
    fn from(error: crate::WorldError) -> Self {
        Self::World(error)
    }
}
impl From<WorldFieldOverlayErrorV1> for ContinuityMembershipErrorV1 {
    fn from(error: WorldFieldOverlayErrorV1) -> Self {
        Self::Overlay(error)
    }
}
impl From<ValueTreeError> for ContinuityMembershipErrorV1 {
    fn from(error: ValueTreeError) -> Self {
        Self::Overlay(error.into())
    }
}
impl From<FormalError> for ContinuityMembershipErrorV1 {
    fn from(error: FormalError) -> Self {
        Self::Work(error)
    }
}
impl std::fmt::Display for ContinuityMembershipErrorV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::World(crate::WorldError::ResourceBalanceInvalid { reason }) => {
                f.write_str(reason)
            }
            Self::World(error) => write!(f, "{error:?}"),
            Self::Overlay(error) => error.fmt(f),
            Self::Work(error) => error.fmt(f),
        }
    }
}
impl std::error::Error for ContinuityMembershipErrorV1 {}
fn charge(
    common: &mut FormalBoundedContextV1<'_>,
    n: usize,
) -> Result<(), ContinuityMembershipErrorV1> {
    common.charge(1, u64::try_from(n).map_err(|_| limited())?)?;
    Ok(())
}
fn copy(
    value: &str,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<String, ContinuityMembershipErrorV1> {
    charge(common, value.len())?;
    let text = value.to_owned();
    if text.capacity() > value.len() {
        return Err(limited().into());
    }
    Ok(text)
}
/// This fixed verb is data. It grants neither agency nor publication permission.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContinuityMembershipKindV1 {
    Joined,
    Left,
}
/// The actual Current source and all prepared copies/fees are owned, not cloned
/// into a fake WorldRules state. Only the two selected maps can change.
pub struct PreparedContinuityMembershipV1 {
    source: CurrentOrganizationMembershipV1,
    plan: crate::world_rules::MembershipEventPlanV1,
    histories: Option<ReservedValue<Histories>>,
    bindings: Option<ReservedValue<Bindings>>,
    identity: WorkIdentityV1,
    memory: MemoryBudget,
    _header: MemoryReservation,
}
impl PreparedContinuityMembershipV1 {
    pub fn requires_controller(&self) -> bool {
        self.plan == crate::world_rules::MembershipEventPlanV1::Join && self.bindings.is_none()
    }
    /// The Runtime owner must derive this data from actual Current controller
    /// records. This storage operation does not authenticate a caller account.
    pub fn with_controller(
        mut self,
        binding: &Binding,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<Self, ContinuityMembershipErrorV1> {
        if self.identity != common.work().identity()
            || !self.memory.same_pool(common.memory())
            || !self.requires_controller()
        {
            return Err(bad().into());
        }
        charge(common, binding.root_agent_id.len())?;
        charge(common, binding.player_id.len())?;
        charge(common, binding.source_id.len())?;
        if binding.root_agent_id.is_empty()
            || binding.player_id.is_empty()
            || binding.source_id.is_empty()
            || binding.generation == 0
        {
            return Err(
                crate::world_rules::invalid("organization_controller_binding_incomplete").into(),
            );
        }
        self.bindings = Some(copy_bindings(&self.source, Some(binding), false, common)?);
        Ok(self)
    }
}
fn probe_keys<'a>(
    keys: impl Iterator<Item = &'a String>,
    target: &str,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<(), ContinuityMembershipErrorV1> {
    for key in keys {
        charge(common, key.len())?;
        charge(common, target.len())?;
    }
    Ok(())
}
fn copy_histories(
    source: &CurrentOrganizationMembershipV1,
    plan: crate::world_rules::MembershipEventPlanV1,
    tick: u64,
    event: &str,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<ReservedValue<Histories>, ContinuityMembershipErrorV1> {
    use crate::world_rules::MembershipEventPlanV1 as P;
    let before = source.history.as_ref().map(|h| h.value());
    if let Some(before) = before {
        probe_keys(before.keys(), &source.member, common)?;
    }
    let existed = before.is_some_and(|h| h.contains_key(&source.member));
    let n = before
        .map_or(0, BTreeMap::len)
        .checked_add(usize::from(!existed))
        .ok_or_else(limited)?;
    let mut layout = btree_output_heap::<String, Vec<Row>>(n)?
        .checked_add(size_of::<Histories>())
        .ok_or_else(limited)?;
    let mut key_bytes = source.member.len();
    if let Some(before) = before {
        for (member, events) in before {
            charge(common, member.len())?;
            key_bytes = key_bytes.checked_add(member.len()).ok_or_else(limited)?;
            layout = layout
                .checked_add(member.len())
                .and_then(|v| v.checked_add(events.len().checked_mul(size_of::<Row>())?))
                .ok_or_else(limited)?;
            for (_, _, reference) in events {
                charge(common, reference.len())?;
                layout = layout.checked_add(reference.len()).ok_or_else(limited)?;
            }
        }
    }
    if !existed {
        layout = layout
            .checked_add(source.member.len())
            .ok_or_else(limited)?;
    }
    if plan == P::Join {
        layout = layout
            .checked_add(size_of::<Row>())
            .and_then(|v| v.checked_add(event.len()))
            .ok_or_else(limited)?;
    }
    let fee = common
        .memory()
        .reserve(layout, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let mut out = Histories::new();
    let mut add =
        |member: &str, events: &[Row], target: bool| -> Result<(), ContinuityMembershipErrorV1> {
            let capacity = events
                .len()
                .checked_add(usize::from(target && plan == P::Join))
                .ok_or_else(limited)?;
            charge(
                common,
                capacity.checked_mul(size_of::<Row>()).ok_or_else(limited)?,
            )?;
            let mut copied = Vec::with_capacity(capacity);
            if copied.capacity() > capacity {
                return Err(limited().into());
            }
            for (at, left, reference) in events {
                copied.push((*at, *left, copy(reference, common)?));
            }
            if target {
                match plan {
                    P::Join => copied.push((tick, None, copy(event, common)?)),
                    P::Close => {
                        if let Some((_, left, _)) = copied.last_mut() {
                            *left = Some(tick);
                        }
                    }
                    _ => {}
                }
            }
            charge(
                common,
                key_bytes
                    .checked_add(n.checked_mul(member.len()).ok_or_else(limited)?)
                    .and_then(|v| v.checked_add(btree_output_heap::<String, Vec<Row>>(n).ok()?))
                    .ok_or_else(limited)?,
            )?;
            out.insert(copy(member, common)?, copied);
            Ok(())
        };
    if let Some(before) = before {
        for (member, events) in before {
            add(member, events, member == &source.member)?;
        }
    }
    if !existed {
        add(&source.member, &[], true)?;
    }
    drop(add);
    Ok(fee.retain(out))
}
fn copy_bindings(
    source: &CurrentOrganizationMembershipV1,
    after: Option<&Binding>,
    remove: bool,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<ReservedValue<Bindings>, ContinuityMembershipErrorV1> {
    let before = source.bindings.as_ref().ok_or_else(bad)?.value();
    probe_keys(before.keys(), &source.member, common)?;
    let existed = before.contains_key(&source.member);
    let n = before
        .len()
        .checked_add(usize::from(after.is_some() && !existed))
        .and_then(|n| n.checked_sub(usize::from(remove && existed)))
        .ok_or_else(limited)?;
    let mut layout = btree_output_heap::<String, Binding>(n)?
        .checked_add(size_of::<Bindings>())
        .ok_or_else(limited)?;
    let mut key_bytes = source.member.len();
    for (id, value) in before {
        charge(common, id.len())?;
        key_bytes = key_bytes.checked_add(id.len()).ok_or_else(limited)?;
        if id == &source.member && (remove || after.is_some()) {
            continue;
        }
        for text in [id, &value.root_agent_id, &value.player_id, &value.source_id] {
            charge(common, text.len())?;
            layout = layout.checked_add(text.len()).ok_or_else(limited)?;
        }
    }
    if let Some(after) = after {
        for text in [
            &source.member,
            &after.root_agent_id,
            &after.player_id,
            &after.source_id,
        ] {
            charge(common, text.len())?;
            layout = layout.checked_add(text.len()).ok_or_else(limited)?;
        }
    }
    let fee = common
        .memory()
        .reserve(layout, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let mut out = Bindings::new();
    let mut add = |id: &str, value: &Binding| -> Result<(), ContinuityMembershipErrorV1> {
        let copied = Binding {
            root_agent_id: copy(&value.root_agent_id, common)?,
            player_id: copy(&value.player_id, common)?,
            source_id: copy(&value.source_id, common)?,
            generation: value.generation,
        };
        charge(
            common,
            key_bytes
                .checked_add(n.checked_mul(id.len()).ok_or_else(limited)?)
                .and_then(|v| v.checked_add(btree_output_heap::<String, Binding>(n).ok()?))
                .ok_or_else(limited)?,
        )?;
        out.insert(copy(id, common)?, copied);
        Ok(())
    };
    for (id, value) in before {
        if id == &source.member && (remove || after.is_some()) {
            continue;
        }
        add(id, value)?;
    }
    if let Some(after) = after {
        add(&source.member, after)?;
    }
    drop(add);
    Ok(fee.retain(out))
}
/// Preflight first, before a genuine Accepted audit. Controller resolution is
/// deferred until the original duplicate/open-history predicates have passed.
#[allow(clippy::too_many_arguments)]
pub fn prepare_current_continuity_membership_v1<M: MerkleNodeReader>(
    source: CurrentOrganizationMembershipV1,
    kind: ContinuityMembershipKindV1,
    tick: u64,
    event_ref: &str,
    tx: &mut ExecutionTransaction<'_, M>,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<PreparedContinuityMembershipV1, ContinuityMembershipErrorV1> {
    source.validate(tx, common.memory(), live)?;
    let header = common
        .memory()
        .reserve(
            size_of::<PreparedContinuityMembershipV1>(),
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?;
    if let Some(history) = source.history.as_ref() {
        probe_keys(history.value().keys(), &source.member, common)?;
    }
    let history = source.history();
    let had_history = history.is_some();
    let plan = crate::world_rules::plan_membership_event_with_work_v1(
        source.registered,
        history,
        &source.member,
        tick,
        event_ref,
        kind == ContinuityMembershipKindV1::Joined,
        &mut |n| charge(common, n),
    )?;
    use crate::world_rules::MembershipEventPlanV1 as P;
    let histories = if plan == P::Noop || (plan == P::EnsureHistory && had_history) {
        None
    } else {
        Some(copy_histories(&source, plan, tick, event_ref, common)?)
    };
    if plan == P::Close {
        if let Some(bindings) = source.bindings.as_ref() {
            probe_keys(bindings.value().keys(), &source.member, common)?;
        }
    }
    let bindings = if plan == P::Close
        && source
            .bindings
            .as_ref()
            .is_some_and(|b| b.value().contains_key(&source.member))
    {
        Some(copy_bindings(&source, None, true, common)?)
    } else {
        None
    };
    Ok(PreparedContinuityMembershipV1 {
        source,
        plan,
        histories,
        bindings,
        identity: common.work().identity(),
        memory: common.memory().clone(),
        _header: header,
    })
}

impl From<crate::incremental_field_member::MemberRecordError> for ContinuityMembershipErrorV1 {
    fn from(error: crate::incremental_field_member::MemberRecordError) -> Self {
        Self::Overlay(WorldFieldOverlayErrorV1::Member(error))
    }
}
fn prepare_path<W: ValueNodeReader + StatePageWriter, C: FnMut() -> bool>(
    keys: &[Key<'_>],
    write: &mut MemberRecordWriteContext<'_, W, C>,
) -> Result<Vec<ReservedValue<Vec<u8>>>, ContinuityMembershipErrorV1> {
    let mut out = Vec::with_capacity(keys.len());
    if out.capacity() > keys.len() {
        return Err(limited().into());
    }
    for key in keys {
        out.push(match key {
            Key::Field(name) => prepare_semantic_path_key_v1(name, write)?,
            Key::Entry(id) => prepare_semantic_path_key_v1(&("entry", *id), write)?,
        });
    }
    Ok(out)
}
/// Publish only fixed history/controller-map paths. The actual branch is opened
/// again and compared to the prepared body before any edit; an identity check
/// or a newly signed guard alone never makes a stale observation current.
#[allow(clippy::too_many_arguments)]
pub fn apply_prepared_continuity_membership_v1<M, S, R, E, C, W, CW>(
    prepared: PreparedContinuityMembershipV1,
    parent: &ReservedValue<OrganizationContinuityQualificationV1>,
    roots: &RootSetV1,
    state: &S,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    scan: &mut StateScanBudget,
    read_work: &mut WorldRuleCompositeReadWorkV1,
    write_work: &mut WorldRuleCompositeWriteWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<Option<ReservedValue<OrganizationContinuityOverlayV1>>, ContinuityMembershipErrorV1>
where
    M: MerkleNodeReader,
    S: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    CW: FnMut() -> bool,
{
    let result = (|| {
        context
            .transaction
            .validate_guard(guard, live)
            .map_err(WorldFieldOverlayErrorV1::from)?;
        context
            .transaction
            .validate_transaction_identity(&prepared.source.identity)
            .map_err(WorldFieldOverlayErrorV1::from)?;
        charge(common, 0)?;
        if prepared.identity != common.work().identity()
            || !prepared.memory.same_pool(common.memory())
            || !context.values.memory.same_pool(common.memory())
            || !write.memory.same_pool(common.memory())
        {
            return Err(bad().into());
        }
        if prepared.requires_controller() {
            return Err(
                crate::world_rules::invalid("organization_controller_binding_unavailable").into(),
            );
        }
        let source = &prepared.source;
        let Some(before) = source.source.as_ref() else {
            parent.check_budget(common.memory(), MemoryClass::Ordinary)?;
            roots.validate().map_err(WorldFieldOverlayErrorV1::from)?;
            parent.validate_binding(roots)?;
            let _phase = common
                .memory()
                .reserve(4096, MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?;
            if context.transaction.parent_fence().parent_roots
                != roots.digest().map_err(WorldFieldOverlayErrorV1::from)?
            {
                return Err(bad().into());
            }
            let mut records = InventoryValueReader {
                values: &mut context.values,
                key_work: &mut *context.key_work,
                semantic_bytes: context.semantic_bytes,
                record_bytes: context.record_bytes,
                cancelled: &mut context.cancelled,
            };
            let (field, present) = crate::incremental_world_rule_parent::read_parent_overlay(
                context.transaction,
                &mut records,
                &mut context.take_state_error,
            )?;
            if present {
                return Err(bad().into());
            }
            let key = records
                .key(
                    field,
                    WorldFieldRecordKindV1::Entry,
                    &(State::BRANCH.name(), ()),
                )?
                .0;
            if lookup(
                context.transaction,
                &key,
                &mut records,
                &mut context.take_state_error,
            )?
            .is_some()
            {
                return Err(bad().into());
            }
            return Ok(None);
        };
        let fresh_guard = context.transaction.guard();
        let actual = bind_current_world_rule_branch_v1(
            parent,
            roots,
            state,
            context,
            scan,
            read_work,
            &fresh_guard,
            live,
        )?;
        charge(common, size_of::<StateRootV1>() * 2)?;
        if actual.body() != before.body() {
            return Err(bad().into());
        }
        if prepared.histories.is_none() && prepared.bindings.is_none() {
            return Ok(None);
        }
        // At most three fixed patches, with depths 2,3,2. All carrier Vecs are
        // admitted before capacity allocation; ID-sized keys have their own fee.
        let outer = size_of::<Vec<Vec<ReservedValue<Vec<u8>>>>>()
            + size_of::<Vec<Option<Encoded<'_>>>>()
            + size_of::<Vec<Option<[u8; 32]>>>()
            + size_of::<Vec<Vec<&[u8]>>>()
            + size_of::<Vec<CompositePathPatchV1<'_>>>();
        let slot = size_of::<Vec<ReservedValue<Vec<u8>>>>()
            + size_of::<Option<Encoded<'_>>>()
            + size_of::<Option<[u8; 32]>>()
            + size_of::<Vec<&[u8]>>()
            + size_of::<CompositePathPatchV1<'_>>();
        let layout = slot
            .checked_mul(3)
            .and_then(|n| n.checked_add(outer))
            .and_then(|n| {
                n.checked_add(9 * (size_of::<ReservedValue<Vec<u8>>>() + size_of::<&[u8]>()))
            })
            .ok_or_else(limited)?;
        let _paths = common
            .memory()
            .reserve(layout, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        charge(common, layout)?;
        let mut paths = Vec::with_capacity(3);
        let mut after = Vec::with_capacity(3);
        let mut expected = Vec::with_capacity(3);
        if paths.capacity() > 3 || after.capacity() > 3 || expected.capacity() > 3 {
            return Err(limited().into());
        }
        let profile = State::BRANCH.profile();
        if let Some(history) = &prepared.histories {
            paths.push(prepare_path(
                &[
                    Key::Field("membership_history"),
                    Key::Entry(&source.organization),
                ],
                write,
            )?);
            expected.push(source.history_value);
            after.push(Some(history.encode(&profile, write, write_work)?));
            if source.history_value.is_none() {
                paths.push(prepare_path(
                    &[Key::Field("membership_history"), Key::Field("_length")],
                    write,
                )?);
                expected.push(source.history_length_value);
                after.push(Some(Encoded::Scalar(MemberPayloadRefV1::U64(
                    source.history_length.checked_add(1).ok_or_else(bad)?,
                ))));
            }
        }
        if let Some(bindings) = &prepared.bindings {
            paths.push(prepare_path(
                &[
                    Key::Field("registrations"),
                    Key::Entry(&source.organization),
                    Key::Field("membership_controller_bindings"),
                ],
                write,
            )?);
            expected.push(source.bindings_value);
            after.push(Some(bindings.encode(&profile, write, write_work)?));
        }
        let mut views = Vec::with_capacity(paths.len());
        if views.capacity() > 3 {
            return Err(limited().into());
        }
        for path in &paths {
            let mut view = Vec::with_capacity(path.len());
            if view.capacity() > 3 {
                return Err(limited().into());
            }
            for key in path {
                view.push(key.as_slice());
            }
            views.push(view);
        }
        let mut changes = Vec::with_capacity(views.len());
        if changes.capacity() > 3 {
            return Err(limited().into());
        }
        for ((path, expected), after) in views.iter().zip(expected).zip(after) {
            changes.push(CompositePathPatchV1 {
                path,
                expected,
                after,
            });
        }
        let fresh_guard = context.transaction.guard();
        Ok(patch_world_rule_branch_v1(
            parent,
            roots,
            Some(&actual),
            changes,
            state,
            context,
            write,
            scan,
            read_work,
            write_work,
            &fresh_guard,
            live,
        )?)
    })();
    if result.is_err() {
        let _: Result<(), ExecutionError> = context
            .transaction
            .contribute(|_| Err(ExecutionError::Aborted));
    }
    result
}
