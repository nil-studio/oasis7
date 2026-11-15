//! Persistable light-read checkpoints. Only a digest pinned outside the service
//! response, or an independently verified initial commit, grants anchor trust.
use super::super::{crypto::*, *};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FormalReadCheckpointV1 {
    pub schema_version: u32,
    pub initial_certificate: SignedInitialActivationV1,
    pub active_authority: AuthorityDefinitionV1,
    pub recovery_policy: RecoveryPolicyV1,
    pub first_normal_position: Option<u64>,
    pub recovery_cut: Option<FormalHeadRefV1>,
    pub retained_record_id: Option<String>,
    pub head_record: CanonicalRecordV1,
    pub head_finalizing_authority_id: String,
}
impl FormalReadCheckpointV1 {
    pub fn digest(&self) -> Result<String, FormalError> {
        hash("oasis7.formal.externally_pinned_read_checkpoint.v1", self)
    }
}
#[derive(Debug, Clone)]
pub struct VerifiedFormalReadAnchorV1 {
    authority: VerifiedAuthorityV1,
    head: VerifiedFormalCommitV1,
    initial_certificate: SignedInitialActivationV1,
}
impl VerifiedFormalReadAnchorV1 {
    pub fn authority(&self) -> &VerifiedAuthorityV1 {
        &self.authority
    }
    pub fn head(&self) -> &VerifiedFormalCommitV1 {
        &self.head
    }
    pub fn initial_certificate(&self) -> &SignedInitialActivationV1 {
        &self.initial_certificate
    }
    /// Derive a separate historical anchor from inclusion beneath the current
    /// qualified business root. This never rewinds the caller's current anchor.
    pub fn from_committed_historical_cut(
        current: &Self,
        historical: &VerifiedCommittedHistoricalCutV1,
    ) -> Result<Self, FormalError> {
        let authority = historical.authority();
        let record = historical.commit().record();
        if historical.delegating_head() != &current.head.head()
            || authority.definition() != current.authority.definition()
            || authority.policy() != current.authority.policy()
            || authority.activation_ref() != current.authority.activation_ref()
            || record.world != current.head.record().world
            || record.position >= current.head.record().position
            || historical.commit().finalizing_authority_id() != authority.authority_id()?
        {
            return Err(invalid("committed historical anchor delegation binding"));
        }
        Ok(Self {
            authority: authority.clone(),
            head: historical.commit().clone(),
            initial_certificate: current.initial_certificate.clone(),
        })
    }
    /// Derive a historical anchor when the old authority was independently
    /// retained from the reader's verified activation/recovery lineage. This
    /// supports old Normal commits across authority epochs without trusting a
    /// provider-supplied authority definition.
    pub fn from_committed_historical_cut_with_verified_authority(
        current: &Self,
        historical: &VerifiedCommittedHistoricalCutV1,
    ) -> Result<Self, FormalError> {
        let authority = historical.authority();
        let record = historical.commit().record();
        if historical.delegating_head() != &current.head.head()
            || authority.definition().profile_version != 2
            || authority.definition().world != current.head.record().world
            || authority.activation_ref() != current.authority.activation_ref()
            || record.world != current.head.record().world
            || record.position >= current.head.record().position
            || record.original_epoch != authority.definition().epoch
            || record.original_authority_id != authority.authority_id()?
            || historical.commit().finalizing_authority_id() != authority.authority_id()?
        {
            return Err(invalid(
                "committed historical anchor lacks verified authority lineage",
            ));
        }
        Ok(Self {
            authority: authority.clone(),
            head: historical.commit().clone(),
            initial_certificate: current.initial_certificate.clone(),
        })
    }
    /// A separately derived historical recovery view. Its qualification and
    /// authority path were checked by the opaque current-root cut verifier.
    pub fn from_committed_historical_recovery_cut(
        current: &Self,
        historical: &VerifiedCommittedHistoricalCutV1,
    ) -> Result<Self, FormalError> {
        if historical.delegating_head() != &current.head.head()
            || historical.delegating_authority().definition() != current.authority.definition()
            || historical.delegating_authority().policy() != current.authority.policy()
            || historical.delegating_authority().activation_ref()
                != current.authority.activation_ref()
            || historical.commit().record().world != current.head.record().world
            || historical.commit().record().position >= current.head.record().position
            || historical.commit().finalizing_authority_id()
                != historical.authority().authority_id()?
            || historical.proof().decision.branch == FormalCompletionBranchV1::Normal
        {
            return Err(invalid("historical recovery anchor delegation"));
        }
        Ok(Self {
            authority: historical.authority().clone(),
            head: historical.commit().clone(),
            initial_certificate: current.initial_certificate.clone(),
        })
    }
    /// Private service publication recovery from a protected native prefix.
    /// This does not replace an ordinary client's externally configured pin,
    /// and it cannot recover a cut preceding that configured minimum. Clients
    /// must still verify every response against their own canonical anchor.
    pub fn from_native_historical_service_restore(
        trusted: &Self,
        restored: &super::NativeCheckpointVerdictV1,
        authority: &VerifiedAuthorityV1,
        commit: &VerifiedFormalCommitV1,
    ) -> Result<Self, FormalError> {
        let record = commit.record();
        if commit.head() != *restored.head()
            || commit.finalizing_authority_id() != restored.authority_id()
        {
            return Err(invalid("native historical exact checked provenance"));
        }
        if restored.minimum() != &trusted.head.head()
            || record.position < trusted.head.record().position
            || restored.current_head().position < record.position
            || authority.definition() != trusted.authority.definition()
            || authority.policy() != trusted.authority.policy()
            || authority.activation_ref() != trusted.authority.activation_ref()
            || record.world != trusted.head.record().world
            || record.activation_ref != trusted.authority.activation_ref()
            || commit.finalizing_authority_id() != authority.authority_id()?
        {
            return Err(invalid(
                "native historical service publication trust binding",
            ));
        }
        if record.position == trusted.head.record().position && record != trusted.head.record() {
            return Err(invalid(
                "native historical service publication minimum fork",
            ));
        }
        Ok(Self {
            authority: authority.clone(),
            head: commit.clone(),
            initial_certificate: trusted.initial_certificate.clone(),
        })
    }
    pub fn from_initial(
        certificate: &SignedInitialActivationV1,
        trust: &InitialActivationTrustV1,
        proof: &FormalQualifiedCommitProofV1,
    ) -> Result<Self, FormalError> {
        let authority = verify_initial_activation(certificate, trust)?;
        let head = verify_formal_qualified(proof, &authority, None, None)?;
        Ok(Self {
            authority,
            head,
            initial_certificate: certificate.clone(),
        })
    }
    /// Verify a Normal proof for the exact already pinned head under an
    /// independently verified historical authority. This cannot advance or
    /// replace the current anchor and is used only to seed typed origin proof
    /// evidence when an empty recovery changed authority at the same cut.
    pub fn from_verified_historical_normal_head(
        current: &Self,
        authority: &VerifiedAuthorityV1,
        proof: &FormalQualifiedCommitProofV1,
    ) -> Result<Self, FormalError> {
        let record = &proof.decision.record;
        if proof.decision.branch != FormalCompletionBranchV1::Normal
            || record != current.head.record()
            || record.world != current.head.record().world
            || record.activation_ref != current.authority.activation_ref()
            || record.original_epoch != authority.definition().epoch
            || record.original_authority_id != authority.authority_id()?
            || authority.definition().profile_version != 2
            || authority.definition().world != current.head.record().world
            || authority.activation_ref() != current.authority.activation_ref()
        {
            return Err(invalid(
                "historical Normal proof does not qualify exact pinned head",
            ));
        }
        let head = verify_formal_qualified(proof, authority, None, None)?;
        if head.record() != current.head.record()
            || head.finalizing_authority_id() != authority.authority_id()?
        {
            return Err(invalid("historical Normal proof differs from pinned head"));
        }
        Ok(Self {
            authority: authority.clone(),
            head,
            initial_certificate: current.initial_certificate.clone(),
        })
    }
    /// Pin must come from local authorized configuration or an independently
    /// trusted channel. Never derive it from a response/checkpoint being checked.
    pub fn from_fixed_pin(
        checkpoint: &FormalReadCheckpointV1,
        pinned_digest: &str,
        trust: &InitialActivationTrustV1,
    ) -> Result<Self, FormalError> {
        hash_hex(pinned_digest)?;
        if checkpoint.schema_version != 1 || checkpoint.digest()? != pinned_digest {
            return Err(invalid("externally pinned formal checkpoint mismatch"));
        }
        let initial = verify_initial_activation(&checkpoint.initial_certificate, trust)?;
        checkpoint
            .active_authority
            .verify_policy(&checkpoint.recovery_policy)?;
        checkpoint.head_record.validate()?;
        hash_hex(&checkpoint.head_finalizing_authority_id)?;
        if checkpoint.recovery_policy != *initial.policy()
            || checkpoint.active_authority.world != initial.definition().world
            || checkpoint.active_authority.profile != initial.definition().profile
            || checkpoint.active_authority.profile_version != initial.definition().profile_version
            || checkpoint.head_record.world != initial.definition().world
            || checkpoint.head_record.activation_ref != initial.activation_ref()
        {
            return Err(invalid(
                "fixed formal checkpoint initial world/policy binding",
            ));
        }
        if checkpoint.active_authority.epoch == 1 {
            if checkpoint.active_authority != *initial.definition()
                || checkpoint.first_normal_position != Some(1)
                || checkpoint.recovery_cut.is_some()
                || checkpoint.retained_record_id.is_some()
            {
                return Err(invalid("fixed initial authority checkpoint windows"));
            }
        } else {
            if checkpoint
                .active_authority
                .predecessor_authority_id
                .is_none()
                || checkpoint.recovery_cut.is_none()
            {
                return Err(invalid("fixed recovered authority checkpoint windows"));
            }
            let cut = checkpoint.recovery_cut.as_ref().expect("checked cut");
            hash_hex(&cut.record_id)?;
            if cut.position.checked_add(1) != Some(checkpoint.active_authority.valid_from_position)
            {
                return Err(invalid("fixed recovered authority cut/start"));
            }
        }
        if let Some(id) = &checkpoint.retained_record_id {
            hash_hex(id)?;
        }
        if checkpoint.head_finalizing_authority_id != checkpoint.active_authority.authority_id()?
            && !(checkpoint
                .active_authority
                .predecessor_authority_id
                .as_ref()
                == Some(&checkpoint.head_finalizing_authority_id)
                && (checkpoint.recovery_cut.as_ref()
                    == Some(&FormalHeadRefV1 {
                        position: checkpoint.head_record.position,
                        record_id: checkpoint.head_record.record_id()?,
                    })
                    || checkpoint.retained_record_id.as_ref()
                        == Some(&checkpoint.head_record.record_id()?)))
        {
            return Err(invalid(
                "fixed read checkpoint requires completed active authority head",
            ));
        }
        let authority = VerifiedAuthorityV1 {
            definition: checkpoint.active_authority.clone(),
            policy: checkpoint.recovery_policy.clone(),
            activation_ref: initial.activation_ref().to_owned(),
            initial: initial.initial.clone(),
            first_normal_position: checkpoint.first_normal_position,
            recovery_cut: checkpoint.recovery_cut.clone(),
            recovery_retained_record: checkpoint.retained_record_id.clone(),
            initial_input_digest: initial.initial_input_digest.clone(),
            durable_set_activation_digest: None,
        };
        // This construction is confined to an independently pinned checkpoint;
        // ordinary unpinned response data can never reach it.
        let mut verified_record_id = [0u8; 32];
        hex::decode_to_slice(
            &checkpoint.head_record.record_id()?,
            &mut verified_record_id,
        )
        .map_err(|_| invalid("verified commit cached record id"))?;
        let head = VerifiedFormalCommitV1 {
            verified_record_id,
            record: checkpoint.head_record.clone(),
            finalizing_authority_id: checkpoint.head_finalizing_authority_id.clone(),
        };
        Ok(Self {
            authority,
            head,
            initial_certificate: checkpoint.initial_certificate.clone(),
        })
    }
    pub fn checkpoint(&self) -> FormalReadCheckpointV1 {
        FormalReadCheckpointV1 {
            schema_version: 1,
            initial_certificate: self.initial_certificate.clone(),
            active_authority: self.authority.definition().clone(),
            recovery_policy: self.authority.policy().clone(),
            first_normal_position: self.authority.first_normal_position,
            recovery_cut: self.authority.recovery_cut.clone(),
            retained_record_id: self.authority.recovery_retained_record.clone(),
            head_record: self.head.record().clone(),
            head_finalizing_authority_id: self.head.finalizing_authority_id().to_owned(),
        }
    }
    pub fn after_verified_commit(
        &self,
        authority: VerifiedAuthorityV1,
        head: VerifiedFormalCommitV1,
    ) -> Result<Self, FormalError> {
        if authority.definition().world != self.authority.definition().world
            || authority.policy() != self.authority.policy()
            || authority.activation_ref() != self.authority.activation_ref()
            || head.record().activation_ref != self.authority.activation_ref()
            || head.record().world != self.head.record().world
            || head.record().position <= self.head.record().position
            || head.finalizing_authority_id() != authority.authority_id()?
        {
            return Err(invalid("formal read anchor advancement binding"));
        }
        Ok(Self {
            authority,
            head,
            initial_certificate: self.initial_certificate.clone(),
        })
    }
    /// A healthy replacement activates only the dual-sealed, dual-installed
    /// exact Qualified boundary; it provides no recovery/replay permission.
    pub fn with_durable_set_activation(
        &self,
        activation: &super::VerifiedDurableSetActivationV1,
    ) -> Result<Self, FormalError> {
        let successor = activation.successor();
        if activation.predecessor().context_digest()? != self.authority.context_digest()?
            || activation.boundary().record() != self.head.record()
            || activation.boundary().finalizing_authority_id()
                != self.head.finalizing_authority_id()
            || successor.definition().predecessor_authority_id.as_deref()
                != Some(self.authority.authority_id()?.as_str())
            || successor.definition().world != self.authority.definition().world
            || successor.policy() != self.authority.policy()
            || successor.activation_ref() != self.authority.activation_ref()
            || successor.durable_set_activation_digest.as_deref()
                != Some(activation.activation_digest())
        {
            return Err(invalid(
                "formal read healthy durable set activation binding",
            ));
        }
        Ok(Self {
            authority: successor.clone(),
            head: self.head.clone(),
            initial_certificate: self.initial_certificate.clone(),
        })
    }
    /// Installs a checked active epoch while retaining the exact qualified cut.
    /// This grants no qualification to its unresolved tail.
    pub fn with_pending_recovery(&self, recovery: &VerifiedRecovery) -> Result<Self, FormalError> {
        if recovery.predecessor().authority_id()? != self.authority.authority_id()?
            || recovery.body().sealed_cut != self.head.head()
        {
            return Err(invalid("read anchor pending recovery binding"));
        }
        Ok(Self {
            authority: recovery.successor().clone(),
            head: self.head.clone(),
            initial_certificate: self.initial_certificate.clone(),
        })
    }
    pub fn with_empty_recovery(&self, recovery: &VerifiedRecovery) -> Result<Self, FormalError> {
        if recovery.predecessor().authority_id()? != self.authority.authority_id()?
            || recovery.body().sealed_cut != self.head.head()
            || recovery.body().tail != RecoveryTailV1::Empty
        {
            return Err(invalid("read anchor empty-recovery binding"));
        }
        Ok(Self {
            authority: recovery.successor().clone(),
            head: self.head.clone(),
            initial_certificate: self.initial_certificate.clone(),
        })
    }
    /// Installs a checked schema-2 successor at the exact retained cut. This
    /// path requires the opaque previous recovery and typed context; schema-1
    /// pending/empty transitions above remain unchanged.
    pub fn with_verified_retained_recovery(
        &self,
        previous: &VerifiedRecovery,
        context: &VerifiedActiveRecoveryContextV2,
        recovery: &VerifiedRecovery,
    ) -> Result<Self, FormalError> {
        let previous_schema = previous.body().schema_version;
        let expected_cursor =
            if previous_schema == 2 || previous.body().tail == RecoveryTailV1::Empty {
                previous.successor()
            } else if previous_schema == 1 {
                previous.original_authority()
            } else {
                return Err(invalid("retained recovery previous schema"));
            };
        if recovery.body().schema_version != 2
            || recovery.body().retained_context.as_ref() != Some(context.commitment())
            || context.commitment().previous_recovery_certificate_digest
                != previous.certificate_digest()
            || context.cut().head() != self.head.head()
            || previous.body().sealed_cut != self.head.head()
            || context.active().authority_id()? != previous.successor().authority_id()?
            || recovery.predecessor().authority_id()? != context.active().authority_id()?
            || recovery.original_authority().authority_id()? != context.original().authority_id()?
            || self.authority.authority_id()? != expected_cursor.authority_id()?
            || self.authority.definition().world != context.original().definition().world
            || self.authority.activation_ref() != context.original().activation_ref()
            || self.authority.policy() != context.original().policy()
        {
            return Err(invalid("read anchor retained-recovery transition binding"));
        }
        Ok(Self {
            authority: recovery.successor().clone(),
            head: self.head.clone(),
            initial_certificate: self.initial_certificate.clone(),
        })
    }
    pub fn after_verified_recovery_commit(
        &self,
        recovery: &VerifiedRecovery,
        head: VerifiedFormalCommitV1,
    ) -> Result<Self, FormalError> {
        if recovery.predecessor().authority_id()? != self.authority.authority_id()?
            || recovery.body().sealed_cut != self.head.head()
            || self.head.record().position.checked_add(1) != Some(head.record().position)
            || head.record().parent_record_id != self.head.head().record_id
            || head.record().world != self.head.record().world
            || head.record().activation_ref != self.authority.activation_ref()
        {
            return Err(invalid("read anchor completed-recovery binding"));
        }
        let successor = recovery.successor().clone();
        if head.finalizing_authority_id() != successor.authority_id()? {
            let retained = match &recovery.body().tail {
                RecoveryTailV1::Prepared { record_id, .. }
                | RecoveryTailV1::Decision { record_id, .. } => record_id,
                _ => return Err(invalid("read anchor replay requires new finalizer proof")),
            };
            if head.finalizing_authority_id() != self.authority.authority_id()?
                || &head.head().record_id != retained
            {
                return Err(invalid("read anchor retained canonical identity"));
            }
        }
        Ok(Self {
            authority: successor,
            head,
            initial_certificate: self.initial_certificate.clone(),
        })
    }
    /// Advances after a retained completion verified under the original
    /// authority. The anchor may already hold the retained epoch's successor.
    pub fn after_verified_retained_recovery_commit(
        &self,
        context: &VerifiedActiveRecoveryContextV2,
        recovery: &VerifiedRecovery,
        head: VerifiedFormalCommitV1,
    ) -> Result<Self, FormalError> {
        if recovery.body().schema_version != 2
            || recovery.body().retained_context.as_ref() != Some(context.commitment())
            || context.cut().head() != self.head.head()
            || recovery.body().sealed_cut != self.head.head()
            || context.active().authority_id()? != recovery.predecessor().authority_id()?
            || context.original().authority_id()? != recovery.original_authority().authority_id()?
            || self.authority.authority_id()? != recovery.successor().authority_id()?
            || self.head.record().position.checked_add(1) != Some(head.record().position)
            || head.record().parent_record_id != self.head.head().record_id
            || head.record().world != self.head.record().world
            || head.record().activation_ref != self.authority.activation_ref()
        {
            return Err(invalid("read anchor retained-recovery completion binding"));
        }
        if head.finalizing_authority_id() != recovery.successor().authority_id()? {
            let retained = match &recovery.body().tail {
                RecoveryTailV1::Prepared { record_id, .. }
                | RecoveryTailV1::Decision { record_id, .. } => record_id,
                _ => return Err(invalid("retained completion requires successor finalizer")),
            };
            if head.finalizing_authority_id() != context.original().authority_id()?
                || &head.head().record_id != retained
            {
                return Err(invalid("retained completion original canonical identity"));
            }
        }
        Ok(Self {
            authority: recovery.successor().clone(),
            head,
            initial_certificate: self.initial_certificate.clone(),
        })
    }
}
