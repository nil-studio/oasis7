use super::super::{
    FormalError, FormalWorldIdentityV1, FullSyncIssuerAnchorV2, FullSyncIssuerTargetV2,
    FullSyncReadRoleV2, InitialActivationTrustV1, SignedFullSyncIssuerObservationV2,
    VerifiedFullSyncIssuerPolicyV2, canonical, digest, encode, invalid, validate_activation_trust,
    verify,
};
use super::witness::SignedFullSyncIssuerObservationV3;
use ed25519_dalek::{Signer, SigningKey};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub(super) const POLICY_DOMAIN_V3: &str = "oasis7.formal.full-sync-issuer.policy.v3";
pub(super) const TRUST_DOMAIN_V3: &str = "oasis7.formal.full-sync-issuer.activation-trust.v3";
pub(super) const MIGRATION_DOMAIN_V3: &str = "oasis7.formal.full-sync-issuer.migration.v3";
pub(super) const COMMAND_DOMAIN_V3: &str = "oasis7.formal.full-sync-issuer.command.v3";
pub(super) const QUERY_DOMAIN_V3: &str = "oasis7.formal.full-sync-issuer.query.v3";
pub(super) const OBSERVATION_DOMAIN_V3: &str = "oasis7.formal.full-sync-issuer.observation.v3";
pub(super) const RECEIPT_DOMAIN_V3: &str = "oasis7.formal.full-sync-issuer.receipt.v3";
pub(super) const LEASE_DOMAIN_V3: &str = "oasis7.formal.full-sync-issuer.lease.v3";
pub(super) const POLICY_DIGEST_DOMAIN_V3: &str = "oasis7.formal.full-sync-issuer.policy-digest.v3";
pub(super) const STATE_DOMAIN_V3: &str = "oasis7.formal.full-sync-issuer.state.v3";
pub(super) const LOG_LINK_DOMAIN_V3: &str = "oasis7.formal.full-sync-issuer.log-link.v3";
pub(super) const RECORD_DIGEST_DOMAIN_V3: &str = "oasis7.formal.full-sync-issuer.record.v3";
pub(super) const STORED_ENTRY_DOMAIN_V3: &str = "oasis7.formal.full-sync-issuer.stored-entry.v3";
pub const MAX_FULL_SYNC_ISSUER_LEDGER_RECORDS_V3: usize = 1_024;
pub const MAX_FULL_SYNC_PROVIDER_KEYS_V3: usize = 1_024;
pub const MAX_FULL_SYNC_PROVIDER_LEASE_MILLIS_V3: u64 = 60_000;
pub(super) const MAX_ENTRY_BYTES_V3: usize = 256 * 1024;
pub(super) const MAX_STORED_ENTRY_BYTES_V3: usize = 512 * 1024;
pub(super) const MAX_LEDGER_BYTES_V3: usize = 64 * 1024 * 1024;
pub(super) const MAX_GRANT_MILLIS_V3: u64 = 30 * 24 * 60 * 60 * 1000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FullSyncIssuerPolicyBodyV3 {
    pub schema_version: u32,
    pub root_public_key: String,
    pub activation_trust_digest: String,
    pub world: FormalWorldIdentityV1,
    pub authority_epoch: u64,
    pub scope_digest: String,
    pub allowed_roles: Vec<FullSyncReadRoleV2>,
    pub provider_public_keys: Vec<String>,
    pub management_public_key: String,
    pub lease_signer_public_key: String,
    pub witness_public_key: String,
    pub signer_generation: u64,
    pub max_authorization_staleness_ms: u64,
    pub max_clock_uncertainty_ms: u64,
    pub clock_profile_digest: String,
}

impl FullSyncIssuerPolicyBodyV3 {
    pub fn validate(&self) -> Result<(), FormalError> {
        self.world.validate()?;
        if self.schema_version != 3
            || self.authority_epoch == 0
            || self.signer_generation == 0
            || self.allowed_roles.is_empty()
            || self.allowed_roles.len() > 3
            || self.allowed_roles.windows(2).any(|pair| pair[0] >= pair[1])
            || self.provider_public_keys.len() > MAX_FULL_SYNC_PROVIDER_KEYS_V3
            || self
                .provider_public_keys
                .windows(2)
                .any(|pair| pair[0] >= pair[1])
        {
            return Err(invalid(
                "full-sync V3 policy version, roles or provider bounds",
            ));
        }
        if self.max_authorization_staleness_ms == 0
            || self.max_authorization_staleness_ms > MAX_FULL_SYNC_PROVIDER_LEASE_MILLIS_V3
            || self.max_clock_uncertainty_ms >= self.max_authorization_staleness_ms
        {
            return Err(invalid("full-sync V3 lease time bounds"));
        }
        for key in [
            &self.root_public_key,
            &self.management_public_key,
            &self.lease_signer_public_key,
            &self.witness_public_key,
            &self.activation_trust_digest,
            &self.scope_digest,
            &self.clock_profile_digest,
        ] {
            canonical::<32>(key)?;
        }
        for provider in &self.provider_public_keys {
            canonical::<32>(provider)?;
        }
        let distinct: BTreeSet<&String> = [
            &self.root_public_key,
            &self.management_public_key,
            &self.lease_signer_public_key,
            &self.witness_public_key,
        ]
        .into_iter()
        .collect();
        if distinct.len() != 4
            || self.provider_public_keys.iter().any(|provider| {
                provider == &self.root_public_key
                    || provider == &self.management_public_key
                    || provider == &self.lease_signer_public_key
                    || provider == &self.witness_public_key
            })
        {
            return Err(invalid("full-sync V3 keys must have distinct roles"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedFullSyncIssuerPolicyV3 {
    pub body: FullSyncIssuerPolicyBodyV3,
    pub signature_hex: String,
}

impl SignedFullSyncIssuerPolicyV3 {
    pub fn sign(body: FullSyncIssuerPolicyBodyV3, key: &SigningKey) -> Result<Self, FormalError> {
        body.validate()?;
        if body.root_public_key != hex::encode(key.verifying_key().as_bytes()) {
            return Err(invalid("full-sync V3 policy root signer mismatch"));
        }
        let signature_hex = hex::encode(
            key.sign(encode(POLICY_DOMAIN_V3, &body, MAX_ENTRY_BYTES_V3)?.as_slice())
                .to_bytes(),
        );
        Ok(Self {
            body,
            signature_hex,
        })
    }
}

/// Root-verified, deployment-pinned V3 issuer policy.
#[derive(Debug, Clone)]
pub struct VerifiedFullSyncIssuerPolicyV3 {
    signed: SignedFullSyncIssuerPolicyV3,
    trust: InitialActivationTrustV1,
    policy_digest: String,
    scope_digest: String,
}

impl VerifiedFullSyncIssuerPolicyV3 {
    pub fn verify(
        signed: &SignedFullSyncIssuerPolicyV3,
        trust: &InitialActivationTrustV1,
        current_v2_policy: &VerifiedFullSyncIssuerPolicyV2,
    ) -> Result<Self, FormalError> {
        let body = &signed.body;
        body.validate()?;
        validate_activation_trust(trust)?;
        if trust != current_v2_policy.trust()
            || body.root_public_key != trust.issuer_public_key
            || body.world != trust.world
            || body.activation_trust_digest != digest(TRUST_DOMAIN_V3, trust, MAX_ENTRY_BYTES_V3)?
            || body.world != current_v2_policy.body().world
            || body.authority_epoch != current_v2_policy.body().authority_epoch
            || body.scope_digest != current_v2_policy.scope_digest()
            || body.witness_public_key != current_v2_policy.body().witness_public_key
            || body.signer_generation <= current_v2_policy.body().signer_generation
            || body.lease_signer_public_key == current_v2_policy.body().issuer_public_key
        {
            return Err(invalid(
                "full-sync V3 policy changes V2 trust scope or signer fence",
            ));
        }
        verify(
            POLICY_DOMAIN_V3,
            body,
            &signed.signature_hex,
            &trust.issuer_public_key,
        )?;
        let policy_digest = digest(POLICY_DIGEST_DOMAIN_V3, body, MAX_ENTRY_BYTES_V3)?;
        Ok(Self {
            signed: signed.clone(),
            trust: trust.clone(),
            policy_digest,
            scope_digest: body.scope_digest.clone(),
        })
    }

    pub fn signed(&self) -> &SignedFullSyncIssuerPolicyV3 {
        &self.signed
    }
    pub fn body(&self) -> &FullSyncIssuerPolicyBodyV3 {
        &self.signed.body
    }
    pub fn policy_digest(&self) -> &str {
        &self.policy_digest
    }
    pub fn scope_digest(&self) -> &str {
        &self.scope_digest
    }
    pub fn trust(&self) -> &InitialActivationTrustV1 {
        &self.trust
    }
}

/// Digest the complete deployment-pinned activation trust for the V3 domain.
pub fn full_sync_activation_trust_digest_v3(
    trust: &InitialActivationTrustV1,
) -> Result<String, FormalError> {
    validate_activation_trust(trust)?;
    digest(TRUST_DOMAIN_V3, trust, MAX_ENTRY_BYTES_V3)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FullSyncIssuerAnchorV3 {
    pub schema_version: u32,
    pub revision: u64,
    pub scope_digest: String,
    pub log_digest: String,
    pub state_digest: String,
    pub policy_digest: String,
    pub signer_generation: u64,
}

impl FullSyncIssuerAnchorV3 {
    pub fn validate(&self) -> Result<(), FormalError> {
        if self.schema_version != 3 || self.revision == 0 || self.signer_generation == 0 {
            return Err(invalid("full-sync V3 anchor version/revision/generation"));
        }
        for value in [
            &self.scope_digest,
            &self.log_digest,
            &self.state_digest,
            &self.policy_digest,
        ] {
            canonical::<32>(value)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FullSyncIssuerMigrationBodyV3 {
    pub schema_version: u32,
    pub previous_v2_policy_digest: String,
    pub previous_v2_signer_generation: u64,
    pub expected_v2_head: FullSyncIssuerAnchorV2,
    pub new_policy: SignedFullSyncIssuerPolicyV3,
    pub request_id: String,
    pub manager_nonce: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedFullSyncIssuerMigrationV3 {
    pub body: FullSyncIssuerMigrationBodyV3,
    pub signature_hex: String,
}

impl SignedFullSyncIssuerMigrationV3 {
    pub fn sign(
        body: FullSyncIssuerMigrationBodyV3,
        key: &SigningKey,
    ) -> Result<Self, FormalError> {
        validate_migration_body(&body)?;
        let signature_hex = hex::encode(
            key.sign(&encode(MIGRATION_DOMAIN_V3, &body, MAX_ENTRY_BYTES_V3)?)
                .to_bytes(),
        );
        Ok(Self {
            body,
            signature_hex,
        })
    }

    pub fn verify(
        &self,
        old_policy: &VerifiedFullSyncIssuerPolicyV2,
        new_policy: &VerifiedFullSyncIssuerPolicyV3,
    ) -> Result<(), FormalError> {
        validate_migration_body(&self.body)?;
        if self.body.previous_v2_policy_digest != old_policy.policy_digest()
            || self.body.previous_v2_signer_generation != old_policy.body().signer_generation
            || self.body.new_policy != *new_policy.signed()
            || self.body.expected_v2_head.scope_digest != old_policy.scope_digest()
        {
            return Err(invalid("full-sync V3 migration policy/head binding"));
        }
        verify(
            MIGRATION_DOMAIN_V3,
            &self.body,
            &self.signature_hex,
            &old_policy.body().management_public_key,
        )
    }
}

pub(super) fn validate_migration_body(
    body: &FullSyncIssuerMigrationBodyV3,
) -> Result<(), FormalError> {
    body.expected_v2_head.validate()?;
    if body.schema_version != 3 || body.previous_v2_signer_generation == 0 {
        return Err(invalid("full-sync V3 migration version/generation"));
    }
    for value in [
        &body.previous_v2_policy_digest,
        &body.request_id,
        &body.manager_nonce,
    ] {
        canonical::<32>(value)?;
    }
    if body.request_id.bytes().all(|byte| byte == b'0')
        || body.manager_nonce.bytes().all(|byte| byte == b'0')
    {
        return Err(invalid("full-sync V3 migration identity"));
    }
    body.new_policy.body.validate()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum FullSyncIssuerPreviousAnchorV3 {
    V2(FullSyncIssuerAnchorV2),
    V3(FullSyncIssuerAnchorV3),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum FullSyncIssuerOperationV3 {
    Issue {
        not_before_unix_ms: u64,
        expires_unix_ms: u64,
    },
    Replace {
        not_before_unix_ms: u64,
        expires_unix_ms: u64,
    },
    Revoke,
}

impl FullSyncIssuerOperationV3 {
    pub(super) fn validate(&self) -> Result<(), FormalError> {
        let bounds = match self {
            Self::Issue {
                not_before_unix_ms,
                expires_unix_ms,
            }
            | Self::Replace {
                not_before_unix_ms,
                expires_unix_ms,
            } => Some((*not_before_unix_ms, *expires_unix_ms)),
            Self::Revoke => None,
        };
        if let Some((not_before, expires)) = bounds {
            let duration = expires
                .checked_sub(not_before)
                .ok_or_else(|| invalid("full-sync V3 grant expiry precedes not-before"))?;
            if duration == 0 || duration > MAX_GRANT_MILLIS_V3 {
                return Err(invalid("full-sync V3 grant duration bound"));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FullSyncIssuerCommandBodyV3 {
    pub schema_version: u32,
    pub world: FormalWorldIdentityV1,
    pub authority_epoch: u64,
    pub scope_digest: String,
    pub policy_digest: String,
    pub signer_generation: u64,
    pub target: FullSyncIssuerTargetV2,
    pub operation: FullSyncIssuerOperationV3,
    pub expected_head: FullSyncIssuerAnchorV3,
    pub expected_target_generation: u64,
    pub request_id: String,
    pub command_nonce: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedFullSyncIssuerCommandV3 {
    pub body: FullSyncIssuerCommandBodyV3,
    pub signature_hex: String,
}

impl SignedFullSyncIssuerCommandV3 {
    pub fn sign(body: FullSyncIssuerCommandBodyV3, key: &SigningKey) -> Result<Self, FormalError> {
        validate_command_body(&body)?;
        let signature_hex = hex::encode(
            key.sign(&encode(COMMAND_DOMAIN_V3, &body, MAX_ENTRY_BYTES_V3)?)
                .to_bytes(),
        );
        Ok(Self {
            body,
            signature_hex,
        })
    }

    pub fn verify(&self, policy: &VerifiedFullSyncIssuerPolicyV3) -> Result<(), FormalError> {
        validate_command_body(&self.body)?;
        if self.body.world != policy.body().world
            || self.body.authority_epoch != policy.body().authority_epoch
            || self.body.scope_digest != policy.scope_digest()
            || self.body.policy_digest != policy.policy_digest()
            || self.body.signer_generation != policy.body().signer_generation
        {
            return Err(invalid("full-sync V3 management policy binding"));
        }
        verify(
            COMMAND_DOMAIN_V3,
            &self.body,
            &self.signature_hex,
            &policy.body().management_public_key,
        )
    }
}

pub(super) fn validate_command_body(body: &FullSyncIssuerCommandBodyV3) -> Result<(), FormalError> {
    body.world.validate()?;
    body.target.validate()?;
    body.expected_head.validate()?;
    if body.schema_version != 3 || body.authority_epoch == 0 || body.signer_generation == 0 {
        return Err(invalid("full-sync V3 command version/scope"));
    }
    for value in [
        &body.scope_digest,
        &body.policy_digest,
        &body.request_id,
        &body.command_nonce,
    ] {
        canonical::<32>(value)?;
    }
    if body.request_id.bytes().all(|byte| byte == b'0')
        || body.command_nonce.bytes().all(|byte| byte == b'0')
    {
        return Err(invalid("full-sync V3 command identity"));
    }
    body.operation.validate()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FullSyncProviderLeaseRequestBodyV3 {
    pub schema_version: u32,
    pub world: FormalWorldIdentityV1,
    pub authority_epoch: u64,
    pub scope_digest: String,
    pub policy_digest: String,
    pub signer_generation: u64,
    pub provider_public_key: String,
    pub provider_incarnation: String,
    pub lease_id: String,
    pub request_id: String,
    pub request_nonce: String,
    pub expected_head: FullSyncIssuerAnchorV3,
    pub target: FullSyncIssuerTargetV2,
    pub target_generation: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedFullSyncProviderLeaseRequestV3 {
    pub body: FullSyncProviderLeaseRequestBodyV3,
    pub signature_hex: String,
}

impl SignedFullSyncProviderLeaseRequestV3 {
    pub fn sign(
        body: FullSyncProviderLeaseRequestBodyV3,
        key: &SigningKey,
    ) -> Result<Self, FormalError> {
        validate_lease_request_body(&body)?;
        let signature_hex = hex::encode(
            key.sign(&encode(LEASE_REQUEST_DOMAIN_V3, &body, MAX_ENTRY_BYTES_V3)?)
                .to_bytes(),
        );
        Ok(Self {
            body,
            signature_hex,
        })
    }

    pub fn verify(&self, policy: &VerifiedFullSyncIssuerPolicyV3) -> Result<(), FormalError> {
        validate_lease_request_body(&self.body)?;
        let body = &self.body;
        if body.world != policy.body().world
            || body.authority_epoch != policy.body().authority_epoch
            || body.scope_digest != policy.scope_digest()
            || body.policy_digest != policy.policy_digest()
            || body.signer_generation != policy.body().signer_generation
            || !policy
                .body()
                .provider_public_keys
                .contains(&body.provider_public_key)
            || !policy.body().allowed_roles.contains(&body.target.role)
        {
            return Err(invalid("full-sync V3 provider request policy binding"));
        }
        verify(
            LEASE_REQUEST_DOMAIN_V3,
            body,
            &self.signature_hex,
            &body.provider_public_key,
        )
    }
}

pub(super) const LEASE_REQUEST_DOMAIN_V3: &str =
    "oasis7.formal.full-sync-issuer.provider-lease-request.v3";

pub(super) fn validate_lease_request_body(
    body: &FullSyncProviderLeaseRequestBodyV3,
) -> Result<(), FormalError> {
    body.world.validate()?;
    body.target.validate()?;
    body.expected_head.validate()?;
    if body.schema_version != 3 || body.authority_epoch == 0 || body.signer_generation == 0 {
        return Err(invalid("full-sync V3 lease request version/scope"));
    }
    for value in [
        &body.scope_digest,
        &body.policy_digest,
        &body.provider_public_key,
        &body.provider_incarnation,
        &body.lease_id,
        &body.request_id,
        &body.request_nonce,
    ] {
        canonical::<32>(value)?;
    }
    if [
        &body.provider_incarnation,
        &body.lease_id,
        &body.request_id,
        &body.request_nonce,
    ]
    .into_iter()
    .any(|value| value.bytes().all(|byte| byte == b'0'))
    {
        return Err(invalid("full-sync V3 lease request identity"));
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FullSyncProviderLeaseBodyV3 {
    pub schema_version: u32,
    pub world: FormalWorldIdentityV1,
    pub authority_epoch: u64,
    pub scope_digest: String,
    pub policy_digest: String,
    pub signer_generation: u64,
    pub provider_public_key: String,
    pub provider_incarnation: String,
    pub lease_id: String,
    pub request_id: String,
    pub request_nonce: String,
    pub target: FullSyncIssuerTargetV2,
    pub target_generation: u64,
    pub expected_head: FullSyncIssuerAnchorV3,
    pub registry_state_digest: String,
    pub decision_revision: u64,
    pub clock_profile_digest: String,
    pub not_before_unix_ms: u64,
    pub expires_unix_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedFullSyncProviderLeaseV3 {
    pub body: FullSyncProviderLeaseBodyV3,
    pub signature_hex: String,
}

impl SignedFullSyncProviderLeaseV3 {
    pub(super) fn sign(
        body: FullSyncProviderLeaseBodyV3,
        policy: &VerifiedFullSyncIssuerPolicyV3,
        key: &SigningKey,
    ) -> Result<Self, FormalError> {
        validate_provider_lease_body(&body)?;
        if hex::encode(key.verifying_key().as_bytes()) != policy.body().lease_signer_public_key
            || body.policy_digest != policy.policy_digest()
            || body.scope_digest != policy.scope_digest()
        {
            return Err(invalid("full-sync V3 lease signer mismatch"));
        }
        let signature_hex = hex::encode(
            key.sign(&encode(LEASE_DOMAIN_V3, &body, MAX_ENTRY_BYTES_V3)?)
                .to_bytes(),
        );
        Ok(Self {
            body,
            signature_hex,
        })
    }

    /// Verify only the issuer's signature and static policy binding. This does
    /// not prove a witness CAS/receipt or authorize a provider to serve data.
    pub fn verify_issuer_signature(
        &self,
        policy: &VerifiedFullSyncIssuerPolicyV3,
    ) -> Result<(), FormalError> {
        validate_provider_lease_body(&self.body)?;
        let body = &self.body;
        if body.world != policy.body().world
            || body.authority_epoch != policy.body().authority_epoch
            || body.scope_digest != policy.scope_digest()
            || body.policy_digest != policy.policy_digest()
            || body.signer_generation != policy.body().signer_generation
            || body.clock_profile_digest != policy.body().clock_profile_digest
            || !policy
                .body()
                .provider_public_keys
                .contains(&body.provider_public_key)
        {
            return Err(invalid("full-sync V3 lease policy binding"));
        }
        verify(
            LEASE_DOMAIN_V3,
            body,
            &self.signature_hex,
            &policy.body().lease_signer_public_key,
        )
    }
}

fn validate_provider_lease_body(body: &FullSyncProviderLeaseBodyV3) -> Result<(), FormalError> {
    body.world.validate()?;
    body.target.validate()?;
    body.expected_head.validate()?;
    for value in [
        &body.scope_digest,
        &body.policy_digest,
        &body.provider_public_key,
        &body.provider_incarnation,
        &body.lease_id,
        &body.request_id,
        &body.request_nonce,
        &body.registry_state_digest,
        &body.clock_profile_digest,
    ] {
        canonical::<32>(value)?;
    }
    if body.schema_version != 3
        || body.authority_epoch == 0
        || body.signer_generation == 0
        || body.decision_revision != body.expected_head.revision
        || body.not_before_unix_ms >= body.expires_unix_ms
        || body.expires_unix_ms.saturating_sub(body.not_before_unix_ms)
            > MAX_FULL_SYNC_PROVIDER_LEASE_MILLIS_V3
        || body.request_id.bytes().all(|byte| byte == b'0')
        || body.request_nonce.bytes().all(|byte| byte == b'0')
        || body.provider_incarnation.bytes().all(|byte| byte == b'0')
        || body.lease_id.bytes().all(|byte| byte == b'0')
    {
        return Err(invalid("full-sync V3 provider lease fields/bounds"));
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FullSyncIssuerProtectedTimeIntervalV3 {
    pub earliest_unix_ms: u64,
    pub latest_unix_ms: u64,
    pub clock_profile_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FullSyncIssuerTargetLeaseV3 {
    pub provider_public_key: String,
    pub provider_incarnation: String,
    pub lease_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FullSyncProviderLeaseStateV3 {
    pub request: SignedFullSyncProviderLeaseRequestV3,
    pub lease: SignedFullSyncProviderLeaseV3,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FullSyncIssuerTargetStateV3 {
    pub target: FullSyncIssuerTargetV2,
    pub generation: u64,
    pub active: bool,
    pub not_before_unix_ms: Option<u64>,
    pub expires_unix_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum FullSyncIssuerEventV3 {
    Migration(Box<SignedFullSyncIssuerMigrationV3>),
    Management(Box<SignedFullSyncIssuerCommandV3>),
    Lease {
        request: Box<SignedFullSyncProviderLeaseRequestV3>,
        lease: Box<SignedFullSyncProviderLeaseV3>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FullSyncIssuerRecordV3 {
    pub schema_version: u32,
    pub policy: SignedFullSyncIssuerPolicyV3,
    pub previous_anchor: FullSyncIssuerPreviousAnchorV3,
    pub event: FullSyncIssuerEventV3,
    pub next_anchor: FullSyncIssuerAnchorV3,
}

impl FullSyncIssuerRecordV3 {
    pub fn digest(&self) -> Result<String, FormalError> {
        digest(RECORD_DIGEST_DOMAIN_V3, self, MAX_ENTRY_BYTES_V3)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FullSyncIssuerCommittedEntryV3 {
    pub record: FullSyncIssuerRecordV3,
    pub receipt: SignedFullSyncIssuerReceiptV3,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FullSyncIssuerReceiptBodyV3 {
    pub schema_version: u32,
    pub observation: FullSyncIssuerReceiptObservationV3,
    pub record_digest: String,
    pub request_id: String,
    pub previous_anchor: FullSyncIssuerPreviousAnchorV3,
    pub next_anchor: FullSyncIssuerAnchorV3,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum FullSyncIssuerReceiptObservationV3 {
    Migration(SignedFullSyncIssuerObservationV2),
    V3(SignedFullSyncIssuerObservationV3),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedFullSyncIssuerReceiptV3 {
    pub body: FullSyncIssuerReceiptBodyV3,
    pub signature_hex: String,
}

impl SignedFullSyncIssuerReceiptV3 {
    pub fn sign(
        observation: FullSyncIssuerReceiptObservationV3,
        record: &FullSyncIssuerRecordV3,
        key: &SigningKey,
    ) -> Result<Self, FormalError> {
        if hex::encode(key.verifying_key().as_bytes()) != record.policy.body.witness_public_key {
            return Err(invalid("full-sync V3 receipt witness signer mismatch"));
        }
        let request_id = match &record.event {
            FullSyncIssuerEventV3::Migration(migration) => &migration.body.request_id,
            FullSyncIssuerEventV3::Management(command) => &command.body.request_id,
            FullSyncIssuerEventV3::Lease { request, .. } => &request.body.request_id,
        };
        let body = FullSyncIssuerReceiptBodyV3 {
            schema_version: 3,
            observation,
            record_digest: record.digest()?,
            request_id: request_id.clone(),
            previous_anchor: record.previous_anchor.clone(),
            next_anchor: record.next_anchor.clone(),
        };
        let signature_hex = hex::encode(
            key.sign(&encode(RECEIPT_DOMAIN_V3, &body, MAX_ENTRY_BYTES_V3)?)
                .to_bytes(),
        );
        Ok(Self {
            body,
            signature_hex,
        })
    }

    pub fn verify_for_record(
        &self,
        record: &FullSyncIssuerRecordV3,
        old_policy: &VerifiedFullSyncIssuerPolicyV2,
        new_policy: &VerifiedFullSyncIssuerPolicyV3,
    ) -> Result<(), FormalError> {
        let body = &self.body;
        let expected_observation_head = match &body.observation {
            FullSyncIssuerReceiptObservationV3::Migration(observation) => {
                observation.verify(old_policy)?;
                if !matches!(&record.event, FullSyncIssuerEventV3::Migration(_)) {
                    return Err(invalid("full-sync V3 migration receipt for non-migration"));
                }
                observation.body.current_anchor == record.previous_anchor_v2()?
                    && observation.body.query.body.minimum_anchor == record.previous_anchor_v2()?
            }
            FullSyncIssuerReceiptObservationV3::V3(observation) => {
                observation.verify(new_policy)?;
                observation.body.current_anchor == record.previous_anchor_v3()?
                    && observation.body.query.body.minimum_anchor == record.previous_anchor_v3()?
            }
        };
        if !expected_observation_head
            || body.schema_version != 3
            || body.record_digest != record.digest()?
            || body.request_id != record.request_id()
            || body.previous_anchor != record.previous_anchor
            || body.next_anchor != record.next_anchor
        {
            return Err(invalid("full-sync V3 witness receipt record/CAS binding"));
        }
        verify(
            RECEIPT_DOMAIN_V3,
            body,
            &self.signature_hex,
            &new_policy.body().witness_public_key,
        )
    }
}

impl FullSyncIssuerRecordV3 {
    pub fn request_id(&self) -> &str {
        match &self.event {
            FullSyncIssuerEventV3::Migration(migration) => &migration.body.request_id,
            FullSyncIssuerEventV3::Management(command) => &command.body.request_id,
            FullSyncIssuerEventV3::Lease { request, .. } => &request.body.request_id,
        }
    }

    pub fn previous_anchor_v2(&self) -> Result<FullSyncIssuerAnchorV2, FormalError> {
        match &self.previous_anchor {
            FullSyncIssuerPreviousAnchorV3::V2(anchor) => Ok(anchor.clone()),
            FullSyncIssuerPreviousAnchorV3::V3(_) => {
                Err(invalid("full-sync V3 record is not at migration boundary"))
            }
        }
    }

    pub fn previous_anchor_v3(&self) -> Result<FullSyncIssuerAnchorV3, FormalError> {
        match &self.previous_anchor {
            FullSyncIssuerPreviousAnchorV3::V3(anchor) => Ok(anchor.clone()),
            FullSyncIssuerPreviousAnchorV3::V2(_) => {
                Err(invalid("full-sync migration record has V2 predecessor"))
            }
        }
    }
}
