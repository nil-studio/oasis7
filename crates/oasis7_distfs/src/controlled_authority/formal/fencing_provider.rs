//! Executable local CI provider. Its ledger is deliberately CONTROLLED_ONLY:
//! privileged rollback/key cloning cannot be ruled out by ordinary file IO.
use super::*;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
#[cfg(unix)]
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
static TEMP: AtomicU64 = AtomicU64::new(0);
#[path = "fencing_writer_provider.rs"]
pub(super) mod writer;
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Ledger {
    version: u32,
    instance: EndpointInstanceBindingV1,
    current: EndpointCheckpointBindingV1,
    pending: Option<CheckpointIntentV1>,
    seal: Option<EpochSealBodyV1>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SignedLedger {
    ledger: Ledger,
    signature: String,
}
pub struct ControlledFileProvider {
    root: PathBuf,
    key: SigningKey,
    resource: String,
}
impl ControlledFileProvider {
    pub fn new(path: impl AsRef<Path>, key: SigningKey) -> Result<Self, FencingError> {
        private_dir(path.as_ref())?;
        let root = fs::canonicalize(path.as_ref()).map_err(io)?;
        let resource = hash_value(&(
            "controlled-file-provider-v1",
            directory_fingerprint(&root)?,
            hex::encode(key.verifying_key().to_bytes()),
        ))?;
        Ok(Self {
            root,
            key,
            resource,
        })
    }
    pub fn public_key(&self) -> String {
        hex::encode(self.key.verifying_key().to_bytes())
    }
    pub fn resource_binding(&self) -> &str {
        &self.resource
    }
}
struct Guard {
    _lock: File,
    file: PathBuf,
    ledger: Ledger,
    key: SigningKey,
    resource: String,
}
impl Guard {
    fn persist(&self) -> Result<(), FencingError> {
        let signature = sign(
            "oasis7.controlled.fencing-ledger.v1",
            &self.ledger,
            &self.key,
        )?;
        let bytes = serde_json::to_vec(&SignedLedger {
            ledger: self.ledger.clone(),
            signature,
        })
        .map_err(|e| invalid(e.to_string()))?;
        atomic(&self.file, &bytes)
    }
}
impl MonotonicEndpointProvider for ControlledFileProvider {
    fn acquire(
        &self,
        instance: &EndpointInstanceBindingV1,
        initial: &EndpointCheckpointBindingV1,
    ) -> Result<Box<dyn MonotonicEndpointGuard>, FencingError> {
        instance.validate()?;
        initial.validate()?;
        // Key+world+epoch registry blocks a second instance claiming the same
        // signer within this provider. This is not proof against another ledger.
        let slot = hash_value(&(
            &instance.world_id,
            &instance.chain_id,
            &instance.genesis_digest,
            instance.authority_epoch,
            &instance.endpoint_public_key,
        ))?;
        let lock_path = self.root.join(format!("{slot}.lock"));
        no_symlink(&lock_path)?;
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(unix)]
        options.mode(0o600);
        let lock = options.open(&lock_path).map_err(io)?;
        if !lock.metadata().map_err(io)?.is_file() {
            return Err(invalid("provider lock is not regular"));
        }
        lock.try_lock().map_err(|_| FencingError::Locked)?;
        let file = self.root.join(format!("{slot}.json"));
        no_symlink(&file)?;
        let ledger = if file.exists() {
            let bytes = bounded(&file)?;
            let signed: SignedLedger =
                serde_json::from_slice(&bytes).map_err(|e| invalid(e.to_string()))?;
            verify(
                "oasis7.controlled.fencing-ledger.v1",
                &signed.ledger,
                &signed.signature,
                &self.public_key(),
            )?;
            if signed.ledger.version != 1 || signed.ledger.instance != *instance {
                return Err(invalid(
                    "provider signer belongs to different exact instance",
                ));
            }
            signed.ledger
        } else {
            Ledger {
                version: 1,
                instance: instance.clone(),
                current: initial.clone(),
                pending: None,
                seal: None,
            }
        };
        let guard = Guard {
            _lock: lock,
            file,
            ledger,
            key: self.key.clone(),
            resource: self.resource.clone(),
        };
        guard.persist()?;
        Ok(Box::new(guard))
    }
}
impl MonotonicEndpointGuard for Guard {
    fn checkpoint(&self) -> &EndpointCheckpointBindingV1 {
        &self.ledger.current
    }
    fn pending(&self) -> Option<&CheckpointIntentV1> {
        self.ledger.pending.as_ref()
    }
    fn seal(&self) -> Option<&EpochSealBodyV1> {
        self.ledger.seal.as_ref()
    }
    fn begin(&mut self, intent: CheckpointIntentV1) -> Result<(), FencingError> {
        if self.ledger.pending.is_some() {
            return Err(invalid("unresolved provider checkpoint intent"));
        }
        if self.ledger.seal.is_some() {
            return Err(FencingError::EpochSealed);
        }
        intent.previous.validate()?;
        intent.next.validate()?;
        if intent.previous != self.ledger.current
            || intent.next.revision
                != intent
                    .previous
                    .revision
                    .checked_add(1)
                    .ok_or_else(|| invalid("progress revision exhausted"))?
            || intent.checkpoint_bytes.len() > 64 * 1024
            || blake3::hash(&intent.checkpoint_bytes).to_hex().as_str() != intent.next.commitment
        {
            return Err(FencingError::Rollback);
        }
        let previous = &intent.previous.head;
        let next = &intent.next.head;
        if next.position < previous.position
            || next.position > previous.position.saturating_add(1)
            || (next.position == previous.position
                && (next.decision_hash != previous.decision_hash
                    || (previous.qualified && !next.qualified)))
            || (next.position > previous.position && previous.position != 0 && !previous.qualified)
        {
            return Err(invalid("provider append/head fence"));
        }
        if let Some(seal) = &intent.seal {
            seal.validate()?;
            if seal.instance != self.ledger.instance
                || seal.checkpoint_before != self.ledger.current
                || intent.next.head != self.ledger.current.head
            {
                return Err(invalid("provider seal snapshot mismatch"));
            }
        }
        self.ledger.pending = Some(intent);
        self.persist()
    }
    fn finish(&mut self) -> Result<(), FencingError> {
        let intent = self
            .ledger
            .pending
            .take()
            .ok_or_else(|| invalid("provider no pending checkpoint"))?;
        self.ledger.current = intent.next;
        if let Some(seal) = intent.seal {
            self.ledger.seal = Some(seal);
        }
        self.persist()
    }
    fn evidence(&self) -> Result<MonotonicProviderEvidenceV1, FencingError> {
        if self.ledger.pending.is_some() {
            return Err(invalid("provider intent not completed"));
        }
        let statement = MonotonicProviderStatementV1 {
            schema_version: 1,
            capability: ProviderCapabilityV1::ControlledLocalTest,
            resource_binding: self.resource.clone(),
            instance_digest: self.ledger.instance.digest()?,
            checkpoint: self.ledger.current.clone(),
            epoch_floor: self
                .ledger
                .seal
                .as_ref()
                .map(|s| s.next_epoch_floor)
                .unwrap_or(self.ledger.instance.authority_epoch),
            seal_digest: self
                .ledger
                .seal
                .as_ref()
                .map(EpochSealBodyV1::digest)
                .transpose()?,
        };
        let signature = sign(PROVIDER_DOMAIN, &statement, &self.key)?;
        Ok(MonotonicProviderEvidenceV1 {
            statement,
            signature,
        })
    }
}
fn io(e: std::io::Error) -> FencingError {
    FencingError::Io(e.to_string())
}
fn no_symlink(path: &Path) -> Result<(), FencingError> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() => Err(invalid("provider symlink")),
        Ok(_) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(io(e)),
    }
}
fn private_dir(path: &Path) -> Result<(), FencingError> {
    no_symlink(path)?;
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    builder.mode(0o700);
    match builder.create(path) {
        Ok(()) => {
            sync(path)?;
            sync(path.parent().unwrap_or(Path::new(".")))
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists && path.is_dir() => Ok(()),
        Err(e) => Err(io(e)),
    }
}
fn bounded(path: &Path) -> Result<Vec<u8>, FencingError> {
    read_bounded(path, 128 * 1024)
}
fn read_bounded(path: &Path, limit: usize) -> Result<Vec<u8>, FencingError> {
    no_symlink(path)?;
    let meta = fs::symlink_metadata(path).map_err(io)?;
    if !meta.is_file() || meta.len() > limit as u64 {
        return Err(invalid("provider ledger file/size"));
    }
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(io)?
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(io)?;
    if bytes.len() > limit {
        return Err(invalid("provider ledger size"));
    }
    Ok(bytes)
}
fn atomic(path: &Path, bytes: &[u8]) -> Result<(), FencingError> {
    atomic_bounded(path, bytes, 128 * 1024)
}
fn atomic_bounded(path: &Path, bytes: &[u8], limit: usize) -> Result<(), FencingError> {
    if bytes.len() > limit {
        return Err(invalid("provider ledger limit"));
    }
    no_symlink(path)?;
    let parent = path.parent().ok_or_else(|| invalid("provider parent"))?;
    loop {
        let temp = parent.join(format!(
            ".floor-pending-{}-{}",
            std::process::id(),
            TEMP.fetch_add(1, Ordering::Relaxed)
        ));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        options.mode(0o600);
        let mut file = match options.open(&temp) {
            Ok(file) => file,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(io(e)),
        };
        let result = (|| {
            file.write_all(bytes).map_err(io)?;
            file.sync_all().map_err(io)?;
            fs::rename(&temp, path).map_err(io)?;
            sync(parent)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temp);
        }
        return result;
    }
}
#[cfg(unix)]
fn sync(path: &Path) -> Result<(), FencingError> {
    File::open(path).and_then(|f| f.sync_all()).map_err(io)
}
#[cfg(not(unix))]
fn sync(_: &Path) -> Result<(), FencingError> {
    Err(FencingError::FencingUnproven)
}
