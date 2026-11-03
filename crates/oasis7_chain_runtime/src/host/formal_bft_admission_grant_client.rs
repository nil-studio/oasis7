//! Subject-side live issuance. The public hint selects a view; only the pinned
//! issuer's new-purpose signature and the Node's fresh gate authorize bytes.
use super::*;
use ed25519_dalek::{Signer, SigningKey};
use oasis7_distfs::protected_bft_v3::{
    BftPrivateRetentionAdmissionReadGrantBodyV1 as Body,
    BftPrivateRetentionAdmissionReadGrantV1 as Grant, BftPrivateRetentionAdmissionReadPurposeV1,
};
use oasis7_net::Libp2pNetwork;
use oasis7_node::{
    NodeBftAdmissionInputAuthorizationV1, NodeBftAdmissionInputCurrentViewV1,
    NodeBftAdmissionInputReadCredentialsV1, NodeBftAdmissionInputReadRequestV1,
};
use oasis7_proto::distributed_bft::v3::BftCandidateReadOperationV3;
use serde::{Deserialize, Serialize};
use std::{
    io::Read,
    sync::{Arc, Mutex},
    time::Instant,
};
/// Selector only. The endpoint must match these values against its complete
/// independently witnessed active BftOriginalAdmissionInputV1 issuer target.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AdmissionGrantTargetV1 {
    pub grant_epoch: u64,
    pub not_before_unix_ms: u64,
    pub expires_unix_ms: u64,
}
pub(crate) struct AdmissionGrantClientConfigV1 {
    pub endpoint: String,
    pub issuer_public_key: String,
    pub target: AdmissionGrantTargetV1,
}
pub(crate) struct RemoteAdmissionGrantCredentialsV1 {
    network: Arc<Libp2pNetwork>,
    endpoint: reqwest::Url,
    client: reqwest::blocking::Client,
    issuer: String,
    target: AdmissionGrantTargetV1,
    subject: SigningKey,
    original: Mutex<Option<OriginalGrant>>,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum AdmissionTransportMode {
    V1,
    V2,
}
struct OriginalGrant {
    mode: AdmissionTransportMode,
    request: wire::GrantRequest,
    grant: Option<Grant>,
    deadline: Instant,
}
impl RemoteAdmissionGrantCredentialsV1 {
    pub(crate) fn new(
        config: AdmissionGrantClientConfigV1,
        network: Arc<Libp2pNetwork>,
        subject: SigningKey,
    ) -> Result<Self, String> {
        let endpoint = reqwest::Url::parse(&config.endpoint)
            .map_err(|_| "admission issuer endpoint invalid")?;
        let loopback = endpoint.host_str().is_some_and(|host| {
            host.parse::<std::net::IpAddr>()
                .is_ok_and(|ip| ip.is_loopback())
        });
        if !(endpoint.scheme() == "https" || (endpoint.scheme() == "http" && loopback))
            || endpoint.path() != wire::PATH
            || !endpoint.username().is_empty()
            || endpoint.password().is_some()
            || endpoint.query().is_some()
            || endpoint.fragment().is_some()
        {
            return Err(
                "admission issuer endpoint requires explicit route and HTTPS or loopback HTTP"
                    .into(),
            );
        }
        let issuer: [u8; 32] = hex::decode(&config.issuer_public_key)
            .map_err(|_| "admission issuer key invalid")?
            .try_into()
            .map_err(|_| "admission issuer key length invalid")?;
        ed25519_dalek::VerifyingKey::from_bytes(&issuer)
            .map_err(|_| "admission issuer key rejected")?;
        if hex::encode(issuer) != config.issuer_public_key
            || config.target.grant_epoch == 0
            || config.target.expires_unix_ms <= config.target.not_before_unix_ms
        {
            return Err("admission issuer selector invalid".into());
        }
        let mut subject_secret = subject.to_bytes();
        let subject_identity = oasis7_net::Libp2pNetworkConfig::default()
            .with_ed25519_identity(&mut subject_secret)
            .map_err(|_| "admission subject identity rejected")?
            .keypair
            .ok_or("admission subject identity missing")?
            .public()
            .to_peer_id();
        if subject_identity == network.peer_id()
            || hex::encode(subject.verifying_key().to_bytes()) == config.issuer_public_key
        {
            return Err("admission subject must differ from Noise and issuer identities".into());
        }
        let client = reqwest::blocking::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .build()
            .map_err(|_| "admission issuer client unavailable")?;
        Ok(Self {
            network,
            endpoint,
            client,
            issuer: config.issuer_public_key,
            target: config.target,
            subject,
            original: Mutex::new(None),
        })
    }
    fn prepare(
        &self,
        read: &NodeBftAdmissionInputReadRequestV1,
        deadline: Instant,
    ) -> Result<wire::GrantRequest, String> {
        if read.requester_peer_id != self.network.peer_id().to_string() {
            return Err("admission requester Noise identity differs".into());
        }
        let view = NodeBftAdmissionInputCurrentViewV1::read_from(
            &self.network,
            &read.serving_peer_id,
            &read.nonce,
            deadline,
        )?;
        if view.binding_digest != read.reference.source.binding_digest {
            return Err("admission hint differs from original source binding".into());
        }
        let body = Body {
            schema_version: 1,
            purpose: BftPrivateRetentionAdmissionReadPurposeV1::OriginalAdmissionInput,
            scope: view.scope,
            reference: read.reference.clone(),
            subject_public_key: hex::encode(self.subject.verifying_key().to_bytes()),
            requester_peer_id: read.requester_peer_id.clone(),
            serving_peer_id: read.serving_peer_id.clone(),
            schedule_digest: view.schedule_digest,
            view_generation: view.generation,
            request_nonce: read.nonce.clone(),
            grant_epoch: self.target.grant_epoch,
            not_before_unix_ms: self.target.not_before_unix_ms,
            expires_unix_ms: self.target.expires_unix_ms,
        };
        wire::GrantRequest::sign(body, &self.subject)
    }
    fn prepare_v2(
        &self,
        read: &oasis7_node::NodeBftAdmissionInputBatchReadRequestV2,
        deadline: Instant,
    ) -> Result<wire::GrantRequest, String> {
        if read.requester_peer_id != self.network.peer_id().to_string() {
            return Err("admission requester Noise identity differs".into());
        }
        let view = NodeBftAdmissionInputCurrentViewV1::read_from_v2(
            &self.network,
            &read.serving_peer_id,
            &read.nonce,
            deadline,
        )?;
        if view.binding_digest != read.reference.source.binding_digest {
            return Err("admission hint differs from original source binding".into());
        }
        let body = Body {
            schema_version: 1,
            purpose: BftPrivateRetentionAdmissionReadPurposeV1::OriginalAdmissionInput,
            scope: view.scope,
            reference: read.reference.clone(),
            subject_public_key: hex::encode(self.subject.verifying_key().to_bytes()),
            requester_peer_id: read.requester_peer_id.clone(),
            serving_peer_id: read.serving_peer_id.clone(),
            schedule_digest: view.schedule_digest,
            view_generation: view.generation,
            request_nonce: read.nonce.clone(),
            grant_epoch: self.target.grant_epoch,
            not_before_unix_ms: self.target.not_before_unix_ms,
            expires_unix_ms: self.target.expires_unix_ms,
        };
        wire::GrantRequest::sign(body, &self.subject)
    }
    fn issue(&self, request: &wire::GrantRequest, deadline: Instant) -> Result<Grant, String> {
        let bytes = serde_json::to_vec(request)
            .map_err(|_| "admission issuance request encoding failed")?;
        if bytes.len() > wire::MAX_FRAME {
            return Err("admission issuance request capacity".into());
        }
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .filter(|d| !d.is_zero())
            .ok_or("admission original issuance deadline exhausted")?;
        let response = self
            .client
            .post(self.endpoint.clone())
            .timeout(remaining)
            .header("Content-Type", "application/json")
            .body(bytes)
            .send()
            .map_err(|_| "admission issuance transport unavailable")?;
        if !response.status().is_success()
            || response
                .content_length()
                .is_some_and(|n| n > wire::MAX_FRAME as u64)
        {
            return Err("admission issuance response rejected".into());
        }
        let mut bytes = Vec::new();
        response
            .take(wire::MAX_FRAME as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| "admission issuance reply unavailable")?;
        if bytes.len() > wire::MAX_FRAME || Instant::now() >= deadline {
            return Err("admission original issuance reply bound".into());
        }
        let grant: Grant = crate::formal_live_service::parse_strict(&bytes)?;
        grant
            .verify_signature(&self.issuer)
            .map_err(|_| "admission issuance signature rejected")?;
        if grant.body != request.body {
            return Err("admission issuance differs from exact original request".into());
        }
        Ok(grant)
    }
}
impl NodeBftAdmissionInputReadCredentialsV1 for RemoteAdmissionGrantCredentialsV1 {
    fn authorize(
        &self,
        read: &NodeBftAdmissionInputReadRequestV1,
        deadline: Instant,
    ) -> Result<NodeBftAdmissionInputAuthorizationV1, String> {
        if Instant::now() >= deadline {
            return Err("admission original issuance deadline exhausted".into());
        }
        read.reference
            .validate()
            .map_err(|_| "admission read reference invalid")?;
        let mut original = self
            .original
            .lock()
            .map_err(|_| "admission original grant poisoned")?;
        // An unknown HTTP result retains its exact original request. Another
        // nonce cannot evict it and make a later retry resample the live hint.
        if original
            .as_ref()
            .is_some_and(|old| old.request.body.request_nonce != read.nonce && old.grant.is_none())
        {
            return Err("admission pending original issuance must be resolved first".into());
        }
        if original
            .as_ref()
            .is_none_or(|old| old.request.body.request_nonce != read.nonce)
        {
            if !matches!(read.operation, BftCandidateReadOperationV3::Manifest) {
                return Err("admission Package requires the original Manifest grant".into());
            }
            let request = self.prepare(read, deadline)?;
            // Remember the exact signed request before HTTP. Lost replies may
            // only retry this body; never resample a hint under the same nonce.
            *original = Some(OriginalGrant {
                mode: AdmissionTransportMode::V1,
                request,
                grant: None,
                deadline,
            });
        }
        let original = original
            .as_mut()
            .ok_or("admission original grant missing")?;
        let body = &original.request.body;
        if original.mode != AdmissionTransportMode::V1
            || body.reference != read.reference
            || body.requester_peer_id != read.requester_peer_id
            || body.serving_peer_id != read.serving_peer_id
        {
            return Err("admission original nonce target changed".into());
        }
        let deadline = deadline.min(original.deadline);
        if Instant::now() >= deadline {
            return Err("admission original issuance deadline exhausted".into());
        }
        let grant = match &original.grant {
            Some(grant) => grant.clone(),
            None => {
                let grant = self.issue(&original.request, deadline)?;
                original.grant = Some(grant.clone());
                grant
            }
        };
        if Instant::now() >= deadline {
            return Err("admission original issuance deadline exhausted".into());
        }
        let signature_hex = hex::encode(self.subject.sign(&read.signing_bytes(&grant)?).to_bytes());
        Ok(NodeBftAdmissionInputAuthorizationV1 {
            subject_public_key: grant.body.subject_public_key.clone(),
            grant,
            signature_hex,
        })
    }
}

impl oasis7_node::NodeBftAdmissionInputBatchCredentialsV2 for RemoteAdmissionGrantCredentialsV1 {
    fn authorize(
        &self,
        read: &oasis7_node::NodeBftAdmissionInputBatchReadRequestV2,
        deadline: Instant,
    ) -> Result<oasis7_node::NodeBftAdmissionInputBatchAuthorizationV2, String> {
        if Instant::now() >= deadline {
            return Err("admission original issuance deadline exhausted".into());
        }
        read.reference
            .validate()
            .map_err(|_| "admission read reference invalid")?;
        let mut original = self
            .original
            .lock()
            .map_err(|_| "admission original grant poisoned")?;
        // An unknown HTTP result retains its exact original request. Another
        // nonce cannot evict it and make a later retry resample the live hint.
        if original
            .as_ref()
            .is_some_and(|old| old.request.body.request_nonce != read.nonce && old.grant.is_none())
        {
            return Err("admission pending original issuance must be resolved first".into());
        }
        if original
            .as_ref()
            .is_none_or(|old| old.request.body.request_nonce != read.nonce)
        {
            if !matches!(
                read.operation,
                oasis7_node::NodeBftAdmissionInputBatchOperationV2::Manifest
            ) {
                return Err("admission Package requires the original Manifest grant".into());
            }
            let request = self.prepare_v2(read, deadline)?;
            // Remember the exact signed request before HTTP. Lost replies may
            // only retry this body; never resample a hint under the same nonce.
            *original = Some(OriginalGrant {
                mode: AdmissionTransportMode::V2,
                request,
                grant: None,
                deadline,
            });
        }
        let original = original
            .as_mut()
            .ok_or("admission original grant missing")?;
        let body = &original.request.body;
        if original.mode != AdmissionTransportMode::V2
            || body.reference != read.reference
            || body.requester_peer_id != read.requester_peer_id
            || body.serving_peer_id != read.serving_peer_id
        {
            return Err("admission original nonce target changed".into());
        }
        let deadline = deadline.min(original.deadline);
        if Instant::now() >= deadline {
            return Err("admission original issuance deadline exhausted".into());
        }
        let grant = match &original.grant {
            Some(grant) => grant.clone(),
            None => {
                let grant = self.issue(&original.request, deadline)?;
                original.grant = Some(grant.clone());
                grant
            }
        };
        if Instant::now() >= deadline {
            return Err("admission original issuance deadline exhausted".into());
        }
        let signature_hex = hex::encode(self.subject.sign(&read.signing_bytes(&grant)?).to_bytes());
        Ok(oasis7_node::NodeBftAdmissionInputBatchAuthorizationV2 {
            subject_public_key: grant.body.subject_public_key.clone(),
            grant,
            signature_hex,
        })
    }
}
