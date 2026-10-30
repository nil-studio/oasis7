//! Immutable local original Prepare bytes. A WAL is never public authority:
//! cold continuation requires an exact authenticated provider reservation.
use super::*;
use oasis7_distfs::protected_bft_v3::{
    BftSuccessorWindowArchiveManifestV1, BftSuccessorWindowInstallIntentBodyV1,
};
use serde::{Deserialize, Serialize};
const MAX_INTENT: usize = 2 * 1024 * 1024;
const DOMAIN: &str = "oasis7.formal.bft.node_auto_prepare.v1";
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AutoPrepareOwnerV1 {
    origin_digest: String,
    material_identity: String,
    private_instance: String,
    provider_public_key: String,
}
impl AutoPrepareOwnerV1 {
    pub(super) fn from_identity(
        identity: &OriginalHeaderIdentityV1,
        pin: &str,
    ) -> Result<Self, String> {
        identity.verify_unchanged()?;
        Ok(Self {
            origin_digest: identity.execution.ledger.origin_digest()?,
            material_identity: identity.material_identity.clone(),
            private_instance: identity.private.instance_id().into(),
            provider_public_key: pin.into(),
        })
    }
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AutoPrepareIntentV1 {
    schema_version: u32,
    domain: String,
    owner: AutoPrepareOwnerV1,
    body: BftSuccessorWindowInstallIntentBodyV1,
    manifest: BftSuccessorWindowArchiveManifestV1,
}
impl AutoPrepareIntentV1 {
    fn validate(
        &self,
        owner: &AutoPrepareOwnerV1,
        binding: &BftSignerBindingV3,
    ) -> Result<(), String> {
        if self.schema_version != 1
            || self.domain != DOMAIN
            || &self.owner != owner
            || self.owner.material_identity != binding.material_identity
        {
            return Err("auto Prepare original owner differs".into());
        }
        self.body
            .checkpoint
            .source
            .validate_for_binding(binding)
            .map_err(|_| "auto Prepare source differs")?;
        self.body
            .validate()
            .map_err(|_| "auto Prepare body rejected")?;
        self.manifest
            .validate_for_checkpoint(&self.body.checkpoint)
            .map_err(|_| "auto Prepare manifest differs")?;
        Ok(())
    }
    fn path(
        records: &Path,
        checkpoint: &oasis7_distfs::protected_bft_v3::BftSuccessorWindowCheckpointBodyV1,
    ) -> Result<PathBuf, String> {
        // The public checkpoint digest entry point validates the typed body
        // and produces canonical hex; no raw path component is admitted here.
        let digest = checkpoint
            .digest()
            .map_err(|_| "auto Prepare digest rejected")?;
        let root = canonical_storage_path(records)?;
        crate::controlled_live_config::require_private_owned(&root, true)?;
        let directory = root.join("successor-auto-prepares-v1");
        if directory.exists() {
            canonical_storage_path(&directory)?;
            crate::controlled_live_config::require_private_owned(&directory, true)?;
        }
        Ok(directory.join(format!("{digest}.prepare.cbor")))
    }
    /// Caller must reserve these exact metadata bytes in the current private
    /// ledger before publication; no new checkpoint quota is minted here.
    pub(super) fn new(
        owner: AutoPrepareOwnerV1,
        binding: &BftSignerBindingV3,
        body: BftSuccessorWindowInstallIntentBodyV1,
        manifest: BftSuccessorWindowArchiveManifestV1,
    ) -> Result<Self, String> {
        let value = Self {
            schema_version: 1,
            domain: DOMAIN.into(),
            owner,
            body,
            manifest,
        };
        value.validate(&value.owner, binding)?;
        value.bytes()?;
        Ok(value)
    }
    pub(super) fn bytes(&self) -> Result<Vec<u8>, String> {
        let bytes = serde_cbor::to_vec(self).map_err(|_| "encode original auto Prepare")?;
        if bytes.len() > MAX_INTENT {
            return Err("auto Prepare WAL capacity".into());
        }
        Ok(bytes)
    }
    fn verify_inventory(&self, records: &Path) -> Result<(), String> {
        let path = Self::path(records, &self.body.checkpoint)?;
        let directory = path.parent().ok_or("auto Prepare directory absent")?;
        if !directory.exists() {
            return Ok(());
        }
        let mut originals = std::collections::BTreeSet::from([self
            .body
            .checkpoint
            .digest()
            .map_err(|_| "auto Prepare candidate digest")?]);
        let additional_file = usize::from(!path.exists());
        let mut total = if additional_file == 0 {
            0
        } else {
            self.bytes()?.len() as u64
        };
        for (count, entry) in std::fs::read_dir(directory)
            .map_err(|_| "read auto Prepare inventory")?
            .enumerate()
        {
            if count + additional_file >= 256 {
                return Err("auto Prepare local inventory file capacity".into());
            }
            let entry = entry.map_err(|_| "inspect auto Prepare inventory")?;
            if !entry
                .file_type()
                .map_err(|_| "inspect auto Prepare type")?
                .is_file()
            {
                return Err("auto Prepare inventory contains nonregular entry".into());
            }
            total = total
                .checked_add(
                    entry
                        .metadata()
                        .map_err(|_| "inspect auto Prepare bytes")?
                        .len(),
                )
                .filter(|bytes| *bytes <= 512 * 1024 * 1024)
                .ok_or("auto Prepare inventory scan byte capacity")?;
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| "auto Prepare filename")?;
            let target = files::stage_target(&name).unwrap_or(&name);
            let digest = target
                .strip_suffix(".prepare.cbor")
                .ok_or("auto Prepare unknown entry")?;
            let value: Self = common_storage::read_optional_canonical(&entry.path(), MAX_INTENT)?
                .ok_or("auto Prepare original entry missing")?;
            files::single_link(&entry.path())?;
            if value.schema_version != 1
                || value.domain != DOMAIN
                || value.owner != self.owner
                || value.body.checkpoint.source != self.body.checkpoint.source
                || value
                    .body
                    .checkpoint
                    .digest()
                    .map_err(|_| "auto Prepare inventory digest")?
                    != digest
            {
                return Err("auto Prepare inventory original identity differs".into());
            }
            value
                .body
                .validate()
                .map_err(|_| "auto Prepare inventory body")?;
            value
                .manifest
                .validate_for_checkpoint(&value.body.checkpoint)
                .map_err(|_| "auto Prepare inventory manifest")?;
            originals.insert(digest.to_owned());
            if originals.len() > 64 {
                return Err("auto Prepare local original count capacity".into());
            }
        }
        Ok(())
    }

    pub(super) fn persist(&self, records: &Path) -> Result<(), String> {
        let path = Self::path(records, &self.body.checkpoint)?;
        let parent = path.parent().ok_or("auto Prepare parent missing")?;
        if !parent.exists() {
            let mut builder = std::fs::DirBuilder::new();
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                builder.mode(0o700);
            }
            builder
                .create(parent)
                .map_err(|_| "create auto Prepare directory")?;
            files::sync(records)?;
        }
        crate::controlled_live_config::require_private_owned(parent, true)?;
        self.verify_inventory(records)?;
        files::publish(&path, &self.bytes()?)?;
        self.verify_inventory(records)
    }
    pub(super) fn load_reserved(
        records: &Path,
        owner: &AutoPrepareOwnerV1,
        binding: &BftSignerBindingV3,
        reserved_body: &BftSuccessorWindowInstallIntentBodyV1,
        reserved_manifest: &BftSuccessorWindowArchiveManifestV1,
        current_generation: u64,
    ) -> Result<Self, String> {
        let path = Self::path(records, &reserved_body.checkpoint)?;
        if !files::stages(&path)?.is_empty() {
            return Err("auto Prepare WAL publication unresolved".into());
        }
        let value: Self = common_storage::read_optional_canonical(&path, MAX_INTENT)?
            .ok_or("original auto Prepare WAL absent; reservation requires explicit recovery")?;
        files::single_link(&path)?;
        value.validate(owner, binding)?;
        value.verify_inventory(records)?;
        if &value.body != reserved_body
            || &value.manifest != reserved_manifest
            || current_generation < value.body.expected_lease_generation
        {
            return Err("auto Prepare WAL differs from authenticated original reservation".into());
        }
        Ok(value)
    }
    pub(super) fn retry(
        self,
        lease: &mut dyn oasis7_distfs::protected_bft_v3::BftSignerLeaseV3,
    ) -> Result<(), String> {
        let (original, _) = lease
            .prepare_successor_window(self.body.clone(), self.manifest.clone())
            .map_err(|_| "original auto Prepare retry failed")?;
        if original.request.intent.body != self.body || original.manifest != self.manifest {
            return Err("auto Prepare retry changed original bytes".into());
        }
        Ok(())
    }
}
