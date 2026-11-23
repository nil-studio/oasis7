//! Existing Decision material is usable only with its original authenticated
//! receipts. Repair never reconstructs a missing Decision or signs a new one.
use super::*;

impl FileFormalEndpoint {
    pub(super) fn verify_pending_for_recovery_repair(&self) -> Result<(), FormalError> {
        let Some(id) = self.store.body.pending.as_deref() else {
            return Ok(());
        };
        let entry: FormalRequestStateV1 = self.store.get(id)?;
        if let FormalRequestStateV1::Frozen {
            input,
            material,
            receipt,
            certificate,
        } = &entry
        {
            let certificate = certificate
                .as_ref()
                .ok_or_else(|| invalid("repair original Frozen certificate missing"))?;
            let parent = self
                .parent
                .as_ref()
                .ok_or_else(|| invalid("repair original Frozen parent missing"))?;
            verify_formal_reservation(input, &self.authority, parent)?;
            certificate.verify(self.authority.definition(), self.authority.activation_ref())?;
            let original_receipt = match self.role {
                EndpointRole::Primary => &certificate.primary,
                EndpointRole::Replica => &certificate.replica,
            };
            if input != &certificate.reservation
                || receipt
                    .as_ref()
                    .is_some_and(|receipt| receipt != original_receipt)
            {
                return Err(invalid("repair original Frozen receipt changed"));
            }
            return verify_frozen_material(&input.body, material);
        }
        let candidate = entry
            .candidate()
            .ok_or_else(|| invalid("repair original candidate missing"))?;
        if candidate.branch != FormalCompletionBranchV1::Normal {
            return Err(invalid("repair requires the original normal continuation"));
        }
        let (context, _closure) = self.candidate_context(candidate, false)?;
        match &entry {
            FormalRequestStateV1::Prepared { receipt, .. } => {
                if let Some(receipt) = receipt {
                    verify_formal_prepare_receipt(receipt, candidate, &context, self.role)?;
                }
            }
            FormalRequestStateV1::Decided {
                prepare, receipt, ..
            } => {
                verify_formal_prepare_receipt(prepare, candidate, &context, self.role)?;
                if receipt.body != context.receipt_body(self.role)? {
                    return Err(invalid("repair original Decision receipt context"));
                }
                verify(
                    FINAL_RECEIPT_SIGN_DOMAIN_V1,
                    &receipt.body,
                    &receipt.signature_hex,
                    &self.public_key,
                )?;
            }
            FormalRequestStateV1::Ready {
                prepare,
                decision,
                complete,
                ..
            } => {
                verify_formal_prepare_receipt(prepare, candidate, &context, self.role)?;
                if decision.record != candidate.candidate.record
                    || decision.writer_signature_hex != candidate.candidate.writer_signature_hex
                    || decision.frozen != candidate.frozen
                    || decision.branch != candidate.branch
                {
                    return Err(invalid("repair original complete Decision changed"));
                }
                let verified =
                    verify_formal_decision(decision, &self.authority, self.parent.as_ref(), None)?;
                if let Some(complete) = complete {
                    if complete.body != verified.complete_receipt_body(self.role)? {
                        return Err(invalid("repair original ProofComplete receipt context"));
                    }
                    verify(
                        PROOF_COMPLETE_SIGN_DOMAIN_V1,
                        &complete.body,
                        &complete.signature_hex,
                        &self.public_key,
                    )?;
                }
            }
            _ => return Err(invalid("repair pending slot is not an unresolved original")),
        }
        Ok(())
    }
}
