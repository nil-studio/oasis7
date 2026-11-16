//! Actual registry, Current manifest and original JSON/SHA release commitment.
use super::instance_current::{CurrentModuleInstanceFactsV1, ModuleInstanceFactsFeesV1};
use super::registry_rules::{ModuleRegistryFactsV1, ModuleRegistryRuleErrorV1};
use super::*;
use crate::world::{
    incremental_module_invocation::AcceptedInvocationDirectoryV1,
    incremental_normal_action_factory_schedule::FactoryScheduleReadErrorV1,
};
use oasis7_formal_protocol::incremental_value::ValueNodeReader;
use oasis7_world_state::{
    incremental_field_overlay::WorldFieldOverlayContextV1,
    incremental_field_read::WorldFieldReadContextV1,
};

pub(super) struct PaidCurrentReleaseShadowHashV1 {
    result: Result<ReservedValue<String>, ReservedValue<String>>,
    facts: ModuleInstanceFactsFeesV1,
    _phase: MemoryReservation,
}
impl PaidCurrentReleaseShadowHashV1 {
    pub(super) fn result(&self) -> Result<&str, &str> {
        self.result
            .as_ref()
            .map(|value| value.as_str())
            .map_err(|error| error.as_str())
    }
    pub(super) fn into_parts(
        self,
    ) -> (
        Result<ReservedValue<String>, ReservedValue<String>>,
        ModuleInstanceFactsFeesV1,
        MemoryReservation,
    ) {
        (self.result, self.facts, self._phase)
    }
}
#[derive(Debug)]
struct WorldReadCause(crate::WorldError);
impl std::fmt::Display for WorldReadCause {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.0)
    }
}
impl std::error::Error for WorldReadCause {}
pub(super) enum Failure {
    Body(BodyMutationError),
    Business(String),
}
impl From<ValueTreeError> for Failure {
    fn from(error: ValueTreeError) -> Self {
        Self::Body(error.into())
    }
}
impl From<BodyMutationError> for Failure {
    fn from(error: BodyMutationError) -> Self {
        Self::Body(error)
    }
}
fn infrastructure(error: FactoryScheduleReadErrorV1) -> Failure {
    match error {
        FactoryScheduleReadErrorV1::Body(error) => Failure::Body(error),
        FactoryScheduleReadErrorV1::World(error) => {
            Failure::Body(BodyMutationError::Original(Box::new(WorldReadCause(error))))
        }
    }
}
fn validation(
    error: ModuleRegistryRuleErrorV1<FactoryScheduleReadErrorV1>,
    label: &str,
) -> Failure {
    match error {
        ModuleRegistryRuleErrorV1::World(error)
        | ModuleRegistryRuleErrorV1::Source(FactoryScheduleReadErrorV1::World(error)) => {
            Failure::Business(format!("{label}{error:?}"))
        }
        ModuleRegistryRuleErrorV1::Source(FactoryScheduleReadErrorV1::Body(error)) => {
            Failure::Body(error)
        }
    }
}
fn json<T: serde::Serialize + ?Sized>(
    value: &T,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<super::governance_json::PaidModuleJsonValueV1, Failure> {
    match super::governance_json::module_json_value_paid_v1(value, common) {
        Ok(value) => Ok(value),
        Err(super::governance_json::ModuleGovernanceJsonErrorV1::Body(error)) => {
            Err(Failure::Body(error))
        }
        Err(super::governance_json::ModuleGovernanceJsonErrorV1::Json { error, .. }) => Err(
            Failure::Business(format!("module release shadow serialize failed: {error}")),
        ),
    }
}
#[allow(clippy::too_many_arguments)]
pub(super) fn evaluate_current_release_shadow_hash_v1<M, P, R, E, C, PE, PC>(
    manifest: &crate::ModuleManifest,
    activate: bool,
    profiles: &crate::ModuleProfileChanges,
    sources: &ModuleLifecycleSourcesV1<'_>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, R, PE, PC>,
    factory: &mut FactoryScheduleWorkV1,
    module: &mut ModuleLifecycleWorkV1<'_>,
    common: &mut FormalBoundedContextV1<'_>,
    directory: &ReservedValue<AcceptedInvocationDirectoryV1>,
    live: &ExecutionFence,
) -> Result<PaidCurrentReleaseShadowHashV1, BodyMutationError>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
{
    let input = match json(&(manifest, profiles), common) {
        Ok(value) => value,
        Err(Failure::Body(error)) => return Err(error),
        Err(Failure::Business(_)) => return Err(invalid()),
    };
    let heap = input
        .fee
        .bytes()
        .checked_mul(8)
        .and_then(|n| n.checked_add(65536))
        .ok_or_else(invalid)?;
    let count = profiles
        .product_profiles
        .len()
        .checked_add(profiles.recipe_profiles.len())
        .and_then(|n| n.checked_add(profiles.factory_profiles.len()))
        .and_then(|n| n.checked_add(1))
        .ok_or_else(invalid)?;
    let levels = usize::BITS as usize - count.leading_zeros() as usize + 1;
    pay(common, heap.checked_mul(levels).ok_or_else(invalid)?)?;
    let phase = common
        .memory()
        .reserve(heap, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let mut source = CurrentModuleInstanceFactsV1::new(
        sources, ctx, parent, factory, module, common, live, directory,
    )
    .map_err(|error| match infrastructure(error) {
        Failure::Body(error) => error,
        _ => invalid(),
    })?;
    let computed = (|| -> Result<ReservedValue<String>, Failure> {
        let mut changes = crate::ModuleChangeSet::default();
        let key =
            oasis7_wasm_abi::ModuleRegistry::record_key(&manifest.module_id, &manifest.version);
        if let Some(record) = source.manifest(&key).map_err(infrastructure)? {
            if record.manifest() != manifest {
                return Err(Failure::Business(format!(
                    "module release shadow rejected: existing manifest mismatch for {key}"
                )));
            }
        } else {
            changes.register.push(manifest.clone());
        }
        if activate {
            let active = source
                .active_version(&manifest.module_id)
                .map_err(infrastructure)?;
            if active
                .as_ref()
                .is_none_or(|version| version.as_str() != manifest.version)
            {
                changes.activate.push(crate::ModuleActivation {
                    module_id: manifest.module_id.clone(),
                    version: manifest.version.clone(),
                });
            }
        }
        let commitment =
            release_profile_commitment_paid_v1(manifest, activate, profiles, source.common_v1())?;
        if changes.is_empty() && commitment.is_none() {
            let before = source.world_manifest_v1().map_err(infrastructure)?;
            let (current, _fee) = before.owned_v1(source.common_v1())?;
            return Ok(
                crate::world::incremental_normal_action_diplomacy::fixed_json_digest_v1(
                    &current,
                    source.common_v1(),
                )?,
            );
        }
        super::registry_rules::validate_module_changes_from_source_v1(&mut source, &changes)
            .map_err(|error| validation(error, "module release shadow validate failed: "))?;
        super::manifest_rules::shadow_validate_module_changes_from_source_v1(&mut source, &changes)
            .map_err(|error| validation(error, "module release shadow dry-run failed: "))?;
        let change_value = json(&changes, source.common_v1())?;
        let before = source.world_manifest_v1().map_err(infrastructure)?;
        let (mut update, _clone) = before.owned_v1(source.common_v1())?;
        update.version = update.version.saturating_add(1);
        let serde_json::Value::Object(content) = &mut update.content else {
            return Err(Failure::Business(
                "module release shadow rejected: current manifest content must be object"
                    .to_owned(),
            ));
        };
        let extra =
            oasis7_world_state::incremental_collection_layout::json_object_output_heap_v1(2)?
                .checked_mul(usize::BITS as usize)
                .and_then(|n| n.checked_add(8192))
                .ok_or_else(invalid)?;
        pay(source.common_v1(), extra)?;
        let _map = source
            .common_v1()
            .memory()
            .reserve(extra, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        content.insert("module_changes".to_owned(), change_value.value);
        if let Some(commitment) = commitment {
            content.insert(
                "module_profile_commitment".to_owned(),
                serde_json::Value::String(commitment.to_string()),
            );
        }
        Ok(
            crate::world::incremental_normal_action_diplomacy::fixed_json_digest_v1(
                &update,
                source.common_v1(),
            )?,
        )
    })();
    let facts = source.into_parts();
    let result = match computed {
        Ok(hash) => Ok(hash),
        Err(Failure::Body(error)) => return Err(error),
        Err(Failure::Business(error)) => {
            let bytes = error
                .len()
                .checked_add(std::mem::size_of::<String>())
                .ok_or_else(invalid)?;
            let fee = common
                .memory()
                .reserve(bytes, MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?;
            Err(fee.retain(error))
        }
    };
    drop(input);
    Ok(PaidCurrentReleaseShadowHashV1 {
        result,
        facts,
        _phase: phase,
    })
}

/// Same sorted profile payload as the original release commitment, with each
/// Value/DTO copy admitted before allocation and SHA streamed through Common.
pub(super) fn release_profile_commitment_paid_v1(
    manifest: &crate::ModuleManifest,
    activate: bool,
    profiles: &crate::ModuleProfileChanges,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<Option<ReservedValue<String>>, Failure> {
    if profiles.is_empty() {
        return Ok(None);
    }
    let measured = json(&(manifest, profiles), common)?;
    let heap = measured
        .fee
        .bytes()
        .checked_mul(4)
        .and_then(|n| n.checked_add(8192))
        .ok_or_else(invalid)?;
    let count = profiles
        .product_profiles
        .len()
        .checked_add(profiles.recipe_profiles.len())
        .and_then(|n| n.checked_add(profiles.factory_profiles.len()))
        .and_then(|n| n.checked_add(1))
        .ok_or_else(invalid)?;
    let levels = usize::BITS as usize - count.leading_zeros() as usize + 1;
    pay(common, heap.checked_mul(levels).ok_or_else(invalid)?)?;
    let _copy = common
        .memory()
        .reserve(heap, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let mut normalized = profiles.clone();
    normalized
        .product_profiles
        .sort_by(|a, b| a.product_id.cmp(&b.product_id));
    normalized
        .recipe_profiles
        .sort_by(|a, b| a.recipe_id.cmp(&b.recipe_id));
    normalized
        .factory_profiles
        .sort_by(|a, b| a.factory_id.cmp(&b.factory_id));
    #[derive(serde::Serialize)]
    struct Payload<'a> {
        module_manifest: &'a crate::ModuleManifest,
        activate: bool,
        profile_changes: &'a crate::ModuleProfileChanges,
    }
    let value = json(
        &Payload {
            module_manifest: manifest,
            activate,
            profile_changes: &normalized,
        },
        common,
    )?;
    Ok(Some(
        crate::world::incremental_normal_action_diplomacy::fixed_json_digest_v1(
            &value.value,
            common,
        )?,
    ))
}
