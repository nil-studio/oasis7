use crate::{
    incremental_field_member::{
        MemberPayloadCodecV1, MemberReadProfileV1, MemberRecordLimitsV1,
        read_member_record_profile_v1,
    },
    incremental_field_presence::{PresenceRecordLimitsV1, read_field_presence_v1},
    incremental_projection::FieldDescriptor,
    incremental_semantic_key::{SemanticKeyBudget, SemanticKeyV1, hash_semantic_key_v1},
    incremental_world_field_key::{WorldFieldKeyV1, WorldFieldRecordKindV1},
};
use oasis7_formal_protocol::incremental_value::{
    ValueNodeReader, ValueReadContext, ValueTreeError,
};

pub(crate) struct InventoryValueReader<'a, 'v, R, C> {
    pub values: &'a mut ValueReadContext<'v, R>,
    pub key_work: &'a mut SemanticKeyBudget,
    pub semantic_bytes: u64,
    pub record_bytes: u64,
    pub cancelled: &'a mut C,
}
impl<R: ValueNodeReader, C: FnMut() -> bool> InventoryValueReader<'_, '_, R, C> {
    pub(crate) fn key<K: SemanticKeyV1 + ?Sized>(
        &mut self,
        field: FieldDescriptor,
        kind: WorldFieldRecordKindV1,
        key: &K,
    ) -> Result<(WorldFieldKeyV1, u64), ValueTreeError> {
        let semantic = hash_semantic_key_v1(
            key,
            self.values.memory,
            self.values.class,
            self.key_work,
            &mut *self.cancelled,
        )
        .map_err(|error| match error {
            crate::incremental_semantic_key::SemanticKeyError::Memory(error) => {
                ValueTreeError::Memory(error)
            }
            crate::incremental_semantic_key::SemanticKeyError::ResourceLimited => {
                ValueTreeError::ResourceLimited("inventory key work")
            }
            crate::incremental_semantic_key::SemanticKeyError::Cancelled => {
                ValueTreeError::Cancelled
            }
            crate::incremental_semantic_key::SemanticKeyError::Invalid(reason) => {
                ValueTreeError::Invalid(reason)
            }
        })?;
        if semantic.encoded_bytes > self.semantic_bytes {
            return Err(ValueTreeError::ResourceLimited("inventory semantic length"));
        }
        Ok((
            WorldFieldKeyV1::new(field, kind, Some(semantic.digest))
                .map_err(ValueTreeError::Invalid)?,
            semantic.encoded_bytes,
        ))
    }
    pub(crate) fn quantity(
        &mut self,
        value: &[u8; 32],
        key: &WorldFieldKeyV1,
        semantic_bytes: u64,
    ) -> Result<i64, ValueTreeError> {
        let mut bytes = [0; 8];
        read_member_record_profile_v1(
            value,
            key,
            &mut *self.values,
            MemberReadProfileV1 {
                codec: MemberPayloadCodecV1::I64,
                semantic_bytes,
                limits: MemberRecordLimitsV1 {
                    semantic_bytes: self.semantic_bytes,
                    payload_bytes: 8,
                    record_bytes: self.record_bytes,
                },
            },
            &mut std::io::sink(),
            &mut &mut bytes[..],
            &mut *self.cancelled,
        )?;
        if (self.cancelled)() {
            return Err(ValueTreeError::Cancelled);
        }
        Ok(i64::from_be_bytes(bytes))
    }
    pub(crate) fn presence(
        &mut self,
        value: &[u8; 32],
        key: &WorldFieldKeyV1,
        semantic_bytes: u64,
    ) -> Result<(), ValueTreeError> {
        let info = read_field_presence_v1(
            value,
            key,
            &mut *self.values,
            PresenceRecordLimitsV1 {
                semantic_bytes: if semantic_bytes == 0 {
                    0
                } else {
                    self.semantic_bytes
                },
                record_bytes: self.record_bytes,
            },
            &mut std::io::sink(),
            &mut *self.cancelled,
        )?;
        if info.semantic_bytes != semantic_bytes {
            return Err(ValueTreeError::Invalid("inventory presence key length"));
        }
        Ok(())
    }
}
