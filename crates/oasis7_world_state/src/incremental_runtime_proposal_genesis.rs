//! Borrowed complete proposal columns and original JSON graphs.
use super::*;
use crate::{
    ManifestPatch, ManifestPatchOp, Proposal, ProposalStatus,
    incremental_field_presence::*,
    incremental_json_records::prepare_json_subtree_v1,
    incremental_record_tree::{self as tree, RecordTreeLabelsV1, RecordTreeWritePolicyV1},
    incremental_runtime_artifact_config::{
        ArtifactConfigPayloadV1, visit_artifact_world_manifest_v1,
    },
    incremental_semantic_key::hash_semantic_key_v1,
};
use oasis7_formal_protocol::{
    incremental_execution::ExecutionWriteV1, incremental_record_root::RecordRootRefV1,
    incremental_state::StatePageWriter, incremental_value::ValueNodeReader,
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
use std::collections::BTreeMap;
pub(super) const LABELS: RecordTreeLabelsV1 = RecordTreeLabelsV1 {
    read: "proposal record read",
    pending: "proposal pending node bytes",
    tree: "proposal column tree",
    digest: "proposal node digest",
    read_allowance: "proposal read allowance",
    admission: "proposal source admission",
    bound: "proposal source bound",
    producer_pool: "proposal producer pool",
    page: "proposal state page",
    write_allowance: "proposal write allowance",
};
struct Builder<'a, 'b, W, C> {
    root: StateRootV1,
    ctx: &'a mut MemberRecordWriteContext<'b, W, C>,
    work: &'a mut RuntimeProposalWriteWorkV1,
}
impl<W: StatePageWriter + ValueNodeReader, C: FnMut() -> bool> Builder<'_, '_, W, C> {
    fn spend(&mut self) -> Result<(), MemberRecordError> {
        spend(self.work, &mut self.ctx.cancelled)?;
        Ok(())
    }
    fn append(&mut self, key: &WorldFieldKeyV1, value: [u8; 32]) -> Result<(), MemberRecordError> {
        self.root = tree::append(
            self.root.clone(),
            key,
            value,
            self.ctx,
            RecordTreeWritePolicyV1 {
                max_pending_node_bytes: self.work.max_pending_node_bytes,
                labels: LABELS,
            },
        )?;
        Ok(())
    }
    fn member<K: SemanticKeyV1 + ?Sized>(
        &mut self,
        key: &K,
        value: MemberPayloadRefV1<'_>,
    ) -> Result<(), MemberRecordError> {
        self.spend()?;
        let row = write_member_record_v1(
            columns(),
            WorldFieldRecordKindV1::Entry,
            key,
            value,
            self.ctx,
        )?;
        self.append(&row.field_key, row.value_root)
    }
    fn text(&mut self, key: &str, value: &str) -> Result<(), MemberRecordError> {
        self.member(&key, MemberPayloadRefV1::Text(value))
    }
    fn unsigned(&mut self, key: &str, value: u64) -> Result<(), MemberRecordError> {
        self.member(&key, MemberPayloadRefV1::U64(value))
    }
    fn optional(&mut self, key: &str, value: Option<u64>) -> Result<(), MemberRecordError> {
        self.member(&(key, "present"), MemberPayloadRefV1::Bool(value.is_some()))?;
        if let Some(value) = value {
            self.member(&(key, "value"), MemberPayloadRefV1::U64(value))?;
        }
        Ok(())
    }
    fn json<K: SemanticKeyV1 + ?Sized>(
        &mut self,
        key: &K,
        value: &serde_json::Value,
    ) -> Result<(), MemberRecordError> {
        let profile = json_profile();
        let root = prepare_json_subtree_v1(profile, value, self.ctx, &mut self.work.json)?;
        let owner = hash_semantic_key_v1(
            key,
            self.ctx.memory,
            MemoryClass::Ordinary,
            self.ctx.key_work,
            &mut self.ctx.cancelled,
        )
        .map_err(MemberRecordError::Key)?;
        let reference = RecordRootRefV1::new(profile.schema, owner.digest, root)
            .map_err(|_| MemberRecordError::Invalid("proposal JSON reference"))?;
        self.member(key, MemberPayloadRefV1::RecordRoot(&reference))
    }
    fn patch(&mut self, patch: Option<&ManifestPatch>) -> Result<(), MemberRecordError> {
        self.member(
            &("patch", "present"),
            MemberPayloadRefV1::Bool(patch.is_some()),
        )?;
        let Some(ManifestPatch {
            base_manifest_hash,
            ops,
            new_version,
        }) = patch
        else {
            return Ok(());
        };
        self.text("patch.base_manifest_hash", base_manifest_hash)?;
        self.optional("patch.new_version", *new_version)?;
        self.unsigned(
            "patch.ops.count",
            u64::try_from(ops.len()).map_err(|_| invalid())?,
        )?;
        for (ordinal, op) in ops.iter().enumerate() {
            let ordinal = u64::try_from(ordinal).map_err(|_| invalid())?;
            let path = match op {
                ManifestPatchOp::Set { path, value } => {
                    self.member(
                        &("patch.ops", (ordinal, "kind")),
                        MemberPayloadRefV1::U64(0),
                    )?;
                    self.json(&("patch.ops", (ordinal, "value")), value)?;
                    path
                }
                ManifestPatchOp::Remove { path } => {
                    self.member(
                        &("patch.ops", (ordinal, "kind")),
                        MemberPayloadRefV1::U64(1),
                    )?;
                    path
                }
            };
            self.member(
                &("patch.ops", (ordinal, "path.count")),
                MemberPayloadRefV1::U64(u64::try_from(path.len()).map_err(|_| invalid())?),
            )?;
            for (index, segment) in path.iter().enumerate() {
                self.member(
                    &(
                        "patch.ops",
                        (
                            ordinal,
                            ("path", u64::try_from(index).map_err(|_| invalid())?),
                        ),
                    ),
                    MemberPayloadRefV1::Text(segment),
                )?;
            }
        }
        Ok(())
    }
    fn status(&mut self, status: &ProposalStatus) -> Result<(), MemberRecordError> {
        match status {
            ProposalStatus::Proposed => self.unsigned("status.kind", 0)?,
            ProposalStatus::Shadowed { manifest_hash } => {
                self.unsigned("status.kind", 1)?;
                self.text("status.manifest_hash", manifest_hash)?;
            }
            ProposalStatus::Approved {
                manifest_hash,
                approver,
            } => {
                self.unsigned("status.kind", 2)?;
                self.text("status.manifest_hash", manifest_hash)?;
                self.text("status.approver", approver)?;
            }
            ProposalStatus::Rejected { reason } => {
                self.unsigned("status.kind", 3)?;
                self.text("status.reason", reason)?;
            }
            ProposalStatus::Applied { manifest_hash } => {
                self.unsigned("status.kind", 4)?;
                self.text("status.manifest_hash", manifest_hash)?;
            }
        }
        Ok(())
    }
}
fn spend(
    work: &mut RuntimeProposalWriteWorkV1,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<(), ValueTreeError> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    work.cells_remaining = work
        .cells_remaining
        .checked_sub(1)
        .ok_or(ValueTreeError::ResourceLimited("proposal GEN cells"))?;
    Ok(())
}
pub fn prepare_runtime_proposal_body_v1<
    W: StatePageWriter + ValueNodeReader,
    C: FnMut() -> bool,
>(
    source: &Proposal,
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut RuntimeProposalWriteWorkV1,
) -> Result<StateRootV1, MemberRecordError> {
    let _scratch = ctx
        .memory
        .reserve(32 << 10, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let mut builder = Builder {
        root: StateRootV1::empty(StateNamespaceV1::Authorization).map_err(|_| invalid())?,
        ctx,
        work,
    };
    builder.spend()?;
    let metadata = write_field_metadata_v1(
        columns(),
        &mut PresenceRecordWriteContext {
            sink: &mut *builder.ctx.sink,
            memory: builder.ctx.memory,
            io: &mut *builder.ctx.io,
            key_work: &mut *builder.ctx.key_work,
            limits: PresenceRecordLimitsV1 {
                semantic_bytes: builder.ctx.limits.semantic_bytes,
                record_bytes: builder.ctx.limits.record_bytes,
            },
            cancelled: &mut builder.ctx.cancelled,
        },
    )?;
    builder.append(&metadata.field_key, metadata.value_root)?;
    let Proposal {
        id,
        author,
        base_manifest_hash,
        manifest,
        patch,
        queued_at_tick,
        not_before_tick,
        activate_epoch,
        timelock_ticks,
        status,
    } = source;
    builder.unsigned("id", *id)?;
    builder.text("author", author)?;
    builder.text("base_manifest_hash", base_manifest_hash)?;
    visit_artifact_world_manifest_v1(manifest, |address, value| {
        if address.section != "manifest"
            || address.ordinal.is_some()
            || address.inner_ordinal.is_some()
            || address.member.is_some()
        {
            return Err(invalid().into());
        }
        match (address.column, value) {
            ("version", ArtifactConfigPayloadV1::Primitive(MemberPayloadRefV1::U64(v))) => {
                builder.unsigned("manifest.version", v)
            }
            ("content", ArtifactConfigPayloadV1::Json(value)) => {
                builder.json(&"manifest.content", value)
            }
            _ => Err(invalid().into()),
        }
    })?;
    builder.patch(patch.as_ref())?;
    builder.optional("queued_at_tick", *queued_at_tick)?;
    builder.optional("not_before_tick", *not_before_tick)?;
    builder.optional("activate_epoch", *activate_epoch)?;
    builder.unsigned("timelock_ticks", *timelock_ticks)?;
    builder.status(status)?;
    if (builder.ctx.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(builder.root)
}
pub fn prepare_runtime_proposal_member_v1<
    W: StatePageWriter + ValueNodeReader,
    C: FnMut() -> bool,
>(
    original_key: u64,
    source: &Proposal,
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut RuntimeProposalWriteWorkV1,
) -> Result<MemberRecordV1, MemberRecordError> {
    let root = prepare_runtime_proposal_body_v1(source, ctx, work)?;
    let owner = hash_semantic_key_v1(
        &original_key,
        ctx.memory,
        MemoryClass::Ordinary,
        ctx.key_work,
        &mut ctx.cancelled,
    )
    .map_err(MemberRecordError::Key)?;
    let reference = RecordRootRefV1::new(schema(), owner.digest, root)
        .map_err(|_| MemberRecordError::Invalid("proposal outer reference"))?;
    write_member_record_v1(
        runtime_proposal_descriptor_v1(),
        WorldFieldRecordKindV1::Entry,
        &original_key,
        MemberPayloadRefV1::RecordRoot(&reference),
        ctx,
    )
}
pub fn prepare_runtime_proposals_genesis_v1<W, C, S>(
    source: &BTreeMap<u64, Proposal>,
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut RuntimeProposalWriteWorkV1,
    sink: &mut S,
) -> Result<(), MemberRecordError>
where
    W: StatePageWriter + ValueNodeReader,
    C: FnMut() -> bool,
    S: FnMut(ReservedValue<ExecutionWriteV1>) -> Result<(), ValueTreeError>,
{
    spend(work, &mut ctx.cancelled)?;
    let meta = write_field_metadata_v1(
        runtime_proposal_descriptor_v1(),
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
    )?;
    crate::incremental_field_write::emit(meta.field_key, meta.value_root, ctx, sink)?;
    for (key, proposal) in source {
        spend(work, &mut ctx.cancelled)?;
        let row = prepare_runtime_proposal_member_v1(*key, proposal, ctx, work)?;
        crate::incremental_field_write::emit(row.field_key, row.value_root, ctx, sink)?;
    }
    if (ctx.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(())
}
