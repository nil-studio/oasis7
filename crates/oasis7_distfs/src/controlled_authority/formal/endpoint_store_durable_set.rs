//! Adopt only provider-signed installed metadata under the candidate lock.
use super::*;
#[path = "endpoint_store_durable_set_copy.rs"]
mod copy;
pub(crate) use copy::copy_authenticated_closure;
#[cfg(test)]
pub(crate) use copy::entry_validation_counts;
impl Store {
    pub(crate) fn read_native_checkpoint(path: &Path) -> Result<Vec<u8>, FormalError> {
        read(&path.join(HOT))
    }
    pub(crate) fn encode_provider_checkpoint(
        body: &HotBody,
        signature: &str,
        public_key: &str,
    ) -> Result<Vec<u8>, FormalError> {
        let bytes = encode(&Hot {
            body: body.clone(),
            signature: String::from(signature),
        })?;
        if Self::verify_public(&bytes, public_key)? != *body {
            return Err(invalid("provider native checkpoint signature"));
        }
        Ok(bytes)
    }

    pub(crate) fn persist_activation_receipts(
        path: &Path,
        transition: &str,
        receipts: &[DurableSetInstallReceiptV1; 2],
    ) -> Result<(), FormalError> {
        hash_hex(transition)?;
        let file = path.join(format!("formal-durable-set-activation-{transition}.cbor"));
        let bytes = encode(receipts)?;
        if fs::symlink_metadata(&file).is_ok() {
            if read(&file)? != bytes {
                return Err(invalid(
                    "healthy activation immutable original receipts conflict",
                ));
            }
            return Ok(());
        }
        Self::atomic(path, &file, &bytes)
    }
    pub(crate) fn retained_activation_receipts(
        path: &Path,
        transition: &str,
    ) -> Result<Option<[DurableSetInstallReceiptV1; 2]>, FormalError> {
        hash_hex(transition)?;
        let file = path.join(format!("formal-durable-set-activation-{transition}.cbor"));
        match fs::symlink_metadata(&file) {
            Ok(metadata) if metadata.is_file() && metadata.len() <= 128 * 1024 => {
                decode(&read(&file)?).map(Some)
            }
            Ok(_) => Err(invalid("healthy activation receipt file regular/bounded")),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(io(error)),
        }
    }
    pub(crate) fn has_candidate_manifest(
        path: &Path,
        transition: &str,
    ) -> Result<bool, FormalError> {
        hash_hex(transition)?;
        let file = path.join(format!("formal-durable-set-candidate-{transition}.cbor"));
        match fs::symlink_metadata(&file) {
            Ok(metadata) if metadata.is_file() && metadata.len() <= 128 * 1024 => Ok(true),
            Ok(_) => Err(invalid("healthy candidate descriptor file regular/bounded")),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(io(error)),
        }
    }
    pub(crate) fn persist_candidate_manifest(
        path: &Path,
        transition: &str,
        bytes: &[u8],
    ) -> Result<(), FormalError> {
        let file = {
            hash_hex(transition)?;
            path.join(format!("formal-durable-set-candidate-{transition}.cbor"))
        };
        if fs::symlink_metadata(&file).is_ok() {
            let existing = read(&file)?;
            if existing != bytes {
                return Err(invalid("healthy candidate original descriptor conflict"));
            }
            return Ok(());
        }
        Self::atomic(path, &file, bytes)
    }
    pub(crate) fn read_candidate_manifest(
        path: &Path,
        transition: &str,
    ) -> Result<Vec<u8>, FormalError> {
        let bytes = {
            hash_hex(transition)?;
            read(&path.join(format!("formal-durable-set-candidate-{transition}.cbor")))?
        };
        if bytes.len() > 128 * 1024 {
            return Err(invalid("healthy candidate descriptor bound"));
        }
        Ok(bytes)
    }

    pub(crate) fn candidate_lock(path: &Path) -> Result<File, FormalError> {
        directory(path)?;
        let lock_path = path.join(".formal-endpoint.lock");
        if fs::symlink_metadata(&lock_path).is_ok() {
            regular(&lock_path)?;
        }
        let mut options = OpenOptions::new();
        options.create(true).truncate(false).read(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
        }
        let lock = options.open(lock_path).map_err(io)?;
        lock.try_lock()
            .map_err(|_| FormalError::Conflict("formal candidate endpoint locked".into()))?;
        Ok(lock)
    }
    pub(crate) fn adopt_installed(
        mut self,
        body: HotBody,
        bytes: Vec<u8>,
        public_key: &str,
    ) -> Result<Self, FormalError> {
        let verified = Self::verify_public(&bytes, public_key)?;
        if verified != body {
            return Err(invalid("healthy installed metadata exact body"));
        }
        self.refresh_public(public_key)?;
        self.validate_refs(&body)?;
        Self::atomic(&self.path, &self.path.join(HOT), &bytes)?;
        self.checkpoint = Self::binding(&body, &bytes);
        self.body = body;
        self.sources.clear();
        self.validate_refs(&self.body)?;
        Ok(self)
    }
    pub(crate) fn from_candidate_lock(
        path: &Path,
        lock: File,
        body: HotBody,
        bytes: Vec<u8>,
        public_key: &str,
        old_sealed_bytes: &[u8],
    ) -> Result<Self, FormalError> {
        if Self::verify_public(&bytes, public_key)? != body {
            return Err(invalid("healthy candidate signed metadata"));
        }
        let file = path.join(HOT);
        if fs::symlink_metadata(&file).is_ok() {
            let existing = read(&file)?;
            if existing != old_sealed_bytes {
                let old = Self::verify_public(&existing, public_key)?;
                if old.instance_digest != body.instance_digest
                    || old.authority_id != body.authority_id
                    || old.epoch != body.epoch
                    || old.durable_set != body.durable_set
                    || old.revision > body.revision
                    || (old.revision == body.revision && existing != bytes)
                {
                    return Err(invalid(
                        "healthy adoption cannot roll back or replace native context",
                    ));
                }
            }
        }
        let store = Self {
            path: path.to_path_buf(),
            checkpoint: Self::binding(&body, &bytes),
            body,
            _lock: lock,
            _bounded_cache_fee: None,
            _bounded_checkpoint_fee: None,
            sources: vec![],
            #[cfg(test)]
            fault: None,
        };
        store.validate_refs(&store.body)?;
        Self::atomic(path, &path.join(HOT), &bytes)?;
        Ok(store)
    }
}
