//! Explicit opt-in typed key possession. Consensus state bytes never change.
use super::*;
use oasis7_proto::distributed_bft::v3::{BftCandidateReadBodyV3, BftCandidateReadProofV3};

#[derive(Clone, Copy, Debug)]
pub struct BftCandidateReadPolicyV3 {
    max_per_session: u32,
    max_per_second: u16,
}
impl BftCandidateReadPolicyV3 {
    pub const STANDARD: Self = Self {
        max_per_session: 1024,
        max_per_second: 32,
    };
    pub fn bounded(max_per_session: u32, max_per_second: u16) -> Result<Self, BftSignerErrorV3> {
        if !(1..=1024).contains(&max_per_session) || !(1..=32).contains(&max_per_second) {
            return Err(invalid("candidate read proof policy exceeds hard bounds"));
        }
        Ok(Self {
            max_per_session,
            max_per_second,
        })
    }
}
impl BftSignerServiceV3 {
    /// Existing open() always disables this additional signing purpose. This
    /// opt-in does not alter binding, namespace, consensus journal or backend.
    pub fn open_with_candidate_read_proofs(
        binding: BftSignerBindingV3,
        provider_key: Box<dyn BftSigningKeyHandleV3>,
        validator_keys: Vec<Box<dyn BftSigningKeyHandleV3>>,
        backend: Box<dyn BftSignerBackendV3>,
        policy: BftCandidateReadPolicyV3,
    ) -> Result<Self, BftSignerErrorV3> {
        let mut service = Self::open(binding, provider_key, validator_keys, backend)?;
        service.candidate_read_policy = Some(policy);
        Ok(service)
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn sign_candidate_read(
        &self,
        runtime: &mut BftSignerServiceRuntimeV3,
        session: &mut BftSignerSessionV3,
        credential: &BftSignerLeaseCredentialV3,
        token_digest: &str,
        snapshot: BftSignerBackendSnapshotV3,
        body: &BftCandidateReadBodyV3,
        request: &SignedBftSignerRequestV3,
        request_digest: &str,
    ) -> Result<ReplyDataV3, BftSignerErrorV3> {
        let policy = self
            .candidate_read_policy
            .ok_or_else(|| invalid("candidate read key proofs are not explicitly enabled"))?;
        body.validate().map_err(invalid)?;
        if body.activation_digest != self.binding.activation_digest
            || body.target_digest != self.binding.handoff().target_digest()
            || body.scope != self.binding.scope
            || body.validator_id != self.binding.local_validator_id
            || !self.binding.provisioned_signing_keys.iter().any(|key| {
                key.validator_id == body.validator_id
                    && key.public_key_hex == body.validator_public_key_hex
            })
        {
            return Err(invalid(
                "candidate read key proof differs from independently provisioned H binding",
            ));
        }
        if session.read_proofs >= policy.max_per_session {
            return Err(invalid(
                "candidate read per-session signature budget exhausted",
            ));
        }
        let now = self.sample_time(runtime, credential, token_digest)?;
        let (start, count) = runtime.read_window.get_or_insert((now, 0));
        if now
            .checked_sub(*start)
            .is_some_and(|elapsed| elapsed >= 1000)
        {
            *start = now;
            *count = 0;
        }
        if *count >= policy.max_per_second {
            return Err(invalid(
                "candidate read protected-clock signature rate exhausted",
            ));
        }
        // Attempts consume this process/session budget even if a key operation
        // fails. Reopen/new lease quotas are not independent antirollback state.
        *count += 1;
        session.read_proofs += 1;
        let key = self
            .validator_keys
            .get(&body.validator_public_key_hex)
            .ok_or_else(|| invalid("candidate read provisioned key handle unavailable"))?;
        let proof = BftCandidateReadProofV3 {
            body: body.clone(),
            signature_hex: hex::encode(
                key.sign_preimage(&body.signing_payload().map_err(invalid)?)?,
            ),
        };
        proof
            .verify_for(body, &body.validator_public_key_hex)
            .map_err(invalid)?;
        // No vote gate is bypassed: this only returns a public key-possession
        // claim. The private service must independently verify actual Noise
        // peers, current active membership, fresh issuer history and read grant.
        let after = runtime
            .backend
            .read_for_lease(credential.generation, token_digest)?;
        self.verify_observed_snapshot(runtime, &after, credential)?;
        if after.state != snapshot.state {
            return Err(invalid("candidate read snapshot changed during key proof"));
        }
        self.state_reply(
            after,
            request,
            request_digest,
            BftSignerReplyKindV3::CandidateReadSigned {
                proof: Box::new(proof),
            },
        )
    }
}
