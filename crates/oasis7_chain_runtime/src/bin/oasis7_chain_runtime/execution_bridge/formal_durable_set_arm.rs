//! Private one-shot operator arm for a healthy durable-set replacement.
//!
//! The arm is only intent. It never seals endpoints, qualifies a boundary, or
//! activates the successor. The old writer embeds the exact signed transition
//! in its normal Frozen input before executing the boundary continuation.
use super::{formal_package, to_cbor};
use ed25519_dalek::{Signature, VerifyingKey};
use oasis7_distfs::controlled_authority::formal::{
    FormalHeadRefV1, SignedNormalDurableSetTransitionV1, VerifiedFormalReadAnchorV1,
    normal_durable_set_transition_signing_bytes,
};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

const MAX_ARM_BYTES: u64 = 256 * 1024;
const ARM_DOMAIN: &str = "oasis7.formal.durable_set_arm_intent.v1";
static TEMP_SUFFIX: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PersistedDurableSetArmV1 {
    schema_version: u32,
    operation_id: String,
    boundary_position: u64,
    predecessor: FormalHeadRefV1,
    transition_digest: String,
    transition: SignedNormalDurableSetTransitionV1,
    intent_digest: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum DurableSetArmPhaseV1 {
    Armed,
    BoundaryQualified,
    Activated,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DurableSetArmStatusV1 {
    pub(crate) operation_id: String,
    pub(crate) boundary_position: u64,
    pub(crate) transition_digest: String,
    pub(crate) phase: DurableSetArmPhaseV1,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DurableSetArmIntentV1 {
    pub(crate) operation_id: String,
    pub(crate) boundary_position: u64,
    pub(crate) predecessor: FormalHeadRefV1,
    pub(crate) transition_digest: String,
    pub(crate) transition: SignedNormalDurableSetTransitionV1,
}

#[derive(Default)]
struct ArmRuntimeState {
    pending_open: bool,
}

/// Shared by the in-process owner and Node hook. The caller serializes public
/// admission with the same gate used for submit; this mutex also prevents an
/// arm state update racing with a hook transition.
pub(crate) struct FormalDurableSetArmControlV1 {
    writer_directory: PathBuf,
    state: Mutex<ArmRuntimeState>,
}

impl FormalDurableSetArmControlV1 {
    pub(crate) fn new(writer_directory: PathBuf, pending_open: bool) -> Result<Self, String> {
        crate::controlled_live_config::require_private_owned(&writer_directory, true)?;
        Ok(Self {
            writer_directory,
            state: Mutex::new(ArmRuntimeState { pending_open }),
        })
    }

    pub(crate) fn set_pending_open(&self, pending_open: bool) -> Result<(), String> {
        self.state
            .lock()
            .map_err(|_| "formal durable-set arm state poisoned")?
            .pending_open = pending_open;
        Ok(())
    }

    pub(crate) fn pending_open(&self) -> Result<bool, String> {
        Ok(self
            .state
            .lock()
            .map_err(|_| "formal durable-set arm state poisoned")?
            .pending_open)
    }

    /// Persist an immutable arm before the boundary is admitted. Exact retries
    /// are idempotent; a different identity at the same boundary is a conflict.
    pub(crate) fn arm(
        &self,
        operation_id: &str,
        transition: SignedNormalDurableSetTransitionV1,
        current: &VerifiedFormalReadAnchorV1,
    ) -> Result<DurableSetArmStatusV1, String> {
        let state = self
            .state
            .lock()
            .map_err(|_| "formal durable-set arm state poisoned")?;
        if state.pending_open {
            return Err(
                "OutcomeUnknown: an original formal continuation is already pending".into(),
            );
        }
        let intent = build_intent(operation_id, transition, current)?;
        let path = self.path(intent.boundary_position);
        let bytes = encode_intent(&intent)?;
        match read_intent(&path) {
            Ok(Some(existing)) => {
                if existing != intent {
                    return Err(
                        "Conflict: immutable durable-set arm already exists for boundary".into(),
                    );
                }
                sync_existing(&path)?;
                Ok(status_for(&existing, current))
            }
            Ok(None) => match publish_immutable(&path, &bytes)? {
                true => Ok(status_for(&intent, current)),
                false => {
                    let Some(existing) = read_intent(&path)? else {
                        return Err(
                            "durable-set arm disappeared after concurrent publication".into()
                        );
                    };
                    if existing != intent {
                        return Err(
                            "Conflict: immutable durable-set arm appeared concurrently".into()
                        );
                    }
                    sync_existing(&path)?;
                    Ok(status_for(&existing, current))
                }
            },
            Err(error) => Err(error),
        }
    }

    /// Read and revalidate the arm for the exact next qualified position.
    /// Absence means no arm; malformed or stale bytes are never treated as it.
    pub(crate) fn for_next_boundary(
        &self,
        current: &VerifiedFormalReadAnchorV1,
    ) -> Result<Option<DurableSetArmIntentV1>, String> {
        let _state = self
            .state
            .lock()
            .map_err(|_| "formal durable-set arm state poisoned")?;
        let boundary = current
            .head()
            .record()
            .position
            .checked_add(1)
            .ok_or("formal durable-set arm boundary overflow")?;
        let path = self.path(boundary);
        let Some(intent) = read_intent(&path)? else {
            return Ok(None);
        };
        verify_intent_against_anchor(&intent, current)?;
        Ok(Some(intent))
    }

    /// Query is deliberately not an activation operation. `Activated` is
    /// reported only from an opaque anchor whose transition boundary and
    /// verified successor activation digest match the immutable arm.
    pub(crate) fn query(
        &self,
        operation_id: &str,
        boundary_position: u64,
        current: &VerifiedFormalReadAnchorV1,
    ) -> Result<Option<DurableSetArmStatusV1>, String> {
        let _state = self
            .state
            .lock()
            .map_err(|_| "formal durable-set arm state poisoned")?;
        let path = self.path(boundary_position);
        let Some(intent) = read_intent(&path)? else {
            return Ok(None);
        };
        if intent.operation_id != operation_id {
            return Err("Conflict: durable-set arm operation identity differs".into());
        }
        verify_persisted_transition_signature(&intent.transition)?;
        Ok(Some(status_for(&intent, current)))
    }

    /// Reconcile original immutable intent before any new admission decision.
    pub(crate) fn existing_for_operation(
        &self,
        operation_id: &str,
        current: &VerifiedFormalReadAnchorV1,
    ) -> Result<Option<SignedNormalDurableSetTransitionV1>, String> {
        let _state = self
            .state
            .lock()
            .map_err(|_| "formal durable-set arm state poisoned")?;
        let mut matched = None;
        for entry in fs::read_dir(&self.writer_directory).map_err(|e| e.to_string())? {
            let path = entry.map_err(|e| e.to_string())?.path();
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            let Some(position) = name
                .strip_prefix("durable-set-transition-")
                .and_then(|name| name.strip_suffix(".cbor"))
            else {
                continue;
            };
            if position.len() != 20 || !position.bytes().all(|byte| byte.is_ascii_digit()) {
                continue;
            }
            if let Some(intent) = read_intent(&path)?
                && intent.operation_id == operation_id
            {
                if matched
                    .as_ref()
                    .is_some_and(|transition| transition != &intent.transition)
                {
                    return Err("Conflict: operation has multiple durable-set transitions".into());
                }
                let status = status_for(&intent, current);
                if status.phase == DurableSetArmPhaseV1::Unknown {
                    return Err(
                        "original durable-set arm cannot be reconciled against current anchor"
                            .into(),
                    );
                }
                matched = Some(intent.transition);
            }
        }
        Ok(matched)
    }

    fn path(&self, boundary_position: u64) -> PathBuf {
        self.writer_directory.join(format!(
            "durable-set-transition-{boundary_position:020}.cbor"
        ))
    }
}

fn build_intent(
    operation_id: &str,
    transition: SignedNormalDurableSetTransitionV1,
    current: &VerifiedFormalReadAnchorV1,
) -> Result<DurableSetArmIntentV1, String> {
    if operation_id != transition.body.operation_id {
        return Err("formal durable-set operation id differs from signed transition".into());
    }
    let transition_digest = formal_package::verify_durable_set_transition_arm(
        &transition,
        current.authority(),
        current.head(),
    )?;
    let boundary_position = transition.body.boundary_position;
    let predecessor = transition.body.predecessor.clone();
    let intent = DurableSetArmIntentV1 {
        operation_id: operation_id.to_owned(),
        boundary_position,
        predecessor,
        transition_digest,
        transition,
    };
    let _ = encode_intent(&intent)?;
    Ok(intent)
}

fn verify_intent_against_anchor(
    intent: &DurableSetArmIntentV1,
    current: &VerifiedFormalReadAnchorV1,
) -> Result<(), String> {
    if intent.operation_id != intent.transition.body.operation_id
        || intent.boundary_position != intent.transition.body.boundary_position
        || intent.predecessor != intent.transition.body.predecessor
        || intent.transition_digest != intent.transition.body.digest().map_err(|e| e.to_string())?
    {
        return Err("formal durable-set arm file identity or digest mismatch".into());
    }
    let digest = formal_package::verify_durable_set_transition_arm(
        &intent.transition,
        current.authority(),
        current.head(),
    )?;
    if digest != intent.transition_digest {
        return Err("formal durable-set arm digest differs from verified predecessor".into());
    }
    Ok(())
}

fn status_for(
    intent: &DurableSetArmIntentV1,
    current: &VerifiedFormalReadAnchorV1,
) -> DurableSetArmStatusV1 {
    let record = current.head().record();
    let current_authority = current.authority();
    let phase = if record.position >= intent.boundary_position
        && current_authority.durable_set_activation_digest().is_some()
        && current_authority.definition() == &intent.transition.body.successor
    {
        DurableSetArmPhaseV1::Activated
    } else if current.head().head() == intent.predecessor
        && record.position.checked_add(1) == Some(intent.boundary_position)
        && current_authority.authority_id().ok().as_deref()
            == Some(intent.transition.body.predecessor_authority_id.as_str())
    {
        DurableSetArmPhaseV1::Armed
    } else if record.position == intent.boundary_position
        && record.parent_record_id == intent.predecessor.record_id
        && record.original_authority_id == intent.transition.body.predecessor_authority_id
        && record.world == intent.transition.body.world
        && record.durable_set_transition_digest.as_deref()
            == Some(intent.transition_digest.as_str())
    {
        if current_authority.durable_set_activation_digest().is_some()
            && current_authority.definition() == &intent.transition.body.successor
        {
            DurableSetArmPhaseV1::Activated
        } else if current_authority.authority_id().ok().as_deref()
            == Some(intent.transition.body.predecessor_authority_id.as_str())
        {
            DurableSetArmPhaseV1::BoundaryQualified
        } else {
            DurableSetArmPhaseV1::Unknown
        }
    } else {
        DurableSetArmPhaseV1::Unknown
    };
    DurableSetArmStatusV1 {
        operation_id: intent.operation_id.clone(),
        boundary_position: intent.boundary_position,
        transition_digest: intent.transition_digest.clone(),
        phase,
    }
}

/// A current signed anchor may be past the old authority's verification
/// window, so the persisted arm must retain enough information to check the
/// signature without treating its local checksum as authority. The successor
/// body preserves the old writer key by contract; the old-authority verifier
/// is still required at the exact predecessor/boundary admission point.
fn verify_persisted_transition_signature(
    transition: &SignedNormalDurableSetTransitionV1,
) -> Result<(), String> {
    transition
        .body
        .successor
        .validate()
        .map_err(|error| error.to_string())?;
    let public_key = hex::decode(&transition.body.successor.writer_public_key)
        .map_err(|_| "formal durable-set transition writer key is not hex")?;
    let public_key: [u8; 32] = public_key
        .try_into()
        .map_err(|_| "formal durable-set transition writer key width")?;
    if hex::encode(public_key) != transition.body.successor.writer_public_key {
        return Err("formal durable-set transition writer key is not canonical hex".into());
    }
    let verifying_key = VerifyingKey::from_bytes(&public_key)
        .map_err(|_| "formal durable-set transition writer key is invalid")?;
    let signature_bytes = hex::decode(&transition.writer_signature_hex)
        .map_err(|_| "formal durable-set transition signature is not hex")?;
    let signature = Signature::from_slice(&signature_bytes)
        .map_err(|_| "formal durable-set transition signature width")?;
    if hex::encode(signature.to_bytes()) != transition.writer_signature_hex {
        return Err("formal durable-set transition signature is not canonical hex".into());
    }
    let signing_bytes = normal_durable_set_transition_signing_bytes(&transition.body)
        .map_err(|error| error.to_string())?;
    verifying_key
        .verify_strict(&signing_bytes, &signature)
        .map_err(|_| "formal durable-set transition signature invalid".into())
}

fn arm_digest(intent: &DurableSetArmIntentV1) -> Result<String, String> {
    validate_intent_fields(intent)?;
    let transition_digest = intent.transition.body.digest().map_err(|e| e.to_string())?;
    let bytes = to_cbor((
        ARM_DOMAIN,
        1u32,
        &intent.operation_id,
        intent.boundary_position,
        &intent.predecessor,
        &intent.transition_digest,
        &transition_digest,
        &intent.transition,
    ))?;
    Ok(oasis7_runtime::blake3_hex(&bytes))
}

fn encode_intent(intent: &DurableSetArmIntentV1) -> Result<Vec<u8>, String> {
    let persisted = PersistedDurableSetArmV1 {
        schema_version: 1,
        operation_id: intent.operation_id.clone(),
        boundary_position: intent.boundary_position,
        predecessor: intent.predecessor.clone(),
        transition_digest: intent.transition_digest.clone(),
        transition: intent.transition.clone(),
        intent_digest: arm_digest(intent)?,
    };
    let bytes = to_cbor(&persisted)?;
    if bytes.is_empty() || bytes.len() as u64 > MAX_ARM_BYTES {
        return Err("formal durable-set arm exceeds size bound".into());
    }
    Ok(bytes)
}

fn validate_intent_fields(intent: &DurableSetArmIntentV1) -> Result<(), String> {
    if intent.operation_id != intent.transition.body.operation_id
        || intent.boundary_position != intent.transition.body.boundary_position
        || intent.predecessor != intent.transition.body.predecessor
        || intent.transition_digest
            != intent
                .transition
                .body
                .digest()
                .map_err(|error| error.to_string())?
    {
        return Err("formal durable-set arm fields differ from signed transition".into());
    }
    Ok(())
}

fn decode_intent(bytes: &[u8]) -> Result<DurableSetArmIntentV1, String> {
    if bytes.is_empty() || bytes.len() as u64 > MAX_ARM_BYTES {
        return Err("formal durable-set arm file size bound".into());
    }
    let persisted: PersistedDurableSetArmV1 =
        serde_cbor::from_slice(bytes).map_err(|error| error.to_string())?;
    let intent = DurableSetArmIntentV1 {
        operation_id: persisted.operation_id,
        boundary_position: persisted.boundary_position,
        predecessor: persisted.predecessor,
        transition_digest: persisted.transition_digest,
        transition: persisted.transition,
    };
    if persisted.schema_version != 1 {
        return Err("formal durable-set arm file schema version".into());
    }
    // These are duplicated for indexed lookup, but the signed transition is
    // the authority. Require every persisted selector to match its signed
    // body before accepting the local checksum or canonical encoding.
    validate_intent_fields(&intent)?;
    if persisted.intent_digest != arm_digest(&intent)? {
        return Err("formal durable-set arm file intent digest mismatch".into());
    }
    if encode_intent(&intent)? != bytes {
        return Err("formal durable-set arm file schema/digest/canonical encoding".into());
    }
    verify_persisted_transition_signature(&intent.transition)?;
    Ok(intent)
}

fn read_intent(path: &Path) -> Result<Option<DurableSetArmIntentV1>, String> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.to_string()),
        Ok(_) => crate::controlled_live_config::require_private_owned(path, false)?,
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW);
    }
    let file = options.open(path).map_err(|error| {
        format!(
            "read formal durable-set arm {} failed: {error}",
            path.display()
        )
    })?;
    let metadata = file.metadata().map_err(|error| error.to_string())?;
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > MAX_ARM_BYTES {
        return Err("formal durable-set arm file metadata bound".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        // SAFETY: geteuid reads only the process effective uid.
        let current_uid = unsafe { libc::geteuid() };
        if metadata.uid() != current_uid || metadata.permissions().mode() & 0o777 != 0o600 {
            return Err("formal durable-set arm file must be owned and mode 0600".into());
        }
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(MAX_ARM_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    decode_intent(&bytes).map(Some)
}

fn publish_immutable(path: &Path, bytes: &[u8]) -> Result<bool, String> {
    let parent = path
        .parent()
        .ok_or("formal durable-set arm parent missing")?;
    crate::controlled_live_config::require_private_owned(parent, true)?;
    let temp = unique_temp_path(path);
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&temp).map_err(|error| {
        format!(
            "create durable-set arm temp {} failed: {error}",
            temp.display()
        )
    })?;
    if let Err(error) = file.write_all(bytes).and_then(|_| file.sync_all()) {
        let _ = fs::remove_file(&temp);
        return Err(format!("write durable-set arm temp failed: {error}"));
    }
    match fs::hard_link(&temp, path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            let _ = fs::remove_file(&temp);
            return Ok(false);
        }
        Err(error) => {
            let _ = fs::remove_file(&temp);
            return Err(format!("publish immutable durable-set arm failed: {error}"));
        }
    }
    fs::remove_file(&temp).map_err(|error| error.to_string())?;
    sync_directory(parent)?;
    Ok(true)
}

fn sync_existing(path: &Path) -> Result<(), String> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW);
    }
    let file = options.open(path).map_err(|error| error.to_string())?;
    file.sync_all().map_err(|error| error.to_string())?;
    sync_directory(path.parent().ok_or("arm parent unavailable")?)
}

fn sync_directory(path: &Path) -> Result<(), String> {
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| format!("sync durable-set arm directory failed: {error}"))
}

fn unique_temp_path(path: &Path) -> PathBuf {
    let suffix = TEMP_SUFFIX.fetch_add(1, Ordering::Relaxed);
    path.with_extension(format!("arm-{}-{suffix}.tmp", std::process::id()))
}

#[cfg(test)]
#[path = "formal_durable_set_arm_tests.rs"]
mod tests;
