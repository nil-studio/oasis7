use super::{
    FormalError, FullSyncIssuerAnchorV2, FullSyncIssuerRecordV2, MAX_ENTRY_BYTES,
    OBSERVATION_DOMAIN, QUERY_DOMAIN, RECEIPT_DOMAIN, VerifiedFullSyncIssuerPolicyV2, canonical,
    encode, invalid, verify,
};
use ed25519_dalek::{Signer, SigningKey};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FullSyncIssuerQueryBodyV2 {
    pub schema_version: u32,
    pub issuer_public_key: String,
    pub policy_digest: String,
    pub scope_digest: String,
    pub observation_nonce: String,
    pub minimum_anchor: FullSyncIssuerAnchorV2,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedFullSyncIssuerQueryV2 {
    pub body: FullSyncIssuerQueryBodyV2,
    pub signature_hex: String,
}

impl SignedFullSyncIssuerQueryV2 {
    pub fn sign(
        policy: &VerifiedFullSyncIssuerPolicyV2,
        minimum_anchor: FullSyncIssuerAnchorV2,
        nonce: [u8; 32],
        key: &SigningKey,
    ) -> Result<Self, FormalError> {
        if hex::encode(key.verifying_key().as_bytes()) != policy.body().issuer_public_key {
            return Err(invalid("full-sync query issuer signer mismatch"));
        }
        minimum_anchor.validate()?;
        if minimum_anchor.scope_digest != policy.scope_digest {
            return Err(invalid("full-sync query minimum scope"));
        }
        let body = FullSyncIssuerQueryBodyV2 {
            schema_version: 2,
            issuer_public_key: policy.body().issuer_public_key.clone(),
            policy_digest: policy.policy_digest.clone(),
            scope_digest: policy.scope_digest.clone(),
            observation_nonce: hex::encode(nonce),
            minimum_anchor,
        };
        validate_query_body(&body)?;
        let signature_hex = hex::encode(
            key.sign(&encode(QUERY_DOMAIN, &body, MAX_ENTRY_BYTES)?)
                .to_bytes(),
        );
        Ok(Self {
            body,
            signature_hex,
        })
    }

    pub fn verify(&self, policy: &VerifiedFullSyncIssuerPolicyV2) -> Result<(), FormalError> {
        validate_query_body(&self.body)?;
        if self.body.issuer_public_key != policy.body().issuer_public_key
            || self.body.policy_digest != policy.policy_digest
            || self.body.scope_digest != policy.scope_digest
            || self.body.minimum_anchor.scope_digest != policy.scope_digest
        {
            return Err(invalid("full-sync query policy/scope binding"));
        }
        verify(
            QUERY_DOMAIN,
            &self.body,
            &self.signature_hex,
            &policy.body().issuer_public_key,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FullSyncIssuerObservationBodyV2 {
    pub schema_version: u32,
    pub query: SignedFullSyncIssuerQueryV2,
    pub current_anchor: FullSyncIssuerAnchorV2,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedFullSyncIssuerObservationV2 {
    pub body: FullSyncIssuerObservationBodyV2,
    pub signature_hex: String,
}

impl SignedFullSyncIssuerObservationV2 {
    /// Signing represents a witness assertion only. It does not establish that
    /// the key is deployed in an independent or nonrollback fault domain.
    pub fn sign(
        body: FullSyncIssuerObservationBodyV2,
        key: &SigningKey,
    ) -> Result<Self, FormalError> {
        let signature_hex = hex::encode(
            key.sign(&encode(OBSERVATION_DOMAIN, &body, MAX_ENTRY_BYTES)?)
                .to_bytes(),
        );
        Ok(Self {
            body,
            signature_hex,
        })
    }

    pub fn verify(&self, policy: &VerifiedFullSyncIssuerPolicyV2) -> Result<(), FormalError> {
        let body = &self.body;
        body.query.verify(policy)?;
        body.current_anchor.validate()?;
        if body.schema_version != 2
            || body.current_anchor.scope_digest != policy.scope_digest
            || !anchor_at_least(&body.current_anchor, &body.query.body.minimum_anchor)
        {
            return Err(FormalError::Rollback(
                "full-sync witness observation floor".into(),
            ));
        }
        verify(
            OBSERVATION_DOMAIN,
            body,
            &self.signature_hex,
            &policy.body().witness_public_key,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FullSyncIssuerReceiptBodyV2 {
    pub schema_version: u32,
    pub observation: SignedFullSyncIssuerObservationV2,
    pub record_digest: String,
    pub request_id: String,
    pub previous_anchor: FullSyncIssuerAnchorV2,
    pub next_anchor: FullSyncIssuerAnchorV2,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedFullSyncIssuerReceiptV2 {
    pub body: FullSyncIssuerReceiptBodyV2,
    pub signature_hex: String,
}

impl SignedFullSyncIssuerReceiptV2 {
    /// Called by a witness only after its unique CAS and full-record durability
    /// contract has succeeded.
    pub fn sign(
        observation: SignedFullSyncIssuerObservationV2,
        record: &FullSyncIssuerRecordV2,
        key: &SigningKey,
    ) -> Result<Self, FormalError> {
        if hex::encode(key.verifying_key().as_bytes()) != record.policy.body.witness_public_key {
            return Err(invalid("full-sync receipt witness signer mismatch"));
        }
        let body = FullSyncIssuerReceiptBodyV2 {
            schema_version: 2,
            observation,
            record_digest: record.digest()?,
            request_id: record.command.body.request_id.clone(),
            previous_anchor: record.previous_anchor.clone(),
            next_anchor: record.next_anchor.clone(),
        };
        let signature_hex = hex::encode(
            key.sign(&encode(RECEIPT_DOMAIN, &body, MAX_ENTRY_BYTES)?)
                .to_bytes(),
        );
        Ok(Self {
            body,
            signature_hex,
        })
    }

    pub fn verify_for_record(
        &self,
        record: &FullSyncIssuerRecordV2,
        policy: &VerifiedFullSyncIssuerPolicyV2,
    ) -> Result<(), FormalError> {
        let body = &self.body;
        body.observation.verify(policy)?;
        if body.schema_version != 2
            || body.record_digest != record.digest()?
            || body.request_id != record.command.body.request_id
            || body.previous_anchor != record.previous_anchor
            || body.next_anchor != record.next_anchor
            || body.observation.body.current_anchor != record.previous_anchor
            || body.observation.body.query.body.minimum_anchor != record.previous_anchor
        {
            return Err(invalid("full-sync witness receipt record/CAS binding"));
        }
        verify(
            RECEIPT_DOMAIN,
            body,
            &self.signature_hex,
            &policy.body().witness_public_key,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FullSyncIssuerCommittedEntryV2 {
    pub record: FullSyncIssuerRecordV2,
    pub receipt: SignedFullSyncIssuerReceiptV2,
}

/// `records_after` is exclusive at `after` and inclusive at `through`; passing
/// the exact genesis anchor therefore requests the complete retained log. A
/// production adapter MUST enforce the 1,024-entry and 64 MiB total framed
/// payload limits before allocating or decoding the returned `Vec`; over-limit
/// or truncated responses must fail closed. The protocol repeats these bounds
/// against canonical encoded entries while reconstructing decoded records,
/// but that cannot protect an adapter that allocates an oversized response
/// first. The adapter must not return records beyond `through`, and callers
/// must verify the returned chain closes exactly at the fresh nonce-bound
/// observation. Records remain untrusted until reconstructed from external
/// trust. Implementations must make query nonces unique and serialize a single
/// CAS.
pub trait FullSyncIssuerWitnessV2: Send {
    fn observe(
        &mut self,
        query: &SignedFullSyncIssuerQueryV2,
    ) -> Result<SignedFullSyncIssuerObservationV2, FormalError>;

    fn records_after(
        &mut self,
        after: &FullSyncIssuerAnchorV2,
        through: &FullSyncIssuerAnchorV2,
    ) -> Result<Vec<FullSyncIssuerCommittedEntryV2>, FormalError>;

    fn commit(
        &mut self,
        observation: &SignedFullSyncIssuerObservationV2,
        record: &FullSyncIssuerRecordV2,
    ) -> Result<SignedFullSyncIssuerReceiptV2, FormalError>;
}

fn validate_query_body(body: &FullSyncIssuerQueryBodyV2) -> Result<(), FormalError> {
    body.minimum_anchor.validate()?;
    for value in [
        &body.issuer_public_key,
        &body.policy_digest,
        &body.scope_digest,
        &body.observation_nonce,
    ] {
        canonical::<32>(value)?;
    }
    if body.schema_version != 2
        || body.observation_nonce.bytes().all(|byte| byte == b'0')
        || body.minimum_anchor.scope_digest != body.scope_digest
    {
        return Err(invalid("full-sync witness query version/nonce/scope"));
    }
    Ok(())
}

fn anchor_at_least(current: &FullSyncIssuerAnchorV2, minimum: &FullSyncIssuerAnchorV2) -> bool {
    current.scope_digest == minimum.scope_digest
        && current.revision >= minimum.revision
        && (current.revision != minimum.revision || current == minimum)
}
