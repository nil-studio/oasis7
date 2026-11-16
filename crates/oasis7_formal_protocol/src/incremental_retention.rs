//! Independent retention commitment DATA. The signed artifact role binds this
//! descriptor; its payload never enters RootSet or names its current record.
use crate::{
    controlled_authority::{
        formal::{CanonicalRecordV1, incremental},
        replicated_protocol::ArtifactRole,
    },
    incremental_execution::ExecutionError,
    incremental_history::wire::{Reader, Writer},
    incremental_state::{StateNamespaceV1, StateRootV1},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use std::cell::RefCell;
pub const RETENTION_LEDGER_MANIFEST_DOMAIN_V1: &[u8] = b"oasis7.iws.retention-ledger.v1\0";
pub const RETENTION_LEDGER_MANIFEST_MAX_BYTES_V1: usize = 256;
pub(crate) const RETENTION_CODEC_PHASE_BYTES_V1: usize = 128 * 1024;
#[derive(Debug, PartialEq, Eq)]
pub struct RetentionLedgerManifestV1 {
    pub roots: [u8; 32],
    pub previous_record: Option<[u8; 32]>,
    pub payload: Option<StateRootV1>,
}
impl RetentionLedgerManifestV1 {
    pub fn canonical_bytes_with_work(
        &self,
        memory: &MemoryBudget,
        work: &mut dyn FnMut(u64, u64) -> Result<(), ExecutionError>,
    ) -> Result<ReservedValue<Vec<u8>>, ExecutionError> {
        // StateRoot's unique codec validates the fixed-depth sparse empty root. Its
        // work/heap is closed and bounded independently of retention member count.
        work(1024, RETENTION_CODEC_PHASE_BYTES_V1 as u64)?;
        let phase = memory
            .reserve(RETENTION_CODEC_PHASE_BYTES_V1, MemoryClass::Ordinary)
            .map_err(ExecutionError::Memory)?;
        let payload = match &self.payload {
            None => None,
            Some(root) => {
                if root.namespace != StateNamespaceV1::Artifacts {
                    return Err(ExecutionError::Invalid("retention payload namespace"));
                }
                Some(
                    root.canonical_bytes()
                        .map_err(|_| ExecutionError::Invalid("retention payload StateRoot"))?,
                )
            }
        };
        let bank = RefCell::new(work);
        let meter = |s, b| (bank.borrow_mut())(s, b);
        let mut out = Writer::with_work(&meter);
        out.hash(&self.roots);
        out.optional(&self.previous_record);
        match &payload {
            None => out.u8(0),
            Some(bytes) => {
                out.u8(1);
                out.nested(bytes)
            }
        }
        let raw = out.finish(
            RETENTION_LEDGER_MANIFEST_DOMAIN_V1,
            RETENTION_LEDGER_MANIFEST_MAX_BYTES_V1,
        )?;
        Ok(phase.retain(raw))
    }
    /// Pure fixed DATA codec; production callers retain an admitted graph phase,
    /// or use decode_paid_with_work below. No acceptance permission is returned.
    pub fn decode_with_work(
        raw: &[u8],
        work: &mut dyn FnMut(u64, u64) -> Result<(), ExecutionError>,
    ) -> Result<Self, ExecutionError> {
        work(1024, (RETENTION_CODEC_PHASE_BYTES_V1 + raw.len()) as u64)?;
        let mut input = Reader::new(
            raw,
            RETENTION_LEDGER_MANIFEST_DOMAIN_V1,
            RETENTION_LEDGER_MANIFEST_MAX_BYTES_V1,
        )?;
        let roots = input.array()?;
        let previous_record = input.optional()?;
        let payload = match input.u8()? {
            0 => None,
            1 => {
                let bytes = input.nested()?;
                let root = StateRootV1::from_canonical_bytes(bytes)
                    .map_err(|_| ExecutionError::Invalid("retention payload StateRoot"))?;
                if root.namespace != StateNamespaceV1::Artifacts {
                    return Err(ExecutionError::Invalid("retention payload namespace"));
                }
                Some(root)
            }
            _ => return Err(ExecutionError::Invalid("retention payload presence tag")),
        };
        input.finish()?;
        work(0, 0)?;
        Ok(Self {
            roots,
            previous_record,
            payload,
        })
    }
    pub fn decode_paid_with_work(
        raw: &[u8],
        memory: &MemoryBudget,
        work: &mut dyn FnMut(u64, u64) -> Result<(), ExecutionError>,
    ) -> Result<ReservedValue<Self>, ExecutionError> {
        work(1, RETENTION_CODEC_PHASE_BYTES_V1 as u64)?;
        let phase = memory
            .reserve(RETENTION_CODEC_PHASE_BYTES_V1, MemoryClass::Ordinary)
            .map_err(ExecutionError::Memory)?;
        let value = Self::decode_with_work(raw, work)?;
        Ok(phase.retain(value))
    }
    pub fn id_with_work(
        &self,
        memory: &MemoryBudget,
        work: &mut dyn FnMut(u64, u64) -> Result<(), ExecutionError>,
    ) -> Result<[u8; 32], ExecutionError> {
        let raw = self.canonical_bytes_with_work(memory, work)?;
        work(1, raw.len() as u64)?;
        let id = *blake3::hash(&raw).as_bytes();
        work(0, 0)?;
        Ok(id)
    }
    /// Exact fixed binding, independent of any caller-provided root selector.
    pub fn validate_binding_v1(
        &self,
        roots: [u8; 32],
        previous_record: Option<[u8; 32]>,
        work: &mut dyn FnMut(u64, u64) -> Result<(), ExecutionError>,
    ) -> Result<(), ExecutionError> {
        work(2, 96)?;
        if self.roots != roots || self.previous_record != previous_record {
            return Err(ExecutionError::Invalid(
                "retention descriptor roots/previous record",
            ));
        }
        Ok(())
    }
}
#[derive(Debug, PartialEq, Eq)]
pub enum RetentionLedgerBindingErrorV1 {
    MissingCommitment,
    Invalid(&'static str),
}
impl std::fmt::Display for RetentionLedgerBindingErrorV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "retention ledger binding: {self:?}")
    }
}
impl std::error::Error for RetentionLedgerBindingErrorV1 {}
pub fn retention_root_id_v1(
    record: &CanonicalRecordV1,
) -> Result<[u8; 32], RetentionLedgerBindingErrorV1> {
    retention_root_id_with_work_v1(record, &mut |_, _| Ok(()))
}
pub fn retention_root_id_with_work_v1<E: From<RetentionLedgerBindingErrorV1>>(
    record: &CanonicalRecordV1,
    work: &mut dyn FnMut(u64, u64) -> Result<(), E>,
) -> Result<[u8; 32], E> {
    work(1, 0)?;
    if !incremental::has_retention_ledger(record.schema_version) {
        return Err(RetentionLedgerBindingErrorV1::MissingCommitment.into());
    }
    let text = record
        .artifact_roots
        .get(&ArtifactRole::IncrementalRetentionLedger)
        .ok_or(RetentionLedgerBindingErrorV1::Invalid(
            "new profile missing signed retention role",
        ))?;
    work(1, text.len() as u64)?;
    let mut id = [0; 32];
    hex::decode_to_slice(text, &mut id)
        .map_err(|_| RetentionLedgerBindingErrorV1::Invalid("signed retention digest"))?;
    Ok(id)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn absence_and_present_empty_are_distinct_original_roots() {
        let memory = MemoryBudget::new(4 << 20, 0).unwrap();
        let none = RetentionLedgerManifestV1 {
            roots: [1; 32],
            previous_record: None,
            payload: None,
        };
        let empty = RetentionLedgerManifestV1 {
            roots: [1; 32],
            previous_record: None,
            payload: Some(StateRootV1::empty(StateNamespaceV1::Artifacts).unwrap()),
        };
        let a = none
            .canonical_bytes_with_work(&memory, &mut |_, _| Ok(()))
            .unwrap();
        let b = empty
            .canonical_bytes_with_work(&memory, &mut |_, _| Ok(()))
            .unwrap();
        assert_ne!(&*a, &*b);
        assert_ne!(
            none.id_with_work(&memory, &mut |_, _| Ok(())).unwrap(),
            empty.id_with_work(&memory, &mut |_, _| Ok(())).unwrap()
        );
        assert_eq!(
            *RetentionLedgerManifestV1::decode_paid_with_work(&a, &memory, &mut |_, _| Ok(()))
                .unwrap(),
            none
        );
        assert_eq!(
            *RetentionLedgerManifestV1::decode_paid_with_work(&b, &memory, &mut |_, _| Ok(()))
                .unwrap(),
            empty
        );
    }
    #[test]
    fn descriptor_checks_namespace_canonical_tail_and_binding() {
        let memory = MemoryBudget::new(4 << 20, 0).unwrap();
        let wrong = RetentionLedgerManifestV1 {
            roots: [1; 32],
            previous_record: Some([2; 32]),
            payload: Some(StateRootV1::empty(StateNamespaceV1::Business).unwrap()),
        };
        assert!(
            wrong
                .canonical_bytes_with_work(&memory, &mut |_, _| Ok(()))
                .is_err()
        );
        let value = RetentionLedgerManifestV1 {
            payload: None,
            ..wrong
        };
        assert!(
            value
                .validate_binding_v1([1; 32], None, &mut |_, _| Ok(()))
                .is_err()
        );
        let mut bytes = value
            .canonical_bytes_with_work(&memory, &mut |_, _| Ok(()))
            .unwrap()
            .to_vec();
        bytes.push(0);
        assert!(RetentionLedgerManifestV1::decode_with_work(&bytes, &mut |_, _| Ok(())).is_err());
        assert!(
            value
                .canonical_bytes_with_work(&memory, &mut |_, _| Err(ExecutionError::Aborted))
                .is_err()
        );
    }
    #[test]
    fn old_and_retention_profile_predicates_are_exact() {
        for v in 0..=11 {
            assert_eq!(incremental::is_iws_record(v), matches!(v, 7..=10));
            assert_eq!(
                incremental::is_iws_initial_record_schema(v),
                matches!(v, 7 | 9)
            );
            assert_eq!(incremental::has_retention_ledger(v), matches!(v, 9 | 10));
        }
    }
}
