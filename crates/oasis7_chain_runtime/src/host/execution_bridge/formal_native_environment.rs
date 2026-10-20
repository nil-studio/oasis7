//! Native recovery dependencies are explicit external typed roots. Capturing
//! bytes or staging chunks grants neither Frozen nor serving authority.
use super::{controlled_capture as capture, formal_package, to_cbor};
use oasis7_distfs::controlled_authority::{
    formal::{
        FormalFrozenBodyV1, coordinator::FormalDurabilityEndpoint,
        endpoint::FormalCandidateMaterialV1, frozen_material_digest, verify_frozen_material,
    },
    replicated_protocol::{ArtifactRole, ClosedRecord},
};
use oasis7_formal_protocol::native_environment::NativeExecutionEnvironmentV1;
use std::collections::BTreeMap;

/// Published cleanup must also release native readback jobs which a retained
/// Qualified retry did not consume through Prepare. The caller retains its
/// durable ReleasePending obligation until every idempotent acknowledgement.
pub(crate) fn release_unused_verification<
    P: FormalDurabilityEndpoint,
    R: FormalDurabilityEndpoint,
>(
    primary: &mut P,
    replica: &mut R,
    candidate: &FormalCandidateMaterialV1,
) -> Result<(), String> {
    let mut ids = std::collections::BTreeSet::new();
    for material in std::iter::once(&candidate.material).chain(candidate.frozen_material.iter()) {
        if let Some(environment) = material
            .native_environment()
            .map_err(|error| error.to_string())?
        {
            ids.insert(environment.digest()?);
        }
    }
    for id in ids {
        primary
            .release_native_environment_verification(&id)
            .map_err(|error| error.to_string())?;
        replica
            .release_native_environment_verification(&id)
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

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
    let chunks = native
        .components
        .iter()
        .map(|component| component.chunks.len() as u64)
        .sum::<u64>();
    let mut endpoints = [
        primary as &mut dyn FormalDurabilityEndpoint,
        replica as &mut dyn FormalDurabilityEndpoint,
    ];
    verify_progress(&descriptor_digest, chunks, |index| {
        endpoints[index]
            .advance_native_environment_verification(native)
            .map_err(|error| error.to_string())
    })
}
fn verify_progress(
    descriptor_digest: &str,
    chunks: u64,
    mut advance: impl FnMut(
        usize,
    ) -> Result<
        oasis7_distfs::controlled_authority::formal::NativeEnvironmentVerificationProgressV1,
        String,
    >,
) -> Result<(), String> {
    let mut last = [None, None];
    let mut instances = [None, None];
    let mut complete = [false, false];
    // Interleave P/R. Renew a completed peer's volatile job while the other
    // advances, so a large but progressing readback cannot age out its pin.
    // Calls remain bounded by chunks+1 per endpoint, including renewals.
    for _ in 0..=chunks {
        for index in 0..2 {
            let progress = advance(index)?;
            if progress.descriptor_digest != descriptor_digest
                || instances[index]
                    .as_ref()
                    .is_some_and(|previous| previous != &progress.endpoint_instance)
                || progress.verified_chunks > chunks
                || (progress.complete && progress.verified_chunks != chunks)
                || last[index].is_some_and(|previous| {
                    if complete[index] {
                        !progress.complete || progress.verified_chunks != previous
                    } else {
                        progress.verified_chunks <= previous
                    }
                })
            {
                return Err("native independent verification made no authenticated progress or lost its live job".into());
            }
            last[index] = Some(progress.verified_chunks);
            instances[index] = Some(progress.endpoint_instance);
            complete[index] = progress.complete;
        }
        if complete == [true, true] {
            return Ok(());
        }
    }
    Err("Unavailable: native environment verification operation budget".into())
}

#[cfg(test)]
#[path = "formal_native_environment_tests.rs"]
mod tests;
