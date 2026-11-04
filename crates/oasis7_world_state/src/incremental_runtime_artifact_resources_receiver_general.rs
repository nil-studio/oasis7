//! Independent typed closure of arbitrary changes in the three original fields.
//! This does not execute a producer or grant business authorization.
use super::*;
use crate::incremental_field_overlay::WorldFieldOverlayErrorV1;
use crate::incremental_field_read::WorldFieldReadContextV1;
use oasis7_formal_protocol::incremental_execution::{TransitionRecordV1, TransitionVerifier};
use oasis7_formal_protocol::incremental_value::ValueReadContext;
#[path = "incremental_runtime_artifact_resources_receiver_record.rs"]
mod changed;
/// Accept the actual authenticated transition independently of any owner write
/// request. The parent must be a complete Ordinary family qualification.
pub fn accept_runtime_artifact_resources_transition_v1<M, P, R, E, C>(
    parent: &ReservedValue<RuntimeArtifactResourcesQualificationV1>,
    verifier: &TransitionVerifier<'_, P>,
    record: &TransitionRecordV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scans: &mut [StateScanBudget; MAX_DEPTH],
    max_payload_bytes: u64,
    work: &mut super::super::RuntimeArtifactResourcesWorkV1,
) -> Result<ReservedValue<RuntimeArtifactResourcesQualificationV1>, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    ctx.check()?;
    if ctx.values.class != MemoryClass::Ordinary
        || parent.class != MemoryClass::Ordinary
        || !parent.uses_budget(ctx.values.memory)
        || parent.reserved_bytes() < std::mem::size_of::<RuntimeArtifactResourcesQualificationV1>()
        || !verifier.memory.same_pool(ctx.values.memory)
        || verifier.class != MemoryClass::Ordinary
        || ctx.state.allocation_class() != Some(MemoryClass::Ordinary)
        || !ctx
            .state
            .allocation_budget()
            .is_some_and(|p| p.same_pool(ctx.values.memory))
    {
        return Err(invalid().into());
    }
    let _phase = ctx
        .values
        .memory
        .reserve(128 << 10, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    parent.validate_binding(
        &record.before.digest().map_err(|_| invalid())?,
        MemoryClass::Ordinary,
    )?;
    if parent.counts[0] != 1 || parent.counts[2] != 1 || parent.counts[1] > 1 {
        return Err(invalid().into());
    }
    let replay = verifier.verify(record);
    let original = (ctx.take_state_error)();
    ctx.check()?;
    if let Some(original) = original {
        return Err(original.into());
    }
    let verified = replay?;
    let record = &verified.record;
    if ctx.root != record.after.state_root(StateNamespaceV1::Artifacts) {
        return Err(invalid().into());
    }
    let mut counts = parent.counts;
    {
        let cancel = RefCell::new(&mut ctx.cancelled);
        let mut family = FamilyVerifier {
            state: ctx.state,
            values: &mut ctx.values,
            key_work: &mut *ctx.key_work,
            semantic_bytes: ctx.semantic_bytes,
            record_bytes: ctx.record_bytes,
            max_payload_bytes,
            take_state_error: &mut ctx.take_state_error,
            cancelled: &cancel,
            work,
        };
        for write in &record.writes {
            family.spend_cell()?;
            if !write.mutation.key.starts_with(WORLD_FIELD_DOMAIN) {
                continue;
            }
            let binding = WorldFieldKeyV1::from_bytes(&write.mutation.key)
                .map_err(ValueTreeError::Invalid)?;
            let Some(index) = field_index(binding.name) else {
                continue;
            };
            let field = Field::ALL[index];
            let key = WorldFieldKeyV1::new(field.descriptor(), binding.kind, binding.semantic_key)
                .map_err(ValueTreeError::Invalid)?;
            if write.namespace != StateNamespaceV1::Artifacts
                || binding.owner != FieldOwner::Runtime
                || binding.namespace != StateNamespaceV1::Artifacts
                || binding.shape != FieldShape::Value
                || write.mutation.key != key.as_bytes()
            {
                return Err(invalid().into());
            }
            let old = write.mutation.before.ok_or_else(invalid)?;
            let after = write.mutation.after.ok_or_else(invalid)?;
            match binding.kind {
                WorldFieldRecordKindV1::Metadata => {
                    let expected = WorldFieldKeyV1::new(
                        field.descriptor(),
                        WorldFieldRecordKindV1::Metadata,
                        None,
                    )
                    .map_err(ValueTreeError::Invalid)?;
                    if key != expected || old != after {
                        return Err(invalid().into());
                    }
                }
                WorldFieldRecordKindV1::Entry => {
                    let expected =
                        family.key(field.descriptor(), WorldFieldRecordKindV1::Entry, &())?;
                    if key != expected.0 {
                        return Err(invalid().into());
                    }
                    if old == after {
                        continue;
                    }
                    let old = family.read_outer_root(field, &old, &key)?;
                    let after = family.read_outer_root(field, &after, &key)?;
                    if old.is_some() != (counts[index] == 1) {
                        return Err(invalid().into());
                    }
                    match (old, after) {
                        (None, None) => {}
                        (Some(_), None) if field == Field::LatestChainResourceDelta => {
                            counts[index] = 0
                        }
                        (None, Some(after)) if field == Field::LatestChainResourceDelta => {
                            family.verify_record(field_record(field), &after, &mut scans[1..])?;
                            counts[index] = 1;
                        }
                        (Some(before), Some(after)) => {
                            family.advance_record(
                                field_record(field),
                                &before,
                                &after,
                                &mut scans[1..],
                            )?;
                            counts[index] = 1;
                        }
                        _ => return Err(invalid().into()),
                    }
                }
                _ => return Err(invalid().into()),
            }
        }
    }
    ctx.check()?;
    let roots = record.after.digest().map_err(|_| invalid())?;
    let result = ctx
        .values
        .memory
        .reserve(
            std::mem::size_of::<RuntimeArtifactResourcesQualificationV1>(),
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?
        .retain(RuntimeArtifactResourcesQualificationV1 {
            roots,
            counts,
            class: MemoryClass::Ordinary,
        });
    ctx.check()?;
    Ok(result)
}
