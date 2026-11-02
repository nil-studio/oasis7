//! Only a real paid Core first handle can select this fixed signer domain.
use super::*;
use oasis7_custody_core::{
    provisioning::PreparedGuardedStopSignatureV2,
    term_guarded_signer_v2::SignedTermGuardedStopSignerPermitV2,
};
impl AccountServiceIssuer {
    pub(crate) fn sign_term_guarded_stop(
        &self,
        token: PreparedGuardedStopSignatureV2,
        now: u64,
    ) -> Result<SignedTermGuardedStopSignerPermitV2> {
        let payload = token.signer_permit(self.issuer.clone());
        payload.validate().map_err(|_| denied())?;
        self.validate_lifetime(
            AccountMachinePurposeV2::TermGuardedStopSignerV2,
            payload.issued_at.0,
            payload.expires_at.0,
            now,
        )?;
        let signature_hex =
            hex::encode(self.sign_bytes(&payload.signing_bytes().map_err(|_| denied())?));
        Ok(SignedTermGuardedStopSignerPermitV2 {
            payload,
            signature_hex,
        })
    }
}
