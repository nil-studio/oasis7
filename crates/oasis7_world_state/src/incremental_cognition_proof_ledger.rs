//! Independent accepted ProofLedger ownership. DATA and business permission stay separate.
#[path = "incremental_cognition_proof_ledger_manifest.rs"]
mod manifest;
pub use crate::incremental_typed_record_codec::{
    RecoveredTypedRecordV1 as ProofLedgerRecoveredV1,
    TypedRecordReadWorkV1 as ProofLedgerReadWorkV1,
    TypedRecordWriteWorkV1 as ProofLedgerWriteWorkV1,
};
use crate::{
    incremental_field_member::*,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_json_records::{
        JsonReadWorkV1, JsonRecordProfileV1, JsonSubtreeQualificationV1, qualify_json_subtree_v1,
    },
    incremental_projection::{FieldDescriptor, FieldDestination, FieldOwner, FieldShape},
    incremental_record_tree::RecordTreeLabelsV1,
    incremental_typed_record_codec::{self as typed, TypedRecordProfileV1},
};
pub use manifest::*;
use oasis7_formal_protocol::{
    incremental_material::VerifiedFactOccurrenceV1,
    incremental_state::{
        MerkleNodeReader, StateNamespaceV1, StatePageWriter, StateRootV1, StateScanBudget,
    },
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
#[path = "incremental_cognition_proof_ledger_source.rs"]
mod source;
pub use source::*;
#[path = "incremental_cognition_proof_ledger_producer.rs"]
mod producer;
pub use producer::*;
#[path = "incremental_cognition_proof_ledger_candidate.rs"]
mod candidate;
pub use candidate::*;
#[path = "incremental_cognition_proof_ledger_overlay.rs"]
mod overlay;
pub use overlay::*;
const LABELS: RecordTreeLabelsV1 = RecordTreeLabelsV1 {
    read: "ProofLedger page",
    pending: "ProofLedger pending",
    tree: "ProofLedger tree",
    digest: "ProofLedger digest",
    read_allowance: "ProofLedger reads",
    admission: "ProofLedger admission",
    bound: "ProofLedger bound",
    producer_pool: "ProofLedger pool",
    page: "ProofLedger page",
    write_allowance: "ProofLedger writes",
};
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("independent Cognition ProofLedger")
}
fn profile(field: CognitionProofLedgerFieldV1) -> TypedRecordProfileV1 {
    TypedRecordProfileV1 {
        field: FieldDescriptor {
            owner: FieldOwner::Cognition,
            name: match field {
                CognitionProofLedgerFieldV1::CommitRecords => "proof_ledger.commit_records.json.v1",
                CognitionProofLedgerFieldV1::ReceiptRegistry => {
                    "proof_ledger.receipt_registry.json.v1"
                }
                CognitionProofLedgerFieldV1::ReceiptLineageRegistry => {
                    "proof_ledger.receipt_lineage_registry.json.v1"
                }
                CognitionProofLedgerFieldV1::Recovery => "proof_ledger.recovery.json.v1",
                CognitionProofLedgerFieldV1::RuntimeBinding => {
                    "proof_ledger.runtime_binding.json.v1"
                }
            },
            shape: FieldShape::Map,
            destination: FieldDestination::State(StateNamespaceV1::Artifacts),
        },
        schema: [0xa1 + field as u8; 32],
        labels: LABELS,
        reference_schema: None,
    }
}
/// GEN only: borrows the five actual original side fields, not a Snapshot copy.
/// The returned roots are provisional until their Result is genuinely accepted.
pub fn prepare_cognition_proof_ledger_field_v1<W, C>(
    field: CognitionProofLedgerFieldV1,
    value: &serde_json::Value,
    write: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut ProofLedgerWriteWorkV1,
) -> Result<ReservedValue<StateRootV1>, MemberRecordError>
where
    W: ValueNodeWriter + ValueNodeReader + StatePageWriter,
    C: FnMut() -> bool,
{
    if matches!(
        field,
        CognitionProofLedgerFieldV1::CommitRecords
            | CognitionProofLedgerFieldV1::ReceiptRegistry
            | CognitionProofLedgerFieldV1::ReceiptLineageRegistry
    ) && !value.is_array()
    {
        return Err(invalid().into());
    }
    typed::prepare_typed_record_body_v1(value, &profile(field), write, work)
}
/// A structural, head-bound independent source. It is not a SafeAlternative permit.
pub struct AcceptedCognitionProofLedgerV1 {
    manifest: ReservedValue<CognitionProofLedgerManifestV1>,
    occurrence: ReservedValue<VerifiedFactOccurrenceV1>,
    fields: [Option<ReservedValue<JsonSubtreeQualificationV1>>; 5],
}
impl AcceptedCognitionProofLedgerV1 {
    pub fn manifest(&self) -> &CognitionProofLedgerManifestV1 {
        &self.manifest
    }
    pub fn occurrence(&self) -> &ReservedValue<VerifiedFactOccurrenceV1> {
        &self.occurrence
    }
    pub fn read_field<M, R, E, C>(
        &self,
        field: CognitionProofLedgerFieldV1,
        read: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
        scan: &mut StateScanBudget,
        work: &mut ProofLedgerReadWorkV1,
    ) -> Result<Option<ProofLedgerRecoveredV1<serde_json::Value>>, ValueTreeError>
    where
        M: MerkleNodeReader,
        R: ValueNodeReader,
        E: FnMut() -> Option<ValueTreeError>,
        C: FnMut() -> bool,
    {
        self.check_pool(read.values.memory, read.values.class)?;
        read.check()?;
        let Some(root) = self.manifest.fields[field.index()].as_ref() else {
            return Ok(None);
        };
        let p = profile(field);
        self.fields[field.index()]
            .as_ref()
            .ok_or_else(invalid)?
            .validate_binding(
                JsonRecordProfileV1 {
                    field: p.field,
                    schema: p.schema,
                },
                root,
            )?;
        let mut field_read = WorldFieldReadContextV1 {
            root,
            state: read.state,
            values: oasis7_formal_protocol::incremental_value::ValueReadContext {
                source: read.values.source,
                memory: read.values.memory,
                class: read.values.class,
                io: &mut *read.values.io,
            },
            key_work: &mut *read.key_work,
            semantic_bytes: read.semantic_bytes,
            record_bytes: read.record_bytes,
            take_state_error: &mut read.take_state_error,
            cancelled: &mut read.cancelled,
        };
        typed::recover_typed_record_body_v1(root, &p, &mut field_read, scan, work).map(Some)
    }
    fn check_pool(
        &self,
        memory: &oasis7_proto::memory_budget::MemoryBudget,
        class: MemoryClass,
    ) -> Result<(), ValueTreeError> {
        if !self.manifest.uses_budget(memory)
            || !self.occurrence.uses_budget(memory)
            || self.occurrence.allocation_class() != class
            || self.fields.iter().flatten().any(|q| !q.uses_budget(memory))
        {
            return Err(invalid());
        }
        Ok(())
    }
}
fn qualify_fields<M, R, E, C>(
    manifest: &CognitionProofLedgerManifestV1,
    read: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut JsonReadWorkV1,
) -> Result<[Option<ReservedValue<JsonSubtreeQualificationV1>>; 5], ValueTreeError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let mut fields = std::array::from_fn(|_| None);
    for field in CognitionProofLedgerFieldV1::ALL {
        if let Some(root) = &manifest.fields[field.index()] {
            let p = profile(field);
            let jp = JsonRecordProfileV1 {
                field: p.field,
                schema: p.schema,
            };
            let mut field_read = WorldFieldReadContextV1 {
                root,
                state: read.state,
                values: oasis7_formal_protocol::incremental_value::ValueReadContext {
                    source: read.values.source,
                    memory: read.values.memory,
                    class: read.values.class,
                    io: &mut *read.values.io,
                },
                key_work: &mut *read.key_work,
                semantic_bytes: read.semantic_bytes,
                record_bytes: read.record_bytes,
                take_state_error: &mut read.take_state_error,
                cancelled: &mut read.cancelled,
            };
            let q = qualify_json_subtree_v1(jp, root, &mut field_read, work)?;
            if field.index() < 3 {
                crate::incremental_json_records::require_json_container_records_v1(
                    jp,
                    &q,
                    true,
                    &mut field_read.records(),
                    work,
                )?;
            }
            fields[field.index()] = Some(q);
        }
    }
    Ok(fields)
}
#[cfg(test)]
#[path = "incremental_cognition_proof_ledger_tests.rs"]
mod tests;
