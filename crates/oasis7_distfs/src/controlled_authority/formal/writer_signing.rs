//! Closed writer signing capabilities. The remote capability never exposes the
//! writer key and preserves the existing Frozen, candidate, and endpoint domains.
use super::{crypto::*, *};
use crate::controlled_authority::replicated_protocol::ClosedRecord;
use ed25519_dalek::{Signer, SigningKey};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
#[path = "writer_signing_remote.rs"]
mod remote;
pub use remote::{RemoteWriterCustodyClientV1, WriterCustodyServerV1, WriterCustodyServiceV1};

const REQUEST_DOMAIN: &str = "oasis7.formal.writer_custody.request.v1";
const RESPONSE_DOMAIN: &str = "oasis7.formal.writer_custody.response.v1";

/// Only the endpoint client builds this value. The service receives and checks
/// the complete typed request; this is not an arbitrary-preimage signing API.
#[derive(Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct WriterEndpointRequestV1 {
    pub(crate) body: transport::RequestBody,
}
impl WriterEndpointRequestV1 {
    pub(crate) fn new(body: transport::RequestBody) -> Self {
        Self { body }
    }
    pub(crate) fn signing_bytes(&self) -> Result<Vec<u8>, FormalError> {
        bytes(transport::REQUEST_DOMAIN, &self.body)
    }
}

/// A capability may sign only the original, versioned writer operations. A
/// caller cannot turn a diagnostic or an arbitrary byte string into authority.
pub trait FormalWriterSignerV1: Send + Sync {
    fn public_key_hex(&self) -> String;
    fn sign_frozen(&self, body: &FormalFrozenBodyV1) -> Result<String, FormalError>;
    fn sign_candidate(
        &self,
        record: &CanonicalRecordV1,
        frozen: Option<&FormalFrozenCertificateV1>,
    ) -> Result<String, FormalError>;
    fn sign_endpoint_request(
        &self,
        request: &WriterEndpointRequestV1,
    ) -> Result<String, FormalError>;
    fn sign_endpoint_request_before(
        &self,
        request: &WriterEndpointRequestV1,
        deadline: std::time::Instant,
    ) -> Result<String, FormalError> {
        remote::check_deadline(deadline)?;
        let signature = self.sign_endpoint_request(request)?;
        remote::check_deadline(deadline)?;
        Ok(signature)
    }
    fn supports_healthy_durable_set(&self) -> bool {
        false
    }
    fn sign_durable_set_transition(
        &self,
        _body: &NormalDurableSetTransitionBodyV1,
    ) -> Result<String, FormalError> {
        Err(invalid(
            "writer custody does not support healthy durable-set signing",
        ))
    }
    fn sign_durable_set_candidate(
        &self,
        _request: &transport::WriterDurableSetRequestV1,
    ) -> Result<String, FormalError> {
        Err(invalid(
            "writer custody does not support healthy candidate authentication",
        ))
    }
}

/// Existing local-key configurations remain explicit legacy capabilities. New
/// custody configurations construct a remote capability instead of loading the
/// writer key into the Runtime or its endpoint clients.
impl FormalWriterSignerV1 for SigningKey {
    fn public_key_hex(&self) -> String {
        hex::encode(self.verifying_key().to_bytes())
    }
    fn sign_frozen(&self, body: &FormalFrozenBodyV1) -> Result<String, FormalError> {
        Ok(hex::encode(
            self.sign(&frozen_signing_bytes(body)?).to_bytes(),
        ))
    }
    fn sign_candidate(
        &self,
        record: &CanonicalRecordV1,
        _frozen: Option<&FormalFrozenCertificateV1>,
    ) -> Result<String, FormalError> {
        Ok(hex::encode(
            self.sign(&canonical_candidate_signing_bytes(record)?)
                .to_bytes(),
        ))
    }
    fn sign_endpoint_request(
        &self,
        request: &WriterEndpointRequestV1,
    ) -> Result<String, FormalError> {
        Ok(hex::encode(self.sign(&request.signing_bytes()?).to_bytes()))
    }
    fn supports_healthy_durable_set(&self) -> bool {
        true
    }
    fn sign_durable_set_transition(
        &self,
        body: &NormalDurableSetTransitionBodyV1,
    ) -> Result<String, FormalError> {
        Ok(hex::encode(
            self.sign(&normal_durable_set_transition_signing_bytes(body)?)
                .to_bytes(),
        ))
    }
    fn sign_durable_set_candidate(
        &self,
        request: &transport::WriterDurableSetRequestV1,
    ) -> Result<String, FormalError> {
        Ok(hex::encode(self.sign(&request.signing_bytes()?).to_bytes()))
    }
}

#[derive(Clone)]
pub struct WriterSigningCapabilityV1(Arc<dyn FormalWriterSignerV1>);
impl WriterSigningCapabilityV1 {
    pub fn new(signer: impl FormalWriterSignerV1 + 'static) -> Self {
        Self(Arc::new(signer))
    }
}
impl From<SigningKey> for WriterSigningCapabilityV1 {
    fn from(key: SigningKey) -> Self {
        Self::new(key)
    }
}
impl FormalWriterSignerV1 for WriterSigningCapabilityV1 {
    fn public_key_hex(&self) -> String {
        self.0.public_key_hex()
    }
    fn sign_frozen(&self, body: &FormalFrozenBodyV1) -> Result<String, FormalError> {
        self.0.sign_frozen(body)
    }
    fn sign_candidate(
        &self,
        record: &CanonicalRecordV1,
        frozen: Option<&FormalFrozenCertificateV1>,
    ) -> Result<String, FormalError> {
        self.0.sign_candidate(record, frozen)
    }
    fn sign_endpoint_request(
        &self,
        request: &WriterEndpointRequestV1,
    ) -> Result<String, FormalError> {
        self.0.sign_endpoint_request(request)
    }
    fn sign_endpoint_request_before(
        &self,
        request: &WriterEndpointRequestV1,
        deadline: std::time::Instant,
    ) -> Result<String, FormalError> {
        self.0.sign_endpoint_request_before(request, deadline)
    }
    fn supports_healthy_durable_set(&self) -> bool {
        self.0.supports_healthy_durable_set()
    }
    fn sign_durable_set_transition(
        &self,
        body: &NormalDurableSetTransitionBodyV1,
    ) -> Result<String, FormalError> {
        self.0.sign_durable_set_transition(body)
    }
    fn sign_durable_set_candidate(
        &self,
        request: &transport::WriterDurableSetRequestV1,
    ) -> Result<String, FormalError> {
        self.0.sign_durable_set_candidate(request)
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub enum WriterCustodyCommandV1 {
    Frozen(Box<FormalFrozenBodyV1>),
    Candidate {
        record: Box<CanonicalRecordV1>,
        frozen: Option<Box<FormalFrozenCertificateV1>>,
    },
    Endpoint(Box<WriterEndpointRequestV1>),
    InstallPlan {
        plan: Box<SignedRecoveryPlanV1>,
        fixed: Box<RecoveryTailWitnessV1>,
        fixed_material: Option<Box<ClosedRecord>>,
        pending: Box<RecoveryPendingHandoffV1>,
    },
    DrainComplete(Box<RecoveryDrainedProofV1>),
    Seal {
        plan_digest: String,
        cut: FormalHeadRefV1,
    },
    Query,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WriterCustodyRequestBodyV1 {
    pub schema_version: u32,
    pub authority_id: String,
    pub challenge: String,
    pub expected_revision: Option<u64>,
    pub credential_public_key: String,
    pub command: WriterCustodyCommandV1,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedWriterCustodyRequestV1 {
    pub body: WriterCustodyRequestBodyV1,
    pub signature_hex: String,
}

pub fn writer_custody_request_signing_bytes(
    body: &WriterCustodyRequestBodyV1,
) -> Result<Vec<u8>, FormalError> {
    bytes(REQUEST_DOMAIN, body)
}

pub fn writer_custody_request_digest(
    request: &SignedWriterCustodyRequestV1,
) -> Result<String, FormalError> {
    hash(
        "oasis7.formal.writer_custody.request_digest.v1",
        &request.body,
    )
}

pub fn verify_writer_custody_request(
    request: &SignedWriterCustodyRequestV1,
) -> Result<(), FormalError> {
    if request.body.schema_version != 1 {
        return Err(invalid("writer custody request schema"));
    }
    hash_hex(&request.body.authority_id)?;
    hash_hex(&request.body.challenge)?;
    public_key(&request.body.credential_public_key)?;
    verify(
        REQUEST_DOMAIN,
        &request.body,
        &request.signature_hex,
        &request.body.credential_public_key,
    )
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WriterCustodySnapshotV1 {
    pub writer_public_key: String,
    pub authority_epoch: u64,
    pub revision: u64,
    pub plan_digest: Option<String>,
    pub drained_digest: Option<String>,
    pub seal_digest: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
pub enum WriterCustodyValueV1 {
    Signature(String),
    Plan(Box<SignedWriterRecoveryPlanAckV1>),
    Drained(Box<RecoveryDrainedProofV1>),
    Seal(Box<SignedWriterEpochSealV1>),
    State(WriterCustodySnapshotV1),
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WriterCustodyResponseBodyV1 {
    pub schema_version: u32,
    pub authority_id: String,
    pub challenge: String,
    pub request_digest: String,
    pub revision: u64,
    pub result: Result<WriterCustodyValueV1, FormalError>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedWriterCustodyResponseV1 {
    pub body: WriterCustodyResponseBodyV1,
    pub signature_hex: String,
}

pub fn writer_custody_response_signing_bytes(
    body: &WriterCustodyResponseBodyV1,
) -> Result<Vec<u8>, FormalError> {
    bytes(RESPONSE_DOMAIN, body)
}

pub fn verify_writer_custody_response(
    response: &SignedWriterCustodyResponseV1,
    request: &SignedWriterCustodyRequestV1,
    writer_public_key: &str,
) -> Result<(), FormalError> {
    if response.body.schema_version != 1
        || response.body.authority_id != request.body.authority_id
        || response.body.challenge != request.body.challenge
        || response.body.request_digest != writer_custody_request_digest(request)?
        || request
            .body
            .expected_revision
            .is_some_and(|floor| response.body.revision < floor)
    {
        return Err(invalid(
            "writer custody response original request/revision binding",
        ));
    }
    public_key(writer_public_key)?;
    verify(
        RESPONSE_DOMAIN,
        &response.body,
        &response.signature_hex,
        writer_public_key,
    )
}
