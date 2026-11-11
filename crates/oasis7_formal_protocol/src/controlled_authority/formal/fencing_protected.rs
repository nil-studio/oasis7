//! Nonexportable endpoint signing under the SAME monotonic/exclusive lease.
//! Implementations must deny new mutation signatures after seal, and retain the
//! exact native metadata bound to their floor. Local file permissions are not
//! protected custody. These are independently pinned provider assertions.
use super::*;
pub const PROTECTED_SIGNATURE_ATTEST_DOMAIN: &str = "oasis7.formal.protected_endpoint_signature.v1";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProtectedEndpointSignaturePurposeV1 {
    NativeCheckpoint,
    FrozenReceipt,
    PrepareReceipt,
    FinalReceipt,
    ProofComplete,
    EpochSeal,
    RpcResponse,
}
impl ProtectedEndpointSignaturePurposeV1 {
    pub fn domain(&self) -> &'static str {
        match self {
            Self::NativeCheckpoint => "oasis7.formal.endpoint_checkpoint.sign.v1",
            Self::FrozenReceipt => "oasis7.formal.frozen_receipt.sign.v1",
            Self::PrepareReceipt => "oasis7.formal.prepare_receipt.sign.v1",
            Self::FinalReceipt => "oasis7.formal.final_receipt.sign.v1",
            Self::ProofComplete => "oasis7.formal.proof_complete.sign.v1",
            Self::EpochSeal => SEAL_DOMAIN,
            Self::RpcResponse => "oasis7.formal.endpoint.rpc.response.v1",
        }
    }
    pub fn mutation(&self) -> bool {
        !matches!(self, Self::EpochSeal | Self::RpcResponse)
    }
}
fn serialize_preimage<S: serde::Serializer>(
    bytes: &[u8],
    serializer: S,
) -> Result<S::Ok, S::Error> {
    serializer.serialize_bytes(bytes)
}
fn deserialize_preimage<'de, D: serde::Deserializer<'de>>(decoder: D) -> Result<Vec<u8>, D::Error> {
    struct Visitor;
    impl<'de> serde::de::Visitor<'de> for Visitor {
        type Value = Vec<u8>;
        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str("bounded signing preimage byte string")
        }
        fn visit_bytes<E: serde::de::Error>(self, bytes: &[u8]) -> Result<Vec<u8>, E> {
            if bytes.len() > 64 * 1024 {
                return Err(E::custom("protected signature preimage bound"));
            }
            Ok(bytes.to_vec())
        }
        fn visit_byte_buf<E: serde::de::Error>(self, bytes: Vec<u8>) -> Result<Vec<u8>, E> {
            if bytes.len() > 64 * 1024 {
                return Err(E::custom("protected signature preimage bound"));
            }
            Ok(bytes)
        }
    }
    decoder.deserialize_byte_buf(Visitor)
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProtectedEndpointSigningRequestV1 {
    pub schema_version: u32,
    pub instance_digest: String,
    pub checkpoint: EndpointCheckpointBindingV1,
    pub challenge: String,
    pub purpose: ProtectedEndpointSignaturePurposeV1,
    #[serde(
        serialize_with = "serialize_preimage",
        deserialize_with = "deserialize_preimage"
    )]
    pub preimage: Vec<u8>,
}
impl ProtectedEndpointSigningRequestV1 {
    pub fn validate(&self) -> Result<(), FencingError> {
        validate_protected_request_with(self, |domain| bytes(domain, &()), |_| Ok(()))
    }
    pub fn digest(&self) -> Result<String, FencingError> {
        self.validate()?;
        hash_value(self)
    }
}
pub fn validate_protected_request_with<B: AsRef<[u8]>, E: From<FencingError>>(
    request: &ProtectedEndpointSigningRequestV1,
    build_prefix: impl FnOnce(&str) -> Result<B, E>,
    mut work: impl FnMut(usize) -> Result<(), E>,
) -> Result<(), E> {
    work(request.instance_digest.len())?;
    canonical_hash(&request.instance_digest)?;
    work(request.challenge.len())?;
    canonical_hash(&request.challenge)?;
    work(request.checkpoint.commitment.len() + request.checkpoint.head.decision_hash.len())?;
    request.checkpoint.validate()?;
    let domain = request.purpose.domain();
    let prefix = build_prefix(domain)?;
    let prefix = prefix
        .as_ref()
        .get(..8 + domain.len())
        .ok_or(FencingError::FencingUnproven)?;
    work(prefix.len())?;
    if request.schema_version != 1
        || request.preimage.len() > 64 * 1024
        || !request.preimage.starts_with(prefix)
    {
        return Err(FencingError::FencingUnproven.into());
    }
    Ok(())
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProtectedSignerCustodyV1 {
    NonexportableLeaseBoundV1,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProtectedEndpointSignatureBodyV1 {
    pub schema_version: u32,
    pub custody: ProtectedSignerCustodyV1,
    pub instance_digest: String,
    pub resource_binding: String,
    pub checkpoint: EndpointCheckpointBindingV1,
    pub request_digest: String,
    pub challenge: String,
    pub endpoint_signature: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProtectedEndpointSignatureV1 {
    pub body: ProtectedEndpointSignatureBodyV1,
    pub provider_signature: String,
}
pub fn protected_endpoint_signature_signing_bytes(
    body: &ProtectedEndpointSignatureBodyV1,
) -> Result<Vec<u8>, FencingError> {
    bytes(PROTECTED_SIGNATURE_ATTEST_DOMAIN, body)
}
pub trait ProtectedSignatureValidationOpsV1 {
    type Error: From<FencingError>;
    fn validate_request(
        &mut self,
        request: &ProtectedEndpointSigningRequestV1,
    ) -> Result<(), Self::Error>;
    fn verify_attestation(
        &mut self,
        body: &ProtectedEndpointSignatureBodyV1,
        signature: &str,
        key: &str,
    ) -> Result<(), Self::Error>;
    fn instance_digest(
        &mut self,
        instance: &EndpointInstanceBindingV1,
    ) -> Result<String, Self::Error>;
    fn request_digest(
        &mut self,
        request: &ProtectedEndpointSigningRequestV1,
    ) -> Result<String, Self::Error>;
    fn authorized(
        &mut self,
        policy: &TrustedFencingPolicyV1,
        resource: &str,
    ) -> Result<bool, Self::Error>;
    fn compare(&mut self, bytes: usize) -> Result<(), Self::Error>;
    fn verify_endpoint(
        &mut self,
        key: &str,
        signature: &str,
        preimage: &[u8],
    ) -> Result<(), Self::Error>;
}
pub fn verify_endpoint_preimage_with<E: From<FencingError>>(
    public_key: &str,
    signature: &str,
    preimage: &[u8],
    mut work: impl FnMut(usize) -> Result<(), E>,
) -> Result<(), E> {
    work(public_key.len())?;
    let public: [u8; 32] = hex::decode(public_key)
        .map_err(|e| invalid(e.to_string()))?
        .try_into()
        .map_err(|_| invalid("protected signer key"))?;
    let key = VerifyingKey::from_bytes(&public).map_err(|e| invalid(e.to_string()))?;
    work(signature.len())?;
    let signature_bytes: [u8; 64] = hex::decode(signature)
        .map_err(|e| invalid(e.to_string()))?
        .try_into()
        .map_err(|_| invalid("protected endpoint signature"))?;
    work(preimage.len())?;
    key.verify_strict(preimage, &Signature::from_bytes(&signature_bytes))
        .map_err(|_| FencingError::FencingUnproven)?;
    Ok(())
}
pub fn verify_protected_endpoint_signature_with<'s, O: ProtectedSignatureValidationOpsV1>(
    signature: &'s ProtectedEndpointSignatureV1,
    request: &ProtectedEndpointSigningRequestV1,
    instance: &EndpointInstanceBindingV1,
    policy: &TrustedFencingPolicyV1,
    ops: &mut O,
) -> Result<&'s str, O::Error> {
    ops.validate_request(request)?;
    ops.verify_attestation(
        &signature.body,
        &signature.provider_signature,
        &policy.provider_public_key,
    )?;
    let body = &signature.body;
    ops.compare(
        body.instance_digest.len()
            + request.instance_digest.len()
            + body.challenge.len()
            + request.challenge.len()
            + body.checkpoint.commitment.len()
            + request.checkpoint.commitment.len()
            + body.checkpoint.head.decision_hash.len()
            + request.checkpoint.head.decision_hash.len()
            + body.request_digest.len(),
    )?;
    if body.schema_version != 1
        || body.custody != ProtectedSignerCustodyV1::NonexportableLeaseBoundV1
        || body.instance_digest != ops.instance_digest(instance)?
        || body.instance_digest != request.instance_digest
        || body.checkpoint != request.checkpoint
        || body.challenge != request.challenge
        || body.request_digest != ops.request_digest(request)?
        || !ops.authorized(policy, &body.resource_binding)?
    {
        return Err(FencingError::FencingUnproven.into());
    }
    ops.verify_endpoint(
        &instance.endpoint_public_key,
        &body.endpoint_signature,
        &request.preimage,
    )?;
    Ok(&body.endpoint_signature)
}
struct LegacyProtectedValidation;
impl ProtectedSignatureValidationOpsV1 for LegacyProtectedValidation {
    type Error = FencingError;
    fn validate_request(
        &mut self,
        r: &ProtectedEndpointSigningRequestV1,
    ) -> Result<(), FencingError> {
        r.validate()
    }
    fn verify_attestation(
        &mut self,
        b: &ProtectedEndpointSignatureBodyV1,
        s: &str,
        k: &str,
    ) -> Result<(), FencingError> {
        verify(PROTECTED_SIGNATURE_ATTEST_DOMAIN, b, s, k)
    }
    fn instance_digest(&mut self, i: &EndpointInstanceBindingV1) -> Result<String, FencingError> {
        i.digest()
    }
    fn request_digest(
        &mut self,
        r: &ProtectedEndpointSigningRequestV1,
    ) -> Result<String, FencingError> {
        r.digest()
    }
    fn authorized(&mut self, p: &TrustedFencingPolicyV1, r: &str) -> Result<bool, FencingError> {
        Ok(p.authorized_resource_bindings.iter().any(|v| v == r))
    }
    fn compare(&mut self, _: usize) -> Result<(), FencingError> {
        Ok(())
    }
    fn verify_endpoint(&mut self, k: &str, s: &str, p: &[u8]) -> Result<(), FencingError> {
        verify_endpoint_preimage_with(k, s, p, |_| Ok(()))
    }
}
pub fn verify_protected_endpoint_signature(
    signature: &ProtectedEndpointSignatureV1,
    request: &ProtectedEndpointSigningRequestV1,
    instance: &EndpointInstanceBindingV1,
    policy: &TrustedFencingPolicyV1,
) -> Result<String, FencingError> {
    verify_protected_endpoint_signature_with(
        signature,
        request,
        instance,
        policy,
        &mut LegacyProtectedValidation,
    )
    .map(str::to_owned)
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProtectedEndpointRegistrationV1 {
    pub authority_id: String,
    pub activation_ref: String,
    pub unsigned_checkpoint_preimage: Vec<u8>,
    pub existing_checkpoint: Option<Vec<u8>>,
}
