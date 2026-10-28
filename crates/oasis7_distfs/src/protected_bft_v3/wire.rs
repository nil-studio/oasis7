use ed25519_dalek::{Signer, SigningKey};
use oasis7_consensus::tendermint_v3::BftProposalIntentV3;
use oasis7_consensus::tendermint_v3::BftPublicProposalV3;
use oasis7_formal_protocol::bft::FormalBftCertifiedHeaderV3;
use oasis7_proto::distributed_bft::v3::BftVoteV3;
use serde::{Deserialize, Serialize, de::DeserializeOwned};

use super::binding::{BftSignerBindingV3, BftSignerErrorV3, invalid};
use super::state::{BftSignerCandidateV3, BftSignerCheckpointV3};

pub(crate) const BFT_SIGNER_REQUEST_DOMAIN_V3: &str = "oasis7.bft.protected-signer.v3.request";
pub(crate) const BFT_SIGNER_REQUEST_DIGEST_DOMAIN_V3: &str =
    "oasis7.bft.protected-signer.v3.request-digest";
pub(crate) const BFT_SIGNER_REPLY_DOMAIN_V3: &str = "oasis7.bft.protected-signer.v3.reply";
pub(crate) const BFT_SIGNER_LEASE_DOMAIN_V3: &str = "oasis7.bft.protected-signer.v3.lease";
pub(crate) const BFT_SIGNER_MAX_WIRE_FRAME_BYTES_V3: usize =
    super::crypto::BFT_SIGNER_MAX_FRAME_BYTES_V3;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BftSignerLeaseCredentialV3 {
    pub generation: u64,
    pub token: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "body", deny_unknown_fields)]
pub(crate) enum BftSignerCommandV3 {
    PrepareAcquire {
        binding_digest: String,
    },
    Acquire {
        binding_digest: String,
        expected_generation: u64,
        prepare_nonce: String,
    },
    Checkpoint,
    ProtectedTime,
    StartHeightTimer {
        expected_revision: u64,
    },
    Advance {
        expected_revision: u64,
        expected_state_digest: String,
        candidate: Box<BftSignerCandidateV3>,
    },
    SignProposal {
        expected_revision: u64,
        intent: Box<BftProposalIntentV3>,
    },
    SignVote {
        expected_revision: u64,
        intent: Box<oasis7_consensus::tendermint_v3::BftVoteIntentV3>,
    },
    SignCandidateRead {
        body: Box<oasis7_proto::distributed_bft::v3::BftCandidateReadBodyV3>,
    },
    ConfirmApplied {
        expected_revision: u64,
        expected_state_digest: String,
        height: u64,
        header_id: String,
        driver_install_digest: String,
    },
    AppendCommittedCertificate {
        expected_revision: u64,
        expected_state_digest: String,
        certificate: Box<FormalBftCertifiedHeaderV3>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BftSignerRequestBodyV3 {
    pub schema_version: u32,
    pub binding_digest: String,
    pub challenge: String,
    pub lease: Option<BftSignerLeaseCredentialV3>,
    pub command: BftSignerCommandV3,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SignedBftSignerRequestV3 {
    pub body: BftSignerRequestBodyV3,
    pub requester_public_key: String,
    pub requester_signature_hex: String,
}

impl SignedBftSignerRequestV3 {
    pub(crate) fn sign(
        binding: &BftSignerBindingV3,
        authentication_key: &SigningKey,
        lease: Option<BftSignerLeaseCredentialV3>,
        command: BftSignerCommandV3,
        challenge: String,
    ) -> Result<Self, BftSignerErrorV3> {
        if hex::encode(authentication_key.verifying_key().to_bytes())
            != binding.requester_auth_public_key
        {
            return Err(invalid("BFT V3 requester key differs from pinned binding"));
        }
        validate_nonce(&challenge, "request challenge")?;
        let body = BftSignerRequestBodyV3 {
            schema_version: 3,
            binding_digest: binding.digest()?,
            challenge,
            lease,
            command,
        };
        let preimage = super::crypto::framed_cbor(
            BFT_SIGNER_REQUEST_DOMAIN_V3,
            &body,
            super::crypto::BFT_SIGNER_MAX_SIGNING_BYTES_V3,
        )?;
        Ok(Self {
            body,
            requester_public_key: binding.requester_auth_public_key.clone(),
            requester_signature_hex: hex::encode(authentication_key.sign(&preimage).to_bytes()),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "body", deny_unknown_fields)]
pub(crate) enum BftSignerReplyKindV3 {
    Prepared {
        expected_generation: u64,
        prepare_nonce: String,
    },
    AcquireConflict {
        current_generation: u64,
        lease_active: bool,
    },
    Acquired,
    Checkpoint,
    ProtectedTime {
        time_ms: i64,
    },
    HeightTimerStarted,
    Advanced,
    ProposalSigned {
        proposal: Box<BftPublicProposalV3>,
    },
    VoteSigned {
        vote: Box<BftVoteV3>,
    },
    CandidateReadSigned {
        proof: Box<oasis7_proto::distributed_bft::v3::BftCandidateReadProofV3>,
    },
    CertificateAppended,
    AppliedConfirmed,
    Rejected {
        message: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BftSignerReplyBodyV3 {
    pub schema_version: u32,
    pub binding_digest: String,
    pub request_digest: String,
    pub challenge: String,
    pub lease_generation: u64,
    pub lease_token: Option<String>,
    pub state_revision: u64,
    pub state_digest: String,
    pub checkpoint: Option<Box<BftSignerCheckpointV3>>,
    pub result: BftSignerReplyKindV3,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SignedBftSignerReplyV3 {
    pub body: BftSignerReplyBodyV3,
    pub provider_signature_hex: String,
}

impl SignedBftSignerReplyV3 {
    pub(crate) fn sign(
        body: BftSignerReplyBodyV3,
        provider_key: &dyn super::binding::BftSigningKeyHandleV3,
    ) -> Result<Self, BftSignerErrorV3> {
        let signature =
            super::crypto::sign_preimage(provider_key, BFT_SIGNER_REPLY_DOMAIN_V3, &body)?;
        Ok(Self {
            body,
            provider_signature_hex: signature,
        })
    }
}

pub(crate) fn validate_request(
    binding: &BftSignerBindingV3,
    request: &SignedBftSignerRequestV3,
) -> Result<(), BftSignerErrorV3> {
    if request.body.schema_version != 3
        || request.body.binding_digest != binding.digest()?
        || request.requester_public_key != binding.requester_auth_public_key
    {
        return Err(invalid("BFT V3 request binding mismatch"));
    }
    validate_nonce(&request.body.challenge, "request challenge")?;
    super::crypto::verify_signature(
        BFT_SIGNER_REQUEST_DOMAIN_V3,
        &request.body,
        &request.requester_signature_hex,
        &binding.requester_auth_public_key,
    )
}

pub(crate) fn request_digest(
    request: &SignedBftSignerRequestV3,
) -> Result<String, BftSignerErrorV3> {
    super::crypto::digest_hex(BFT_SIGNER_REQUEST_DIGEST_DOMAIN_V3, &request.body)
}

pub(crate) fn validate_reply(
    binding: &BftSignerBindingV3,
    trust: &super::binding::BftSignerTrustV3,
    request: &SignedBftSignerRequestV3,
    reply: &SignedBftSignerReplyV3,
) -> Result<(), BftSignerErrorV3> {
    let digest = request_digest(request)?;
    if reply.body.schema_version != 3
        || reply.body.binding_digest != binding.digest()?
        || reply.body.request_digest != digest
        || reply.body.challenge != request.body.challenge
    {
        return Err(invalid("BFT V3 provider reply does not bind the request"));
    }
    super::crypto::verify_signature(
        BFT_SIGNER_REPLY_DOMAIN_V3,
        &reply.body,
        &reply.provider_signature_hex,
        &trust.provider_public_key,
    )
}

pub(crate) fn encode_frame<T: Serialize>(value: &T) -> Result<Vec<u8>, BftSignerErrorV3> {
    let body = super::crypto::encode_bounded(value, BFT_SIGNER_MAX_WIRE_FRAME_BYTES_V3)?;
    let len = u32::try_from(body.len()).map_err(|_| invalid("BFT V3 TCP frame length overflow"))?;
    let mut frame = Vec::with_capacity(4 + body.len());
    frame.extend_from_slice(&len.to_be_bytes());
    frame.extend_from_slice(&body);
    Ok(frame)
}

pub(crate) fn read_frame<T: DeserializeOwned + Serialize>(
    stream: &mut impl std::io::Read,
) -> Result<T, BftSignerErrorV3> {
    let mut header = [0u8; 4];
    stream
        .read_exact(&mut header)
        .map_err(|error| invalid(format!("BFT V3 TCP frame header: {error}")))?;
    let len = usize::try_from(u32::from_be_bytes(header))
        .map_err(|_| invalid("BFT V3 TCP frame length conversion"))?;
    if len == 0 || len > BFT_SIGNER_MAX_WIRE_FRAME_BYTES_V3 {
        return Err(invalid("BFT V3 TCP frame length is outside its bound"));
    }
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(len)
        .map_err(|_| invalid("BFT V3 TCP frame allocation failed"))?;
    bytes.resize(len, 0);
    stream
        .read_exact(&mut bytes)
        .map_err(|error| invalid(format!("BFT V3 TCP frame body: {error}")))?;
    super::crypto::decode_bounded(&bytes, BFT_SIGNER_MAX_WIRE_FRAME_BYTES_V3)
}

fn validate_nonce(value: &str, label: &str) -> Result<(), BftSignerErrorV3> {
    if value.len() != 64
        || value
            .bytes()
            .any(|byte| !byte.is_ascii_digit() && !(b'a'..=b'f').contains(&byte))
    {
        return Err(invalid(format!("BFT V3 {label} must be canonical nonce")));
    }
    Ok(())
}
