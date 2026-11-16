use super::*;
use crate::{
    incremental_cognition_fields::CognitionEnvelopeRefV1,
    incremental_inventory_value_read::InventoryValueReader,
    incremental_json_records::{JsonReadWorkV1, JsonWriteWorkV1},
    incremental_semantic_key::SemanticKeyBudget,
};
use oasis7_formal_protocol::incremental_value::{ValueIoBudget, ValueReadContext};
use oasis7_proto::memory_budget::MemoryBudget;
use std::{cell::RefCell, collections::BTreeMap, sync::Arc};
struct Pages(RefCell<BTreeMap<[u8; 32], Arc<ReservedValue<Vec<u8>>>>>);
impl ValueNodeWriter for Pages {
    fn put_node(
        &mut self,
        id: [u8; 32],
        bytes: ReservedValue<Vec<u8>>,
    ) -> Result<(), ValueTreeError> {
        self.0.borrow_mut().insert(id, Arc::new(bytes));
        Ok(())
    }
}
impl StatePageWriter for Pages {}
impl ValueNodeReader for Pages {
    fn read_node(
        &self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        let rows = self.0.borrow();
        let row = rows.get(id).ok_or_else(invalid)?;
        if row.len() > max {
            return Err(invalid());
        }
        Ok(Arc::new(
            memory.reserve(row.len(), class)?.retain(row.to_vec()),
        ))
    }
}
fn io() -> ValueIoBudget {
    ValueIoBudget {
        nodes_remaining: 1_000_000,
        read_bytes_remaining: 128 << 20,
        write_bytes_remaining: 128 << 20,
    }
}
fn json() -> JsonReadWorkV1 {
    JsonReadWorkV1 {
        cells_remaining: 100_000,
        json_nodes_remaining: 10_000,
        max_depth: 32,
        max_payload_bytes: 1 << 20,
    }
}
fn envelope(receipts: Option<serde_json::Value>) -> serde_json::Value {
    let mut value = serde_json::json!({"schema_version":"cognition-persistence.v1","cognition_journal":{"schema_version":"cognition-journal.v1","head_seq":0,"head_digest":"original","events":[]},"responses":[],"commit_records":[],"receipt_registry":[],"receipt_lineage_registry":[],"feedback_outbox":{},"idempotency_index":{},"staged_actions":{},"scheduler_state":{},"continuations":[],"recovery":null});
    if let Some(receipts) = receipts {
        value["agent_delegation"] = serde_json::json!({"grants":{},"spent":{},"decisions":{},"receipts":receipts,"controls":{}})
    }
    value
}
#[test]
fn v3_genesis_roundtrip_preserves_absent_empty_and_nonempty_nested_original_receipts() {
    let memory = MemoryBudget::new(64 << 20, 8 << 20).unwrap();
    let mut pages = Pages(RefCell::new(BTreeMap::new()));
    let mut quota = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 64 << 20,
    };
    let mut work = ProofLedgerWriteWorkV1 {
        cells_remaining: 1_000_000,
        max_pending_node_bytes: 1 << 20,
        json: JsonWriteWorkV1 {
            cells_remaining: 1_000_000,
            json_nodes_remaining: 100_000,
            max_json_depth: 32,
            max_pending_node_bytes: 1 << 20,
        },
    };
    let originals = [
        None,
        Some(serde_json::json!({})),
        Some(
            serde_json::json!({" receipt α ":{"receipt_id":"inner≠outer","unknown":[null,true,-7,{"":"value"}]},"z":{"receipt_id":"z"}}),
        ),
    ];
    let mut roots = Vec::new();
    for original in originals {
        let value = envelope(original.clone());
        let borrowed = CognitionEnvelopeRefV1::new(&value).unwrap();
        let manifest = prepare_cognition_proof_ledger_genesis_v1(
            borrowed,
            [13; 32],
            &mut MemberRecordWriteContext {
                sink: &mut pages,
                memory: &memory,
                io: &mut quota,
                key_work: &mut keys,
                limits: MemberRecordLimitsV1 {
                    semantic_bytes: 1 << 20,
                    payload_bytes: 1 << 20,
                    record_bytes: 2 << 20,
                },
                cancelled: || false,
            },
            &mut work,
        )
        .unwrap();
        assert_eq!(manifest.wire_version, CognitionProofLedgerWireVersionV1::V3);
        assert_eq!(manifest.previous_record, None);
        let wire = manifest.canonical_bytes().unwrap();
        assert_eq!(wire.len(), COGNITION_PROOF_LEDGER_BYTES_V3);
        assert!(wire.starts_with(COGNITION_PROOF_LEDGER_MAGIC_V3));
        let decoded = CognitionProofLedgerManifestV1::from_canonical_bytes(&wire).unwrap();
        assert_eq!(*manifest, decoded);
        assert!(
            decoded
                .require_field_source(CognitionProofLedgerFieldV1::DelegationReceipts)
                .is_ok()
        );
        let root = decoded.fields[CognitionProofLedgerFieldV1::DelegationReceipts.index()].as_ref();
        assert_eq!(root.is_some(), original.is_some());
        roots.push(root.cloned());
        if let (Some(root), Some(original)) = (root, original) {
            let p = profile(CognitionProofLedgerFieldV1::DelegationReceipts);
            let jp = JsonRecordProfileV1 {
                field: p.field,
                schema: p.schema,
            };
            let mut stop = || false;
            let mut values = ValueReadContext {
                source: &pages,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut quota,
            };
            let mut records = InventoryValueReader {
                values: &mut values,
                key_work: &mut keys,
                semantic_bytes: 1 << 20,
                record_bytes: 2 << 20,
                cancelled: &mut stop,
            };
            // The same original qualification and materialization kernels used by
            // authenticated Cold; no serde DTO alias/normalization is introduced.
            let q = crate::incremental_json_records::qualify_json_subtree_records_v1(
                jp,
                root,
                &mut records,
                &mut json(),
            )
            .unwrap();
            crate::incremental_json_records::require_json_container_records_v1(
                jp,
                &q,
                false,
                &mut records,
                &mut json(),
            )
            .unwrap();
            let (actual, _fee) = crate::incremental_json_records::materialize_records(
                jp,
                &q,
                &mut records,
                &mut json(),
                1 << 20,
            )
            .unwrap();
            assert_eq!(actual, original);
            assert!(value.get("delegation.receipts").is_none());
            assert_eq!(value["agent_delegation"]["receipts"], actual);
        }
    }
    assert_ne!(roots[0], roots[1]);
    assert_ne!(roots[1], roots[2]);
}
#[test]
fn v2_codec_rejects_version_length_mismatch_and_legacy_slot_smuggling() {
    let root = StateRootV1::empty(StateNamespaceV1::Artifacts).unwrap();
    let mut manifest = CognitionProofLedgerManifestV1 {
        wire_version: CognitionProofLedgerWireVersionV1::V2,
        roots: [3; 32],
        previous_record: None,
        fields: [
            Some(root.clone()),
            Some(root.clone()),
            Some(root.clone()),
            Some(root.clone()),
            None,
            Some(root),
            None,
            None,
        ],
    };
    let wire = manifest.canonical_bytes().unwrap();
    let mut wrong = wire.to_vec();
    wrong[..COGNITION_PROOF_LEDGER_MAGIC_V1.len()].copy_from_slice(COGNITION_PROOF_LEDGER_MAGIC_V1);
    assert!(CognitionProofLedgerManifestV1::from_canonical_bytes(&wrong).is_err());
    manifest.wire_version = CognitionProofLedgerWireVersionV1::V1;
    assert!(manifest.canonical_bytes().is_err());
}

#[test]
fn v3_original_service_values_preserve_absence_empty_nonempty_and_legacy_value_domain() {
    let memory = MemoryBudget::new(64 << 20, 8 << 20).unwrap();
    let mut pages = Pages(RefCell::new(BTreeMap::new()));
    let mut quota = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 64 << 20,
    };
    let mut work = ProofLedgerWriteWorkV1 {
        cells_remaining: 1_000_000,
        max_pending_node_bytes: 1 << 20,
        json: JsonWriteWorkV1 {
            cells_remaining: 1_000_000,
            json_nodes_remaining: 100_000,
            max_json_depth: 32,
            max_pending_node_bytes: 1 << 20,
        },
    };
    let cases = [
        (None, None),
        (Some(serde_json::json!({})), Some(serde_json::json!({}))),
        (
            Some(serde_json::json!({" [a,b] ":{"explanation":"原文","unknown":[null,true]}})),
            Some(serde_json::json!({"commit:original":"original-digest"})),
        ),
        (
            Some(serde_json::Value::Null),
            Some(serde_json::json!(["old raw extra", false])),
        ),
    ];
    let mut prior = None;
    for (causal, digests) in cases {
        let mut original = envelope(None);
        if let Some(value) = &causal {
            original["service_causal_proposals"] = value.clone()
        }
        if let Some(value) = &digests {
            original["action_digests"] = value.clone()
        }
        let manifest = prepare_cognition_proof_ledger_genesis_v1(
            CognitionEnvelopeRefV1::new(&original).unwrap(),
            [51; 32],
            &mut MemberRecordWriteContext {
                sink: &mut pages,
                memory: &memory,
                io: &mut quota,
                key_work: &mut keys,
                limits: MemberRecordLimitsV1 {
                    semantic_bytes: 1 << 20,
                    payload_bytes: 1 << 20,
                    record_bytes: 2 << 20,
                },
                cancelled: || false,
            },
            &mut work,
        )
        .unwrap();
        assert_eq!(manifest.wire_version, CognitionProofLedgerWireVersionV1::V3);
        let wire = manifest.canonical_bytes().unwrap();
        assert_eq!(wire.len(), COGNITION_PROOF_LEDGER_BYTES_V3);
        assert!(wire.starts_with(COGNITION_PROOF_LEDGER_MAGIC_V3));
        assert_eq!(
            *manifest,
            CognitionProofLedgerManifestV1::from_canonical_bytes(&wire).unwrap()
        );
        if let Some(previous) = prior {
            assert_ne!(previous, wire)
        }
        prior = Some(wire);
        for (field, expected) in [
            (CognitionProofLedgerFieldV1::ServiceCausalProposals, causal),
            (CognitionProofLedgerFieldV1::ActionDigests, digests),
        ] {
            assert!(manifest.require_field_source(field).is_ok());
            let root = manifest.fields[field.index()].as_ref();
            assert_eq!(root.is_some(), expected.is_some());
            if let (Some(root), Some(expected)) = (root, expected) {
                let p = profile(field);
                let jp = JsonRecordProfileV1 {
                    field: p.field,
                    schema: p.schema,
                };
                let mut stop = || false;
                let mut values = ValueReadContext {
                    source: &pages,
                    memory: &memory,
                    class: MemoryClass::Ordinary,
                    io: &mut quota,
                };
                let mut records = InventoryValueReader {
                    values: &mut values,
                    key_work: &mut keys,
                    semantic_bytes: 1 << 20,
                    record_bytes: 2 << 20,
                    cancelled: &mut stop,
                };
                let q = crate::incremental_json_records::qualify_json_subtree_records_v1(
                    jp,
                    root,
                    &mut records,
                    &mut json(),
                )
                .unwrap();
                let (actual, _fee) = crate::incremental_json_records::materialize_records(
                    jp,
                    &q,
                    &mut records,
                    &mut json(),
                    1 << 20,
                )
                .unwrap();
                assert_eq!(actual, expected);
            }
        }
    }
}
