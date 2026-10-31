//! Exact fee/nonce chronology, separate from any context/action grant.
use super::*;
impl Replay<'_> {
    pub(super) fn apply_purpose(
        &mut self,
        event: &SessionEventKindV2,
        at: UnixMillis,
    ) -> Result<()> {
        match event {
            SessionEventKindV2::PurposeHttpCharged { ingress_attempt_id } => {
                let fee = self
                    .s
                    .purpose_http_admissions
                    .get(ingress_attempt_id)
                    .ok_or_else(invalid)?;
                if fee.charged_at != at || fee.hop != SessionControlHopV1::Host {
                    return Err(invalid());
                }
                self.live(fee.purpose, at)?;
                let amount = match &fee.class {
                    SessionPurposeHttpAdmissionClassV1::GenericOne => 1,
                    SessionPurposeHttpAdmissionClassV1::ToolNonceThree { .. } => {
                        let original = self
                            .s
                            .tool_nonce_ingresses
                            .get(ingress_attempt_id)
                            .ok_or_else(invalid)?;
                        if original.charged_at != at
                            || original.request.purpose != fee.purpose
                            || self.generations.get(&fee.purpose)
                                != Some(&original.credential_generation.0)
                        {
                            return Err(invalid());
                        }
                        3
                    }
                };
                Self::unique(&mut self.purpose_fees, ingress_attempt_id)?;
                self.root_transport
                    .get_mut(&fee.purpose)
                    .ok_or_else(invalid)?
                    .charge(amount)
                    .map_err(|_| invalid())?;
            }
            SessionEventKindV2::ToolMetadataNonceIssued { request_id } => {
                let link = self
                    .s
                    .tool_nonce_links
                    .get(request_id)
                    .ok_or_else(invalid)?;
                let c = self.s.challenges.get(request_id).ok_or_else(invalid)?;
                if c.kind != SessionChallengeKindV1::ToolMetadata
                    || link.created_at != at
                    || !self.purpose_fees.contains(&link.first_ingress)
                    || self
                        .root_transport
                        .get(&c.purpose)
                        .ok_or_else(invalid)?
                        .remaining()
                        .map_err(|_| invalid())?
                        == 0
                {
                    return Err(invalid());
                }
                self.issue_challenge(request_id, at)?;
            }
            // In particular, future execution stage events remain unsupported.
            _ => return Err(invalid()),
        }
        Ok(())
    }
}
