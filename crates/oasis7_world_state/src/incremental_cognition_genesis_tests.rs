use super::*;
use crate::{
    incremental_field_member::{MemberRecordLimitsV1, read_member_record_v1},
    incremental_semantic_key::SemanticKeyBudget,
    incremental_world_field_key::WorldFieldKeyV1,
};
use oasis7_formal_protocol::incremental_value::{ValueIoBudget, ValueNodeReader, ValueReadContext};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass};
use std::{cell::RefCell, collections::BTreeMap, sync::Arc};
type PageMap = BTreeMap<[u8; 32], Arc<ReservedValue<Vec<u8>>>>;
struct Pages {
    nodes: ReservedValue<RefCell<PageMap>>,
}
impl ValueNodeWriter for Pages {
    fn put_node(
        &mut self,
        id: [u8; 32],
        bytes: ReservedValue<Vec<u8>>,
    ) -> Result<(), ValueTreeError> {
        assert!(self.nodes.borrow().len() < 32);
        self.nodes.borrow_mut().insert(id, Arc::new(bytes));
        Ok(())
    }
}
impl ValueNodeReader for Pages {
    fn read_node(
        &self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        let nodes = self.nodes.borrow();
        let bytes = nodes
            .get(id)
            .ok_or(ValueTreeError::Invalid("test missing"))?;
        if class != MemoryClass::Ordinary || !bytes.uses_budget(memory) || bytes.len() > max {
            return Err(ValueTreeError::Invalid("test source"));
        }
        Ok(bytes.clone())
    }
}
fn source() -> serde_json::Value {
    serde_json::json!({"schema_version":"cognition-persistence.v1", "cognition_journal":{"schema_version":"cognition-journal.v1","head_seq":u64::MAX,"head_digest":"原digest","events":[]}, "responses":[],"commit_records":[],"receipt_registry":[],"receipt_lineage_registry":[],"feedback_outbox":{},"idempotency_index":{},"staged_actions":{},"scheduler_state":{},"continuations":[],"recovery":null,"reorg_invalidation_epoch":0})
}
fn io() -> ValueIoBudget {
    ValueIoBudget {
        nodes_remaining: 1000,
        read_bytes_remaining: 1 << 20,
        write_bytes_remaining: 1 << 20,
    }
}
#[test]
fn actual_gen_preserves_original_control_payloads_and_finite_record_allowance() {
    let memory = MemoryBudget::new(8 << 20, 0).unwrap();
    let _input = memory.reserve(16 << 10, MemoryClass::Ordinary).unwrap();
    let source = source();
    let mut pages = Pages {
        nodes: memory
            .reserve(32 << 10, MemoryClass::Ordinary)
            .unwrap()
            .retain(RefCell::new(BTreeMap::new())),
    };
    let _writes_admission = memory.reserve(16 << 10, MemoryClass::Ordinary).unwrap();
    let mut writes = Vec::with_capacity(10);
    let mut quota = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    };
    let limits = MemberRecordLimitsV1 {
        semantic_bytes: 4096,
        payload_bytes: 4096,
        record_bytes: 8192,
    };
    let mut records = 10;
    prepare_cognition_controls_genesis_v1(
        CognitionEnvelopeRefV1::new(&source).unwrap(),
        &mut MemberRecordWriteContext {
            sink: &mut pages,
            memory: &memory,
            io: &mut quota,
            key_work: &mut keys,
            limits,
            cancelled: || false,
        },
        &mut records,
        &mut |write| {
            writes.push(write);
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(records, 0);
    assert_eq!(writes.len(), 10);
    let mut quota = io();
    for (index, field) in CognitionControlFieldV1::REQUIRED
        .into_iter()
        .chain([CognitionControlFieldV1::ReorgInvalidationEpoch])
        .enumerate()
    {
        let write = &writes[index * 2 + 1];
        let key = WorldFieldKeyV1::new(
            field.descriptor(),
            WorldFieldRecordKindV1::Entry,
            Some({
                let mut hash = blake3::Hasher::new();
                hash.update(b"oasis7.iws.semantic-key.v1\0");
                hash.update(&[0]);
                *hash.finalize().as_bytes()
            }),
        )
        .unwrap();
        assert_eq!(write.mutation.key, key.as_bytes());
        let _output_admission = memory.reserve(1024, MemoryClass::Ordinary).unwrap();
        let mut payload = Vec::with_capacity(1024);
        read_member_record_v1(
            &write.mutation.after.unwrap(),
            &key,
            &mut ValueReadContext {
                source: &pages,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut quota,
            },
            limits,
            &mut std::io::sink(),
            &mut payload,
            || false,
        )
        .unwrap();
        let expected: &[u8] = match field {
            CognitionControlFieldV1::Schema => b"cognition-persistence.v1",
            CognitionControlFieldV1::JournalSchema => b"cognition-journal.v1",
            CognitionControlFieldV1::JournalHeadSeq => &u64::MAX.to_be_bytes(),
            CognitionControlFieldV1::JournalHeadDigest => "原digest".as_bytes(),
            CognitionControlFieldV1::ReorgInvalidationEpoch => &0u64.to_be_bytes(),
        };
        assert_eq!(payload.as_slice(), expected);
    }
}
