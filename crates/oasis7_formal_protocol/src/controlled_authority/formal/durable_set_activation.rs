//! Installation attestation is a trusted-provider contract, not a local status.
//! No native endpoint currently issues this new receipt; absent evidence fails.
use super::{crypto::*, *};
use crate::controlled_authority::replicated_protocol::EndpointRole;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DurableSetInstallBodyV1 {
    pub schema_version: u32,
    pub transition_digest: String,
    pub successor_authority_id: String,
    pub instance: fencing::EndpointInstanceBindingV1,
    pub boundary: FormalHeadRefV1,
    pub independent_closure_root: String,
    /// Final H logical object inventory independently checked by the provider.
    pub independent_inventory_digest: String,
    pub checkpoint: fencing::EndpointCheckpointBindingV1,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DurableSetInstallReceiptV1 {
    pub body: DurableSetInstallBodyV1,
    pub endpoint_signature_hex: String,
    /// Explicit provider attestation of independent closure and exact install.
    pub provider_signature_hex: String,
    pub provider_evidence: fencing::MonotonicProviderEvidenceV1,
}
pub fn durable_set_install_endpoint_signing_bytes(
    body: &DurableSetInstallBodyV1,
) -> Result<Vec<u8>, FormalError> {
    bytes("oasis7.formal.durable_set_install.endpoint.v1", body)
}
pub fn durable_set_install_provider_signing_bytes(
    body: &DurableSetInstallBodyV1,
) -> Result<Vec<u8>, FormalError> {
    bytes("oasis7.formal.durable_set_install.provider.v1", body)
}
pub struct VerifiedDurableSetActivationV1 {
    successor: VerifiedAuthorityV1,
    predecessor: VerifiedAuthorityV1,
    activation_digest: String,
    boundary: VerifiedFormalCommitV1,
    transition_digest: String,
}
impl VerifiedDurableSetActivationV1 {
    /// Checks public marker/read-step evidence against THIS opaque permission.
    /// It neither derives a new permission nor promotes receipt/status fields.
    pub fn verify_evidence(
        &self,
        signed: &SignedNormalDurableSetTransitionV1,
        old_seals: [&fencing::EpochSealReceiptV1; 2],
        new_installs: [&DurableSetInstallReceiptV1; 2],
    ) -> Result<(), FormalError> {
        signed.body.validate(&self.predecessor)?;
        if signed.body.digest()? != self.transition_digest
            || signed.body.boundary_position != self.boundary.head().position
            || signed.body.predecessor.record_id != self.boundary.record().parent_record_id
            || signed.body.predecessor.position.checked_add(1)
                != Some(self.boundary.head().position)
            || self
                .boundary
                .record()
                .durable_set_transition_digest
                .as_deref()
                != Some(self.transition_digest.as_str())
        {
            return Err(invalid(
                "healthy marker exact opaque transition and boundary",
            ));
        }
        verify(
            "oasis7.formal.normal_durable_set_transition.writer.v1",
            &signed.body,
            &signed.writer_signature_hex,
            &self.predecessor.definition().writer_public_key,
        )?;
        // All fields come from the already verified token; raw evidence only
        // supplies bytes whose digest and writer authorization matched above.
        let transition = VerifiedNormalDurableSetTransitionV1 {
            body: signed.body.clone(),
            writer_signature_hex: signed.writer_signature_hex.clone(),
            old: self.predecessor.clone(),
            boundary: self.boundary.clone(),
            digest: self.transition_digest.clone(),
        };
        let checked = verify_durable_set_activation(&transition, old_seals, new_installs)?;
        if checked.activation_digest != self.activation_digest
            || checked.successor.context_digest()? != self.successor.context_digest()?
        {
            return Err(invalid("healthy marker exact dual activation permission"));
        }
        Ok(())
    }

    pub fn predecessor(&self) -> &VerifiedAuthorityV1 {
        &self.predecessor
    }
    pub fn activation_digest(&self) -> &str {
        &self.activation_digest
    }

    pub fn successor(&self) -> &VerifiedAuthorityV1 {
        &self.successor
    }
    pub fn boundary(&self) -> &VerifiedFormalCommitV1 {
        &self.boundary
    }
    pub fn transition_digest(&self) -> &str {
        &self.transition_digest
    }
}
pub fn verify_durable_set_activation(
    transition: &VerifiedNormalDurableSetTransitionV1,
    old_seals: [&fencing::EpochSealReceiptV1; 2],
    new_installs: [&DurableSetInstallReceiptV1; 2],
) -> Result<VerifiedDurableSetActivationV1, FormalError> {
    let policy = fencing::TrustedFencingPolicyV1 {
        provider_public_key: transition.old.policy().fencing_provider_public_key.clone(),
        authorized_resource_bindings: transition
            .old
            .policy()
            .authorized_fencing_resource_bindings
            .clone(),
    };
    let expected_head = transition.boundary.head();
    hash_hex(&new_installs[0].body.independent_inventory_digest)?;
    if new_installs[0].body.independent_inventory_digest
        != new_installs[1].body.independent_inventory_digest
    {
        return Err(invalid(
            "durable set installs must retain the same independent H inventory",
        ));
    }

    for (index, role) in [EndpointRole::Primary, EndpointRole::Replica]
        .into_iter()
        .enumerate()
    {
        let old = match role {
            EndpointRole::Primary => &transition.old.definition().primary.instance,
            EndpointRole::Replica => &transition.old.definition().replica.instance,
        };
        let seal = old_seals[index];
        fencing::verify_epoch_seal(seal, &policy).map_err(|e| invalid(e.to_string()))?;
        if &seal.body.instance != old
            || seal.body.recovery_operation != transition.digest
            || seal.body.highest_complete.position != expected_head.position
            || seal.body.highest_complete.decision_hash != expected_head.record_id
            || !seal.body.highest_complete.qualified
            || seal.body.checkpoint_before.head != seal.body.highest_complete
            || seal.body.tail != fencing::SealedTailV1::empty()
            || seal.body.next_epoch_floor != transition.body.successor.epoch
        {
            return Err(invalid("durable set exact old seal/boundary/empty slot"));
        }
        let install = new_installs[index];
        let expected_instance = match role {
            EndpointRole::Primary => &transition.body.successor.primary.instance,
            EndpointRole::Replica => &transition.body.successor.replica.instance,
        };
        let body = &install.body;
        if body.schema_version != 1
            || &body.instance != expected_instance
            || body.transition_digest != transition.digest
            || body.successor_authority_id != transition.body.successor.authority_id()?
            || body.boundary != expected_head
            || body.independent_closure_root != transition.body.independent_closure_root
            || body.checkpoint.head.position != expected_head.position
            || body.checkpoint.head.decision_hash != expected_head.record_id
            || !body.checkpoint.head.qualified
            || body.checkpoint.revision == 0
            || (role == EndpointRole::Primary
                && (body.checkpoint.revision <= seal.provider.statement.checkpoint.revision
                    || install.provider_evidence.statement.resource_binding
                        != seal.provider.statement.resource_binding))
        {
            return Err(invalid(
                "durable set install instance/closure/checkpoint revision",
            ));
        }
        body.checkpoint
            .validate()
            .map_err(|e| invalid(e.to_string()))?;
        fencing::verify_monotonic_checkpoint_evidence(
            &install.provider_evidence,
            &body.instance,
            &body.checkpoint,
            &policy,
        )
        .map_err(|e| invalid(e.to_string()))?;
        verify(
            "oasis7.formal.durable_set_install.endpoint.v1",
            body,
            &install.endpoint_signature_hex,
            &body.instance.endpoint_public_key,
        )?;
        verify(
            "oasis7.formal.durable_set_install.provider.v1",
            body,
            &install.provider_signature_hex,
            &policy.provider_public_key,
        )?;
    }
    let activation_digest = hash(
        "oasis7.formal.durable_set_activation.v1",
        &(
            transition.digest(),
            [&old_seals[0].body, &old_seals[1].body],
            [&new_installs[0].body, &new_installs[1].body],
        ),
    )?;
    let successor = VerifiedAuthorityV1 {
        definition: transition.body.successor.clone(),
        policy: transition.old.policy().clone(),
        activation_ref: transition.old.activation_ref().to_owned(),
        durable_set_activation_digest: Some(activation_digest.clone()),
        initial: None,
        first_normal_position: Some(transition.body.successor.valid_from_position),
        // Exact boundary reused by the existing normal continuity fence; this
        // derives no VerifiedRecovery token or replay authority.
        recovery_cut: Some(transition.boundary.head()),
        recovery_retained_record: None,
        initial_input_digest: None,
        recovery_activation: None,
    };
    Ok(VerifiedDurableSetActivationV1 {
        successor,
        predecessor: transition.old.clone(),
        activation_digest,
        boundary: transition.boundary.clone(),
        transition_digest: transition.digest.clone(),
    })
}
