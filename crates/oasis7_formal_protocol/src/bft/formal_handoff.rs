//! Versioned bridge from a qualified controlled-profile head to the first
//! formal BFT V2 schedule. This verifies the signed handoff declaration; it
//! does not replace execution-driver replay of the H closure.

use crate::controlled_authority::formal::{
    FORMAL_PROFILE, FormalHeadRefV1, FormalRecordKindV1, FormalWorldIdentityV1,
    VerifiedFormalReadAnchorV1,
};
use oasis7_proto::distributed_bft::v2::{
    BFT_MAX_SIGNED_MATERIAL_BYTES_V2, BFT_MAX_VALIDATORS_V2, BFT_PROTOCOL_VERSION_V2, BftBlockV2,
    BftScopeV2, BftValidatorTransitionRefV2, BftValidatorTransitionV2, BftValidatorV2,
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

use super::verified_commit::{BftHeightProposerScheduleV2, FormalHandoffScheduleAnchorV1};
use super::{BftSchedulePolicyV2, BftValidatorSetV2};

pub const BFT_FORMAL_PROFILE_V1: &str = "bft";
pub const BFT_FORMAL_PROFILE_VERSION_V1: u32 = 1;
pub const BFT_FORMAL_TICK_MAPPING_RULESET_V1: &str = "formal-position-equals-runtime-tick-v1";
pub const BFT_FORMAL_EXECUTION_COMMITMENT_MAX_BYTES_V1: usize = 256 * 1024;
const BFT_FORMAL_PROFILE_TARGET_MAX_BYTES_V1: usize = BFT_MAX_SIGNED_MATERIAL_BYTES_V2;
const BFT_PROFILE_TARGET_DOMAIN_V1: &str = "oasis7.formal.bft.profile_target.v1";
const BFT_HANDOFF_ACTIVATION_DOMAIN_V1: &str = "oasis7.formal.bft.handoff_activation.v1";
pub const BFT_FORMAL_EXECUTION_DOMAIN_V1: &str = "oasis7.formal.bft.execution.v1";

#[path = "formal_prefix.rs"]
mod prefix;
pub use prefix::{
    BFT_FORMAL_PREFIX_MAX_BYTES_V1, BFT_FORMAL_PREFIX_MAX_HEIGHTS_V1, FormalBftCertifiedEntryV1,
    FormalBftCertifiedPrefixV1, VerifiedFormalBftPrefixV1,
};

/// Static V1 target configuration committed by a qualified formal H record.
/// It deliberately contains no H output root or manifest, avoiding a cycle
/// through the record that stores this digest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftProfileTargetV1 {
    pub schema_version: u32,
    pub profile: String,
    pub profile_version: u32,
    pub wire_version: u16,
    pub scope: BftScopeV2,
    pub authority_epoch: u64,
    pub initial_validators: Vec<BftValidatorV2>,
    pub schedule_policy: BftSchedulePolicyV2,
    pub tick_mapping_ruleset: String,
}

impl BftProfileTargetV1 {
    pub fn new(
        scope: BftScopeV2,
        authority_epoch: u64,
        initial_validators: Vec<BftValidatorV2>,
        schedule_policy: BftSchedulePolicyV2,
    ) -> Result<Self, String> {
        let target = Self {
            schema_version: 1,
            profile: BFT_FORMAL_PROFILE_V1.to_owned(),
            profile_version: BFT_FORMAL_PROFILE_VERSION_V1,
            wire_version: BFT_PROTOCOL_VERSION_V2,
            scope,
            authority_epoch,
            initial_validators,
            schedule_policy,
            tick_mapping_ruleset: BFT_FORMAL_TICK_MAPPING_RULESET_V1.to_owned(),
        };
        target.validate()?;
        Ok(target)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != 1
            || self.profile != BFT_FORMAL_PROFILE_V1
            || self.profile_version != BFT_FORMAL_PROFILE_VERSION_V1
            || self.wire_version != BFT_PROTOCOL_VERSION_V2
            || self.authority_epoch == 0
            || self.tick_mapping_ruleset != BFT_FORMAL_TICK_MAPPING_RULESET_V1
            || self.initial_validators.is_empty()
            || self.initial_validators.len() > BFT_MAX_VALIDATORS_V2
        {
            return Err("unsupported formal BFT V1 target configuration".into());
        }
        validate_target_configuration(
            &self.scope,
            &self.initial_validators,
            &self.schedule_policy,
            &self.profile,
        )
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>, String> {
        self.validate()?;
        canonical_bytes(
            self,
            BFT_FORMAL_PROFILE_TARGET_MAX_BYTES_V1,
            "BFT profile target",
        )
    }

    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, String> {
        let value: Self = decode_canonical(
            bytes,
            BFT_FORMAL_PROFILE_TARGET_MAX_BYTES_V1,
            "BFT profile target",
        )?;
        value.validate()?;
        Ok(value)
    }

    /// Digest written to ProfileHandoffV1.next_authority_digest.
    pub fn target_digest(&self) -> Result<String, String> {
        let bytes = self.canonical_bytes()?;
        Ok(domain_digest(BFT_PROFILE_TARGET_DOMAIN_V1, &bytes))
    }

    pub fn profile(&self) -> &str {
        &self.profile
    }

    pub fn profile_version(&self) -> u32 {
        self.profile_version
    }

    pub fn wire_version(&self) -> u16 {
        self.wire_version
    }

    pub fn scope(&self) -> &BftScopeV2 {
        &self.scope
    }

    pub fn authority_epoch(&self) -> u64 {
        self.authority_epoch
    }

    pub fn initial_validators(&self) -> &[BftValidatorV2] {
        &self.initial_validators
    }

    pub fn schedule_policy(&self) -> &BftSchedulePolicyV2 {
        &self.schedule_policy
    }

    pub fn tick_mapping_ruleset(&self) -> &str {
        &self.tick_mapping_ruleset
    }

    pub fn initial_validator_set_hash(&self) -> Result<String, String> {
        Ok(self.validator_set()?.set_hash().to_owned())
    }

    pub fn schedule_policy_hash(&self) -> Result<String, String> {
        self.schedule_policy.hash()
    }

    fn validator_set(&self) -> Result<BftValidatorSetV2, String> {
        BftValidatorSetV2::new(self.scope.clone(), self.initial_validators.clone())
    }
}

pub(super) fn validate_target_configuration(
    scope: &BftScopeV2,
    validators: &[BftValidatorV2],
    policy: &BftSchedulePolicyV2,
    profile: &str,
) -> Result<(), String> {
    scope.validate()?;
    policy.validate()?;
    if policy.profile_id != profile {
        return Err("formal BFT schedule policy must use the target profile id".into());
    }
    if validators
        .windows(2)
        .any(|pair| pair[0].validator_id >= pair[1].validator_id)
    {
        return Err("formal BFT target validators must be sorted and unique".into());
    }
    BftValidatorSetV2::new(scope.clone(), validators.to_vec())?;
    Ok(())
}

/// Opaque proof that a qualified formal H committed to this exact static
/// profile target. It does not prove the restored World matches H.
#[derive(Debug, Clone)]
pub struct VerifiedBftHandoffTargetV1 {
    target: BftProfileTargetV1,
    target_digest: String,
    activation_digest: String,
    handoff_head: FormalHeadRefV1,
    raw_snapshot_root: String,
    governing_manifest_digest: String,
    finalizing_authority_id: String,
    initial_validator_set_hash: String,
    schedule_policy_hash: String,
}

impl VerifiedBftHandoffTargetV1 {
    pub fn target(&self) -> &BftProfileTargetV1 {
        &self.target
    }

    pub fn target_digest(&self) -> &str {
        &self.target_digest
    }

    pub fn activation_digest(&self) -> &str {
        &self.activation_digest
    }

    pub fn handoff_head(&self) -> &FormalHeadRefV1 {
        &self.handoff_head
    }

    pub fn handoff_position(&self) -> u64 {
        self.handoff_head.position
    }

    pub fn activation_height(&self) -> Result<u64, String> {
        self.handoff_head
            .position
            .checked_add(1)
            .ok_or_else(|| "formal BFT activation height overflow".into())
    }

    pub fn scope(&self) -> &BftScopeV2 {
        self.target.scope()
    }

    pub fn raw_snapshot_root(&self) -> &str {
        &self.raw_snapshot_root
    }

    pub fn governing_manifest_digest(&self) -> &str {
        &self.governing_manifest_digest
    }

    pub fn finalizing_authority_id(&self) -> &str {
        &self.finalizing_authority_id
    }

    pub fn initial_validator_set_hash(&self) -> &str {
        &self.initial_validator_set_hash
    }

    pub fn schedule_policy_hash(&self) -> &str {
        &self.schedule_policy_hash
    }

    /// Derives the sole H+1 schedule from the exact first-block preimage.
    /// The caller must then verify the QC; the execution driver must separately
    /// replay H's closure and compare the actual World to these bound claims.
    pub fn initial_schedule_for_commitment(
        &self,
        commitment: &FormalBftExecutionCommitmentV1,
        block: &BftBlockV2,
    ) -> Result<BftHeightProposerScheduleV2, String> {
        commitment.verify_block(block, &self.activation_digest)?;
        let activation_height = self.activation_height()?;
        if commitment.scope != self.target.scope
            || commitment.height != activation_height
            || commitment.parent_commit_ref != self.handoff_head.record_id
            || commitment.parent_tick != self.handoff_head.position
            || commitment.authority_epoch != self.target.authority_epoch
            || commitment.active_validator_set_hash != self.initial_validator_set_hash
            || commitment.schedule_policy_hash != self.schedule_policy_hash
        {
            return Err("formal BFT first commitment does not bind the H handoff".into());
        }
        self.initial_schedule_after_handoff_replay(
            &commitment.parent_semantic_state_root,
            &commitment.manifest_hash,
        )
    }

    /// Derive the H+1 schedule after the caller has independently replayed H's
    /// authenticated closure and checked the supplied semantic root against
    /// the resulting World. This capability binds the exact H and static
    /// target, but it does not itself authenticate a World replay.
    pub fn initial_schedule_after_handoff_replay(
        &self,
        checked_parent_semantic_state_root: &str,
        checked_parent_manifest_hash: &str,
    ) -> Result<BftHeightProposerScheduleV2, String> {
        // The qualified record binds the cognition runtime digest. The World
        // manifest is a distinct claim authenticated by caller replay and QC.
        validate_hash("replayed H World manifest", checked_parent_manifest_hash)?;
        let set = self.target.validator_set()?;
        BftHeightProposerScheduleV2::from_formal_handoff_v1(
            set,
            self.target.schedule_policy.clone(),
            FormalHandoffScheduleAnchorV1 {
                authority_epoch: self.target.authority_epoch,
                height: self.activation_height()?,
                parent_commit_ref: self.handoff_head.record_id.clone(),
                parent_tick: self.handoff_head.position,
                parent_state_root: checked_parent_semantic_state_root.to_owned(),
                parent_manifest_hash: checked_parent_manifest_hash.to_owned(),
            },
        )
    }
}

/// Verify that the already-qualified formal head H commits to the exact target.
/// This consumes no peer-supplied trust: the anchor must have been independently
/// authenticated before this function is called.
pub fn verify_formal_bft_handoff_v1(
    anchor: &VerifiedFormalReadAnchorV1,
    target: BftProfileTargetV1,
) -> Result<VerifiedBftHandoffTargetV1, String> {
    target.validate()?;
    let target_digest = target.target_digest()?;
    let fields = verify_handoff_fields(
        anchor,
        &target.scope,
        &target.profile,
        target.profile_version,
        &target_digest,
    )?;
    let activation_digest = activation_digest(
        &target_digest,
        &fields.head,
        &fields.world,
        &fields.raw_snapshot_root,
        &fields.governing_manifest_digest,
        &fields.finalizing_authority_id,
    )?;
    Ok(VerifiedBftHandoffTargetV1 {
        initial_validator_set_hash: target.initial_validator_set_hash()?,
        schedule_policy_hash: target.schedule_policy_hash()?,
        target,
        target_digest,
        activation_digest,
        handoff_head: fields.head,
        raw_snapshot_root: fields.raw_snapshot_root,
        governing_manifest_digest: fields.governing_manifest_digest,
        finalizing_authority_id: fields.finalizing_authority_id,
    })
}

pub(super) struct QualifiedBftHandoffFields {
    pub(super) head: FormalHeadRefV1,
    pub(super) world: FormalWorldIdentityV1,
    pub(super) raw_snapshot_root: String,
    pub(super) governing_manifest_digest: String,
    pub(super) finalizing_authority_id: String,
}

/// Common controlled-H authentication, parameterized by an already validated
/// version-specific target. This does not choose an activation or wire version.
pub(super) fn verify_handoff_fields(
    anchor: &VerifiedFormalReadAnchorV1,
    scope: &BftScopeV2,
    profile: &str,
    profile_version: u32,
    target_digest: &str,
) -> Result<QualifiedBftHandoffFields, String> {
    let commit = anchor.head();
    let record = commit.record();
    record.validate().map_err(|error| error.to_string())?;
    if record.kind != FormalRecordKindV1::Execution
        || record.world != anchor.authority().definition().world
        || record.world.world_id != scope.world_id
        || record.world.chain_id != scope.chain_id
        || record.world.genesis_digest != scope.genesis_hash
        || anchor.authority().definition().profile != FORMAL_PROFILE
        || record.activation_ref != anchor.authority().activation_ref()
    {
        return Err("formal BFT handoff H authority or scope binding".into());
    }
    let handoff = record
        .handoff
        .as_ref()
        .ok_or_else(|| "qualified formal H does not contain a profile handoff".to_string())?;
    handoff
        .validate(record.position)
        .map_err(|error| error.to_string())?;
    if handoff.next_profile != profile
        || handoff.next_profile_version != profile_version
        || handoff.next_authority_digest != target_digest
    {
        return Err("formal H handoff does not bind the supplied BFT target".into());
    }
    let handoff_head = commit.head();
    let raw_snapshot_root = record.after_state_root.clone();
    let governing_manifest_digest = record.governing_manifest_digest.clone();
    let finalizing_authority_id = commit.finalizing_authority_id().to_owned();
    Ok(QualifiedBftHandoffFields {
        head: handoff_head,
        world: record.world.clone(),
        raw_snapshot_root,
        governing_manifest_digest,
        finalizing_authority_id,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FormalBftExecutionCommitmentV1 {
    pub domain: String,
    pub schema_version: u32,
    pub profile: String,
    pub profile_version: u32,
    pub activation_digest: String,
    pub scope: BftScopeV2,
    pub height: u64,
    pub parent_commit_ref: String,
    pub parent_tick: u64,
    pub parent_semantic_state_root: String,
    pub authority_epoch: u64,
    pub active_validator_set_hash: String,
    pub schedule_policy_hash: String,
    pub manifest_hash: String,
    pub post_manifest_hash: String,
    pub action_root: String,
    pub event_root: String,
    pub execution_state_root: String,
    pub state_root: String,
    pub projection_root: String,
    pub result_root: String,
    pub transition: Option<BftValidatorTransitionV2>,
    pub transition_ref: Option<BftValidatorTransitionRefV2>,
}

impl FormalBftExecutionCommitmentV1 {
    pub fn validate(&self) -> Result<(), String> {
        if self.domain != BFT_FORMAL_EXECUTION_DOMAIN_V1
            || self.schema_version != 1
            || self.profile != BFT_FORMAL_PROFILE_V1
            || self.profile_version != BFT_FORMAL_PROFILE_VERSION_V1
            || self.height == 0
            || self.parent_tick.checked_add(1) != Some(self.height)
            || self.authority_epoch == 0
        {
            return Err("unsupported formal BFT execution commitment header".into());
        }
        self.scope.validate()?;
        for (name, value) in [
            ("activation_digest", self.activation_digest.as_str()),
            ("parent_commit_ref", self.parent_commit_ref.as_str()),
            (
                "parent_semantic_state_root",
                self.parent_semantic_state_root.as_str(),
            ),
            (
                "active_validator_set_hash",
                self.active_validator_set_hash.as_str(),
            ),
            ("schedule_policy_hash", self.schedule_policy_hash.as_str()),
            ("manifest_hash", self.manifest_hash.as_str()),
            ("post_manifest_hash", self.post_manifest_hash.as_str()),
            ("action_root", self.action_root.as_str()),
            ("event_root", self.event_root.as_str()),
            ("execution_state_root", self.execution_state_root.as_str()),
            ("state_root", self.state_root.as_str()),
            ("projection_root", self.projection_root.as_str()),
            ("result_root", self.result_root.as_str()),
        ] {
            validate_hash(name, value)?;
        }
        if self.transition.is_some() && self.transition_ref.is_some() {
            return Err("formal BFT commitment cannot contain both transition forms".into());
        }
        if let Some(transition) = &self.transition {
            transition.validate()?;
            if transition.source_commit_ref != self.parent_commit_ref
                || transition.source_state_root != self.parent_semantic_state_root
            {
                return Err("formal BFT transition does not bind commitment parent".into());
            }
        }
        if let Some(transition_ref) = &self.transition_ref {
            transition_ref.validate()?;
        }
        Ok(())
    }

    pub fn preimage(&self) -> &Self {
        self
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>, String> {
        self.validate()?;
        canonical_bytes(
            self,
            BFT_FORMAL_EXECUTION_COMMITMENT_MAX_BYTES_V1,
            "formal BFT execution commitment",
        )
    }

    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, String> {
        let value: Self = decode_canonical(
            bytes,
            BFT_FORMAL_EXECUTION_COMMITMENT_MAX_BYTES_V1,
            "formal BFT execution commitment",
        )?;
        value.validate()?;
        Ok(value)
    }

    pub fn digest(&self) -> Result<String, String> {
        Ok(blake3::hash(&self.canonical_bytes()?).to_hex().to_string())
    }

    pub fn verify_block(
        &self,
        block: &BftBlockV2,
        expected_activation_digest: &str,
    ) -> Result<(), String> {
        self.validate()?;
        block.validate()?;
        if self.activation_digest != expected_activation_digest
            || self.scope != block.scope
            || self.height != block.height
            || self.parent_commit_ref != block.parent_commit_ref
            || self.authority_epoch != block.authority_epoch
            || self.active_validator_set_hash != block.active_validator_set_hash
            || self.schedule_policy_hash != block.schedule_policy_hash
            || self.manifest_hash != block.manifest_hash
            || self.post_manifest_hash != block.post_manifest_hash
            || self.action_root != block.action_root
            || self.event_root != block.event_root
            || self.execution_state_root != block.execution_state_root
            || self.state_root != block.state_root
            || self.transition != block.transition
            || self.transition_ref != block.transition_ref
            || self.digest()? != block.execution_block_hash
        {
            return Err("formal BFT execution commitment does not match its block".into());
        }
        Ok(())
    }

    pub fn activation_digest(&self) -> &str {
        &self.activation_digest
    }

    pub fn scope(&self) -> &BftScopeV2 {
        &self.scope
    }

    pub fn height(&self) -> u64 {
        self.height
    }

    pub fn parent_commit_ref(&self) -> &str {
        &self.parent_commit_ref
    }

    pub fn parent_tick(&self) -> u64 {
        self.parent_tick
    }

    pub fn parent_semantic_state_root(&self) -> &str {
        &self.parent_semantic_state_root
    }

    pub fn authority_epoch(&self) -> u64 {
        self.authority_epoch
    }

    pub fn active_validator_set_hash(&self) -> &str {
        &self.active_validator_set_hash
    }

    pub fn schedule_policy_hash(&self) -> &str {
        &self.schedule_policy_hash
    }

    pub fn manifest_hash(&self) -> &str {
        &self.manifest_hash
    }

    pub fn post_manifest_hash(&self) -> &str {
        &self.post_manifest_hash
    }

    pub fn action_root(&self) -> &str {
        &self.action_root
    }

    pub fn event_root(&self) -> &str {
        &self.event_root
    }

    pub fn execution_state_root(&self) -> &str {
        &self.execution_state_root
    }

    pub fn state_root(&self) -> &str {
        &self.state_root
    }

    pub fn projection_root(&self) -> &str {
        &self.projection_root
    }

    pub fn result_root(&self) -> &str {
        &self.result_root
    }

    pub fn transition(&self) -> Option<&BftValidatorTransitionV2> {
        self.transition.as_ref()
    }

    pub fn transition_ref(&self) -> Option<&BftValidatorTransitionRefV2> {
        self.transition_ref.as_ref()
    }
}

#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct ActivationDigestPayloadV1<'a> {
    domain: &'static str,
    schema_version: u32,
    target_digest: &'a str,
    handoff_head: &'a FormalHeadRefV1,
    world: &'a FormalWorldIdentityV1,
    raw_snapshot_root: &'a str,
    governing_manifest_digest: &'a str,
    finalizing_authority_id: &'a str,
}

fn activation_digest(
    target_digest: &str,
    handoff_head: &FormalHeadRefV1,
    world: &FormalWorldIdentityV1,
    raw_snapshot_root: &str,
    governing_manifest_digest: &str,
    finalizing_authority_id: &str,
) -> Result<String, String> {
    let payload = ActivationDigestPayloadV1 {
        domain: BFT_HANDOFF_ACTIVATION_DOMAIN_V1,
        schema_version: 1,
        target_digest,
        handoff_head,
        world,
        raw_snapshot_root,
        governing_manifest_digest,
        finalizing_authority_id,
    };
    let bytes = canonical_bytes(&payload, 16 * 1024, "formal BFT activation digest")?;
    Ok(blake3::hash(&bytes).to_hex().to_string())
}

pub(super) fn validate_hash(name: &str, value: &str) -> Result<(), String> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(format!("{name} must be canonical lowercase hex32"));
    }
    Ok(())
}

pub(super) fn canonical_bytes<T: Serialize>(
    value: &T,
    maximum: usize,
    context: &str,
) -> Result<Vec<u8>, String> {
    let bytes = canonical_cbor_with_self_describe(value)
        .map_err(|error| format!("encode {context}: {error}"))?;
    if bytes.is_empty() || bytes.len() > maximum {
        return Err(format!("{context} exceeds its canonical byte bound"));
    }
    Ok(bytes)
}

/// Match the canonical-CBOR bytes historically produced by oasis7_cas without
/// making storage part of this portable proof crate's normal dependency graph.
fn canonical_cbor_with_self_describe<T: Serialize>(value: &T) -> Result<Vec<u8>, String> {
    let canonical_value = serde_cbor::value::to_value(value)
        .map_err(|error| format!("canonicalize value: {error}"))?;
    let mut bytes = Vec::with_capacity(256);
    let mut serializer = serde_cbor::ser::Serializer::new(&mut bytes);
    serializer
        .self_describe()
        .map_err(|error| format!("write self-describe tag: {error}"))?;
    canonical_value
        .serialize(&mut serializer)
        .map_err(|error| format!("serialize canonical value: {error}"))?;
    Ok(bytes)
}

pub(super) fn decode_canonical<T: Serialize + DeserializeOwned>(
    bytes: &[u8],
    maximum: usize,
    context: &str,
) -> Result<T, String> {
    if bytes.is_empty() || bytes.len() > maximum {
        return Err(format!("{context} is empty or exceeds its byte bound"));
    }
    let value: T =
        serde_cbor::from_slice(bytes).map_err(|error| format!("decode {context}: {error}"))?;
    if canonical_bytes(&value, maximum, context)? != bytes {
        return Err(format!("{context} is not canonical CBOR"));
    }
    Ok(value)
}

pub(super) fn domain_digest(domain: &str, bytes: &[u8]) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(&(domain.len() as u64).to_be_bytes());
    hasher.update(domain.as_bytes());
    hasher.update(bytes);
    hasher.finalize().to_hex().to_string()
}

#[cfg(test)]
#[path = "formal_handoff_tests.rs"]
mod tests;
