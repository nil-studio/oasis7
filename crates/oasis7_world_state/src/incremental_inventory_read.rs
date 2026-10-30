//! Typed point contents for the three inventory fields; no root authority.
use crate::{
    MaterialLedgerId,
    incremental_field_member::{
        MemberPayloadCodecV1, MemberReadProfileV1, MemberRecordLimitsV1,
        read_member_record_profile_v1,
    },
    incremental_field_presence::{PresenceRecordLimitsV1, read_field_presence_v1},
    incremental_inventory_genesis::descriptor,
    incremental_projection::{FieldDescriptor, FieldShape},
    incremental_semantic_key::{SemanticKeyBudget, SemanticKeyV1, hash_semantic_key_v1},
    incremental_world_field_key::{WorldFieldKeyV1, WorldFieldRecordKindV1},
};
use oasis7_agent_api::world_action::ResourceKind;
use oasis7_formal_protocol::{
    incremental_state::{MerkleNodeReader, StateNamespaceV1, StateRootV1},
    incremental_value::{ValueNodeReader, ValueReadContext, ValueTreeError},
};
use std::cell::{Cell, RefCell};

pub struct InventoryReadContext<'a, M, R, E, C> {
    pub root: &'a StateRootV1,
    pub state: &'a M,
    pub values: ValueReadContext<'a, R>,
    pub key_work: &'a mut SemanticKeyBudget,
    pub semantic_bytes: u64,
    pub record_bytes: u64,
    /// Recover the original typed failure across Merkle's String boundary.
    /// Any outstanding failure aborts the current lookup, even after Ok.
    pub take_state_error: E,
    pub cancelled: C,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LedgerQuantityV1 {
    LedgerAbsent,
    MaterialAbsent,
    Value(i64),
}
pub(crate) struct InventoryPointV1 {
    pub key: WorldFieldKeyV1,
    pub root: Option<[u8; 32]>,
    pub quantity: Option<i64>,
}
pub(crate) struct LedgerPointV1 {
    pub group: WorldFieldKeyV1,
    pub group_root: Option<[u8; 32]>,
    pub member: InventoryPointV1,
}
struct CancelState<'a, M, C> {
    source: &'a M,
    cancelled: RefCell<&'a mut C>,
    seen: Cell<bool>,
}
impl<M: MerkleNodeReader, C: FnMut() -> bool> MerkleNodeReader for CancelState<'_, M, C> {
    type Node = M::Node;
    fn allocation_class(&self) -> Option<oasis7_proto::memory_budget::MemoryClass> {
        self.source.allocation_class()
    }
    fn allocation_budget(&self) -> Option<&oasis7_proto::memory_budget::MemoryBudget> {
        self.source.allocation_budget()
    }
    fn read_node(&self, digest: &str, max_bytes: usize) -> Result<Option<Self::Node>, String> {
        if (self.cancelled.borrow_mut())() {
            self.seen.set(true);
            return Err("inventory cancelled".into());
        }
        let result = self.source.read_node(digest, max_bytes);
        if (self.cancelled.borrow_mut())() {
            self.seen.set(true);
            return Err("inventory cancelled".into());
        }
        result
    }
}
impl<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
> InventoryReadContext<'_, M, R, E, C>
{
    fn check(&mut self) -> Result<(), ValueTreeError> {
        if (self.cancelled)() {
            return Err(ValueTreeError::Cancelled);
        }
        if self.root.namespace != StateNamespaceV1::Business {
            return Err(ValueTreeError::Invalid("inventory namespace"));
        }
        if self.state.allocation_class() != Some(self.values.class)
            || !self
                .state
                .allocation_budget()
                .is_some_and(|pool| pool.same_pool(self.values.memory))
        {
            return Err(ValueTreeError::Invalid("inventory state pool/class"));
        }
        Ok(())
    }
    fn lookup(&mut self, key: &WorldFieldKeyV1) -> Result<Option<[u8; 32]>, ValueTreeError> {
        self.check()?;
        let _workspace = self.values.memory.reserve(512 << 10, self.values.class)?;
        let (result, cancelled) = {
            let source = CancelState {
                source: self.state,
                cancelled: RefCell::new(&mut self.cancelled),
                seen: Cell::new(false),
            };
            let result = self.root.get(key.as_bytes(), &source);
            (result, source.seen.get())
        };
        let original = (self.take_state_error)();
        if cancelled || (self.cancelled)() {
            return Err(ValueTreeError::Cancelled);
        }
        if let Some(error) = original {
            return Err(error);
        }
        result.map_err(|_| ValueTreeError::Invalid("inventory state lookup"))
    }
    fn metadata(
        &mut self,
        name: &'static str,
        shape: FieldShape,
    ) -> Result<FieldDescriptor, ValueTreeError> {
        self.check()?;
        let field = descriptor(name, shape)
            .map_err(|_| ValueTreeError::Invalid("inventory registration"))?;
        let key = WorldFieldKeyV1::new(field, WorldFieldRecordKindV1::Metadata, None)
            .map_err(ValueTreeError::Invalid)?;
        let value = self
            .lookup(&key)?
            .ok_or(ValueTreeError::Invalid("inventory missing field metadata"))?;
        read_field_presence_v1(
            &value,
            &key,
            &mut self.values,
            PresenceRecordLimitsV1 {
                semantic_bytes: 0,
                record_bytes: self.record_bytes,
            },
            &mut std::io::sink(),
            &mut self.cancelled,
        )?;
        Ok(field)
    }
    fn key<K: SemanticKeyV1 + ?Sized>(
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
            &mut self.cancelled,
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
    fn quantity(
        &mut self,
        value: &[u8; 32],
        key: &WorldFieldKeyV1,
        semantic_bytes: u64,
    ) -> Result<i64, ValueTreeError> {
        let mut bytes = [0; 8];
        read_member_record_profile_v1(
            value,
            key,
            &mut self.values,
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
            &mut self.cancelled,
        )?;
        if (self.cancelled)() {
            return Err(ValueTreeError::Cancelled);
        }
        Ok(i64::from_be_bytes(bytes))
    }
    pub(crate) fn simple_point<K: SemanticKeyV1 + ?Sized>(
        &mut self,
        name: &'static str,
        key: &K,
    ) -> Result<InventoryPointV1, ValueTreeError> {
        let field = self.metadata(name, FieldShape::Map)?;
        let (key, bytes) = self.key(field, WorldFieldRecordKindV1::Entry, key)?;
        let root = self.lookup(&key)?;
        let quantity = root
            .map(|value| self.quantity(&value, &key, bytes))
            .transpose()?;
        Ok(InventoryPointV1 {
            key,
            root,
            quantity,
        })
    }
    pub fn resource(&mut self, key: ResourceKind) -> Result<Option<i64>, ValueTreeError> {
        self.simple_point("resources", &key)
            .map(|point| point.quantity)
    }
    pub fn material(&mut self, key: &str) -> Result<Option<i64>, ValueTreeError> {
        self.simple_point("materials", key)
            .map(|point| point.quantity)
    }
    pub fn ledger(
        &mut self,
        ledger: &MaterialLedgerId,
        material: &str,
    ) -> Result<LedgerQuantityV1, ValueTreeError> {
        let point = self.ledger_point(ledger, material)?;
        Ok(match (point.group_root, point.member.quantity) {
            (None, _) => LedgerQuantityV1::LedgerAbsent,
            (Some(_), None) => LedgerQuantityV1::MaterialAbsent,
            (Some(_), Some(value)) => LedgerQuantityV1::Value(value),
        })
    }
    pub(crate) fn ledger_point(
        &mut self,
        ledger: &MaterialLedgerId,
        material: &str,
    ) -> Result<LedgerPointV1, ValueTreeError> {
        let field = self.metadata("material_ledgers", FieldShape::NestedMap)?;
        let (group, group_bytes) = self.key(field, WorldFieldRecordKindV1::Group, ledger)?;
        let group_value = self.lookup(&group)?;
        if let Some(value) = group_value {
            let info = read_field_presence_v1(
                &value,
                &group,
                &mut self.values,
                PresenceRecordLimitsV1 {
                    semantic_bytes: self.semantic_bytes,
                    record_bytes: self.record_bytes,
                },
                &mut std::io::sink(),
                &mut self.cancelled,
            )?;
            if info.semantic_bytes != group_bytes {
                return Err(ValueTreeError::Invalid("inventory group key length"));
            }
        }
        let (key, bytes) = self.key(field, WorldFieldRecordKindV1::Entry, &(ledger, material))?;
        let value = self.lookup(&key)?;
        if group_value.is_none() && value.is_some() {
            return Err(ValueTreeError::Invalid("inventory orphan member"));
        }
        let quantity = value
            .map(|value| self.quantity(&value, &key, bytes))
            .transpose()?;
        Ok(LedgerPointV1 {
            group,
            group_root: group_value,
            member: InventoryPointV1 {
                key,
                root: value,
                quantity,
            },
        })
    }
}
