//! Original authorization MapSet identities; no permission or role authority.
use crate::{
    incremental_field_member::*, incremental_field_presence::*,
    incremental_field_read::WorldFieldReadContextV1, incremental_projection::WORLD_STATE_FIELDS,
    state::WorldState,
};
use oasis7_formal_protocol::{
    incremental_execution::ExecutionWriteV1,
    incremental_state::{MerkleNodeReader, StateNamespaceV1},
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueTreeError},
};
use oasis7_proto::memory_budget::ReservedValue;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthorizationMembershipV1 {
    /// Only the queried Group marker is absent; complete-family qualification is separate.
    GroupAbsent,
    MemberAbsent,
    Present,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AuthorizationMapSetsGenesisV1 {
    pub groups: u64,
    pub members: u64,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthorizationMapSetFieldV1 {
    DataAccessPermissions,
    ModuleReleaseRoleBindings,
}
impl AuthorizationMapSetFieldV1 {
    pub const ALL: [Self; 2] = [Self::DataAccessPermissions, Self::ModuleReleaseRoleBindings];
    pub fn name(self) -> &'static str {
        match self {
            Self::DataAccessPermissions => "data_access_permissions",
            Self::ModuleReleaseRoleBindings => "module_release_role_bindings",
        }
    }
    fn descriptor(self) -> Result<FieldDescriptor, ValueTreeError> {
        WORLD_STATE_FIELDS
            .iter()
            .copied()
            .find(|field| {
                field.name == self.name()
                    && field.owner == FieldOwner::WorldState
                    && field.shape == FieldShape::MapSet
                    && field.destination == FieldDestination::State(StateNamespaceV1::Authorization)
            })
            .ok_or(ValueTreeError::Invalid("authorization MapSet registration"))
    }
}
fn start<W, C: FnMut() -> bool>(
    context: &mut MemberRecordWriteContext<'_, W, C>,
    records: &mut u64,
) -> Result<(), MemberRecordError> {
    if (context.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    if *records == 0 {
        return Err(ValueTreeError::ResourceLimited("authorization MapSet records").into());
    }
    *records -= 1;
    Ok(())
}
fn presence<
    W: ValueNodeWriter,
    C: FnMut() -> bool,
    S: FnMut(ReservedValue<ExecutionWriteV1>) -> Result<(), ValueTreeError>,
>(
    field: FieldDescriptor,
    subject: Option<&str>,
    context: &mut MemberRecordWriteContext<'_, W, C>,
    records: &mut u64,
    sink: &mut S,
) -> Result<(), MemberRecordError> {
    start(context, records)?;
    let mut write = PresenceRecordWriteContext {
        sink: &mut *context.sink,
        memory: context.memory,
        io: &mut *context.io,
        key_work: &mut *context.key_work,
        limits: PresenceRecordLimitsV1 {
            semantic_bytes: context.limits.semantic_bytes,
            record_bytes: context.limits.record_bytes,
        },
        cancelled: &mut context.cancelled,
    };
    let record = if let Some(subject) = subject {
        write_field_group_v1(field, subject, &mut write)?
    } else {
        write_field_metadata_v1(field, &mut write)?
    };
    crate::incremental_field_write::emit(record.field_key, record.value_root, context, sink)
}
/// Caller admits source and batch storage, discarding every prefix on error.
pub fn prepare_authorization_mapsets_genesis_v1<
    W: ValueNodeWriter,
    C: FnMut() -> bool,
    S: FnMut(ReservedValue<ExecutionWriteV1>) -> Result<(), ValueTreeError>,
>(
    source: &WorldState,
    context: &mut MemberRecordWriteContext<'_, W, C>,
    records: &mut u64,
    sink: &mut S,
) -> Result<AuthorizationMapSetsGenesisV1, MemberRecordError> {
    let mut summary = AuthorizationMapSetsGenesisV1 {
        groups: 0,
        members: 0,
    };
    for kind in AuthorizationMapSetFieldV1::ALL {
        let field = kind.descriptor()?;
        presence(field, None, context, records, sink)?;
        let groups = match kind {
            AuthorizationMapSetFieldV1::DataAccessPermissions => &source.data_access_permissions,
            AuthorizationMapSetFieldV1::ModuleReleaseRoleBindings => {
                &source.module_release_role_bindings
            }
        };
        for (subject, members) in groups {
            presence(field, Some(subject), context, records, sink)?;
            summary.groups = summary
                .groups
                .checked_add(1)
                .ok_or(MemberRecordError::Invalid(
                    "authorization MapSet group count",
                ))?;
            for member in members {
                start(context, records)?;
                let record = write_member_record_v1(
                    field,
                    WorldFieldRecordKindV1::Entry,
                    &(subject.as_str(), member.as_str()),
                    MemberPayloadRefV1::Unit,
                    context,
                )?;
                crate::incremental_field_write::emit(
                    record.field_key,
                    record.value_root,
                    context,
                    sink,
                )?;
                summary.members =
                    summary
                        .members
                        .checked_add(1)
                        .ok_or(MemberRecordError::Invalid(
                            "authorization MapSet member count",
                        ))?;
            }
        }
    }
    Ok(summary)
}

pub fn read_authorization_membership_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    kind: AuthorizationMapSetFieldV1,
    subject: &str,
    member: &str,
    context: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<AuthorizationMembershipV1, ValueTreeError> {
    context.check()?;
    let field = kind.descriptor()?;
    let metadata = WorldFieldKeyV1::new(field, WorldFieldRecordKindV1::Metadata, None)
        .map_err(ValueTreeError::Invalid)?;
    let root = context.lookup(&metadata)?.ok_or(ValueTreeError::Invalid(
        "authorization MapSet missing metadata",
    ))?;
    context.records().presence(&root, &metadata, 0)?;
    let (group, group_bytes) = context.key(field, WorldFieldRecordKindV1::Group, subject)?;
    let group_root = context.lookup(&group)?;
    if let Some(root) = group_root {
        context.records().presence(&root, &group, group_bytes)?;
    }
    let (key, key_bytes) = context.key(field, WorldFieldRecordKindV1::Entry, &(subject, member))?;
    let root = context.lookup(&key)?;
    if group_root.is_none() && root.is_some() {
        return Err(ValueTreeError::Invalid("authorization MapSet orphan key"));
    }
    if let Some(root) = root {
        let _: [u8; 0] =
            context
                .records()
                .fixed(&root, &key, key_bytes, MemberPayloadCodecV1::Unit)?;
        Ok(AuthorizationMembershipV1::Present)
    } else if group_root.is_some() {
        Ok(AuthorizationMembershipV1::MemberAbsent)
    } else {
        Ok(AuthorizationMembershipV1::GroupAbsent)
    }
}

#[path = "incremental_authorization_mapsets_overlay.rs"]
mod overlay;
pub use overlay::*;
