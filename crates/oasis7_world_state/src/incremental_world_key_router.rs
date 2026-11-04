//! Complete key grammar, separate from typed values, physical availability,
//! original-history closure and original commit authority.
use crate::{
    incremental_agent_body::{AGENT_BODY_KEY_DOMAIN_V1, AgentBodyKeyV1},
    incremental_agent_fields::{AGENT_FIELD_KEY_DOMAIN_V1, AgentFieldKeyV1},
    incremental_cognition_fields::CognitionControlFieldV1,
    incremental_cognition_json::CognitionJsonFieldV1,
    incremental_event_fields::{EVENT_SEQUENCE_KEY_V1, MAILBOX_KEY_DOMAIN_V1, MailboxKeyV1},
    incremental_module_fields::ModuleStateKeyV1,
    incremental_module_instance_fields::ModuleInstanceKeyV1,
    incremental_module_instance_index::*,
    incremental_projection::{FieldDestination, FieldOwner, WORLD_STATE_FIELDS},
    incremental_runtime_registry::*,
    incremental_scheduler_fields::{MODULE_WAKE_INDEX_KEY_V1, ModuleWakeKeyV1},
    incremental_world_field_key::WorldFieldKeyV1,
};
use oasis7_formal_protocol::{
    incremental_execution::RootSetV1,
    incremental_state::{
        MerkleNodeReader, StateNamespaceV1 as Ns, StateScanBudget, StateScanContext, StateScanError,
    },
    incremental_value::ValueTreeError,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
#[cfg(test)]
#[path = "incremental_world_key_router_tests.rs"]
mod tests;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(usize)]
pub enum WorldKeyOwnerV1 {
    WorldStateField = 0,
    RuntimeField,
    CognitionField,
    AgentField,
    AgentBody,
    Mailbox,
    ModuleState,
    ModuleInstance,
    ModuleInstanceIndex,
    ModuleWake,
    Allocator,
}
pub const WORLD_KEY_OWNER_COUNT_V1: usize = 11;
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("closed World key routing")
}
fn require(
    namespace: Ns,
    expected: Ns,
    owner: WorldKeyOwnerV1,
) -> Result<WorldKeyOwnerV1, ValueTreeError> {
    if namespace != expected {
        Err(invalid())
    } else {
        Ok(owner)
    }
}
/// Recognizes one canonical key and its actual namespace. This does not verify
/// the semantic key or value and is never a complete typed owner proof.
pub fn route_world_key_v1(namespace: Ns, key: &[u8]) -> Result<WorldKeyOwnerV1, ValueTreeError> {
    use WorldKeyOwnerV1 as O;
    if key.starts_with(b"oasis7.iws.world-field-key.v1\0") {
        let parsed = WorldFieldKeyV1::from_bytes(key).map_err(ValueTreeError::Invalid)?;
        if parsed.namespace != namespace {
            return Err(invalid());
        }
        let (owner, field) = match parsed.owner {
            FieldOwner::WorldState => {
                // Opaque module state has its sole dedicated key family.
                if parsed.name == "module_states" {
                    return Err(invalid());
                }
                (
                    O::WorldStateField,
                    WORLD_STATE_FIELDS
                        .iter()
                        .copied()
                        .find(|f| f.name == parsed.name),
                )
            }
            FieldOwner::Runtime => (
                O::RuntimeField,
                RUNTIME_TYPED_GENESIS_FIELDS_V1
                    .iter()
                    .copied()
                    .find(|f| f.name == parsed.name),
            ),
            FieldOwner::Cognition => (
                O::CognitionField,
                CognitionControlFieldV1::ALL
                    .into_iter()
                    .map(|f| f.descriptor())
                    .chain(
                        CognitionJsonFieldV1::ALL
                            .into_iter()
                            .map(|f| f.descriptor()),
                    )
                    .find(|f| f.name == parsed.name),
            ),
        };
        let field = field.ok_or_else(invalid)?;
        if field.owner != parsed.owner
            || field.shape != parsed.shape
            || field.destination != FieldDestination::State(namespace)
            || WorldFieldKeyV1::new(field, parsed.kind, parsed.semantic_key)
                .map_err(ValueTreeError::Invalid)?
                .as_bytes()
                != key
        {
            return Err(invalid());
        }
        return Ok(owner);
    }
    if key.starts_with(AGENT_FIELD_KEY_DOMAIN_V1) {
        let (_, field, _) = AgentFieldKeyV1::from_bytes(key).map_err(ValueTreeError::Invalid)?;
        return require(namespace, field.namespace(), O::AgentField);
    }
    if key.starts_with(AGENT_BODY_KEY_DOMAIN_V1) {
        AgentBodyKeyV1::from_bytes(key).map_err(ValueTreeError::Invalid)?;
        return require(namespace, Ns::Business, O::AgentBody);
    }
    if key.starts_with(MAILBOX_KEY_DOMAIN_V1) {
        MailboxKeyV1::from_bytes(key).map_err(ValueTreeError::Invalid)?;
        return require(namespace, Ns::Scheduler, O::Mailbox);
    }
    if ModuleStateKeyV1::is_state_key(key) {
        ModuleStateKeyV1::state_key_from_bytes(key).map_err(ValueTreeError::Invalid)?;
        return require(namespace, Ns::Business, O::ModuleState);
    }
    if ModuleInstanceKeyV1::is_instance_key(key) {
        ModuleInstanceKeyV1::from_bytes(key).map_err(ValueTreeError::Invalid)?;
        return require(namespace, Ns::Business, O::ModuleInstance);
    }
    if key == MODULE_INSTANCE_INDEX_KEY_V1 {
        return require(namespace, Ns::Business, O::ModuleInstanceIndex);
    }
    if is_module_instance_count_key(key) {
        validate_module_instance_count_key(key).map_err(ValueTreeError::Invalid)?;
        return require(namespace, Ns::Business, O::ModuleInstanceIndex);
    }
    if key == MODULE_WAKE_INDEX_KEY_V1 {
        return require(namespace, Ns::Scheduler, O::ModuleWake);
    }
    // The existing parser authenticates framing even though it does not expose
    // a prefix predicate. No other key is accepted by this fallback.
    if ModuleWakeKeyV1::instance_from_bytes(key).is_ok() {
        return require(namespace, Ns::Scheduler, O::ModuleWake);
    }
    if key == EVENT_SEQUENCE_KEY_V1 {
        return require(namespace, Ns::Allocator, O::Allocator);
    }
    if key.starts_with(RUNTIME_SEQUENCE_KEY_DOMAIN_V1) {
        parse_runtime_sequence_key_v1(key).map_err(ValueTreeError::Invalid)?;
        return require(namespace, Ns::Allocator, O::Allocator);
    }
    Err(invalid())
}
pub struct WorldKeyRoutingQualificationV1 {
    roots: [u8; 32],
    class: MemoryClass,
    counts: [u64; WORLD_KEY_OWNER_COUNT_V1],
    memory: MemoryBudget,
}
impl WorldKeyRoutingQualificationV1 {
    pub fn count(&self, owner: WorldKeyOwnerV1) -> u64 {
        self.counts[owner as usize]
    }
    pub fn class(&self) -> MemoryClass {
        self.class
    }
    pub fn validate_binding(&self, roots: &RootSetV1) -> Result<(), ValueTreeError> {
        let _phase = self.memory.reserve(32 << 10, self.class)?;
        roots.validate().map_err(|_| invalid())?;
        if roots.digest().map_err(|_| invalid())? != self.roots {
            return Err(invalid());
        }
        let count = self
            .counts
            .iter()
            .try_fold(0u64, |n, c| n.checked_add(*c))
            .ok_or_else(invalid)?;
        let leaves = roots
            .state
            .iter()
            .try_fold(0u64, |n, r| n.checked_add(r.leaf_count))
            .ok_or_else(invalid)?;
        if count != leaves {
            return Err(invalid());
        }
        Ok(())
    }
}
/// Full bootstrap/recovery routing scan with a single continuous scan budget.
/// No leaf is collected, and unknown keys stop the scan without a capability.
pub fn qualify_world_key_routing_v1<M: MerkleNodeReader>(
    roots: &RootSetV1,
    state: &M,
    memory: &MemoryBudget,
    class: MemoryClass,
    scan: &mut StateScanBudget,
    mut take_state_error: impl FnMut() -> Option<ValueTreeError>,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<WorldKeyRoutingQualificationV1>, ValueTreeError> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    if let Some(error) = take_state_error() {
        return Err(error);
    }
    roots.validate().map_err(|_| invalid())?;
    if state
        .allocation_budget()
        .is_none_or(|b| !b.same_pool(memory))
        || state.allocation_class() != Some(class)
    {
        return Err(invalid());
    }
    let _phase = memory.reserve(32 << 10, class)?;
    let charge = memory.reserve(512, class)?;
    let mut counts = [0u64; WORLD_KEY_OWNER_COUNT_V1];
    for root in &roots.state {
        let result = StateScanContext {
            source: state,
            memory,
            class,
            io: &mut *scan,
        }
        .visit_entries(
            root,
            |entry| {
                let owner = route_world_key_v1(entry.namespace, &entry.key)?;
                counts[owner as usize] =
                    counts[owner as usize].checked_add(1).ok_or_else(invalid)?;
                Ok::<_, ValueTreeError>(())
            },
            &mut cancelled,
        );
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        if let Some(error) = take_state_error() {
            return Err(error);
        }
        match result {
            Ok(_) => {}
            Err(StateScanError::Visitor(e)) => return Err(e),
            Err(StateScanError::Memory(e)) => return Err(e.into()),
            Err(StateScanError::Cancelled) => return Err(ValueTreeError::Cancelled),
            Err(StateScanError::ResourceLimited) => {
                return Err(ValueTreeError::ResourceLimited("World key routing scan"));
            }
            Err(_) => return Err(invalid()),
        }
    }
    let result = WorldKeyRoutingQualificationV1 {
        roots: roots.digest().map_err(|_| invalid())?,
        class,
        counts,
        memory: memory.clone(),
    };
    result.validate_binding(roots)?;
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    if let Some(error) = take_state_error() {
        return Err(error);
    }
    Ok(charge.retain(result))
}

/// Promotes only the resource class of an exact same-pool key-routing proof.
/// Typed values, physical leases and World readiness remain separate.
pub fn readmit_world_key_routing_ordinary_v1(
    source: &ReservedValue<WorldKeyRoutingQualificationV1>,
    roots: &RootSetV1,
    memory: &MemoryBudget,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<WorldKeyRoutingQualificationV1>, ValueTreeError> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    if !source.uses_budget(memory)
        || !source.memory.same_pool(memory)
        || source.class != MemoryClass::Recovery
        || source.reserved_bytes() < 512
    {
        return Err(invalid());
    }
    source.validate_binding(roots)?;
    let charge = memory.reserve(512, MemoryClass::Ordinary)?;
    let value = WorldKeyRoutingQualificationV1 {
        roots: source.roots,
        class: MemoryClass::Ordinary,
        counts: source.counts,
        memory: memory.clone(),
    };
    value.validate_binding(roots)?;
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(charge.retain(value))
}
