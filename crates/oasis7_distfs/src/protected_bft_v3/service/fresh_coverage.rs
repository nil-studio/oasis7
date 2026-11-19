//! Remote activation on the existing authenticated signer lease.
use super::*;
use crate::protected_bft_v3::fresh_coverage::check_namespace_budget;
use crate::protected_bft_v3::{
    BftPrivateRetentionActivationV1, BftPrivateRetentionAttestorV1,
    BftPrivateRetentionCreationMarkerV1,
};

/// Checks the independently provisioned original Runtime source and its Fresh
/// creation identity. A serialized marker and a reused binding are insufficient.
pub trait BftFreshSourceVerifierV1: Send + Sync {
    fn verify_original_runtime_source(
        &self,
        binding: &BftSignerBindingV3,
        marker: &BftPrivateRetentionCreationMarkerV1,
    ) -> Result<(), BftSignerErrorV3>;
}

pub struct BftFreshCoverageSupportV1 {
    pub attestor: BftPrivateRetentionAttestorV1,
    pub marker: BftPrivateRetentionCreationMarkerV1,
    pub verifier: Box<dyn BftFreshSourceVerifierV1>,
}

impl BftFreshCoverageSupportV1 {
    pub(super) fn validate_config(
        &self,
        binding: &BftSignerBindingV3,
        provider: &str,
        state: &BftSignerStateV3,
    ) -> Result<(), BftSignerErrorV3> {
        let initial = self.attestor.verify_marker(binding, &self.marker)?;
        let capability = self.attestor.digest()?;
        if self.attestor.provider_public_key != provider
            || state.private_retention.as_ref().map_or_else(
                || *state != initial,
                |head| head.capability_digest != capability,
            )
        {
            return Err(invalid(
                "Fresh private retention is unavailable for this source",
            ));
        }
        Ok(())
    }
}

impl BftSignerServiceV3 {
    pub(super) fn provision_fresh_coverage(
        &self,
        runtime: &mut BftSignerServiceRuntimeV3,
        snapshot: BftSignerBackendSnapshotV3,
        request: &SignedBftSignerRequestV3,
        request_digest: &str,
    ) -> Result<ReplyDataV3, BftSignerErrorV3> {
        let support = self
            .fresh_coverage
            .as_ref()
            .ok_or_else(|| invalid("private retention capability unsupported"))?;
        let BftSignerCommandV3::ProvisionPrivateRetention {
            expected_revision,
            expected_state_digest,
        } = &request.body.command
        else {
            return Err(invalid("private retention activation command kind"));
        };
        let initial = support
            .attestor
            .verify_marker(&self.binding, &support.marker)?;
        if *expected_revision != initial.revision || *expected_state_digest != initial.state_digest
        {
            return Err(invalid(
                "activation must name the exact original Fresh checkpoint",
            ));
        }
        let generation = snapshot.lease_generation;
        let token = &snapshot.lease_token_digest;
        let observed = runtime
            .backend
            .read_fresh_coverage_for_lease(generation, token)?;
        if observed.signer != snapshot || observed.marker != support.marker {
            return Err(invalid(
                "atomic Fresh source read differs from leased original",
            ));
        }
        support
            .verifier
            .verify_original_runtime_source(&self.binding, &observed.marker)?;

        let (activation, next_state) = match &observed.original {
            Some(original) => {
                original.verify(
                    &self.binding,
                    &support.attestor,
                    &support.marker,
                    &snapshot.state,
                    generation,
                )?;
                (original.clone(), snapshot.state.clone())
            }
            None if snapshot.state == initial => BftPrivateRetentionActivationV1::prepare(
                &support.attestor,
                &snapshot,
                observed.reserved_response_bytes,
                self.provider_key.as_ref(),
            )?,
            None => return Err(invalid("original Fresh activation receipt is absent")),
        };
        let mut next = observed.clone();
        next.signer.state = next_state;
        next.original = Some(activation.clone());

        // Charge a widest fixed encoding before disclosure, even on replay.
        // Neither preflight construction nor a backend return is readback.
        let checkpoint = self.make_checkpoint(
            next.signer.state.clone(),
            generation,
            request.body.challenge.clone(),
            request_digest.into(),
        )?;
        let upper = SignedBftSignerReplyV3 {
            body: BftSignerReplyBodyV3 {
                schema_version: 3,
                binding_digest: initial.binding_digest,
                request_digest: request_digest.into(),
                challenge: request.body.challenge.clone(),
                lease_generation: generation,
                lease_token: None,
                state_revision: next.signer.state.revision,
                state_digest: next.signer.state.state_digest.clone(),
                checkpoint: Some(Box::new(checkpoint)),
                result: BftSignerReplyKindV3::PrivateRetention {
                    ledger: Box::new(activation.clone()),
                    latest_seal: None,
                    reserved_response_bytes: u64::MAX,
                },
            },
            provider_signature_hex: "0".repeat(128),
        };
        next.reserved_response_bytes = observed
            .reserved_response_bytes
            .checked_add(encode_frame(&upper)?.len() as u64)
            .ok_or_else(|| invalid("private retention transfer reservation overflow"))?;
        check_namespace_budget(&activation.receipt.body, next.reserved_response_bytes)?;

        let verified = validate_state(
            &self.binding,
            Some(&support.attestor.provider_public_key),
            &next.signer.state,
        )?;
        let committed = runtime
            .backend
            .compare_and_commit_fresh_coverage(&observed, &next)
            .and_then(|()| {
                runtime
                    .backend
                    .read_fresh_coverage_for_lease(generation, token)
            });
        match committed {
            Ok(readback) if readback == next => {}
            _ => {
                runtime.poisoned = true;
                return Err(invalid(
                    "original Fresh activation commit/readback uncertain",
                ));
            }
        }
        runtime.verified = Some(verified);
        self.state_reply(
            next.signer,
            request,
            request_digest,
            BftSignerReplyKindV3::PrivateRetention {
                ledger: Box::new(activation),
                latest_seal: None,
                reserved_response_bytes: next.reserved_response_bytes,
            },
        )
    }
}
