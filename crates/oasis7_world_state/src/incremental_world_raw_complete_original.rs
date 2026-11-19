//! Mailbox IDs retain their original WorldEvent identity and Event provenance.
use crate::{
    WorldEventBody,
    incremental_domain_event_proof::*,
    incremental_pending_action_original_binding::{
        PendingActionOriginalErrorV1, PendingActionOriginalWorkV1,
    },
    incremental_world_event_decode::DecodedWorldEventV1,
};
use oasis7_formal_protocol::{
    controlled_authority::{
        formal::{VerifiedCandidateContextV1, VerifiedFormalCommitV1},
        replicated_protocol::ArtifactRole,
    },
    incremental_execution::RootSetV1,
    incremental_material::{
        CheckedCandidateEventFactOccurrenceV1, CheckedCommitCore, CheckedFactOriginals,
        VerifiedEventFactOccurrenceV1,
    },
    incremental_value::ValueTreeError,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use std::sync::Arc;
pub type MailboxWorldEventOriginalWorkV1 = PendingActionOriginalWorkV1;
pub type MailboxWorldEventOriginalErrorV1<E> = PendingActionOriginalErrorV1<E>;

/// These are genuine private protocol capabilities, never caller-reported IDs.
pub enum MailboxProvenanceScopeV1<'a> {
    Candidate {
        candidate: &'a VerifiedCandidateContextV1,
        core: &'a ReservedValue<CheckedCommitCore>,
        originals: &'a ReservedValue<CheckedFactOriginals>,
    },
    Accepted {
        head: &'a VerifiedFormalCommitV1,
        roots: &'a RootSetV1,
    },
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MailboxProvenanceAnchorV1 {
    record: [u8; 32],
    body: [u8; 32],
    roots: [u8; 32],
}
impl MailboxProvenanceAnchorV1 {
    pub fn record_id(&self) -> [u8; 32] {
        self.record
    }
    pub fn body_id(&self) -> [u8; 32] {
        self.body
    }
    pub fn roots_id(&self) -> [u8; 32] {
        self.roots
    }
}
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("mailbox WorldEvent provenance")
}
fn digest(text: &str) -> Result<[u8; 32], ValueTreeError> {
    let mut id = [0; 32];
    hex::decode_to_slice(text, &mut id).map_err(|_| invalid())?;
    Ok(id)
}
impl MailboxProvenanceScopeV1<'_> {
    pub(crate) fn anchor(
        &self,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<MailboxProvenanceAnchorV1, ValueTreeError> {
        let _phase = memory.reserve(128 << 10, class)?;
        match self {
            Self::Candidate {
                candidate,
                core,
                originals,
            } => {
                if class != MemoryClass::Ordinary
                    || !core.uses_budget(memory)
                    || core.reserved_bytes() < std::mem::size_of::<CheckedCommitCore>()
                    || !originals.uses_budget(memory)
                    || originals.reserved_bytes() < std::mem::size_of::<CheckedFactOriginals>()
                    || originals.body_id() != core.body().digest().map_err(|_| invalid())?
                    || originals.fact_set_id() != core.body().facts
                    || digest(&candidate.record().record_id().map_err(|_| invalid())?)?
                        != core.candidate_record_id()
                {
                    return Err(invalid());
                }
                Ok(MailboxProvenanceAnchorV1 {
                    record: core.candidate_record_id(),
                    body: core.body().digest().map_err(|_| invalid())?,
                    roots: core.roots().digest().map_err(|_| invalid())?,
                })
            }
            Self::Accepted { head, roots } => {
                roots.validate().map_err(|_| invalid())?;
                let record = head.record();
                if !oasis7_formal_protocol::controlled_authority::formal::incremental::is_iws_record(
                    record.schema_version,
                ) {
                    return Err(invalid());
                }
                let root_id = roots.digest().map_err(|_| invalid())?;
                let bound = record
                    .artifact_roots
                    .get(&ArtifactRole::IncrementalRootSet)
                    .ok_or_else(invalid)?;
                let body = record
                    .artifact_roots
                    .get(&ArtifactRole::IncrementalCommitBody)
                    .ok_or_else(invalid)?;
                if digest(bound)? != root_id
                    || record.after_state_root != hex::encode(root_id)
                    || record.world.world_id != roots.world_id
                    || record.world.genesis_digest != hex::encode(roots.genesis_config)
                    || roots.height.checked_add(1) != Some(record.position)
                {
                    return Err(invalid());
                }
                Ok(MailboxProvenanceAnchorV1 {
                    record: head.verified_record_id(),
                    body: digest(body)?,
                    roots: root_id,
                })
            }
        }
    }
}
/// The occurrence remains charged and owned alongside the exact decoded event.
pub enum MailboxWorldEventOccurrenceV1 {
    Candidate(ReservedValue<CheckedCandidateEventFactOccurrenceV1>),
    Accepted(ReservedValue<VerifiedEventFactOccurrenceV1>),
}
#[derive(Clone, Copy)]
enum ProvenanceKind {
    Candidate,
    Accepted,
    CandidateInherited,
}
pub struct CheckedMailboxWorldEventOriginalV1 {
    event: Arc<ReservedValue<DecodedWorldEventV1>>,
    occurrence: MailboxWorldEventOccurrenceV1,
    anchor: MailboxProvenanceAnchorV1,
    class: MemoryClass,
    kind: ProvenanceKind,
}
impl CheckedMailboxWorldEventOriginalV1 {
    pub fn event(&self) -> &crate::WorldEvent {
        self.event.event()
    }
    pub fn allocation_class(&self) -> MemoryClass {
        self.class
    }
    pub fn anchor(&self) -> MailboxProvenanceAnchorV1 {
        self.anchor
    }
    pub fn original_id(&self) -> [u8; 32] {
        match &self.occurrence {
            MailboxWorldEventOccurrenceV1::Candidate(p) => p.location().original,
            MailboxWorldEventOccurrenceV1::Accepted(p) => p.location().original,
        }
    }
    pub(crate) fn validate(
        carrier: &ReservedValue<Self>,
        id: &[u8; 32],
        anchor: MailboxProvenanceAnchorV1,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<(), ValueTreeError> {
        if !carrier.uses_budget(memory)
            || carrier.reserved_bytes() < std::mem::size_of::<Self>() + 512
            || carrier.anchor != anchor
            || carrier.class != class
            || carrier.original_id() != *id
            || !carrier.event.uses_budget(memory)
            || carrier.event.allocation_class() != class
        {
            return Err(invalid());
        }
        match (&carrier.kind, &carrier.occurrence) {
            (ProvenanceKind::Candidate, MailboxWorldEventOccurrenceV1::Candidate(_))
            | (
                ProvenanceKind::Accepted | ProvenanceKind::CandidateInherited,
                MailboxWorldEventOccurrenceV1::Accepted(_),
            ) => Ok(()),
            _ => Err(invalid()),
        }
    }
}
/// Reuses the unique WorldEvent Domain verifier, preserving id/time/caused_by.
/// The scope and the independently authenticated occurrence must match exactly.
/// An accepted parent occurrence can only enter a candidate through its actual
/// checked direct-parent Core; it never becomes a Formal accepted occurrence.
pub fn verify_mailbox_world_event_original_v1<E>(
    event: Arc<ReservedValue<DecodedWorldEventV1>>,
    occurrence: MailboxWorldEventOccurrenceV1,
    scope: &MailboxProvenanceScopeV1<'_>,
    memory: &MemoryBudget,
    class: MemoryClass,
    work: &mut MailboxWorldEventOriginalWorkV1,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<CheckedMailboxWorldEventOriginalV1>, MailboxWorldEventOriginalErrorV1<E>>
{
    if cancelled() {
        return Err(ValueTreeError::Cancelled.into());
    }
    let anchor = scope.anchor(memory, class)?;
    if !event.uses_budget(memory)
        || event.reserved_bytes() < std::mem::size_of::<DecodedWorldEventV1>()
        || event.allocation_class() != class
        || !matches!(event.event().body, WorldEventBody::Domain(_))
    {
        return Err(invalid().into());
    }
    let len = match &occurrence {
        MailboxWorldEventOccurrenceV1::Candidate(p) => p.original_bytes(),
        MailboxWorldEventOccurrenceV1::Accepted(p) => p.original_bytes(),
    };
    if len > work.max_original_bytes {
        return Err(ValueTreeError::ResourceLimited("mailbox WorldEvent bytes").into());
    }
    work.objects_remaining =
        work.objects_remaining
            .checked_sub(1)
            .ok_or(ValueTreeError::ResourceLimited(
                "mailbox WorldEvent occurrences",
            ))?;
    work.original_bytes_remaining =
        work.original_bytes_remaining
            .checked_sub(len)
            .ok_or(ValueTreeError::ResourceLimited(
                "mailbox WorldEvent verification bytes",
            ))?;
    let charge = memory
        .reserve(
            std::mem::size_of::<CheckedMailboxWorldEventOriginalV1>() + 512,
            class,
        )
        .map_err(ValueTreeError::Memory)?;
    let placeholder = oasis7_wasm_abi::CanonicalCborWorkV1 {
        visits_remaining: 0,
        bytes_remaining: 0,
        max_depth: work.canonical.max_depth,
        max_map_keys: work.canonical.max_map_keys,
        max_key_bytes: work.canonical.max_key_bytes,
    };
    let mut context = DomainEventProofContextV1::new(
        memory,
        class,
        std::mem::replace(&mut work.canonical, placeholder),
        &mut cancelled,
    );
    let result =
        (|| match (&occurrence, scope) {
            (
                MailboxWorldEventOccurrenceV1::Candidate(raw),
                MailboxProvenanceScopeV1::Candidate { .. },
            ) => {
                let _proof = check_candidate_domain_event_occurrence_v1(
                    &CandidateDomainEventProofRequestV1 {
                        event: event.event(),
                        occurrence: raw,
                        expected_candidate_record: anchor.record,
                        expected_roots: anchor.roots,
                    },
                    &mut context,
                )?;
                Ok(ProvenanceKind::Candidate)
            }
            (
                MailboxWorldEventOccurrenceV1::Accepted(raw),
                MailboxProvenanceScopeV1::Accepted { .. },
            ) => {
                if raw.allocation_class() != class {
                    return Err(oasis7_wasm_abi::CanonicalCborStreamError::Invalid(
                        "mailbox occurrence class",
                    ));
                }
                let _proof = verify_domain_event_occurrence_v1(
                    &DomainEventProofRequestV1 {
                        event: event.event(),
                        occurrence: raw,
                        expected_head_record: anchor.record,
                        expected_roots: anchor.roots,
                    },
                    &mut context,
                )?;
                Ok(ProvenanceKind::Accepted)
            }
            (
                MailboxWorldEventOccurrenceV1::Accepted(raw),
                MailboxProvenanceScopeV1::Candidate { core, .. },
            ) => {
                let parent_roots = core.parent_roots().ok_or(
                    oasis7_wasm_abi::CanonicalCborStreamError::Invalid("mailbox candidate parent"),
                )?;
                if raw.allocation_class() != MemoryClass::Ordinary
                    || core.parent_record_id() != Some(raw.head_record_id())
                    || parent_roots.digest().map_err(|_| {
                        oasis7_wasm_abi::CanonicalCborStreamError::Invalid("mailbox parent roots")
                    })? != raw.roots_id()
                    || core.parent_body().and_then(|body| body.digest().ok())
                        != Some(raw.head_body_id())
                {
                    return Err(oasis7_wasm_abi::CanonicalCborStreamError::Invalid(
                        "mailbox direct-parent provenance",
                    ));
                }
                let _proof = verify_domain_event_occurrence_v1(
                    &DomainEventProofRequestV1 {
                        event: event.event(),
                        occurrence: raw,
                        expected_head_record: raw.head_record_id(),
                        expected_roots: raw.roots_id(),
                    },
                    &mut context,
                )?;
                Ok(ProvenanceKind::CandidateInherited)
            }
            _ => Err(oasis7_wasm_abi::CanonicalCborStreamError::Invalid(
                "mailbox provenance mode",
            )),
        })();
    work.canonical = context
        .into_work()
        .map_err(MailboxWorldEventOriginalErrorV1::Encoding)?;
    let kind = result.map_err(MailboxWorldEventOriginalErrorV1::Encoding)?;
    if cancelled() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(charge.retain(CheckedMailboxWorldEventOriginalV1 {
        event,
        occurrence,
        anchor,
        class,
        kind,
    }))
}

#[cfg(test)]
#[path = "incremental_world_raw_complete_original_tests.rs"]
mod tests;
