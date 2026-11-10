//! Native endpoint seal and external monotonic progress contract. A signed
//! checkpoint or a controlled local ledger is never nonrollback evidence.
use crate::controlled_authority::LocalRequestIdentity;
use crate::controlled_authority::replicated_protocol::{EndpointRole, FixedTrust, HeadAnchor};
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};
use std::fmt;
#[path = "fencing_durable_set.rs"]
mod durable_set;
#[path = "fencing_protected.rs"]
mod protected;
pub use durable_set::*;
pub use protected::*;
pub const SEAL_DOMAIN: &str = "oasis7.endpoint.epoch-seal.v1";
pub const PROVIDER_DOMAIN: &str = "oasis7.endpoint.monotonic-provider.v1";
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FencingError {
    Invalid(String),
    Io(String),
    Locked,
    Rollback,
    EpochSealed,
    FencingUnproven,
}
impl fmt::Display for FencingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(s) | Self::Io(s) => f.write_str(s),
            Self::Locked => f.write_str("FENCING_PROVIDER_LOCKED"),
            Self::Rollback => f.write_str("FENCING_PROGRESS_ROLLBACK"),
            Self::EpochSealed => f.write_str("EPOCH_SEALED"),
            Self::FencingUnproven => f.write_str("FENCING_UNPROVEN"),
        }
    }
}
impl std::error::Error for FencingError {}
pub fn invalid(s: impl Into<String>) -> FencingError {
    FencingError::Invalid(s.into())
}
pub fn hash_value<T: Serialize>(value: &T) -> Result<String, FencingError> {
    Ok(hash_encoded(
        &serde_cbor::to_vec(value).map_err(|e| invalid(e.to_string()))?,
    ))
}
/// The same digest over an already encoded canonical protocol value.
pub fn hash_encoded(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}
pub fn canonical_hash(hash: &str) -> Result<(), FencingError> {
    if hash.len() != 64
        || !hash
            .bytes()
            .all(|v| v.is_ascii_digit() || (b'a'..=b'f').contains(&v))
    {
        return Err(invalid("canonical fencing hash/key"));
    }
    Ok(())
}
fn bytes<T: Serialize>(domain: &str, value: &T) -> Result<Vec<u8>, FencingError> {
    let mut bytes = (domain.len() as u64).to_be_bytes().to_vec();
    bytes.extend(domain.as_bytes());
    bytes.extend(serde_cbor::to_vec(value).map_err(|e| invalid(e.to_string()))?);
    Ok(bytes)
}
/// Shared signing-preimage codec for operational provider adapters.
pub fn encoding_bytes<T: Serialize>(domain: &str, value: &T) -> Result<Vec<u8>, FencingError> {
    bytes(domain, value)
}
pub(crate) fn sign<T: Serialize>(
    domain: &str,
    value: &T,
    key: &SigningKey,
) -> Result<String, FencingError> {
    sign_with_preimage(key, || bytes(domain, value), |_| Ok(()))
}
/// Build the original framing at its evaluation point, with caller-paid IO.
pub fn sign_with_preimage<B, E>(
    key: &SigningKey,
    build: impl FnOnce() -> Result<B, E>,
    mut work: impl FnMut(usize) -> Result<(), E>,
) -> Result<String, E>
where
    B: AsRef<[u8]>,
{
    let preimage = build()?;
    work(preimage.as_ref().len())?;
    Ok(hex::encode(key.sign(preimage.as_ref()).to_bytes()))
}
/// Sign a fencing protocol preimage with the adapter's explicitly held key.
pub fn sign_fencing_body<T: Serialize>(
    domain: &str,
    value: &T,
    key: &SigningKey,
) -> Result<String, FencingError> {
    sign(domain, value, key)
}
pub fn verify<T: Serialize>(
    domain: &str,
    value: &T,
    signature: &str,
    key: &str,
) -> Result<(), FencingError> {
    verify_with_preimage(signature, key, || bytes(domain, value), |_| Ok(()))
}
/// The legacy key/signature/error order is shared by bounded adapters.
pub fn verify_with_preimage<B, E>(
    signature: &str,
    key: &str,
    build: impl FnOnce() -> Result<B, E>,
    mut work: impl FnMut(usize) -> Result<(), E>,
) -> Result<(), E>
where
    B: AsRef<[u8]>,
    E: From<FencingError>,
{
    work(key.len())?;
    canonical_hash(key).map_err(E::from)?;
    work(key.len())?;
    let key: [u8; 32] = hex::decode(key)
        .map_err(|e| E::from(invalid(e.to_string())))?
        .try_into()
        .map_err(|_| E::from(invalid("fencing key size")))?;
    work(signature.len())?;
    let signature: [u8; 64] = hex::decode(signature)
        .map_err(|e| E::from(invalid(e.to_string())))?
        .try_into()
        .map_err(|_| E::from(invalid("fencing signature size")))?;
    work(key.len())?;
    let verifier = VerifyingKey::from_bytes(&key).map_err(|e| E::from(invalid(e.to_string())))?;
    let preimage = build()?;
    work(preimage.as_ref().len())?;
    verifier
        .verify_strict(preimage.as_ref(), &Signature::from_bytes(&signature))
        .map_err(|e| E::from(invalid(e.to_string())))
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EndpointInstanceBindingV1 {
    pub schema_version: u32,
    pub world_id: String,
    pub chain_id: String,
    pub genesis_digest: String,
    pub authority_epoch: u64,
    pub endpoint_role: EndpointRole,
    pub endpoint_id: String,
    pub endpoint_public_key: String,
    pub instance_id: String,
    pub storage_id: String,
    pub storage_fingerprint: String,
}
impl EndpointInstanceBindingV1 {
    pub fn validate(&self) -> Result<(), FencingError> {
        if self.schema_version != 1
            || self.authority_epoch == 0
            || [&self.world_id, &self.chain_id, &self.endpoint_id]
                .iter()
                .any(|s| s.trim().is_empty() || s.len() > 256)
        {
            return Err(invalid("endpoint instance identity/schema"));
        }
        for value in [
            &self.genesis_digest,
            &self.endpoint_public_key,
            &self.instance_id,
            &self.storage_id,
            &self.storage_fingerprint,
        ] {
            canonical_hash(value)?;
        }
        Ok(())
    }
    pub fn digest(&self) -> Result<String, FencingError> {
        self.validate()?;
        hash_value(self)
    }
    pub fn matches_trust(
        &self,
        trust: &FixedTrust,
        role: EndpointRole,
    ) -> Result<(), FencingError> {
        self.validate()?;
        let (id, key) = match role {
            EndpointRole::Primary => (&trust.primary_id, &trust.primary_key),
            EndpointRole::Replica => (&trust.replica_id, &trust.replica_key),
        };
        if self.world_id != trust.world_id
            || self.chain_id != trust.chain_id
            || self.genesis_digest != trust.genesis_digest
            || self.authority_epoch != trust.authority_epoch
            || self.endpoint_role != role
            || &self.endpoint_id != id
            || &self.endpoint_public_key != key
        {
            return Err(invalid("endpoint exact instance/trust mismatch"));
        }
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EndpointCheckpointBindingV1 {
    pub revision: u64,
    pub commitment: String,
    pub head: HeadAnchor,
}
impl EndpointCheckpointBindingV1 {
    pub fn validate(&self) -> Result<(), FencingError> {
        canonical_hash(&self.commitment)?;
        canonical_hash(&self.head.decision_hash)
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SealedTailKindV1 {
    Empty,
    Frozen,
    Prepared,
    Decision,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SealedTailV1 {
    pub kind: SealedTailKindV1,
    pub request: Option<LocalRequestIdentity>,
    pub position: Option<u64>,
    pub frozen_input_digest: Option<String>,
    pub frozen_certificate_digest: Option<String>,
    pub proposal_digest: Option<String>,
    pub replica_prepare_digest: Option<String>,
    pub primary_receipt_digest: Option<String>,
    pub replica_receipt_digest: Option<String>,
}
impl SealedTailV1 {
    pub fn empty() -> Self {
        Self {
            kind: SealedTailKindV1::Empty,
            request: None,
            position: None,
            frozen_input_digest: None,
            frozen_certificate_digest: None,
            proposal_digest: None,
            replica_prepare_digest: None,
            primary_receipt_digest: None,
            replica_receipt_digest: None,
        }
    }
    fn validate(&self, highest: &HeadAnchor) -> Result<(), FencingError> {
        for digest in [
            &self.frozen_input_digest,
            &self.frozen_certificate_digest,
            &self.proposal_digest,
            &self.replica_prepare_digest,
            &self.primary_receipt_digest,
            &self.replica_receipt_digest,
        ]
        .into_iter()
        .flatten()
        {
            canonical_hash(digest)?;
        }
        if self.kind == SealedTailKindV1::Empty {
            if self != &Self::empty() {
                return Err(invalid("nonempty empty seal tail"));
            }
            return Ok(());
        }
        self.request
            .as_ref()
            .ok_or_else(|| invalid("seal tail request"))?
            .validate()
            .map_err(|e| invalid(e.to_string()))?;
        if self.position != highest.position.checked_add(1) {
            return Err(invalid("seal tail position"));
        }
        match self.kind {
            SealedTailKindV1::Frozen if self.frozen_input_digest.is_none() => {
                Err(invalid("frozen seal tail"))
            }
            SealedTailKindV1::Prepared | SealedTailKindV1::Decision
                if self.proposal_digest.is_none() =>
            {
                Err(invalid("candidate seal tail"))
            }
            _ => Ok(()),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EpochSealBodyV1 {
    pub schema_version: u32,
    pub instance: EndpointInstanceBindingV1,
    pub recovery_operation: String,
    pub highest_complete: HeadAnchor,
    pub checkpoint_before: EndpointCheckpointBindingV1,
    pub tail: SealedTailV1,
    pub next_epoch_floor: u64,
}
impl EpochSealBodyV1 {
    pub fn validate(&self) -> Result<(), FencingError> {
        self.instance.validate()?;
        self.checkpoint_before.validate()?;
        canonical_hash(&self.recovery_operation)?;
        canonical_hash(&self.highest_complete.decision_hash)?;
        if self.schema_version != 1
            || self.next_epoch_floor <= self.instance.authority_epoch
            || (self.highest_complete.position != 0 && !self.highest_complete.qualified)
            || self.checkpoint_before.head.position < self.highest_complete.position
            || self.checkpoint_before.head.position
                > self.highest_complete.position.saturating_add(1)
        {
            return Err(invalid("epoch seal boundary"));
        }
        if self.checkpoint_before.head.position == self.highest_complete.position {
            if self.checkpoint_before.head != self.highest_complete
                || self.tail.kind == SealedTailKindV1::Decision
            {
                return Err(invalid("seal complete-prefix/head mismatch"));
            }
        } else if self.tail.kind != SealedTailKindV1::Decision
            || self.tail.proposal_digest.as_deref()
                != Some(self.checkpoint_before.head.decision_hash.as_str())
            || self.checkpoint_before.head.qualified
        {
            return Err(invalid("seal pending decision/head mismatch"));
        }
        self.tail.validate(&self.highest_complete)
    }
    pub fn digest(&self) -> Result<String, FencingError> {
        self.validate()?;
        hash_value(self)
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProviderCapabilityV1 {
    ControlledLocalTest,
    MonotonicExclusiveSignerStorageV1,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MonotonicProviderStatementV1 {
    pub schema_version: u32,
    pub capability: ProviderCapabilityV1,
    pub resource_binding: String,
    pub instance_digest: String,
    pub checkpoint: EndpointCheckpointBindingV1,
    pub epoch_floor: u64,
    pub seal_digest: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MonotonicProviderEvidenceV1 {
    pub statement: MonotonicProviderStatementV1,
    pub signature: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EpochSealReceiptV1 {
    pub body: EpochSealBodyV1,
    pub endpoint_signature: String,
    pub provider: MonotonicProviderEvidenceV1,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protected_signature: Option<ProtectedEndpointSignatureV1>,
}
impl EpochSealReceiptV1 {
    pub fn sign(
        body: EpochSealBodyV1,
        provider: MonotonicProviderEvidenceV1,
        key: &SigningKey,
    ) -> Result<Self, FencingError> {
        body.validate()?;
        if hex::encode(key.verifying_key().to_bytes()) != body.instance.endpoint_public_key {
            return Err(invalid("seal endpoint signer"));
        }
        let endpoint_signature = sign(SEAL_DOMAIN, &body, key)?;
        Ok(Self {
            body,
            endpoint_signature,
            provider,
            protected_signature: None,
        })
    }
    pub fn digest(&self) -> Result<String, FencingError> {
        hash_value(self)
    }
}
#[derive(Debug, Clone)]
pub struct TrustedFencingPolicyV1 {
    pub provider_public_key: String,
    pub authorized_resource_bindings: Vec<String>,
}
/// Native consumers must validate the provider statement against independently
/// pinned deployment policy before using the acquired guard as a write gate.
pub fn verify_monotonic_checkpoint_evidence(
    evidence: &MonotonicProviderEvidenceV1,
    instance: &EndpointInstanceBindingV1,
    checkpoint: &EndpointCheckpointBindingV1,
    policy: &TrustedFencingPolicyV1,
) -> Result<(), FencingError> {
    verify_monotonic_checkpoint_evidence_with(
        evidence,
        instance,
        checkpoint,
        policy,
        |statement, signature, key| verify(PROVIDER_DOMAIN, statement, signature, key),
        EndpointInstanceBindingV1::digest,
        |policy, resource| {
            Ok(policy
                .authorized_resource_bindings
                .iter()
                .any(|v| v == resource))
        },
    )
}
/// Preserve the original statement verification and comparison order while
/// allowing operational adapters to pay for encoding, hashing and traversal.
pub fn verify_monotonic_checkpoint_evidence_with<E>(
    evidence: &MonotonicProviderEvidenceV1,
    instance: &EndpointInstanceBindingV1,
    checkpoint: &EndpointCheckpointBindingV1,
    policy: &TrustedFencingPolicyV1,
    verify_statement: impl FnOnce(&MonotonicProviderStatementV1, &str, &str) -> Result<(), E>,
    instance_digest: impl FnOnce(&EndpointInstanceBindingV1) -> Result<String, E>,
    authorized: impl FnOnce(&TrustedFencingPolicyV1, &str) -> Result<bool, E>,
) -> Result<(), E>
where
    E: From<FencingError>,
{
    let statement = &evidence.statement;
    verify_statement(statement, &evidence.signature, &policy.provider_public_key)?;
    if statement.schema_version != 1
        || statement.capability != ProviderCapabilityV1::MonotonicExclusiveSignerStorageV1
        || !authorized(policy, &statement.resource_binding)?
        || statement.instance_digest != instance_digest(instance)?
        || &statement.checkpoint != checkpoint
        || statement.epoch_floor < instance.authority_epoch
    {
        return Err(E::from(FencingError::FencingUnproven));
    }
    Ok(())
}
/// Constructor is private: recovery consumers cannot substitute a bool/config
/// assertion for successfully checked trusted provider evidence.
#[derive(Debug, Clone)]
pub struct FencingVerdict {
    seal_digest: String,
    instance_digest: String,
    body: EpochSealBodyV1,
    provider_public_key: String,
    resource_binding: String,
}
impl FencingVerdict {
    pub fn provider_public_key(&self) -> &str {
        &self.provider_public_key
    }
    pub fn resource_binding(&self) -> &str {
        &self.resource_binding
    }
    pub fn seal_body(&self) -> &EpochSealBodyV1 {
        &self.body
    }
    pub fn seal_digest(&self) -> &str {
        &self.seal_digest
    }
    pub fn instance_digest(&self) -> &str {
        &self.instance_digest
    }
}
/// Test-only authority fixture; this is never deployment/provider evidence.
#[cfg(test)]
pub(crate) fn test_verified_seal(
    body: EpochSealBodyV1,
    provider_public_key: String,
) -> FencingVerdict {
    body.validate().expect("valid test seal boundary");
    FencingVerdict {
        seal_digest: body.digest().expect("test seal digest"),
        instance_digest: body.instance.digest().expect("test instance digest"),
        resource_binding: body.instance.storage_fingerprint.clone(),
        provider_public_key,
        body,
    }
}
#[cfg(test)]
pub(crate) fn test_verified_seal_with_resource(
    body: EpochSealBodyV1,
    provider_public_key: String,
    resource_binding: String,
) -> FencingVerdict {
    let mut verdict = test_verified_seal(body, provider_public_key);
    verdict.resource_binding = resource_binding;
    verdict
}
pub fn verify_epoch_seal(
    receipt: &EpochSealReceiptV1,
    policy: &TrustedFencingPolicyV1,
) -> Result<FencingVerdict, FencingError> {
    receipt.body.validate()?;
    verify(
        SEAL_DOMAIN,
        &receipt.body,
        &receipt.endpoint_signature,
        &receipt.body.instance.endpoint_public_key,
    )?;
    let statement = &receipt.provider.statement;
    if statement.capability != ProviderCapabilityV1::MonotonicExclusiveSignerStorageV1 {
        return Err(FencingError::FencingUnproven);
    }
    verify(
        PROVIDER_DOMAIN,
        statement,
        &receipt.provider.signature,
        &policy.provider_public_key,
    )?;
    let seal_digest = receipt.body.digest()?;
    let protected = receipt
        .protected_signature
        .as_ref()
        .ok_or(FencingError::FencingUnproven)?;
    let request = ProtectedEndpointSigningRequestV1 {
        schema_version: 1,
        instance_digest: receipt.body.instance.digest()?,
        checkpoint: statement.checkpoint.clone(),
        challenge: protected.body.challenge.clone(),
        purpose: ProtectedEndpointSignaturePurposeV1::EpochSeal,
        preimage: bytes(SEAL_DOMAIN, &receipt.body)?,
    };
    if verify_protected_endpoint_signature(protected, &request, &receipt.body.instance, policy)?
        != receipt.endpoint_signature
        || protected.body.resource_binding != statement.resource_binding
    {
        return Err(FencingError::FencingUnproven);
    }
    let instance_digest = receipt.body.instance.digest()?;
    statement.checkpoint.validate()?;
    if statement.schema_version != 1
        || !policy
            .authorized_resource_bindings
            .contains(&statement.resource_binding)
        || statement.instance_digest != instance_digest
        || statement.seal_digest.as_deref() != Some(&seal_digest)
        || statement.epoch_floor != receipt.body.next_epoch_floor
        || statement.checkpoint.revision
            != receipt
                .body
                .checkpoint_before
                .revision
                .checked_add(1)
                .ok_or_else(|| invalid("seal revision overflow"))?
        || statement.checkpoint.head != receipt.body.checkpoint_before.head
    {
        return Err(invalid("trusted provider seal binding"));
    }
    Ok(FencingVerdict {
        seal_digest,
        instance_digest,
        body: receipt.body.clone(),
        provider_public_key: policy.provider_public_key.clone(),
        resource_binding: statement.resource_binding.clone(),
    })
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckpointIntentV1 {
    pub previous: EndpointCheckpointBindingV1,
    pub next: EndpointCheckpointBindingV1,
    pub checkpoint_bytes: Vec<u8>,
    pub seal: Option<EpochSealBodyV1>,
}
/// Held across checkpoint publication AND receipt signing. Production providers
/// must guarantee nonrollback progress and exclusive signer/storage identity.
pub trait MonotonicEndpointGuard: Send {
    fn sign_endpoint(
        &mut self,
        _request: ProtectedEndpointSigningRequestV1,
    ) -> Result<ProtectedEndpointSignatureV1, FencingError> {
        Err(FencingError::FencingUnproven)
    }
    fn protected_native_checkpoint(&self) -> Option<&[u8]> {
        None
    }
    fn durable_set_state(&self) -> Option<&DurableSetProviderStateV1> {
        None
    }
    fn checkpoint(&self) -> &EndpointCheckpointBindingV1;
    fn pending(&self) -> Option<&CheckpointIntentV1>;
    fn seal(&self) -> Option<&EpochSealBodyV1>;
    fn begin(&mut self, intent: CheckpointIntentV1) -> Result<(), FencingError>;
    fn finish(&mut self) -> Result<(), FencingError>;
    fn evidence(&self) -> Result<MonotonicProviderEvidenceV1, FencingError>;
}
pub trait MonotonicEndpointProvider: Send + Sync {
    fn install_durable_set(
        &self,
        _instance: &EndpointInstanceBindingV1,
        _sealed: &crate::controlled_authority::formal::VerifiedSealedDurableSetTransitionV1,
        _request: DurableSetNativeInstallRequestV1,
    ) -> Result<Box<dyn MonotonicEndpointGuard>, FencingError> {
        Err(FencingError::FencingUnproven)
    }
    fn query_durable_set_install(
        &self,
        _instance: &EndpointInstanceBindingV1,
        _sealed: &crate::controlled_authority::formal::VerifiedSealedDurableSetTransitionV1,
    ) -> Result<Box<dyn MonotonicEndpointGuard>, FencingError> {
        Err(FencingError::FencingUnproven)
    }
    fn activate_durable_set(
        &self,
        _instance: &EndpointInstanceBindingV1,
        _sealed: &crate::controlled_authority::formal::VerifiedSealedDurableSetTransitionV1,
        _activation: &crate::controlled_authority::formal::VerifiedDurableSetActivationV1,
        _request: DurableSetNativeActivationRequestV1,
    ) -> Result<Box<dyn MonotonicEndpointGuard>, FencingError> {
        Err(FencingError::FencingUnproven)
    }
    /// The oracle independently authorizes this instance/authority registration,
    /// owns the nonexportable endpoint key, and retains exact signed metadata.
    fn initialize_protected(
        &self,
        _instance: &EndpointInstanceBindingV1,
        _registration: ProtectedEndpointRegistrationV1,
    ) -> Result<Box<dyn MonotonicEndpointGuard>, FencingError> {
        Err(FencingError::FencingUnproven)
    }
    fn acquire(
        &self,
        instance: &EndpointInstanceBindingV1,
        initial: &EndpointCheckpointBindingV1,
    ) -> Result<Box<dyn MonotonicEndpointGuard>, FencingError>;
}
pub fn epoch_seal_signing_bytes(body: &EpochSealBodyV1) -> Result<Vec<u8>, FencingError> {
    body.validate()?;
    bytes(SEAL_DOMAIN, body)
}
