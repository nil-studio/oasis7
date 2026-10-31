use super::{OwnerKeyKind, OwnerSemanticKeyProbe, WorldStateOwnerError};
use crate::{
    incremental_field_member::{MemberRecordLimitsV1, WorldFieldKeyV1, read_member_record_v1},
    incremental_module_marketplace::ModuleMarketplaceFieldV1,
    incremental_projection::{FieldDescriptor, FieldDestination, FieldOwner, FieldShape},
    incremental_world_field_key::{WorldFieldKeyRefV1, WorldFieldRecordKindV1},
};
use oasis7_formal_protocol::{
    incremental_state::{StateEntryV1, StateNamespaceV1},
    incremental_value::{ValueNodeReader, ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::MemoryClass;
use std::io;

pub(super) const FIELD_COUNT: u8 = ModuleMarketplaceFieldV1::ALL.len() as u8;

pub(super) fn field_from_descriptor(field: FieldDescriptor) -> Option<ModuleMarketplaceFieldV1> {
    ModuleMarketplaceFieldV1::from_name(field.name)
}

pub(super) fn valid_descriptor(field: FieldDescriptor) -> bool {
    field.owner == FieldOwner::WorldState
        && field.shape == FieldShape::Map
        && field.destination == FieldDestination::State(StateNamespaceV1::Business)
        && ModuleMarketplaceFieldV1::from_name(field.name)
            .is_some_and(|candidate| candidate.descriptor().ok() == Some(field))
}

pub(super) fn validate_member<R: ValueNodeReader>(
    field: FieldDescriptor,
    entry: &StateEntryV1,
    parsed: WorldFieldKeyRefV1<'_>,
    values: &mut ValueReadContext<'_, R>,
    max_value_bytes: u64,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<(), WorldStateOwnerError> {
    let Some(owner) = field_from_descriptor(field) else {
        return Err(WorldStateOwnerError::Invalid(
            "module marketplace field descriptor",
        ));
    };
    if !valid_descriptor(field)
        || parsed.owner != FieldOwner::WorldState
        || parsed.namespace != StateNamespaceV1::Business
        || parsed.shape != FieldShape::Map
        || parsed.kind != WorldFieldRecordKindV1::Entry
        || entry.namespace != StateNamespaceV1::Business
        || values.class != MemoryClass::Ordinary
    {
        return Err(WorldStateOwnerError::Invalid(
            "module marketplace descriptor/member binding",
        ));
    }
    let key = WorldFieldKeyV1::new(field, parsed.kind, parsed.semantic_key)
        .map_err(WorldStateOwnerError::Invalid)?;
    if key.as_bytes() != entry.key.as_slice() {
        return Err(WorldStateOwnerError::Invalid(
            "module marketplace field key mismatch",
        ));
    }
    let codec = owner.codec();
    let limits = MemberRecordLimitsV1 {
        semantic_bytes: max_value_bytes,
        payload_bytes: max_value_bytes,
        record_bytes: max_value_bytes,
    };
    let mut key_probe = OwnerSemanticKeyProbe::new(OwnerKeyKind::Text, max_value_bytes);
    let mut discard = io::sink();
    let info = read_member_record_v1(
        &entry.value,
        &key,
        values,
        limits,
        &mut key_probe,
        &mut discard,
        &mut *cancelled,
    );
    if key_probe.invalid {
        return Err(WorldStateOwnerError::Invalid(
            "module marketplace semantic key profile",
        ));
    }
    let info = info.map_err(WorldStateOwnerError::Value)?;
    if !key_probe.finish() {
        return Err(WorldStateOwnerError::Invalid(
            "module marketplace semantic key framing",
        ));
    }
    if info.codec != codec {
        return Err(WorldStateOwnerError::Invalid(
            "module marketplace payload codec",
        ));
    }
    validate_payload(
        owner,
        info.payload_bytes,
        &entry.value,
        &key,
        values,
        max_value_bytes,
        cancelled,
    )
}

fn validate_payload<R: ValueNodeReader>(
    owner: ModuleMarketplaceFieldV1,
    payload_bytes: u64,
    value: &[u8; 32],
    key: &WorldFieldKeyV1,
    values: &mut ValueReadContext<'_, R>,
    max_value_bytes: u64,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<(), WorldStateOwnerError> {
    if payload_bytes > max_value_bytes {
        return Err(WorldStateOwnerError::ResourceLimited(
            "module marketplace payload",
        ));
    }
    let payload_len = usize::try_from(payload_bytes)
        .map_err(|_| WorldStateOwnerError::Invalid("module marketplace payload length"))?;
    let charge = values
        .memory
        .reserve(payload_len, values.class)
        .map_err(WorldStateOwnerError::Memory)?;
    let mut payload = Vec::new();
    payload
        .try_reserve_exact(payload_len)
        .map_err(|_| WorldStateOwnerError::ResourceLimited("module marketplace payload"))?;
    if payload.capacity() > charge.bytes() {
        return Err(WorldStateOwnerError::Invalid(
            "module marketplace payload allocation",
        ));
    }
    payload.resize(payload_len, 0);
    let mut key_probe = OwnerSemanticKeyProbe::new(OwnerKeyKind::Text, max_value_bytes);
    let info = read_member_record_v1(
        value,
        key,
        values,
        MemberRecordLimitsV1 {
            semantic_bytes: max_value_bytes,
            payload_bytes: max_value_bytes,
            record_bytes: max_value_bytes,
        },
        &mut key_probe,
        &mut payload.as_mut_slice(),
        &mut *cancelled,
    )
    .map_err(WorldStateOwnerError::Value)?;
    if key_probe.invalid || !key_probe.finish() {
        return Err(WorldStateOwnerError::Invalid(
            "module marketplace semantic key framing",
        ));
    }
    if info.codec != owner.codec()
        || info.payload_bytes != payload_bytes
        || !validate_typed_payload(owner, &payload)
    {
        return Err(WorldStateOwnerError::Invalid(
            "module marketplace typed payload",
        ));
    }
    if cancelled() {
        return Err(WorldStateOwnerError::Value(ValueTreeError::Cancelled));
    }
    Ok(())
}

fn validate_typed_payload(owner: ModuleMarketplaceFieldV1, bytes: &[u8]) -> bool {
    match owner {
        ModuleMarketplaceFieldV1::VisualEntities => parse_visual_entity(bytes),
        ModuleMarketplaceFieldV1::ArtifactListings => parse_artifact_listing(bytes),
        ModuleMarketplaceFieldV1::ArtifactBids => parse_artifact_bids(bytes),
    }
}

fn take<'a>(bytes: &mut &'a [u8], length: usize) -> Option<&'a [u8]> {
    let (prefix, suffix) = bytes.split_at_checked(length)?;
    *bytes = suffix;
    Some(prefix)
}
fn u8_value(bytes: &mut &[u8]) -> Option<u8> {
    Some(*take(bytes, 1)?.first()?)
}
fn u64_value(bytes: &mut &[u8]) -> Option<u64> {
    Some(u64::from_be_bytes(take(bytes, 8)?.try_into().ok()?))
}
fn i64_value(bytes: &mut &[u8]) -> Option<i64> {
    Some(i64::from_be_bytes(take(bytes, 8)?.try_into().ok()?))
}
fn text(bytes: &mut &[u8]) -> Option<()> {
    let length = usize::try_from(u64_value(bytes)?).ok()?;
    std::str::from_utf8(take(bytes, length)?).ok()?;
    Some(())
}
fn resource_tag(bytes: &mut &[u8]) -> Option<()> {
    match u8_value(bytes)? {
        0 | 1 => Some(()),
        _ => None,
    }
}
fn starts_with_domain(bytes: &mut &[u8], domain: &[u8]) -> bool {
    take(bytes, domain.len()).is_some_and(|found| found == domain)
}
fn no_trailing(bytes: &[u8]) -> bool {
    bytes.is_empty()
}

fn parse_visual_entity(mut bytes: &[u8]) -> bool {
    if !starts_with_domain(
        &mut bytes,
        crate::incremental_field_member::MODULE_VISUAL_ENTITY_DOMAIN,
    ) {
        return false;
    }
    if text(&mut bytes).is_none() || text(&mut bytes).is_none() || text(&mut bytes).is_none() {
        return false;
    }
    match u8_value(&mut bytes) {
        Some(0) => {}
        Some(1) if text(&mut bytes).is_some() => {}
        _ => return false,
    }
    let anchor_valid = match u8_value(&mut bytes) {
        Some(0 | 1) => text(&mut bytes).is_some(),
        Some(2) => {
            i64_value(&mut bytes).is_some()
                && i64_value(&mut bytes).is_some()
                && i64_value(&mut bytes).is_some()
        }
        _ => false,
    };
    anchor_valid && no_trailing(bytes)
}
fn parse_artifact_listing(mut bytes: &[u8]) -> bool {
    starts_with_domain(
        &mut bytes,
        crate::incremental_field_member::MODULE_ARTIFACT_LISTING_DOMAIN,
    ) && u64_value(&mut bytes).is_some()
        && text(&mut bytes).is_some()
        && resource_tag(&mut bytes).is_some()
        && i64_value(&mut bytes).is_some()
        && u64_value(&mut bytes).is_some()
        && no_trailing(bytes)
}
fn parse_artifact_bids(mut bytes: &[u8]) -> bool {
    if !starts_with_domain(
        &mut bytes,
        crate::incremental_field_member::MODULE_ARTIFACT_BIDS_DOMAIN,
    ) {
        return false;
    }
    let Some(count) = u64_value(&mut bytes) else {
        return false;
    };
    if count > (bytes.len() as u64) / 33 {
        return false;
    }
    for _ in 0..count {
        if u64_value(&mut bytes).is_none()
            || text(&mut bytes).is_none()
            || resource_tag(&mut bytes).is_none()
            || i64_value(&mut bytes).is_none()
            || u64_value(&mut bytes).is_none()
        {
            return false;
        }
    }
    no_trailing(bytes)
}
