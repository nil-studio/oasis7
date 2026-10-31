//! Typed borrowed storage for the three WorldState module-market maps.
//!
//! The codecs preserve each source value exactly, including Vec order and
//! optional/empty visual labels. This is a field owner primitive; it does not
//! authorize a sale, installation, or complete World activation.
use crate::{
    incremental_field_member::{
        MemberPayloadRefV1, MemberRecordError, MemberRecordWriteContext, write_member_record_v1,
    },
    incremental_field_presence::{
        PresenceRecordLimitsV1, PresenceRecordWriteContext, write_field_metadata_v1,
    },
    incremental_projection::{
        FieldDescriptor, FieldDestination, FieldOwner, FieldShape, WORLD_STATE_FIELDS,
    },
    incremental_world_field_key::WorldFieldRecordKindV1,
    state::WorldState,
};
use oasis7_formal_protocol::{
    incremental_execution::ExecutionWriteV1,
    incremental_state::StateNamespaceV1,
    incremental_value::{ValueNodeWriter, ValueTreeError},
};
use oasis7_proto::memory_budget::ReservedValue;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModuleMarketplaceFieldV1 {
    VisualEntities,
    ArtifactListings,
    ArtifactBids,
}
impl ModuleMarketplaceFieldV1 {
    pub const ALL: [Self; 3] = [
        Self::VisualEntities,
        Self::ArtifactListings,
        Self::ArtifactBids,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::VisualEntities => "module_visual_entities",
            Self::ArtifactListings => "module_artifact_listings",
            Self::ArtifactBids => "module_artifact_bids",
        }
    }

    pub fn descriptor(self) -> Result<FieldDescriptor, MemberRecordError> {
        WORLD_STATE_FIELDS
            .iter()
            .copied()
            .find(|field| {
                field.owner == FieldOwner::WorldState
                    && field.name == self.name()
                    && field.shape == FieldShape::Map
                    && field.destination == FieldDestination::State(StateNamespaceV1::Business)
            })
            .ok_or(MemberRecordError::Invalid(
                "module marketplace registration",
            ))
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|field| field.name() == name)
    }

    pub(crate) fn codec(self) -> crate::incremental_field_member::MemberPayloadCodecV1 {
        use crate::incremental_field_member::MemberPayloadCodecV1 as C;
        match self {
            Self::VisualEntities => C::ModuleVisualEntity,
            Self::ArtifactListings => C::ModuleArtifactListing,
            Self::ArtifactBids => C::ModuleArtifactBids,
        }
    }
}

fn record_start<W, C: FnMut() -> bool>(
    context: &mut MemberRecordWriteContext<'_, W, C>,
    remaining: &mut u64,
) -> Result<(), MemberRecordError> {
    if (context.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    *remaining = remaining
        .checked_sub(1)
        .ok_or(ValueTreeError::ResourceLimited(
            "module marketplace records",
        ))?;
    Ok(())
}

fn emit<W, C, S>(
    key: crate::incremental_world_field_key::WorldFieldKeyV1,
    root: [u8; 32],
    context: &mut MemberRecordWriteContext<'_, W, C>,
    sink: &mut S,
) -> Result<(), MemberRecordError>
where
    C: FnMut() -> bool,
    S: FnMut(ReservedValue<ExecutionWriteV1>) -> Result<(), ValueTreeError>,
{
    crate::incremental_field_write::emit(key, root, context, sink)
}

/// Produce all three maps with the registered Business descriptors. Every
/// value is a closed typed member; no World/value clone or arbitrary Serde
/// encoding is involved. The caller must discard all writes on any error.
pub fn prepare_module_marketplace_genesis_v1<
    W: ValueNodeWriter,
    C: FnMut() -> bool,
    S: FnMut(ReservedValue<ExecutionWriteV1>) -> Result<(), ValueTreeError>,
>(
    source: &WorldState,
    context: &mut MemberRecordWriteContext<'_, W, C>,
    remaining: &mut u64,
    sink: &mut S,
) -> Result<u64, MemberRecordError> {
    let mut written = 0u64;
    for field in ModuleMarketplaceFieldV1::ALL {
        let descriptor = field.descriptor()?;
        record_start(context, remaining)?;
        let metadata = write_field_metadata_v1(
            descriptor,
            &mut PresenceRecordWriteContext {
                sink: &mut *context.sink,
                memory: context.memory,
                io: &mut *context.io,
                key_work: &mut *context.key_work,
                limits: PresenceRecordLimitsV1 {
                    semantic_bytes: context.limits.semantic_bytes,
                    record_bytes: context.limits.record_bytes,
                },
                cancelled: &mut context.cancelled,
            },
        )?;
        emit(metadata.field_key, metadata.value_root, context, sink)?;

        macro_rules! member {
            ($key:expr, $payload:expr) => {{
                record_start(context, remaining)?;
                let record = write_member_record_v1(
                    descriptor,
                    WorldFieldRecordKindV1::Entry,
                    $key,
                    $payload,
                    context,
                )?;
                emit(record.field_key, record.value_root, context, sink)?;
                written = written
                    .checked_add(1)
                    .ok_or(MemberRecordError::Invalid("module marketplace entry count"))?;
            }};
        }

        match field {
            ModuleMarketplaceFieldV1::VisualEntities => {
                for (key, value) in &source.module_visual_entities {
                    member!(key.as_str(), MemberPayloadRefV1::ModuleVisualEntity(value));
                }
            }
            ModuleMarketplaceFieldV1::ArtifactListings => {
                for (key, value) in &source.module_artifact_listings {
                    member!(
                        key.as_str(),
                        MemberPayloadRefV1::ModuleArtifactListing(value)
                    );
                }
            }
            ModuleMarketplaceFieldV1::ArtifactBids => {
                for (key, value) in &source.module_artifact_bids {
                    member!(key.as_str(), MemberPayloadRefV1::ModuleArtifactBids(value));
                }
            }
        }
    }
    if (context.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(written)
}

#[cfg(test)]
#[path = "incremental_module_marketplace_tests.rs"]
mod tests;
