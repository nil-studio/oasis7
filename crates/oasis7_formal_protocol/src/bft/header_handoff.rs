//! Explicit wire-3 admission from an independently qualified controlled H.
use oasis7_proto::distributed_bft::{
    v2::{BftScopeV2, BftValidatorV2},
    v3::{BFT_MAX_SIGNED_MATERIAL_BYTES_V3, BFT_MAX_VALIDATORS_V3, BFT_PROTOCOL_VERSION_V3},
};
use serde::{
    Deserialize, Deserializer, Serialize,
    de::{self, SeqAccess, Visitor},
};
use std::fmt;

use crate::controlled_authority::formal::{FormalHeadRefV1, VerifiedFormalReadAnchorV1};

use super::{
    BFT_FORMAL_PROFILE_V1, BFT_FORMAL_PROFILE_VERSION_V1, BFT_FORMAL_TICK_MAPPING_RULESET_V1,
    BftSchedulePolicyV2, BftValidatorSetV2, BoundFormalBftHeaderV3,
    formal_handoff::{
        canonical_bytes, decode_canonical, domain_digest, validate_hash,
        validate_target_configuration, verify_handoff_fields,
    },
    header_certificate::BftHeaderHeightScheduleV3,
    verified_commit::{BftHeightProposerScheduleV2, FormalHandoffScheduleAnchorV1},
};

const TARGET_DOMAIN: &str = "oasis7.formal.bft.profile_target.v2";
const ACTIVATION_DOMAIN: &str = "oasis7.formal.bft.handoff_activation.v2";

fn bounded_validators<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<BftValidatorV2>, D::Error> {
    struct Validators;
    impl<'de> Visitor<'de> for Validators {
        type Value = Vec<BftValidatorV2>;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("a bounded BFT validator sequence")
        }

        fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
            if seq
                .size_hint()
                .is_some_and(|size| size > BFT_MAX_VALIDATORS_V3)
            {
                return Err(de::Error::custom(
                    "BFT target validator count exceeds bound",
                ));
            }
            let mut validators = Vec::new();
            while let Some(validator) = seq.next_element()? {
                if validators.len() == BFT_MAX_VALIDATORS_V3 {
                    return Err(de::Error::custom(
                        "BFT target validator count exceeds bound",
                    ));
                }
                validators.push(validator);
            }
            Ok(validators)
        }
    }
    deserializer.deserialize_seq(Validators)
}

/// Logical execution profile 1 with an explicitly different signed identity wire.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftProfileTargetV2 {
    pub schema_version: u32,
    pub profile: String,
    pub profile_version: u32,
    pub wire_version: u16,
    pub scope: BftScopeV2,
    pub authority_epoch: u64,
    #[serde(deserialize_with = "bounded_validators")]
    pub initial_validators: Vec<BftValidatorV2>,
    pub schedule_policy: BftSchedulePolicyV2,
    pub tick_mapping_ruleset: String,
}

impl BftProfileTargetV2 {
    pub fn new(
        scope: BftScopeV2,
        authority_epoch: u64,
        initial_validators: Vec<BftValidatorV2>,
        schedule_policy: BftSchedulePolicyV2,
    ) -> Result<Self, String> {
        let target = Self {
            schema_version: 2,
            profile: BFT_FORMAL_PROFILE_V1.into(),
            profile_version: BFT_FORMAL_PROFILE_VERSION_V1,
            wire_version: BFT_PROTOCOL_VERSION_V3,
            scope,
            authority_epoch,
            initial_validators,
            schedule_policy,
            tick_mapping_ruleset: BFT_FORMAL_TICK_MAPPING_RULESET_V1.into(),
        };
        target.validate()?;
        Ok(target)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != 2
            || self.profile != BFT_FORMAL_PROFILE_V1
            || self.profile_version != BFT_FORMAL_PROFILE_VERSION_V1
            || self.wire_version != BFT_PROTOCOL_VERSION_V3
            || self.authority_epoch == 0
            || self.tick_mapping_ruleset != BFT_FORMAL_TICK_MAPPING_RULESET_V1
            || self.initial_validators.is_empty()
            || self.initial_validators.len() > BFT_MAX_VALIDATORS_V3
        {
            return Err("unsupported formal BFT header target configuration".into());
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
        canonical_bytes(self, BFT_MAX_SIGNED_MATERIAL_BYTES_V3, "BFT wire-3 target")
    }

    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, String> {
        let value: Self =
            decode_canonical(bytes, BFT_MAX_SIGNED_MATERIAL_BYTES_V3, "BFT wire-3 target")?;
        value.validate()?;
        Ok(value)
    }

    pub fn target_digest(&self) -> Result<String, String> {
        Ok(domain_digest(TARGET_DOMAIN, &self.canonical_bytes()?))
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

/// Qualified admission only. Actual restoration and candidate execution remain
/// driver obligations; this type cannot be obtained from a wire-2 target.
#[derive(Debug, Clone)]
pub struct VerifiedBftHandoffTargetV2 {
    target: BftProfileTargetV2,
    target_digest: String,
    activation_digest: String,
    handoff_head: FormalHeadRefV1,
    raw_snapshot_root: String,
    governing_manifest_digest: String,
    finalizing_authority_id: String,
}

impl VerifiedBftHandoffTargetV2 {
    pub fn target(&self) -> &BftProfileTargetV2 {
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
    pub fn raw_snapshot_root(&self) -> &str {
        &self.raw_snapshot_root
    }
    pub fn governing_manifest_digest(&self) -> &str {
        &self.governing_manifest_digest
    }
    pub fn finalizing_authority_id(&self) -> &str {
        &self.finalizing_authority_id
    }

    pub fn activation_height(&self) -> Result<u64, String> {
        self.handoff_head
            .position
            .checked_add(1)
            .ok_or_else(|| "formal BFT header activation height overflow".into())
    }

    pub fn initial_schedule_for_commitment(
        &self,
        bound: &BoundFormalBftHeaderV3,
    ) -> Result<BftHeaderHeightScheduleV3, String> {
        let commitment = bound.commitment();
        if commitment.activation_digest != self.activation_digest
            || commitment.scope != self.target.scope
            || commitment.height != self.activation_height()?
            || commitment.parent_commit_ref != self.handoff_head.record_id
            || commitment.parent_tick != self.handoff_head.position
            || commitment.authority_epoch != self.target.authority_epoch
            || commitment.active_validator_set_hash != self.target.initial_validator_set_hash()?
            || commitment.schedule_policy_hash != self.target.schedule_policy_hash()?
        {
            return Err("formal BFT header commitment does not bind the H handoff".into());
        }
        self.initial_schedule_after_handoff_replay(
            &commitment.parent_semantic_state_root,
            &commitment.manifest_hash,
        )
    }

    /// Caller must independently restore H and authenticate both supplied roots.
    pub fn initial_schedule_after_handoff_replay(
        &self,
        parent_semantic_root: &str,
        parent_manifest_hash: &str,
    ) -> Result<BftHeaderHeightScheduleV3, String> {
        validate_hash("replayed H World manifest", parent_manifest_hash)?;
        let inner = BftHeightProposerScheduleV2::from_formal_handoff_v1(
            self.target.validator_set()?,
            self.target.schedule_policy.clone(),
            FormalHandoffScheduleAnchorV1 {
                authority_epoch: self.target.authority_epoch,
                height: self.activation_height()?,
                parent_commit_ref: self.handoff_head.record_id.clone(),
                parent_tick: self.handoff_head.position,
                parent_state_root: parent_semantic_root.into(),
                parent_manifest_hash: parent_manifest_hash.into(),
            },
        )?;
        Ok(BftHeaderHeightScheduleV3::from_handoff(
            inner,
            self.activation_digest.clone(),
        ))
    }
}

pub fn verify_formal_bft_handoff_v2(
    anchor: &VerifiedFormalReadAnchorV1,
    target: BftProfileTargetV2,
) -> Result<VerifiedBftHandoffTargetV2, String> {
    target.validate()?;
    let target_digest = target.target_digest()?;
    let fields = verify_handoff_fields(
        anchor,
        &target.scope,
        &target.profile,
        target.profile_version,
        &target_digest,
    )?;
    #[derive(Serialize)]
    struct Activation<'a> {
        domain: &'static str,
        schema_version: u32,
        target_digest: &'a str,
        handoff_head: &'a FormalHeadRefV1,
        world: &'a crate::controlled_authority::formal::FormalWorldIdentityV1,
        raw_snapshot_root: &'a str,
        governing_manifest_digest: &'a str,
        finalizing_authority_id: &'a str,
    }
    let payload = Activation {
        domain: ACTIVATION_DOMAIN,
        schema_version: 2,
        target_digest: &target_digest,
        handoff_head: &fields.head,
        world: &fields.world,
        raw_snapshot_root: &fields.raw_snapshot_root,
        governing_manifest_digest: &fields.governing_manifest_digest,
        finalizing_authority_id: &fields.finalizing_authority_id,
    };
    let activation_digest = blake3::hash(&canonical_bytes(
        &payload,
        16 * 1024,
        "formal BFT wire-3 activation",
    )?)
    .to_hex()
    .to_string();
    Ok(VerifiedBftHandoffTargetV2 {
        target,
        target_digest,
        activation_digest,
        handoff_head: fields.head,
        raw_snapshot_root: fields.raw_snapshot_root,
        governing_manifest_digest: fields.governing_manifest_digest,
        finalizing_authority_id: fields.finalizing_authority_id,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_validator_declaration_is_rejected_before_elements() {
        let mut bytes = serde_cbor::to_vec(&std::collections::BTreeMap::from([(
            "initial_validators",
            Vec::<BftValidatorV2>::new(),
        )]))
        .unwrap();
        assert_eq!(bytes.pop(), Some(0x80));
        // The declaration says 257 elements, but contains none. The count
        // bound must reject it before either allocation or reading an element.
        bytes.extend_from_slice(&[0x99, 0x01, 0x01]);
        let error = BftProfileTargetV2::decode_canonical(&bytes).unwrap_err();
        assert!(error.contains("validator count exceeds bound"), "{error}");
    }
}
