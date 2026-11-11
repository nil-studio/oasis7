use super::*;
use crate::{
    incremental_rollback_delta::*, incremental_rollback_qualification::qualify_rollback_record_v1,
};
struct SharedWriter<'a>(&'a Pages);
impl ValueNodeWriter for SharedWriter<'_> {
    fn put_node(
        &mut self,
        id: [u8; 32],
        bytes: ReservedValue<Vec<u8>>,
    ) -> Result<(), ValueTreeError> {
        if self.0.nodes.borrow().len() >= 65536 {
            return Err(ValueTreeError::ResourceLimited("rollback test pages"));
        }
        self.0.nodes.borrow_mut().insert(id, Arc::new(bytes));
        Ok(())
    }
}
impl ValueNodeReader for SharedWriter<'_> {
    fn read_node(
        &self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        ValueNodeReader::read_node(self.0, id, max, memory, class)
    }
}
impl StatePageWriter for SharedWriter<'_> {}
#[test]
fn bool_delta_preserves_large_identity_and_rejects_inexact_original() {
    let memory = MemoryBudget::new(256 << 20, 4 << 20).unwrap();
    let _source_charge = memory.reserve(8 << 20, MemoryClass::Ordinary).unwrap();
    let mut pages = Pages::new(&memory);
    let before = RollbackAuthorityRecord {
        authority_id: "x".repeat(1 << 20),
        role: RollbackAuthorityRole::OnCall,
        public_key_hex: "11".repeat(32),
        active: false,
    };
    let mut after = before.clone();
    after.active = true;
    let root = prepare_rollback_record_v1(
        RollbackRecordRefV1::Authority(&before),
        &mut MemberRecordWriteContext {
            sink: &mut pages,
            memory: &memory,
            io: &mut io(),
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 8 << 20,
            },
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 4096,
                payload_bytes: 2 << 20,
                record_bytes: 3 << 20,
            },
            cancelled: || false,
        },
        &mut RollbackWorkV1 {
            cells_remaining: 100,
            max_pending_node_bytes: 8 << 20,
        },
    )
    .unwrap();
    let mut read_io = io();
    let mut values = ValueReadContext {
        source: &pages,
        memory: &memory,
        class: MemoryClass::Ordinary,
        io: &mut read_io,
    };
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 16 << 20,
    };
    let mut work = RollbackWorkV1 {
        cells_remaining: 1000,
        max_pending_node_bytes: 8 << 20,
    };
    let mut cancelled = || false;
    let mut ctx = RollbackDecodeContextV1 {
        values: &mut values,
        keys: &mut keys,
        work: &mut work,
        limits: RollbackDecodeLimitsV1 {
            max_output_bytes: 2 << 20,
            max_text_bytes: 2 << 20,
            max_record_bytes: 3 << 20,
        },
        cancelled: &mut cancelled,
    };
    let qualification =
        qualify_rollback_record_v1(&root, RollbackRecordRefV1::Authority(&before), &mut ctx)
            .unwrap();
    let baseline = ctx.values.io.read_bytes_remaining;
    let mut writer = SharedWriter(&pages);
    let mut write_io = io();
    let mut write_keys = SemanticKeyBudget {
        bytes_remaining: 8 << 20,
    };
    let mut output = MemberRecordWriteContext {
        sink: &mut writer,
        memory: &memory,
        io: &mut write_io,
        key_work: &mut write_keys,
        limits: MemberRecordLimitsV1 {
            semantic_bytes: 4096,
            payload_bytes: 2 << 20,
            record_bytes: 3 << 20,
        },
        cancelled: || false,
    };
    let changed = prepare_rollback_record_delta_v1(
        &root,
        &qualification,
        RollbackRecordRefV1::Authority(&before),
        RollbackRecordRefV1::Authority(&after),
        &mut ctx,
        &mut output,
    )
    .unwrap();
    assert_ne!(*changed, root);
    let read_bytes = baseline - ctx.values.io.read_bytes_remaining;
    assert!(
        read_bytes < 1 << 20,
        "bool update read unchanged identity: {read_bytes}"
    );
    let recovered = recover_rollback_authority_v1(&changed, &mut ctx).unwrap();
    assert_eq!(*recovered, after);
    let mut wrong = before.clone();
    wrong.public_key_hex = "22".repeat(32);
    assert!(
        prepare_rollback_record_delta_v1(
            &root,
            &qualification,
            RollbackRecordRefV1::Authority(&wrong),
            RollbackRecordRefV1::Authority(&after),
            &mut ctx,
            &mut output
        )
        .is_err()
    );
}
