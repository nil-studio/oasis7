//! Execution originals from the same immutable GEN World. Catalogs, policy and
//! Manifest are DATA; endpoint native/readback/acceptance gates remain mandatory.
use super::super::originals::Originals;
use super::*;
use oasis7_formal_protocol::{
    controlled_authority::formal::{bounded_context::WorkIdentityV1, context_wire},
    incremental_artifacts::{
        ArtifactCatalogV1, ArtifactKindV1, ExecutionManifestModeV1, IwsExecutionManifestV1,
    },
    native_environment::{
        MAX_NATIVE_DESCRIPTOR_BYTES, NativeEnvironmentRole, NativeExecutionEnvironmentV1,
    },
};
use std::cell::RefCell;
#[path = "formal_iws_genesis_projection.rs"]
mod projection;

/// The actual producer owner stays with its paid originals. There is no public
/// constructor, clone, caller-selected replacement root, or acceptance bit.
pub(crate) struct ProvisionalGenesisExecutionInputsV1 {
    business: ProvisionalProjectedGenesisBusinessV1,
    rules: PaidArtifactCatalog,
    rules_original: OriginalArtifactRefV1,
    rules_wire: ReservedValue<Vec<u8>>,
    policy_original: OriginalArtifactRefV1,
    policy_wire: ReservedValue<Vec<u8>>,
    native_original: OriginalArtifactRefV1,
    native_wire: ReservedValue<Vec<u8>>,
    manifest: ReservedValue<IwsExecutionManifestV1>,
    manifest_original: OriginalArtifactRefV1,
    manifest_wire: ReservedValue<Vec<u8>>,
    governing_manifest: [u8; 32],
    projection: oasis7_runtime::incremental_genesis_projection::PaidGenesisProjectionCommitmentV1,
    source_history: source_history::PaidGenesisSourceHistoryV1,
    namespaces: [(u64, u64); 3],
    core: WorkIdentityV1,
    allocation: MemoryReservation,
}

impl ProvisionalGenesisExecutionInputsV1 {
    pub(super) fn source_history(&self) -> &source_history::PaidGenesisSourceHistoryV1 {
        &self.source_history
    }
    pub(crate) fn business(&self) -> &ProvisionalProjectedGenesisBusinessV1 {
        &self.business
    }

    pub(crate) fn manifest(&self) -> &IwsExecutionManifestV1 {
        &self.manifest
    }

    pub(crate) fn governing_manifest(&self) -> [u8; 32] {
        self.governing_manifest
    }

    pub(crate) fn projection_root_bounded<'a>(
        &'a self,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<&'a str> {
        self.validate_context(common)?;
        self.projection.root_bounded(common).map_err(Error::Body)
    }

    pub(crate) fn manifest_original(&self) -> (OriginalArtifactRefV1, &ReservedValue<Vec<u8>>) {
        (self.manifest_original, &self.manifest_wire)
    }

    pub(crate) fn native_original(&self) -> (OriginalArtifactRefV1, &ReservedValue<Vec<u8>>) {
        (self.native_original, &self.native_wire)
    }

    pub(crate) fn policy_original(&self) -> (OriginalArtifactRefV1, &ReservedValue<Vec<u8>>) {
        (self.policy_original, &self.policy_wire)
    }

    pub(crate) fn rules_original(&self) -> (OriginalArtifactRefV1, &ReservedValue<Vec<u8>>) {
        (self.rules_original, &self.rules_wire)
    }

    pub(crate) fn rules_catalog(&self) -> &ArtifactCatalogV1 {
        self.rules.catalog()
    }

    pub(crate) fn rules_catalog_original(
        &self,
    ) -> (OriginalArtifactRefV1, &ReservedValue<Vec<u8>>) {
        self.rules.original()
    }

    pub(crate) fn validate_context(&self, common: &mut FormalBoundedContextV1<'_>) -> Result<()> {
        self.business.validate_context(common)?;
        self.rules.validate_context(common)?;
        self.source_history.validate_context(common)?;
        self.projection
            .validate_context(common)
            .map_err(Error::Body)?;
        common.charge(1, 256).map_err(context_error)?;
        if self.core != common.work().identity()
            || !self.allocation.uses_budget(common.memory())
            || !self.manifest.uses_budget(common.memory())
            || [
                &self.rules_wire,
                &self.policy_wire,
                &self.native_wire,
                &self.manifest_wire,
            ]
            .iter()
            .any(|wire| !wire.uses_budget(common.memory()))
            || self.manifest.native != self.native_original.id
            || self.manifest.policy != self.policy_original
            || self.manifest.wasm != self.business.wasm_catalog_original().0.id
            || self.manifest.rules != self.rules.original().0.id
            || !matches!(self.manifest.mode, ExecutionManifestModeV1::Genesis)
            || self.manifest.scope.genesis_config != self.business.config_ref.id
        {
            return Err(PageStoreError::Corrupt("GEN execution originals context/binding").into());
        }
        Ok(())
    }

    pub(in super::super) fn validate_stores(
        &self,
        stores: [&IncrementalPageStore; 3],
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<()> {
        self.validate_context(common)?;
        if !stores[0].memory_budget().same_pool(common.memory())
            || namespaces(stores, common)? != self.namespaces
        {
            return Err(PageStoreError::Corrupt("GEN original page namespaces changed").into());
        }
        Ok(())
    }

    pub(in super::super) fn into_projected_bounded_v1(
        self,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<oasis7_runtime::incremental_genesis_projection::RuntimeProjectedGenesisV1> {
        self.validate_context(common)?;
        self.business.into_projected_bounded_v1(common)
    }
}

fn namespaces(
    stores: [&IncrementalPageStore; 3],
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<[(u64, u64); 3]> {
    let mut out = [(0, 0); 3];
    for (store, slot) in stores.into_iter().zip(&mut out) {
        let bytes = store
            .root()
            .as_os_str()
            .as_encoded_bytes()
            .len()
            .checked_mul(8)
            .and_then(|n| n.checked_add(4096))
            .ok_or(PageStoreError::ResourceLimited(
                "GEN namespace inspection allocation",
            ))?;
        common.charge(8, bytes as u64).map_err(context_error)?;
        let _fee = common
            .memory()
            .reserve(bytes, MemoryClass::Ordinary)
            .map_err(PageStoreError::Memory)?;
        *slot = super::super::lease_id(store)?;
    }
    if out[0] == out[1] || out[0] == out[2] || out[1] == out[2] {
        return Err(PageStoreError::Corrupt("GEN namespaces are not independent").into());
    }
    Ok(out)
}

fn encode<T: serde::Serialize>(
    value: &T,
    max: usize,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<ReservedValue<Vec<u8>>> {
    context_wire::encode_paid(
        value,
        max,
        common,
        "GEN original byte limit",
        "GEN original CBOR",
    )
    .map_err(context_error)
}

/// This is the original descriptor grammar, not a new native verifier. The
/// original canonical encoding bounds the legacy validation's one transient
/// Vec and name set before that fixed kernel runs. Actual component originals,
/// registered endpoint jobs and sealed worker admission are checked downstream.
fn native_wire(
    native: &NativeExecutionEnvironmentV1,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<ReservedValue<Vec<u8>>> {
    let wire = encode(native, MAX_NATIVE_DESCRIPTOR_BYTES, common)?;
    common.consume_steps(1).map_err(context_error)?;
    if native.components.len() > 256 {
        return Err(PageStoreError::Corrupt("GEN native descriptor component bound").into());
    }
    // A whole maximal BTree node per component bounds the original set's
    // allocation. The original serde Vec grows to <2x its bounded final wire.
    let workspace = wire
        .len()
        .checked_mul(4)
        .and_then(|n| n.checked_add(native.components.len() * 2048 + 8192))
        .ok_or(PageStoreError::ResourceLimited(
            "GEN native validation workspace",
        ))?;
    common.charge(1, workspace as u64).map_err(context_error)?;
    let _fee = common
        .memory()
        .reserve(workspace, MemoryClass::Ordinary)
        .map_err(PageStoreError::Memory)?;
    for component in &native.components {
        common
            .charge(1, component.name.len() as u64)
            .map_err(context_error)?;
        if component.chunks.len() > 4096 || component.name.len() > 4096 {
            return Err(
                PageStoreError::Corrupt("GEN native descriptor component/chunk bound").into(),
            );
        }
        common
            .charge(
                component.chunks.len() as u64 + 8,
                (component.name.len() * 11 * 9
                    + component.content_digest.len()
                    + component.chunks.len() * 64) as u64,
            )
            .map_err(context_error)?;
    }
    common
        .charge(wire.len() as u64 + 8, (wire.len() * 4) as u64)
        .map_err(context_error)?;
    native.validate().map_err(|error| {
        Error::Body(BodyMutationError::Original(Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            error,
        ))))
    })?;
    common.consume_steps(1).map_err(context_error)?;
    Ok(wire)
}

pub(in super::super) fn native_digest(
    native: &NativeExecutionEnvironmentV1,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<[u8; 32]> {
    digest(&native_wire(native, common)?, common)
}

pub(in super::super) fn validate_native_original(
    native: &NativeExecutionEnvironmentV1,
    original: (OriginalArtifactRefV1, &ReservedValue<Vec<u8>>),
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<()> {
    let wire = native_wire(native, common)?;
    common
        .charge(1, (wire.len() + original.1.len()) as u64)
        .map_err(context_error)?;
    if !original.1.uses_budget(common.memory())
        || wire.as_slice() != original.1.as_slice()
        || digest(&wire, common)? != original.0.id
        || wire.len() as u64 != original.0.byte_len
    {
        return Err(
            PageStoreError::Corrupt("GEN staged native differs from captured original").into(),
        );
    }
    Ok(())
}

/// `native` is privately borrowed from Host's actual pre-approval capture. This
/// producer preserves that descriptor. It is still DATA until both real endpoints
/// independently verify their native originals and the complete candidate.
pub(in super::super) fn persist(
    request: &super::super::ProjectedGenesisInputsRequestV1<'_>,
    business: ProvisionalProjectedGenesisBusinessV1,
    history_work: source_history::GenesisSourceHistoryWorkV1,
    work: &mut IwsGenesisTransferWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    stop: &AtomicBool,
) -> Result<ProvisionalGenesisExecutionInputsV1> {
    let world = request.world;
    let native = request.native.descriptor();
    let source = request.source;
    let targets = request.targets;
    let config = request.config;
    business.validate_context(common)?;
    cancelled(stop)?;
    if !source.memory_budget().same_pool(common.memory()) {
        return Err(PageStoreError::Corrupt("GEN execution source pool").into());
    }
    let namespaces = namespaces([source, targets[0], targets[1]], common)?;
    let n = size_of::<ProvisionalGenesisExecutionInputsV1>();
    common.charge(1, n as u64).map_err(context_error)?;
    let allocation = common
        .memory()
        .reserve(n, MemoryClass::Ordinary)
        .map_err(PageStoreError::Memory)?;
    let objects = [
        open_store(source, 0, work, common, stop)?,
        open_store(targets[0], 1, work, common, stop)?,
        open_store(targets[1], 2, work, common, stop)?,
    ];
    let native_wire = native_wire(native, common)?;
    let native_original = write_original(
        &objects,
        &native_wire,
        Some(config.execution_environment),
        work,
        common,
        stop,
    )?;
    let mut executable = None;
    for component in &native.components {
        common.charge(1, 64).map_err(context_error)?;
        if component.role == NativeEnvironmentRole::Executable {
            if executable.replace(&component.content_digest).is_some() {
                return Err(PageStoreError::Corrupt("GEN native executable multiplicity").into());
            }
        }
    }
    let executable = executable.ok_or(PageStoreError::Corrupt("GEN native executable missing"))?;
    common.charge(1, 64).map_err(context_error)?;
    let mut executable_id = [0; 32];
    hex::decode_to_slice(executable, &mut executable_id)
        .map_err(|_| PageStoreError::Corrupt("GEN native executable digest"))?;

    // Preserve the exact legacy policy tuple grammar. The small inner policy
    // CBOR is only a field; its hash is NOT execution_policy_digest.
    let policy_cbor = encode(
        &crate::controlled_execution_policy::GuardedExecutionPolicy::current(),
        work.max_original_bytes,
        common,
    )?;
    let policy_wire = encode(
        &(
            "oasis7.formal.execution_policy.v1",
            &*policy_cbor,
            executable,
            world.release_security_policy(),
        ),
        work.max_original_bytes,
        common,
    )?;
    let policy_original = write_original(&objects, &policy_wire, None, work, common, stop)?;
    drop(policy_cbor);

    let rules_wire = encode(
        &(
            world.manifest(),
            world.policies(),
            world.release_security_policy(),
        ),
        work.max_original_bytes,
        common,
    )?;
    let rules_original = write_original(&objects, &rules_wire, None, work, common, stop)?;
    let mut rules = CatalogBuilder::new(ArtifactKindV1::Rules, common)?;
    let failed = RefCell::new(None);
    let result = {
        let originals = RefCell::new(Originals {
            stores: &objects,
            physical: &mut work.objects_physical,
            logical: &mut work.objects_logical,
            occurrences: &mut work.originals_remaining,
            max_bytes: work.max_original_bytes,
            cancelled: stop,
            error: &failed,
        });
        rules.append_original(rules_original, &originals, common)
    };
    if let Some(error) = failed.into_inner() {
        return Err(error.into());
    }
    result?;
    let rules = rules.finish(&objects, work, common, stop)?;

    // The frozen manifest is <=1024B. Admit its scope and the codec's body/output
    // Vecs before invoking the one original fixed-format codec.
    common.charge(32, 8192).map_err(context_error)?;
    let mut fee = common
        .memory()
        .reserve(8192, MemoryClass::Ordinary)
        .map_err(PageStoreError::Memory)?;
    let scope = artifact_catalog::protocol(common, |pay| config.scope_with_work(pay))?;
    let manifest = IwsExecutionManifestV1 {
        scope,
        format_profile: config.format_profile,
        native: native_original.id,
        executable: executable_id,
        wasm: business.wasm_catalog_original().0.id,
        rules: rules.original().0.id,
        policy: policy_original,
        mode: ExecutionManifestModeV1::Genesis,
    };
    let bytes = manifest
        .canonical_bytes()
        .map_err(artifact_catalog::execution_error)?;
    if bytes.len() > 1024 || bytes.capacity() > 1024 {
        return Err(PageStoreError::Corrupt("GEN manifest encoding allocation").into());
    }
    let wire_fee = fee
        .split(bytes.capacity() + size_of::<Vec<u8>>())
        .map_err(|_| PageStoreError::Corrupt("GEN manifest wire accounting"))?;
    let manifest_wire = wire_fee.retain(bytes);
    let manifest_fee = fee
        .split(size_of::<IwsExecutionManifestV1>() + manifest.scope.world_id.capacity())
        .map_err(|_| PageStoreError::Corrupt("GEN manifest owner accounting"))?;
    let manifest = manifest_fee.retain(manifest);
    let manifest_original = write_original(&objects, &manifest_wire, None, work, common, stop)?;
    // This binding is produced by the same captured business metadata. It is
    // not the execution Manifest above and is never taken from an input digest.
    let source = business
        .projected()
        .business_source_bounded_v1(common)
        .map_err(Error::Body)?;
    let binding = source
        .runtime_binding_bounded_v1(common, &mut work.canonical)
        .map_err(Error::Body)?;
    common
        .charge(1, binding.runtime_manifest_hash.0.len() as u64)
        .map_err(context_error)?;
    let governing = binding
        .runtime_manifest_hash
        .0
        .strip_prefix("blake3:")
        .ok_or(PageStoreError::Corrupt(
            "GEN governing manifest digest prefix",
        ))?;
    let mut governing_manifest = [0; 32];
    hex::decode_to_slice(governing, &mut governing_manifest)
        .map_err(|_| PageStoreError::Corrupt("GEN governing manifest digest"))?;
    drop(binding);
    drop(source);
    let projection = projection::capture(
        business.projected().projected(),
        &objects[0],
        request.source.memory_budget(),
        ObjectIo {
            physical: &mut work.objects_physical[0],
            logical: &mut work.objects_logical[0],
            class: MemoryClass::Ordinary,
            cancelled: stop,
        },
        common,
        &mut work.canonical,
    )?;
    let source_history = source_history::prepare(
        world,
        &business,
        [request.source, targets[0], targets[1]],
        history_work,
        work,
        common,
        stop,
    )?;
    let output = ProvisionalGenesisExecutionInputsV1 {
        business,
        rules,
        rules_original,
        rules_wire,
        policy_original,
        policy_wire,
        native_original,
        native_wire,
        manifest,
        manifest_original,
        manifest_wire,
        governing_manifest,
        projection,
        source_history,
        namespaces,
        core: common.work().identity(),
        allocation,
    };
    output.validate_context(common)?;
    Ok(output)
}
