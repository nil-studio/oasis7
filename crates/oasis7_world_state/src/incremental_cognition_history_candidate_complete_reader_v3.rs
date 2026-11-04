//! Actual reader-to-source conjunction. Bootstrap scans only a new active scope;
//! ordinary inheritance replays the exact new locator suffix, never retained H.
use super::*;
use crate::{
    incremental_cognition_history_cbor::*,
    incremental_cognition_history_hash_outboard::CognitionHeadHashWorkV1,
    incremental_cognition_lifecycle_index_v2::CognitionLifecycleWorkV2,
    incremental_cognition_lifecycle_qualification_v2::*,
    incremental_field_member::{MemberRecordError, MemberRecordWriteContext},
    incremental_field_read::WorldFieldReadContextV1,
};
use oasis7_formal_protocol::{
    incremental_material::{CandidateFactRequestV1, CoreError, check_candidate_fact_v1},
    incremental_state::{MerkleNodeReader, StatePageWriter},
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
use std::io::Read;
#[path = "incremental_cognition_history_candidate_reader_keys_v3.rs"]
mod keys;

/// Supplies the actual original bytes. The consumer still authenticates their
/// ID, length, schema and occurrence; returning Ok alone grants no proof.
/// Implementations must finish their verified cursor and propagate its error.
pub trait CognitionHistoryOriginalReadV3 {
    fn with_original<T>(
        &mut self,
        id: &[u8; 32],
        bytes: u64,
        consume: impl FnOnce(&mut dyn Read) -> Result<T, ValueTreeError>,
    ) -> Result<T, ValueTreeError>;
}
pub struct CognitionHistoryCandidateReaderWorkV3<'a> {
    pub indexes: &'a mut CognitionHistoryIndexWorkV1,
    pub cbor: &'a mut CognitionHistoryCborWorkV1,
    pub hashes: &'a mut CognitionHeadHashWorkV1,
    pub lifecycle: &'a mut CognitionLifecycleWorkV2,
}
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("Cognition source reader binding")
}
fn member(error: MemberRecordError) -> ValueTreeError {
    match error {
        MemberRecordError::Value(e) => e,
        MemberRecordError::Io(e) => ValueTreeError::Io(e),
        MemberRecordError::Invalid(e) => ValueTreeError::Invalid(e),
        MemberRecordError::Key(_) => invalid(),
    }
}
fn formal(error: CoreError<std::convert::Infallible>) -> ValueTreeError {
    use oasis7_formal_protocol::incremental_execution::ExecutionError;
    match error {
        CoreError::Cancelled => ValueTreeError::Cancelled,
        CoreError::ResourceLimited(e)
        | CoreError::Execution(ExecutionError::ResourceLimited(e)) => {
            ValueTreeError::ResourceLimited(e)
        }
        CoreError::Execution(ExecutionError::Memory(e)) => ValueTreeError::Memory(e),
        _ => invalid(),
    }
}
/// Checks active source provenance, not permission to reset business state,
/// skip terminal recovery, publish a receipt, or call providers/effects.
/// Both contexts share the caller's actual pool and continuous IO/work limits.
pub fn check_candidate_complete_cognition_history_from_readers_v3<M, R, E, C, W, D, O>(
    request: &CognitionHistorySelectionRequestV1<'_>,
    selected: ReservedValue<CheckedCognitionHistoryViewV1>,
    current: CognitionHistoryCurrentSourceV1<'_>,
    parent: Option<&ReservedValue<CompleteCognitionHistorySourceV3>>,
    read: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, D>,
    originals: &mut O,
    work: &mut CognitionHistoryCandidateReaderWorkV3<'_>,
) -> Result<ReservedValue<CheckedCognitionHistorySourceV3>, ValueTreeError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: StatePageWriter + ValueNodeReader,
    D: FnMut() -> bool,
    O: CognitionHistoryOriginalReadV3,
{
    read.check()?;
    let memory = read.values.memory;
    if read.values.class != MemoryClass::Ordinary || !write.memory.same_pool(memory) {
        return Err(invalid());
    }
    // Binds the actual current family/absence before any source proof is built.
    let disabled = match &current {
        CognitionHistoryCurrentSourceV1::Disabled(absence) => Some(match parent {
            Some(parent) => check_candidate_disabled_cognition_history_successor_v3(
                request,
                &selected,
                absence,
                parent,
                memory,
                &mut read.cancelled,
            )?,
            None => check_candidate_disabled_cognition_history_source_v3(
                request,
                &selected,
                absence,
                None,
                memory,
                &mut read.cancelled,
            )?,
        }),
        CognitionHistoryCurrentSourceV1::Enabled(_) => None,
    };
    let current = bind_candidate_cognition_history_current_v3(
        request,
        &selected,
        request.core.roots(),
        current,
        read,
    )?;
    let mut indexes = std::array::from_fn(|_| None);
    let mut heads = std::array::from_fn(|_| None);
    let mut lifecycle = None;
    if selected.manifest().enabled {
        for (slot, field) in [
            CognitionHistoryFieldV1::JournalEvents,
            CognitionHistoryFieldV1::Responses,
            CognitionHistoryFieldV1::EconomyJournal,
            CognitionHistoryFieldV1::EconomyProvisionJournal,
        ]
        .into_iter()
        .enumerate()
        {
            let declared = selected.manifest().index_ref(field);
            if !declared.present {
                continue;
            }
            let old = parent.and_then(|p| p.indexes()[slot].as_ref());
            let full = if old.is_none() {
                Some(qualify_cognition_history_index_v1(
                    field,
                    declared,
                    read,
                    work.indexes,
                    |_, _| Ok(()),
                )?)
            } else {
                None
            };
            let mut index = match (parent, old, full.as_ref()) {
                (Some(p), None, Some(full)) => {
                    CandidateCognitionHistoryIndexBuilderV3::activate_field(
                        request,
                        &selected,
                        p,
                        full,
                        memory,
                        &mut read.cancelled,
                    )?
                }
                _ => CandidateCognitionHistoryIndexBuilderV3::new(
                    request,
                    &selected,
                    field,
                    full.as_ref(),
                    old.zip(parent.map(|p| p.selected())),
                    memory,
                    &mut read.cancelled,
                )?,
            };
            let head_slot = match slot {
                0 => Some(0),
                2 => Some(1),
                3 => Some(2),
                _ => None,
            };
            let mut head = if let Some(h) = head_slot {
                Some(match (parent, old, full.as_ref()) {
                    (Some(p), Some(old), _) => CandidateCognitionHeadSourceBuilderV3::ordinary(
                        request,
                        &selected,
                        p.selected(),
                        p.heads()[h].as_ref().ok_or_else(invalid)?,
                        old,
                        memory,
                        &mut read.cancelled,
                    )?,
                    (Some(p), None, Some(full)) => {
                        CandidateCognitionHeadSourceBuilderV3::activate_field(
                            request,
                            &selected,
                            p,
                            full,
                            memory,
                            &mut read.cancelled,
                        )?
                    }
                    (None, _, Some(full)) => CandidateCognitionHeadSourceBuilderV3::genesis(
                        request,
                        &selected,
                        full,
                        memory,
                        &mut read.cancelled,
                    )?,
                    _ => return Err(invalid()),
                })
            } else {
                None
            };
            let mut life = if slot == 0 {
                Some(match (parent, old) {
                    (Some(p), None) => CandidateCognitionLifecycleBuilderV3::activate(
                        request,
                        &selected,
                        p,
                        memory,
                        &mut read.cancelled,
                    )?,
                    _ => CandidateCognitionLifecycleBuilderV3::new(
                        request,
                        &selected,
                        parent
                            .map(|p| {
                                p.lifecycle_parent()
                                    .map(|life| (life, p.selected()))
                                    .ok_or_else(invalid)
                            })
                            .transpose()?,
                        memory,
                        &mut read.cancelled,
                    )?,
                })
            } else {
                None
            };
            let start = old.map_or(0, |index| index.record_count());
            // No full qualification or prefix point lookup on inherited fields.
            for ordinal in start..declared.records {
                let point = super::index::read_cognition_history_index_point_parts_v1(
                    &declared.root,
                    field,
                    ordinal,
                    read,
                    work.indexes,
                )?;
                let row = point.row();
                let occurrence = check_candidate_fact_v1(
                    &CandidateFactRequestV1 {
                        kind: field.fact_kind(),
                        candidate: request.candidate,
                        core: request.core,
                        originals: request.originals,
                        ordinal: row.location.ordinal,
                    },
                    memory,
                    &mut read.cancelled,
                )
                .map_err(formal)?;
                let schema = originals.with_original(
                    &row.location.original,
                    row.encoded_bytes,
                    |mut input| {
                        verify_cognition_history_original_record_schema_v1(
                            CognitionOriginalCborRequestV1 {
                                field,
                                id: row.location.original,
                                bytes: row.encoded_bytes,
                            },
                            &mut input,
                            memory,
                            MemoryClass::Ordinary,
                            work.cbor,
                            &mut read.cancelled,
                        )
                    },
                )?;
                let original = bind_checked_cognition_indexed_original_v3(
                    point,
                    schema,
                    &occurrence,
                    &selected,
                    memory,
                    &mut read.cancelled,
                )?;
                index
                    .push(&original, read, write, work.indexes)
                    .map_err(member)?;
                if let Some(life) = &mut life {
                    let strings = originals.with_original(
                        &row.location.original,
                        row.encoded_bytes,
                        |mut input| {
                            keys::read_keys(
                                &mut input,
                                row.encoded_bytes,
                                memory,
                                work.cbor,
                                &mut read.cancelled,
                            )
                        },
                    )?;
                    life.push(
                        &original,
                        std::array::from_fn(|i| strings[i].as_str()),
                        read,
                        write,
                        work.lifecycle,
                    )
                    .map_err(member)?;
                }
                if let Some(head) = &mut head {
                    let point = super::index::read_cognition_history_index_point_parts_v1(
                        &declared.root,
                        field,
                        ordinal,
                        read,
                        work.indexes,
                    )?;
                    originals.with_original(
                        &row.location.original,
                        row.encoded_bytes,
                        |mut input| {
                            let mut source_work = CandidateCognitionHeadSourceWorkV3 {
                                cbor: work.cbor,
                                hashes: work.hashes,
                            };
                            if old.is_some() {
                                head.push_delta(
                                    point,
                                    &mut input,
                                    read,
                                    write,
                                    work.indexes,
                                    &mut source_work,
                                )
                                .map_err(member)
                            } else {
                                head.push(point, &mut input, &mut source_work, &mut read.cancelled)
                            }
                        },
                    )?;
                }
            }
            indexes[slot] = Some(index.finish(&mut read.cancelled)?);
            if let Some(life) = life {
                lifecycle = Some(life.finish(&mut read.cancelled)?);
            }
            if let Some(head) = head {
                let h = head_slot.ok_or_else(invalid)?;
                let cache_ref = selected
                    .derived()
                    .and_then(|refs| refs.caches[h])
                    .ok_or_else(invalid)?;
                let cache = recover_cognition_head_cache_v1(
                    cache_ref,
                    &mut read.values,
                    work.hashes,
                    &mut read.cancelled,
                )?;
                heads[h] = Some(head.finish(&cache, work.hashes, &mut read.cancelled)?);
            }
        }
    }
    read.check()?;
    if (write.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    qualify_checked_cognition_history_source_v3(
        CheckedCognitionHistoryComponentsV3 {
            selected,
            current,
            indexes,
            heads,
            lifecycle,
            disabled,
        },
        memory,
        &mut read.cancelled,
    )
}
