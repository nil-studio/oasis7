//! Point set operations in one qualified Case; no unrelated grant/Text scan.
use super::*;
use crate::{
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
    incremental_inventory_value_read::InventoryValueReader,
};
use oasis7_agent_api::world_action::OrganizationEstateScopeV1;
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFence, PreparedGuard},
    incremental_state::{StatePageWriter, StateWriteV1},
};
use oasis7_proto::memory_budget::ReservedValue;
#[derive(Clone, Copy)]
pub enum OrganizationRelativeSetKeyV1<'a> {
    Member(&'a str),
    Scope {
        grant: &'a str,
        scope: &'a OrganizationEstateScopeV1,
    },
}
impl<'a> OrganizationRelativeSetKeyV1<'a> {
    fn paths(self) -> (K<'a>, RecordPathV1<'a>, &'static str, RecordPathV1<'a>) {
        match self {
            Self::Member(member) => (
                K::OrganizationCaseInner,
                RecordPathV1::Record,
                "members",
                RecordPathV1::Member("members", member),
            ),
            Self::Scope { grant, scope } => (
                K::OrganizationGrantInner(grant),
                RecordPathV1::Record,
                "scopes",
                super::super::members::scope_path(scope),
            ),
        }
    }
}
pub struct OrganizationSetRequestV1<'a> {
    pub case: &'a str,
    pub key: OrganizationRelativeSetKeyV1<'a>,
    pub qualification: &'a ReservedValue<OrganizationRecordQualificationV1>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrganizationSetReadV1 {
    GrantAbsent,
    Absent,
    Present,
}
/// The context root is the qualified inner Case root, not the outer Auth root.
pub fn read_qualified_organization_set_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    request: OrganizationSetRequestV1<'_>,
    context: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<OrganizationSetReadV1, ValueTreeError> {
    fields_read::check_request(
        &OrganizationFieldRequestV1 {
            owner: OrganizationRootKeyV1::Case(request.case),
            record: OrganizationRelativeRecordV1::Case,
            field: OrganizationScalarFieldV1::Id,
            qualification: request.qualification,
        },
        context,
    )?;
    let (record, marker, collection, path) = request.key.paths();
    for dependency in [marker, RecordPathV1::Collection(collection)] {
        let key = record.fixed_path(dependency, context)?;
        let Some(root) = context.lookup(&key.0)? else {
            if matches!(dependency, RecordPathV1::Record)
                && matches!(request.key, OrganizationRelativeSetKeyV1::Scope { .. })
            {
                context.check()?;
                return Ok(OrganizationSetReadV1::GrantAbsent);
            }
            return Err(ValueTreeError::Invalid(
                "organization set missing owner marker",
            ));
        };
        super::super::marker(
            &root,
            &key.0,
            key.1,
            &mut context.values,
            context.record_bytes,
            &mut context.cancelled,
        )?;
    }
    let key = record.fixed_path(path, context)?;
    let value = if let Some(root) = context.lookup(&key.0)? {
        super::super::marker(
            &root,
            &key.0,
            key.1,
            &mut context.values,
            context.record_bytes,
            &mut context.cancelled,
        )?;
        OrganizationSetReadV1::Present
    } else {
        OrganizationSetReadV1::Absent
    };
    context.check()?;
    Ok(value)
}
pub struct OrganizationSetChangeV1<'a> {
    pub request: OrganizationSetRequestV1<'a>,
    pub expected: OrganizationSetReadV1,
    pub after: bool,
    pub max_pending_node_bytes: usize,
}
pub fn change_qualified_organization_set_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: StatePageWriter,
    CW: FnMut() -> bool,
>(
    change: OrganizationSetChangeV1<'_>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<Option<ReservedValue<OrganizationRecordQualificationV1>>, WorldFieldOverlayErrorV1> {
    let owner = OrganizationRootKeyV1::Case(change.request.case);
    let cancel = &mut context.cancelled;
    let error = &mut context.take_state_error;
    context.transaction.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        if !tx.uses_budget(write.memory) {
            return Err(ValueTreeError::Invalid("organization set writer pool").into());
        }
        let _operation = write
            .memory
            .reserve(16 << 10, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let (record, marker, collection, path) = change.request.key.paths();
        let (outer, child, key, before, current) = {
            let mut combined = || cancel() || (write.cancelled)();
            let mut records = InventoryValueReader {
                values: &mut context.values,
                key_work: context.key_work,
                semantic_bytes: context.semantic_bytes,
                record_bytes: context.record_bytes,
                cancelled: &mut combined,
            };
            let (outer, child) =
                access::current(owner, change.request.qualification, tx, &mut records, error)?;
            let marker_key = record.overlay_path(marker, &mut records)?;
            let marker_root = inner_tree::lookup(child, &marker_key.0, &mut records)?;
            let (key, before, current) = if let Some(root) = marker_root {
                super::super::marker(
                    &root,
                    &marker_key.0,
                    marker_key.1,
                    records.values,
                    records.record_bytes,
                    records.cancelled,
                )?;
                let meta =
                    record.overlay_path(RecordPathV1::Collection(collection), &mut records)?;
                let root = inner_tree::lookup(child, &meta.0, &mut records)?.ok_or(
                    ValueTreeError::Invalid("organization set collection marker"),
                )?;
                super::super::marker(
                    &root,
                    &meta.0,
                    meta.1,
                    records.values,
                    records.record_bytes,
                    records.cancelled,
                )?;
                let key = record.overlay_path(path, &mut records)?;
                let before = inner_tree::lookup(child, &key.0, &mut records)?;
                if let Some(root) = before {
                    super::super::marker(
                        &root,
                        &key.0,
                        key.1,
                        records.values,
                        records.record_bytes,
                        records.cancelled,
                    )?;
                }
                let current = if before.is_some() {
                    OrganizationSetReadV1::Present
                } else {
                    OrganizationSetReadV1::Absent
                };
                (Some(key), before, current)
            } else if matches!(
                change.request.key,
                OrganizationRelativeSetKeyV1::Scope { .. }
            ) {
                (None, None, OrganizationSetReadV1::GrantAbsent)
            } else {
                return Err(ValueTreeError::Invalid("organization set case marker").into());
            };
            (outer, child, key, before, current)
        };
        if current != change.expected
            || (current == OrganizationSetReadV1::GrantAbsent && change.after)
        {
            return Err(ValueTreeError::Invalid("organization set expected/owner").into());
        }
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        if (current == OrganizationSetReadV1::Present) == change.after {
            return Ok(None);
        }
        let key = key.ok_or(ValueTreeError::Invalid("organization set target"))?;
        let after = if change.after {
            let mut combined = MemberRecordWriteContext {
                sink: &mut *write.sink,
                memory: write.memory,
                io: &mut *write.io,
                key_work: &mut *write.key_work,
                limits: write.limits,
                cancelled: || cancel() || (write.cancelled)(),
            };
            let row = record.write_path(path, MemberPayloadRefV1::Unit, &mut combined)?;
            if row.field_key != key.0 {
                return Err(ValueTreeError::Invalid("organization set target identity").into());
            }
            Some(row.value_root)
        } else {
            None
        };
        let delta = {
            let mut combined = || cancel() || (write.cancelled)();
            let mut records = InventoryValueReader {
                values: &mut context.values,
                key_work: context.key_work,
                semantic_bytes: context.semantic_bytes,
                record_bytes: context.record_bytes,
                cancelled: &mut combined,
            };
            inner_tree::build(
                child,
                vec![StateWriteV1 {
                    key: key.0.as_bytes().to_vec(),
                    before,
                    after,
                }],
                &mut records,
                change.max_pending_node_bytes,
            )?
        };
        let row = {
            let mut combined = MemberRecordWriteContext {
                sink: &mut *write.sink,
                memory: write.memory,
                io: &mut *write.io,
                key_work: &mut *write.key_work,
                limits: write.limits,
                cancelled: || cancel() || (write.cancelled)(),
            };
            access::store(owner, &outer, &delta, &mut combined)?
        };
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        tx.write(
            StateNamespaceV1::Authorization,
            outer.as_bytes(),
            Some(row.value_root),
        )?;
        let digest = WorldFieldKeyV1::from_bytes(outer.as_bytes())
            .map_err(ValueTreeError::Invalid)?
            .semantic_key
            .ok_or(ValueTreeError::Invalid("organization set owner digest"))?;
        let proof = closure::from_typed(owner, digest, &delta.after, write.memory)?;
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(Some(proof))
    })
}
