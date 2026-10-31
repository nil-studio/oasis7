//! Fail-closed WorldState owner coverage for profile-3 endpoint verification.
//!
//! Generic state/value verification proves the six trees and all referenced
//! value bytes. This layer binds structural keys to the frozen WorldState
//! registry and verifies the currently implemented scalar-u64 owner family.
//! Other WorldState, Runtime, and Cognition owners remain explicit gaps; this
//! report is never a receipt or an activation token.

use crate::{
    incremental_field_member::{
        MemberPayloadCodecV1, MemberReadProfileV1, MemberRecordLimitsV1, WorldFieldKeyV1,
        WorldFieldRecordKindV1, read_member_record_profile_v1,
    },
    incremental_field_presence::{PresenceRecordLimitsV1, read_field_presence_v1},
    incremental_projection::{
        FieldDescriptor, FieldDestination, FieldOwner, FieldShape, WORLD_STATE_FIELDS,
    },
    incremental_semantic_key::{SEMANTIC_KEY_DOMAIN, SemanticKeyRefV1, read_semantic_key_v1},
    incremental_world_u64::WorldU64FieldV1,
};
use oasis7_formal_protocol::{
    incremental_state::{
        MerkleNodeReader, StateNamespaceV1, StateRootV1, StateScanBudget, StateScanContext,
        StateScanError,
    },
    incremental_value::{ValueNodeReader, ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
use std::{cell::RefCell, io};

const OWNER_SCAN_METADATA_BYTES: usize = 4096;
const U64_SEMANTIC_BYTES: u64 = (SEMANTIC_KEY_DOMAIN.len() + 1) as u64;

#[derive(Debug, Clone, Copy, Default)]
struct SeenField {
    metadata: u8,
    entries: u8,
}

/// Bounded diagnostics for a state whose generic tree/value closure passed but
/// whose typed owner inventory is not yet complete. Keeping the fields charged
/// prevents the diagnostic from escaping the endpoint's shared pool.
#[derive(Debug)]
pub struct UnsupportedWorldStateOwnersV1 {
    body_id: [u8; 32],
    unsupported_fields: ReservedValue<Vec<&'static str>>,
    runtime_registry_missing: bool,
    cognition_registry_missing: bool,
    checked_u64_fields: u8,
}

impl UnsupportedWorldStateOwnersV1 {
    pub fn body_id(&self) -> [u8; 32] {
        self.body_id
    }

    pub fn unsupported_fields(&self) -> &[&'static str] {
        self.unsupported_fields.as_slice()
    }

    pub fn runtime_registry_missing(&self) -> bool {
        self.runtime_registry_missing
    }

    pub fn cognition_registry_missing(&self) -> bool {
        self.cognition_registry_missing
    }

    pub fn checked_u64_fields(&self) -> u8 {
        self.checked_u64_fields
    }
}

#[derive(Debug)]
pub enum WorldStateOwnerError {
    Scan(String),
    Value(ValueTreeError),
    Memory(oasis7_proto::memory_budget::MemoryLimitExceeded),
    ResourceLimited(&'static str),
    Invalid(&'static str),
}
impl std::fmt::Display for WorldStateOwnerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Scan(error) => write!(f, "WorldState owner scan: {error}"),
            Self::Value(error) => error.fmt(f),
            Self::Memory(error) => error.fmt(f),
            Self::ResourceLimited(error) => write!(f, "WorldState owner resource limited: {error}"),
            Self::Invalid(error) => write!(f, "WorldState owner closure: {error}"),
        }
    }
}
impl std::error::Error for WorldStateOwnerError {}

fn registered_descriptor(
    owner: FieldOwner,
    namespace: StateNamespaceV1,
    shape: FieldShape,
    name: &str,
) -> Result<Option<(usize, FieldDescriptor)>, WorldStateOwnerError> {
    let mut found = None;
    for (index, field) in WORLD_STATE_FIELDS.iter().copied().enumerate() {
        if field.owner == owner
            && field.name == name
            && field.shape == shape
            && field.destination == FieldDestination::State(namespace)
        {
            if found.is_some() {
                return Err(WorldStateOwnerError::Invalid(
                    "ambiguous WorldState descriptor registration",
                ));
            }
            found = Some((index, field));
        }
    }
    Ok(found)
}

fn u64_field(field: FieldDescriptor) -> Option<WorldU64FieldV1> {
    WorldU64FieldV1::ALL
        .into_iter()
        .find(|candidate| candidate.descriptor().ok() == Some(field))
}

fn read_u64_entry<R: ValueNodeReader>(
    scalar: WorldU64FieldV1,
    field: FieldDescriptor,
    key: WorldFieldKeyV1,
    value: &[u8; 32],
    values: &mut ValueReadContext<'_, R>,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<(), WorldStateOwnerError> {
    if scalar.descriptor().ok() != Some(field) {
        return Err(WorldStateOwnerError::Invalid("u64 owner registration"));
    }
    let semantic_len = usize::try_from(U64_SEMANTIC_BYTES)
        .map_err(|_| WorldStateOwnerError::Invalid("u64 semantic key length"))?;
    let mut semantic = [0u8; 64];
    let mut semantic_output = &mut semantic[..semantic_len];
    let payload_len = 8usize;
    let mut payload = [0u8; 8];
    let mut payload_output = &mut payload[..];
    let record_limit = crate::incremental_field_member::FIELD_MEMBER_HEADER_BYTES
        .checked_add(key.as_bytes().len())
        .and_then(|size| size.checked_add(semantic_len))
        .and_then(|size| size.checked_add(payload_len))
        .and_then(|size| u64::try_from(size).ok())
        .ok_or(WorldStateOwnerError::Invalid("u64 record limit overflow"))?;
    let profile = MemberReadProfileV1 {
        codec: MemberPayloadCodecV1::U64,
        semantic_bytes: U64_SEMANTIC_BYTES,
        limits: MemberRecordLimitsV1 {
            semantic_bytes: U64_SEMANTIC_BYTES,
            payload_bytes: 8,
            record_bytes: record_limit,
        },
    };
    let info = read_member_record_profile_v1(
        value,
        &key,
        values,
        profile,
        &mut semantic_output,
        &mut payload_output,
        || cancelled(),
    )
    .map_err(WorldStateOwnerError::Value)?;
    if info.codec != MemberPayloadCodecV1::U64
        || info.semantic_bytes != U64_SEMANTIC_BYTES
        || info.payload_bytes != 8
        || !semantic_output.is_empty()
        || !payload_output.is_empty()
        || read_semantic_key_v1(&semantic[..semantic_len]) != Ok(SemanticKeyRefV1::Unit)
    {
        return Err(WorldStateOwnerError::Invalid("u64 owner member profile"));
    }
    let _value = u64::from_be_bytes(payload);
    Ok(())
}

/// Scan all six namespaces and bind every encountered structural key to the
/// frozen WorldState descriptor table. `verify_state_values` must already have
/// established every value-tree closure. This function adds owner typing for
/// the seven implemented scalar-u64 descriptors, checks their required
/// metadata/entry records, and returns explicit unsupported owner families.
/// Callers must reject the incomplete report before issuing a profile-3 receipt.
pub fn verify_world_state_owner_coverage<S: MerkleNodeReader, V: ValueNodeReader>(
    roots: &[StateRootV1; 6],
    body_id: [u8; 32],
    state: &S,
    values: &mut ValueReadContext<'_, V>,
    state_io: &mut StateScanBudget,
    cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<UnsupportedWorldStateOwnersV1>, WorldStateOwnerError> {
    if state.allocation_class() != Some(values.class)
        || !state
            .allocation_budget()
            .is_some_and(|pool| pool.same_pool(values.memory))
        || values.class != MemoryClass::Ordinary
    {
        return Err(WorldStateOwnerError::Invalid("owner source pool/class"));
    }
    let memory = values.memory;
    let seen_bytes = WORLD_STATE_FIELDS
        .len()
        .checked_mul(std::mem::size_of::<SeenField>())
        .ok_or(WorldStateOwnerError::Invalid("owner schema size overflow"))?;
    let schema_charge = memory
        .reserve(OWNER_SCAN_METADATA_BYTES.max(seen_bytes), values.class)
        .map_err(WorldStateOwnerError::Memory)?;
    let cancelled = RefCell::new(cancelled);
    let mut seen = Vec::with_capacity(WORLD_STATE_FIELDS.len());
    seen.resize(WORLD_STATE_FIELDS.len(), SeenField::default());
    if seen.capacity() * std::mem::size_of::<SeenField>() > schema_charge.bytes() {
        return Err(WorldStateOwnerError::Invalid("owner schema allocation"));
    }
    let mut unknown_owner = false;
    for root in roots {
        let mut scan = StateScanContext {
            source: state,
            memory,
            class: values.class,
            io: state_io,
        };
        let result = scan.visit_entries(
            root,
            |entry| {
                if (cancelled.borrow_mut())() {
                    return Err(WorldStateOwnerError::Value(ValueTreeError::Cancelled));
                }
                if entry.namespace != root.namespace {
                    return Err(WorldStateOwnerError::Invalid(
                        "entry namespace/root mismatch",
                    ));
                }
                let parsed =
                    crate::incremental_world_field_key::WorldFieldKeyV1::from_bytes(&entry.key)
                        .map_err(WorldStateOwnerError::Invalid)?;
                let Some((index, field)) = registered_descriptor(
                    parsed.owner,
                    parsed.namespace,
                    parsed.shape,
                    parsed.name,
                )?
                else {
                    unknown_owner = true;
                    return Ok(());
                };
                let key = WorldFieldKeyV1::new(field, parsed.kind, parsed.semantic_key)
                    .map_err(WorldStateOwnerError::Invalid)?;
                if key.as_bytes() != entry.key.as_slice() {
                    return Err(WorldStateOwnerError::Invalid(
                        "registered field key mismatch",
                    ));
                }
                match parsed.kind {
                    WorldFieldRecordKindV1::Metadata => {
                        seen[index].metadata = seen[index].metadata.saturating_add(1);
                        read_field_presence_v1(
                            &entry.value,
                            &key,
                            values,
                            PresenceRecordLimitsV1 {
                                semantic_bytes: 0,
                                record_bytes: 512,
                            },
                            &mut io::sink(),
                            || (cancelled.borrow_mut())(),
                        )
                        .map_err(WorldStateOwnerError::Value)?;
                    }
                    WorldFieldRecordKindV1::Entry if let Some(scalar) = u64_field(field) => {
                        seen[index].entries = seen[index].entries.saturating_add(1);
                        let mut cancel_now = || (cancelled.borrow_mut())();
                        read_u64_entry(scalar, field, key, &entry.value, values, &mut cancel_now)?;
                    }
                    _ if u64_field(field).is_some() => {
                        return Err(WorldStateOwnerError::Invalid(
                            "unexpected scalar-u64 record kind",
                        ));
                    }
                    _ => {
                        // Other owner records have bounded generic value
                        // closure, but remain untyped and are listed as
                        // unsupported below.
                    }
                }
                Ok(())
            },
            || (cancelled.borrow_mut())(),
        );
        match result {
            Ok(_) => {}
            Err(StateScanError::Visitor(error)) => return Err(error),
            Err(StateScanError::Cancelled) => {
                return Err(WorldStateOwnerError::Value(ValueTreeError::Cancelled));
            }
            Err(StateScanError::Memory(error)) => return Err(WorldStateOwnerError::Memory(error)),
            Err(StateScanError::Tree(error)) => {
                return Err(WorldStateOwnerError::Scan(error));
            }
            Err(StateScanError::Invalid(error)) => {
                return Err(WorldStateOwnerError::Invalid(error));
            }
            Err(StateScanError::ResourceLimited) => {
                return Err(WorldStateOwnerError::ResourceLimited("scan allowance"));
            }
        }
    }
    if (cancelled.borrow_mut())() {
        return Err(WorldStateOwnerError::Value(ValueTreeError::Cancelled));
    }

    let unsupported_capacity =
        WORLD_STATE_FIELDS
            .len()
            .checked_add(3)
            .ok_or(WorldStateOwnerError::Invalid(
                "owner diagnostic size overflow",
            ))?;
    let fields_bytes = unsupported_capacity
        .checked_mul(std::mem::size_of::<&'static str>())
        .ok_or(WorldStateOwnerError::Invalid(
            "owner diagnostic size overflow",
        ))?;
    let fields_charge = memory
        .reserve(fields_bytes, values.class)
        .map_err(WorldStateOwnerError::Memory)?;
    let mut unsupported = Vec::with_capacity(unsupported_capacity);
    if unsupported.capacity() * std::mem::size_of::<&'static str>() > fields_charge.bytes() {
        return Err(WorldStateOwnerError::Invalid("owner diagnostic allocation"));
    }

    let mut checked_u64_fields = 0u8;
    for (index, field) in WORLD_STATE_FIELDS.iter().copied().enumerate() {
        let FieldDestination::State(_) = field.destination else {
            continue;
        };
        if let Some(scalar) = u64_field(field) {
            if seen[index].metadata != 1 || seen[index].entries != 1 {
                return Err(WorldStateOwnerError::Invalid(
                    "required scalar-u64 metadata/entry missing or duplicate",
                ));
            }
            if scalar.descriptor().ok() != Some(field) {
                return Err(WorldStateOwnerError::Invalid("scalar-u64 descriptor"));
            }
            checked_u64_fields = checked_u64_fields.saturating_add(1);
        } else {
            if seen[index].metadata != 1 {
                return Err(WorldStateOwnerError::Invalid(
                    "required field metadata missing or duplicate",
                ));
            }
            unsupported.push(field.name);
        }
    }
    if unknown_owner {
        unsupported.push("unregistered-owner-key");
    }
    // These registries are not yet joined to the WorldState descriptor table;
    // keep them explicit even if a particular root happens to contain no such
    // field. Absence is not proof of a complete Runtime/Cognition schema.
    unsupported.push("Runtime-owner-registry");
    unsupported.push("Cognition-owner-registry");
    if unsupported.len() > unsupported.capacity() {
        return Err(WorldStateOwnerError::Invalid("owner diagnostic capacity"));
    }
    let report_charge = memory
        .reserve(
            std::mem::size_of::<UnsupportedWorldStateOwnersV1>(),
            values.class,
        )
        .map_err(WorldStateOwnerError::Memory)?;
    let report = UnsupportedWorldStateOwnersV1 {
        body_id,
        unsupported_fields: fields_charge.retain(unsupported),
        runtime_registry_missing: true,
        cognition_registry_missing: true,
        checked_u64_fields,
    };
    drop(schema_charge);
    Ok(report_charge.retain(report))
}

#[cfg(test)]
#[path = "incremental_owner_closure_tests.rs"]
mod tests;
