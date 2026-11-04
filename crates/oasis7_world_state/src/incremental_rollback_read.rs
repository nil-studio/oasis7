//! Bounded authenticated rollback child point reads; not full family qualification.
use crate::{
    incremental_field_member::*,
    incremental_inventory_value_read::InventoryValueReader,
    incremental_record_tree,
    incremental_rollback_genesis::{LABELS, RollbackWorkV1, descriptor},
    incremental_rollback_records::RollbackPathV1,
    incremental_semantic_key::SemanticKeyBudget,
    incremental_world_field_key::{WorldFieldKeyV1, WorldFieldRecordKindV1},
};
use oasis7_formal_protocol::{
    incremental_record_root::{RECORD_ROOT_BYTES, RecordRootRefV1},
    incremental_state::{StateNamespaceV1, StateRootV1},
    incremental_value::{ValueNodeReader, ValueReadContext, ValueTreeError},
};
use std::io::{Cursor, Write};
macro_rules! with_key {
    ($part:expr,$key:ident=>$body:expr) => {
        match $part {
            RollbackPathV1::Column(name) => {
                let $key = &("column", name);
                $body
            }
            RollbackPathV1::Ordinal(ordinal) => {
                let $key = &("ordinal", ordinal);
                $body
            }
        }
    };
}
/// Caller retains output reservation and chooses the closed expected codec.
/// Every intermediate root is bound to its exact semantic component and schema.
pub fn read_rollback_cell_v1<R: ValueNodeReader, C: FnMut() -> bool>(
    root: &StateRootV1,
    schema: [u8; 32],
    path: &[RollbackPathV1],
    codec: MemberPayloadCodecV1,
    values: &mut ValueReadContext<'_, R>,
    key_work: &mut SemanticKeyBudget,
    work: &mut RollbackWorkV1,
    max_payload_bytes: u64,
    max_record_bytes: u64,
    output: &mut impl Write,
    cancelled: &mut C,
) -> Result<MemberRecordInfoV1, ValueTreeError> {
    if path.is_empty() || path.len() > 4 || root.namespace != StateNamespaceV1::Authorization {
        return Err(ValueTreeError::Invalid("rollback path/root"));
    }
    let _charge = values.memory.reserve(8192, values.class)?;
    let mut root = root.clone();
    let mut reader = InventoryValueReader {
        values,
        key_work,
        semantic_bytes: max_record_bytes,
        record_bytes: max_record_bytes,
        cancelled,
    };
    for (position, part) in path.iter().enumerate() {
        if (reader.cancelled)() {
            return Err(ValueTreeError::Cancelled);
        }
        work.cells_remaining = work
            .cells_remaining
            .checked_sub(1)
            .ok_or(ValueTreeError::ResourceLimited("rollback read cells"))?;
        let key = with_key!(*part,key=>reader.key(descriptor(),WorldFieldRecordKindV1::Entry,key))?;
        let id = incremental_record_tree::lookup(&root, &key.0, &mut reader, LABELS)?
            .ok_or(ValueTreeError::Invalid("rollback missing cell"))?;
        let last = position + 1 == path.len();
        let profile = MemberReadProfileV1 {
            codec: if last {
                codec
            } else {
                MemberPayloadCodecV1::RecordRoot
            },
            semantic_bytes: key.1,
            limits: MemberRecordLimitsV1 {
                semantic_bytes: max_record_bytes,
                payload_bytes: if last {
                    max_payload_bytes
                } else {
                    RECORD_ROOT_BYTES as u64
                },
                record_bytes: max_record_bytes,
            },
        };
        if last {
            return read_member_record_profile_v1(
                &id,
                &key.0,
                reader.values,
                profile,
                &mut std::io::sink(),
                output,
                &mut *reader.cancelled,
            );
        }
        let mut bytes = [0; RECORD_ROOT_BYTES];
        read_member_record_profile_v1(
            &id,
            &key.0,
            reader.values,
            profile,
            &mut std::io::sink(),
            &mut Cursor::new(bytes.as_mut_slice()),
            &mut *reader.cancelled,
        )?;
        let binding =
            WorldFieldKeyV1::from_bytes(key.0.as_bytes()).map_err(ValueTreeError::Invalid)?;
        let reference = RecordRootRefV1::from_canonical_bytes(&bytes)
            .map_err(|_| ValueTreeError::Invalid("rollback root reference"))?;
        root = reference
            .validate_binding(
                &schema,
                &binding
                    .semantic_key
                    .ok_or(ValueTreeError::Invalid("rollback semantic binding"))?,
                StateNamespaceV1::Authorization,
            )
            .map_err(|_| ValueTreeError::Invalid("rollback child binding"))?
            .clone();
    }
    Err(ValueTreeError::Invalid("rollback missing terminal"))
}
