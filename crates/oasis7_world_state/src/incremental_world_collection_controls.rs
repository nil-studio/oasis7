//! Complete structural closure of authorization collections and the delivery cursor.
//! These tokens confer no permission, replay, delivery or World-ready authority.
use crate::{
    incremental_field_member::*,
    incremental_field_presence::{PresenceRecordLimitsV1, read_field_presence_v1},
    incremental_field_read::WorldFieldReadContextV1,
    incremental_semantic_key::SEMANTIC_KEY_DOMAIN,
};
use oasis7_formal_protocol::{
    incremental_execution::{RootSetV1, TransitionRecordV1, TransitionVerifier},
    incremental_state::{
        MerkleNodeReader, StateNamespaceV1, StateScanBudget, StateScanContext, StateScanError,
    },
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
#[path = "incremental_world_collection_controls_index.rs"]
mod index;
use index::{
    CollectionControlsGroupMetaV1, CollectionControlsIndexV1, CollectionControlsIndexWorkV1,
};
#[path = "incremental_world_collection_controls_family.rs"]
mod family;
#[path = "incremental_world_collection_controls_receiver.rs"]
mod receiver;
#[path = "incremental_world_collection_controls_semantic.rs"]
mod semantic;
pub use family::{
    qualify_world_collection_controls_v1, readmit_world_collection_controls_ordinary_v1,
};
pub use receiver::accept_world_collection_controls_transition_v1;
pub(crate) use receiver::accept_world_collection_controls_verified_v1;
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("World collection controls closure")
}
pub struct WorldCollectionControlsWorkV1 {
    pub records_remaining: u64,
    pub index_steps_remaining: u64,
    pub max_record_bytes: u64,
}
impl WorldCollectionControlsWorkV1 {
    fn step(&mut self, c: &mut impl FnMut() -> bool) -> Result<(), ValueTreeError> {
        if c() {
            return Err(ValueTreeError::Cancelled);
        }
        self.records_remaining =
            self.records_remaining
                .checked_sub(1)
                .ok_or(ValueTreeError::ResourceLimited(
                    "collection control records",
                ))?;
        Ok(())
    }
}
pub struct WorldCollectionControlsQualificationV1 {
    roots: [u8; 32],
    class: MemoryClass,
    counts: [[u64; 2]; 3],
    cursor_entries: u64,
    groups: CollectionControlsIndexV1,
}
impl WorldCollectionControlsQualificationV1 {
    pub fn field_count(&self) -> usize {
        4
    }
    pub fn allocation_class(&self) -> MemoryClass {
        self.class
    }
    pub fn group_member_counts(&self) -> [[u64; 2]; 3] {
        self.counts
    }
    pub fn cursor_entries(&self) -> u64 {
        self.cursor_entries
    }
    pub fn validate_binding(&self, roots: &RootSetV1) -> Result<(), ValueTreeError> {
        if self.roots != roots.digest().map_err(|_| invalid())? {
            return Err(invalid());
        }
        Ok(())
    }
    fn check(&self, m: &MemoryBudget, class: MemoryClass) -> Result<(), ValueTreeError> {
        if self.class != class || self.groups.class() != class || !self.groups.uses_budget(m) {
            return Err(invalid());
        }
        Ok(())
    }
}
fn descriptor(i: usize) -> Result<FieldDescriptor, ValueTreeError> {
    use crate::incremental_authorization_mapsets::AuthorizationMapSetFieldV1 as F;
    match i {
        0 => F::DataAccessPermissions.descriptor(),
        1 => F::ModuleReleaseRoleBindings.descriptor(),
        2 => crate::incremental_authenticated_nonces::descriptor(),
        3 => crate::incremental_delivery_cursor::descriptor().map_err(|_| invalid()),
        _ => Err(invalid()),
    }
}
fn key(raw: &[u8]) -> Result<Option<(usize, WorldFieldKeyV1)>, ValueTreeError> {
    if !raw.starts_with(b"oasis7.iws.world-field-key.v1\0") {
        return Ok(None);
    }
    let p = WorldFieldKeyV1::from_bytes(raw).map_err(ValueTreeError::Invalid)?;
    let i = match p.name {
        "data_access_permissions" => 0,
        "module_release_role_bindings" => 1,
        "authenticated_collect_data_last_nonces" => 2,
        "product_validation_delivery_cursor" => 3,
        _ => return Ok(None),
    };
    let key = WorldFieldKeyV1::new(descriptor(i)?, p.kind, p.semantic_key)
        .map_err(ValueTreeError::Invalid)?;
    if key.as_bytes() != raw {
        return Err(invalid());
    }
    Ok(Some((i, key)))
}
fn namespace(i: usize) -> StateNamespaceV1 {
    if i == 3 {
        StateNamespaceV1::Scheduler
    } else {
        StateNamespaceV1::Authorization
    }
}
#[derive(Clone, Copy)]
enum Row {
    Meta(usize),
    Group(usize, [u8; 32]),
    Member(usize, [u8; 32]),
    Cursor,
}
fn row<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    i: usize,
    key: &WorldFieldKeyV1,
    value: &[u8; 32],
    inherited: bool,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut WorldCollectionControlsWorkV1,
) -> Result<Row, ValueTreeError> {
    ctx.check()?;
    work.step(&mut ctx.cancelled)?;
    let parsed = WorldFieldKeyV1::from_bytes(key.as_bytes()).map_err(ValueTreeError::Invalid)?;
    let max = work.max_record_bytes.min(ctx.record_bytes);
    if !inherited {
        ctx.values.verify_closure(value, max, &mut ctx.cancelled)?;
    }
    if parsed.kind == WorldFieldRecordKindV1::Metadata {
        ctx.records().presence(value, key, 0)?;
        return Ok(Row::Meta(i));
    }
    let shape = match (i, parsed.kind) {
        (0..=2, WorldFieldRecordKindV1::Group) => semantic::Shape::Group,
        (0..=2, WorldFieldRecordKindV1::Entry) => semantic::Shape::Member,
        (3, WorldFieldRecordKindV1::Entry) => semantic::Shape::Unit,
        _ => return Err(invalid()),
    };
    let mut probe = semantic::Probe::new(shape);
    let limit = if inherited {
        max
    } else {
        max.min(ctx.semantic_bytes)
    };
    if parsed.kind == WorldFieldRecordKindV1::Group {
        read_field_presence_v1(
            value,
            key,
            &mut ctx.values,
            PresenceRecordLimitsV1 {
                semantic_bytes: limit,
                record_bytes: max,
            },
            &mut probe,
            &mut ctx.cancelled,
        )?;
    } else {
        let limits = MemberRecordLimitsV1 {
            semantic_bytes: limit,
            payload_bytes: 16,
            record_bytes: max,
        };
        let info = if inherited {
            read_member_record_key_v1(
                value,
                key,
                &mut ctx.values,
                limits,
                &mut probe,
                &mut ctx.cancelled,
            )?
        } else {
            read_member_record_v1(
                value,
                key,
                &mut ctx.values,
                limits,
                &mut probe,
                &mut std::io::sink(),
                &mut ctx.cancelled,
            )?
        };
        let expected = match i {
            0 | 1 => (MemberPayloadCodecV1::Unit, 0),
            2 => (MemberPayloadCodecV1::U64, 8),
            3 => (MemberPayloadCodecV1::DeliveryCursor, 16),
            _ => return Err(invalid()),
        };
        if (info.codec, info.payload_bytes) != expected {
            return Err(invalid());
        }
    }
    let group = probe.finish()?;
    if i == 3 {
        ctx.check()?;
        return Ok(Row::Cursor);
    }
    let scope = WorldFieldKeyV1::new(descriptor(i)?, WorldFieldRecordKindV1::Group, Some(group))
        .map_err(ValueTreeError::Invalid)?;
    let digest = *blake3::hash(scope.as_bytes()).as_bytes();
    ctx.check()?;
    Ok(if parsed.kind == WorldFieldRecordKindV1::Group {
        if parsed.semantic_key != Some(group) {
            return Err(invalid());
        }
        Row::Group(i, digest)
    } else {
        Row::Member(i, digest)
    })
}
fn adjust(
    index: &mut CollectionControlsIndexV1,
    row: Row,
    add: bool,
    counts: &mut [[u64; 2]; 3],
    cursor: &mut u64,
    work: &mut CollectionControlsIndexWorkV1,
    c: &mut impl FnMut() -> bool,
) -> Result<Option<[u8; 32]>, ValueTreeError> {
    let update = |n: &mut u64| -> Result<(), ValueTreeError> {
        *n = if add {
            n.checked_add(1)
        } else {
            n.checked_sub(1)
        }
        .ok_or_else(invalid)?;
        Ok(())
    };
    let (i, key, is_group) = match row {
        Row::Meta(_) => return Ok(None),
        Row::Cursor => {
            update(cursor)?;
            return Ok(None);
        }
        Row::Group(i, k) => (i, k, true),
        Row::Member(i, k) => (i, k, false),
    };
    update(&mut counts[i][usize::from(!is_group)])?;
    let mut meta = index
        .get(key, work, c)?
        .unwrap_or(CollectionControlsGroupMetaV1 {
            key,
            present: false,
            members: 0,
        });
    if is_group {
        if meta.present == add {
            return Err(invalid());
        }
        meta.present = add;
    } else {
        update(&mut meta.members)?;
    }
    *index = index.insert(meta, work, c)?;
    Ok(Some(key))
}
fn final_counts(
    groups: &CollectionControlsIndexV1,
    counts: &[[u64; 2]; 3],
    cursor: u64,
) -> Result<(), ValueTreeError> {
    let n = counts
        .iter()
        .try_fold(0u64, |n, c| n.checked_add(c[0]).ok_or_else(invalid))?;
    if cursor != 1 || u64::try_from(groups.len()).map_err(|_| invalid())? != n {
        return Err(invalid());
    }
    Ok(())
}

#[cfg(test)]
#[path = "incremental_world_collection_controls_tests.rs"]
mod tests;

#[path = "incremental_world_collection_controls_current_group.rs"]
mod current_group;
pub use current_group::{
    DataAccessMembershipChangeV1, change_current_data_access_membership_v1,
    read_current_data_access_group_size_v1,
};
