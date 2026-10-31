use super::{OwnerKeyKind, OwnerSemanticKeyProbe, WorldStateOwnerError};
use crate::{
    incremental_field_member::{
        MemberPayloadCodecV1, MemberRecordLimitsV1, WorldFieldKeyV1, read_member_record_v1,
    },
    incremental_identity_amounts::read_identity_amounts_record_v1,
    incremental_ledger_value::Utf8Output,
    incremental_projection::{FieldDescriptor, FieldDestination, FieldOwner, FieldShape},
    incremental_world_balances::WorldBalanceFieldV1,
    incremental_world_field_key::{WorldFieldKeyRefV1, WorldFieldRecordKindV1},
};
use oasis7_formal_protocol::{
    incremental_state::{StateEntryV1, StateNamespaceV1, StateRootV1},
    incremental_value::{ValueNodeReader, ValueReadContext},
};
use std::io;

pub(super) fn is_installed_target(field: FieldDescriptor) -> bool {
    field.name == "installed_module_targets"
}

pub(super) fn is_world_balance(field: FieldDescriptor) -> bool {
    WorldBalanceFieldV1::ALL
        .into_iter()
        .any(|candidate| candidate.name() == field.name)
}

pub(super) const INSTALLED_TARGET_FIELD_COUNT: u8 = 1;
pub(super) const WORLD_BALANCE_FIELD_COUNT: u8 = WorldBalanceFieldV1::ALL.len() as u8;

pub(super) fn valid_installed_target_descriptor(field: FieldDescriptor) -> bool {
    valid_target_descriptor(field)
}

pub(super) fn valid_world_balance_descriptor(field: FieldDescriptor) -> bool {
    valid_balance_descriptor(field)
}

fn valid_target_descriptor(field: FieldDescriptor) -> bool {
    field.owner == FieldOwner::WorldState
        && field.name == "installed_module_targets"
        && field.shape == FieldShape::Map
        && field.destination == FieldDestination::State(StateNamespaceV1::Artifacts)
}

fn valid_balance_descriptor(field: FieldDescriptor) -> bool {
    WorldBalanceFieldV1::ALL.into_iter().any(|candidate| {
        candidate.name() == field.name
            && field.owner == FieldOwner::WorldState
            && field.shape == FieldShape::Map
            && field.destination == FieldDestination::State(StateNamespaceV1::Business)
    })
}

pub(super) fn validate_installed_target_member<R: ValueNodeReader>(
    field: FieldDescriptor,
    entry: &StateEntryV1,
    parsed: WorldFieldKeyRefV1<'_>,
    values: &mut ValueReadContext<'_, R>,
    max_value_bytes: u64,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<(), WorldStateOwnerError> {
    if !valid_target_descriptor(field)
        || parsed.kind != WorldFieldRecordKindV1::Entry
        || parsed.owner != FieldOwner::WorldState
        || parsed.namespace != StateNamespaceV1::Artifacts
        || parsed.shape != FieldShape::Map
        || entry.namespace != StateNamespaceV1::Artifacts
    {
        return Err(WorldStateOwnerError::Invalid(
            "installed target descriptor/member binding",
        ));
    }
    let key = WorldFieldKeyV1::new(field, parsed.kind, parsed.semantic_key)
        .map_err(WorldStateOwnerError::Invalid)?;
    if key.as_bytes() != entry.key.as_slice() {
        return Err(WorldStateOwnerError::Invalid(
            "installed target field key mismatch",
        ));
    }
    let mut key_probe = OwnerSemanticKeyProbe::new(OwnerKeyKind::Text, max_value_bytes);
    let mut discarded = io::sink();
    let mut payload = Utf8Output::new(&mut discarded);
    let result = read_member_record_v1(
        &entry.value,
        &key,
        values,
        MemberRecordLimitsV1 {
            semantic_bytes: max_value_bytes,
            payload_bytes: max_value_bytes,
            record_bytes: max_value_bytes,
        },
        &mut key_probe,
        &mut payload,
        cancelled,
    );
    if key_probe.invalid {
        return Err(WorldStateOwnerError::Invalid(
            "installed target semantic key profile",
        ));
    }
    if payload.invalid() {
        return Err(WorldStateOwnerError::Invalid(
            "installed target text payload UTF-8",
        ));
    }
    let info = result.map_err(WorldStateOwnerError::Value)?;
    if !key_probe.finish() {
        return Err(WorldStateOwnerError::Invalid(
            "installed target semantic key framing",
        ));
    }
    let valid = match info.codec {
        MemberPayloadCodecV1::Unit => info.payload_bytes == 0,
        MemberPayloadCodecV1::Text => payload.complete(),
        _ => false,
    };
    if !valid {
        return Err(WorldStateOwnerError::Invalid(
            "installed target payload profile",
        ));
    }
    Ok(())
}

pub(super) fn validate_world_balance_member<R>(
    root: &StateRootV1,
    field: FieldDescriptor,
    entry: &StateEntryV1,
    parsed: WorldFieldKeyRefV1<'_>,
    values: &mut ValueReadContext<'_, R>,
    max_value_bytes: u64,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<(), WorldStateOwnerError>
where
    R: ValueNodeReader,
{
    if !valid_balance_descriptor(field)
        || parsed.kind != WorldFieldRecordKindV1::Entry
        || parsed.owner != FieldOwner::WorldState
        || parsed.namespace != StateNamespaceV1::Business
        || parsed.shape != FieldShape::Map
        || entry.namespace != StateNamespaceV1::Business
        || root.namespace != StateNamespaceV1::Business
    {
        return Err(WorldStateOwnerError::Invalid(
            "world balance descriptor/member binding",
        ));
    }
    let key = WorldFieldKeyV1::new(field, parsed.kind, parsed.semantic_key)
        .map_err(WorldStateOwnerError::Invalid)?;
    if key.as_bytes() != entry.key.as_slice() {
        return Err(WorldStateOwnerError::Invalid(
            "world balance field key mismatch",
        ));
    }
    let mut key_probe = OwnerSemanticKeyProbe::new(OwnerKeyKind::Text, max_value_bytes);
    let mut discarded = io::sink();
    let decoded = read_identity_amounts_record_v1(
        &entry.value,
        &key,
        values,
        MemberRecordLimitsV1 {
            semantic_bytes: max_value_bytes,
            payload_bytes: max_value_bytes,
            record_bytes: max_value_bytes,
        },
        &mut key_probe,
        &mut discarded,
        cancelled,
    );
    if key_probe.invalid {
        return Err(WorldStateOwnerError::Invalid(
            "world balance semantic key profile",
        ));
    }
    let (info, _, _) = decoded.map_err(WorldStateOwnerError::Value)?;
    if !key_probe.finish() {
        return Err(WorldStateOwnerError::Invalid(
            "world balance semantic key profile",
        ));
    }
    if info.codec != MemberPayloadCodecV1::IdentityU64Triple {
        return Err(WorldStateOwnerError::Invalid("world balance payload codec"));
    }
    Ok(())
}
