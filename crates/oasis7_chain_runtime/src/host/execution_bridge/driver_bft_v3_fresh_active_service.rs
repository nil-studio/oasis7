//! Independent first Fresh Reserve/Ready consumer. Source guards stay unchanged.
use super::*;
use oasis7_distfs::protected_bft_v3::{
    BftPrivateRetentionAdmissionInputV1 as Input,
    BftPrivateRetentionVerificationContextV1 as Context,
};
impl ActualBftPrivateSourceCreationVerifierV1 {
    pub(super) fn verify_fresh_active_input(
        &self,
        binding: &BftSignerBindingV3,
        input: &Input,
        context: &Context<'_>,
        original: bool,
        deadline: std::time::Instant,
    ) -> Result<
        (
            super::super::successor_verifier::VerifiedActiveAdmissionOwnerV1,
            node_execution::VerifiedActiveAdmissionInputV1,
        ),
        String,
    > {
        successor_archive::live(deadline)?;
        if !context.attestor().allow_authenticated_successor_origins {
            return Err("fixed original role denies fresh Active admission".into());
        }
        let verifier = self
            .active_admission_verifier
            .as_ref()
            .ok_or("Fresh Active independent verifier unavailable")?;
        input.validate().map_err(|e| e.to_string())?;
        if input.source != context.attestor().source {
            return Err("Fresh fixed source differs".into());
        }
        let existing = context
            .original_entry(&input.key)
            .map_err(|e| e.to_string())?;
        let parent = if original || existing.is_some() {
            if let Some(entry) = existing {
                if entry.fact != input.fact().map_err(|e| e.to_string())? {
                    return Err("Fresh retry changed original fact".into());
                }
            }
            context.authenticated_original_admission_parent(binding, &input.key)
        } else {
            context.authenticated_current_admission_parent(binding)
        }
        .map_err(|e| e.to_string())?;
        let owner =
            verifier.recover_active_admission_parent(binding, context, &parent, deadline)?;
        let grant = node_execution::verify_active_admission_original(input, &owner, &parent)?;
        successor_archive::live(deadline)?;
        Ok((owner, grant))
    }
    pub(super) fn verify_fresh_active_ready(
        &self,
        binding: &BftSignerBindingV3,
        entry: &BftPrivateRetentionEntryV1,
        input: &Input,
        context: &Context<'_>,
        local: bool,
        deadline: std::time::Instant,
    ) -> Result<node_execution::VerifiedActiveAdmissionInputV1, String> {
        if entry.key != input.key || entry.fact != input.fact().map_err(|e| e.to_string())? {
            return Err("Fresh Ready exact key/fact differs".into());
        }
        let (owner, grant) =
            self.verify_fresh_active_input(binding, input, context, true, deadline)?;
        node_execution::verify_active_admission_ready(
            owner.private_directory(),
            &self.store,
            input,
            &grant,
        )?;
        if local {
            let ledger = context.ledger().ok_or("Fresh Local ledger missing")?;
            let index = ledger
                .selection_index
                .as_ref()
                .ok_or("Fresh Local protected choice missing")?;
            let identity = grant.selection_identity();
            if !index.selections.iter().any(|selection| {
                selection.key == input.key
                    && &selection.reference == grant.reference()
                    && selection.kind == identity.kind()
                    && selection.local_identity_digest == identity.digest()
                    && selection.local_identity_bytes == identity.bytes()
            }) {
                return Err("Fresh Local differs from exact protected typed choice".into());
            }
            node_execution::verify_active_selection_current(
                owner.private_directory(),
                &self.store,
                input,
                &grant,
            )?;
        }
        successor_archive::live(deadline)?;
        Ok(grant)
    }
}
