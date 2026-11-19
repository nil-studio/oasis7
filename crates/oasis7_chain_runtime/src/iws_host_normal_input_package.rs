//! Actual Node continuation -> signed IWS Frozen DATA. The same installed
//! accepted parent supplies every retained role. Endpoint publication, not
//! this builder, creates Frozen authority.
use super::*;
use crate::iws_host_runtime::IwsHostRuntimeV1;
use crate::iws_normal_input_origin::PaidNodeInputOriginV1;
use oasis7_distfs::controlled_authority::formal::endpoint::{
    FormalArchiveReadWorkV1, HostAcceptedParentReadViewV1,
};
use oasis7_formal_protocol::{
    controlled_authority::formal::{
        AUTHORITY_ID_DOMAIN_V1, FROZEN_RESERVATION_SIGN_DOMAIN_V1,
        frozen_material_digest_bounded_v1, incremental::IWS_FROZEN_SCHEMA,
        normal_frozen_hash_bounded,
    },
    incremental_artifacts::{
        ExecutionManifestModeV1, IwsExecutionManifestV1, OriginalArtifactRefV1,
    },
    incremental_execution::ExecutionFence,
};
use oasis7_node::{
    NodeLocalExecutionContinuation, VerifiedNodeLocalContinuationV1,
    VerifiedNodeRestoredContinuationV1,
};
use oasis7_proto::memory_budget::MemoryBudget;
use std::sync::atomic::{AtomicBool, Ordering};

const PARENT_ROLES: [ArtifactRole; 7] = [
    ArtifactRole::IncrementalGenesisConfig,
    ArtifactRole::IncrementalRootSet,
    ArtifactRole::IncrementalCommitBody,
    ArtifactRole::IncrementalHistoryPrefix,
    ArtifactRole::Wasm,
    ArtifactRole::Rules,
    ArtifactRole::NativeExecutionEnvironment,
];

pub(in crate::iws_host_runtime) struct PaidPreparedNodeNormalInputV1 {
    pub(in crate::iws_host_runtime) reservation: SignedFormalFrozenV1,
    pub(in crate::iws_host_runtime) material: ClosedRecord,
    pub(in crate::iws_host_runtime) origin: PaidNodeInputOriginV1,
    record: [u8; 32],
    fence: ExecutionFence,
    namespaces: ((u64, u64), (u64, u64)),
    core: WorkIdentityV1,
    fee: MemoryReservation,
}

impl PaidPreparedNodeNormalInputV1 {
    pub(in crate::iws_host_runtime) fn into_origin(self) -> PaidNodeInputOriginV1 {
        self.origin
    }

    pub(in crate::iws_host_runtime) fn validate_primary_bounded(
        &self,
        host: &IwsHostRuntimeV1,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), NormalPackageErrorV1> {
        self.origin.validate_context(common)?;
        common.charge(1, 192)?;
        let current = host.current.current().ok_or_else(invalid)?;
        current.source.validate_context(common)?;
        if self.core != common.work().identity()
            || !self.fee.uses_budget(common.memory())
            || current.source.record_id() != self.record
            || current.source.fence() != self.fence
            || (
                current.source.page_namespace_identity(),
                current.source.native_namespace_identity(),
            ) != self.namespaces
        {
            return Err(invalid());
        }
        Ok(())
    }
}

impl IwsHostRuntimeV1 {
    pub(in crate::iws_host_runtime) fn prepare_fresh_normal_input_bounded(
        &self,
        source: &VerifiedNodeLocalContinuationV1<'_>,
        writer: &SigningKey,
        input_limits: &crate::iws_normal_input_decode::NormalInputDecodeLimitsV1,
        memory: &MemoryBudget,
        archive: &FormalArchiveReadWorkV1,
        cancelled: &AtomicBool,
    ) -> Result<PaidPreparedNodeNormalInputV1, NormalPackageErrorV1> {
        let origin = PaidNodeInputOriginV1::fresh(
            source,
            &mut archive.bounded_context(memory, &mut || cancelled.load(Ordering::Acquire)),
        )?;
        self.prepare_node_normal_input_inner(
            source.original(),
            origin,
            writer,
            input_limits,
            memory,
            archive,
            cancelled,
        )
    }

    pub(in crate::iws_host_runtime) fn prepare_restored_normal_input_bounded(
        &self,
        source: &VerifiedNodeRestoredContinuationV1<'_>,
        writer: &SigningKey,
        input_limits: &crate::iws_normal_input_decode::NormalInputDecodeLimitsV1,
        memory: &MemoryBudget,
        archive: &FormalArchiveReadWorkV1,
        cancelled: &AtomicBool,
    ) -> Result<PaidPreparedNodeNormalInputV1, NormalPackageErrorV1> {
        let origin = PaidNodeInputOriginV1::restored(
            source,
            &mut archive.bounded_context(memory, &mut || cancelled.load(Ordering::Acquire)),
        )?;
        self.prepare_node_normal_input_inner(
            source.original(),
            origin,
            writer,
            input_limits,
            memory,
            archive,
            cancelled,
        )
    }

    fn prepare_node_normal_input_inner(
        &self,
        original: &NodeLocalExecutionContinuation,
        origin: PaidNodeInputOriginV1,
        writer: &SigningKey,
        input_limits: &crate::iws_normal_input_decode::NormalInputDecodeLimitsV1,
        memory: &MemoryBudget,
        archive: &FormalArchiveReadWorkV1,
        cancelled: &AtomicBool,
    ) -> Result<PaidPreparedNodeNormalInputV1, NormalPackageErrorV1> {
        let current = self.current.current().ok_or_else(invalid)?;
        current.source.with_parent_read_view_bounded(
            memory,
            archive,
            &mut || cancelled.load(Ordering::Acquire),
            |parent| {
                let proof =
                    parent.read_qualified_parent_proof_bounded(memory, archive, &mut || {
                        cancelled.load(Ordering::Acquire)
                    })?;
                let mut stop = || cancelled.load(Ordering::Acquire);
                let mut common = archive.bounded_context(memory, &mut stop);
                proof.validate_parent_bounded(&parent, &mut common)?;
                current
                    .resident
                    .as_ref()
                    .ok_or_else(invalid)?
                    .validate_live_binding_v1(
                        current.source.record_id(),
                        &parent.fence(),
                        &mut common,
                    )?;
                build(
                    original,
                    origin,
                    &parent,
                    proof.material(),
                    writer,
                    input_limits,
                    &mut common,
                )
            },
        )
    }
}

fn digest(
    text: &str,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<[u8; 32], NormalPackageErrorV1> {
    common.charge(1, text.len() as u64)?;
    let mut id = [0u8; 32];
    hex::decode_to_slice(text, &mut id).map_err(|_| invalid())?;
    Ok(id)
}

fn material_digest(
    material: &ClosedRecord,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<[u8; 32], NormalPackageErrorV1> {
    // The original streaming encoder consumes one continuing child allowance.
    // Its full wire byte count bounds serializer work as well as hash bytes.
    common.charge(1, 64)?;
    let (memory, work, stop) = common.parts();
    let child = work.begin_child()?;
    let before = child.remaining_steps().min(child.remaining_read_bytes());
    let mut left = before;
    let result = frozen_material_digest_bounded_v1(material, memory, &mut left, stop);
    let spent = before.checked_sub(left).ok_or_else(invalid)?;
    let debit = child.consume(spent, spent);
    child.finish();
    let id = result?;
    debit?;
    Ok(id)
}

fn build(
    original: &NodeLocalExecutionContinuation,
    origin: PaidNodeInputOriginV1,
    parent: &HostAcceptedParentReadViewV1<'_>,
    parent_material: &ClosedRecord,
    writer: &SigningKey,
    input_limits: &crate::iws_normal_input_decode::NormalInputDecodeLimitsV1,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<PaidPreparedNodeNormalInputV1, NormalPackageErrorV1> {
    origin.validate_context(common)?;
    let record = parent.head().record();
    let boundary = crate::iws_host_runtime::node_boundary::read_from_parent_bounded(
        parent,
        parent_material,
        input_limits,
        common,
    )
    .map_err(NormalPackageErrorV1::Boundary)?;
    let accepted = boundary.boundary();
    common.charge(
        1,
        origin.input_bytes().len() as u64 + origin.continuation_bytes().len() as u64,
    )?;
    if original.schema_version != 1
        || original.context.world_id != record.world.world_id
        || original.context.node_id != original.context.proposer_id
        || original.predecessor_execution_height != original.predecessor_committed_height
        || original.predecessor_execution_height != accepted.height
        || original.predecessor_committed_height.checked_add(1) != Some(original.context.height)
        || original.predecessor_node_block_hash.as_deref()
            != Some(accepted.consensus_block_hash.as_str())
        || original.predecessor_execution_block_hash.as_deref()
            != Some(accepted.execution_block_hash.as_str())
        || original.predecessor_execution_state_root.as_deref()
            != Some(record.after_state_root.as_str())
    {
        return Err(invalid());
    }
    let root = oasis7_consensus::node_consensus_action::compute_consensus_action_root_bounded(
        &original.context.committed_actions,
        common,
    )?;
    common.charge(1, original.context.action_root.len() as u64)?;
    if *root != original.context.action_root {
        return Err(invalid());
    }
    let mut public_key = [0u8; 64];
    common.charge(1, 192)?;
    hex::encode_to_slice(writer.verifying_key().as_bytes(), &mut public_key)
        .map_err(|_| invalid())?;
    if public_key.as_slice() != parent.authority().definition().writer_public_key.as_bytes() {
        return Err(NormalPackageErrorV1::Invalid(
            "normal Frozen original writer key mismatch",
        ));
    }
    let mut a = Allocation::new(common)?;
    a.admit(size_of::<PaidPreparedNodeNormalInputV1>(), common)?;
    let operation_wire = context_wire::encode_paid(
        &("oasis7.formal.node_request.v1", &record.world, original),
        64 << 20,
        common,
        "normal request bound",
        "normal Node request",
    )?;
    let operation = a.hash(&operation_wire, common)?;
    let mut objects = a.vector(
        parent_material
            .objects
            .len()
            .checked_add(3)
            .ok_or_else(invalid)?,
        common,
    )?;
    for object in &parent_material.objects {
        objects.push(a.object(object, common)?);
    }
    let parent_manifest = record
        .artifact_roots
        .get(&ArtifactRole::ExecutionManifest)
        .ok_or_else(invalid)?;
    let at = material::locate(&objects, parent_manifest, common)?;
    // The canonical manifest codec has a hard 1024-byte frame limit. Its
    // decoded scope and re-encoded frame coexist under this paid fixed bound.
    a.admit(8192, common)?;
    common.charge(4096, 4096)?;
    let mut manifest = IwsExecutionManifestV1::from_canonical_bytes(&objects[at].bytes)?;
    let input = material::put(origin.input_bytes(), &mut objects, &mut a, common)?;
    let continuation = material::put(origin.continuation_bytes(), &mut objects, &mut a, common)?;
    manifest.mode = ExecutionManifestModeV1::Execution {
        parent_commit: parent.fence().parent_commit,
        parent_roots: parent.fence().parent_roots,
        operation: digest(&operation, common)?,
        input: digest(&input, common)?,
        continuation: OriginalArtifactRefV1 {
            id: digest(&continuation, common)?,
            byte_len: origin.continuation_bytes().len() as u64,
        },
    };
    let manifest_raw = manifest.canonical_bytes()?;
    let manifest_id = material::put(&manifest_raw, &mut objects, &mut a, common)?;
    let at = material::locate(&objects, &manifest_id, common)?;
    let mut refs = a.vector(2, common)?;
    let policy_id = a.hex(&manifest.policy.id, common)?;
    material::add_ref(&policy_id, &mut refs, &mut a, common)?;
    material::add_ref(&continuation, &mut refs, &mut a, common)?;
    objects[at].references = refs;
    a.admit(
        9 * (11 * (size_of::<(ArtifactRole, String)>() + size_of::<usize>()) + 128),
        common,
    )?;
    let mut roots = BTreeMap::new();
    for role in PARENT_ROLES {
        common.charge(1, 64)?;
        roots.insert(
            role,
            a.text(
                record.artifact_roots.get(&role).ok_or_else(invalid)?,
                common,
            )?,
        );
    }
    roots.insert(ArtifactRole::Input, a.text(&input, common)?);
    roots.insert(
        ArtifactRole::ExecutionManifest,
        a.text(&manifest_id, common)?,
    );
    let objects = material::select_closure(objects, &roots, &mut a, common)?;
    let material = ClosedRecord {
        payload_digest: a.text(&input, common)?,
        before_state_root: a.text(&record.after_state_root, common)?,
        after_state_root: a.text(&record.after_state_root, common)?,
        roots,
        objects,
    };
    material
        .validate_iws_bounded(common)
        .map_err(NormalPackageErrorV1::Material)?;
    let material_id = material_digest(&material, common)?;
    let authority_id = normal_frozen_hash_bounded(
        AUTHORITY_ID_DOMAIN_V1,
        parent.authority().definition(),
        common,
    )?;
    let body = FormalFrozenBodyV1 {
        schema_version: IWS_FROZEN_SCHEMA,
        world: a.world(&record.world, common)?,
        authority_id: a.text(&authority_id, common)?,
        epoch: parent.authority().definition().epoch,
        activation_ref: a.text(parent.authority().activation_ref(), common)?,
        parent: FormalHeadRefV1 {
            position: record.position,
            record_id: a.hex(&parent.head().verified_record_id(), common)?,
        },
        next_position: record.position.checked_add(1).ok_or_else(invalid)?,
        request_digest: operation,
        payload_digest: input,
        runtime_manifest_digest: manifest_id,
        governing_manifest_digest: a.text(&record.governing_manifest_digest, common)?,
        execution_policy_digest: a.text(&record.execution_policy_digest, common)?,
        pre_state_root: a.text(&record.after_state_root, common)?,
        frozen_material_digest: a.hex(&material_id, common)?,
        handoff: None,
    };
    // Validation traverses only the already-paid fixed-format body above.
    common.charge(
        1,
        4096 + record.world.world_id.len() as u64 + record.world.chain_id.len() as u64,
    )?;
    body.validate()?;
    let wire = context_wire::encode_paid(
        &body,
        64 << 20,
        common,
        "normal Frozen bound",
        "normal Frozen body",
    )?;
    let domain = FROZEN_RESERVATION_SIGN_DOMAIN_V1.as_bytes();
    let capacity = wire
        .len()
        .checked_add(domain.len() + 8)
        .ok_or_else(invalid)?;
    let mut frame = a.vector::<u8>(capacity, common)?;
    for raw in [
        &(domain.len() as u64).to_be_bytes()[..],
        domain,
        wire.as_slice(),
    ] {
        for chunk in raw.chunks(8192) {
            common.charge(1, chunk.len() as u64)?;
            frame.extend_from_slice(chunk);
        }
    }
    common.charge(1, frame.len() as u64)?;
    let signature = writer.sign(&frame).to_bytes();
    let mut signature_hex = a.vector::<u8>(128, common)?;
    signature_hex.resize(128, 0);
    common.charge(1, 192)?;
    hex::encode_to_slice(signature, &mut signature_hex).map_err(|_| invalid())?;
    let signature_hex = String::from_utf8(signature_hex).map_err(|_| invalid())?;
    origin.validate_context(common)?;
    let reservation = SignedFormalFrozenV1 {
        body,
        signature_hex,
    };
    // The original verifier enforces authority succession and committed-parent
    // continuity before any endpoint can receive these new DATA bytes.
    oasis7_formal_protocol::controlled_authority::formal::verify_formal_reservation_bounded(
        &reservation,
        parent.authority(),
        parent.head(),
        common,
    )?;
    Ok(PaidPreparedNodeNormalInputV1 {
        reservation,
        material,
        origin,
        record: parent.head().verified_record_id(),
        fence: parent.fence(),
        namespaces: parent.namespace_identities(),
        core: common.work().identity(),
        fee: a.fee,
    })
}
