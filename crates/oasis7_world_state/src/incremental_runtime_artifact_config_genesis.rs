use super::*;
use crate::{
    incremental_field_member::*,
    incremental_field_presence::*,
    incremental_json_records::{JsonRecordProfileV1, JsonWriteWorkV1, prepare_json_subtree_v1},
    incremental_record_tree::{self as tree, RecordTreeLabelsV1, RecordTreeWritePolicyV1},
    incremental_semantic_key::{SemanticKeyV1, hash_semantic_key_v1},
};
use oasis7_formal_protocol::{
    incremental_execution::ExecutionWriteV1,
    incremental_record_root::RecordRootRefV1,
    incremental_state::{StateNamespaceV1, StatePageWriter, StateRootV1},
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};

pub struct ArtifactConfigWriteWorkV1 {
    pub cells_remaining: u64,
    pub json: JsonWriteWorkV1,
    pub max_pending_node_bytes: usize,
}
impl ArtifactConfigWriteWorkV1 {
    fn cell(&mut self) -> Result<(), MemberRecordError> {
        self.cells_remaining = self
            .cells_remaining
            .checked_sub(1)
            .ok_or(ValueTreeError::ResourceLimited("artifact config cells"))?;
        Ok(())
    }
}
pub(super) const LABELS: RecordTreeLabelsV1 = RecordTreeLabelsV1 {
    read: "artifact config read",
    pending: "artifact config pending",
    tree: "artifact config tree",
    digest: "artifact config digest",
    read_allowance: "artifact config read allowance",
    admission: "artifact config admission",
    bound: "artifact config bound",
    producer_pool: "artifact config pool",
    page: "artifact config page",
    write_allowance: "artifact config write allowance",
};
pub(super) fn body_descriptor() -> FieldDescriptor {
    FieldDescriptor {
        owner: FieldOwner::Runtime,
        name: "runtime_artifact_config_body_v1",
        shape: FieldShape::Map,
        destination: FieldDestination::State(StateNamespaceV1::Artifacts),
    }
}
pub(super) fn json_profile() -> JsonRecordProfileV1 {
    JsonRecordProfileV1 {
        field: FieldDescriptor {
            name: "runtime_artifact_config_json_v1",
            ..body_descriptor()
        },
        schema: *blake3::hash(b"oasis7.iws.runtime-artifact-config-json.v1\0").as_bytes(),
    }
}
fn metadata<W: ValueNodeWriter, C: FnMut() -> bool>(
    field: FieldDescriptor,
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
) -> Result<PresenceRecordV1, MemberRecordError> {
    write_field_metadata_v1(
        field,
        &mut PresenceRecordWriteContext {
            sink: &mut *ctx.sink,
            memory: ctx.memory,
            io: &mut *ctx.io,
            key_work: &mut *ctx.key_work,
            limits: PresenceRecordLimitsV1 {
                semantic_bytes: ctx.limits.semantic_bytes,
                record_bytes: ctx.limits.record_bytes,
            },
            cancelled: &mut ctx.cancelled,
        },
    )
}
pub(super) fn row<K: SemanticKeyV1 + ?Sized, W: ValueNodeWriter, C: FnMut() -> bool>(
    field: FieldDescriptor,
    key: &K,
    value: MemberPayloadRefV1<'_>,
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
) -> Result<MemberRecordV1, MemberRecordError> {
    write_member_record_v1(field, WorldFieldRecordKindV1::Entry, key, value, ctx)
}
pub(super) fn root_row<K: SemanticKeyV1 + ?Sized, W: ValueNodeWriter, C: FnMut() -> bool>(
    field: FieldDescriptor,
    key: &K,
    schema: [u8; 32],
    child: StateRootV1,
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
) -> Result<MemberRecordV1, MemberRecordError> {
    let _scratch = ctx
        .memory
        .reserve(8192, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let semantic = hash_semantic_key_v1(
        key,
        ctx.memory,
        MemoryClass::Ordinary,
        ctx.key_work,
        &mut ctx.cancelled,
    )
    .map_err(MemberRecordError::Key)?;
    let reference = RecordRootRefV1::new(schema, semantic.digest, child)
        .map_err(|_| MemberRecordError::Invalid("artifact config root binding"))?;
    row(field, key, MemberPayloadRefV1::RecordRoot(&reference), ctx)
}
pub enum Body<'a> {
    Manifest(&'a crate::Manifest),
    Module(&'a ModuleRecord),
    Active(&'a str),
    Limits(&'a ModuleLimits),
    Security(ArtifactSecurityPolicyRefV1),
}
impl Body<'_> {
    pub(super) fn kind(&self) -> schema::BodyKind {
        match self {
            Self::Manifest(_) => schema::BodyKind::Manifest,
            Self::Module(_) => schema::BodyKind::Module,
            Self::Active(_) => schema::BodyKind::Active,
            Self::Limits(_) => schema::BodyKind::Limits,
            Self::Security(_) => schema::BodyKind::Security,
        }
    }
}
pub(super) fn body<W: StatePageWriter + ValueNodeReader, C: FnMut() -> bool>(
    source: Body<'_>,
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut ArtifactConfigWriteWorkV1,
) -> Result<StateRootV1, MemberRecordError> {
    let policy = RecordTreeWritePolicyV1 {
        max_pending_node_bytes: work.max_pending_node_bytes,
        labels: LABELS,
    };
    let mut root = StateRootV1::empty(StateNamespaceV1::Artifacts)
        .map_err(|_| MemberRecordError::Invalid("artifact config namespace"))?;
    work.cell()?;
    let meta = metadata(body_descriptor(), ctx)?;
    root = tree::append(root, &meta.field_key, meta.value_root, ctx, policy)?;
    work.cell()?;
    let kind = source.kind();
    let marker = row(
        body_descriptor(),
        &(),
        MemberPayloadRefV1::Text(kind.label()),
        ctx,
    )?;
    root = tree::append(root, &marker.field_key, marker.value_root, ctx, policy)?;
    let mut emit = |address: ArtifactConfigAddressV1<'_>,
                    payload: ArtifactConfigPayloadV1<'_>|
     -> Result<(), MemberRecordError> {
        schema::validate(kind, address, payload)?;
        work.cell()?;
        // Presence bits prevent None aliasing ordinal zero or the empty key.
        let key = address.semantic();
        let record = match payload {
            ArtifactConfigPayloadV1::Primitive(value) => row(body_descriptor(), &key, value, ctx)?,
            ArtifactConfigPayloadV1::Json(value) => {
                let child = prepare_json_subtree_v1(json_profile(), value, ctx, &mut work.json)?;
                root_row(body_descriptor(), &key, json_profile().schema, child, ctx)?
            }
        };
        root = tree::append(
            root.clone(),
            &record.field_key,
            record.value_root,
            ctx,
            policy,
        )?;
        Ok(())
    };
    match source {
        Body::Manifest(value) => visit_artifact_world_manifest_v1(value, &mut emit)?,
        Body::Module(value) => visit_artifact_module_record_v1(value, &mut emit)?,
        Body::Limits(value) => visit_artifact_module_limits_v1(value, &mut emit)?,
        Body::Security(value) => visit_artifact_security_policy_v1(value, &mut emit)?,
        Body::Active(value) => emit(
            ArtifactConfigAddressV1 {
                section: "active",
                ordinal: None,
                inner_ordinal: None,
                member: None,
                column: "value",
            },
            ArtifactConfigPayloadV1::Primitive(MemberPayloadRefV1::Text(value)),
        )?,
    }
    if (ctx.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(root)
}
/// Borrowed provisional GEN of precisely eight Runtime descriptors. Four
/// registry columns bind the same complete record body with independent outer
/// schema/key bindings; no embedded ID is substituted for an original map key.
pub fn prepare_artifact_config_genesis_v1<W, C, S>(
    source: &ArtifactConfigSourceRefV1<'_>,
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut ArtifactConfigWriteWorkV1,
    sink: &mut S,
) -> Result<(), MemberRecordError>
where
    W: StatePageWriter + ValueNodeReader,
    C: FnMut() -> bool,
    S: FnMut(ReservedValue<ExecutionWriteV1>) -> Result<(), ValueTreeError>,
{
    if (ctx.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    let _operation = ctx
        .memory
        .reserve(32 << 10, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    for field in ArtifactConfigFieldV1::ALL {
        work.cell()?;
        let meta = metadata(field.descriptor(), ctx)?;
        crate::incremental_field_write::emit(meta.field_key, meta.value_root, ctx, sink)?;
    }
    for (field, value) in [
        (
            ArtifactConfigFieldV1::Manifest,
            Body::Manifest(source.manifest),
        ),
        (ArtifactConfigFieldV1::Limits, Body::Limits(source.limits)),
        (
            ArtifactConfigFieldV1::Security,
            Body::Security(source.security),
        ),
    ] {
        let child = body(value, ctx, work)?;
        work.cell()?;
        let record = root_row(field.descriptor(), &(), field.schema(), child, ctx)?;
        crate::incremental_field_write::emit(record.field_key, record.value_root, ctx, sink)?;
    }
    let ModuleRegistry { records, active } = source.registry;
    for (key, value) in records {
        let child = body(Body::Module(value), ctx, work)?;
        for field in [
            ArtifactConfigFieldV1::RegistryManifest,
            ArtifactConfigFieldV1::RegistryRegisteredAt,
            ArtifactConfigFieldV1::RegistryRegisteredBy,
            ArtifactConfigFieldV1::RegistryAuditEventId,
        ] {
            work.cell()?;
            let record = root_row(field.descriptor(), key, field.schema(), child.clone(), ctx)?;
            crate::incremental_field_write::emit(record.field_key, record.value_root, ctx, sink)?;
        }
    }
    for (key, value) in active {
        let child = body(Body::Active(value), ctx, work)?;
        work.cell()?;
        let field = ArtifactConfigFieldV1::RegistryActive;
        let record = root_row(field.descriptor(), key, field.schema(), child, ctx)?;
        crate::incremental_field_write::emit(record.field_key, record.value_root, ctx, sink)?;
    }
    if (ctx.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(())
}
