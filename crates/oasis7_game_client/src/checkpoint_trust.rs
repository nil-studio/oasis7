//! Explicit renewal-config preparation. This never returns current authority.
use crate::remote_world_service::FormalReadTrustConfigV1;
use oasis7_formal_protocol::bft::{
    BftProfileTargetV2, BftSuccessorWindowSourceV1,
    checkpoint_trust_policy::{BFT_CHECKPOINT_CONFIG_MAX_BYTES_V1, BftCheckpointTrustPolicyV1},
    verify_formal_bft_handoff_v2,
};
use oasis7_formal_protocol::controlled_authority::formal::FormalHeadRefV1;
use serde::{Deserialize, Serialize};
use std::{io::Read, path::Path};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftCheckpointHandoffClosureV1 {
    pub handoff_material_identity: String,
    pub handoff_raw_snapshot_root: String,
}

/// Unsupported successor tags are deliberately absent, not decoded as H.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", deny_unknown_fields)]
pub enum BftCheckpointAnchorRefV1 {
    #[serde(rename = "qualified_handoff_v2")]
    QualifiedHandoffV2 {
        source: BftSuccessorWindowSourceV1,
        checkpoint_sequence: u64,
        checkpoint_digest: String,
        handoff_head: FormalHeadRefV1,
        initial_schedule_digest: String,
        material_closure: BftCheckpointHandoffClosureV1,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftCheckpointTrustConfigV1 {
    pub schema_version: u32,
    pub formal_read_trust: FormalReadTrustConfigV1,
    pub target: BftProfileTargetV2,
    pub consumer_binding_digest: String,
    pub initial_anchor: BftCheckpointAnchorRefV1,
    pub policy: BftCheckpointTrustPolicyV1,
    pub policy_digest: String,
    pub initial_authenticated_at_unix_ms: u64,
    pub initial_not_before_unix_ms: u64,
    pub initial_expires_at_unix_ms: u64,
    pub initial_safety_valid_until_unix_ms: u64,
}

/// Configuration consistency only. It has no conversion into a signing,
/// installation, renewal, current-service or automatic-follow capability.
pub struct PreparedBftCheckpointTrustConfigV1 {
    config: BftCheckpointTrustConfigV1,
}
impl PreparedBftCheckpointTrustConfigV1 {
    pub fn config(&self) -> &BftCheckpointTrustConfigV1 {
        &self.config
    }
    /// The host must obtain these values by actual qualified-H replay, not DTOs.
    pub fn check_replayed_handoff(
        &self,
        material_identity: &str,
        schedule_digest: &str,
    ) -> Result<(), String> {
        let BftCheckpointAnchorRefV1::QualifiedHandoffV2 {
            initial_schedule_digest,
            material_closure,
            ..
        } = &self.config.initial_anchor;
        if material_identity != material_closure.handoff_material_identity
            || schedule_digest != initial_schedule_digest
        {
            return Err("checkpoint configuration differs from replayed H closure/schedule".into());
        }
        Ok(())
    }
}

fn canonical<T: Serialize>(value: &T) -> Result<Vec<u8>, String> {
    let value = serde_cbor::value::to_value(value).map_err(|error| error.to_string())?;
    let mut bytes = Vec::new();
    let mut serializer = serde_cbor::ser::Serializer::new(&mut bytes);
    serializer
        .self_describe()
        .map_err(|error| error.to_string())?;
    value
        .serialize(&mut serializer)
        .map_err(|error| error.to_string())?;
    Ok(bytes)
}
fn hash(value: &str) -> Result<(), String> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err("checkpoint pin must be lowercase hex32".into());
    }
    Ok(())
}

impl BftCheckpointTrustConfigV1 {
    /// Pins must be supplied from the original independently authorized deployment,
    /// bound to this world, H and policy. Matching arguments does not establish
    /// their provenance or prove the C0-to-H history. Never derive them from this DTO.
    pub fn prepare(
        self,
        independently_pinned_policy: &str,
        expected_binding: &str,
        original_c0_pin: &str,
    ) -> Result<PreparedBftCheckpointTrustConfigV1, String> {
        hash(independently_pinned_policy)?;
        hash(expected_binding)?;
        hash(original_c0_pin)?;
        if canonical(&self)?.len() > BFT_CHECKPOINT_CONFIG_MAX_BYTES_V1
            || canonical(&self.formal_read_trust)?.len() > 64 * 1024
        {
            return Err("checkpoint trust input exceeds bound".into());
        }
        if self.schema_version != 1
            || self.policy_digest != independently_pinned_policy
            || self.policy.digest()? != independently_pinned_policy
            || self.consumer_binding_digest != expected_binding
        {
            return Err("checkpoint configuration does not match independent pins".into());
        }
        let anchor = self.formal_read_trust.verified_anchor()?;
        let handoff = verify_formal_bft_handoff_v2(&anchor, self.target.clone())?;
        let BftCheckpointAnchorRefV1::QualifiedHandoffV2 {
            source,
            checkpoint_sequence,
            checkpoint_digest,
            handoff_head,
            initial_schedule_digest,
            material_closure,
        } = &self.initial_anchor;
        source.validate()?;
        if source.binding_digest != expected_binding {
            return Err("checkpoint source differs from consumer binding".into());
        }
        hash(initial_schedule_digest)?;
        if *checkpoint_sequence != 0
            || *checkpoint_digest != source.origin_digest()?
            || handoff_head != handoff.handoff_head()
            || source.scope != self.target.scope
            || source.handoff_position != handoff_head.position
            || source.handoff_record_id != handoff_head.record_id
            || source.target_digest != handoff.target_digest()
            || source.activation_digest != handoff.activation_digest()
            || source.handoff_snapshot_root != handoff.raw_snapshot_root()
            || material_closure.handoff_material_identity != source.material_identity
            || material_closure.handoff_raw_snapshot_root != source.handoff_snapshot_root
        {
            return Err("checkpoint anchor differs from original qualified H".into());
        }
        let origin = &self.policy.origin;
        if origin.original_c0_checkpoint_digest != original_c0_pin
            || origin.formal_world != self.formal_read_trust.initial_trust.world
            || origin.bft_scope != self.target.scope
            || origin.initial_activation_digest
                != anchor
                    .initial_certificate()
                    .body
                    .activation_ref()
                    .map_err(|e| e.to_string())?
            || origin.handoff_head != *handoff_head
            || origin.handoff_target_digest != source.target_digest
            || origin.handoff_activation_digest != source.activation_digest
            || origin.handoff_raw_snapshot_root != source.handoff_snapshot_root
        {
            return Err("checkpoint policy has a different origin".into());
        }
        let start = self.initial_not_before_unix_ms;
        let end = self.initial_expires_at_unix_ms;
        let authenticated = self.initial_authenticated_at_unix_ms;
        let safe = self.initial_safety_valid_until_unix_ms;
        if authenticated > start
            || start >= end
            || end > safe
            || safe > self.policy.security_policy.absolute_safe_until_unix_ms
            || authenticated < self.policy.security_policy.security_not_before_unix_ms
            || end
                .checked_sub(authenticated)
                .is_none_or(|n| n > self.policy.max_checkpoint_validity_ms)
        {
            return Err("initial checkpoint validity exceeds original safety bounds".into());
        }
        for (lower, upper) in [
            (
                self.policy.root_not_before_unix_ms,
                self.policy.root_expires_at_unix_ms,
            ),
            (
                self.policy.policy_not_before_unix_ms,
                self.policy.policy_expires_at_unix_ms,
            ),
        ] {
            if authenticated < lower || end > upper {
                return Err("initial checkpoint exceeds root/policy window".into());
            }
        }
        Ok(PreparedBftCheckpointTrustConfigV1 { config: self })
    }

    pub fn decode_and_prepare(
        bytes: &[u8],
        policy_pin: &str,
        expected_binding: &str,
        original_c0_pin: &str,
    ) -> Result<PreparedBftCheckpointTrustConfigV1, String> {
        oasis7_formal_protocol::bft::checkpoint_trust_policy::preflight_checkpoint_config(bytes)?;
        let config: Self = serde_cbor::from_slice(bytes)
            .map_err(|e| format!("decode checkpoint configuration: {e}"))?;
        if canonical(&config)? != bytes {
            return Err("checkpoint configuration is not canonical CBOR".into());
        }
        config.prepare(policy_pin, expected_binding, original_c0_pin)
    }
}

/// New explicit entry: never used as a fallback for the old strict JSON loader.
pub fn load_checkpoint_trust_preparation(
    path: &Path,
    policy_pin: &str,
    expected_binding: &str,
    original_c0_pin: &str,
) -> Result<PreparedBftCheckpointTrustConfigV1, String> {
    if !path.is_absolute() {
        return Err("checkpoint trust path must be absolute".into());
    }
    // Keep the actual open bounded and refuse a replaced leaf symlink or
    // FIFO, even if a caller preflighted the file before this point.
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let file = options
        .open(path)
        .map_err(|e| format!("open checkpoint trust input: {e}"))?;
    if !file
        .metadata()
        .map_err(|e| format!("stat checkpoint trust input: {e}"))?
        .is_file()
    {
        return Err("checkpoint trust input must be a regular file".into());
    }
    let mut bytes = Vec::new();
    file.take(BFT_CHECKPOINT_CONFIG_MAX_BYTES_V1 as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    BftCheckpointTrustConfigV1::decode_and_prepare(
        &bytes,
        policy_pin,
        expected_binding,
        original_c0_pin,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_unsupported_anchor_instead_of_treating_it_as_handoff() {
        let bytes = canonical(&serde_json::json!({"kind":"successor_window_v1"})).unwrap();
        assert!(serde_cbor::from_slice::<BftCheckpointAnchorRefV1>(&bytes).is_err());
    }
    #[cfg(unix)]
    #[test]
    fn checkpoint_loader_refuses_symlink_and_fifo_without_waiting_for_writer() {
        use std::ffi::CString;
        use std::os::unix::{ffi::OsStrExt, fs::symlink};
        use std::time::{SystemTime, UNIX_EPOCH};

        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "oasis7-checkpoint-loader-{}-{nonce}",
            std::process::id()
        ));
        std::fs::create_dir(&root).unwrap();
        struct Cleanup(std::path::PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let _cleanup = Cleanup(root.clone());
        let regular = root.join("config.cbor");
        std::fs::write(&regular, b"invalid CBOR").unwrap();
        let linked = root.join("linked.cbor");
        symlink(&regular, &linked).unwrap();
        let fifo = root.join("blocked-fifo");
        let fifo_path = CString::new(fifo.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(fifo_path.as_ptr(), 0o600) }, 0);
        let pin = "11".repeat(32);

        for blocked in [&linked, &fifo] {
            let result = load_checkpoint_trust_preparation(blocked, &pin, &pin, &pin);
            let error = match result {
                Ok(_) => panic!("unsafe checkpoint file unexpectedly accepted"),
                Err(error) => error,
            };
            assert!(
                error.starts_with("open checkpoint trust input:")
                    || error == "checkpoint trust input must be a regular file",
                "unexpected checkpoint file error: {error}"
            );
        }
        // A real regular file reaches the existing canonical framing checks.
        assert!(load_checkpoint_trust_preparation(&regular, &pin, &pin, &pin).is_err());
    }

    #[test]
    fn rejects_oversize_and_legacy_config_at_the_new_entry() {
        let pin = "11".repeat(32);
        assert!(
            BftCheckpointTrustConfigV1::decode_and_prepare(
                &vec![0; BFT_CHECKPOINT_CONFIG_MAX_BYTES_V1 + 1],
                &pin,
                &pin,
                &pin
            )
            .is_err()
        );
        assert!(BftCheckpointTrustConfigV1::decode_and_prepare(b"{}", &pin, &pin, &pin).is_err());
        assert!(
            load_checkpoint_trust_preparation(Path::new("relative"), &pin, &pin, &pin).is_err()
        );
    }
}
