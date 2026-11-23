//! Provider-side authorization for the existing healthy H boundary. The native
//! owner supplies its held H, never a caller-selected checkpoint or raw marker.
use super::{crypto::*, *};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HealthySealAuthorizationV1 {
    pub schema_version: u32,
    pub transition: SignedNormalDurableSetTransitionV1,
    pub predecessor_proof: FormalQualifiedCommitProofV1,
    pub boundary_proof: FormalQualifiedCommitProofV1,
}

#[cfg(test)]
#[path = "healthy_seal_tests.rs"]
mod tests;
impl HealthySealAuthorizationV1 {
    pub fn verify(
        &self,
        native: &proof::NativeHotBodyV1,
        authority: &VerifiedAuthorityV1,
    ) -> Result<VerifiedNormalDurableSetTransitionV1, FormalError> {
        let boundary = &self.boundary_proof.decision.record;
        if self.schema_version != 1
            || native.version != 1
            || native.authority_id != authority.authority_id()?
            || native.activation_ref != authority.activation_ref()
            || native.epoch != authority.definition().epoch
            || native.pending.is_some()
            || native.decision_head.is_some()
            || native.recovery_plan.is_some()
            || native.head.position != boundary.position
            || native.head.record_id != boundary.record_id()?
            || self.predecessor_proof.decision.record.record_id()? != boundary.parent_record_id
            || self.transition.body.predecessor.position
                != self.predecessor_proof.decision.record.position
            || self.transition.body.predecessor.record_id != boundary.parent_record_id
        {
            return Err(invalid(
                "healthy seal requires exact protected H and its actual parent",
            ));
        }
        // The only delegated ancestry here is H-1's location under the held H.
        // All its original writer/Frozen/final/complete signatures still run.
        // A legacy recovery prefix needs its original permission; this narrow
        // authorization must not reinterpret that prefix as a Normal record.
        let parent = proof::verify_normal_qualified_from_authenticated_native_ancestry(
            &self.predecessor_proof,
            authority,
        )?;
        let boundary =
            verify_formal_qualified(&self.boundary_proof, authority, Some(&parent), None)?;
        verify_normal_durable_set_transition(&self.transition, authority, &parent, &boundary)
    }
    pub fn verify_seal(
        &self,
        native: &proof::NativeHotBodyV1,
        authority: &VerifiedAuthorityV1,
        seal: &fencing::EpochSealBodyV1,
    ) -> Result<(), FormalError> {
        let transition = self.verify(native, authority)?;
        let instance = match seal.instance.endpoint_role {
            crate::controlled_authority::replicated_protocol::EndpointRole::Primary => {
                &authority.definition().primary.instance
            }
            crate::controlled_authority::replicated_protocol::EndpointRole::Replica => {
                &authority.definition().replica.instance
            }
        };
        if seal.schema_version != 1
            || seal.recovery_plan.is_some()
            || seal.writer_seal.is_some()
            || &seal.instance != instance
            || native.instance_digest != instance.digest().map_err(|e| invalid(e.to_string()))?
            || seal.recovery_operation != transition.digest()
            || seal.next_epoch_floor != transition.body().successor.epoch
            || seal.highest_complete.position != native.head.position
            || seal.highest_complete.decision_hash != native.head.record_id
            || !seal.highest_complete.qualified
            || seal.tail != fencing::SealedTailV1::empty()
            || seal.checkpoint_before.head != seal.highest_complete
        {
            return Err(invalid(
                "healthy seal differs from its fully verified original transition",
            ));
        }
        Ok(())
    }
}
