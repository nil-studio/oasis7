//! Operator-pinned ordinary controlled-v2 read dependencies. This constructs a
//! transport, not trusted evidence: every remote response still crosses the
//! existing V3 signature, history, protected-time and per-chunk lease gate.
use super::super::formal_live_service::full_sync_read_gate::{
    FullSyncV3ReadGateConfig, remote_evidence::RemoteFullSyncReadEvidenceSourceV3,
    validate_full_sync_read_mode_gate_pair,
};
use ed25519_dalek::SigningKey;
use oasis7_distfs::controlled_authority::formal::{
    InitialActivationTrustV1,
    full_sync_issuer::{
        FullSyncIssuerAnchorV3, SignedFullSyncIssuerPolicyV2, SignedFullSyncIssuerPolicyV3,
        VerifiedFullSyncIssuerPolicyV2, VerifiedFullSyncIssuerPolicyV3,
    },
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FormalLiveReadAuthorizationV3 {
    pub evidence_endpoint: String,
    pub signed_v2_policy: SignedFullSyncIssuerPolicyV2,
    pub signed_v3_policy: SignedFullSyncIssuerPolicyV3,
    pub minimum_anchor: FullSyncIssuerAnchorV3,
}

impl FormalLiveReadAuthorizationV3 {
    /// Bind to the already loaded trust and actual Node identity, never a
    /// second key path or an endpoint-provided replacement policy. Construction
    /// performs no RPC and does not assert that the deployment is qualified.
    pub(crate) fn build(
        &self,
        trust: &InitialActivationTrustV1,
        provider: &SigningKey,
    ) -> Result<FullSyncV3ReadGateConfig, String> {
        let v2 = VerifiedFullSyncIssuerPolicyV2::verify(&self.signed_v2_policy, trust)
            .map_err(|error| format!("verify configured full-sync V2 policy: {error}"))?;
        let v3 = VerifiedFullSyncIssuerPolicyV3::verify(&self.signed_v3_policy, trust, &v2)
            .map_err(|error| format!("verify configured full-sync V3 policy: {error}"))?;
        let source = RemoteFullSyncReadEvidenceSourceV3::new(&self.evidence_endpoint)?
            .with_provider_lease_requests(v2, v3, provider.clone())?;
        Ok(FullSyncV3ReadGateConfig {
            signed_v2_policy: self.signed_v2_policy.clone(),
            signed_v3_policy: self.signed_v3_policy.clone(),
            minimum_anchor: self.minimum_anchor.clone(),
            source: Arc::new(source),
        })
    }
}

impl super::ValidatedFormalLiveConfigV1 {
    /// Assemble optional dependencies only after enforcing the configured mode.
    /// The runtime still validates the pair before creating its listener.
    pub(crate) fn build_full_sync_read_gate(
        &self,
        provider: &SigningKey,
    ) -> Result<Option<FullSyncV3ReadGateConfig>, String> {
        let configured = self.config.full_sync_read_authorization_v3.as_ref();
        validate_full_sync_read_mode_gate_pair(
            self.config.full_sync_provider_read_authorization,
            configured.is_some(),
        )?;
        configured
            .map(|config| config.build(&self.trust.initial_trust, provider))
            .transpose()
    }
}
