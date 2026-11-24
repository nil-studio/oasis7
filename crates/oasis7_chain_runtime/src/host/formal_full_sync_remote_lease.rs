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
    pending: Mutex<BTreeMap<String, PendingLeaseRequestV3>>,
}

struct PendingLeaseRequestV3 {
    signed: SignedFullSyncProviderLeaseRequestV3,
    // Keep the verified prefix even if the next transport submission fails.
    last_verified_head: FullSyncIssuerAnchorV3,
    // Any failed submission requires a read-only reconciliation before retry.
    submission_verified: bool,
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
        // A previously verified successful submission may replay its exact
        // original receipt. A failed submission cannot loop forever on a stale
        // CAS: first obtain a read-only, independently verified complete prefix.
        if pending
            .get(&key)
            .is_some_and(|entry| entry.submission_verified)
        {
            return self.submit_pending(source, request, &mut pending, &key, true);
        }
        // Reserve the caller's nonce for the final authorization response.
        // The preliminary observation must not reuse it across a lease CAS.
        let mut probe = request.clone();
        probe.observation_nonce = fresh_nonce().map_err(FormalReadError::Internal)?;
        if probe.observation_nonce == request.observation_nonce {
            return Err(FormalReadError::Internal(
                "lease bootstrap nonce collides with authorization nonce".into(),
            ));
        }
        let evidence = source.fetch_raw(&probe, None)?;
        let state = self.verify_response_at_least(
            &probe,
            &evidence,
            pending.get(&key).map(|entry| &entry.last_verified_head),
        )?;
        if let Some(original) = pending.get(&key) {
            let committed = state
                .leases()
                .values()
                .any(|pair| pair.request == original.signed);
            let old_head = &original.signed.body.expected_head;
            let exact_prefix = evidence
                .history
                .v3_entries
                .iter()
                .any(|entry| &entry.record.next_anchor == old_head);
            if committed || (exact_prefix && state.anchor().revision > old_head.revision) {
                // Only a signed complete history can resolve an unknown
                // outcome or prove that the original CAS can no longer win.
                pending.remove(&key);
            } else if state.anchor() != old_head {
                return Err(FormalReadError::Integrity(
                    "lease recovery history does not prove the original CAS prefix".into(),
                ));
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
        if pending.contains_key(&key) {
            // Its exact head is still current. Preserve all signed bytes,
            // including IDs and nonce, even when the previous reply was lost.
            return self.submit_pending(source, request, &mut pending, &key, true);
        }
        if has_lease {
            let final_evidence = source.fetch_raw(request, None)?;
            self.verify_response_at_least(request, &final_evidence, Some(state.anchor()))?;
            return Ok(final_evidence);
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
        pending.insert(
            key.clone(),
            PendingLeaseRequestV3 {
                signed,
                last_verified_head: state.anchor().clone(),
                submission_verified: false,
            },
        );
        self.submit_pending(source, request, &mut pending, &key, false)
    }

    fn submit_pending(
        &self,
        source: &RemoteFullSyncReadEvidenceSourceV3,
        request: &FullSyncIssuerReadEvidenceRequestV3,
        pending: &mut BTreeMap<String, PendingLeaseRequestV3>,
        key: &str,
        retire_on_success: bool,
    ) -> FormalReadResult<FullSyncIssuerReadEvidenceV3> {
        let entry = pending
            .get_mut(key)
            .ok_or_else(|| FormalReadError::Internal("missing pending lease request".into()))?;
        // Set uncertainty before sending; neither HTTP success nor a signature
        // alone is proof that the issuer committed this exact operation.
        entry.submission_verified = false;
        let evidence = source.fetch_raw(request, Some(entry.signed.clone()))?;
        let state = self.verify_response_at_least(
            request,
            &evidence,
            Some(&entry.last_verified_head),
        )?;
        entry.last_verified_head = state.anchor().clone();
        if !state
            .leases()
            .values()
            .any(|pair| pair.request == entry.signed)
        {
            return Err(FormalReadError::HistoryUnavailable(
                "issuer history does not contain the exact submitted lease".into(),
            ));
        }
        entry.submission_verified = true;
        if retire_on_success {
            pending.remove(key);
        }
        Ok(evidence)
    }

    fn verify_response_at_least(
        &self,
        request: &FullSyncIssuerReadEvidenceRequestV3,
        evidence: &FullSyncIssuerReadEvidenceV3,
        previous: Option<&FullSyncIssuerAnchorV3>,
    ) -> FormalReadResult<VerifiedFullSyncIssuerStateV3> {
        let state = self.verify_evidence(request, evidence)?;
        if let Some(anchor) = previous
            && !evidence
                .history
                .v3_entries
                .iter()
                .any(|entry| &entry.record.next_anchor == anchor)
        {
            return Err(FormalReadError::Integrity(
                "lease response does not extend the verified bootstrap prefix".into(),
            ));
        }
        Ok(state)
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
