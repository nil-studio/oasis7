//! Bounded, independently pinned *original* P/R recovery input cold reader.
//!
//! Dark-only source preparation: no endpoint write, privilege, live switch or
//! authorization is granted by reading a file or verifying a historical cut.
//! The caller must supply independently authenticated root/proof pins.
use super::*;
use oasis7_formal_protocol::controlled_authority::formal::{
    FormalQualifiedCommitProofV1, RECOVERY_PENDING_HANDOFF_ENCODED_LIMIT_V1,
    RecoveryPendingHandoffV1, VerifiedFormalCommitV1, VerifiedRecovery,
    VerifiedRecoveryPlanLegacyCutV1,
};
use std::{fs::File, io::Read, path::Path};

/// Bound the encoded representation, not just the total raw signed payloads.
/// The protocol's 16 MiB raw budget allows up to 34 MiB of CBOR byte strings.
pub const NATIVE_RECOVERY_ORIGINAL_FILE_LIMIT_V1: usize = RECOVERY_PENDING_HANDOFF_ENCODED_LIMIT_V1;

/// An original pending handoff whose *exact* canonical file was verified
/// against an independent caller-supplied root. It is not an execution grant.
#[derive(Debug, Clone)]
pub struct VerifiedNativeRecoveryPendingV1 {
    handoff: RecoveryPendingHandoffV1,
    root: String,
}

impl VerifiedNativeRecoveryPendingV1 {
    pub fn handoff(&self) -> &RecoveryPendingHandoffV1 {
        &self.handoff
    }

    pub fn root(&self) -> &str {
        &self.root
    }

    /// Cold-read original signed requests without fabricating a new request,
    /// acceptance, or pending lineage. The independently protected root MUST
    /// come from an authenticated source; the file itself supplies no trust.
    pub fn read(path: &Path, independent_root: &str) -> Result<Self, FormalError> {
        let file = File::open(path).map_err(|error| FormalError::Io(error.to_string()))?;
        if file
            .metadata()
            .map_err(|error| FormalError::Io(error.to_string()))?
            .len()
            > NATIVE_RECOVERY_ORIGINAL_FILE_LIMIT_V1 as u64
        {
            return Err(invalid("native original recovery encoded byte limit"));
        }
        let mut bounded = file.take(NATIVE_RECOVERY_ORIGINAL_FILE_LIMIT_V1 as u64 + 1);
        let mut original_bytes = Vec::new();
        bounded
            .read_to_end(&mut original_bytes)
            .map_err(|error| FormalError::Io(error.to_string()))?;
        if original_bytes.is_empty()
            || original_bytes.len() > NATIVE_RECOVERY_ORIGINAL_FILE_LIMIT_V1
        {
            return Err(invalid("native original recovery encoded byte limit"));
        }

        let handoff: RecoveryPendingHandoffV1 =
            serde_cbor::from_slice(&original_bytes).map_err(|error| invalid(error.to_string()))?;
        // Reject alternative encodings rather than silently replacing the
        // exact persisted original with a normalized reinterpretation.
        if serde_cbor::to_vec(&handoff).map_err(|error| invalid(error.to_string()))?
            != original_bytes
        {
            return Err(invalid("native original recovery noncanonical original"));
        }
        let root = handoff.root()?; // enforces identity/order/duplicates/raw budget
        if root != independent_root {
            return Err(invalid(
                "native original recovery independent root mismatch",
            ));
        }
        Ok(Self { handoff, root })
    }
}

/// Check the *original* complete Qualified cut and every existing signed
/// final/proof-complete receipt, then bind its digest to an independent pin.
/// Does not adopt a new root or authorize any post-recovery request.
pub fn verify_native_recovery_original_legacy_cut(
    proof: &FormalQualifiedCommitProofV1,
    original_recovery: &VerifiedRecovery,
    original_parent: &VerifiedFormalCommitV1,
    independent_proof_digest: &str,
) -> Result<VerifiedRecoveryPlanLegacyCutV1, FormalError> {
    let checked =
        VerifiedRecoveryPlanLegacyCutV1::verify(proof, original_recovery, original_parent)?;
    if checked.proof_digest() != independent_proof_digest {
        return Err(invalid(
            "native recovery original Qualified proof pin mismatch",
        ));
    }
    Ok(checked)
}

#[cfg(test)]
mod tests {
    use super::*;
    use oasis7_formal_protocol::controlled_authority::{
        LocalRequestIdentity, formal::RecoveryPendingRequestV1,
    };
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TEST_FILE: AtomicU64 = AtomicU64::new(0);

    struct TestFile(std::path::PathBuf);
    impl TestFile {
        fn new() -> Self {
            let n = NEXT_TEST_FILE.fetch_add(1, Ordering::Relaxed);
            Self(
                std::env::temp_dir()
                    .join(format!("o7-original-cold-{}-{n}.cbor", std::process::id())),
            )
        }
    }
    impl Drop for TestFile {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    fn request(id: &str, signed: &[u8]) -> RecoveryPendingRequestV1 {
        RecoveryPendingRequestV1 {
            identity: LocalRequestIdentity {
                verified_subject: "pinned-subject".into(),
                operation_domain: "world-submit".into(),
                nonce_scope: "original-nonce".into(),
                request_id: id.into(),
            },
            signed_request: signed.to_vec(),
        }
    }

    #[test]
    fn cold_read_preserves_exact_original_identity_and_signed_bytes() {
        let file = TestFile::new();
        let handoff = RecoveryPendingHandoffV1::from_requests(vec![
            request("b", &[0, 255, 0, 6]),
            request("a", &[1, 2, 3]),
        ])
        .unwrap();
        let root = handoff.root().unwrap();
        std::fs::write(&file.0, serde_cbor::to_vec(&handoff).unwrap()).unwrap();

        for _ in 0..2 {
            let checked = VerifiedNativeRecoveryPendingV1::read(&file.0, &root).unwrap();
            assert_eq!(checked.root(), root);
            assert_eq!(checked.handoff(), &handoff);
            assert_eq!(checked.handoff().requests[0].signed_request, [1, 2, 3]);
        }
        assert!(VerifiedNativeRecoveryPendingV1::read(&file.0, &"f".repeat(64)).is_err());
        let mut changed = handoff.clone();
        changed.requests[0].signed_request.push(9);
        std::fs::write(&file.0, serde_cbor::to_vec(&changed).unwrap()).unwrap();
        assert!(VerifiedNativeRecoveryPendingV1::read(&file.0, &root).is_err());
    }

    #[test]
    fn cold_reader_rejects_reorder_duplicate_corruption_and_oversized_file() {
        let file = TestFile::new();
        let handoff =
            RecoveryPendingHandoffV1::from_requests(vec![request("a", &[1]), request("b", &[2])])
                .unwrap();
        let root = handoff.root().unwrap();
        let mut reversed = handoff.clone();
        reversed.requests.reverse();
        std::fs::write(&file.0, serde_cbor::to_vec(&reversed).unwrap()).unwrap();
        assert!(VerifiedNativeRecoveryPendingV1::read(&file.0, &root).is_err());

        let duplicated = RecoveryPendingHandoffV1 {
            schema_version: 1,
            requests: vec![request("a", &[1]), request("a", &[1])],
        };
        std::fs::write(&file.0, serde_cbor::to_vec(&duplicated).unwrap()).unwrap();
        assert!(VerifiedNativeRecoveryPendingV1::read(&file.0, &root).is_err());
        std::fs::write(&file.0, [0xff, 0]).unwrap();
        assert!(VerifiedNativeRecoveryPendingV1::read(&file.0, &root).is_err());
        let oversized = File::create(&file.0).unwrap();
        oversized
            .set_len(NATIVE_RECOVERY_ORIGINAL_FILE_LIMIT_V1 as u64 + 1)
            .unwrap();
        drop(oversized);
        assert!(VerifiedNativeRecoveryPendingV1::read(&file.0, &root).is_err());
    }

    #[test]
    fn cold_reader_rejects_noncanonical_encoding_with_correct_root() {
        let file = TestFile::new();
        let handoff = RecoveryPendingHandoffV1::from_requests(vec![request("a", &[1])]).unwrap();
        let root = handoff.root().unwrap();
        let canonical = serde_cbor::to_vec(&handoff).unwrap();
        // The same two-entry map with a non-minimal length encoding.
        assert_eq!(canonical[0], 0xa2);
        let mut noncanonical = vec![0xb8, 2];
        noncanonical.extend_from_slice(&canonical[1..]);
        let decoded: RecoveryPendingHandoffV1 = serde_cbor::from_slice(&noncanonical).unwrap();
        assert_eq!(decoded, handoff);
        assert_eq!(decoded.root().unwrap(), root);
        std::fs::write(&file.0, &canonical).unwrap();
        assert!(VerifiedNativeRecoveryPendingV1::read(&file.0, &root).is_ok());
        std::fs::write(&file.0, &noncanonical).unwrap();
        let error = VerifiedNativeRecoveryPendingV1::read(&file.0, &root).unwrap_err();
        assert!(error.to_string().contains("noncanonical original"));
    }

    #[test]
    fn original_qualified_cut_requires_the_independent_proof_pin() {
        use oasis7_formal_protocol::controlled_authority::formal::{
            FormalCompletionBranchV1, formal_qualified_proof_digest,
            test_fixtures::{proof, recovery_fixture_with_profile, verified_recovery},
        };

        for branch in [
            FormalCompletionBranchV1::CompletePrepared,
            FormalCompletionBranchV1::ReplayFrozen,
        ] {
            let (intent, original, parent, frozen, record) =
                recovery_fixture_with_profile(branch, None, 10, 2);
            let recovery = verified_recovery(&intent, &original);
            let completed = proof(
                record,
                recovery.successor(),
                branch,
                Some(recovery.certificate_digest().into()),
                Some(frozen),
                if branch == FormalCompletionBranchV1::CompletePrepared {
                    4
                } else {
                    10
                },
                11,
                12,
            );
            let pin = formal_qualified_proof_digest(&completed).unwrap();
            let checked =
                verify_native_recovery_original_legacy_cut(&completed, &recovery, &parent, &pin)
                    .unwrap();
            assert_eq!(checked.proof_digest(), pin);
            assert_eq!(checked.cut().record(), &completed.decision.record);
            let wrong_pin = if pin == "0".repeat(64) {
                "1".repeat(64)
            } else {
                "0".repeat(64)
            };
            let error = verify_native_recovery_original_legacy_cut(
                &completed, &recovery, &parent, &wrong_pin,
            )
            .err()
            .expect("the independently pinned digest must match");
            assert!(error.to_string().contains("proof pin mismatch"));

            let mut corrupted = completed;
            corrupted.replica_complete.signature_hex = "00".repeat(64);
            let corrupted_pin = formal_qualified_proof_digest(&corrupted).unwrap();
            assert!(
                verify_native_recovery_original_legacy_cut(
                    &corrupted,
                    &recovery,
                    &parent,
                    &corrupted_pin,
                )
                .is_err()
            );
        }
    }
}
