//! Explicit process-local capability for a pinned DevLocal operator.
use super::{ReleaseSecurityPolicy, World, WorldError};
use crate::CapabilityAuthorizationEvent;
use crate::service_admission::{SignedReadRequest, participant::*, participant_policy::*};
use oasis7_client_api::world_service::private_goal::verify_owner_signature_strict;
use oasis7_proto::storage_profile::StorageProfile;
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DevLocalParticipantAuthority {
    operator_public_key: String,
    release_security_policy: ReleaseSecurityPolicy,
}
fn deny(reason: impl Into<String>) -> WorldError {
    WorldError::DistributedValidationFailed {
        reason: reason.into(),
    }
}
impl DevLocalParticipantAuthority {
    pub fn new(
        profile: StorageProfile,
        release: &ReleaseSecurityPolicy,
        operator_public_key: String,
    ) -> Result<Self, WorldError> {
        if profile != StorageProfile::DevLocal || release.is_production_hardened() {
            return Err(deny(
                "DevLocal participant authority requires effective DevLocal non-hardened policy",
            ));
        }
        let bytes: [u8; 32] = hex::decode(&operator_public_key)
            .map_err(|_| deny("invalid local operator pin"))?
            .try_into()
            .map_err(|_| deny("invalid local operator pin"))?;
        if hex::encode(bytes) != operator_public_key
            || ed25519_dalek::VerifyingKey::from_bytes(&bytes)
                .map_err(|_| deny("invalid local operator pin"))?
                .is_weak()
        {
            return Err(deny("invalid local operator pin"));
        }
        Ok(Self {
            operator_public_key,
            release_security_policy: release.clone(),
        })
    }
    pub fn operator_public_key(&self) -> &str {
        &self.operator_public_key
    }
    pub fn release_security_policy(&self) -> &ReleaseSecurityPolicy {
        &self.release_security_policy
    }
}
impl World {
    /// Validate the explicit local capability against the current candidate before publication.
    /// Worlds without this capability retain their existing governed behavior.
    pub fn validate_devlocal_participant_authority_context(&self) -> Result<(), WorldError> {
        if self.devlocal_participant_authority.is_some() {
            self.validate_devlocal_participant_context()?;
        }
        Ok(())
    }
    pub fn devlocal_participant_authority(&self) -> Option<&DevLocalParticipantAuthority> {
        self.devlocal_participant_authority.as_ref()
    }
    pub fn with_devlocal_participant_authority(
        mut self,
        authority: DevLocalParticipantAuthority,
    ) -> Result<Self, WorldError> {
        // The explicit Host capability contains effective policy, never snapshot-selected authority.
        if self.release_security_policy.is_production_hardened() {
            return Err(deny(
                "hardened world rejects DevLocal participant authority",
            ));
        }
        self.release_security_policy = authority.release_security_policy.clone();
        self.devlocal_participant_authority = Some(authority);
        self.validate_devlocal_participant_context()?;
        Ok(self)
    }
    pub(super) fn validate_devlocal_participant_context(
        &self,
    ) -> Result<&DevLocalParticipantAuthority, WorldError> {
        let context = self
            .devlocal_participant_authority
            .as_ref()
            .ok_or_else(|| deny("explicit DevLocal participant authority required"))?;
        if self.release_security_policy.is_production_hardened()
            || self.release_security_policy != context.release_security_policy
            || self.formal_history_reader.is_some()
            || self.state.retained_history_root.is_some()
            || self.state.governance_finality_signer_registry.is_some()
            || !self.state.governance_validator_admissions.is_empty()
            || !self.governance_finality_epoch_snapshots.is_empty()
            || !self.rollback_authority_registry.is_empty()
            || !self
                .capability_revocation_state
                .authority_finality_proofs
                .is_empty()
            || !self
                .capability_revocation_state
                .participants
                .policy_history
                .is_empty()
        {
            return Err(deny(
                "DevLocal participant authority conflicts with effective world authority",
            ));
        }
        Ok(context)
    }
    pub(crate) fn validate_devlocal_participant_authorization(
        &self,
        policy: &ParticipantEntryPolicyV1,
        signed: &SignedReadRequest<DevLocalParticipantPolicyAuthorizationV1>,
    ) -> Result<(), WorldError> {
        let context = self.validate_devlocal_participant_context()?;
        if signed.subject_public_key != context.operator_public_key
            || &signed.request.policy != policy
        {
            return Err(deny("DevLocal participant operator or policy mismatch"));
        }
        verify_owner_signature_strict(
            devlocal_policy_signing_domain(policy.generation),
            &signed.request,
            &signed.subject_public_key,
            &signed.signature_hex,
        )
        .map_err(deny)?;
        self.validate_participant_manifest(policy, &signed.request.manifest)
    }
    pub fn install_devlocal_participant_policy(
        &mut self,
        policy: ParticipantEntryPolicyV1,
        authorization: SignedReadRequest<DevLocalParticipantPolicyAuthorizationV1>,
    ) -> Result<(), WorldError> {
        self.verify_capability_authorization_root()?;
        self.validate_devlocal_participant_authorization(&policy, &authorization)?;
        if self
            .capability_revocation_state
            .participants
            .devlocal_policy_history
            .last()
            == Some(&authorization)
            && self
                .capability_revocation_state
                .participants
                .policy
                .as_ref()
                == Some(&policy)
        {
            return Ok(());
        }
        self.append_capability_authorization_event_batch(vec![
            CapabilityAuthorizationEvent::DevLocalParticipantPolicyInstalled { authorization },
        ])?;
        Ok(())
    }
}
pub(super) fn project_devlocal_policy(
    world: &World,
    registry: &mut crate::capability_authorization::CapabilityRevocationState,
    signed: &SignedReadRequest<DevLocalParticipantPolicyAuthorizationV1>,
) -> Result<(), WorldError> {
    if !registry.participants.policy_history.is_empty() {
        return Err(deny(
            "governed participant authority cannot become DevLocal",
        ));
    }
    world.validate_devlocal_participant_authorization(&signed.request.policy, signed)?;
    super::participant_policy::project_authorized_policy(
        world,
        registry,
        &signed.request.policy,
        &signed.request.manifest,
    )?;
    registry
        .participants
        .devlocal_policy_history
        .push(signed.clone());
    Ok(())
}
