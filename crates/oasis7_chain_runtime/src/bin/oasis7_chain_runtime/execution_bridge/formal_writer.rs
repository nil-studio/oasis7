//! Native writer transaction boundary; only dual qualified results advance World.
use super::{
    controlled_capture as capture, durable_transaction, formal_executor, formal_package, to_cbor,
};
use ed25519_dalek::{Signer, SigningKey};
use oasis7_distfs::controlled_authority::{
    formal::{
        coordinator::{FormalCoordinator, FormalDurabilityEndpoint},
        endpoint::{
            FormalCandidateMaterialV1, FormalRequestStateV1, VerifiedNativeCheckpointRestoreV1,
        },
        *,
    },
    replicated_protocol::{ArtifactRole, ClosedRecord},
};
use oasis7_node::NodeLocalExecutionContinuation;
use oasis7_runtime::{Journal, ReleaseSecurityPolicy, Snapshot, World, blake3_hex};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[cfg(test)]
#[path = "formal_writer_tests.rs"]
mod tests;

#[path = "formal_writer_attempts.rs"]
mod attempts;
#[path = "formal_writer_initial_history.rs"]
mod initial_history;
#[path = "formal_writer_recovery.rs"]
mod recovery_runtime;
#[path = "formal_writer_saved_candidate.rs"]
mod saved_candidate;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Pending {
    schema_version: u32,
    original: SignedFormalFrozenV1,
    material: ClosedRecord,
    candidate: Option<FormalCandidateMaterialV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    frozen_certificate: Option<FormalFrozenCertificateV1>,
}
#[cfg(all(test, unix))]
pub(crate) fn pending_test_block_hash(path: &Path) -> Result<String, String> {
    let pending: Pending = read(path)?;
    verify_frozen_material(&pending.original.body, &pending.material).map_err(|e| e.to_string())?;
    let manifest: super::formal_package::FormalExecutionManifestV1 =
        super::controlled_capture::decode_role(&pending.material, ArtifactRole::ExecutionManifest)?;
    if manifest
        .continuation
        .context
        .node_block_hash
        .trim()
        .is_empty()
    {
        return Err("test original frozen Node block hash missing".into());
    }
    Ok(manifest.continuation.context.node_block_hash)
}
#[cfg(all(test, unix))]
pub(crate) fn pending_test_position(path: &Path) -> Result<u64, String> {
    let pending: Pending = read(path)?;
    verify_frozen_material(&pending.original.body, &pending.material).map_err(|e| e.to_string())?;
    Ok(pending.original.body.next_position)
}

#[cfg(all(test, unix))]
pub(crate) fn retirement_test_target(root: &Path, sidecar: &Path) -> Result<(String, u64), String> {
    let (schema, position, block, digest): (u32, u64, String, String) = read(sidecar)?;
    let (candidate, proof) = read_published_candidate_at(root, position)?
        .ok_or("retired original publication missing")?;
    if schema != 1
        || position < 2
        || candidate.candidate.record.position != position
        || proof.decision.record != candidate.candidate.record
        || super::formal_attempt_lifecycle::candidate_digest(&candidate)? != digest
    {
        return Err("retired original candidate/publication binding differs".into());
    }
    let frozen = candidate
        .frozen_material
        .as_ref()
        .ok_or("retired original Frozen missing")?;
    let reservation = &candidate
        .frozen
        .as_ref()
        .ok_or("retired original reservation missing")?
        .reservation
        .body;
    verify_frozen_material(reservation, frozen).map_err(|error| error.to_string())?;
    let manifest: super::formal_package::FormalExecutionManifestV1 =
        super::controlled_capture::decode_role(frozen, ArtifactRole::ExecutionManifest)?;
    if manifest.continuation.context.node_block_hash != block
        || block.trim().is_empty()
        || reservation.next_position != position
    {
        return Err("retired original Frozen continuation binding differs".into());
    }
    Ok((block, position))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Published {
    schema_version: u32,
    candidate: FormalCandidateMaterialV1,
    proof: FormalQualifiedCommitProofV1,
}
struct NativePublication {
    publication: Published,
    pending: Option<Pending>,
    recovery: Option<VerifiedRecovery>,
}
fn restored_pending(state: Option<&FormalRequestStateV1>) -> Result<Option<Pending>, String> {
    let Some(state) = state else {
        return Ok(None);
    };
    if let FormalRequestStateV1::Frozen {
        input,
        material,
        certificate,
        ..
    } = state
    {
        return Ok(Some(Pending {
            schema_version: 1,
            original: input.clone(),
            material: material.clone(),
            candidate: None,
            frozen_certificate: certificate.clone(),
        }));
    }
    let candidate = state
        .candidate()
        .ok_or("native pending candidate missing")?;
    Ok(Some(Pending {
        schema_version: 1,
        original: candidate
            .frozen
            .as_ref()
            .ok_or("native pending lacks original Frozen certificate")?
            .reservation
            .clone(),
        material: candidate
            .frozen_material
            .clone()
            .ok_or("native pending lacks original frozen material")?,
        candidate: Some(candidate.clone()),
        frozen_certificate: candidate.frozen.clone(),
    }))
}
/// Storage is a cache of proof-bearing facts. Restart revalidates each step
/// against the independently provided anchor, rather than trusting a local head.
pub(crate) struct FormalWriter<P: FormalDurabilityEndpoint, R: FormalDurabilityEndpoint> {
    _lock: std::fs::File,
    root: PathBuf,
    writer: SigningKey,
    release: ReleaseSecurityPolicy,
    world: World,
    anchor: VerifiedFormalReadAnchorV1,
    coordinator: FormalCoordinator<P, R>,
    pending: Option<Pending>,
    poisoned: bool,
    business_reader: Option<super::formal_business_package::SharedBusinessReader>,
    recovery: Option<VerifiedRecovery>,
    attempt_scan: Option<std::fs::ReadDir>,
    attempt_reclaim_current: Option<PathBuf>,
}
#[path = "formal_writer_published.rs"]
mod published;
pub(crate) use published::{read_published_candidate_at, read_published_proof_at};

#[path = "formal_writer_restore.rs"]
mod restoration;
use restoration::{restore, restore_qualified};

fn read<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, String> {
    let bytes = crate::controlled_history_cli::read_bounded(path, 64 * 1024 * 1024)?;
    published::decode(&bytes)
}

/// Extract immutable profile-2 CAS writes from the already approved initial
/// closure. The caller has independently verified the initial proof and pin;
/// these bytes only seed a hash-checking cache and do not authorize a cut.
pub(crate) fn initial_business_nodes(
    candidate: &FormalCandidateMaterialV1,
) -> Result<
    Vec<oasis7_distfs::controlled_authority::formal::business_index::BusinessIndexNodeWriteV1>,
    String,
> {
    if candidate.candidate.record.kind != FormalRecordKindV1::InitialActivation {
        return Err("formal initial business cache requires InitialActivation".into());
    }
    verify_record_material(&candidate.candidate.record, &candidate.material)
        .map_err(|error| error.to_string())?;
    let envelope = super::formal_business_package::envelope(&candidate.material, true)?
        .ok_or("formal initial business envelope missing")?;
    let writes: Vec<_> = envelope
        .delta
        .node_writes
        .into_iter()
        .chain(
            envelope
                .retained_history_delta
                .into_iter()
                .flat_map(|delta| delta.node_writes),
        )
        .collect();
    for node in &writes {
        if blake3::hash(&node.bytes).to_hex().as_str() != node.digest {
            return Err("formal initial business cache node hash mismatch".into());
        }
    }
    Ok(writes)
}

impl<P: FormalDurabilityEndpoint, R: FormalDurabilityEndpoint> FormalWriter<P, R> {
    #[cfg(test)]
    pub(crate) fn open(
        root: PathBuf,
        writer: SigningKey,
        release: ReleaseSecurityPolicy,
        world: World,
        anchor: VerifiedFormalReadAnchorV1,
        primary: P,
        replica: R,
    ) -> Result<Self, String> {
        Self::open_inner(root, writer, release, world, anchor, primary, replica, None)
    }
    /// Both live endpoints must independently restore the same qualified head.
    /// The supplied anchor remains the external origin of this read lineage.
    #[expect(
        clippy::too_many_arguments,
        reason = "Native opening binds independent trust, replica restores, endpoint handles, and an authenticated reader."
    )]
    pub(crate) fn open_native(
        root: PathBuf,
        writer: SigningKey,
        release: ReleaseSecurityPolicy,
        trusted: VerifiedFormalReadAnchorV1,
        primary_restore: &VerifiedNativeCheckpointRestoreV1,
        replica_restore: &VerifiedNativeCheckpointRestoreV1,
        primary: P,
        replica: R,
        reader: std::sync::Arc<dyn business_index::BusinessIndexNodeReader + Send + Sync>,
    ) -> Result<Self, String> {
        Self::open_native_context(
            root,
            writer,
            release,
            trusted,
            None,
            primary_restore,
            replica_restore,
            primary,
            replica,
            reader,
        )
    }
    #[cfg(test)]
    #[expect(
        clippy::too_many_arguments,
        reason = "The initialization fixture supplies the complete signed activation and endpoint pair explicitly."
    )]
    pub(crate) fn open_initial(
        root: PathBuf,
        writer: SigningKey,
        release: ReleaseSecurityPolicy,
        anchor: VerifiedFormalReadAnchorV1,
        candidate: FormalCandidateMaterialV1,
        proof: FormalQualifiedCommitProofV1,
        primary: P,
        replica: R,
    ) -> Result<Self, String> {
        if anchor.head().record().kind != FormalRecordKindV1::InitialActivation
            || anchor.head().record().position != 1
            || &candidate.candidate.record != anchor.head().record()
            || proof.decision.record != candidate.candidate.record
        {
            return Err("formal initialization differs from externally verified activation".into());
        }
        let verified = verify_formal_qualified(&proof, anchor.authority(), None, None)
            .map_err(|e| e.to_string())?;
        if verified.head() != anchor.head().head() {
            return Err("formal initialization proof differs from fixed activation".into());
        }
        let world = restore(&candidate, &release)?;
        Self::open_inner(
            root,
            writer,
            release,
            world,
            anchor,
            primary,
            replica,
            Some(NativePublication {
                publication: Published {
                    schema_version: 1,
                    candidate,
                    proof,
                },
                pending: None,
                recovery: None,
            }),
        )
    }
    #[expect(
        clippy::too_many_arguments,
        reason = "Writer construction keeps key, policy, qualified anchor, endpoint pair, and optional publication distinct."
    )]
    fn open_inner(
        root: PathBuf,
        writer: SigningKey,
        release: ReleaseSecurityPolicy,
        mut world: World,
        mut anchor: VerifiedFormalReadAnchorV1,
        primary: P,
        replica: R,
        native: Option<NativePublication>,
    ) -> Result<Self, String> {
        let recovery = native.as_ref().and_then(|native| native.recovery.clone());
        if hex::encode(writer.verifying_key().to_bytes())
            != anchor.authority().definition().writer_public_key
        {
            return Err("formal writer key differs from external authority".into());
        }
        if blake3_hex(&to_cbor(world.snapshot())?) != anchor.head().record().after_state_root {
            return Err("formal writer initial World differs from external anchor".into());
        }
        super::controlled_live::private_directory(&root)?;
        use std::os::fd::AsRawFd;
        use std::os::unix::fs::OpenOptionsExt;
        let lock = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(root.join("writer.lock"))
            .map_err(|e| e.to_string())?;
        if !lock.metadata().map_err(|e| e.to_string())?.is_file() {
            return Err("formal writer lock is not a regular file".into());
        }
        crate::controlled_live_config::require_private_owned(&root.join("writer.lock"), false)?;
        if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
            return Err("formal writer directory is occupied".into());
        }
        let published = root.join("published");
        super::controlled_live::private_directory(&published)?;
        if let Some(native) = &native {
            durable_transaction::write_private_file_durable(
                &published.join(format!(
                    "{:020}.cbor",
                    native.publication.candidate.candidate.record.position
                )),
                &to_cbor(&native.publication)?,
            )?;
        }
        // History files contain independent complete steps, never recursive
        // embeddings of all prior records. No directory listing sets authority.
        loop {
            if native.is_some() {
                break;
            }
            let next = anchor
                .head()
                .record()
                .position
                .checked_add(1)
                .ok_or("formal writer position overflow")?;
            let path = published.join(format!("{next:020}.cbor"));
            match std::fs::symlink_metadata(&path) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
                Err(error) => return Err(error.to_string()),
                Ok(meta) if !meta.is_file() || meta.file_type().is_symlink() => {
                    return Err("formal writer history file refused".into());
                }
                Ok(_) => {}
            }
            let stored: Published = read(&path)?;
            if stored.schema_version != 1
                || stored.candidate.candidate.record != stored.proof.decision.record
            {
                return Err("formal writer stored proof/candidate mismatch".into());
            }
            let head = verify_formal_qualified(
                &stored.proof,
                anchor.authority(),
                Some(anchor.head()),
                None,
            )
            .map_err(|e| e.to_string())?;
            world = restore(&stored.candidate, &release)?;
            anchor = anchor
                .after_verified_commit(anchor.authority().clone(), head)
                .map_err(|e| e.to_string())?;
        }
        if native.is_none() {
            for entry in std::fs::read_dir(&published).map_err(|e| e.to_string())? {
                let entry = entry.map_err(|e| e.to_string())?;
                let name = entry.file_name();
                let name = name.to_str().ok_or("formal writer history name encoding")?;
                if let Some(position) = name.strip_suffix(".cbor") {
                    if position.len() != 20 || !position.bytes().all(|b| b.is_ascii_digit()) {
                        return Err("formal writer history name malformed".into());
                    }
                    let position: u64 = position
                        .parse()
                        .map_err(|_| "formal writer history position overflow")?;
                    if position > anchor.head().record().position {
                        return Err(
                            "HistoryUnavailable: formal writer publication prefix has a gap".into(),
                        );
                    }
                }
            }
        }
        let path = root.join("pending.cbor");
        let mut pending = match std::fs::symlink_metadata(&path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(error.to_string()),
            Ok(meta) if !meta.is_file() || meta.file_type().is_symlink() => {
                return Err("formal writer pending file refused".into());
            }
            Ok(_) => Some(read::<Pending>(&path)?),
        };
        if let Some(remote) = native.as_ref().and_then(|native| native.pending.as_ref()) {
            if let Some(local) = &pending {
                if local.original != remote.original || local.material != remote.material {
                    return Err(
                        "native writer cache conflicts with authenticated pending original".into(),
                    );
                }
                if let (Some(local), Some(remote_candidate)) = (&local.candidate, &remote.candidate)
                    && (local.candidate != remote_candidate.candidate
                        || local.material != remote_candidate.material
                        || local.frozen != remote_candidate.frozen
                        || local.frozen_material != remote_candidate.frozen_material)
                {
                    return Err("native writer cache conflicts with authenticated candidate".into());
                }
            }
            // Native pending fixes the immutable original and its tail rank.
            // Reuse a checked current-writer candidate as local computation;
            // it still must obtain Prepare/Decision/Qualified at both endpoints.
            let cached = pending
                .as_ref()
                .map(|local| {
                    saved_candidate::checked_cached_candidate(
                        &anchor,
                        recovery.as_ref(),
                        local,
                        remote,
                    )
                })
                .transpose()?
                .flatten();
            let mut reconciled = remote.clone();
            if cached.is_some() {
                reconciled.candidate = cached;
            }
            pending = Some(reconciled);
            durable_transaction::write_private_file_durable(
                &path,
                &to_cbor(pending.as_ref().unwrap())?,
            )?;
        }
        if let (Some(local), Some(native)) = (pending.as_mut(), native.as_ref())
            && local.original.body.request_digest
                == native.publication.candidate.candidate.record.request_digest
        {
            if let Some(candidate) = &local.candidate
                && !recovery_runtime::same_candidate_payload(
                    candidate,
                    &native.publication.candidate,
                )
            {
                return Err(
                    "terminal native publication conflicts with cached signed candidate".into(),
                );
            }
            if native
                .publication
                .candidate
                .frozen
                .as_ref()
                .is_none_or(|cert| cert.reservation != local.original)
                || native.publication.candidate.frozen_material.as_ref() != Some(&local.material)
            {
                return Err(
                    "terminal native publication conflicts with cached Frozen input".into(),
                );
            }
            local.candidate = Some(native.publication.candidate.clone());
            local.frozen_certificate = native.publication.candidate.frozen.clone();
            durable_transaction::write_private_file_durable(&path, &to_cbor(local)?)?;
        }
        if let Some(pending) = &pending {
            if pending.schema_version != 1 {
                return Err("formal writer pending schema".into());
            }
            verify_frozen_material(&pending.original.body, &pending.material)
                .map_err(|e| e.to_string())?;
            // A publication may precede clearing the pending cache. That exact
            // original request is terminal; it cannot create another height.
            let record = anchor.head().record();
            if record.request_digest != pending.original.body.request_digest {
                verify_formal_reservation(
                    &pending.original,
                    recovery_runtime::pending_original_authority(
                        &anchor,
                        recovery.as_ref(),
                        pending,
                    )?,
                    anchor.head(),
                )
                .map_err(|e| e.to_string())?;
            } else if pending.candidate.as_ref().is_none_or(|c| {
                &c.candidate.record != record
                    || c.frozen
                        .as_ref()
                        .is_none_or(|f| f.reservation != pending.original)
                    || c.frozen_material.as_ref() != Some(&pending.material)
            }) {
                return Err(
                    "formal writer terminal pending differs from published original".into(),
                );
            }
        }
        let coordinator = FormalCoordinator::new(
            primary,
            replica,
            anchor.authority().clone(),
            recovery
                .as_ref()
                .map_or(anchor.authority(), |context| context.original_authority())
                .clone(),
            recovery.clone(),
            Some(anchor.head().clone()),
        )
        .map_err(|e| e.to_string())?;
        let mut instance = Self {
            _lock: lock,
            root,
            writer,
            release,
            world,
            anchor,
            coordinator,
            pending,
            poisoned: false,
            business_reader: None,
            recovery,
            attempt_scan: None,
            attempt_reclaim_current: None,
        };
        if let Some((candidate, proof)) =
            read_published_candidate_at(&instance.root, instance.anchor.head().record().position)?
        {
            instance.reclaim_published_attempts(&Published {
                schema_version: 1,
                candidate,
                proof,
            });
        }
        Ok(instance)
    }
    pub(crate) fn anchor(&self) -> &VerifiedFormalReadAnchorV1 {
        &self.anchor
    }
    pub(crate) fn world(&self) -> &World {
        &self.world
    }
    pub(crate) fn with_business_reader(
        mut self,
        reader: std::sync::Arc<dyn oasis7_distfs::controlled_authority::formal::business_index::BusinessIndexNodeReader + Send + Sync>,
    ) -> Result<Self, String> {
        if self.anchor.authority().definition().profile_version != 2 {
            return Err("formal business reader requires explicitly authorized profile 2".into());
        }
        if self.world.state().retained_history_root.is_some() {
            let bytes = to_cbor(self.world.snapshot())?;
            let parent = oasis7_runtime::VerifiedFormalRetainedHistoryParentV1::verify(
                &self.anchor,
                &bytes,
            )?;
            self.world
                .attach_verified_formal_history(parent, reader.clone())?;
        }
        self.business_reader = Some(super::formal_business_package::SharedBusinessReader(reader));
        Ok(self)
    }
    pub(crate) fn committed_business_nodes(
        &self,
    ) -> Result<
        Vec<oasis7_distfs::controlled_authority::formal::business_index::BusinessIndexNodeWriteV1>,
        String,
    > {
        let publication: Published = read(
            &self
                .root
                .join("published")
                .join(format!("{:020}.cbor", self.anchor.head().record().position)),
        )?;
        if publication.schema_version != 1
            || &publication.candidate.candidate.record != self.anchor.head().record()
            || publication.proof.decision.record != publication.candidate.candidate.record
        {
            return Err("formal business cache lacks exact qualified publication".into());
        }
        verify_record_material(
            &publication.candidate.candidate.record,
            &publication.candidate.material,
        )
        .map_err(|e| e.to_string())?;
        super::formal_business_package::tree(
            &publication.candidate.material,
            self.anchor.head().record(),
            &self.world,
            true,
        )?;
        let envelope =
            super::formal_business_package::envelope(&publication.candidate.material, true)?
                .ok_or("formal business envelope missing")?;
        let writes: Vec<_> = envelope
            .delta
            .node_writes
            .into_iter()
            .chain(
                envelope
                    .retained_history_delta
                    .into_iter()
                    .flat_map(|delta| delta.node_writes),
            )
            .collect();
        for node in &writes {
            if blake3::hash(&node.bytes).to_hex().as_str() != node.digest {
                return Err("formal business cache node hash mismatch".into());
            }
        }
        Ok(writes)
    }
    pub(crate) fn committed_execution_record(
        &self,
    ) -> Result<super::ExecutionBridgeRecord, String> {
        let publication: Published = read(
            &self
                .root
                .join("published")
                .join(format!("{:020}.cbor", self.anchor.head().record().position)),
        )?;
        if publication.schema_version != 1
            || &publication.candidate.candidate.record != self.anchor.head().record()
            || publication.proof.decision.record != publication.candidate.candidate.record
        {
            return Err("formal node bootstrap lacks exact qualified publication".into());
        }
        verify_record_material(
            &publication.candidate.candidate.record,
            &publication.candidate.material,
        )
        .map_err(|e| e.to_string())?;
        let record: super::ExecutionBridgeRecord =
            super::formal_business_package::execution_result(
                &publication.candidate.material,
                self.anchor.authority().definition().profile_version == 2,
            )
            .map_err(|_| {
                "formal initial activation requires explicit approved node bootstrap".to_owned()
            })?;
        if record.world_id != self.anchor.head().record().world.world_id
            || record.execution_state_root != self.anchor.head().record().after_state_root
        {
            return Err("formal node result/canonical root mismatch".into());
        }
        Ok(record)
    }
    pub(crate) fn committed_initial_bootstrap(
        &self,
    ) -> Result<oasis7_node::NodeExecutionBootstrap, String> {
        if self.anchor.head().record().kind != FormalRecordKindV1::InitialActivation {
            return Err("formal initial bootstrap requested from execution head".into());
        }
        let publication: Published = read(
            &self
                .root
                .join("published")
                .join(format!("{:020}.cbor", self.anchor.head().record().position)),
        )?;
        if publication.schema_version != 1
            || &publication.candidate.candidate.record != self.anchor.head().record()
            || publication.proof.decision.record != publication.candidate.candidate.record
        {
            return Err("formal initial bootstrap lacks exact qualified closure".into());
        }
        verify_record_material(
            &publication.candidate.candidate.record,
            &publication.candidate.material,
        )
        .map_err(|e| e.to_string())?;
        let result: super::formal_initial::InitialResultV1 =
            super::formal_business_package::execution_result(
                &publication.candidate.material,
                self.anchor.authority().definition().profile_version == 2,
            )
            .map_err(|_| "formal initial Node bootstrap was not externally committed".to_owned())?;
        let bootstrap = result.node_bootstrap;
        if result.schema_version != 1
            || result.domain != "oasis7.formal.initial_node_result.v1"
            || result.world != self.anchor.head().record().world
            || result.state_root != self.anchor.head().record().after_state_root
            || result.logical_tick != self.world.state().time
            || bootstrap.height != result.logical_tick
            || bootstrap.execution_state_root != result.state_root
            || bootstrap.consensus_block_hash.trim().is_empty()
            || bootstrap.execution_block_hash.trim().is_empty()
        {
            return Err(
                "formal initial Node bootstrap differs from authenticated World boundary".into(),
            );
        }
        Ok(oasis7_node::NodeExecutionBootstrap {
            height: bootstrap.height,
            consensus_block_hash: bootstrap.consensus_block_hash,
            execution_block_hash: bootstrap.execution_block_hash,
            execution_state_root: bootstrap.execution_state_root,
        })
    }
    fn persist(&mut self) -> Result<(), String> {
        let bytes = to_cbor(
            self.pending
                .as_ref()
                .ok_or("formal writer pending missing")?,
        )?;
        if bytes.len() > 64 * 1024 * 1024 {
            return Err("formal writer pending exceeds bound".into());
        }
        durable_transaction::write_private_file_durable(&self.root.join("pending.cbor"), &bytes)
    }
    /// Callers must resume an existing original before selecting new inputs.
    pub(crate) fn commit(
        &mut self,
        original: &NodeLocalExecutionContinuation,
    ) -> Result<FormalQualifiedCommitProofV1, String> {
        self.prepare(original)?;
        self.resume_original()
    }
    /// Persist and obtain both durable Frozen acknowledgements before handing
    /// the original continuation back to the node. This never executes World.
    pub(crate) fn prepare(
        &mut self,
        original: &NodeLocalExecutionContinuation,
    ) -> Result<(), String> {
        self.prepare_with_durable_set_transition(original, None)
    }

    /// Prepare the exact original request, optionally carrying the old
    /// writer's one-shot healthy durable-set transition inside its Frozen
    /// input. `None` on an existing pending request means resume its captured
    /// decision unchanged; it never strips an already Frozen transition.
    pub(crate) fn prepare_with_durable_set_transition(
        &mut self,
        original: &NodeLocalExecutionContinuation,
        transition: Option<&SignedNormalDurableSetTransitionV1>,
    ) -> Result<(), String> {
        let timing = super::formal_execution_timing::FormalExecutionTiming::begin(
            "freeze",
            original.context.height,
        );
        #[cfg(all(test, unix))]
        super::formal_crash_test_boundary::select_original_input(original)?;
        if self.poisoned {
            return Err("OutcomeUnknown: reopen and query original formal request".into());
        }
        if self.pending.is_none() {
            if self.recovery.is_some() && transition.is_some() {
                return Err(
                    "healthy durable-set transition cannot be armed during recovery".into(),
                );
            }
            let current_record = self.anchor.head().record();
            if current_record.durable_set_transition_digest.is_some()
                && self
                    .anchor
                    .authority()
                    .durable_set_activation_digest()
                    .is_none()
                && self
                    .anchor
                    .authority()
                    .authority_id()
                    .map_err(|e| e.to_string())?
                    == current_record.original_authority_id
            {
                return Err(
                    "old formal authority cannot continue past a healthy durable-set boundary before activation".into(),
                );
            }
            let business = if self.anchor.authority().definition().profile_version == 2 {
                if self.business_reader.is_none() {
                    return Err("formal profile 2 cold index reader unavailable".into());
                }
                let publication: Published = read(
                    &self
                        .root
                        .join("published")
                        .join(format!("{:020}.cbor", self.anchor.head().record().position)),
                )?;
                if &publication.candidate.candidate.record != self.anchor.head().record() {
                    return Err("formal business publication differs from predecessor".into());
                }
                let tree = super::formal_business_package::tree(
                    &publication.candidate.material,
                    self.anchor.head().record(),
                    &self.world,
                    true,
                )?
                .ok_or("formal business predecessor descriptor missing")?;
                Some((tree, publication.proof, publication.candidate.material))
            } else {
                None
            };
            let (mut body, mut material) =
                timing.step("build_frozen", None, || -> Result<_, String> {
                    Ok(match (business, transition) {
                        (Some((tree, parent_qualified, parent_material)), transition) => {
                            formal_package::freeze_material_from_parent(
                                self.anchor.authority(),
                                self.anchor.head(),
                                original,
                                &self.world,
                                &self.release,
                                tree,
                                parent_qualified,
                                transition.cloned(),
                                &parent_material,
                            )?
                        }
                        (None, Some(transition)) => {
                            formal_package::freeze_material_with_durable_set_transition(
                                self.anchor.authority(),
                                self.anchor.head(),
                                original,
                                &self.world,
                                &self.release,
                                transition,
                            )?
                        }
                        (None, None) => formal_package::freeze_material(
                            self.anchor.authority(),
                            self.anchor.head(),
                            original,
                            &self.world,
                            &self.release,
                        )?,
                    })
                })?;
            if self
                .anchor
                .head()
                .record()
                .artifact_roots
                .contains_key(&ArtifactRole::NativeExecutionEnvironment)
            {
                let (primary, replica) = self.coordinator.endpoints_mut();
                let environment =
                    timing.step("stage_native", Some(&body.request_digest), || {
                        super::formal_native_environment::stage_current(primary, replica)
                    })?;
                if self
                    .anchor
                    .head()
                    .record()
                    .artifact_roots
                    .get(&ArtifactRole::NativeExecutionEnvironment)
                    != Some(&environment.digest()?)
                {
                    return Err(
                        "Unavailable: native environment changed from the qualified predecessor"
                            .into(),
                    );
                }
                timing.step("attach_native", None, || {
                    super::formal_native_environment::attach_frozen(
                        &mut body,
                        &mut material,
                        &environment,
                    )
                })?;
            }
            timing.material("frozen_material", &body.request_digest, || {
                (
                    material.objects.len(),
                    material
                        .objects
                        .iter()
                        .map(|object| object.bytes.len())
                        .sum(),
                )
            });
            let signature_hex = hex::encode(
                self.writer
                    .sign(&frozen_signing_bytes(&body).map_err(|e| e.to_string())?)
                    .to_bytes(),
            );
            self.pending = Some(Pending {
                schema_version: 1,
                original: SignedFormalFrozenV1 {
                    body,
                    signature_hex,
                },
                material,
                candidate: None,
                frozen_certificate: None,
            });
            if let Err(error) = timing.step("persist_frozen", None, || self.persist()) {
                self.poisoned = true;
                return Err(error);
            }
            #[cfg(all(test, unix))]
            super::formal_crash_test_boundary::stop_after("pending_durable")?;
        } else if let Some(pending) = self.pending.as_ref() {
            let captured: formal_package::FormalExecutionManifestV1 =
                capture::decode_role(&pending.material, ArtifactRole::ExecutionManifest)?;
            if &captured.continuation != original {
                return Err("formal writer must resume exact pending continuation".into());
            }
            if transition.is_some_and(|transition| {
                captured.durable_set_transition.as_ref() != Some(transition)
            }) {
                return Err("formal pending Frozen transition differs from durable-set arm".into());
            }
        }
        let pending = self
            .pending
            .as_ref()
            .ok_or("formal writer pending missing")?;
        // An already published original remains terminal until acknowledged.
        if pending.original.body.request_digest == self.anchor.head().record().request_digest {
            return Ok(());
        }
        let result = self.close_pending_reservation();
        if result.is_err() {
            self.poisoned = true;
        }
        #[cfg(all(test, unix))]
        if result.is_ok() {
            super::formal_crash_test_boundary::stop_after("input_frozen")?;
        }
        result
    }
    /// Replace only an exact terminal predecessor. The old pending file stays
    /// durable until `prepare` atomically writes the successor's original.
    pub(crate) fn prepare_successor(
        &mut self,
        original: &NodeLocalExecutionContinuation,
        previous_record_id: &str,
    ) -> Result<(), String> {
        self.prepare_successor_with_durable_set_transition(original, previous_record_id, None)
    }

    pub(crate) fn prepare_successor_with_durable_set_transition(
        &mut self,
        original: &NodeLocalExecutionContinuation,
        previous_record_id: &str,
        transition: Option<&SignedNormalDurableSetTransitionV1>,
    ) -> Result<(), String> {
        if self.poisoned || self.anchor.head().head().record_id != previous_record_id {
            return Err("formal successor lacks exact acknowledged qualified predecessor".into());
        }
        let previous = self
            .pending
            .as_ref()
            .ok_or("formal terminal predecessor missing")?;
        if previous
            .candidate
            .as_ref()
            .is_none_or(|candidate| &candidate.candidate.record != self.anchor.head().record())
        {
            return Err("formal successor cannot replace unresolved original".into());
        }
        let saved = self.pending.take();
        let result = self.prepare_with_durable_set_transition(original, transition);
        if self.pending.is_none() {
            self.pending = saved;
        }
        result
    }
    pub(crate) fn pending_continuation(
        &self,
    ) -> Result<Option<NodeLocalExecutionContinuation>, String> {
        self.pending
            .as_ref()
            .map(|pending| {
                let captured: formal_package::FormalExecutionManifestV1 =
                    capture::decode_role(&pending.material, ArtifactRole::ExecutionManifest)?;
                Ok(captured.continuation)
            })
            .transpose()
    }
    pub(crate) fn resume_original(&mut self) -> Result<FormalQualifiedCommitProofV1, String> {
        if self.poisoned {
            return Err("OutcomeUnknown: reopen and query original formal request".into());
        }
        let result = self.resume();
        if result.is_err() {
            self.poisoned = true;
        }
        result
    }
    fn resume(&mut self) -> Result<FormalQualifiedCommitProofV1, String> {
        let mut timing = super::formal_execution_timing::FormalExecutionTiming::begin(
            "resume",
            self.anchor.head().record().position,
        );
        let pending = self
            .pending
            .as_ref()
            .ok_or("formal writer has no pending original")?
            .clone();
        if pending.original.body.request_digest == self.anchor.head().record().request_digest {
            let path = self
                .root
                .join("published")
                .join(format!("{:020}.cbor", self.anchor.head().record().position));
            let original: Published = read(&path)?;
            if original.candidate
                != pending
                    .candidate
                    .ok_or("terminal pending candidate missing")?
            {
                return Err("formal writer original publication mismatch".into());
            }
            self.reclaim_published_attempts(&original);
            return Ok(original.proof);
        }
        let candidate =
            timing.step("candidate_for_resume", None, || self.candidate_for_resume())?;
        timing.material(
            "candidate_material",
            &candidate.candidate.record.request_digest,
            || {
                (
                    candidate.material.objects.len(),
                    candidate
                        .material
                        .objects
                        .iter()
                        .map(|object| object.bytes.len())
                        .sum(),
                )
            },
        );
        timing.mark("candidate");
        self.pending
            .as_mut()
            .ok_or("formal pending missing")?
            .candidate = Some(candidate.clone());
        timing.step(
            "persist_candidate",
            Some(&candidate.candidate.record.request_digest),
            || self.persist(),
        )?;
        timing.mark("candidate_saved");
        #[cfg(all(test, unix))]
        super::formal_crash_test_boundary::stop_after("candidate_saved")?;
        if let Some(environment) =
            super::formal_native_environment::descriptor(&candidate.material)?
        {
            let (primary, replica) = self.coordinator.endpoints_mut();
            super::formal_native_environment::verify_registered(primary, replica, &environment)?;
        }
        let proof = timing
            .step(
                "submit_candidate",
                Some(&candidate.candidate.record.request_digest),
                || self.coordinator.submit(candidate.clone()),
            )
            .map_err(|e| e.to_string())?;
        timing.mark("dual_qualified");
        #[cfg(all(test, unix))]
        super::formal_crash_test_boundary::stop_after("dual_qualified")?;
        let head = verify_formal_qualified(
            &proof,
            if proof.decision.branch == FormalCompletionBranchV1::Normal {
                self.anchor.authority()
            } else {
                self.recovery
                    .as_ref()
                    .ok_or("recovery proof lacks checked context")?
                    .original_authority()
            },
            Some(self.anchor.head()),
            if proof.decision.branch == FormalCompletionBranchV1::Normal {
                None
            } else {
                self.recovery.as_ref()
            },
        )
        .map_err(|e| e.to_string())?;
        let next_anchor = self
            .anchor
            .after_verified_commit(self.anchor.authority().clone(), head.clone())
            .map_err(|error| error.to_string())?;
        timing.mark("qualified_verified");
        let world = if let Some(reader) = self.business_reader.as_ref() {
            if let Some(envelope) =
                super::formal_business_package::envelope(&candidate.material, true)?
            {
                for node in envelope.delta.node_writes.iter().chain(
                    envelope
                        .retained_history_delta
                        .iter()
                        .flat_map(|delta| delta.node_writes.iter()),
                ) {
                    reader.0.cache_content_node(&node.digest, &node.bytes)?;
                }
            }
            restore_qualified(&candidate, &self.release, &next_anchor, reader.0.clone())?
        } else {
            restore(&candidate, &self.release)?
        };
        timing.mark("world_restored");
        let stored = Published {
            schema_version: 1,
            candidate,
            proof: proof.clone(),
        };
        let bytes = to_cbor(&stored)?;
        if bytes.len() > 64 * 1024 * 1024 {
            return Err("formal writer publication exceeds bound".into());
        }
        durable_transaction::write_private_file_durable(
            &self
                .root
                .join("published")
                .join(format!("{:020}.cbor", head.record().position)),
            &bytes,
        )?;
        #[cfg(all(test, unix))]
        super::formal_crash_test_boundary::stop_after("publication_saved")?;
        self.anchor = next_anchor;
        self.world = world;
        self.reclaim_published_attempts(&stored);
        timing.mark("publication_saved");
        // Keep terminal original as an idempotency cache. `acknowledge` is a
        // distinct durable step so a lost return never silently picks next input.
        Ok(proof)
    }
    pub(crate) fn acknowledge(&mut self, record_id: &str) -> Result<(), String> {
        if self.poisoned
            || self.anchor.head().head().record_id != record_id
            || self.pending.as_ref().is_none_or(|p| {
                p.original.body.request_digest != self.anchor.head().record().request_digest
            })
        {
            return Err("formal writer acknowledgement differs from qualified original".into());
        }
        if let Some((candidate, proof)) =
            read_published_candidate_at(&self.root, self.anchor.head().record().position)?
        {
            self.reclaim_published_attempts(&Published {
                schema_version: 1,
                candidate,
                proof,
            });
        }
        let removed = std::fs::remove_file(self.root.join("pending.cbor"))
            .and_then(|_| std::fs::File::open(&self.root)?.sync_all());
        if let Err(error) = removed {
            self.poisoned = true;
            return Err(format!(
                "OutcomeUnknown: formal acknowledgement durability: {error}"
            ));
        }
        self.pending = None;
        Ok(())
    }
}
