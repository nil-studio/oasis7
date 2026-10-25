use std::io::Read;

#[cfg(test)]
use ed25519_dalek::SigningKey;
use oasis7_consensus::tendermint_v1::{
    BftProposalIntentV1, BftSigningJournalEventV1, BftVoteIntentV1,
};
use oasis7_proto::distributed_bft::{BftProposalV1, BftVoteV1};
use serde::Serialize;

use super::super::backend::BftSignerBackendSnapshotV1;
use super::super::binding::{
    BftSignerBindingV1, BftSignerError, BftSigningKeyHandleV1, invalid, verify_signature,
};
use super::super::validation::BftSignerStateV1;
use super::super::wire::{
    BFT_REQUEST_DOMAIN_V1, BFT_SIGNER_MAX_SIGNING_BYTES_V1, BftSignerLeaseCredentialV1,
    BftSignerReplyKindV1, SignedBftSignerRequestV1, framed_cbor,
};

pub(super) struct ReplyData {
    pub(super) state: Option<BftSignerStateV1>,
    pub(super) state_revision: u64,
    pub(super) state_digest: String,
    pub(super) generation: u64,
    pub(super) lease_token: Option<String>,
    pub(super) result: BftSignerReplyKindV1,
}

pub(super) fn state_reply(
    snapshot: BftSignerBackendSnapshotV1,
    result: BftSignerReplyKindV1,
) -> ReplyData {
    ReplyData {
        state_revision: snapshot.state.revision,
        state_digest: snapshot.state.state_digest.clone(),
        state: Some(snapshot.state),
        generation: snapshot.lease_generation,
        lease_token: None,
        result,
    }
}

pub(super) fn acquire_conflict_reply(snapshot: BftSignerBackendSnapshotV1) -> ReplyData {
    ReplyData {
        state: None,
        state_revision: snapshot.state.revision,
        state_digest: snapshot.state.state_digest,
        generation: snapshot.lease_generation,
        lease_token: None,
        result: BftSignerReplyKindV1::AcquireConflict {
            current_generation: snapshot.lease_generation,
            lease_active: !snapshot.lease_token_digest.is_empty(),
        },
    }
}

pub(super) fn validate_request(
    binding: &BftSignerBindingV1,
    request: &SignedBftSignerRequestV1,
) -> Result<(), BftSignerError> {
    if request.body.schema_version != 1 || request.body.binding_digest != binding.digest()? {
        return Err(invalid("BFT signer request schema or binding mismatch"));
    }
    validate_nonce(&request.body.challenge)?;
    if request.authentication_public_key != binding.requester_auth_public_key {
        return Err(invalid("BFT signer requester authentication key mismatch"));
    }
    verify_signature(
        BFT_REQUEST_DOMAIN_V1,
        &request.body,
        &request.signature_hex,
        &request.authentication_public_key,
    )
}

pub(super) fn validate_hex_digest(value: &str) -> Result<(), BftSignerError> {
    if value.len() != 64
        || value
            .bytes()
            .any(|byte| !byte.is_ascii_digit() && !(b'a'..=b'f').contains(&byte))
    {
        return Err(invalid("BFT signer digest must be canonical lowercase hex"));
    }
    Ok(())
}

pub(super) fn validate_lease_credential(
    lease: &BftSignerLeaseCredentialV1,
) -> Result<(), BftSignerError> {
    if lease.generation == 0 || lease.token.len() != 64 {
        return Err(invalid("BFT signer lease credential"));
    }
    if lease
        .token
        .bytes()
        .any(|byte| !byte.is_ascii_digit() && !(b'a'..=b'f').contains(&byte))
    {
        return Err(invalid("BFT signer lease token encoding"));
    }
    Ok(())
}

pub(super) fn validate_nonce(nonce: &str) -> Result<(), BftSignerError> {
    if nonce.len() != 64
        || nonce
            .bytes()
            .any(|byte| !byte.is_ascii_digit() && !(b'a'..=b'f').contains(&byte))
    {
        return Err(invalid(
            "BFT signer challenge must be 32-byte lowercase hex",
        ));
    }
    Ok(())
}

pub(super) fn bounded_error(error: BftSignerError) -> String {
    const MAX_ERROR_BYTES: usize = 512;
    let message = error.to_string();
    if message.len() <= MAX_ERROR_BYTES {
        return message;
    }
    let mut end = MAX_ERROR_BYTES;
    while !message.is_char_boundary(end) {
        end -= 1;
    }
    message[..end].to_string()
}

pub(super) fn random_token() -> Result<String, BftSignerError> {
    #[cfg(unix)]
    {
        let mut bytes = [0u8; 32];
        std::fs::File::open("/dev/urandom")
            .and_then(|mut file| file.read_exact(&mut bytes))
            .map_err(|error| invalid(format!("BFT signer secure random: {error}")))?;
        Ok(hex::encode(bytes))
    }
    #[cfg(not(unix))]
    {
        Err(invalid(
            "BFT protected signer requires a secure random source",
        ))
    }
}

pub(super) fn sign_body<T: Serialize>(
    domain: &str,
    body: &T,
    key: &dyn BftSigningKeyHandleV1,
) -> Result<String, BftSignerError> {
    let preimage = framed_cbor(domain, body, BFT_SIGNER_MAX_SIGNING_BYTES_V1)?;
    Ok(hex::encode(key.sign_preimage(&preimage)?))
}

pub(super) fn find_signed_proposal(
    journal: &oasis7_consensus::tendermint_v1::BftSigningJournalV1,
    intent: &BftProposalIntentV1,
) -> Result<Option<BftProposalV1>, BftSignerError> {
    for event in journal.events() {
        if let BftSigningJournalEventV1::Proposal {
            proposal,
            local: true,
            ..
        } = event
            && proposal.header.height == intent.header.height
            && proposal.header.round == intent.header.round
            && proposal.header.proposer_id == intent.header.proposer_id
        {
            let mut header = proposal.header.clone();
            header.signature_hex.clear();
            if header == intent.header
                && proposal.block == intent.block
                && proposal.valid_round_evidence == intent.valid_round_evidence
            {
                proposal
                    .header
                    .verify_signature()
                    .map_err(|error| invalid(format!("stored proposal signature: {error}")))?;
                return Ok(Some(proposal.as_ref().clone()));
            }
            return Err(invalid(
                "BFT signer proposal slot contains another decision",
            ));
        }
    }
    Ok(None)
}

pub(super) fn find_signed_vote(
    journal: &oasis7_consensus::tendermint_v1::BftSigningJournalV1,
    intent: &BftVoteIntentV1,
) -> Result<Option<BftVoteV1>, BftSignerError> {
    for event in journal.events() {
        if let BftSigningJournalEventV1::Vote {
            vote, local: true, ..
        } = event
            && vote.height == intent.height
            && vote.round == intent.round
            && vote.phase == intent.phase
            && vote.validator_id == intent.validator_id
        {
            let mut unsigned = vote.clone();
            unsigned.signature_hex.clear();
            if unsigned.height == intent.height
                && unsigned.round == intent.round
                && unsigned.phase == intent.phase
                && unsigned.value == intent.value
                && unsigned.validator_id == intent.validator_id
            {
                vote.verify_signature()
                    .map_err(|error| invalid(format!("stored vote signature: {error}")))?;
                return Ok(Some(vote.as_ref().clone()));
            }
            return Err(invalid("BFT signer vote slot contains another decision"));
        }
    }
    Ok(None)
}

/// A test-only software implementation can wrap `SigningKey`; production
/// providers should implement the key-handle contract with protected custody.
#[cfg(test)]
pub(crate) struct SoftwareBftSigningKeyV1(SigningKey);

#[cfg(test)]
impl SoftwareBftSigningKeyV1 {
    pub(crate) fn new(key: SigningKey) -> Self {
        Self(key)
    }
}

#[cfg(test)]
impl BftSigningKeyHandleV1 for SoftwareBftSigningKeyV1 {
    fn public_key_hex(&self) -> String {
        hex::encode(self.0.verifying_key().to_bytes())
    }

    fn sign_preimage(&self, preimage: &[u8]) -> Result<[u8; 64], BftSignerError> {
        use ed25519_dalek::Signer;
        Ok(self.0.sign(preimage).to_bytes())
    }
}

#[cfg(test)]
fn _test_key_handle_is_only_a_wrapper(_: SigningKey) {}
