//! Exact original P/R pending material for the *existing* formal Host configuration.
//! Cold-reading this does not freeze a current Node owner, install a recovery,
//! prove protected custody, sign an issuer plan or grant new admission.
use std::path::Path;

use oasis7_distfs::controlled_authority::formal::{
    FormalQualifiedCommitProofV1, VerifiedFormalCommitV1, VerifiedRecovery,
    endpoint::{
        VerifiedNativeRecoveryPendingV1, read_native_recovery_pending_from_managed_archive,
        verify_native_recovery_original_legacy_cut,
    },
};
use oasis7_formal_protocol::controlled_authority::formal::VerifiedRecoveryPlanLegacyCutV1;

/// Historical original pending material, independently pinned by the caller.
/// Its shape, not this wrapper, establishes no present owner entitlement.
pub(super) struct HostOriginalRecoveryPendingV1 {
    pending: VerifiedNativeRecoveryPendingV1,
    authority_id: String,
}

impl HostOriginalRecoveryPendingV1 {
    pub(super) fn pending(&self) -> &VerifiedNativeRecoveryPendingV1 {
        &self.pending
    }
    pub(super) fn original_authority_id(&self) -> &str {
        &self.authority_id
    }
}

/// This entry is suitable for an already authorized, operator-owned writer
/// directory with an independently retained pending root and exact count.
/// It deliberately does not load a path from an unverified remote DTO.
pub(super) fn cold_read_pinned_host_pending(
    writer_directory: &Path,
    authority_id: &str,
    independent_root: &str,
    independent_count: usize,
) -> Result<HostOriginalRecoveryPendingV1, String> {
    if authority_id.is_empty() || independent_count > 4096 {
        return Err("invalid original Host recovery authority or pending count".into());
    }
    let pending = read_native_recovery_pending_from_managed_archive(
        writer_directory,
        "pending-handoff.cbor",
        independent_root,
    )
    .map_err(|error| error.to_string())?;
    if pending.handoff().requests.len() != independent_count {
        return Err("original Host recovery pending count differs from pin".into());
    }
    Ok(HostOriginalRecoveryPendingV1 {
        pending,
        authority_id: authority_id.to_owned(),
    })
}

/// Cold-join the actual shipping owner's Frozen original into the existing
/// verified Host path. The legacy config-root pending filename is NOT where
/// the real owner wrote its original: it lives under the exact operation and
/// incarnation's private `runtime-owner/operations/<digest>/original-pending`.
/// Require the original stopped/unknown owner receipt, BOTH prepared/Frozen
/// pins and an independently supplied root/count before re-opening the exact
/// immutable bytes as a typed native recovery source. This is still not a
/// protected recovery-plan decision or permission to start a new writer.
pub(super) fn cold_read_original_for_config(
    config_path: &Path,
    instance: &super::formal_live_runtime::FormalRuntimeInstanceV1,
    independent_authority_id: &str,
    independent_root: &str,
    independent_count: usize,
) -> Result<HostOriginalRecoveryPendingV1, String> {
    let config = super::formal_live_config::FormalLiveConfigV1::load(config_path)?;
    let actual_authority = config
        .anchor
        .authority()
        .authority_id()
        .map_err(|error| error.to_string())?;
    let world = &config.anchor.authority().definition().world;
    if actual_authority != independent_authority_id
        || instance.authority_id != actual_authority
        || instance.world_id != world.world_id
        || instance.chain_id != world.chain_id
        || instance.config_digest != super::node_lifecycle::local_io::sha256(config_path)?
    {
        return Err("original Host recovery owner or verified configuration differs".into());
    }
    super::controlled_live_config::require_private_owned(
        &config.config.writer_directory,
        true,
    )?;
    // This existing production owner path checks its exact durable owner
    // identity/epoch, non-serving status, independent root/count and byte-
    // identical immutable prepared+final originals. Never trust only a
    // config-root file or a local self-reported pin.
    let original = super::formal_live_runtime::cold_read_original_pending_for_owner(
        config_path,
        instance,
        independent_root,
        independent_count,
    )?;
    let operation_id = instance.operation_id.as_str();
    let operation_digest = super::node_lifecycle::local_io::digest(&operation_id)?;
    let directory = config
        .config
        .writer_directory
        .join("runtime-owner")
        .join("operations")
        .join(operation_digest)
        .join("original-pending");
    let read = cold_read_pinned_host_pending(
        &directory,
        independent_authority_id,
        independent_root,
        independent_count,
    )?;
    if read.pending().handoff() != &original {
        return Err("original Host recovery owner and cold bytes disagree".into());
    }
    Ok(read)
}

/// Reuse the exact original complete Qualified verifier. A caller must also
/// bind its separate protected owner, endpoint receipt and original floor;
/// returning a checked cut cannot activate that owner.
pub(super) fn verify_host_original_recovery_cut(
    proof: &FormalQualifiedCommitProofV1,
    original_recovery: &VerifiedRecovery,
    original_parent: &VerifiedFormalCommitV1,
    independent_proof_digest: &str,
) -> Result<VerifiedRecoveryPlanLegacyCutV1, String> {
    verify_native_recovery_original_legacy_cut(
        proof,
        original_recovery,
        original_parent,
        independent_proof_digest,
    )
    .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use oasis7_distfs::controlled_authority::LocalRequestIdentity;
    use oasis7_formal_protocol::controlled_authority::formal::{
        RecoveryPendingHandoffV1, RecoveryPendingRequestV1,
    };
    use std::sync::atomic::{AtomicU64, Ordering};

    struct Scratch(std::path::PathBuf);
    impl Scratch {
        fn new() -> Self {
            static N: AtomicU64 = AtomicU64::new(0);
            let nonce = N.fetch_add(1, Ordering::Relaxed);
            let dir = std::env::temp_dir().join(format!(
                "o7-host-original-pending-{}-{nonce}",
                std::process::id()
            ));
            std::fs::create_dir(&dir).unwrap();
            Self(dir)
        }
        fn path(&self) -> &Path {
            &self.0
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn cold_host_original_pending_preserves_signed_bytes_and_refuses_wrong_pins() {
        let scratch = Scratch::new();
        let original = RecoveryPendingHandoffV1::from_requests(vec![
            RecoveryPendingRequestV1 {
                identity: LocalRequestIdentity {
                    verified_subject: "subject".into(),
                    operation_domain: "submit".into(),
                    nonce_scope: "scope".into(),
                    request_id: "id".into(),
                },
                signed_request: vec![0, 255, 5, 9],
            },
        ])
        .unwrap();
        let root = original.root().unwrap();
        let file = scratch.path().join("pending-handoff.cbor");
        let exact_bytes = serde_cbor::to_vec(&original).unwrap();
        std::fs::write(&file, &exact_bytes).unwrap();

        for _ in 0..2 {
            let checked =
                cold_read_pinned_host_pending(scratch.path(), "original-authority", &root, 1)
                    .unwrap();
            assert_eq!(checked.original_authority_id(), "original-authority");
            assert_eq!(checked.pending().root(), root);
            assert_eq!(checked.pending().handoff(), &original);
        }
        assert!(cold_read_pinned_host_pending(scratch.path(), "", &root, 1).is_err());
        assert!(cold_read_pinned_host_pending(scratch.path(), "original-authority", &root, 0)
            .is_err());
        assert!(cold_read_pinned_host_pending(
            scratch.path(),
            "original-authority",
            &"f".repeat(64),
            1,
        )
        .is_err());

        std::fs::write(&file, b"corrupt original signed export").unwrap();
        assert!(cold_read_pinned_host_pending(scratch.path(), "original-authority", &root, 1)
            .is_err());
        std::fs::write(&file, &exact_bytes).unwrap();
        let reopened =
            cold_read_pinned_host_pending(scratch.path(), "original-authority", &root, 1)
                .unwrap();
        assert_eq!(reopened.pending().handoff(), &original);
    }
}
