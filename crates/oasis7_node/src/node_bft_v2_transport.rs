//! V2 certificate admission for the existing single-datagram UDP transport.

use crate::NodeError;
use crate::gossip_udp::GossipMessage;
use oasis7_consensus::tendermint_v2::{BFT_MAX_ROUNDS_PER_HEIGHT_V2, BftValidatorSetV2};
use oasis7_proto::distributed_bft::v2::{
    BFT_PROTOCOL_VERSION_V2, BftBlockV2, BftCommitCertificateV2, BftProposalHeaderV2,
    BftProposalV2, BftValidRoundEvidenceV2, BftValidatorTransitionV2, BftVotePhaseV2, BftVoteV2,
    BftVoteValueV2,
};

const UDP_MAX_PAYLOAD_BYTES: usize = 65_507;

pub(super) fn validate_capacity(
    block: &BftBlockV2,
    set: &BftValidatorSetV2,
) -> Result<(), NodeError> {
    let block_id = block.block_id().map_err(error)?;
    let candidates = set
        .validators()
        .values()
        .map(|validator| {
            serde_json::to_vec(&validator.validator_id)
                .map(|encoded| (encoded.len(), validator))
                .map_err(|encode_error| error(format!("encode validator identity: {encode_error}")))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let proposer = candidates
        .into_iter()
        .max_by_key(|(encoded_len, _)| *encoded_len)
        .map(|(_, validator)| validator)
        .ok_or_else(|| error("empty BFT V2 validator set"))?;
    let max_round = BFT_MAX_ROUNDS_PER_HEIGHT_V2;
    let base_header = |validator: &oasis7_proto::distributed_bft::v2::BftValidatorV2,
                       round,
                       valid_round| BftProposalHeaderV2 {
        protocol_version: BFT_PROTOCOL_VERSION_V2,
        scope: block.scope.clone(),
        height: block.height,
        round,
        active_validator_set_hash: set.set_hash().to_string(),
        schedule_policy_hash: block.schedule_policy_hash.clone(),
        proposer_id: validator.validator_id.clone(),
        block_id: block_id.clone(),
        valid_round,
        public_key_hex: validator.public_key_hex.clone(),
        signature_hex: "ff".repeat(64),
    };
    let evidence = BftValidRoundEvidenceV2 {
        proposal: base_header(proposer, max_round - 1, None),
        prevotes: set
            .validators()
            .values()
            .map(|validator| {
                vote(
                    block,
                    set,
                    validator,
                    max_round - 1,
                    BftVotePhaseV2::Prevote,
                    &block_id,
                )
            })
            .collect(),
    };
    let proposal = BftProposalV2 {
        header: base_header(proposer, max_round, Some(max_round - 1)),
        block: block.clone(),
        valid_round_evidence: Some(evidence),
    };
    let certificate = BftCommitCertificateV2 {
        proposal: Box::new(proposal),
        precommits: set
            .validators()
            .values()
            .map(|validator| {
                vote(
                    block,
                    set,
                    validator,
                    max_round,
                    BftVotePhaseV2::Precommit,
                    &block_id,
                )
            })
            .collect(),
    };
    let bytes = serde_json::to_vec(&GossipMessage::BftV2 {
        message: oasis7_proto::distributed_bft::v2::BftMessageV2::Commit(certificate),
    })
    .map_err(|encode_error| error(format!("encode BFT V2 UDP envelope: {encode_error}")))?
    .len();
    if bytes > UDP_MAX_PAYLOAD_BYTES {
        return Err(error(format!(
            "BFT V2 candidate cannot fit a complete maximum-round QC in one UDP datagram: bytes={bytes} max={UDP_MAX_PAYLOAD_BYTES}; no signature released"
        )));
    }
    Ok(())
}

/// Check both the candidate's QC and the first QC signed by a scheduled set.
/// The future probe uses maximum numeric widths so an irreversible transition
/// cannot install a set whose first certificate cannot traverse this UDP link.
pub(super) fn validate_candidate_capacity(
    block: &BftBlockV2,
    set: &BftValidatorSetV2,
) -> Result<(), NodeError> {
    validate_capacity(block, set)?;
    let Some(transition) = &block.transition else {
        return Ok(());
    };
    validate_future_transition_capacity(block, transition)
}

fn validate_future_transition_capacity(
    source_block: &BftBlockV2,
    transition: &BftValidatorTransitionV2,
) -> Result<(), NodeError> {
    let future_set = BftValidatorSetV2::new(
        source_block.scope.clone(),
        transition.new_validators.clone(),
    )
    .map_err(error)?;
    if future_set.set_hash() != transition.new_validator_set_hash {
        return Err(error(
            "BFT V2 future validator set hash differs from transition",
        ));
    }
    let future_block = BftBlockV2 {
        protocol_version: BFT_PROTOCOL_VERSION_V2,
        scope: source_block.scope.clone(),
        height: u64::MAX,
        parent_commit_ref: "ff".repeat(32),
        authority_epoch: u64::MAX,
        active_validator_set_hash: future_set.set_hash().to_string(),
        schedule_policy_hash: source_block.schedule_policy_hash.clone(),
        manifest_hash: "ee".repeat(32),
        post_manifest_hash: "dd".repeat(32),
        actions_cbor: vec![0x80],
        action_root: "cc".repeat(32),
        event_root: "bb".repeat(32),
        execution_block_hash: "aa".repeat(32),
        execution_state_root: "99".repeat(32),
        state_root: "88".repeat(32),
        transition: None,
        transition_ref: None,
    };
    validate_capacity(&future_block, &future_set)
}

pub(super) fn validate_configuration(set: &BftValidatorSetV2) -> Result<(), NodeError> {
    let block = BftBlockV2 {
        protocol_version: BFT_PROTOCOL_VERSION_V2,
        scope: set.scope().clone(),
        height: u64::MAX,
        parent_commit_ref: set.scope().genesis_hash.clone(),
        authority_epoch: u64::MAX,
        active_validator_set_hash: set.set_hash().to_string(),
        schedule_policy_hash: "ff".repeat(32),
        manifest_hash: "ff".repeat(32),
        post_manifest_hash: "ee".repeat(32),
        actions_cbor: vec![0x80],
        action_root: "dd".repeat(32),
        event_root: "cc".repeat(32),
        execution_block_hash: "bb".repeat(32),
        execution_state_root: "aa".repeat(32),
        state_root: "99".repeat(32),
        transition: None,
        transition_ref: None,
    };
    validate_capacity(&block, set)
}

fn vote(
    block: &BftBlockV2,
    set: &BftValidatorSetV2,
    validator: &oasis7_proto::distributed_bft::v2::BftValidatorV2,
    round: u64,
    phase: BftVotePhaseV2,
    block_id: &str,
) -> BftVoteV2 {
    BftVoteV2 {
        protocol_version: BFT_PROTOCOL_VERSION_V2,
        scope: block.scope.clone(),
        height: block.height,
        round,
        phase,
        active_validator_set_hash: set.set_hash().to_string(),
        schedule_policy_hash: block.schedule_policy_hash.clone(),
        value: BftVoteValueV2::Block {
            block_id: block_id.to_string(),
        },
        validator_id: validator.validator_id.clone(),
        public_key_hex: validator.public_key_hex.clone(),
        signature_hex: "ff".repeat(64),
    }
}

fn error(reason: impl Into<String>) -> NodeError {
    NodeError::Consensus {
        reason: reason.into(),
    }
}

#[cfg(test)]
mod tests {
    use ed25519_dalek::SigningKey;
    use oasis7_consensus::tendermint_v2::BftValidatorSetV2;
    use oasis7_proto::distributed_bft::v2::{
        BFT_MAX_VALIDATORS_V2, BFT_PROTOCOL_VERSION_V2, BftBlockV2, BftScopeV2,
        BftValidatorTransitionV2, BftValidatorV2,
    };

    use super::{validate_candidate_capacity, validate_capacity, validate_configuration};

    fn scope() -> BftScopeV2 {
        BftScopeV2 {
            world_id: "capacity-world-v2".into(),
            chain_id: "capacity-chain-v2".into(),
            genesis_hash: "11".repeat(32),
        }
    }

    fn set(ids: impl IntoIterator<Item = String>) -> BftValidatorSetV2 {
        let validators = ids
            .into_iter()
            .enumerate()
            .map(|(index, validator_id)| {
                let mut seed = [0_u8; 32];
                let ordinal = u16::try_from(index + 1).expect("bounded validator index");
                seed[..2].copy_from_slice(&ordinal.to_le_bytes());
                seed[31] = 0x7f;
                let key = SigningKey::from_bytes(&seed);
                BftValidatorV2 {
                    validator_id,
                    public_key_hex: hex::encode(key.verifying_key().to_bytes()),
                    voting_power: 1,
                }
            })
            .collect();
        BftValidatorSetV2::new(scope(), validators).expect("valid V2 test set")
    }

    fn block(set: &BftValidatorSetV2, height: u64, epoch: u64, action_bytes: usize) -> BftBlockV2 {
        BftBlockV2 {
            protocol_version: BFT_PROTOCOL_VERSION_V2,
            scope: set.scope().clone(),
            height,
            parent_commit_ref: "22".repeat(32),
            authority_epoch: epoch,
            active_validator_set_hash: set.set_hash().to_string(),
            schedule_policy_hash: "33".repeat(32),
            manifest_hash: "44".repeat(32),
            post_manifest_hash: "55".repeat(32),
            actions_cbor: vec![1; action_bytes],
            action_root: "66".repeat(32),
            event_root: "77".repeat(32),
            execution_block_hash: "88".repeat(32),
            execution_state_root: "99".repeat(32),
            state_root: "aa".repeat(32),
            transition: None,
            transition_ref: None,
        }
    }

    fn assert_udp_capacity_error(error: crate::NodeError) {
        let message = error.to_string();
        assert!(
            message.contains("65507") && message.contains("UDP datagram"),
            "expected a UDP capacity rejection, got: {message}"
        );
    }

    #[test]
    fn proposer_identity_capacity_uses_longest_json_encoded_id_not_map_order() {
        let longest = format!("{}x", "\"".repeat(255));
        let validators = [
            longest.clone(),
            "a-short".into(),
            "b-short".into(),
            "z-short".into(),
        ];
        let active = set(validators);
        let selected = active
            .validators()
            .values()
            .max_by_key(|validator| {
                serde_json::to_vec(&validator.validator_id)
                    .expect("id JSON")
                    .len()
            })
            .expect("nonempty set");
        assert_eq!(selected.validator_id, longest);
        assert!(selected.validator_id.as_str() < "z-short");
        validate_configuration(&active).expect("four-member escaped set still fits");
    }

    #[test]
    fn configured_udp_capacity_uses_maximum_height_and_epoch_widths() {
        let active = set((0..4).map(|index| format!("validator-{index}")));
        let mut fitting = 0;
        let mut failing = 65_508;
        while failing - fitting > 1 {
            let candidate = fitting + (failing - fitting) / 2;
            if validate_capacity(&block(&active, 1, 1, candidate), &active).is_ok() {
                fitting = candidate;
            } else {
                failing = candidate;
            }
        }
        assert!(fitting > 0);
        validate_capacity(&block(&active, 1, 1, fitting), &active)
            .expect("low-width complete certificate fits");
        assert_udp_capacity_error(
            validate_capacity(&block(&active, u64::MAX, u64::MAX, fitting), &active)
                .expect_err("maximum-width history fields must be included"),
        );
        validate_configuration(&active)
            .expect("a fitting empty-action configuration remains admissible");

        let oversized =
            set((0..BFT_MAX_VALIDATORS_V2).map(|index| format!("{}-{index:03}", "\"".repeat(250))));
        assert_udp_capacity_error(
            validate_configuration(&oversized)
                .expect_err("startup must reject a set whose maximum QC exceeds UDP"),
        );
    }

    fn transition(old: &BftValidatorSetV2, new: &BftValidatorSetV2) -> BftValidatorTransitionV2 {
        BftValidatorTransitionV2 {
            source_commit_ref: "22".repeat(32),
            source_state_root: "99".repeat(32),
            source_profile: "controlled-bft".into(),
            source_epoch: 1,
            target_epoch: 3,
            epoch_length_ticks: 4,
            mapping_ruleset_hash: "aa".repeat(32),
            scheduled_height: 10,
            activation_height: 12,
            old_validator_set_hash: old.set_hash().to_string(),
            new_validators: new.validators().values().cloned().collect(),
            new_validator_set_hash: new.set_hash().to_string(),
            governance_action_digest: "bb".repeat(32),
            projected_registry_digest: "cc".repeat(32),
        }
    }

    fn transition_block(
        old: &BftValidatorSetV2,
        transition: BftValidatorTransitionV2,
    ) -> BftBlockV2 {
        let mut block = block(old, 10, 1, 1);
        block.transition = Some(transition);
        block
    }

    #[test]
    fn transition_preflights_the_scheduled_set_before_signing_irreversible_source() {
        let old = set((0..4).map(|index| format!("old-{index}")));
        let small_future = set((4..8).map(|index| format!("new-{index}")));
        let fitting_transition = transition(&old, &small_future);
        let fitting_source = transition_block(&old, fitting_transition);
        validate_candidate_capacity(&fitting_source, &old)
            .expect("source and first future-set certificate both fit");

        let maximum_future =
            set((0..BFT_MAX_VALIDATORS_V2).map(|index| format!("future-{index:03}")));
        let oversized_source = transition_block(&old, transition(&old, &maximum_future));
        validate_capacity(&oversized_source, &old)
            .expect("transition source QC itself fits the existing set");
        assert_udp_capacity_error(
            validate_candidate_capacity(&oversized_source, &old)
                .expect_err("do not commit a transition to an unusable UDP set"),
        );
    }
}
