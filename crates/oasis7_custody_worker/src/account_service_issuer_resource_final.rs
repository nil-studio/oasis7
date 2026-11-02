//! Unique Account service identity; only the actual Core paid Resource token.
use super::*;
use oasis7_custody_core::{
    approval::ApprovedWorldV1,
    provisioning::{
        OwnerSetupResourceFinalHttpV2, PaidResourceReadIssuerPinV1, PaidResourceReadProofV1,
        PaidResourceReadStageV1, PreparedOwnerSetupResourceDispatchV2,
    },
};
impl AccountServiceIssuer {
    pub(crate) fn owner_resource_final_pin(
        &self,
        world: &ApprovedWorldV1,
    ) -> Result<PaidResourceReadIssuerPinV1> {
        let pin = self
            .pins
            .iter()
            .find(|p| p.allowed_purpose == AccountMachinePurposeV2::OwnerSetupResourceFinalV2)
            .ok_or_else(denied)?;
        Ok(PaidResourceReadIssuerPinV1 {
            namespace: pin.namespace.clone(),
            service_id: pin.service_id.clone(),
            environment: pin.environment.clone(),
            key_id: pin.key_id.clone(),
            public_key: pin.public_key.clone(),
            issuer_epoch: pin.issuer_epoch,
            issuer_config_generation: pin.config_generation,
            not_before: pin.not_before,
            not_after: pin.not_after,
            native_audience_origin: pin.native_audience_origin.clone(),
            world_audience: world.clone(),
            allowed_stage: PaidResourceReadStageV1::OwnerSetupFinal,
        })
    }
    pub(crate) fn sign_owner_resource_final(
        &self,
        prepared: PreparedOwnerSetupResourceDispatchV2,
        now: u64,
    ) -> Result<OwnerSetupResourceFinalHttpV2> {
        let p = prepared.payload();
        let expected = self.owner_resource_final_pin(&p.world_audience)?;
        if prepared.issuer_pin() != &expected
            || p.issuer_namespace != self.issuer.namespace
            || p.issuer_service_id != self.issuer.service_id
            || p.issuer_environment != self.issuer.environment
            || p.issuer_key_id != self.issuer.key_id
            || p.issuer_epoch != self.issuer.issuer_epoch
            || p.issuer_config_generation != self.issuer.config_generation
            || p.native_audience_origin != self.issuer.native_audience_origin
            || p.stage != PaidResourceReadStageV1::OwnerSetupFinal
        {
            return Err(denied());
        }
        self.validate_lifetime(
            AccountMachinePurposeV2::OwnerSetupResourceFinalV2,
            p.issued_at.0,
            p.expires_at.0,
            now,
        )?;
        let bytes = p.signing_bytes().map_err(|_| denied())?;
        let proof = PaidResourceReadProofV1 {
            payload: p.clone(),
            signature_hex: hex::encode(self.sign_bytes(&bytes)),
        };
        prepared.into_http(proof).map_err(|_| denied())
    }
}
