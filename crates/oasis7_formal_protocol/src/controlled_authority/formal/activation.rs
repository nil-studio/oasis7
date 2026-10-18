use super::{crypto::*, *};
use crate::controlled_authority::replicated_protocol::ArtifactRole;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InitialActivationBodyV1 {
    pub schema_version: u32,
    pub authority: AuthorityDefinitionV1,
    pub recovery_policy: RecoveryPolicyV1,
    pub initial_state_root: String,
    pub runtime_manifest_digest: String,
    /// Governing Cognition manifest hash, distinct from the closure manifest blob.
    pub governing_manifest_digest: String,
    pub execution_policy_digest: String,
    /// Input is the certificate itself and is intentionally excluded here to
    /// avoid a certificate/record hash cycle.
    pub artifact_roots: BTreeMap<ArtifactRole, String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedInitialActivationV1 {
    pub body: InitialActivationBodyV1,
    pub signature_hex: String,
}
/// Trust is supplied by the deployment authorization, never by the envelope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InitialActivationTrustV1 {
    pub issuer_public_key: String,
    pub world: FormalWorldIdentityV1,
    pub initial_state_root: String,
    pub governing_manifest_digest: String,
    pub recovery_policy_digest: String,
}
/// Authorization only: this token does not prove a committed initial record,
/// a running writer, native fencing or readiness of a runtime.
#[derive(Debug, Clone)]
pub struct VerifiedAuthorityV1 {
    pub(crate) definition: AuthorityDefinitionV1,
    pub(crate) policy: RecoveryPolicyV1,
    pub(crate) activation_ref: String,
    pub(crate) durable_set_activation_digest: Option<String>,
    pub(crate) initial: Option<InitialActivationBodyV1>,
    pub(crate) first_normal_position: Option<u64>,
    pub(crate) recovery_cut: Option<FormalHeadRefV1>,
    pub(crate) recovery_retained_record: Option<String>,
    pub(crate) initial_input_digest: Option<String>,
}
impl VerifiedAuthorityV1 {
    /// Verified initial activation body retained with this authority lineage.
    /// Reading the signed payload does not grant authority or permit mutation.
    pub fn initial_body(&self) -> Option<&InitialActivationBodyV1> {
        self.initial.as_ref()
    }
    pub fn definition(&self) -> &AuthorityDefinitionV1 {
        &self.definition
    }
    pub fn policy(&self) -> &RecoveryPolicyV1 {
        &self.policy
    }
    pub fn activation_ref(&self) -> &str {
        &self.activation_ref
    }
    pub fn durable_set_activation_digest(&self) -> Option<&str> {
        self.durable_set_activation_digest.as_deref()
    }
    pub fn authority_id(&self) -> Result<String, FormalError> {
        self.definition.authority_id()
    }
    /// Commitment to the exact verified authorization window, not merely its
    /// reusable public authority definition ID.
    pub fn context_digest(&self) -> Result<String, FormalError> {
        let legacy = hash(
            "oasis7.formal.verified_authority_context.v1",
            &(
                &self.definition,
                &self.policy,
                &self.activation_ref,
                self.first_normal_position,
                &self.recovery_cut,
                &self.recovery_retained_record,
                &self.initial_input_digest,
            ),
        )?;
        match &self.durable_set_activation_digest {
            None => Ok(legacy),
            Some(digest) => hash(
                "oasis7.formal.verified_authority_context.v2",
                &(legacy, digest),
            ),
        }
    }
}
impl InitialActivationBodyV1 {
    pub fn validate(&self) -> Result<(), FormalError> {
        self.authority.verify_policy(&self.recovery_policy)?;
        if !matches!(self.schema_version, 1 | 2)
            || self.authority.epoch != 1
            || self.authority.valid_from_position != 1
            || self.authority.predecessor_authority_id.is_some()
            || self.artifact_roots.len() != if self.schema_version == 2 { 9 } else { 8 }
            || self
                .artifact_roots
                .contains_key(&ArtifactRole::NativeExecutionEnvironment)
                != (self.schema_version == 2)
            || self.artifact_roots.contains_key(&ArtifactRole::Input)
        {
            return Err(invalid("initial formal authorization boundary/roots"));
        }
        for digest in [
            &self.initial_state_root,
            &self.runtime_manifest_digest,
            &self.governing_manifest_digest,
            &self.execution_policy_digest,
        ] {
            hash_hex(digest)?;
        }
        for digest in self.artifact_roots.values() {
            hash_hex(digest)?;
        }
        if self.artifact_roots.get(&ArtifactRole::Snapshot) != Some(&self.initial_state_root)
            || self.artifact_roots.get(&ArtifactRole::ExecutionManifest)
                != Some(&self.runtime_manifest_digest)
        {
            return Err(invalid("initial formal state/manifest closure"));
        }
        Ok(())
    }
    pub fn activation_ref(&self) -> Result<String, FormalError> {
        self.validate()?;
        hash("oasis7.formal.initial_authorization.v1", self)
    }
}
pub fn initial_activation_signing_bytes(
    body: &InitialActivationBodyV1,
) -> Result<Vec<u8>, FormalError> {
    body.validate()?;
    bytes("oasis7.formal.initial_authorization.sign.v1", body)
}
pub fn initial_activation_input_bytes(
    certificate: &SignedInitialActivationV1,
) -> Result<Vec<u8>, FormalError> {
    certificate.body.validate()?;
    serde_cbor::to_vec(certificate).map_err(|error| invalid(error.to_string()))
}
pub fn verify_initial_activation(
    certificate: &SignedInitialActivationV1,
    trust: &InitialActivationTrustV1,
) -> Result<VerifiedAuthorityV1, FormalError> {
    let body = &certificate.body;
    public_key(&trust.issuer_public_key)?;
    body.validate()?;
    trust.world.validate()?;
    for value in [
        &trust.issuer_public_key,
        &trust.initial_state_root,
        &trust.governing_manifest_digest,
        &trust.recovery_policy_digest,
    ] {
        hash_hex(value)?;
    }
    if body.authority.world != trust.world
        || body.initial_state_root != trust.initial_state_root
        || body.governing_manifest_digest != trust.governing_manifest_digest
        || body.recovery_policy.digest()? != trust.recovery_policy_digest
    {
        return Err(invalid("initial formal external trust mismatch"));
    }
    verify(
        "oasis7.formal.initial_authorization.sign.v1",
        body,
        &certificate.signature_hex,
        &trust.issuer_public_key,
    )?;
    let input_digest = blake3::hash(&initial_activation_input_bytes(certificate)?)
        .to_hex()
        .to_string();
    Ok(VerifiedAuthorityV1 {
        definition: body.authority.clone(),
        policy: body.recovery_policy.clone(),
        activation_ref: body.activation_ref()?,
        durable_set_activation_digest: None,
        initial: Some(body.clone()),
        first_normal_position: Some(1),
        recovery_cut: None,
        recovery_retained_record: None,
        initial_input_digest: Some(input_digest),
    })
}
