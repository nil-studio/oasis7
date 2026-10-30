//! Provisional bounded quantity changes, never a business commit/head publish.
use crate::{
    MaterialLedgerId,
    incremental_field_member::{
        MemberPayloadRefV1, MemberRecordError, MemberRecordWriteContext, write_member_record_v1,
    },
    incremental_field_presence::{
        PresenceRecordLimitsV1, PresenceRecordWriteContext, write_field_group_v1,
    },
    incremental_inventory_genesis::{InventoryWriteSink, descriptor},
    incremental_inventory_read::{InventoryReadContext, LedgerQuantityV1},
    incremental_projection::{FieldDescriptor, FieldShape},
    incremental_semantic_key::SemanticKeyV1,
    incremental_world_field_key::{WorldFieldKeyV1, WorldFieldRecordKindV1},
};
#[path = "incremental_inventory_change_binding.rs"]
mod binding;
use binding::WriteBinding;
pub(crate) use binding::write_digest;
use oasis7_agent_api::world_action::ResourceKind;
use oasis7_formal_protocol::{
    incremental_state::{MerkleNodeReader, STATE_ROOT_BYTES, StateWriteV1},
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueTreeError},
};
use oasis7_proto::memory_budget::MemoryClass;

pub enum InventoryChangeRefV1<'a> {
    Resource {
        key: ResourceKind,
        expected: Option<i64>,
        after: Option<i64>,
    },
    Material {
        key: &'a str,
        expected: Option<i64>,
        after: Option<i64>,
    },
    LedgerMaterial {
        ledger: &'a MaterialLedgerId,
        material: &'a str,
        expected: LedgerQuantityV1,
        after: Option<i64>,
        create_ledger: bool,
    },
}
#[derive(Debug)]
pub enum InventoryChangeError {
    Precondition,
    Invalid(&'static str),
    Value(ValueTreeError),
    Record(MemberRecordError),
}
impl From<ValueTreeError> for InventoryChangeError {
    fn from(e: ValueTreeError) -> Self {
        Self::Value(e)
    }
}
impl From<MemberRecordError> for InventoryChangeError {
    fn from(e: MemberRecordError) -> Self {
        Self::Record(e)
    }
}
impl std::fmt::Display for InventoryChangeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Precondition => f.write_str("inventory precondition"),
            Self::Invalid(reason) => f.write_str(reason),
            Self::Value(e) => e.fmt(f),
            Self::Record(e) => e.fmt(f),
        }
    }
}
impl std::error::Error for InventoryChangeError {}
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct InventoryDependencyV1 {
    pub key: WorldFieldKeyV1,
    pub root: Option<[u8; 32]>,
    pub writable: bool,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedInventoryChangeV1 {
    pub(crate) before_root: [u8; STATE_ROOT_BYTES],
    pub(crate) records: u8,
    pub(crate) write_digests: [Option<[u8; 32]>; 2],
    pub(crate) dependencies: [Option<InventoryDependencyV1>; 3],
}
impl PreparedInventoryChangeV1 {
    pub fn before_root(&self) -> &[u8; STATE_ROOT_BYTES] {
        &self.before_root
    }
    pub fn records(&self) -> u8 {
        self.records
    }
}
fn simple_dependencies(
    point: &crate::incremental_inventory_read::InventoryPointV1,
) -> [Option<InventoryDependencyV1>; 3] {
    [
        Some(InventoryDependencyV1 {
            key: point.metadata.clone(),
            root: Some(point.metadata_root),
            writable: false,
        }),
        Some(InventoryDependencyV1 {
            key: point.key.clone(),
            root: point.root,
            writable: true,
        }),
        None,
    ]
}
fn emit<S: InventoryWriteSink>(
    key: &WorldFieldKeyV1,
    before: Option<[u8; 32]>,
    after: Option<[u8; 32]>,
    memory: &oasis7_proto::memory_budget::MemoryBudget,
    sink: &mut S,
) -> Result<(), InventoryChangeError> {
    let charge = memory
        .reserve(1024 + key.as_bytes().len(), MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    sink.put_write(charge.retain(StateWriteV1 {
        key: key.as_bytes().to_vec(),
        before,
        after,
    }))?;
    Ok(())
}
fn member<
    K: SemanticKeyV1 + ?Sized,
    W: ValueNodeWriter,
    C: FnMut() -> bool,
    CR: FnMut() -> bool,
    S: InventoryWriteSink,
>(
    field: FieldDescriptor,
    key: &K,
    point: &crate::incremental_inventory_read::InventoryPointV1,
    after: Option<i64>,
    write: &mut MemberRecordWriteContext<'_, W, C>,
    sink: &mut S,
    cancelled_read: &mut CR,
) -> Result<u8, InventoryChangeError> {
    if cancelled_read() || (write.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    if point.quantity == after {
        return Ok(0);
    }
    let root = if let Some(quantity) = after {
        let record = write_member_record_v1(
            field,
            WorldFieldRecordKindV1::Entry,
            key,
            MemberPayloadRefV1::I64(quantity),
            &mut MemberRecordWriteContext {
                sink: &mut *write.sink,
                memory: write.memory,
                io: &mut *write.io,
                key_work: &mut *write.key_work,
                limits: write.limits,
                cancelled: || cancelled_read() || (write.cancelled)(),
            },
        )?;
        if record.field_key.as_bytes() != point.key.as_bytes() {
            return Err(InventoryChangeError::Invalid(
                "inventory generated member key",
            ));
        }
        Some(record.value_root)
    } else {
        None
    };
    if cancelled_read() || (write.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    emit(&point.key, point.root, root, write.memory, sink)?;
    Ok(1)
}
/// Input is the owner's final point decision, not an additive business command.
/// On any error the caller must discard all emitted writes/provisional pages.
pub fn prepare_inventory_change_v1<M, R, E, C, W, CW, S>(
    change: InventoryChangeRefV1<'_>,
    read: &mut InventoryReadContext<'_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    sink: &mut S,
) -> Result<PreparedInventoryChangeV1, InventoryChangeError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeWriter,
    CW: FnMut() -> bool,
    S: InventoryWriteSink,
{
    if (read.cancelled)() || (write.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    if read.values.class != MemoryClass::Ordinary || !read.values.memory.same_pool(write.memory) {
        return Err(InventoryChangeError::Invalid("inventory change pool/class"));
    }
    let _metadata = write
        .memory
        .reserve(4096, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let before_root = read
        .root
        .canonical_bytes()
        .map_err(|_| InventoryChangeError::Invalid("inventory source root"))?
        .try_into()
        .map_err(|_| InventoryChangeError::Invalid("inventory source root length"))?;
    let mut capture = WriteBinding {
        sink,
        digests: [None, None],
        count: 0,
    };
    let sink = &mut capture;
    let mut dependencies = [None, None, None];
    let records = match change {
        InventoryChangeRefV1::Resource {
            key,
            expected,
            after,
        } => {
            let point = read.simple_point("resources", &key)?;
            if point.quantity != expected {
                return Err(InventoryChangeError::Precondition);
            }
            dependencies = simple_dependencies(&point);
            member(
                descriptor("resources", FieldShape::Map)?,
                &key,
                &point,
                after,
                write,
                sink,
                &mut read.cancelled,
            )?
        }
        InventoryChangeRefV1::Material {
            key,
            expected,
            after,
        } => {
            let point = read.simple_point("materials", key)?;
            if point.quantity != expected {
                return Err(InventoryChangeError::Precondition);
            }
            dependencies = simple_dependencies(&point);
            member(
                descriptor("materials", FieldShape::Map)?,
                key,
                &point,
                after,
                write,
                sink,
                &mut read.cancelled,
            )?
        }
        InventoryChangeRefV1::LedgerMaterial {
            ledger,
            material,
            expected,
            after,
            create_ledger,
        } => {
            let point = read.ledger_point(ledger, material)?;
            let current = match (point.group_root, point.member.quantity) {
                (None, _) => LedgerQuantityV1::LedgerAbsent,
                (Some(_), None) => LedgerQuantityV1::MaterialAbsent,
                (Some(_), Some(value)) => LedgerQuantityV1::Value(value),
            };
            if current != expected {
                return Err(InventoryChangeError::Precondition);
            }
            if point.group_root.is_none() && !create_ledger && after.is_some() {
                return Err(InventoryChangeError::Invalid(
                    "inventory ledger creation not allowed",
                ));
            }
            dependencies[0] = Some(InventoryDependencyV1 {
                key: point.member.metadata.clone(),
                root: Some(point.member.metadata_root),
                writable: false,
            });
            dependencies[1] = Some(InventoryDependencyV1 {
                key: point.group.clone(),
                root: point.group_root,
                writable: point.group_root.is_none() && create_ledger,
            });
            dependencies[2] = Some(InventoryDependencyV1 {
                key: point.member.key.clone(),
                root: point.member.root,
                writable: true,
            });
            let mut records = 0;
            let field = descriptor("material_ledgers", FieldShape::NestedMap)?;
            if point.group_root.is_none() && create_ledger {
                let record = write_field_group_v1(
                    field,
                    ledger,
                    &mut PresenceRecordWriteContext {
                        sink: &mut *write.sink,
                        memory: write.memory,
                        io: &mut *write.io,
                        key_work: &mut *write.key_work,
                        limits: PresenceRecordLimitsV1 {
                            semantic_bytes: write.limits.semantic_bytes,
                            record_bytes: write.limits.record_bytes,
                        },
                        cancelled: || (read.cancelled)() || (write.cancelled)(),
                    },
                )?;
                if record.field_key.as_bytes() != point.group.as_bytes() {
                    return Err(InventoryChangeError::Invalid(
                        "inventory generated group key",
                    ));
                }
                if (read.cancelled)() || (write.cancelled)() {
                    return Err(ValueTreeError::Cancelled.into());
                }
                emit(
                    &point.group,
                    None,
                    Some(record.value_root),
                    write.memory,
                    sink,
                )?;
                records += 1;
            }
            records
                + member(
                    field,
                    &(ledger, material),
                    &point.member,
                    after,
                    write,
                    sink,
                    &mut read.cancelled,
                )?
        }
    };
    if (read.cancelled)() || (write.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    if usize::from(records) != capture.count {
        return Err(InventoryChangeError::Invalid("inventory output count"));
    }
    Ok(PreparedInventoryChangeV1 {
        before_root,
        records,
        dependencies,
        write_digests: capture.digests,
    })
}
