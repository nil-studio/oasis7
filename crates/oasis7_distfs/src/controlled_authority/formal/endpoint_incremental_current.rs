use oasis7_formal_protocol::{
    incremental_material::{CheckedCommitCore, OriginalObjectStreamReader},
    incremental_value::ValueTreeError,
};
use oasis7_proto::memory_budget::ReservedValue;
// Complete Current consumer: mailbox proof comes from authenticated Event facts.
use oasis7_formal_protocol::{
    incremental_execution::TransitionVerifier, incremental_state::MerkleNodeReader,
};
use oasis7_world_state::{
    incremental_cognition_current_complete_v3::CognitionCurrentPresenceV3,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_world_current_complete::{
        WorldCurrentStateErrorV1, WorldCurrentStateQualificationV1,
        accept_world_current_state_transition_v1, qualify_world_current_state_v1,
    },
    incremental_world_current_limits::WorldCurrentStateLimitsV1,
};
/// Provisional structural evidence only. History/source conjunction remains
/// mandatory before this can enter the endpoint's receipt path.
pub(super) struct CheckedCurrent {
    pub(super) qualification: ReservedValue<WorldCurrentStateQualificationV1>,
}
#[allow(clippy::too_many_arguments)]
pub(super) fn check_current<M, P, R, E, C, O, B>(
    core: &ReservedValue<CheckedCommitCore>,
    parent: Option<&ReservedValue<WorldCurrentStateQualificationV1>>,
    verifier: Option<&TransitionVerifier<'_, P>>,
    presence: CognitionCurrentPresenceV3,
    scope: &oasis7_world_state::incremental_world_raw_complete::MailboxProvenanceScopeV1<'_>,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    limits: &WorldCurrentStateLimitsV1,
    scratch_pages: usize,
    mailbox: &mut B,
    objects: &mut O,
) -> Result<CheckedCurrent, WorldCurrentStateErrorV1<O::Error>>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: oasis7_formal_protocol::incremental_value::ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    O: OriginalObjectStreamReader,
    B: oasis7_world_state::incremental_world_raw_complete::MailboxWorldEventOriginalReaderV1<O>,
{
    if (ctx.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    if !core.uses_budget(ctx.values.memory) {
        return Err(ValueTreeError::Invalid("endpoint Current core pool").into());
    }
    let mut work = limits.work()?;
    let scratch = super::scratch::Scratch::new(ctx.values.memory, ctx.values.class, scratch_pages)
        .map_err(|_| ValueTreeError::ResourceLimited("endpoint counter scratch admission"))?;
    let qualification = match (core.transition(), parent) {
        (Some(record), Some(parent)) => accept_world_current_state_transition_v1(
            parent,
            verifier.ok_or(ValueTreeError::Invalid(
                "endpoint missing transition verifier",
            ))?,
            record,
            presence,
            scope,
            ctx,
            &mut work,
            &scratch,
            objects,
            mailbox,
        )?,
        (None, None) => qualify_world_current_state_v1(
            core.roots(),
            presence,
            scope,
            ctx,
            &mut work,
            &scratch,
            objects,
            mailbox,
        )?,
        _ => {
            return Err(ValueTreeError::Invalid("endpoint exact Current parent/transition").into());
        }
    };
    qualification
        .validate_binding(core.roots())
        .map_err(WorldCurrentStateErrorV1::Binding)?;
    Ok(CheckedCurrent { qualification })
}

/// Explicit production allowances. No default or implicit quota expansion.
#[derive(Debug, Clone, Copy)]
pub struct IwsCurrentLimitsV1 {
    pub current: WorldCurrentStateLimitsV1,
    pub scratch_pages: usize,
    pub cognition: CognitionCurrentPresenceV3,
    pub decode_bytes: u64,
    pub decode_values: u64,
    pub decode_depth: usize,
    pub decode_max_original_bytes: usize,
    pub decode_max_heap_bytes: usize,
    pub canonical_visits: u64,
    pub canonical_bytes: u64,
    pub canonical_map_keys: usize,
    pub canonical_key_bytes: usize,
}
impl IwsCurrentLimitsV1 {
    pub(super) fn candidate_mailbox(
        &self,
        memory: &oasis7_proto::memory_budget::MemoryBudget,
        class: oasis7_proto::memory_budget::MemoryClass,
    ) -> Result<super::mailbox::CandidateMailboxReader, ValueTreeError> {
        self.validate()?;
        super::mailbox::CandidateMailboxReader::new(
            oasis7_world_state::incremental_world_event_decode::WorldEventDecodeWorkV1 {
                structure: oasis7_proto::cbor_decode_preflight::CborPreflightWorkV1 {
                    bytes_remaining: self.decode_bytes,
                    values_remaining: self.decode_values,
                    max_depth: self.decode_depth,
                },
                canonical: oasis7_wasm_abi::CanonicalCborWorkV1 {
                    visits_remaining: self.canonical_visits,
                    bytes_remaining: self.canonical_bytes,
                    max_depth: self.decode_depth,
                    max_map_keys: self.canonical_map_keys,
                    max_key_bytes: self.canonical_key_bytes,
                },
                max_original_bytes: self.decode_max_original_bytes,
                max_heap_bytes: self.decode_max_heap_bytes,
            },
            self.current.mailbox_occurrences_remaining,
            self.current.records_remaining,
            memory,
            class,
        )
    }
    pub(super) fn validate(&self) -> Result<(), ValueTreeError> {
        self.current.validate()?;
        self.scratch_pages
            .checked_mul(256)
            .and_then(|n| n.checked_add(512))
            .ok_or(ValueTreeError::Invalid("endpoint scratch limit overflow"))?;
        Ok(())
    }
}
