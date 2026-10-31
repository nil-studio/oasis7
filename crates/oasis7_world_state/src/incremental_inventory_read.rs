//! Typed point contents for the three inventory fields; no root authority.
use crate::{
    MaterialLedgerId,
    incremental_inventory_genesis::descriptor,
    incremental_projection::{FieldDescriptor, FieldShape},
    incremental_semantic_key::SemanticKeyV1,
    incremental_world_field_key::{WorldFieldKeyV1, WorldFieldRecordKindV1},
};
use oasis7_agent_api::world_action::ResourceKind;
use oasis7_formal_protocol::{
    incremental_state::MerkleNodeReader,
    incremental_value::{ValueNodeReader, ValueTreeError},
};

pub use crate::incremental_field_read::WorldFieldReadContextV1 as InventoryReadContext;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LedgerQuantityV1 {
    LedgerAbsent,
    MaterialAbsent,
    Value(i64),
}
pub(crate) struct InventoryPointV1 {
    pub metadata: WorldFieldKeyV1,
    pub metadata_root: [u8; 32],
    pub key: WorldFieldKeyV1,
    pub root: Option<[u8; 32]>,
    pub quantity: Option<i64>,
}
pub(crate) struct LedgerPointV1 {
    pub group: WorldFieldKeyV1,
    pub group_root: Option<[u8; 32]>,
    pub member: InventoryPointV1,
}
pub(crate) struct InventoryGroupPointV1 {
    pub key: WorldFieldKeyV1,
    pub root: Option<[u8; 32]>,
}
pub(crate) struct InventoryResolvedPointV1 {
    pub member: InventoryPointV1,
    pub group: Option<InventoryGroupPointV1>,
}
impl InventoryResolvedPointV1 {
    pub(crate) fn simple(member: InventoryPointV1) -> Self {
        Self {
            member,
            group: None,
        }
    }
    pub(crate) fn ledger(point: LedgerPointV1) -> Self {
        Self {
            member: point.member,
            group: Some(InventoryGroupPointV1 {
                key: point.group,
                root: point.group_root,
            }),
        }
    }
    pub(crate) fn quantity(&self) -> LedgerQuantityV1 {
        match &self.group {
            None => self
                .member
                .quantity
                .map_or(LedgerQuantityV1::MaterialAbsent, LedgerQuantityV1::Value),
            Some(group) => match (group.root, self.member.quantity) {
                (None, _) => LedgerQuantityV1::LedgerAbsent,
                (Some(_), None) => LedgerQuantityV1::MaterialAbsent,
                (Some(_), Some(value)) => LedgerQuantityV1::Value(value),
            },
        }
    }
}
impl<
    'v,
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
> InventoryReadContext<'v, M, R, E, C>
{
    fn metadata(
        &mut self,
        name: &'static str,
        shape: FieldShape,
    ) -> Result<(FieldDescriptor, WorldFieldKeyV1, [u8; 32]), ValueTreeError> {
        self.check()?;
        let field = descriptor(name, shape)
            .map_err(|_| ValueTreeError::Invalid("inventory registration"))?;
        let key = WorldFieldKeyV1::new(field, WorldFieldRecordKindV1::Metadata, None)
            .map_err(ValueTreeError::Invalid)?;
        let value = self
            .lookup(&key)?
            .ok_or(ValueTreeError::Invalid("inventory missing field metadata"))?;
        self.records().presence(&value, &key, 0)?;
        Ok((field, key, value))
    }
    pub(crate) fn simple_point<K: SemanticKeyV1 + ?Sized>(
        &mut self,
        name: &'static str,
        key: &K,
    ) -> Result<InventoryPointV1, ValueTreeError> {
        let (field, metadata, metadata_root) = self.metadata(name, FieldShape::Map)?;
        let (key, bytes) = self.key(field, WorldFieldRecordKindV1::Entry, key)?;
        let root = self.lookup(&key)?;
        let quantity = root
            .map(|value| self.quantity(&value, &key, bytes))
            .transpose()?;
        Ok(InventoryPointV1 {
            metadata,
            metadata_root,
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
        let (field, metadata, metadata_root) =
            self.metadata("material_ledgers", FieldShape::NestedMap)?;
        let (group, group_bytes) = self.key(field, WorldFieldRecordKindV1::Group, ledger)?;
        let group_value = self.lookup(&group)?;
        if let Some(value) = group_value {
            self.records().presence(&value, &group, group_bytes)?;
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
                metadata,
                metadata_root,
                key,
                root: value,
                quantity,
            },
        })
    }
}
