const COMPLETE_COGNITION_ROOT_SCRATCH_BYTES_V3: usize = 128 << 10;
use super::*;
use crate::{
    incremental_cognition_absence::{
        CognitionCurrentAbsenceQualificationV1, copy_cognition_absence_ordinary_v1,
        qualify_cognition_current_absence_v1,
    },
    incremental_field_overlay::WorldFieldOverlayErrorV1,
    incremental_json_records::JsonChangeWorkV1,
    incremental_verified_world_transition::VerifiedWorldTransitionV1,
};
use oasis7_proto::memory_budget::MemoryBudget;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CognitionCurrentPresenceV3 {
    Enabled,
    Disabled,
}
enum CompleteProof {
    Enabled(ReservedValue<CognitionCurrentStateQualificationV1>),
    Disabled(ReservedValue<CognitionCurrentAbsenceQualificationV1>),
}
pub struct CompleteCognitionCurrentQualificationV3 {
    proof: CompleteProof,
    memory: MemoryBudget,
}
pub struct CognitionCurrentCompleteWorkV3 {
    pub scan: StateScanBudget,
    pub json: JsonChangeWorkV1,
    pub outer_cells_remaining: u64,
}
impl CompleteCognitionCurrentQualificationV3 {
    pub fn allocation_class(&self) -> MemoryClass {
        match &self.proof {
            CompleteProof::Enabled(v) => v.class(),
            CompleteProof::Disabled(v) => v.class(),
        }
    }
    pub fn presence(&self) -> CognitionCurrentPresenceV3 {
        match &self.proof {
            CompleteProof::Enabled(_) => CognitionCurrentPresenceV3::Enabled,
            CompleteProof::Disabled(_) => CognitionCurrentPresenceV3::Disabled,
        }
    }
    pub fn validate_binding(&self, roots: &RootSetV1) -> Result<(), ValueTreeError> {
        let _scratch = self.memory.reserve(
            COMPLETE_COGNITION_ROOT_SCRATCH_BYTES_V3,
            self.allocation_class(),
        )?;
        let digest = roots.digest().map_err(|_| invalid())?;
        match &self.proof {
            CompleteProof::Enabled(v) => v.validate_binding(&digest),
            CompleteProof::Disabled(v) => v.validate_binding(&digest),
        }
    }
    pub fn history_source(
        &self,
    ) -> crate::incremental_cognition_history::CognitionHistoryCurrentSourceV1<'_> {
        use crate::incremental_cognition_history::CognitionHistoryCurrentSourceV1;
        match &self.proof {
            CompleteProof::Enabled(v) => CognitionHistoryCurrentSourceV1::Enabled(v),
            CompleteProof::Disabled(v) => CognitionHistoryCurrentSourceV1::Disabled(v),
        }
    }
}
fn wrap(
    proof: CompleteProof,
    memory: &MemoryBudget,
) -> Result<ReservedValue<CompleteCognitionCurrentQualificationV3>, ValueTreeError> {
    let class = match &proof {
        CompleteProof::Enabled(v) => v.class(),
        CompleteProof::Disabled(v) => v.class(),
    };
    let charge = memory.reserve(
        4096usize.max(std::mem::size_of::<CompleteCognitionCurrentQualificationV3>()),
        class,
    )?;
    Ok(charge.retain(CompleteCognitionCurrentQualificationV3 {
        proof,
        memory: memory.clone(),
    }))
}
pub fn qualify_complete_cognition_current_v3<M, R, E, C>(
    roots: &RootSetV1,
    presence: CognitionCurrentPresenceV3,
    context: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut CognitionCurrentCompleteWorkV3,
) -> Result<ReservedValue<CompleteCognitionCurrentQualificationV3>, ValueTreeError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    context.check()?;
    let proof = match presence {
        CognitionCurrentPresenceV3::Enabled => {
            CompleteProof::Enabled(qualify_cognition_current_state_v1(
                roots,
                context,
                &mut work.scan,
                &mut work.json.json,
                &mut work.outer_cells_remaining,
            )?)
        }
        CognitionCurrentPresenceV3::Disabled => {
            CompleteProof::Disabled(qualify_cognition_current_absence_v1(
                roots,
                context,
                &mut work.scan,
                &mut work.outer_cells_remaining,
            )?)
        }
    };
    let result = wrap(proof, context.values.memory)?;
    context.check()?;
    Ok(result)
}
pub(crate) fn accept_complete_cognition_current_verified_v3<M, R, E, C>(
    parent: &ReservedValue<CompleteCognitionCurrentQualificationV3>,
    witness: &VerifiedWorldTransitionV1,
    context: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    after: CognitionCurrentPresenceV3,
    work: &mut CognitionCurrentCompleteWorkV3,
) -> Result<ReservedValue<CompleteCognitionCurrentQualificationV3>, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    context.check()?;
    witness.check_pool_class(context.values.memory, context.values.class)?;
    if !parent.uses_budget(context.values.memory)
        || parent.reserved_bytes() < 4096
        || parent.allocation_class() != MemoryClass::Ordinary
        || context.values.class != MemoryClass::Ordinary
        || !witness
            .record()
            .after
            .state
            .iter()
            .any(|r| r == context.root)
    {
        return Err(invalid().into());
    }
    parent.validate_binding(&witness.record().before)?;
    let memory = context.values.memory;
    let proof = match (&parent.proof, after) {
        (CompleteProof::Disabled(old), CognitionCurrentPresenceV3::Disabled) => {
            for write in &witness.record().writes {
                work.json.paths_remaining = work
                    .json
                    .paths_remaining
                    .checked_sub(1)
                    .ok_or(ValueTreeError::ResourceLimited("disabled Cognition writes"))?;
                context.check()?;
                if write
                    .mutation
                    .key
                    .starts_with(b"oasis7.iws.world-field-key.v1\0")
                {
                    let key = crate::incremental_world_field_key::WorldFieldKeyV1::from_bytes(
                        &write.mutation.key,
                    )
                    .map_err(ValueTreeError::Invalid)?;
                    if key.owner == crate::incremental_projection::FieldOwner::Cognition {
                        return Err(invalid().into());
                    }
                }
            }
            CompleteProof::Disabled(
                crate::incremental_cognition_absence::advance_cognition_absence_no_changes_v3(
                    old,
                    witness,
                    memory,
                    &mut context.cancelled,
                )?,
            )
        }
        (mode, _) => {
            let empty;
            let enabled = match mode {
                CompleteProof::Enabled(value) => value,
                CompleteProof::Disabled(_) => {
                    let charge = memory
                        .reserve(32 << 10, MemoryClass::Ordinary)
                        .map_err(ValueTreeError::Memory)?;
                    empty = charge.retain(CognitionCurrentStateQualificationV1 {
                        roots: *witness.before_digest(),
                        class: MemoryClass::Ordinary,
                        controls: 0,
                        entries: std::array::from_fn(|_| None),
                    });
                    &empty
                }
            };
            let before_root = witness
                .record()
                .before
                .state
                .iter()
                .find(|r| r.namespace == context.root.namespace)
                .ok_or_else(invalid)?;
            let outcome = {
                let mut before = WorldFieldReadContextV1 {
                    root: before_root,
                    state: context.state,
                    values: ValueReadContext {
                        source: context.values.source,
                        memory,
                        class: context.values.class,
                        io: &mut *context.values.io,
                    },
                    key_work: &mut *context.key_work,
                    semantic_bytes: context.semantic_bytes,
                    record_bytes: context.record_bytes,
                    take_state_error: &mut context.take_state_error,
                    cancelled: &mut context.cancelled,
                };
                group_receiver::accept_cognition_current_presence_verified_v3(
                    enabled,
                    witness,
                    &mut before,
                    &mut work.json,
                    after == CognitionCurrentPresenceV3::Enabled,
                )?
            };
            match outcome {
                group_receiver::CognitionCurrentPresenceOutcomeV3::Enabled(value) => CompleteProof::Enabled(value),
                group_receiver::CognitionCurrentPresenceOutcomeV3::Empty(value) => CompleteProof::Disabled(
                    crate::incremental_cognition_absence::copy_checked_empty_cognition_group_v3(&value, memory, &mut context.cancelled)?),
            }
        }
    };
    let result = wrap(proof, memory)?;
    context.check()?;
    Ok(result)
}
pub fn readmit_complete_cognition_current_ordinary_v3(
    source: &ReservedValue<CompleteCognitionCurrentQualificationV3>,
    roots: &RootSetV1,
    memory: &MemoryBudget,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<CompleteCognitionCurrentQualificationV3>, ValueTreeError> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    if !source.uses_budget(memory)
        || source.reserved_bytes() < 4096
        || source.allocation_class() != MemoryClass::Recovery
    {
        return Err(invalid());
    }
    source.validate_binding(roots)?;
    let _scratch = memory.reserve(
        COMPLETE_COGNITION_ROOT_SCRATCH_BYTES_V3,
        MemoryClass::Ordinary,
    )?;
    let digest = roots.digest().map_err(|_| invalid())?;
    let proof = match &source.proof {
        CompleteProof::Enabled(value) => CompleteProof::Enabled(
            copy_cognition_current_ordinary_v1(value, roots, memory, &mut cancelled)?,
        ),
        CompleteProof::Disabled(value) => CompleteProof::Disabled(
            copy_cognition_absence_ordinary_v1(value, &digest, memory, &mut cancelled)?,
        ),
    };
    let result = wrap(proof, memory)?;
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(result)
}
