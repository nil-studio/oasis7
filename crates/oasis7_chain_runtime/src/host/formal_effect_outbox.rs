//! Qualified post-commit external effect obligations.
//!
//! This module is deliberately a reader of committed material, not a World
//! mutator. A `QualifiedEffectOutboxV1` can only be built from an opaque
//! verified formal anchor and the complete closed record at that anchor.

use oasis7_distfs::controlled_authority::{
    formal::{FormalWorldIdentityV1, VerifiedFormalReadAnchorV1, verify_record_material},
    replicated_protocol::{ArtifactRole, ClosedRecord},
};
use oasis7_runtime::{EffectIntent, blake3_hex};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::BTreeMap;

const OUTBOX_ID_DOMAIN: &str = "oasis7.formal.effect_outbox.identity.v1";
const QUALIFIED_EFFECT_DOMAIN: &str = "oasis7.formal.effect.qualified-source.v1";
const QUALIFIED_OUTBOX_DOMAIN: &str = "oasis7.formal.effect.qualified-outbox.v1";
const MAX_EFFECTS_PER_OUTBOX: usize = 4096;
const MAX_EFFECT_OUTBOX_BYTES: usize = 16 * 1024 * 1024;
const MAX_TARGET_ID_BYTES: usize = 128;

#[cfg(test)]
#[path = "formal_effect_outbox_tests.rs"]
mod tests;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct QualifiedEffectCommitRefV1 {
    pub(super) position: u64,
    pub(super) record_id: String,
    pub(super) active_authority_id: String,
    pub(super) finalizing_authority_id: String,
}

/// External identity is world scoped. `EffectIntent::identity_key()` is only
/// an internal world-local map key and is never used as the external identity
/// by itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ExternalEffectIdentityV1 {
    pub(super) world: FormalWorldIdentityV1,
    pub(super) birth_era: u64,
    pub(super) intent_id: String,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct QualifiedEffectObligationV1 {
    pub(super) identity: ExternalEffectIdentityV1,
    /// Hash of the complete, canonical EffectIntent, including routing
    /// selectors. This is also the payload identity used by v2 receipts.
    pub(super) payload_digest: String,
    pub(super) target_id: String,
    pub(super) receiver_contract_digest: String,
    pub(super) intent: EffectIntent,
    /// The first qualified closure that exposed this obligation. Later
    /// snapshots can repeat the item, but cannot rewrite this provenance.
    pub(super) committed_at: QualifiedEffectCommitRefV1,
    // A serde DTO is not a qualification capability. This private marker is
    // only constructed after verifying the source ClosedRecord against an
    // opaque VerifiedFormalReadAnchorV1.
    qualification: QualifiedEffectSourceSeal,
}

/// Durable retry state stores only this untrusted immutable binding. It never
/// crosses the `drive_qualified_obligation` boundary; the caller must obtain
/// a fresh opaque obligation from the qualified closed record first.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct EffectObligationBindingV1 {
    pub(super) identity: ExternalEffectIdentityV1,
    pub(super) payload_digest: String,
    pub(super) target_id: String,
    pub(super) receiver_contract_digest: String,
    pub(super) intent: EffectIntent,
    /// First qualified commit provenance, retained across later observations.
    pub(super) committed_at: QualifiedEffectCommitRefV1,
}

impl EffectObligationBindingV1 {
    pub(super) fn from_qualified(obligation: &QualifiedEffectObligationV1) -> Self {
        Self {
            identity: obligation.identity.clone(),
            payload_digest: obligation.payload_digest.clone(),
            target_id: obligation.target_id.clone(),
            receiver_contract_digest: obligation.receiver_contract_digest.clone(),
            intent: obligation.intent.clone(),
            committed_at: obligation.committed_at.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct QualifiedEffectSourceV1 {
    world: FormalWorldIdentityV1,
    position: u64,
    record_id: String,
    active_authority_id: String,
    effect_outbox_root: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
struct QualifiedEffectSourceSeal {
    source: QualifiedEffectSourceV1,
    obligation_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct BlockedEffectV1 {
    pub(super) world: FormalWorldIdentityV1,
    pub(super) birth_era: Option<u64>,
    pub(super) intent_id: String,
    pub(super) committed_at: QualifiedEffectCommitRefV1,
    pub(super) reason: String,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct QualifiedEffectOutboxV1 {
    pub(super) commit: QualifiedEffectCommitRefV1,
    pub(super) obligations: Vec<QualifiedEffectObligationV1>,
    pub(super) blocked: Vec<BlockedEffectV1>,
    qualification: QualifiedEffectSourceV1,
    qualification_seal: String,
}

/// Strict delivery-aware `EffectIntent.params` schema. The complete `payload`
/// remains application data; these two selectors are the only routing inputs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DeliveryAwareEffectParametersV1 {
    schema_version: u16,
    target_id: String,
    receiver_contract_digest: String,
    payload: JsonValue,
}

impl QualifiedEffectOutboxV1 {
    pub(super) fn from_qualified_material(
        anchor: &VerifiedFormalReadAnchorV1,
        material: &ClosedRecord,
    ) -> Result<Self, String> {
        let record = anchor.head().record();
        verify_record_material(record, material)
            .map_err(|error| format!("qualified EffectOutbox closure rejected: {error}"))?;

        let outbox_root = material
            .roots
            .get(&ArtifactRole::EffectOutbox)
            .ok_or("qualified record has no EffectOutbox root")?;
        let outbox_object = material
            .objects
            .iter()
            .find(|object| &object.content_hash == outbox_root)
            .ok_or("qualified EffectOutbox object is missing")?;
        if outbox_object.bytes.is_empty() || outbox_object.bytes.len() > MAX_EFFECT_OUTBOX_BYTES {
            return Err("qualified EffectOutbox exceeds bounded artifact size".into());
        }
        let (pending, inflight): (Vec<EffectIntent>, BTreeMap<String, EffectIntent>) =
            serde_cbor::from_slice(&outbox_object.bytes)
                .map_err(|error| format!("qualified EffectOutbox decode failed: {error}"))?;
        if pending.len().saturating_add(inflight.len()) > MAX_EFFECTS_PER_OUTBOX {
            return Err("qualified EffectOutbox exceeds bounded entry count".into());
        }

        let world = record.world.clone();
        if world != anchor.authority().definition().world {
            return Err("qualified EffectOutbox world differs from verified authority".into());
        }
        let commit = QualifiedEffectCommitRefV1 {
            position: record.position,
            record_id: anchor.head().head().record_id.as_str().to_owned(),
            active_authority_id: anchor
                .authority()
                .authority_id()
                .map_err(|error| error.to_string())?,
            finalizing_authority_id: anchor.head().finalizing_authority_id().to_owned(),
        };
        let source = QualifiedEffectSourceV1 {
            world: world.clone(),
            position: record.position,
            record_id: anchor.head().head().record_id.as_str().to_owned(),
            active_authority_id: commit.active_authority_id.clone(),
            effect_outbox_root: outbox_root.clone(),
        };

        let mut seen = BTreeMap::new();
        let mut obligations = Vec::new();
        let mut blocked = Vec::new();
        for intent in pending {
            append_intent(
                &world,
                &commit,
                &source,
                intent,
                &mut seen,
                &mut obligations,
                &mut blocked,
            )?;
        }
        for (map_key, intent) in inflight {
            if map_key != intent.identity_key() {
                return Err("qualified inflight EffectIntent map key mismatch".into());
            }
            append_intent(
                &world,
                &commit,
                &source,
                intent,
                &mut seen,
                &mut obligations,
                &mut blocked,
            )?;
        }
        obligations.sort_by(|left, right| {
            (
                &left.identity.world.world_id,
                &left.identity.world.chain_id,
                &left.identity.world.genesis_digest,
                left.identity.birth_era,
                &left.identity.intent_id,
            )
                .cmp(&(
                    &right.identity.world.world_id,
                    &right.identity.world.chain_id,
                    &right.identity.world.genesis_digest,
                    right.identity.birth_era,
                    &right.identity.intent_id,
                ))
        });
        blocked.sort_by(|left, right| {
            (&left.birth_era, &left.intent_id).cmp(&(&right.birth_era, &right.intent_id))
        });
        let qualification_seal = qualified_outbox_digest(&source, &commit, &obligations, &blocked)?;
        Ok(Self {
            commit,
            obligations,
            blocked,
            qualification: source,
            qualification_seal,
        })
    }
}

fn append_intent(
    world: &FormalWorldIdentityV1,
    commit: &QualifiedEffectCommitRefV1,
    source: &QualifiedEffectSourceV1,
    intent: EffectIntent,
    seen: &mut BTreeMap<(Option<u64>, String), EffectIntent>,
    obligations: &mut Vec<QualifiedEffectObligationV1>,
    blocked: &mut Vec<BlockedEffectV1>,
) -> Result<(), String> {
    if intent.intent_id.trim().is_empty() || intent.intent_id.len() > 256 {
        return Err("qualified EffectIntent has invalid intent_id".into());
    }
    let seen_key = (intent.birth_era, intent.intent_id.clone());
    if let Some(previous) = seen.get(&seen_key) {
        if previous == &intent {
            return Ok(());
        }
        return Err(
            "qualified EffectOutbox repeats an effect identity with conflicting bytes".into(),
        );
    }
    seen.insert(seen_key, intent.clone());
    let Some(birth_era) = intent.birth_era else {
        blocked.push(blocked_effect(
            world,
            commit,
            &intent,
            "legacy EffectIntent has no committed birth_era",
        ));
        return Ok(());
    };
    let params: DeliveryAwareEffectParametersV1 =
        match serde_json::from_value(intent.params.clone()) {
            Ok(params) => params,
            Err(_) => {
                blocked.push(blocked_effect(
                    world,
                    commit,
                    &intent,
                    "EffectIntent params lack the strict delivery-aware schema",
                ));
                return Ok(());
            }
        };
    if params.schema_version != 1
        || !valid_target_id(&params.target_id)
        || !valid_digest(&params.receiver_contract_digest)
    {
        blocked.push(blocked_effect(
            world,
            commit,
            &intent,
            "EffectIntent delivery selectors are invalid or unsupported",
        ));
        return Ok(());
    }
    let payload_digest = intent
        .payload_digest()
        .map_err(|error| format!("EffectIntent payload digest failed: {error:?}"))?;
    let identity = ExternalEffectIdentityV1 {
        world: world.clone(),
        birth_era,
        intent_id: intent.intent_id.clone(),
    };
    let obligation_digest = qualified_obligation_digest(
        &identity,
        &payload_digest,
        &params.target_id,
        &params.receiver_contract_digest,
        &intent,
        commit,
        source,
    )?;
    obligations.push(QualifiedEffectObligationV1 {
        identity,
        payload_digest,
        target_id: params.target_id,
        receiver_contract_digest: params.receiver_contract_digest,
        intent,
        committed_at: commit.clone(),
        qualification: QualifiedEffectSourceSeal {
            source: source.clone(),
            obligation_digest,
        },
    });
    Ok(())
}

fn blocked_effect(
    world: &FormalWorldIdentityV1,
    commit: &QualifiedEffectCommitRefV1,
    intent: &EffectIntent,
    reason: &str,
) -> BlockedEffectV1 {
    BlockedEffectV1 {
        world: world.clone(),
        birth_era: intent.birth_era,
        intent_id: intent.intent_id.clone(),
        committed_at: commit.clone(),
        reason: reason.to_owned(),
    }
}

pub(super) fn delivery_identity_key(identity: &ExternalEffectIdentityV1) -> Result<String, String> {
    let bytes = serde_cbor::to_vec(&(
        OUTBOX_ID_DOMAIN,
        &identity.world,
        identity.birth_era,
        &identity.intent_id,
    ))
    .map_err(|error| format!("effect identity encode failed: {error}"))?;
    Ok(blake3_hex(&bytes))
}

pub(super) fn validate_qualified_effect_obligation(
    obligation: &QualifiedEffectObligationV1,
) -> Result<(), String> {
    validate_effect_obligation_binding(&EffectObligationBindingV1::from_qualified(obligation))?;
    validate_source_matches_commit(&obligation.qualification.source, &obligation.committed_at)?;
    if obligation.qualification.source.world != obligation.identity.world {
        return Err("qualified EffectIntent source world differs from its identity".into());
    }
    if !valid_digest(&obligation.qualification.source.effect_outbox_root)
        || qualified_obligation_digest(
            &obligation.identity,
            &obligation.payload_digest,
            &obligation.target_id,
            &obligation.receiver_contract_digest,
            &obligation.intent,
            &obligation.committed_at,
            &obligation.qualification.source,
        )? != obligation.qualification.obligation_digest
    {
        return Err("qualified EffectIntent source seal mismatch".into());
    }
    delivery_identity_key(&obligation.identity)?;
    Ok(())
}

pub(super) fn validate_effect_obligation_binding(
    binding: &EffectObligationBindingV1,
) -> Result<(), String> {
    if binding.intent.intent_id != binding.identity.intent_id
        || binding.intent.birth_era != Some(binding.identity.birth_era)
        || binding.intent.intent_id.trim().is_empty()
        || binding.intent.intent_id.len() > 256
    {
        return Err("EffectIntent binding identity differs from its external identity".into());
    }
    let expected_payload_digest = binding
        .intent
        .payload_digest()
        .map_err(|error| format!("EffectIntent binding digest failed: {error:?}"))?;
    if expected_payload_digest != binding.payload_digest {
        return Err("EffectIntent binding payload digest mismatch".into());
    }
    let params: DeliveryAwareEffectParametersV1 =
        serde_json::from_value(binding.intent.params.clone())
            .map_err(|_| "EffectIntent binding delivery schema is invalid".to_owned())?;
    if params.schema_version != 1
        || !valid_target_id(&params.target_id)
        || !valid_digest(&params.receiver_contract_digest)
        || params.target_id != binding.target_id
        || params.receiver_contract_digest != binding.receiver_contract_digest
    {
        return Err("EffectIntent binding route selectors differ from its params".into());
    }
    delivery_identity_key(&binding.identity)?;
    Ok(())
}

pub(super) fn validate_qualified_effect_outbox(
    outbox: &QualifiedEffectOutboxV1,
) -> Result<(), String> {
    if qualified_outbox_digest(
        &outbox.qualification,
        &outbox.commit,
        &outbox.obligations,
        &outbox.blocked,
    )? != outbox.qualification_seal
    {
        return Err("qualified EffectOutbox source seal mismatch".into());
    }
    validate_source_matches_commit(&outbox.qualification, &outbox.commit)?;
    if outbox.qualification.world
        != outbox
            .obligations
            .first()
            .map(|obligation| obligation.identity.world.clone())
            .unwrap_or_else(|| outbox.qualification.world.clone())
    {
        return Err("qualified EffectOutbox world differs from its source".into());
    }
    for obligation in &outbox.obligations {
        if obligation.committed_at != outbox.commit {
            return Err("qualified EffectIntent commit differs from its outbox".into());
        }
        validate_qualified_effect_obligation(obligation)?;
        if obligation.qualification.source != outbox.qualification {
            return Err("qualified EffectIntent came from a different closed record".into());
        }
    }
    Ok(())
}

fn qualified_outbox_digest(
    source: &QualifiedEffectSourceV1,
    commit: &QualifiedEffectCommitRefV1,
    obligations: &[QualifiedEffectObligationV1],
    blocked: &[BlockedEffectV1],
) -> Result<String, String> {
    let sealed_obligations: Vec<_> = obligations
        .iter()
        .map(|obligation| {
            (
                &obligation.identity,
                &obligation.payload_digest,
                &obligation.target_id,
                &obligation.receiver_contract_digest,
                &obligation.intent,
                &obligation.committed_at,
                &obligation.qualification,
            )
        })
        .collect();
    let bytes = serde_cbor::to_vec(&(
        QUALIFIED_OUTBOX_DOMAIN,
        source,
        commit,
        sealed_obligations,
        blocked,
    ))
    .map_err(|error| format!("qualified EffectOutbox seal encode failed: {error}"))?;
    Ok(blake3_hex(&bytes))
}

fn validate_source_matches_commit(
    source: &QualifiedEffectSourceV1,
    commit: &QualifiedEffectCommitRefV1,
) -> Result<(), String> {
    if source.position != commit.position
        || source.record_id != commit.record_id
        || source.active_authority_id != commit.active_authority_id
        || !valid_digest(&source.effect_outbox_root)
    {
        return Err("qualified Effect source seal differs from its commit reference".into());
    }
    Ok(())
}

fn qualified_obligation_digest(
    identity: &ExternalEffectIdentityV1,
    payload_digest: &str,
    target_id: &str,
    receiver_contract_digest: &str,
    intent: &EffectIntent,
    commit: &QualifiedEffectCommitRefV1,
    source: &QualifiedEffectSourceV1,
) -> Result<String, String> {
    let bytes = serde_cbor::to_vec(&(
        QUALIFIED_EFFECT_DOMAIN,
        source,
        identity,
        payload_digest,
        target_id,
        receiver_contract_digest,
        intent,
        commit,
    ))
    .map_err(|error| format!("qualified EffectIntent seal encode failed: {error}"))?;
    Ok(blake3_hex(&bytes))
}

#[cfg(test)]
pub(super) fn test_only_qualified_obligation(
    world: FormalWorldIdentityV1,
    commit: QualifiedEffectCommitRefV1,
    intent: EffectIntent,
) -> Result<QualifiedEffectObligationV1, String> {
    // Explicit test-layer fixture only: this does not exercise formal-anchor
    // or ClosedRecord qualification. Production obligations can only be
    // constructed by `from_qualified_material`.
    let source = QualifiedEffectSourceV1 {
        world: world.clone(),
        position: commit.position,
        record_id: commit.record_id.clone(),
        active_authority_id: commit.active_authority_id.clone(),
        effect_outbox_root: "f".repeat(64),
    };
    let mut seen = BTreeMap::new();
    let mut obligations = Vec::new();
    let mut blocked = Vec::new();
    append_intent(
        &world,
        &commit,
        &source,
        intent,
        &mut seen,
        &mut obligations,
        &mut blocked,
    )?;
    if !blocked.is_empty() || obligations.len() != 1 {
        return Err("test fixture EffectIntent did not satisfy delivery schema".into());
    }
    Ok(obligations.remove(0))
}

fn valid_target_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_TARGET_ID_BYTES
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"._-".contains(&byte)
        })
}

pub(super) fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
