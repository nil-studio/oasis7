//! Root-backed invocation preparation. No authorization, dispatch or selected head.
use super::{
    incremental_body::BodyMutationError,
    incremental_module_instance_family::ModuleInstanceFamilyQualificationV1,
    incremental_module_read::ModuleReadContext,
};
use oasis7_agent_api::world_action::ModuleInstallTarget;
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFence, PreparedGuard, RootSetV1},
    incremental_material::OriginalObjectStreamReader,
    incremental_state::{MerkleNodeReader, StateScanBudget},
    incremental_value::{ValueNodeReader, ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, MemoryReservation, ReservedValue};
use oasis7_world_state::{
    ModuleRegistry,
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
    incremental_field_read::WorldFieldReadContextV1,
    incremental_installed_module_targets::{
        InstalledModuleTargetReadV1, read_installed_module_target_overlay_v1,
    },
    incremental_module_instance_fields::{
        ModuleInstallTargetRefV1 as Target, ModuleInstanceFieldV1 as Field, ModuleInstanceKeyV1,
        ModuleInstanceScalarRefV1 as Scalar,
    },
    incremental_runtime_artifact_config::*,
    incremental_runtime_artifact_content::*,
};
use sha2::{Digest, Sha256};
use std::{
    io::{self, Cursor, Write},
    sync::Arc,
};
#[path = "incremental_module_original.rs"]
mod original;

#[derive(Debug)]
pub enum ModuleInvocationPreparationErrorV1<E> {
    Body(BodyMutationError),
    Overlay(WorldFieldOverlayErrorV1),
    Original(RuntimeArtifactOriginalErrorV1<E>),
    Value(ValueTreeError),
}
impl<E> From<BodyMutationError> for ModuleInvocationPreparationErrorV1<E> {
    fn from(v: BodyMutationError) -> Self {
        Self::Body(v)
    }
}
impl<E> From<WorldFieldOverlayErrorV1> for ModuleInvocationPreparationErrorV1<E> {
    fn from(v: WorldFieldOverlayErrorV1) -> Self {
        Self::Overlay(v)
    }
}
impl<E> From<ValueTreeError> for ModuleInvocationPreparationErrorV1<E> {
    fn from(v: ValueTreeError) -> Self {
        Self::Value(v)
    }
}
impl<E> From<oasis7_formal_protocol::incremental_execution::ExecutionError>
    for ModuleInvocationPreparationErrorV1<E>
{
    fn from(v: oasis7_formal_protocol::incremental_execution::ExecutionError) -> Self {
        Self::Body(v.into())
    }
}
pub struct ModuleInvocationPreparationRequestV1<'a> {
    pub invocation_id: &'a str,
    pub parent_roots: &'a RootSetV1,
    pub instances: &'a ReservedValue<ModuleInstanceFamilyQualificationV1>,
    pub config: &'a ReservedValue<ArtifactConfigFamilyQualificationV1>,
    pub content: &'a ReservedValue<RuntimeArtifactContentQualificationV1>,
    /// Only a genuine typed current-body token can bypass same-TX bootstrap.
    pub current_active: Option<&'a ReservedValue<ArtifactConfigBodyQualificationV1>>,
    pub current_record: Option<&'a ReservedValue<ArtifactConfigBodyQualificationV1>>,
    pub guard: &'a PreparedGuard,
    pub live: &'a ExecutionFence,
}
pub struct ModuleInvocationPreparationLimitsV1 {
    pub selection_heap_bytes: usize,
    pub max_text_bytes: usize,
    pub manifest: SelectedModuleManifestLimitsV1,
    pub manifest_json_bytes_remaining: u64,
}
/// A charged source/selection result. This grants no business invocation or
/// authority permission. The caller must still establish all execution owners.
pub struct PreparedRootModuleInvocationV1 {
    instance_id: String,
    module_id: String,
    target: ModuleInstallTarget,
    manifest: ReservedValue<SelectedModuleManifestV1>,
    manifest_sha256: [u8; 32],
    wasm: Arc<[u8]>,
    _wasm_charge: MemoryReservation,
    _selection_charge: MemoryReservation,
    guard: PreparedGuard,
    parent: [u8; 32],
}
impl PreparedRootModuleInvocationV1 {
    pub fn instance_id(&self) -> &str {
        &self.instance_id
    }
    pub fn module_id(&self) -> &str {
        &self.module_id
    }
    pub fn install_target(&self) -> &ModuleInstallTarget {
        &self.target
    }
    pub fn manifest(&self) -> &oasis7_world_state::ModuleManifest {
        self.manifest.manifest()
    }
    pub fn manifest_sha256(&self) -> &[u8; 32] {
        &self.manifest_sha256
    }
    /// Borrow the Arc; its memory admission cannot be detached from this owner.
    pub fn wasm(&self) -> &[u8] {
        &self.wasm
    }
    pub fn guard(&self) -> &PreparedGuard {
        &self.guard
    }
    pub fn parent_roots_digest(&self) -> &[u8; 32] {
        &self.parent
    }
}
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("root module invocation preparation")
}
fn text_copy(value: &str, heap: &mut usize) -> Result<String, ValueTreeError> {
    *heap = heap
        .checked_sub(value.len().checked_add(128).ok_or_else(invalid)?)
        .ok_or(ValueTreeError::ResourceLimited("module selection heap"))?;
    Ok(value.to_owned())
}
struct JsonHash<'a, C> {
    hash: Sha256,
    remaining: &'a mut u64,
    cancel: &'a mut C,
    error: Option<ValueTreeError>,
}
impl<C: FnMut() -> bool> Write for JsonHash<'_, C> {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        if self.error.is_some() {
            return Err(io::Error::other("manifest JSON stopped"));
        }
        if (self.cancel)() {
            self.error = Some(ValueTreeError::Cancelled);
            return Err(io::Error::other("manifest JSON cancelled"));
        }
        // One unit per attempt also bounds zero-length/custom writer work.
        let cost = (b.len() as u64).saturating_add(1);
        let Some(left) = self.remaining.checked_sub(cost) else {
            self.error = Some(ValueTreeError::ResourceLimited("manifest JSON hash work"));
            return Err(io::Error::other("manifest JSON hash work"));
        };
        *self.remaining = left;
        self.hash.update(b);
        Ok(b.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn manifest_hash<C: FnMut() -> bool>(
    manifest: &oasis7_world_state::ModuleManifest,
    remaining: &mut u64,
    cancel: &mut C,
) -> Result<[u8; 32], ValueTreeError> {
    let mut sink = JsonHash {
        hash: Sha256::new(),
        remaining,
        cancel,
        error: None,
    };
    let encoded = serde_json::to_writer(&mut sink, manifest);
    if let Some(error) = sink.error {
        return Err(error);
    }
    encoded.map_err(|_| invalid())?;
    if (sink.cancel)() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(sink.hash.finalize().into())
}

/// The original instance -> inactive-count suppression -> active-registry
/// fallback, using only this transaction's authenticated reads. All errors
/// poison the same outer transaction, including source/encoding final errors.
#[expect(
    clippy::too_many_arguments,
    reason = "Independent state, value, schema and original contexts retain their continuous admission budgets."
)]
/// Actual Current manifest selection without touching artifact/WASM originals.
/// This is source DATA and grants no execution, authorization or tick permission.
pub(in crate::world) struct CurrentModuleManifestSelectionV1 {
    instance_id: String,
    module_id: String,
    target: ModuleInstallTarget,
    manifest: ReservedValue<SelectedModuleManifestV1>,
    _selection_charge: MemoryReservation,
    guard: PreparedGuard,
    parent: [u8; 32],
    memory: oasis7_proto::memory_budget::MemoryBudget,
}
impl CurrentModuleManifestSelectionV1 {
    pub(in crate::world) fn instance_id(&self) -> &str {
        &self.instance_id
    }
    pub(in crate::world) fn module_id(&self) -> &str {
        &self.module_id
    }
    pub(in crate::world) fn install_target(&self) -> &ModuleInstallTarget {
        &self.target
    }
    pub(in crate::world) fn manifest(&self) -> &oasis7_world_state::ModuleManifest {
        self.manifest.manifest()
    }
    pub(in crate::world) fn validate<M: MerkleNodeReader>(
        &self,
        tx: &oasis7_formal_protocol::incremental_execution::ExecutionTransaction<'_, M>,
        memory: &oasis7_proto::memory_budget::MemoryBudget,
        live: &ExecutionFence,
    ) -> Result<(), ModuleInvocationPreparationErrorV1<std::convert::Infallible>> {
        tx.validate_guard(&self.guard, live)?;
        if self.parent != tx.parent_fence().parent_roots
            || !self.memory.same_pool(memory)
            || !self.manifest.uses_budget(memory)
            || !tx.uses_budget(memory)
        {
            return Err(invalid().into());
        }
        Ok(())
    }
}
pub(in crate::world) fn read_current_root_module_manifest_selection_v1<M, R, E, C>(
    request: ModuleInvocationPreparationRequestV1<'_>,
    state: &M,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut ArtifactConfigReadWorkV1,
    limits: &mut ModuleInvocationPreparationLimitsV1,
) -> Result<
    Option<CurrentModuleManifestSelectionV1>,
    ModuleInvocationPreparationErrorV1<std::convert::Infallible>,
>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    with_current_module_manifest_selection_v1(
        InvocationRouteV1::InstanceOrActive,
        request,
        state,
        ctx,
        scan,
        work,
        limits,
        |selected, _, _| Ok(selected),
    )
}
pub(in crate::world) fn read_current_registry_module_manifest_selection_v1<M, R, E, C>(
    request: ModuleInvocationPreparationRequestV1<'_>,
    state: &M,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut ArtifactConfigReadWorkV1,
    limits: &mut ModuleInvocationPreparationLimitsV1,
) -> Result<
    Option<CurrentModuleManifestSelectionV1>,
    ModuleInvocationPreparationErrorV1<std::convert::Infallible>,
>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    with_current_module_manifest_selection_v1(
        InvocationRouteV1::RegistryActive,
        request,
        state,
        ctx,
        scan,
        work,
        limits,
        |selected, _, _| Ok(selected),
    )
}

fn with_current_module_manifest_selection_v1<M, R, E, C, OE, OUT>(
    route: InvocationRouteV1,
    request: ModuleInvocationPreparationRequestV1<'_>,
    state: &M,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut ArtifactConfigReadWorkV1,
    limits: &mut ModuleInvocationPreparationLimitsV1,
    output: impl FnOnce(
        CurrentModuleManifestSelectionV1,
        &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, &mut E, &mut C>,
        &mut ModuleInvocationPreparationLimitsV1,
    ) -> Result<OUT, ModuleInvocationPreparationErrorV1<OE>>,
) -> Result<Option<OUT>, ModuleInvocationPreparationErrorV1<OE>>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let WorldFieldOverlayContextV1 {
        transaction,
        values,
        key_work,
        semantic_bytes,
        record_bytes,
        take_state_error,
        cancelled,
    } = ctx;
    transaction.contribute(|tx| {
        tx.validate_guard(request.guard, request.live)?;
        let memory = values.memory;
        if values.class != MemoryClass::Ordinary
            || !tx.uses_budget(memory)
            || !request.config.uses_budget(memory)
            || request.config.allocation_class() != values.class
            || !request.content.uses_budget(memory)
            || request.content.allocation_class() != values.class
            || request.config.reserved_bytes()
                < std::mem::size_of::<ArtifactConfigFamilyQualificationV1>()
            || request.content.reserved_bytes()
                < std::mem::size_of::<RuntimeArtifactContentQualificationV1>()
            || request.parent_roots.digest()? != tx.parent_fence().parent_roots
        {
            return Err(invalid().into());
        }
        request
            .config
            .validate_binding(&tx.parent_fence().parent_roots)?;
        request.content.validate_binding(request.parent_roots)?;
        let selection_charge = memory
            .reserve(limits.selection_heap_bytes, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let _scratch = memory
            .reserve(32 << 10, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let mut heap = limits.selection_heap_bytes;
        // Two provisional fixed-capacity text outputs are charged before allocation.
        heap = heap
            .checked_sub(limits.max_text_bytes.checked_mul(2).ok_or_else(invalid)?)
            .ok_or(ValueTreeError::ResourceLimited(
                "module selection text workspace",
            ))?;
        let selection = if matches!(route, InvocationRouteV1::RegistryActive) {
            None
        } else {
            let mut read = ModuleReadContext {
                qualification: request.instances,
                values: values.source,
                memory,
                io: &mut *values.io,
                max_value_bytes: limits.max_text_bytes as u64,
                cancelled: &mut *cancelled,
            };
            read.check(tx)?;
            // Oversize map keys cannot occur under the dedicated instance codec;
            // they remain legitimate arbitrary global module IDs/count queries.
            let active = if ModuleInstanceKeyV1::new(request.invocation_id, Field::Active).is_ok() {
                read.read_instance_field(request.invocation_id, Field::Active, tx)?
            } else {
                None
            };
            if let Some(active) = active {
                if !matches!(
                    Scalar::from_canonical_bytes(&active, Field::Active)
                        .map_err(BodyMutationError::Invalid)?,
                    Scalar::Bool(true)
                ) {
                    return Ok(None);
                }
                let mut text = |field| -> Result<String, ModuleInvocationPreparationErrorV1<OE>> {
                    let value = read
                        .read_instance_field(request.invocation_id, field, tx)?
                        .ok_or_else(invalid)?;
                    match Scalar::from_canonical_bytes(&value, field)
                        .map_err(BodyMutationError::Invalid)?
                    {
                        Scalar::Text(v) => Ok(text_copy(v, &mut heap)?),
                        _ => Err(invalid().into()),
                    }
                };
                let id = text(Field::InstanceId)?;
                let module = text(Field::ModuleId)?;
                let version = text(Field::Version)?;
                let value = read
                    .read_instance_field(request.invocation_id, Field::Target, tx)?
                    .ok_or_else(invalid)?;
                let target = match Scalar::from_canonical_bytes(&value, Field::Target)
                    .map_err(BodyMutationError::Invalid)?
                {
                    Scalar::Target(Target::SelfAgent) => ModuleInstallTarget::SelfAgent,
                    Scalar::Target(Target::LocationInfrastructure(v)) => {
                        ModuleInstallTarget::LocationInfrastructure {
                            location_id: text_copy(v, &mut heap)?,
                        }
                    }
                    _ => return Err(invalid().into()),
                };
                Some((id, module, version, target))
            } else if read.module_instance_count(request.invocation_id, tx)? > 0 {
                return Ok(None);
            } else {
                None
            }
        };
        let mut overlay = WorldFieldOverlayContextV1 {
            transaction: tx,
            values: ValueReadContext {
                source: values.source,
                memory,
                class: values.class,
                io: &mut *values.io,
            },
            key_work,
            semantic_bytes: *semantic_bytes,
            record_bytes: *record_bytes,
            take_state_error: &mut *take_state_error,
            cancelled: &mut *cancelled,
        };
        let (instance_id, module_id, version, target) = if let Some(selected) = selection {
            selected
        } else {
            let Some(active) = open_artifact_config_body_overlay_v1(
                ArtifactConfigSelectedBodyRequestV1 {
                    field: ArtifactConfigFieldV1::RegistryActive,
                    key: ArtifactConfigRecordKeyV1::Member(request.invocation_id),
                    family: request.config,
                    parent_roots: request.parent_roots,
                    current: request.current_active,
                },
                state,
                &mut overlay,
                scan,
                work,
            )?
            else {
                return Ok(None);
            };
            let mut bytes = vec![0; limits.max_text_bytes];
            let mut output = Cursor::new(bytes.as_mut_slice());
            // New authenticated reads advance the transaction revision too.
            // The caller's guard was checked before any operation; this local
            // getter must bind the revision after our own preceding reads.
            let current_guard = overlay.transaction.guard();
            let value = read_qualified_artifact_config_overlay_v1(
                ArtifactConfigCellRequestV1 {
                    field: ArtifactConfigFieldV1::RegistryActive,
                    key: ArtifactConfigRecordKeyV1::Member(request.invocation_id),
                    address: ArtifactConfigAddressV1 {
                        section: "active",
                        ordinal: None,
                        inner_ordinal: None,
                        member: None,
                        column: "value",
                    },
                    family: request.config,
                    qualification: &active,
                    max_payload_bytes: limits.max_text_bytes as u64,
                },
                &mut overlay,
                &current_guard,
                request.live,
                &mut output,
            )?;
            if !matches!(*value, ArtifactConfigValueV1::Text { .. }) {
                return Err(invalid().into());
            }
            let used = output.position() as usize;
            bytes.truncate(used);
            let version = String::from_utf8(bytes).map_err(|_| invalid())?;
            let mut target_bytes = vec![0; limits.max_text_bytes];
            let mut output = Cursor::new(target_bytes.as_mut_slice());
            let target = if matches!(route, InvocationRouteV1::RegistryActive) {
                None
            } else {
                read_installed_module_target_overlay_v1(
                    request.invocation_id,
                    &mut overlay,
                    limits.max_text_bytes as u64,
                    &mut output,
                )?
            };
            let used = output.position() as usize;
            target_bytes.truncate(used);
            let target = match target {
                None | Some(InstalledModuleTargetReadV1::SelfAgent) => {
                    ModuleInstallTarget::SelfAgent
                }
                Some(InstalledModuleTargetReadV1::LocationInfrastructure { .. }) => {
                    ModuleInstallTarget::LocationInfrastructure {
                        location_id: String::from_utf8(target_bytes).map_err(|_| invalid())?,
                    }
                }
            };
            (
                text_copy(request.invocation_id, &mut heap)?,
                text_copy(request.invocation_id, &mut heap)?,
                version,
                target,
            )
        };
        let key_len = module_id
            .len()
            .checked_add(version.len())
            .and_then(|n| n.checked_add(129))
            .ok_or_else(invalid)?;
        heap = heap
            .checked_sub(key_len)
            .ok_or(ValueTreeError::ResourceLimited(
                "module selection record key",
            ))?;
        let _ = heap;
        let key = ModuleRegistry::record_key(&module_id, &version);
        let body = open_artifact_config_body_overlay_v1(
            ArtifactConfigSelectedBodyRequestV1 {
                field: ArtifactConfigFieldV1::RegistryManifest,
                key: ArtifactConfigRecordKeyV1::Member(&key),
                family: request.config,
                parent_roots: request.parent_roots,
                current: request.current_record,
            },
            state,
            &mut overlay,
            scan,
            work,
        )?
        .ok_or_else(invalid)?;
        let manifest = read_selected_module_manifest_v1(
            &body,
            &mut WorldFieldReadContextV1 {
                root: body.root(),
                state,
                values: ValueReadContext {
                    source: values.source,
                    memory,
                    class: values.class,
                    io: &mut *overlay.values.io,
                },
                key_work: &mut *overlay.key_work,
                semantic_bytes: *semantic_bytes,
                record_bytes: *record_bytes,
                take_state_error: &mut *overlay.take_state_error,
                cancelled: &mut *overlay.cancelled,
            },
            work,
            scan,
            &limits.manifest,
        )?;
        if (overlay.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        let guard = overlay.transaction.guard();
        overlay.transaction.validate_guard(&guard, request.live)?;
        let selected = CurrentModuleManifestSelectionV1 {
            instance_id,
            module_id,
            target,
            manifest,
            _selection_charge: selection_charge,
            guard,
            parent: overlay.transaction.parent_fence().parent_roots,
            memory: memory.clone(),
        };
        output(selected, &mut overlay, limits).map(Some)
    })
}

fn finish_current_module_manifest_selection_v1<M, R, E, C, O>(
    selected: CurrentModuleManifestSelectionV1,
    overlay: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    originals: &mut O,
    original_work: &mut RuntimeArtifactOriginalWorkV1,
    limits: &mut ModuleInvocationPreparationLimitsV1,
    live: &ExecutionFence,
) -> Result<PreparedRootModuleInvocationV1, ModuleInvocationPreparationErrorV1<O::Error>>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    O: OriginalObjectStreamReader,
{
    let CurrentModuleManifestSelectionV1 {
        instance_id,
        module_id,
        target,
        manifest,
        _selection_charge: selection_charge,
        parent,
        ..
    } = selected;
    let memory = overlay.values.memory;
    let manifest_sha256 = manifest_hash(
        manifest.manifest(),
        &mut limits.manifest_json_bytes_remaining,
        &mut overlay.cancelled,
    )?;
    let original = match read_runtime_artifact_content_overlay_v1(
        RuntimeArtifactContentFieldV1::Original,
        &manifest.manifest().wasm_hash,
        overlay,
    )? {
        RuntimeArtifactContentValueV1::Original(Some(v)) => v,
        _ => return Err(invalid().into()),
    };
    let (wasm, wasm_charge) = original::selected_original(
        original,
        originals,
        memory,
        original_work,
        &mut overlay.cancelled,
    )
    .map_err(ModuleInvocationPreparationErrorV1::Original)?;
    if (overlay.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    let guard = overlay.transaction.guard();
    overlay.transaction.validate_guard(&guard, live)?;
    Ok(PreparedRootModuleInvocationV1 {
        instance_id,
        module_id,
        target,
        manifest,
        manifest_sha256,
        wasm,
        _wasm_charge: wasm_charge,
        _selection_charge: selection_charge,
        guard,
        parent,
    })
}

fn prepare_root_module_invocation_route_v1<M, R, E, C, O>(
    route: InvocationRouteV1,
    request: ModuleInvocationPreparationRequestV1<'_>,
    state: &M,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut ArtifactConfigReadWorkV1,
    originals: &mut O,
    original_work: &mut RuntimeArtifactOriginalWorkV1,
    limits: &mut ModuleInvocationPreparationLimitsV1,
) -> Result<Option<PreparedRootModuleInvocationV1>, ModuleInvocationPreparationErrorV1<O::Error>>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    O: OriginalObjectStreamReader,
{
    let parent_roots = request.parent_roots;
    let live = request.live;
    with_current_module_manifest_selection_v1(
        route,
        request,
        state,
        ctx,
        scan,
        work,
        limits,
        |selected, overlay, limits| {
            let mut prepared = finish_current_module_manifest_selection_v1(
                selected,
                overlay,
                originals,
                original_work,
                limits,
                live,
            )?;
            // Preserve the ordinary path's original final parent encoding check.
            prepared.parent = parent_roots.digest()?;
            Ok(prepared)
        },
    )
}

#[path = "incremental_module_execution_selected.rs"]
mod execution;
pub use execution::execute_selected_root_module_output_v1;

#[derive(Clone, Copy)]
enum InvocationRouteV1 {
    InstanceOrActive,
    RegistryActive,
}
#[path = "incremental_module_invocation_route.rs"]
mod route;
pub use route::{prepare_root_module_invocation_v1, prepare_root_module_registry_invocation_v1};

#[path = "incremental_module_execution_target.rs"]
mod target;
pub(in crate::world) use target::{PreparedRuleOverrideEventV1, RootRuleOverrideAuditV1};
pub use target::{
    RootModuleExecutionContextV1, RootModuleExecutionErrorV1, RootModuleExecutionFamiliesV1,
    RootModuleExecutionWorkV1, RootModuleTickOutcomeV1, RootRulePhaseOutcomeV1,
    RootRulePhaseWorkV1,
};

#[path = "incremental_module_invocation_enumerate.rs"]
mod enumerate;
pub use enumerate::{
    AcceptedInvocationDirectoryV1, CurrentInvocationEnumerationErrorV1,
    CurrentInvocationEnumerationWorkV1, build_accepted_invocation_directory_v1,
    collect_current_module_invocation_ids_v1,
};

pub(in crate::world) use enumerate::{
    FrozenInvocationDirectoryV1, any_current_active_artifact_v1, current_registry_active_keys_v1,
    freeze_current_invocation_directory_v1,
};

pub(in crate::world) use target::{CompletedNonmovementActionV1, NonmovementSelectedWorkV1};

pub(in crate::world) use target::NonmovementSelectedErrorV1;

pub(in crate::world) use target::SelectedAcceptedDispositionV1;

pub(in crate::world) use target::EffectiveActionWorkBankV1;

pub(in crate::world) use target::{
    CompletedNormalExtensionV1, NormalCoreExtensionWorkV1, NormalCoreExtensionWorkViewV1,
};

// Private result handoff for the fixed normal publisher and genuine fixtures.
pub(in crate::world) use target::MatchingBeforePublicationV1;

pub(in crate::world) use target::{PaidDueProductVmFailureV1, PaidFactoryModuleExecutionV1};

/// Complete the original artifact/WASM suffix only for one opaque slot from
/// the fully preselected gameplay Tick route. No caller manifest is accepted.
pub(in crate::world) fn finish_captured_gameplay_tick_manifest_v1<M, R, E, C, O>(
    slot: crate::world::incremental_normal_gameplay_cycles::DueGameplayTickSlotV1,
    core: oasis7_formal_protocol::controlled_authority::formal::bounded_context::WorkIdentityV1,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    originals: &mut O,
    original_work: &mut RuntimeArtifactOriginalWorkV1,
    limits: &mut ModuleInvocationPreparationLimitsV1,
    live: &ExecutionFence,
) -> Result<
    (
        Option<PreparedCapturedGameplayTickV1>,
        Option<crate::world::incremental_normal_gameplay_cycles::CapturedGameplayWorldHashV1>,
    ),
    ModuleInvocationPreparationErrorV1<O::Error>,
>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    O: OriginalObjectStreamReader,
{
    slot.validate(ctx.transaction, ctx.values.memory, core, live)?;
    let (selection, hash) = slot.into_selection();
    let Some(selected) = selection else {
        return Ok((None, hash));
    };
    ctx.transaction
        .validate_transaction_identity(&selected.guard)?;
    if selected.parent != ctx.transaction.parent_fence().parent_roots
        || !selected.memory.same_pool(ctx.values.memory)
        || !selected.manifest.uses_budget(ctx.values.memory)
        || ctx.values.class != MemoryClass::Ordinary
    {
        return Err(invalid().into());
    }
    // The original due route removes the wake but does not open WASM for an
    // active manifest that has no Tick subscription.
    if !crate::world::module_tick_runtime::module_has_tick_subscription(selected.manifest()) {
        return Ok((None, hash));
    }
    let prepared = finish_current_module_manifest_selection_v1(
        selected,
        ctx,
        originals,
        original_work,
        limits,
        live,
    )?;
    Ok((
        Some(PreparedCapturedGameplayTickV1 { selected: prepared }),
        hash,
    ))
}

/// Only the genuine due-slot finisher can create this captured-input carrier.
/// Ordinary selections cannot use its read-only input handoff.
pub(in crate::world) struct PreparedCapturedGameplayTickV1 {
    selected: PreparedRootModuleInvocationV1,
}
impl std::ops::Deref for PreparedCapturedGameplayTickV1 {
    type Target = PreparedRootModuleInvocationV1;
    fn deref(&self) -> &Self::Target {
        &self.selected
    }
}
impl PreparedCapturedGameplayTickV1 {
    pub(in crate::world) fn finish_after_input<M, R, E, C>(
        mut self,
        ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
        live: &ExecutionFence,
    ) -> Result<PreparedRootModuleInvocationV1, BodyMutationError>
    where
        M: MerkleNodeReader,
        R: ValueNodeReader,
        E: FnMut() -> Option<ValueTreeError>,
        C: FnMut() -> bool,
    {
        ctx.transaction
            .validate_transaction_identity(&self.selected.guard)?;
        if self.selected.parent != ctx.transaction.parent_fence().parent_roots
            || !self.selected.manifest.uses_budget(ctx.values.memory)
            || ctx.values.class != MemoryClass::Ordinary
        {
            return Err(BodyMutationError::Invalid("captured Tick input handoff"));
        }
        if (ctx.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        let guard = ctx.transaction.guard();
        ctx.transaction.validate_guard(&guard, live)?;
        self.selected.guard = guard;
        Ok(self.selected)
    }
}

pub(in crate::world) use target::with_current_root_module_meter_v1;

pub(in crate::world) use target::with_current_root_history_index_meter_v1;


pub(in crate::world) use target::PreparedFactoryModuleFollowupV1;



pub(in crate::world) use enumerate::{
    current_registry_active_keys_with_work_v1, current_registry_record_keys_v1,
};
