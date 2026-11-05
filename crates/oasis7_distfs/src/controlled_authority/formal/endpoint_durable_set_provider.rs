//! Independently provisioned signer/storage providers use this verifier under
//! their exclusive storage lease before signing an install. The configured
//! directory and instance come from that provider's registry, never the wire.
use super::*;
use crate::controlled_authority::fencing::EndpointInstanceBindingDirectoryExt;
use store::durable_set::copy_authenticated_closure;

pub struct VerifiedDurableSetNativeInstallPlanV1 {
    body: HotBody,
    preimage: Vec<u8>,
    inventory_digest: String,
}
impl VerifiedDurableSetNativeInstallPlanV1 {
    pub fn checkpoint_signing_preimage(&self) -> &[u8] {
        &self.preimage
    }
    pub fn inventory_digest(&self) -> &str {
        &self.inventory_digest
    }
    /// Wraps an independently protected endpoint signature. No key is exported.
    pub fn signed_checkpoint(
        &self,
        signature_hex: &str,
        instance: &fencing::EndpointInstanceBindingV1,
    ) -> Result<Vec<u8>, FormalError> {
        if self.body.instance_digest != instance.digest().map_err(fencing_error)? {
            return Err(invalid("provider checkpoint instance"));
        }
        Store::encode_provider_checkpoint(&self.body, signature_hex, &instance.endpoint_public_key)
    }
    pub fn activation_plan(
        &self,
        activation: &VerifiedDurableSetActivationV1,
    ) -> Result<Self, FormalError> {
        let binding = self
            .body
            .durable_set
            .as_ref()
            .ok_or(FormalError::FencingUnproven)?;
        if binding.transition_digest != activation.transition_digest()
            || self.body.head != activation.boundary().head()
            || self.body.authority_id != activation.successor().authority_id()?
            || binding.activation_digest.is_some()
        {
            return Err(invalid("provider activation exact awaiting plan"));
        }
        let mut body = self.body.clone();
        body.revision = body
            .revision
            .checked_add(1)
            .ok_or_else(|| invalid("provider activation revision overflow"))?;
        body.durable_set
            .as_mut()
            .expect("checked binding")
            .activation_digest = Some(activation.activation_digest().into());
        let preimage = Store::checkpoint_signing_bytes(&body)?;
        Ok(Self {
            body,
            preimage,
            inventory_digest: self.inventory_digest.clone(),
        })
    }
    #[cfg(test)]
    pub(super) fn body(&self) -> &HotBody {
        &self.body
    }
}
/// This performs a complete authenticated readback, including all referenced
/// historical requests and material, not just a caller-supplied root comparison.
/// The operation budget is explicit and checked; there is no lifetime cap.
/// Caller must hold its exclusive registered storage lease (the owner holds
/// the native lock). This function grants no signing key or active authority.
pub fn verify_durable_set_native_install_plan(
    registered_directory: &Path,
    registered_instance: &fencing::EndpointInstanceBindingV1,
    sealed: &VerifiedSealedDurableSetTransitionV1,
    request: &fencing::DurableSetNativeInstallRequestV1,
    operation_byte_budget: u64,
) -> Result<VerifiedDurableSetNativeInstallPlanV1, FormalError> {
    request.validate_bounds().map_err(fencing_error)?;
    let transition = sealed.transition();
    let expected_instance = match registered_instance.endpoint_role {
        EndpointRole::Primary => &transition.body().successor.primary.instance,
        EndpointRole::Replica => &transition.body().successor.replica.instance,
    };
    if registered_instance != expected_instance || request.authorization != *sealed.evidence() {
        return Err(invalid(
            "healthy install registered identity and sealed authorization",
        ));
    }
    registered_instance
        .verify_directory(registered_directory)
        .map_err(fencing_error)?;
    let source_seal = [
        &sealed.evidence().primary_seal,
        &sealed.evidence().replica_seal,
    ]
    .into_iter()
    .find(|seal| {
        Store::verify_public(
            &request.source_checkpoint_bytes,
            &seal.body.instance.endpoint_public_key,
        )
        .is_ok_and(|body| {
            body.seal.as_ref() == Some(&seal.body)
                && Store::binding(&body, &request.source_checkpoint_bytes)
                    == seal.provider.statement.checkpoint
        })
    })
    .ok_or_else(|| invalid("healthy install source exact protected seal checkpoint"))?;
    let source_body = Store::verify_public(
        &request.source_checkpoint_bytes,
        &source_seal.body.instance.endpoint_public_key,
    )?;
    let source = FileFormalArchiveSourceV1::new(registered_directory.to_path_buf())?;
    let inventory = copy_authenticated_closure(
        registered_directory,
        &request.source_checkpoint_bytes,
        &source_seal.body.instance.endpoint_public_key,
        &source,
        transition
            .predecessor_authority()
            .definition()
            .profile_version,
        operation_byte_budget,
        false,
    )?;
    if inventory.inventory_digest != request.local_closure_manifest_digest
        || request.boundary_proof.decision.record != *transition.boundary().record()
        || request.predecessor_proof.decision.record.record_id()?
            != transition.body().predecessor.record_id
        || request.predecessor_proof.decision.record.position
            != transition.body().predecessor.position
    {
        return Err(invalid(
            "healthy install independently readback inventory and exact H lineage",
        ));
    }
    let read_node = |id: &str| -> Result<HistoryNode, FormalError> {
        let bytes = source
            .read_object(id)?
            .ok_or_else(|| invalid("install authenticated history node absent"))?;
        if blake3::hash(&bytes).to_hex().as_str() != id {
            return Err(invalid("install history checksum"));
        }
        serde_cbor::from_slice(&bytes).map_err(|e| invalid(e.to_string()))
    };
    let h_node = read_node(
        source_body
            .head_node
            .as_ref()
            .or(source_body.imported_head_node.as_ref())
            .ok_or_else(|| invalid("install H node missing"))?,
    )?;
    let parent_link = h_node
        .ancestors
        .first()
        .ok_or_else(|| invalid("install H predecessor missing"))?;
    let parent_node = read_node(&parent_link.node)?;
    for (node, expected) in [
        (&h_node, &request.boundary_proof),
        (&parent_node, &request.predecessor_proof),
    ] {
        let bytes = source
            .read_object(&node.entry)?
            .ok_or_else(|| invalid("install H qualified entry absent"))?;
        if blake3::hash(&bytes).to_hex().as_str() != node.entry {
            return Err(invalid("install entry checksum"));
        }
        let entry: FormalRequestStateV1 =
            serde_cbor::from_slice(&bytes).map_err(|e| invalid(e.to_string()))?;
        let FormalRequestStateV1::Qualified { proof, .. } = entry else {
            return Err(invalid("install exact Qualified entry"));
        };
        if &proof != expected {
            return Err(invalid("install exact authenticated public proof"));
        }
    }
    let role = registered_instance.endpoint_role;
    let old_floor = &sealed.evidence().primary_seal.provider.statement.checkpoint;
    let revision = if role == EndpointRole::Primary {
        if request.expected_old_checkpoint.as_ref() != Some(old_floor) {
            return Err(invalid("healthy primary install exact old floor"));
        }
        old_floor
            .revision
            .checked_add(1)
            .ok_or_else(|| invalid("healthy install revision overflow"))?
    } else {
        if request.expected_old_checkpoint.is_some() {
            return Err(invalid("new replica has no old floor"));
        }
        1
    };
    let expected = HotBody {
        version: 1,
        instance_digest: registered_instance.digest().map_err(fencing_error)?,
        authority_id: transition.body().successor.authority_id()?,
        activation_ref: transition.predecessor_authority().activation_ref().into(),
        epoch: transition.body().successor.epoch,
        revision,
        head: transition.boundary().head(),
        head_node: None,
        imported_head_node: source_body.head_node.or(source_body.imported_head_node),
        import_digest: Some(transition.digest().into()),
        base_parent: Some(CheckpointParent {
            record: transition.boundary().record().clone(),
            finalizing_authority_id: transition.boundary().finalizing_authority_id().into(),
        }),
        index: source_body.index,
        pending: None,
        decision_head: None,
        seal: None,
        data_closure: None,
        durable_set: Some(fencing::DurableSetNativeBindingV1 {
            transition_digest: transition.digest().into(),
            independent_inventory_digest: inventory.inventory_digest.clone(),
            activation_digest: None,
        }),
    };
    let preimage = Store::checkpoint_signing_bytes(&expected)?;
    if request.registration.authority_id != expected.authority_id
        || request.registration.activation_ref != expected.activation_ref
        || request.registration.existing_checkpoint.is_some()
        || request.registration.unsigned_checkpoint_preimage != preimage
    {
        return Err(invalid(
            "healthy install exact independently reconstructed native checkpoint",
        ));
    }
    Ok(VerifiedDurableSetNativeInstallPlanV1 {
        body: expected,
        preimage,
        inventory_digest: inventory.inventory_digest,
    })
}
