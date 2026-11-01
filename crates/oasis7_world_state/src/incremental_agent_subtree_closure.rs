//! Full structural bootstrap with bounded admitted metadata, not a history witness.
use super::*;
use crate::{
    incremental_agent_body::{AGENT_BODY_VALUE_BYTES, AgentBodyValueV1},
    incremental_event_fields::{
        MAILBOX_EVENT_REFERENCE_BYTES, MAILBOX_FRONTIER_BYTES, MailboxFrontierV1, MailboxKeyV1,
        mailbox_event_id_v1,
    },
    incremental_expected_bytes::Compare,
    incremental_field_read::WorldFieldReadContextV1,
};
use oasis7_formal_protocol::{
    incremental_execution::RootSetV1,
    incremental_state::{MerkleNodeReader, StateScanBudget, StateScanContext, StateScanError},
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::ReservedValue;
use schema::{Group, group};
use std::{cell::RefCell, collections::BTreeMap};
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("agent subtree structural closure")
}
struct EntryRef<'a> {
    namespace: oasis7_formal_protocol::incremental_state::StateNamespaceV1,
    key: &'a [u8],
    value: [u8; 32],
}
fn verify_entry<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    F: FnMut(AgentMailboxReferenceV1<'_>) -> Result<(), ValueTreeError>,
>(
    roots: &RootSetV1,
    entry: EntryRef<'_>,
    local: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut AgentSubtreeScanWorkV1,
    table: &mut BTreeMap<[u8; 32], Stats>,
    observe: &mut F,
) -> Result<(), ValueTreeError> {
    work.cells_remaining = work
        .cells_remaining
        .checked_sub(1)
        .ok_or(ValueTreeError::ResourceLimited("agent structural cells"))?;
    let Some(parsed) = parse_agent_subtree_key_v1(entry.namespace, entry.key)? else {
        return Ok(());
    };
    let max = match parsed {
        AgentSubtreeKeyV1::Field { .. } => work.max_scalar_bytes,
        AgentSubtreeKeyV1::Body { .. } => AGENT_BODY_VALUE_BYTES as u64,
        AgentSubtreeKeyV1::Mailbox { ordinal: None, .. } => MAILBOX_FRONTIER_BYTES as u64,
        AgentSubtreeKeyV1::Mailbox {
            ordinal: Some(_), ..
        } => MAILBOX_EVENT_REFERENCE_BYTES as u64,
    };
    local
        .values
        .verify_closure(&entry.value, max, &mut local.cancelled)?;
    match parsed {
        AgentSubtreeKeyV1::Field {
            owner,
            field,
            ordinal,
        } => {
            read_agent_scalar_tree_v1(
                field,
                &entry.value,
                &mut local.values,
                work.max_scalar_bytes,
                &mut std::io::sink(),
                &mut local.cancelled,
            )?;
            if matches!(group(field), Group::Slot | Group::Cargo) {
                row(roots, owner, field, ordinal, local)?;
            }
            let s = stats_entry(table, owner, work.max_agents)?;
            if !field.indexed() {
                s.mask |= schema::bit(field);
            }
            if group(field) == Group::Slot {
                s.slot_max = Some(s.slot_max.map_or(ordinal, |n| n.max(ordinal)));
                if field == AgentFieldV1::SlotId {
                    s.slots = s.slots.checked_add(1).ok_or_else(invalid)?;
                }
            }
            if group(field) == Group::Cargo {
                s.cargo_max = Some(s.cargo_max.map_or(ordinal, |n| n.max(ordinal)));
                if field == AgentFieldV1::CargoId {
                    s.cargo = s.cargo.checked_add(1).ok_or_else(invalid)?;
                }
            }
        }
        AgentSubtreeKeyV1::Body { agent } => {
            let digest = owner(agent, local.key_work, local.semantic_bytes)?;
            AgentBodyValueV1::from_canonical_bytes(&read::fixed_tree::<
                AGENT_BODY_VALUE_BYTES,
                _,
                _,
            >(
                &entry.value,
                &mut local.values,
                &mut local.cancelled,
            )?)
            .map_err(ValueTreeError::Invalid)?;
            let expected = anchor(roots, agent, digest, local, work.max_scalar_bytes)?;
            let s = stats_entry(table, digest, work.max_agents)?;
            if s.expected.is_some() {
                return Err(invalid());
            }
            s.expected = Some(expected);
        }
        AgentSubtreeKeyV1::Mailbox { agent, ordinal } => {
            let digest = owner(agent, local.key_work, local.semantic_bytes)?;
            if let Some(ordinal) = ordinal {
                let original =
                    mailbox_event_id_v1(&read::fixed_tree::<MAILBOX_EVENT_REFERENCE_BYTES, _, _>(
                        &entry.value,
                        &mut local.values,
                        &mut local.cancelled,
                    )?)
                    .map_err(ValueTreeError::Invalid)?;
                observe(AgentMailboxReferenceV1 {
                    agent,
                    ordinal,
                    original,
                })?;
                let s = stats_entry(table, digest, work.max_agents)?;
                s.mail = s.mail.checked_add(1).ok_or_else(invalid)?;
                s.mail_min = Some(s.mail_min.map_or(ordinal, |n| n.min(ordinal)));
                s.mail_max = Some(s.mail_max.map_or(ordinal, |n| n.max(ordinal)));
            } else {
                MailboxFrontierV1::from_canonical_bytes(&read::fixed_tree::<
                    MAILBOX_FRONTIER_BYTES,
                    _,
                    _,
                >(
                    &entry.value,
                    &mut local.values,
                    &mut local.cancelled,
                )?)
                .map_err(ValueTreeError::Invalid)?;
                let s = stats_entry(table, digest, work.max_agents)?;
                if s.frontier {
                    return Err(invalid());
                }
                s.frontier = true;
            }
        }
    }
    local.check()?;
    Ok(())
}
/// Exact original state structure only. External original/history facts have
/// not been certified by this private-constructor, non-Clone capability.
#[derive(Debug)]
pub struct AgentSubtreeStructureQualificationV1 {
    roots: [u8; 32],
    agents: u64,
    mailbox_references: u64,
}
impl AgentSubtreeStructureQualificationV1 {
    pub fn validate_binding(&self, roots: &RootSetV1) -> Result<(), ValueTreeError> {
        if roots.digest().map_err(|_| invalid())? != self.roots {
            return Err(invalid());
        }
        Ok(())
    }
    pub(super) fn validate_digest(&self, roots: &[u8; 32]) -> Result<(), ValueTreeError> {
        if &self.roots != roots {
            return Err(invalid());
        }
        Ok(())
    }
    pub fn agent_count(&self) -> u64 {
        self.agents
    }
    pub fn mailbox_reference_count(&self) -> u64 {
        self.mailbox_references
    }
}
pub struct AgentSubtreeScanWorkV1 {
    /// Finite operation admission for scratch counters; not a format limit.
    pub max_agents: usize,
    pub cells_remaining: u64,
    pub max_scalar_bytes: u64,
}
pub struct AgentMailboxReferenceV1<'a> {
    pub agent: &'a str,
    pub ordinal: u64,
    pub original: [u8; 32],
}
#[derive(Clone, Copy)]
struct Expected {
    mask: u64,
    slots: u64,
    slot_next: u64,
    cargo: u64,
    cargo_next: u64,
    head: u64,
    tail: u64,
}
#[derive(Default)]
struct Stats {
    expected: Option<Expected>,
    mask: u64,
    slots: u64,
    cargo: u64,
    mail: u64,
    slot_max: Option<u64>,
    cargo_max: Option<u64>,
    mail_min: Option<u64>,
    mail_max: Option<u64>,
    frontier: bool,
}
fn stats_entry(
    table: &mut BTreeMap<[u8; 32], Stats>,
    owner: [u8; 32],
    max: usize,
) -> Result<&mut Stats, ValueTreeError> {
    if !table.contains_key(&owner) && table.len() >= max {
        return Err(ValueTreeError::ResourceLimited("agent structural owners"));
    }
    Ok(table.entry(owner).or_default())
}
fn key(
    owner: [u8; 32],
    field: AgentFieldV1,
    ordinal: u64,
) -> Result<AgentFieldKeyV1, ValueTreeError> {
    AgentFieldKeyV1::from_owner_digest(owner, field, ordinal).map_err(ValueTreeError::Invalid)
}
fn lookup<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    roots: &RootSetV1,
    owner: [u8; 32],
    field: AgentFieldV1,
    ordinal: u64,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<[u8; 32], ValueTreeError> {
    let k = key(owner, field, ordinal)?;
    ctx.lookup_raw(roots.state_root(field.namespace()), k.as_bytes())?
        .ok_or_else(invalid)
}
fn control<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    roots: &RootSetV1,
    owner: [u8; 32],
    field: AgentFieldV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<AgentSubtreeValueV1, ValueTreeError> {
    let id = lookup(roots, owner, field, 0, ctx)?;
    read_agent_scalar_tree_v1(
        field,
        &id,
        &mut ctx.values,
        64,
        &mut std::io::sink(),
        &mut ctx.cancelled,
    )
}
fn number(value: AgentSubtreeValueV1) -> Result<u64, ValueTreeError> {
    if let AgentSubtreeValueV1::U64(n) = value {
        Ok(n)
    } else {
        Err(invalid())
    }
}
fn boolean(value: AgentSubtreeValueV1) -> Result<bool, ValueTreeError> {
    if let AgentSubtreeValueV1::Bool(n) = value {
        Ok(n)
    } else {
        Err(invalid())
    }
}
pub(super) fn owner(
    agent: &str,
    work: &mut crate::incremental_semantic_key::SemanticKeyBudget,
    maximum: u64,
) -> Result<[u8; 32], ValueTreeError> {
    if agent.len() as u64 > maximum {
        return Err(ValueTreeError::ResourceLimited("agent identity allowance"));
    }
    work.bytes_remaining = work
        .bytes_remaining
        .checked_sub(AGENT_FIELD_KEY_BYTES as u64 + agent.len() as u64)
        .ok_or(ValueTreeError::ResourceLimited("agent identity work"))?;
    let k =
        AgentFieldKeyV1::new(agent, AgentFieldV1::Identity, 0).map_err(ValueTreeError::Invalid)?;
    Ok(AgentFieldKeyV1::from_bytes(k.as_bytes())
        .map_err(ValueTreeError::Invalid)?
        .0)
}
fn anchor<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    roots: &RootSetV1,
    agent: &str,
    owner: [u8; 32],
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    max: u64,
) -> Result<Expected, ValueTreeError> {
    use AgentFieldV1 as F;
    let activity = boolean(control(roots, owner, F::ActivityPresent, ctx)?)?;
    let intent = boolean(control(roots, owner, F::IntentPresent, ctx)?)?;
    let mask = schema::required(activity, intent);
    for &field in AgentFieldV1::ALL {
        if mask & schema::bit(field) != 0 {
            lookup(roots, owner, field, 0, ctx)?;
        }
    }
    for field in [Some(F::Identity), intent.then_some(F::IntentAgent)]
        .into_iter()
        .flatten()
    {
        let id = lookup(roots, owner, field, 0, ctx)?;
        let mut compare = Compare::new(agent.as_bytes());
        let value = read_agent_scalar_tree_v1(
            field,
            &id,
            &mut ctx.values,
            max,
            &mut compare,
            &mut ctx.cancelled,
        )?;
        if value
            != (AgentSubtreeValueV1::Text {
                utf8_bytes: agent.len() as u64,
            })
            || !compare.is_equal()
        {
            return Err(invalid());
        }
    }
    let frontier = MailboxKeyV1::new(agent, None).map_err(ValueTreeError::Invalid)?;
    let id = ctx
        .lookup_raw(
            roots
                .state_root(oasis7_formal_protocol::incremental_state::StateNamespaceV1::Scheduler),
            frontier.as_bytes(),
        )?
        .ok_or_else(invalid)?;
    let frontier =
        MailboxFrontierV1::from_canonical_bytes(&read::fixed_tree::<MAILBOX_FRONTIER_BYTES, _, _>(
            &id,
            &mut ctx.values,
            &mut ctx.cancelled,
        )?)
        .map_err(ValueTreeError::Invalid)?;
    let expected = Expected {
        mask,
        slots: number(control(roots, owner, F::SlotCount, ctx)?)?,
        slot_next: number(control(roots, owner, F::SlotNext, ctx)?)?,
        cargo: number(control(roots, owner, F::CargoCount, ctx)?)?,
        cargo_next: number(control(roots, owner, F::CargoNext, ctx)?)?,
        head: frontier.head,
        tail: frontier.tail,
    };
    if expected.slots > expected.slot_next || expected.cargo > expected.cargo_next {
        return Err(invalid());
    }
    Ok(expected)
}
fn row<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    roots: &RootSetV1,
    owner: [u8; 32],
    field: AgentFieldV1,
    ordinal: u64,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<(), ValueTreeError> {
    use AgentFieldV1 as F;
    let (anchor, columns) = if group(field) == Group::Slot {
        (
            F::SlotId,
            [F::SlotId, F::SlotType, F::SlotInstalled, F::SlotLocked],
        )
    } else {
        (
            F::CargoId,
            [F::CargoId, F::CargoKind, F::CargoQuantity, F::CargoUnitSize],
        )
    };
    if field == anchor {
        for f in columns {
            lookup(roots, owner, f, ordinal, ctx)?;
        }
    } else {
        lookup(roots, owner, anchor, ordinal, ctx)?;
    }
    Ok(())
}
/// One continuous authenticated six-root scan. The observer exposes original
/// dependencies only; even an observer returning Ok cannot certify history.
pub fn qualify_agent_subtree_structure_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    F: FnMut(AgentMailboxReferenceV1<'_>) -> Result<(), ValueTreeError>,
>(
    roots: &RootSetV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut AgentSubtreeScanWorkV1,
    observe: &mut F,
) -> Result<ReservedValue<AgentSubtreeStructureQualificationV1>, ValueTreeError> {
    roots.validate().map_err(|_| invalid())?;
    ctx.check()?;
    if !roots.state.iter().any(|root| root == ctx.root) {
        return Err(invalid());
    }
    let retained = ctx.values.memory.reserve(512, ctx.values.class)?;
    let bytes = work
        .max_agents
        .checked_mul(512)
        .and_then(|n| n.checked_add(128 << 10))
        .ok_or(ValueTreeError::ResourceLimited("agent metadata admission"))?;
    let _metadata = ctx.values.memory.reserve(bytes, ctx.values.class)?;
    let mut table = BTreeMap::new();
    let cancel = RefCell::new(&mut ctx.cancelled);
    for root in &roots.state {
        let mut local = WorldFieldReadContextV1 {
            root,
            state: ctx.state,
            values: oasis7_formal_protocol::incremental_value::ValueReadContext {
                source: ctx.values.source,
                memory: ctx.values.memory,
                class: ctx.values.class,
                io: &mut *ctx.values.io,
            },
            key_work: &mut *ctx.key_work,
            semantic_bytes: ctx.semantic_bytes,
            record_bytes: ctx.record_bytes,
            take_state_error: &mut ctx.take_state_error,
            cancelled: || (cancel.borrow_mut())(),
        };
        let result = StateScanContext {
            source: ctx.state,
            memory: local.values.memory,
            class: local.values.class,
            io: &mut *scan,
        }
        .visit_entries(
            root,
            |entry| {
                verify_entry(
                    roots,
                    EntryRef {
                        namespace: entry.namespace,
                        key: &entry.key,
                        value: entry.value,
                    },
                    &mut local,
                    work,
                    &mut table,
                    observe,
                )
            },
            || (cancel.borrow_mut())(),
        );
        let original = (local.take_state_error)();
        if (cancel.borrow_mut())() {
            return Err(ValueTreeError::Cancelled);
        }
        if let Some(error) = original {
            return Err(error);
        }
        match result {
            Ok(_) => {}
            Err(StateScanError::Visitor(error)) => return Err(error),
            Err(StateScanError::Memory(error)) => return Err(error.into()),
            Err(StateScanError::Cancelled) => return Err(ValueTreeError::Cancelled),
            Err(StateScanError::ResourceLimited) => {
                return Err(ValueTreeError::ResourceLimited("agent structural scan"));
            }
            Err(_) => return Err(invalid()),
        }
    }
    let mut references = 0u64;
    for s in table.values() {
        if (cancel.borrow_mut())() {
            return Err(ValueTreeError::Cancelled);
        }
        let mail = validate_stats(s)?;
        references = references.checked_add(mail).ok_or_else(invalid)?;
    }
    if (cancel.borrow_mut())() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(retained.retain(AgentSubtreeStructureQualificationV1 {
        roots: roots.digest().map_err(|_| invalid())?,
        agents: table.len() as u64,
        mailbox_references: references,
    }))
}

fn validate_stats(s: &Stats) -> Result<u64, ValueTreeError> {
    let e = s.expected.ok_or_else(invalid)?;
    if s.mask != e.mask
        || !s.frontier
        || s.slots != e.slots
        || s.cargo != e.cargo
        || s.mail != e.tail - e.head
        || s.slot_max.is_some_and(|n| n >= e.slot_next)
        || s.cargo_max.is_some_and(|n| n >= e.cargo_next)
        || s.mail_min.is_some_and(|n| n < e.head)
        || s.mail_max.is_some_and(|n| n >= e.tail)
    {
        return Err(invalid());
    }
    Ok(s.mail)
}
#[path = "incremental_agent_subtree_lifecycle_verify.rs"]
mod lifecycle;
pub(super) use lifecycle::verify_lifecycle_actor;
/// Owner-private inheritance after a receiver proves every changed relation.
pub(super) fn inherit(
    parent: &AgentSubtreeStructureQualificationV1,
    roots: &RootSetV1,
    memory: &oasis7_proto::memory_budget::MemoryBudget,
    class: oasis7_proto::memory_budget::MemoryClass,
    agent_delta: i128,
    reference_delta: i128,
) -> Result<ReservedValue<AgentSubtreeStructureQualificationV1>, ValueTreeError> {
    let agents = u64::try_from(i128::from(parent.agents) + agent_delta).map_err(|_| invalid())?;
    let mailbox_references = u64::try_from(i128::from(parent.mailbox_references) + reference_delta)
        .map_err(|_| invalid())?;
    let charge = memory.reserve(512, class)?;
    Ok(charge.retain(AgentSubtreeStructureQualificationV1 {
        roots: roots.digest().map_err(|_| invalid())?,
        agents,
        mailbox_references,
    }))
}
