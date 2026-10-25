//! Versioned full-sync grant management decisions.
//!
//! This module defines a deterministic, root-authorized management ledger and
//! the contract a separately protected witness must implement. It does not
//! authorize a provider to serve data, and no production witness implementation
//! is implied by the protocol types below.
use super::{
    FormalError, FormalWorldIdentityV1, InitialActivationTrustV1,
    issuer::{canonical, invalid, verify},
};
use ed25519_dalek::{Signer, SigningKey};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

const POLICY_DOMAIN: &str = "oasis7.formal.full-sync-issuer.policy.v2";
const ACTIVATION_TRUST_DOMAIN: &str = "oasis7.formal.full-sync-issuer.activation-trust.v2";
const COMMAND_DOMAIN: &str = "oasis7.formal.full-sync-issuer.command.v2";
const QUERY_DOMAIN: &str = "oasis7.formal.full-sync-issuer.query.v2";
const OBSERVATION_DOMAIN: &str = "oasis7.formal.full-sync-issuer.observation.v2";
const RECEIPT_DOMAIN: &str = "oasis7.formal.full-sync-issuer.receipt.v2";
const POLICY_DIGEST_DOMAIN: &str = "oasis7.formal.full-sync-issuer.policy-digest.v2";
const SCOPE_DOMAIN: &str = "oasis7.formal.full-sync-issuer.scope.v2";
const STATE_DOMAIN: &str = "oasis7.formal.full-sync-issuer.state.v2";
const GENESIS_DOMAIN: &str = "oasis7.formal.full-sync-issuer.genesis.v2";
const LOG_LINK_DOMAIN: &str = "oasis7.formal.full-sync-issuer.log-link.v2";
const RECORD_DIGEST_DOMAIN: &str = "oasis7.formal.full-sync-issuer.record.v2";
const STORED_ENTRY_DOMAIN: &str = "oasis7.formal.full-sync-issuer.stored-entry.v2";
const MAX_ENTRY_BYTES: usize = 256 * 1024;
const MAX_STORED_ENTRY_BYTES: usize = 512 * 1024;
const MAX_RECEIPT_RESERVE_BYTES: usize = 64 * 1024;
const MAX_LEDGER_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_FULL_SYNC_ISSUER_LEDGER_RECORDS_V2: usize = 1_024;
const MAX_LEDGER_RECORDS: usize = MAX_FULL_SYNC_ISSUER_LEDGER_RECORDS_V2;
const MAX_TARGETS: usize = 1_024;
const MAX_GRANT_MILLIS: u64 = 30 * 24 * 60 * 60 * 1000;

/// Read roles understood by this V2 management ledger.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum FullSyncReadRoleV2 {
    StateSync,
    FullReadOnly,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FullSyncIssuerPolicyBodyV2 {
    pub schema_version: u32,
    pub root_public_key: String,
    pub activation_trust_digest: String,
    pub world: FormalWorldIdentityV1,
    pub authority_epoch: u64,
    pub allowed_roles: Vec<FullSyncReadRoleV2>,
    pub management_public_key: String,
    pub issuer_public_key: String,
    pub witness_public_key: String,
    pub signer_generation: u64,
}

impl FullSyncIssuerPolicyBodyV2 {
    fn validate(&self) -> Result<(), FormalError> {
        self.world.validate()?;
        if self.schema_version != 2 || self.authority_epoch == 0 || self.signer_generation == 0 {
            return Err(invalid("full-sync issuer policy version/epoch/generation"));
        }
        for key in [
            &self.root_public_key,
            &self.management_public_key,
            &self.issuer_public_key,
            &self.witness_public_key,
        ] {
            canonical::<32>(key)?;
        }
        canonical::<32>(&self.activation_trust_digest)?;
        let distinct: BTreeSet<&String> = [
            &self.root_public_key,
            &self.management_public_key,
            &self.issuer_public_key,
            &self.witness_public_key,
        ]
        .into_iter()
        .collect();
        if distinct.len() != 4 {
            return Err(invalid(
                "full-sync root, manager, issuer and witness keys must be distinct",
            ));
        }
        if self.allowed_roles.is_empty()
            || self.allowed_roles.len() > 2
            || self.allowed_roles.windows(2).any(|pair| pair[0] >= pair[1])
        {
            return Err(invalid(
                "full-sync issuer roles must be sorted unique and nonempty",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedFullSyncIssuerPolicyV2 {
    pub body: FullSyncIssuerPolicyBodyV2,
    pub signature_hex: String,
}

impl SignedFullSyncIssuerPolicyV2 {
    pub fn sign(body: FullSyncIssuerPolicyBodyV2, key: &SigningKey) -> Result<Self, FormalError> {
        body.validate()?;
        if body.root_public_key != hex::encode(key.verifying_key().as_bytes()) {
            return Err(invalid("full-sync policy root signer mismatch"));
        }
        let signature_hex = hex::encode(
            key.sign(&encode(POLICY_DOMAIN, &body, MAX_ENTRY_BYTES)?)
                .to_bytes(),
        );
        Ok(Self {
            body,
            signature_hex,
        })
    }
}

/// A root-verified policy. Construction requires deployment-supplied trust;
/// the policy envelope cannot nominate its own root.
#[derive(Debug, Clone)]
pub struct VerifiedFullSyncIssuerPolicyV2 {
    signed: SignedFullSyncIssuerPolicyV2,
    trust: InitialActivationTrustV1,
    policy_digest: String,
    scope_digest: String,
}

impl VerifiedFullSyncIssuerPolicyV2 {
    pub fn verify(
        signed: &SignedFullSyncIssuerPolicyV2,
        trust: &InitialActivationTrustV1,
    ) -> Result<Self, FormalError> {
        signed.body.validate()?;
        validate_activation_trust(trust)?;
        if signed.body.root_public_key != trust.issuer_public_key
            || signed.body.world != trust.world
            || signed.body.activation_trust_digest
                != digest(ACTIVATION_TRUST_DOMAIN, trust, MAX_ENTRY_BYTES)?
        {
            return Err(invalid(
                "full-sync policy does not match external root/world trust",
            ));
        }
        verify(
            POLICY_DOMAIN,
            &signed.body,
            &signed.signature_hex,
            &trust.issuer_public_key,
        )?;
        let policy_digest = digest(POLICY_DIGEST_DOMAIN, &signed.body, MAX_ENTRY_BYTES)?;
        let scope_digest = scope_digest(&signed.body)?;
        Ok(Self {
            signed: signed.clone(),
            trust: trust.clone(),
            policy_digest,
            scope_digest,
        })
    }

    pub fn signed(&self) -> &SignedFullSyncIssuerPolicyV2 {
        &self.signed
    }
    pub fn body(&self) -> &FullSyncIssuerPolicyBodyV2 {
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

/// Digest the complete deployment-supplied root trust object for policy
/// construction. This does not derive or replace the external trust input.
pub fn full_sync_activation_trust_digest_v2(
    trust: &InitialActivationTrustV1,
) -> Result<String, FormalError> {
    validate_activation_trust(trust)?;
    digest(ACTIVATION_TRUST_DOMAIN, trust, MAX_ENTRY_BYTES)
}

fn validate_activation_trust(trust: &InitialActivationTrustV1) -> Result<(), FormalError> {
    trust.world.validate()?;
    for value in [
        &trust.issuer_public_key,
        &trust.initial_state_root,
        &trust.governing_manifest_digest,
        &trust.recovery_policy_digest,
    ] {
        canonical::<32>(value)?;
    }
    Ok(())
}

/// Immutable issuer-key generation transition, carried in the same CAS record
/// as the first decision made under the new root-signed policy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FullSyncIssuerPolicyTransitionV2 {
    pub previous_policy_digest: String,
    pub previous_signer_generation: u64,
    pub new_policy_digest: String,
    pub new_signer_generation: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FullSyncIssuerTargetV2 {
    pub node_public_key: String,
    pub role: FullSyncReadRoleV2,
}

impl FullSyncIssuerTargetV2 {
    fn validate(&self) -> Result<(), FormalError> {
        canonical::<32>(&self.node_public_key).map(|_| ())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum FullSyncIssuerOperationV2 {
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

impl FullSyncIssuerOperationV2 {
    fn validate(&self) -> Result<(), FormalError> {
        let times = match self {
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
        if let Some((not_before, expires)) = times {
            let duration = expires
                .checked_sub(not_before)
                .ok_or_else(|| invalid("full-sync grant expiry precedes not-before"))?;
            if duration == 0 || duration > MAX_GRANT_MILLIS {
                return Err(invalid("full-sync grant duration bound"));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FullSyncIssuerAnchorV2 {
    pub schema_version: u32,
    pub revision: u64,
    pub scope_digest: String,
    pub log_digest: String,
    pub state_digest: String,
    pub policy_digest: Option<String>,
    pub signer_generation: u64,
}

impl FullSyncIssuerAnchorV2 {
    pub fn validate(&self) -> Result<(), FormalError> {
        if self.schema_version != 2 {
            return Err(invalid("full-sync issuer anchor version"));
        }
        for value in [&self.scope_digest, &self.log_digest, &self.state_digest] {
            canonical::<32>(value)?;
        }
        if let Some(digest) = &self.policy_digest {
            canonical::<32>(digest)?;
        }
        if (self.revision == 0) != self.policy_digest.is_none()
            || (self.revision == 0) != (self.signer_generation == 0)
        {
            return Err(invalid("full-sync issuer anchor genesis policy/generation"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FullSyncIssuerCommandBodyV2 {
    pub schema_version: u32,
    pub world: FormalWorldIdentityV1,
    pub authority_epoch: u64,
    pub scope_digest: String,
    pub policy_digest: String,
    pub signer_generation: u64,
    pub transition: Option<FullSyncIssuerPolicyTransitionV2>,
    pub target: FullSyncIssuerTargetV2,
    pub operation: FullSyncIssuerOperationV2,
    pub expected_head: FullSyncIssuerAnchorV2,
    /// Zero denotes a target not yet present in the log.
    pub expected_target_generation: u64,
    /// Caller-generated unpredictable 256-bit idempotency identity.
    pub request_id: String,
    /// Stable manager anti-replay nonce. Retries keep the same signed command.
    pub command_nonce: String,
}

impl FullSyncIssuerCommandBodyV2 {
    fn validate(&self) -> Result<(), FormalError> {
        self.world.validate()?;
        self.target.validate()?;
        self.expected_head.validate()?;
        for value in [
            &self.scope_digest,
            &self.policy_digest,
            &self.request_id,
            &self.command_nonce,
        ] {
            canonical::<32>(value)?;
        }
        if self.schema_version != 2
            || self.authority_epoch == 0
            || self.signer_generation == 0
            || self.request_id.bytes().all(|byte| byte == b'0')
            || self.command_nonce.bytes().all(|byte| byte == b'0')
        {
            return Err(invalid("full-sync management command identity/version"));
        }
        if let Some(transition) = &self.transition {
            canonical::<32>(&transition.previous_policy_digest)?;
            canonical::<32>(&transition.new_policy_digest)?;
            if transition.previous_signer_generation == 0 || transition.new_signer_generation == 0 {
                return Err(invalid("full-sync policy transition generation"));
            }
        }
        self.operation.validate()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedFullSyncIssuerCommandV2 {
    pub body: FullSyncIssuerCommandBodyV2,
    pub signature_hex: String,
}

impl SignedFullSyncIssuerCommandV2 {
    pub fn sign(body: FullSyncIssuerCommandBodyV2, key: &SigningKey) -> Result<Self, FormalError> {
        body.validate()?;
        let signature_hex = hex::encode(
            key.sign(&encode(COMMAND_DOMAIN, &body, MAX_ENTRY_BYTES)?)
                .to_bytes(),
        );
        Ok(Self {
            body,
            signature_hex,
        })
    }

    pub fn verify(&self, policy: &VerifiedFullSyncIssuerPolicyV2) -> Result<(), FormalError> {
        self.body.validate()?;
        if self.body.world != policy.body().world
            || self.body.authority_epoch != policy.body().authority_epoch
            || self.body.scope_digest != policy.scope_digest
            || self.body.policy_digest != policy.policy_digest
            || self.body.signer_generation != policy.body().signer_generation
        {
            return Err(invalid("full-sync command policy/scope binding"));
        }
        verify(
            COMMAND_DOMAIN,
            &self.body,
            &self.signature_hex,
            &policy.body().management_public_key,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FullSyncIssuerTargetStateV2 {
    pub target: FullSyncIssuerTargetV2,
    pub generation: u64,
    pub active: bool,
    pub not_before_unix_ms: Option<u64>,
    pub expires_unix_ms: Option<u64>,
}

impl FullSyncIssuerTargetStateV2 {
    fn validate(&self) -> Result<(), FormalError> {
        self.target.validate()?;
        if self.generation == 0 {
            return Err(invalid("full-sync target state generation"));
        }
        match (self.active, self.not_before_unix_ms, self.expires_unix_ms) {
            (true, Some(not_before), Some(expires)) => FullSyncIssuerOperationV2::Issue {
                not_before_unix_ms: not_before,
                expires_unix_ms: expires,
            }
            .validate(),
            (false, None, None) => Ok(()),
            _ => Err(invalid("full-sync target active grant fields")),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FullSyncIssuerRecordV2 {
    pub schema_version: u32,
    pub policy: SignedFullSyncIssuerPolicyV2,
    pub command: SignedFullSyncIssuerCommandV2,
    pub resulting_target_state: FullSyncIssuerTargetStateV2,
    pub previous_anchor: FullSyncIssuerAnchorV2,
    pub next_anchor: FullSyncIssuerAnchorV2,
}

impl FullSyncIssuerRecordV2 {
    pub fn digest(&self) -> Result<String, FormalError> {
        digest(RECORD_DIGEST_DOMAIN, self, MAX_ENTRY_BYTES)
    }
}

#[derive(Debug, Clone)]
pub struct VerifiedFullSyncIssuerStateV2 {
    configured_policy: VerifiedFullSyncIssuerPolicyV2,
    active_policy: Option<VerifiedFullSyncIssuerPolicyV2>,
    anchor: FullSyncIssuerAnchorV2,
    target_states: BTreeMap<FullSyncIssuerTargetV2, FullSyncIssuerTargetStateV2>,
    entries: Vec<FullSyncIssuerCommittedEntryV2>,
    requests: BTreeMap<String, usize>,
    request_digests: BTreeMap<String, String>,
    issuer_keys: BTreeSet<String>,
    manager_nonces: BTreeSet<(String, String)>,
    encoded_bytes: usize,
}

struct PreparedFullSyncIssuerNextStateV2 {
    target_states: BTreeMap<FullSyncIssuerTargetV2, FullSyncIssuerTargetStateV2>,
    configured_policy: VerifiedFullSyncIssuerPolicyV2,
    request_digests: BTreeMap<String, String>,
    issuer_keys: BTreeSet<String>,
    manager_nonces: BTreeSet<(String, String)>,
}

struct SuccessorStateDigestInput<'a> {
    target_states: &'a BTreeMap<FullSyncIssuerTargetV2, FullSyncIssuerTargetStateV2>,
    request_digests: &'a BTreeMap<String, String>,
    issuer_keys: &'a BTreeSet<String>,
    manager_nonces: &'a BTreeSet<(String, String)>,
}

impl VerifiedFullSyncIssuerStateV2 {
    pub fn genesis_anchor(
        policy: &VerifiedFullSyncIssuerPolicyV2,
    ) -> Result<FullSyncIssuerAnchorV2, FormalError> {
        Ok(Self::genesis(policy.clone())?.anchor)
    }

    pub fn from_committed_records(
        configured_policy: VerifiedFullSyncIssuerPolicyV2,
        records: Vec<FullSyncIssuerCommittedEntryV2>,
    ) -> Result<Self, FormalError> {
        if records.len() > MAX_LEDGER_RECORDS {
            return Err(invalid("full-sync issuer record count bound"));
        }
        let mut state = Self::genesis(configured_policy)?;
        for entry in records {
            state.apply_committed_entry(entry)?;
        }
        state.validate_configured_policy()?;
        Ok(state)
    }

    /// Verify a complete witness response from genesis through the exact
    /// nonce-bound observation; a prefix floor must be present at its exact
    /// revision and digest in that same reconstructed chain.
    pub fn from_witness_observation(
        configured_policy: VerifiedFullSyncIssuerPolicyV2,
        records: Vec<FullSyncIssuerCommittedEntryV2>,
        observation: &SignedFullSyncIssuerObservationV2,
    ) -> Result<Self, FormalError> {
        observation.verify(&configured_policy)?;
        let expected_head = observation.body.current_anchor.clone();
        let minimum = observation.body.query.body.minimum_anchor.clone();
        let state = Self::from_committed_records(configured_policy, records)?;
        let prefix = state.anchor_at_revision(minimum.revision)?;
        if prefix != minimum || state.anchor != expected_head {
            return Err(FormalError::Rollback(
                "full-sync witness records do not close exact observed head/prefix".into(),
            ));
        }
        Ok(state)
    }

    fn genesis(configured_policy: VerifiedFullSyncIssuerPolicyV2) -> Result<Self, FormalError> {
        let empty = BTreeMap::<FullSyncIssuerTargetV2, FullSyncIssuerTargetStateV2>::new();
        let request_digests = BTreeMap::<String, String>::new();
        let issuer_keys = BTreeSet::<String>::new();
        let manager_nonces = BTreeSet::<(String, String)>::new();
        let state_digest = digest(
            STATE_DOMAIN,
            &(
                &empty,
                &request_digests,
                &issuer_keys,
                &manager_nonces,
                None::<String>,
                0u64,
            ),
            MAX_LEDGER_BYTES,
        )?;
        let log_digest = digest(
            GENESIS_DOMAIN,
            &configured_policy.scope_digest,
            MAX_ENTRY_BYTES,
        )?;
        let anchor = FullSyncIssuerAnchorV2 {
            schema_version: 2,
            revision: 0,
            scope_digest: configured_policy.scope_digest.clone(),
            log_digest,
            state_digest,
            policy_digest: None,
            signer_generation: 0,
        };
        anchor.validate()?;
        Ok(Self {
            configured_policy,
            active_policy: None,
            anchor,
            target_states: empty,
            entries: Vec::new(),
            requests: BTreeMap::new(),
            request_digests,
            issuer_keys,
            manager_nonces,
            encoded_bytes: 0,
        })
    }

    pub fn anchor(&self) -> &FullSyncIssuerAnchorV2 {
        &self.anchor
    }
    pub fn policy(&self) -> &VerifiedFullSyncIssuerPolicyV2 {
        &self.configured_policy
    }
    pub fn records(&self) -> &[FullSyncIssuerCommittedEntryV2] {
        &self.entries
    }
    pub fn target_state(
        &self,
        target: &FullSyncIssuerTargetV2,
    ) -> Option<&FullSyncIssuerTargetStateV2> {
        self.target_states.get(target)
    }

    fn anchor_at_revision(&self, revision: u64) -> Result<FullSyncIssuerAnchorV2, FormalError> {
        if revision == 0 {
            let initial = Self::genesis(self.configured_policy.clone())?;
            return Ok(initial.anchor);
        }
        let index = usize::try_from(revision - 1)
            .map_err(|_| invalid("full-sync anchor revision conversion"))?;
        self.entries
            .get(index)
            .map(|entry| entry.record.next_anchor.clone())
            .ok_or_else(|| invalid("full-sync anchor revision absent from records"))
    }

    fn validate_configured_policy(&self) -> Result<(), FormalError> {
        if self.configured_policy.scope_digest != self.anchor.scope_digest {
            return Err(invalid("full-sync configured policy scope mismatch"));
        }
        match &self.active_policy {
            None => Ok(()),
            Some(active) => {
                let configured = &self.configured_policy;
                if configured.body().signer_generation < active.body().signer_generation {
                    return Err(FormalError::Rollback(
                        "full-sync configured signer generation".into(),
                    ));
                }
                if configured.body().signer_generation == active.body().signer_generation {
                    if configured.policy_digest != active.policy_digest {
                        return Err(invalid("full-sync same-generation policy conflict"));
                    }
                } else if self
                    .issuer_keys
                    .contains(&configured.body().issuer_public_key)
                {
                    return Err(invalid("full-sync issuer key reused across generations"));
                }
                Ok(())
            }
        }
    }

    fn apply_committed_entry(
        &mut self,
        entry: FullSyncIssuerCommittedEntryV2,
    ) -> Result<(), FormalError> {
        let record = &entry.record;
        if record.schema_version != 2 || self.entries.len() >= MAX_LEDGER_RECORDS {
            return Err(invalid("full-sync record version/capacity"));
        }
        let policy =
            VerifiedFullSyncIssuerPolicyV2::verify(&record.policy, &self.configured_policy.trust)?;
        if policy.scope_digest != self.configured_policy.scope_digest {
            return Err(invalid(
                "full-sync historical policy changed fixed ledger scope",
            ));
        }
        self.validate_policy_step(&policy, record.command.body.transition.as_ref())?;
        record.command.verify(&policy)?;
        if record.previous_anchor != self.anchor
            || record.command.body.expected_head != record.previous_anchor
            || !management_role_authorized(
                &policy,
                &record.command.body,
                self.target_states.contains_key(&record.command.body.target),
            )
        {
            return Err(FormalError::Rollback(
                "full-sync record previous anchor".into(),
            ));
        }
        let expected_state = self.transition_target_state(&record.command.body)?;
        if record.resulting_target_state != expected_state {
            return Err(invalid(
                "full-sync record target state does not match reducer",
            ));
        }
        let request_id = &record.command.body.request_id;
        if self.requests.contains_key(request_id) {
            return Err(FormalError::Conflict(
                "full-sync request identity reused in log".into(),
            ));
        }
        let nonce_key = (
            policy.body().management_public_key.clone(),
            record.command.body.command_nonce.clone(),
        );
        if self.manager_nonces.contains(&nonce_key) {
            return Err(FormalError::Conflict(
                "full-sync manager nonce reused in log".into(),
            ));
        }
        if !self.target_states.contains_key(&expected_state.target)
            && self.target_states.len() >= MAX_TARGETS
        {
            return Err(invalid("full-sync target capacity"));
        }
        let mut next_targets = self.target_states.clone();
        next_targets.insert(expected_state.target.clone(), expected_state.clone());
        let mut next_requests = self.request_digests.clone();
        next_requests.insert(request_id.clone(), command_digest(&record.command)?);
        let mut next_issuer_keys = self.issuer_keys.clone();
        next_issuer_keys.insert(policy.body().issuer_public_key.clone());
        let mut next_nonces = self.manager_nonces.clone();
        next_nonces.insert(nonce_key.clone());
        let expected_next = successor_anchor(
            &self.anchor,
            SuccessorStateDigestInput {
                target_states: &next_targets,
                request_digests: &next_requests,
                issuer_keys: &next_issuer_keys,
                manager_nonces: &next_nonces,
            },
            &record.policy,
            &record.command,
            &policy,
        )?;
        if record.next_anchor != expected_next {
            return Err(invalid("full-sync record successor anchor"));
        }
        entry.receipt.verify_for_record(record, &policy)?;
        let stored_bytes = encode(STORED_ENTRY_DOMAIN, &entry, MAX_STORED_ENTRY_BYTES)?.len();
        let encoded_bytes = self
            .encoded_bytes
            .checked_add(stored_bytes)
            .filter(|bytes| *bytes <= MAX_LEDGER_BYTES)
            .ok_or_else(|| invalid("full-sync ledger byte capacity"))?;
        self.anchor = expected_next;
        self.target_states = next_targets;
        self.request_digests = next_requests;
        self.issuer_keys = next_issuer_keys;
        self.manager_nonces = next_nonces;
        self.encoded_bytes = encoded_bytes;
        self.active_policy = Some(policy);
        self.requests.insert(request_id.clone(), self.entries.len());
        self.entries.push(entry);
        Ok(())
    }

    fn validate_policy_step(
        &self,
        next: &VerifiedFullSyncIssuerPolicyV2,
        transition: Option<&FullSyncIssuerPolicyTransitionV2>,
    ) -> Result<(), FormalError> {
        if next.scope_digest != self.anchor.scope_digest {
            return Err(invalid("full-sync policy changes immutable scope"));
        }
        match &self.active_policy {
            None => {
                if transition.is_some() {
                    return Err(invalid("full-sync initial policy has no prior transition"));
                }
            }
            Some(previous) => {
                match next
                    .body()
                    .signer_generation
                    .cmp(&previous.body().signer_generation)
                {
                    std::cmp::Ordering::Less => {
                        return Err(FormalError::Rollback(
                            "full-sync old signer generation".into(),
                        ));
                    }
                    std::cmp::Ordering::Equal => {
                        if next.policy_digest != previous.policy_digest || transition.is_some() {
                            return Err(invalid("full-sync same-generation policy replacement"));
                        }
                    }
                    std::cmp::Ordering::Greater => {
                        let expected = FullSyncIssuerPolicyTransitionV2 {
                            previous_policy_digest: previous.policy_digest.clone(),
                            previous_signer_generation: previous.body().signer_generation,
                            new_policy_digest: next.policy_digest.clone(),
                            new_signer_generation: next.body().signer_generation,
                        };
                        if transition != Some(&expected) {
                            return Err(invalid("full-sync policy transition binding"));
                        }
                        if self.issuer_keys.contains(&next.body().issuer_public_key) {
                            return Err(invalid("full-sync issuer key reused across generations"));
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn transition_target_state(
        &self,
        command: &FullSyncIssuerCommandBodyV2,
    ) -> Result<FullSyncIssuerTargetStateV2, FormalError> {
        let current = self.target_states.get(&command.target);
        let current_generation = current.map_or(0, |state| state.generation);
        if command.expected_target_generation != current_generation {
            return Err(FormalError::Conflict(
                "full-sync target generation compare-and-set".into(),
            ));
        }
        let next_generation = current_generation
            .checked_add(1)
            .ok_or_else(|| invalid("full-sync target generation overflow"))?;
        let (active, not_before, expires) = match &command.operation {
            FullSyncIssuerOperationV2::Issue {
                not_before_unix_ms,
                expires_unix_ms,
            } => {
                if current.is_some_and(|state| state.active) {
                    return Err(FormalError::Conflict(
                        "full-sync issue requires inactive target".into(),
                    ));
                }
                (true, Some(*not_before_unix_ms), Some(*expires_unix_ms))
            }
            FullSyncIssuerOperationV2::Replace {
                not_before_unix_ms,
                expires_unix_ms,
            } => {
                if !current.is_some_and(|state| state.active) {
                    return Err(FormalError::Conflict(
                        "full-sync replace requires active target".into(),
                    ));
                }
                (true, Some(*not_before_unix_ms), Some(*expires_unix_ms))
            }
            FullSyncIssuerOperationV2::Revoke => (false, None, None),
        };
        let result = FullSyncIssuerTargetStateV2 {
            target: command.target.clone(),
            generation: next_generation,
            active,
            not_before_unix_ms: not_before,
            expires_unix_ms: expires,
        };
        result.validate()?;
        Ok(result)
    }

    /// Exact request-ID replay is resolved before current-policy/head checks;
    /// an identical request returns its original full signed decision.
    pub fn prepare(
        &self,
        command: SignedFullSyncIssuerCommandV2,
    ) -> Result<PreparedFullSyncIssuerDecisionV2<'_>, FormalError> {
        canonical::<32>(&command.body.request_id)?;
        if let Some(index) = self.requests.get(&command.body.request_id) {
            let original = &self.entries[*index];
            if original.record.command == command {
                return Ok(PreparedFullSyncIssuerDecisionV2 {
                    state: self,
                    previous_anchor: original.record.previous_anchor.clone(),
                    existing: Some(original),
                    candidate: None,
                    next_state: None,
                });
            }
            return Err(FormalError::Conflict(
                "full-sync request identity content conflict".into(),
            ));
        }
        self.configured_policy.body().validate()?;
        self.validate_configured_policy()?;
        command.verify(&self.configured_policy)?;
        self.validate_policy_step(&self.configured_policy, command.body.transition.as_ref())?;
        if command.body.expected_head != self.anchor {
            return Err(FormalError::Conflict(
                "full-sync global head compare-and-set".into(),
            ));
        }
        let nonce_key = (
            self.configured_policy.body().management_public_key.clone(),
            command.body.command_nonce.clone(),
        );
        if self.manager_nonces.contains(&nonce_key) {
            return Err(FormalError::Conflict(
                "full-sync manager nonce already used".into(),
            ));
        }
        if !management_role_authorized(
            &self.configured_policy,
            &command.body,
            self.target_states.contains_key(&command.body.target),
        ) {
            return Err(invalid("full-sync command role not allowed by policy"));
        }
        let resulting_target_state = self.transition_target_state(&command.body)?;
        if !self.target_states.contains_key(&command.body.target)
            && self.target_states.len() >= MAX_TARGETS
        {
            return Err(invalid("full-sync target capacity"));
        }
        if self.entries.len() >= MAX_LEDGER_RECORDS {
            return Err(invalid("full-sync record capacity"));
        }
        let mut next_targets = self.target_states.clone();
        next_targets.insert(command.body.target.clone(), resulting_target_state.clone());
        let mut next_requests = self.request_digests.clone();
        next_requests.insert(command.body.request_id.clone(), command_digest(&command)?);
        let mut next_issuer_keys = self.issuer_keys.clone();
        next_issuer_keys.insert(self.configured_policy.body().issuer_public_key.clone());
        let mut next_nonces = self.manager_nonces.clone();
        next_nonces.insert(nonce_key);
        let next_anchor = successor_anchor(
            &self.anchor,
            SuccessorStateDigestInput {
                target_states: &next_targets,
                request_digests: &next_requests,
                issuer_keys: &next_issuer_keys,
                manager_nonces: &next_nonces,
            },
            self.configured_policy.signed(),
            &command,
            &self.configured_policy,
        )?;
        let candidate = FullSyncIssuerRecordV2 {
            schema_version: 2,
            policy: self.configured_policy.signed().clone(),
            command,
            resulting_target_state,
            previous_anchor: self.anchor.clone(),
            next_anchor,
        };
        let _ = encode(RECORD_DIGEST_DOMAIN, &candidate, MAX_ENTRY_BYTES)?;
        let candidate_bytes =
            encode(STORED_ENTRY_DOMAIN, &candidate, MAX_STORED_ENTRY_BYTES)?.len();
        self.encoded_bytes
            .checked_add(candidate_bytes)
            .and_then(|bytes| bytes.checked_add(MAX_RECEIPT_RESERVE_BYTES))
            .filter(|bytes| *bytes <= MAX_LEDGER_BYTES)
            .ok_or_else(|| invalid("full-sync ledger byte capacity"))?;
        Ok(PreparedFullSyncIssuerDecisionV2 {
            state: self,
            previous_anchor: self.anchor.clone(),
            existing: None,
            candidate: Some(candidate),
            next_state: Some(PreparedFullSyncIssuerNextStateV2 {
                target_states: next_targets,
                configured_policy: self.configured_policy.clone(),
                request_digests: next_requests,
                issuer_keys: next_issuer_keys,
                manager_nonces: next_nonces,
            }),
        })
    }
}

pub struct PreparedFullSyncIssuerDecisionV2<'a> {
    state: &'a VerifiedFullSyncIssuerStateV2,
    previous_anchor: FullSyncIssuerAnchorV2,
    existing: Option<&'a FullSyncIssuerCommittedEntryV2>,
    candidate: Option<FullSyncIssuerRecordV2>,
    next_state: Option<PreparedFullSyncIssuerNextStateV2>,
}

impl<'a> PreparedFullSyncIssuerDecisionV2<'a> {
    pub fn existing(&self) -> Option<&'a FullSyncIssuerCommittedEntryV2> {
        self.existing
    }
    pub fn record(&self) -> Option<&FullSyncIssuerRecordV2> {
        self.candidate.as_ref()
    }
    pub fn previous(&self) -> &FullSyncIssuerAnchorV2 {
        &self.previous_anchor
    }
    pub fn next(&self) -> Option<&FullSyncIssuerAnchorV2> {
        self.candidate.as_ref().map_or_else(
            || self.existing.map(|entry| &entry.record.next_anchor),
            |record| Some(&record.next_anchor),
        )
    }
    pub fn commit(
        self,
        receipt: SignedFullSyncIssuerReceiptV2,
    ) -> Result<VerifiedFullSyncIssuerDecisionV2, FormalError> {
        let record = self
            .candidate
            .ok_or_else(|| invalid("idempotent decision already committed"))?;
        let policy = self
            .next_state
            .as_ref()
            .map(|state| state.configured_policy.clone())
            .ok_or_else(|| invalid("prepared full-sync policy unavailable"))?;
        receipt.verify_for_record(&record, &policy)?;
        let next_state = self
            .next_state
            .ok_or_else(|| invalid("prepared next state unavailable"))?;
        let index = self.state.entries.len();
        let request_id = record.command.body.request_id.clone();
        let entry = FullSyncIssuerCommittedEntryV2 {
            record: record.clone(),
            receipt,
        };
        let mut state = self.state.clone();
        let stored_bytes = encode(STORED_ENTRY_DOMAIN, &entry, MAX_STORED_ENTRY_BYTES)?.len();
        let encoded_bytes = state
            .encoded_bytes
            .checked_add(stored_bytes)
            .filter(|bytes| *bytes <= MAX_LEDGER_BYTES)
            .ok_or_else(|| invalid("full-sync ledger byte capacity"))?;
        state.anchor = record.next_anchor.clone();
        state.target_states = next_state.target_states;
        state.request_digests = next_state.request_digests;
        state.issuer_keys = next_state.issuer_keys;
        state.manager_nonces = next_state.manager_nonces;
        state.encoded_bytes = encoded_bytes;
        state.active_policy = Some(policy.clone());
        state.configured_policy = next_state.configured_policy;
        state
            .issuer_keys
            .insert(policy.body().issuer_public_key.clone());
        state.requests.insert(request_id, index);
        state.entries.push(entry.clone());
        Ok(VerifiedFullSyncIssuerDecisionV2 { entry, state })
    }
}

#[derive(Debug, Clone)]
pub struct VerifiedFullSyncIssuerDecisionV2 {
    entry: FullSyncIssuerCommittedEntryV2,
    state: VerifiedFullSyncIssuerStateV2,
}

fn management_role_authorized(
    policy: &VerifiedFullSyncIssuerPolicyV2,
    command: &FullSyncIssuerCommandBodyV2,
    target_is_known: bool,
) -> bool {
    policy.body().allowed_roles.contains(&command.target.role)
        || (target_is_known && matches!(&command.operation, FullSyncIssuerOperationV2::Revoke))
}

impl VerifiedFullSyncIssuerDecisionV2 {
    pub fn record(&self) -> &FullSyncIssuerRecordV2 {
        &self.entry.record
    }
    pub fn receipt(&self) -> &SignedFullSyncIssuerReceiptV2 {
        &self.entry.receipt
    }
    pub fn entry(&self) -> &FullSyncIssuerCommittedEntryV2 {
        &self.entry
    }
    pub fn state(&self) -> &VerifiedFullSyncIssuerStateV2 {
        &self.state
    }
}

fn successor_anchor(
    previous: &FullSyncIssuerAnchorV2,
    state_input: SuccessorStateDigestInput<'_>,
    policy: &SignedFullSyncIssuerPolicyV2,
    command: &SignedFullSyncIssuerCommandV2,
    verified_policy: &VerifiedFullSyncIssuerPolicyV2,
) -> Result<FullSyncIssuerAnchorV2, FormalError> {
    let revision = previous
        .revision
        .checked_add(1)
        .ok_or_else(|| invalid("full-sync ledger revision overflow"))?;
    let state_digest = digest(
        STATE_DOMAIN,
        &(
            state_input.target_states,
            state_input.request_digests,
            state_input.issuer_keys,
            state_input.manager_nonces,
            Some(verified_policy.policy_digest.as_str()),
            verified_policy.body().signer_generation,
        ),
        MAX_LEDGER_BYTES,
    )?;
    let log_digest = digest(
        LOG_LINK_DOMAIN,
        &(previous, policy, command, &state_digest),
        MAX_ENTRY_BYTES,
    )?;
    Ok(FullSyncIssuerAnchorV2 {
        schema_version: 2,
        revision,
        scope_digest: previous.scope_digest.clone(),
        log_digest,
        state_digest,
        policy_digest: Some(verified_policy.policy_digest.clone()),
        signer_generation: verified_policy.body().signer_generation,
    })
}

fn scope_digest(policy: &FullSyncIssuerPolicyBodyV2) -> Result<String, FormalError> {
    digest(
        SCOPE_DOMAIN,
        &(
            &policy.root_public_key,
            &policy.world,
            policy.authority_epoch,
            &policy.witness_public_key,
            &policy.activation_trust_digest,
        ),
        MAX_ENTRY_BYTES,
    )
}

fn command_digest(command: &SignedFullSyncIssuerCommandV2) -> Result<String, FormalError> {
    digest(
        "oasis7.formal.full-sync-issuer.signed-command.v2",
        command,
        MAX_ENTRY_BYTES,
    )
}

fn encode<T: Serialize>(domain: &str, value: &T, max_bytes: usize) -> Result<Vec<u8>, FormalError> {
    let serialized = serde_cbor::to_vec(value).map_err(|error| invalid(error.to_string()))?;
    let mut output = Vec::with_capacity(domain.len() + 8 + serialized.len());
    output.extend((domain.len() as u64).to_be_bytes());
    output.extend(domain.as_bytes());
    output.extend(serialized);
    if output.len() > max_bytes {
        return Err(invalid("full-sync issuer encoded material byte bound"));
    }
    Ok(output)
}

fn digest<T: Serialize>(domain: &str, value: &T, max_bytes: usize) -> Result<String, FormalError> {
    Ok(blake3::hash(&encode(domain, value, max_bytes)?)
        .to_hex()
        .to_string())
}

#[path = "full_sync_issuer_witness.rs"]
mod witness;
pub use witness::*;

#[path = "full_sync_issuer_v3/mod.rs"]
pub mod v3;
pub use v3::*;

#[cfg(test)]
#[path = "full_sync_issuer_tests.rs"]
mod tests;
