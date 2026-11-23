//! A one-way migration commitment for an already Qualified legacy completion.
//! No unverified proof bytes or raw digest can construct this authorization.
use super::*;

#[derive(Debug, Clone)]
pub struct VerifiedRecoveryPlanLegacyCutV1 {
    pub(super) cut: VerifiedFormalCommitV1,
    pub(super) proof_digest: String,
}

impl VerifiedRecoveryPlanLegacyCutV1 {
    pub fn verify(
        proof: &FormalQualifiedCommitProofV1,
        original_recovery: &VerifiedRecovery,
    ) -> Result<Self, FormalError> {
        if !matches!(original_recovery.body().schema_version, 1 | 2)
            || original_recovery.body().tail == RecoveryTailV1::Empty
            || !matches!(
                proof.decision.branch,
                FormalCompletionBranchV1::CompletePrepared | FormalCompletionBranchV1::ReplayFrozen
            )
        {
            return Err(invalid(
                "legacy plan cut requires its completed original recovery",
            ));
        }
        // This is the original full verifier: writer/Frozen signatures, original
        // parent, exact recovery branch/certificate, both final receipts and
        // both ProofComplete receipts all precede the digest commitment.
        let cut = verify_formal_qualified(
            proof,
            original_recovery.original_authority(),
            Some(original_recovery.sealed_commit()),
            Some(original_recovery),
        )?;
        if cut.finalizing_authority_id() != original_recovery.successor().authority_id()?
            || cut.record().original_epoch >= original_recovery.successor().definition().epoch
        {
            return Err(invalid("legacy plan cut original/finalizing epoch"));
        }
        Ok(Self {
            cut,
            proof_digest: formal_qualified_proof_digest(proof)?,
        })
    }
    pub fn cut(&self) -> &VerifiedFormalCommitV1 {
        &self.cut
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::controlled_authority::formal::test_fixtures::*;

    #[test]
    fn legacy_plan_pins_fully_verified_original_qualified_bytes_for_both_completion_branches() {
        for branch in [
            FormalCompletionBranchV1::CompletePrepared,
            FormalCompletionBranchV1::ReplayFrozen,
        ] {
            let (intent, original, _, frozen, record) =
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
            let legacy = VerifiedRecoveryPlanLegacyCutV1::verify(&completed, &recovery).unwrap();
            let authority = recovery.successor();
            let mut successor = authority.definition().clone();
            successor.epoch = 3;
            successor.writer_public_key = public(13);
            successor.predecessor_authority_id = Some(authority.authority_id().unwrap());
            successor.valid_from_position = legacy.cut().head().position + 1;
            for (binding, n) in [(&mut successor.primary, 14), (&mut successor.replica, 15)] {
                binding.instance.authority_epoch = 3;
                binding.instance.endpoint_public_key = public(n);
                binding.instance.endpoint_id = format!("epoch3-{n}");
                binding.instance.instance_id = h(n + 20);
                binding.instance.storage_id = h(n + 30);
                binding.instance.storage_fingerprint = h(n + 40);
                binding.fault_domain_id = format!("epoch3-domain-{n}");
            }
            let template = RecoveryCertificateBodyV1 {
                schema_version: 1,
                world: successor.world.clone(),
                operation_id: h(151),
                predecessor_authority_id: authority.authority_id().unwrap(),
                predecessor_epoch: 2,
                sealed_cut: legacy.cut().head(),
                tail: RecoveryTailV1::Empty,
                fencing_evidence_digest: h(0),
                successor,
                first_normal_position: Some(4),
                retained_context: None,
            };
            let pending = RecoveryPendingHandoffV1::empty();
            let fixed = RecoveryTailWitnessV1::Empty;
            let body = RecoveryPlanBodyV1::new(
                template,
                authority,
                legacy.cut(),
                &fixed,
                &pending,
                RecoveryPlanAuthorityV1 {
                    recovery_credential_public_key: public(28),
                    isolation_evidence_digest: h(152),
                },
            )
            .unwrap()
            .with_verified_legacy_cut(&legacy)
            .unwrap();
            assert!(
                VerifiedRecoveryPlanV1::verify(
                    body.clone(),
                    authority,
                    legacy.cut(),
                    &fixed,
                    &pending
                )
                .is_err()
            );
            let checked = VerifiedRecoveryPlanV1::verify_after_legacy_completion(
                body,
                authority,
                legacy.cut(),
                &fixed,
                &pending,
                &legacy,
            )
            .unwrap();
            let plan = SignedRecoveryPlanV1::sign(&checked, &key(1)).unwrap();
            let drained = RecoveryDrainedProofV1 {
                schema_version: 1,
                plan_digest: plan.digest().unwrap(),
                fixed,
                qualified: completed.clone(),
            };
            assert_eq!(
                drained.verify(&plan, authority).unwrap(),
                legacy.cut().head()
            );
            for variant in 0..4 {
                let mut changed = completed.clone();
                match variant {
                    0 => changed.decision.writer_signature_hex = "00".repeat(64),
                    1 => changed.decision.primary.body.recovery_certificate_digest = Some(h(153)),
                    2 => changed.replica_complete.signature_hex = "00".repeat(64),
                    _ => changed.decision.branch = FormalCompletionBranchV1::Normal,
                }
                assert!(VerifiedRecoveryPlanLegacyCutV1::verify(&changed, &recovery).is_err());
                let mut changed_drained = drained.clone();
                changed_drained.qualified = changed;
                assert!(
                    changed_drained.verify(&plan, authority).is_err(),
                    "{branch:?}/{variant}"
                );
            }
        }
    }
}
