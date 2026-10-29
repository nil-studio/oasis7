//! Exact provider-signed lease acquisition, separate from management authority.
//! The independently durable issuer resolves a repeated signed request as its
//! original operation; an uncertain transport result never creates a new one.
use super::*;
use oasis7_distfs::controlled_authority::formal::full_sync_issuer::{
    FullSyncProviderLeaseRequestBodyV3, SignedFullSyncProviderLeaseRequestV3,
};
use std::collections::BTreeMap;

pub(super) struct ProviderLeaseClientV3 {
    v2: VerifiedFullSyncIssuerPolicyV2,
    v3: VerifiedFullSyncIssuerPolicyV3,
    provider: ed25519_dalek::SigningKey,
    pending: Mutex<BTreeMap<String, SignedFullSyncProviderLeaseRequestV3>>,
}
impl ProviderLeaseClientV3 {
    pub fn new(
        v2: VerifiedFullSyncIssuerPolicyV2,
        v3: VerifiedFullSyncIssuerPolicyV3,
        provider: ed25519_dalek::SigningKey,
    ) -> Result<Self, String> {
        let public = hex::encode(provider.verifying_key().to_bytes());
        if !v3.body().provider_public_keys.contains(&public) {
            return Err("lease requester key is not allowed by the pinned V3 policy".into());
        }
        Ok(Self {
            v2,
            v3,
            provider,
            pending: Mutex::new(BTreeMap::new()),
        })
    }
    pub fn fetch(
        &self,
        source: &RemoteFullSyncReadEvidenceSourceV3,
        request: &FullSyncIssuerReadEvidenceRequestV3,
    ) -> FormalReadResult<FullSyncIssuerReadEvidenceV3> {
        if request.provider_public_key != hex::encode(self.provider.verifying_key().to_bytes()) {
            return Err(FormalReadError::Integrity(
                "lease request provider differs from local signer".into(),
            ));
        }
        let key = hex::encode(
            blake3::hash(
                &serde_cbor::to_vec(&(
                    &request.provider_incarnation,
                    &request.target,
                    request.target_generation,
                ))
                .map_err(|_| FormalReadError::Internal("encode lease operation identity".into()))?,
            )
            .as_bytes(),
        );
        // One source serializes acquisition. This mutex does not span private
        // material reads or Node/reducer operations, and every RPC uses the
        // caller's original authorization deadline.
        let mut pending = self
            .pending
            .lock()
            .map_err(|_| FormalReadError::Internal("lease request state poisoned".into()))?;
        let evidence = source.fetch_raw(request, pending.get(&key).cloned())?;
        let state = self.verify_evidence(request, &evidence)?;
        if let Some(original) = pending.get(&key) {
            let committed = state
                .leases()
                .values()
                .any(|pair| &pair.request == original);
            let old_head = &original.body.expected_head;
            let exact_prefix = evidence
                .history
                .v3_entries
                .iter()
                .any(|entry| &entry.record.next_anchor == old_head);
            // A complete signed prefix proves either the exact committed
            // original, or that its CAS head has definitively advanced without
            // that request. Only then may a fresh operation be prepared.
            if committed || (exact_prefix && state.anchor().revision > old_head.revision) {
                pending.remove(&key);
            }
        }
        let current = state
            .target_state(&request.target)
            .ok_or_else(|| FormalReadError::GrantRejected("lease target is absent".into()))?;
        if !current.active || current.generation != request.target_generation {
            return Err(FormalReadError::GrantRejected(
                "lease target is inactive or generation changed".into(),
            ));
        }
        let latest = evidence.observation.body.protected_time.latest_unix_ms;
        let has_lease = state.leases().values().any(|pair| {
            let b = &pair.lease.body;
            b.provider_public_key == request.provider_public_key
                && b.provider_incarnation == request.provider_incarnation
                && b.target == request.target
                && b.target_generation == request.target_generation
                && b.not_before_unix_ms <= evidence.observation.body.protected_time.earliest_unix_ms
                && b.expires_unix_ms > latest
        });
        if has_lease {
            pending.remove(&key);
            return Ok(evidence);
        }
        // A retained request has already been submitted above. Do not refresh
        // its expected head, IDs or signature when the result remains unknown.
        if pending.contains_key(&key) {
            return Ok(evidence);
        }
        if pending.len() >= 1024 {
            return Err(FormalReadError::HistoryUnavailable(
                "lease request pending capacity exhausted".into(),
            ));
        }
        let random = || {
            fresh_nonce()
                .map(hex::encode)
                .map_err(FormalReadError::Internal)
        };
        let signed = SignedFullSyncProviderLeaseRequestV3::sign(
            FullSyncProviderLeaseRequestBodyV3 {
                schema_version: 3,
                world: self.v3.body().world.clone(),
                authority_epoch: self.v3.body().authority_epoch,
                scope_digest: self.v3.scope_digest().into(),
                policy_digest: self.v3.policy_digest().into(),
                signer_generation: self.v3.body().signer_generation,
                provider_public_key: request.provider_public_key.clone(),
                provider_incarnation: request.provider_incarnation.clone(),
                lease_id: random()?,
                request_id: random()?,
                request_nonce: random()?,
                expected_head: state.anchor().clone(),
                target: request.target.clone(),
                target_generation: request.target_generation,
            },
            &self.provider,
        )
        .map_err(|_| {
            FormalReadError::Integrity("provider lease request signing rejected".into())
        })?;
        signed.verify(&self.v3).map_err(|_| {
            FormalReadError::Integrity("provider lease request does not match policy".into())
        })?;
        pending.insert(key, signed.clone());
        // Only a real committed lease in the returned complete signed history
        // can authorize a read. An HTTP acknowledgement or this signature alone
        // never becomes a lease capability.
        source.fetch_raw(request, Some(signed))
    }
    fn verify_evidence(
        &self,
        request: &FullSyncIssuerReadEvidenceRequestV3,
        evidence: &FullSyncIssuerReadEvidenceV3,
    ) -> FormalReadResult<VerifiedFullSyncIssuerStateV3> {
        evidence.query.verify(&self.v3).map_err(|_| {
            FormalReadError::Integrity("lease bootstrap query signature rejected".into())
        })?;
        if evidence.query.body.observation_nonce != hex::encode(request.observation_nonce)
            || evidence.query.body.minimum_anchor != request.minimum_anchor
            || evidence.observation.body.query != evidence.query
            || Instant::now() >= request.deadline
        {
            return Err(FormalReadError::Integrity(
                "lease bootstrap is not the original fresh query".into(),
            ));
        }
        evidence.observation.verify(&self.v3).map_err(|_| {
            FormalReadError::Integrity("lease bootstrap witness signature rejected".into())
        })?;
        if serde_cbor::to_vec(&evidence.history)
            .map_err(|_| {
                FormalReadError::Integrity("lease bootstrap history encoding rejected".into())
            })?
            .len()
            > MAX_HISTORY_BYTES
        {
            return Err(FormalReadError::Integrity(
                "lease bootstrap history exceeds canonical budget".into(),
            ));
        }
        VerifiedFullSyncIssuerStateV3::from_witness_observation(
            self.v2.clone(),
            self.v3.clone(),
            evidence.history.clone(),
            &evidence.observation,
        )
        .map_err(|_| FormalReadError::Integrity("lease bootstrap complete history rejected".into()))
    }
}
