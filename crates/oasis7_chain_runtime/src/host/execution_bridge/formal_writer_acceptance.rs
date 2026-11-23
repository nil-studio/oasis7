//! Original, immutable inputs for the native process acceptance harness.
use super::*;

pub(crate) fn original_frozen(
    root: &Path,
    position: u64,
) -> Result<(SignedFormalFrozenV1, ClosedRecord), String> {
    let pending_path = root.join("pending.cbor");
    let retained = if pending_path.exists() {
        let pending: Pending = read(&pending_path)?;
        (pending.original.body.next_position == position)
            .then_some((pending.original, pending.material))
    } else {
        None
    };
    let (input, material) = match retained {
        Some(retained) => retained,
        None => {
            let (candidate, _) = read_published_candidate_at(root, position)?
                .ok_or("CA-R1 original publication is missing")?;
            (
                candidate
                    .frozen
                    .ok_or("CA-R1 original reservation is missing")?
                    .reservation,
                candidate
                    .frozen_material
                    .ok_or("CA-R1 original Frozen closure is missing")?,
            )
        }
    };
    if input.body.next_position != position {
        return Err("CA-R1 original input position changed".into());
    }
    verify_frozen_material(&input.body, &material).map_err(|error| error.to_string())?;
    Ok((input, material))
}

/// Mutations retain the signed original commitments and therefore must be
/// refused by a real owner. Each output changes one named original dependency;
/// the harness restores the exact original file before its positive reopen.
pub(crate) fn corrupted_pending(
    path: &Path,
    prepared: bool,
    writer: &dyn FormalWriterSignerV1,
) -> Result<Vec<(&'static str, Vec<u8>)>, String> {
    let original: Pending = read(path)?;
    verify_frozen_material(&original.original.body, &original.material)
        .map_err(|error| error.to_string())?;
    let mut variants = Vec::new();
    if prepared {
        let mut wrong = original.clone();
        let candidate = wrong
            .candidate
            .as_mut()
            .ok_or("CA-R2 prepared candidate missing")?;
        candidate.candidate.record.after_state_root = "ab".repeat(32);
        candidate.candidate.writer_signature_hex = writer
            .sign_candidate(&candidate.candidate.record, candidate.frozen.as_ref())
            .map_err(|error| error.to_string())?;
        variants.push(("prepared-result-root", to_cbor(&wrong)?));

        let mut wrong = original.clone();
        let candidate = wrong.candidate.as_mut().unwrap();
        let mut snapshot: Snapshot =
            capture::decode_snapshot_role(&candidate.material, ArtifactRole::Snapshot)?;
        let last = snapshot
            .tick_consensus_records
            .last_mut()
            .ok_or("CA-R2 actual prepared tick randomness is missing")?;
        last.block.header.randomness_seed = "ab".repeat(32);
        replace_role(
            &mut candidate.material,
            ArtifactRole::Snapshot,
            snapshot_to_cbor(&snapshot)?,
        )?;
        variants.push(("prepared-randomness", to_cbor(&wrong)?));
    } else {
        let mut wrong = original.clone();
        wrong.original.body.parent.record_id = "ab".repeat(32);
        resign_original(&mut wrong, writer)?;
        variants.push(("original-parent", to_cbor(&wrong)?));
        let mut wrong = original.clone();
        wrong.original.body.parent.position = wrong
            .original
            .body
            .parent
            .position
            .checked_add(1)
            .ok_or("CA-R2 parent overflow")?;
        wrong.original.body.next_position = wrong
            .original
            .body
            .next_position
            .checked_add(1)
            .ok_or("CA-R2 next position overflow")?;
        resign_original(&mut wrong, writer)?;
        variants.push(("higher-parent-head", to_cbor(&wrong)?));

        for (role, label) in [
            (
                ArtifactRole::NativeExecutionEnvironment,
                "native-descriptor-missing",
            ),
            (ArtifactRole::NonceIndex, "original-nonce-missing"),
            (
                ArtifactRole::ExecutionManifest,
                "original-execution-manifest-missing",
            ),
        ] {
            let mut wrong = original.clone();
            let id = wrong
                .material
                .roots
                .get(&role)
                .ok_or("CA-R2 required original role missing in fixture")?
                .clone();
            let count = wrong.material.objects.len();
            wrong
                .material
                .objects
                .retain(|object| object.content_hash != id);
            if wrong.material.objects.len() + 1 != count {
                return Err("CA-R2 original role object is not unique".into());
            }
            variants.push((label, to_cbor(&wrong)?));
        }
        let mut wrong = original.clone();
        let wasm = &wrong.material.roots[&ArtifactRole::Wasm];
        let artifact = wrong
            .material
            .objects
            .iter()
            .find(|object| &object.content_hash == wasm)
            .and_then(|object| object.references.first())
            .ok_or("CA-R2 real WASM artifact reference missing")?
            .clone();
        let object = wrong
            .material
            .objects
            .iter_mut()
            .find(|object| object.content_hash == artifact)
            .ok_or("CA-R2 real WASM artifact bytes missing")?;
        if !object.bytes.starts_with(b"\0asm") {
            return Err("CA-R2 expected original WebAssembly binary".into());
        }
        object.bytes[0] ^= 1;
        variants.push(("original-wasm-bytes", to_cbor(&wrong)?));

        let mut wrong = original.clone();
        let mut snapshot: Snapshot =
            capture::decode_snapshot_role(&wrong.material, ArtifactRole::Snapshot)?;
        snapshot.state.time = snapshot
            .state
            .time
            .checked_add(1)
            .ok_or("CA-R2 clock overflow")?;
        replace_role(
            &mut wrong.material,
            ArtifactRole::Snapshot,
            snapshot_to_cbor(&snapshot)?,
        )?;
        variants.push(("original-world-clock", to_cbor(&wrong)?));

        let mut wrong = original.clone();
        let mut manifest: formal_package::FormalExecutionManifestV1 =
            capture::decode_role(&wrong.material, ArtifactRole::ExecutionManifest)?;
        manifest.continuation.context.committed_at_unix_ms = manifest
            .continuation
            .context
            .committed_at_unix_ms
            .checked_add(1)
            .ok_or("CA-R2 committed clock overflow")?;
        replace_role(
            &mut wrong.material,
            ArtifactRole::ExecutionManifest,
            to_cbor(&manifest)?,
        )?;
        variants.push(("original-committed-clock", to_cbor(&wrong)?));
    }
    Ok(variants)
}

fn resign_original(pending: &mut Pending, writer: &dyn FormalWriterSignerV1) -> Result<(), String> {
    // A valid old-writer signature cannot substitute a different original
    // parent. The failure must bind the saved material, not malformed signing.
    pending.original.signature_hex = writer
        .sign_frozen(&pending.original.body)
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn replace_role(
    material: &mut ClosedRecord,
    role: ArtifactRole,
    bytes: Vec<u8>,
) -> Result<(), String> {
    let id = material.roots.get(&role).ok_or("CA-R2 role missing")?;
    let object = material
        .objects
        .iter_mut()
        .find(|object| &object.content_hash == id)
        .ok_or("CA-R2 role object missing")?;
    if object.bytes == bytes {
        return Err("CA-R2 mutation did not change the original".into());
    }
    object.bytes = bytes;
    Ok(())
}
