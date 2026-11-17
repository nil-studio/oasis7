//! Provider-side binding adapters for the portable original source identity.
use super::binding::{BftSignerBindingV3, BftSignerErrorV3, invalid};
pub use oasis7_formal_protocol::bft::{
    BFT_SUCCESSOR_WINDOW_ORIGIN_DOMAIN_V1, BftSuccessorWindowSourceV1,
};

pub fn successor_source_from_binding_v3(
    binding: &BftSignerBindingV3,
) -> Result<BftSuccessorWindowSourceV1, BftSignerErrorV3> {
    binding.validate()?;
    Ok(BftSuccessorWindowSourceV1 {
        scope: binding.scope.clone(),
        binding_digest: binding.digest()?,
        handoff_position: binding.handoff_position,
        handoff_record_id: binding.handoff_record_id.clone(),
        handoff_snapshot_root: binding.handoff().raw_snapshot_root().into(),
        target_digest: binding.handoff().target_digest().into(),
        activation_digest: binding.activation_digest.clone(),
        material_identity: binding.material_identity.clone(),
    })
}

pub fn validate_successor_source_for_binding_v3(
    source: &BftSuccessorWindowSourceV1,
    binding: &BftSignerBindingV3,
) -> Result<(), BftSignerErrorV3> {
    source.validate().map_err(invalid)?;
    if source != &successor_source_from_binding_v3(binding)? {
        return Err(invalid(
            "successor source differs from original qualified H/binding",
        ));
    }
    Ok(())
}
