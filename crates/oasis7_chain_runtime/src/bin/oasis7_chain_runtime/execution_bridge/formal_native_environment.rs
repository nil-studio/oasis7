//! Native recovery dependencies are explicit external typed roots. Capturing
//! bytes or staging chunks grants neither Frozen nor serving authority.
use super::{controlled_capture as capture, formal_package, to_cbor};
use oasis7_distfs::controlled_authority::{
    formal::{
        FormalFrozenBodyV1, coordinator::FormalDurabilityEndpoint, frozen_material_digest,
        verify_frozen_material,
    },
    replicated_protocol::{ArtifactRole, ClosedRecord},
};
use oasis7_formal_protocol::native_environment::NativeExecutionEnvironmentV1;
use std::collections::BTreeMap;

pub(crate) fn descriptor(
    material: &ClosedRecord,
) -> Result<Option<NativeExecutionEnvironmentV1>, String> {
    let Some(id) = material
        .roots
        .get(&ArtifactRole::NativeExecutionEnvironment)
    else {
        return Ok(None);
    };
    let object = material
        .objects
        .iter()
        .find(|object| &object.content_hash == id)
        .ok_or("native environment descriptor missing from signed closure")?;
    if blake3::hash(&object.bytes).to_hex().as_str() != id || !object.references.is_empty() {
        return Err("native descriptor bytes or external-reference encoding".into());
    }
    NativeExecutionEnvironmentV1::decode(&object.bytes).map(Some)
}

pub(super) fn verify_current(
    material: &ClosedRecord,
    manifest: &formal_package::FormalExecutionManifestV1,
) -> Result<(), String> {
    match (
        manifest.schema_version,
        &manifest.native_environment_ref,
        descriptor(material)?,
    ) {
        (1, None, None) => Ok(()),
        (2, Some(id), Some(original)) if original.digest()? == *id => {
            if !original.components.iter().any(|component| {
                component.role
                    == oasis7_formal_protocol::native_environment::NativeEnvironmentRole::Executable
                    && component.content_digest == manifest.executable_digest
            }) {
                return Err("native descriptor executable differs from Frozen manifest".into());
            }
            let current = crate::native_environment_capture::capture(|_, _| Ok(()))?;
            if original != current {
                return Err("Unavailable: current native environment differs from original Frozen environment".into());
            }
            Ok(())
        }
        _ => Err("formal manifest native environment schema/root mismatch".into()),
    }
}

pub(super) fn attach_frozen(
    body: &mut FormalFrozenBodyV1,
    material: &mut ClosedRecord,
    native: &NativeExecutionEnvironmentV1,
) -> Result<(), String> {
    build_frozen_attachment_unverified(body, material, native)?;
    verify_frozen_material(body, material).map_err(|error| error.to_string())
}

/// A restore-only attachment must match the token's fully verified original
/// before this function can return. No unchecked builder is exposed to callers.
pub(super) fn attach_frozen_for_exact_restore(
    body: &mut FormalFrozenBodyV1,
    material: &mut ClosedRecord,
    native: &NativeExecutionEnvironmentV1,
    verified: &formal_package::VerifiedFrozenSnapshot<'_>,
) -> Result<(), String> {
    build_frozen_attachment_unverified(body, material, native)?;
    verified.verify_exact_rebuild(body, material)
}

fn build_frozen_attachment_unverified(
    body: &mut FormalFrozenBodyV1,
    material: &mut ClosedRecord,
    native: &NativeExecutionEnvironmentV1,
) -> Result<(), String> {
    let mut manifest: formal_package::FormalExecutionManifestV1 =
        capture::decode_role(material, ArtifactRole::ExecutionManifest)?;
    if manifest.schema_version != 1 || manifest.native_environment_ref.is_some() {
        return Err("native attachment requires the original unextended Frozen builder".into());
    }
    native.validate()?;
    if !native.components.iter().any(|component| {
        component.role
            == oasis7_formal_protocol::native_environment::NativeEnvironmentRole::Executable
            && component.content_digest == manifest.executable_digest
    }) {
        return Err("native archive executable differs from Frozen execution policy".into());
    }
    let mut objects: BTreeMap<_, _> = material
        .objects
        .iter()
        .map(|object| (object.content_hash.clone(), object.clone()))
        .collect();
    let old_manifest = material
        .roots
        .get(&ArtifactRole::ExecutionManifest)
        .ok_or("Frozen execution manifest missing")?
        .clone();
    let native_id = formal_package::insert(&mut objects, native.encode()?, vec![]);
    material.roots.remove(&ArtifactRole::ExecutionManifest);
    material
        .roots
        .insert(ArtifactRole::NativeExecutionEnvironment, native_id.clone());
    manifest.schema_version = 2;
    manifest.domain = "oasis7.formal.execution_input.v2".into();
    manifest.native_environment_ref = Some(native_id);
    manifest.artifact_roots = material.roots.clone();
    objects.remove(&old_manifest);
    let manifest_id = formal_package::insert(
        &mut objects,
        to_cbor(&manifest)?,
        material.roots.values().cloned().collect(),
    );
    material
        .roots
        .insert(ArtifactRole::ExecutionManifest, manifest_id.clone());
    material.objects = objects.into_values().collect();
    body.schema_version = 3;
    body.runtime_manifest_digest = manifest_id;
    body.frozen_material_digest =
        frozen_material_digest(material).map_err(|error| error.to_string())?;
    Ok(())
}

/// First determine the full descriptor; then stage a second exact capture.
/// Changed mappings or bytes leave only unselected CAS data, never a Frozen.
pub(crate) fn stage_current<P: FormalDurabilityEndpoint, R: FormalDurabilityEndpoint>(
    primary: &mut P,
    replica: &mut R,
) -> Result<NativeExecutionEnvironmentV1, String> {
    use oasis7_distfs::controlled_authority::formal::native_environment::NativeEnvironmentChunkBytesV1;
    let native = crate::native_environment_capture::capture(|_, _| Ok(()))?;
    let mut batch = Vec::new();
    let mut flush = |batch: &mut Vec<NativeEnvironmentChunkBytesV1>| -> Result<(), String> {
        if batch.is_empty() {
            return Ok(());
        }
        primary
            .stage_native_environment(&native, batch)
            .map_err(|error| error.to_string())?;
        replica
            .stage_native_environment(&native, batch)
            .map_err(|error| error.to_string())?;
        batch.clear();
        Ok(())
    };
    let second = crate::native_environment_capture::capture(|digest, bytes| {
        batch.push(NativeEnvironmentChunkBytesV1 {
            digest: digest.into(),
            bytes: bytes.to_vec(),
        });
        if batch.len() == 8 {
            flush(&mut batch)?;
        }
        Ok(())
    })?;
    flush(&mut batch)?;
    if native != second {
        return Err("Unavailable: native environment changed between capture and staging".into());
    }
    verify_registered(primary, replica, &native)?;
    Ok(native)
}

pub(crate) fn verify_registered<P: FormalDurabilityEndpoint, R: FormalDurabilityEndpoint>(
    primary: &mut P,
    replica: &mut R,
    native: &NativeExecutionEnvironmentV1,
) -> Result<(), String> {
    let descriptor_digest = native.digest()?;
    let maximum_calls = native
        .components
        .iter()
        .map(|component| component.chunks.len() as u64)
        .sum::<u64>()
        + 1;
    for endpoint in [
        primary as &mut dyn FormalDurabilityEndpoint,
        replica as &mut dyn FormalDurabilityEndpoint,
    ] {
        let mut last = None;
        let mut instance = None;
        let mut complete = false;
        for _ in 0..maximum_calls {
            let progress = endpoint
                .advance_native_environment_verification(native)
                .map_err(|error| error.to_string())?;
            if progress.descriptor_digest != descriptor_digest
                || instance
                    .as_ref()
                    .is_some_and(|previous| previous != &progress.endpoint_instance)
                || last.is_some_and(|previous| progress.verified_chunks <= previous)
            {
                return Err(
                    "native independent verification made no authenticated progress".into(),
                );
            }
            if progress.complete {
                complete = true;
                break;
            }
            last = Some(progress.verified_chunks);
            instance = Some(progress.endpoint_instance);
        }
        if !complete {
            return Err("Unavailable: native environment verification operation budget".into());
        }
    }
    Ok(())
}
