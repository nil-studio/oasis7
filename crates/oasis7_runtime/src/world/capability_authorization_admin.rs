//! Administrative admission APIs for proof-bearing capability authority state.
//!
//! These APIs deliberately keep the legacy record/certificate entry point
//! fail-closed.  Authority is a trust root and can only enter the journal
//! together with a quorum-signed binding that replay can verify.

use super::super::capability_authorization::{
    CapabilityAgentIdentity, CapabilityAuthorityFinalityProof, CapabilityAuthorityRecord,
};
use super::super::governance::GovernanceFinalityCertificate;
use super::super::{CapabilityAuthorizationEvent, WorldError};
use super::World;
use super::capability_authorization::{deny, validate_agent_identity, validate_authority_record};

impl World {
    /// Attempt to install an authority record without the proof that binds
    /// every signed authority field to historical finality.
    ///
    /// This compatibility entry point intentionally remains rejected.  A
    /// process-local certificate is not a sufficient trust root.
    pub fn install_capability_authority_record_with_finality(
        &mut self,
        record: CapabilityAuthorityRecord,
        certificate: GovernanceFinalityCertificate,
    ) -> Result<(), WorldError> {
        let _ = (record, certificate);
        Err(deny(
            "authority installation requires a capability authority finality proof",
        ))
    }

    /// Install an authority record with a quorum-signed capability binding.
    /// The proof repeats the historical governance certificate and signs all
    /// authority metadata so replay can verify the exact record admitted.
    pub fn install_capability_authority_record_with_finality_proof(
        &mut self,
        record: CapabilityAuthorityRecord,
        proof: CapabilityAuthorityFinalityProof,
    ) -> Result<(), WorldError> {
        self.verify_capability_authorization_root()?;
        validate_authority_record(&record)?;
        self.verify_capability_authority_finality_proof(&record, &proof)?;
        if self.chain_resource_manifest.world_id != "unbound"
            && self.chain_resource_manifest.world_id != record.world_id
        {
            return Err(deny("authority record world does not match live world"));
        }
        self.append_capability_authorization_event_batch(vec![
            CapabilityAuthorizationEvent::AuthorityInstalledWithProof { record, proof },
        ])?;
        Ok(())
    }

    /// Apply a quorum-proof-bearing revocation transition.  The updated
    /// authority record is journaled as the transition evidence; replay
    /// verifies that it is monotonic before exposing the new revocation set.
    pub fn revoke_capability_grant_v2_with_finality_proof(
        &mut self,
        grant_id: &str,
        mut record: CapabilityAuthorityRecord,
        proof: CapabilityAuthorityFinalityProof,
    ) -> Result<(), WorldError> {
        if grant_id.trim().is_empty() {
            return Err(deny("capability revocation grant id is required"));
        }
        record.revoked_grant_ids.insert(grant_id.to_string());
        self.install_capability_authority_record_with_finality_proof(record, proof)
    }

    /// Apply a proof-bearing immutable supersession transition. The
    /// replacement may be issued in the same governed batch; replay checks
    /// canonical ids, monotonic state, and cycle freedom before use.
    pub fn supersede_capability_grant_v2_with_finality_proof(
        &mut self,
        grant_id: &str,
        replacement_grant_id: &str,
        mut record: CapabilityAuthorityRecord,
        proof: CapabilityAuthorityFinalityProof,
    ) -> Result<(), WorldError> {
        if grant_id.trim().is_empty() || replacement_grant_id.trim().is_empty() {
            return Err(deny("capability supersession grant ids are required"));
        }
        if grant_id == replacement_grant_id {
            return Err(deny("capability supersession cannot target itself"));
        }
        record
            .superseded_by
            .insert(grant_id.to_string(), replacement_grant_id.to_string());
        self.install_capability_authority_record_with_finality_proof(record, proof)
    }

    /// Apply a proof-bearing trust-root/key rotation.  The authority
    /// transition validator requires a newer key epoch and a non-empty
    /// rotation receipt, and immediately stops accepting grants signed by the
    /// prior key epoch.
    pub fn rotate_capability_trust_root_with_finality_proof(
        &mut self,
        record: CapabilityAuthorityRecord,
        proof: CapabilityAuthorityFinalityProof,
    ) -> Result<(), WorldError> {
        self.install_capability_authority_record_with_finality_proof(record, proof)
    }

    pub fn revoke_capability_grant_v2_with_proof(
        &mut self,
        grant_id: &str,
        record: CapabilityAuthorityRecord,
        proof: CapabilityAuthorityFinalityProof,
    ) -> Result<(), WorldError> {
        self.revoke_capability_grant_v2_with_finality_proof(grant_id, record, proof)
    }

    pub fn supersede_capability_grant_v2_with_proof(
        &mut self,
        grant_id: &str,
        replacement_grant_id: &str,
        record: CapabilityAuthorityRecord,
        proof: CapabilityAuthorityFinalityProof,
    ) -> Result<(), WorldError> {
        self.supersede_capability_grant_v2_with_finality_proof(
            grant_id,
            replacement_grant_id,
            record,
            proof,
        )
    }

    pub fn rotate_capability_trust_root_with_proof(
        &mut self,
        record: CapabilityAuthorityRecord,
        proof: CapabilityAuthorityFinalityProof,
    ) -> Result<(), WorldError> {
        self.rotate_capability_trust_root_with_finality_proof(record, proof)
    }

    /// Install the durable owner/generation binding for a live Agent subject.
    /// Agent gameplay state does not carry capability credentials.  The host
    /// must therefore install this binding before an Agent grant can execute;
    /// each generation is journaled and immutable, while a strictly newer
    /// generation may replace it after a transfer/reset/re-claim.
    pub fn install_capability_agent_identity(
        &mut self,
        agent_id: impl Into<String>,
        owner_binding: impl Into<String>,
        generation: u64,
    ) -> Result<(), WorldError> {
        self.verify_capability_authorization_root()?;
        let agent_id = agent_id.into();
        let identity = CapabilityAgentIdentity {
            owner_binding: owner_binding.into(),
            generation,
        };
        validate_agent_identity(agent_id.as_str(), &identity)?;
        if !self.state.agents.contains_key(agent_id.as_str()) {
            return Err(deny("capability agent identity requires a live agent"));
        }
        if let Some(existing) = self
            .capability_revocation_state
            .agent_identities
            .get(agent_id.as_str())
        {
            if generation < existing.generation {
                return Err(deny("capability agent identity generation regressed"));
            }
            if generation == existing.generation && existing != &identity {
                return Err(deny(
                    "capability agent identity changed without a new generation",
                ));
            }
        }
        if self
            .capability_revocation_state
            .agent_identities
            .get(agent_id.as_str())
            == Some(&identity)
        {
            return Ok(());
        }
        self.append_capability_authorization_event_batch(vec![
            CapabilityAuthorizationEvent::AgentIdentityInstalled { agent_id, identity },
        ])?;
        Ok(())
    }
}
