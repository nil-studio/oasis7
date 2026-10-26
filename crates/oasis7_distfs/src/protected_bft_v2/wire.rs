use ed25519_dalek::{Signer, SigningKey};
use oasis7_consensus::tendermint_v2::BftProposalIntentV2;
use oasis7_proto::distributed_bft::v2::{BftCommitCertificateV2, BftProposalV2, BftVoteV2};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

use super::binding::{BftSignerBindingV2, BftSignerErrorV2, invalid};
use super::install::{
    BftSignedTransitionInstallAckV2, BftTransitionInstallIntentV2, BftTransitionInstallReceiptV2,
};
use super::state::{BftSignerCandidateV2, BftSignerCheckpointV2};

pub(crate) const BFT_SIGNER_REQUEST_DOMAIN_V2: &str = "oasis7.bft.protected-signer.v2.request";
pub(crate) const BFT_SIGNER_REQUEST_DIGEST_DOMAIN_V2: &str =
    "oasis7.bft.protected-signer.v2.request-digest";
pub(crate) const BFT_SIGNER_REPLY_DOMAIN_V2: &str = "oasis7.bft.protected-signer.v2.reply";
pub(crate) const BFT_SIGNER_LEASE_DOMAIN_V2: &str = "oasis7.bft.protected-signer.v2.lease";
pub(crate) const BFT_SIGNER_MAX_WIRE_FRAME_BYTES_V2: usize =
    super::crypto::BFT_SIGNER_MAX_FRAME_BYTES_V2;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BftSignerLeaseCredentialV2 {
    pub generation: u64,
    pub token: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "body", deny_unknown_fields)]
pub(crate) enum BftSignerCommandV2 {
    PrepareAcquire {
        binding: Box<BftSignerBindingV2>,
    },
    Acquire {
        binding: Box<BftSignerBindingV2>,
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
        candidate: Box<BftSignerCandidateV2>,
    },
    SignProposal {
        expected_revision: u64,
        intent: Box<BftProposalIntentV2>,
    },
    SignVote {
        expected_revision: u64,
        intent: Box<oasis7_consensus::tendermint_v2::BftVoteIntentV2>,
    },
    AppendCommittedCertificate {
        expected_revision: u64,
        expected_state_digest: String,
        certificate: Box<BftCommitCertificateV2>,
    },
    PrepareTransitionInstall {
        expected_revision: u64,
        expected_state_digest: String,
        request_id: String,
        transition_source_height: u64,
        runtime_instance_id: String,
    },
    CompleteTransitionInstall {
        intent: Box<BftTransitionInstallIntentV2>,
        acknowledgement: Box<BftSignedTransitionInstallAckV2>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BftSignerRequestBodyV2 {
    pub schema_version: u32,
    pub binding_digest: String,
    pub challenge: String,
    pub lease: Option<BftSignerLeaseCredentialV2>,
    pub command: BftSignerCommandV2,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SignedBftSignerRequestV2 {
    pub body: BftSignerRequestBodyV2,
    pub requester_public_key: String,
    pub requester_signature_hex: String,
}

impl SignedBftSignerRequestV2 {
    pub(crate) fn sign(
        binding: &BftSignerBindingV2,
        authentication_key: &SigningKey,
        lease: Option<BftSignerLeaseCredentialV2>,
        command: BftSignerCommandV2,
        challenge: String,
    ) -> Result<Self, BftSignerErrorV2> {
        if hex::encode(authentication_key.verifying_key().to_bytes())
            != binding.requester_auth_public_key
        {
            return Err(invalid("BFT V2 requester key differs from pinned binding"));
        }
        validate_nonce(&challenge, "request challenge")?;
        let body = BftSignerRequestBodyV2 {
            schema_version: 2,
            binding_digest: binding.digest()?,
            challenge,
            lease,
            command,
        };
        let preimage = super::crypto::framed_cbor(
            BFT_SIGNER_REQUEST_DOMAIN_V2,
            &body,
            super::crypto::BFT_SIGNER_MAX_SIGNING_BYTES_V2,
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
pub(crate) enum BftSignerReplyKindV2 {
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
        proposal: Box<BftProposalV2>,
    },
    VoteSigned {
        vote: Box<BftVoteV2>,
    },
    CertificateAppended,
    InstallPreparedPending {
        intent: Box<BftTransitionInstallIntentV2>,
    },
    InstallPreparedCompleted {
        receipt: Box<BftTransitionInstallReceiptV2>,
    },
    TransitionInstalled {
        receipt: Box<BftTransitionInstallReceiptV2>,
    },
    Rejected {
        message: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BftSignerReplyBodyV2 {
    pub schema_version: u32,
    pub binding_digest: String,
    pub request_digest: String,
    pub challenge: String,
    pub lease_generation: u64,
    pub lease_token: Option<String>,
    pub state_revision: u64,
    pub state_digest: String,
    pub checkpoint: Option<Box<BftSignerCheckpointV2>>,
    pub result: BftSignerReplyKindV2,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SignedBftSignerReplyV2 {
    pub body: BftSignerReplyBodyV2,
    pub provider_signature_hex: String,
}

impl SignedBftSignerReplyV2 {
    pub(crate) fn sign(
        body: BftSignerReplyBodyV2,
        provider_key: &dyn super::binding::BftSigningKeyHandleV2,
    ) -> Result<Self, BftSignerErrorV2> {
        let signature =
            super::crypto::sign_preimage(provider_key, BFT_SIGNER_REPLY_DOMAIN_V2, &body)?;
        Ok(Self {
            body,
            provider_signature_hex: signature,
        })
    }
}

pub(crate) fn validate_request(
    binding: &BftSignerBindingV2,
    request: &SignedBftSignerRequestV2,
) -> Result<(), BftSignerErrorV2> {
    if request.body.schema_version != 2
        || request.body.binding_digest != binding.digest()?
        || request.requester_public_key != binding.requester_auth_public_key
    {
        return Err(invalid("BFT V2 request binding mismatch"));
    }
    validate_nonce(&request.body.challenge, "request challenge")?;
    super::crypto::verify_signature(
        BFT_SIGNER_REQUEST_DOMAIN_V2,
        &request.body,
        &request.requester_signature_hex,
        &binding.requester_auth_public_key,
    )
}

pub(crate) fn request_digest(
    request: &SignedBftSignerRequestV2,
) -> Result<String, BftSignerErrorV2> {
    super::crypto::digest_hex(BFT_SIGNER_REQUEST_DIGEST_DOMAIN_V2, &request.body)
}

pub(crate) fn validate_reply(
    binding: &BftSignerBindingV2,
    trust: &super::binding::BftSignerTrustV2,
    request: &SignedBftSignerRequestV2,
    reply: &SignedBftSignerReplyV2,
) -> Result<(), BftSignerErrorV2> {
    let digest = request_digest(request)?;
    if reply.body.schema_version != 2
        || reply.body.binding_digest != binding.digest()?
        || reply.body.request_digest != digest
        || reply.body.challenge != request.body.challenge
    {
        return Err(invalid("BFT V2 provider reply does not bind the request"));
    }
    super::crypto::verify_signature(
        BFT_SIGNER_REPLY_DOMAIN_V2,
        &reply.body,
        &reply.provider_signature_hex,
        &trust.provider_public_key,
    )
}

pub(crate) fn encode_frame<T: Serialize>(value: &T) -> Result<Vec<u8>, BftSignerErrorV2> {
    let body = super::crypto::encode_bounded(value, BFT_SIGNER_MAX_WIRE_FRAME_BYTES_V2)?;
    let len = u32::try_from(body.len()).map_err(|_| invalid("BFT V2 TCP frame length overflow"))?;
    let mut frame = Vec::with_capacity(4 + body.len());
    frame.extend_from_slice(&len.to_be_bytes());
    frame.extend_from_slice(&body);
    Ok(frame)
}

pub(crate) fn read_frame<T: DeserializeOwned + Serialize>(
    stream: &mut impl std::io::Read,
) -> Result<T, BftSignerErrorV2> {
    let mut header = [0u8; 4];
    stream
        .read_exact(&mut header)
        .map_err(|error| invalid(format!("BFT V2 TCP frame header: {error}")))?;
    let len = usize::try_from(u32::from_be_bytes(header))
        .map_err(|_| invalid("BFT V2 TCP frame length conversion"))?;
    if len == 0 || len > BFT_SIGNER_MAX_WIRE_FRAME_BYTES_V2 {
        return Err(invalid("BFT V2 TCP frame length is outside its bound"));
    }
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(len)
        .map_err(|_| invalid("BFT V2 TCP frame allocation failed"))?;
    bytes.resize(len, 0);
    stream
        .read_exact(&mut bytes)
        .map_err(|error| invalid(format!("BFT V2 TCP frame body: {error}")))?;
    super::crypto::decode_bounded(&bytes, BFT_SIGNER_MAX_WIRE_FRAME_BYTES_V2)
}

fn validate_nonce(value: &str, label: &str) -> Result<(), BftSignerErrorV2> {
    if value.len() != 64
        || value
            .bytes()
            .any(|byte| !byte.is_ascii_digit() && !(b'a'..=b'f').contains(&byte))
    {
        return Err(invalid(format!("BFT V2 {label} must be canonical nonce")));
    }
    Ok(())
}
