//! Bounded local historical reader. It cannot mint qualified cuts or Current.
use super::formal_resource_paid_read_v1::AuthorizedResourceArchiveReadV1;
use super::*;
use cap_std::fs::Dir;
use oasis7_client_storage::private_storage;
/// Configured external origin is held separately from all archive files.
pub struct ResourceGoalArchiveReaderV2 {
    directory: Dir,
    trust: InitialActivationTrustV1,
    maximum_entries: usize,
    maximum_total_bytes: u64,
    maximum_record_bytes: usize,
}
impl ResourceGoalArchiveReaderV2 {
    pub fn open_existing_path(
        path: &std::path::Path,
        trust: InitialActivationTrustV1,
        maximum_entries: usize,
        maximum_total_bytes: u64,
        maximum_record_bytes: usize,
    ) -> FormalReadResult<Self> {
        let dir = crate::resource_world_paid_ledger_v1::existing_dir(path)
            .map_err(FormalReadError::Integrity)?;
        Self::from_existing_directory(
            dir,
            trust,
            maximum_entries,
            maximum_total_bytes,
            maximum_record_bytes,
        )
    }

    /// The service supplies an already opened private capability directory.
    /// No directory is created and no archive cert/checkpoint selects trust.
    pub fn from_existing_directory(
        directory: Dir,
        trust: InitialActivationTrustV1,
        maximum_entries: usize,
        maximum_total_bytes: u64,
        maximum_record_bytes: usize,
    ) -> FormalReadResult<Self> {
        private_storage::private_directory(&directory).map_err(FormalReadError::Integrity)?;
        if maximum_entries == 0 || maximum_total_bytes == 0 || maximum_record_bytes == 0 {
            return Err(FormalReadError::RequestRejected(
                "Resource archive reader bounds".into(),
            ));
        }
        Ok(Self {
            directory,
            trust,
            maximum_entries,
            maximum_total_bytes,
            maximum_record_bytes,
        })
    }
    /// A caller cannot read private files using only a key/digest or bool.
    /// This handle exists only after the permanent physical claim and current
    /// participant/Owner/task/delegation/Goal checks in the actual opaque cut.
    pub fn load_exact(
        &self,
        authorized: &AuthorizedResourceArchiveReadV1,
    ) -> FormalReadResult<VerifiedResourceGoalArchiveV2> {
        authorized.ensure_live()?;
        private_storage::private_directory(&self.directory).map_err(FormalReadError::Integrity)?;
        let mut names = Vec::new();
        let mut total = 0u64;
        for entry in self
            .directory
            .entries()
            .map_err(|e| FormalReadError::Internal(e.to_string()))?
        {
            let entry = entry.map_err(|e| FormalReadError::Internal(e.to_string()))?;
            let name = entry.file_name().into_string().map_err(|_| {
                FormalReadError::Integrity("Resource archive filename encoding".into())
            })?;
            if name == ".activation-spool.lock" {
                continue;
            }
            let digest = name
                .strip_suffix(".activation-v2.cbor")
                .or_else(|| name.strip_suffix(".activation.cbor"))
                .ok_or_else(|| {
                    FormalReadError::Integrity("Resource archive unknown entry".into())
                })?;
            if digest.len() != 64
                || !digest
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            {
                return Err(FormalReadError::Integrity(
                    "Resource archive filename".into(),
                ));
            }
            let metadata = self
                .directory
                .symlink_metadata(&name)
                .map_err(|e| FormalReadError::Internal(e.to_string()))?;
            if !metadata.is_file() {
                return Err(FormalReadError::Integrity(
                    "Resource archive entry type".into(),
                ));
            }
            total = total.checked_add(metadata.len()).ok_or_else(|| {
                FormalReadError::Integrity("Resource archive size overflow".into())
            })?;
            names.push(name);
            if names.len() > self.maximum_entries || total > self.maximum_total_bytes {
                return Err(FormalReadError::HistoryUnavailable(
                    "Resource archive reader storage bound".into(),
                ));
            }
        }
        names.sort();
        for name in names {
            let Some(digest) = name.strip_suffix(".activation-v2.cbor") else {
                continue;
            };
            let bytes =
                private_storage::read_private(&self.directory, &name, self.maximum_record_bytes)
                    .map_err(FormalReadError::Integrity)?;
            if blake3_hex(&bytes) != digest {
                return Err(FormalReadError::Integrity(
                    "Resource archive file digest mismatch".into(),
                ));
            }
            let verified =
                ResourceGoalArchiveRecordV2::decode_strict(&bytes, self.maximum_record_bytes)?
                    .verify(&self.trust)?;
            if verified.exact_result(&authorized.key) == Some(&authorized.result) {
                if verified.activation_commit().world != authorized.commit.world
                    || verified.activation_commit().binding.provider_world_id
                        != authorized.commit.binding.provider_world_id
                    || verified.activation_commit().binding.branch_id
                        != authorized.commit.binding.branch_id
                    || verified.activation_commit().binding.reorg_generation
                        != authorized.commit.binding.reorg_generation
                    || verified.activation_commit().position > authorized.commit.position
                {
                    return Err(FormalReadError::Integrity(
                        "Resource archive incompatible current history".into(),
                    ));
                }
                authorized.ensure_live()?;
                return Ok(verified);
            }
        }
        Err(FormalReadError::HistoryUnavailable(
            "exact Resource Goal activation unavailable".into(),
        ))
    }
}
