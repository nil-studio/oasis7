//! Admission bound for the engineering UDP transport. The unsigned sizing
//! envelope never establishes a verified certificate or releases signatures.
use super::*;
use oasis7_proto::distributed_bft::{
    BFT_PROTOCOL_VERSION_V1, BftBlockV1, BftProposalHeaderV1, BftValidRoundEvidenceV1,
    BftVotePhaseV1, BftVoteV1, BftVoteValueV1,
};

pub(super) fn validate_capacity(
    block: &BftBlockV1,
    set: &BftValidatorSetV1,
) -> Result<(), NodeError> {
    let candidates = set
        .validators()
        .values()
        .map(|v| {
            serde_json::to_vec(&v.validator_id)
                .map(|bytes| (bytes.len(), v))
                .map_err(|e| error(e.to_string()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let proposer = candidates
        .into_iter()
        .max_by_key(|(size, _)| *size)
        .map(|(_, v)| v)
        .ok_or_else(|| error("empty BFT set"))?;
    let round = oasis7_consensus::tendermint_v1::BFT_MAX_ROUNDS_PER_HEIGHT_V1;
    let header = BftProposalHeaderV1 {
        protocol_version: BFT_PROTOCOL_VERSION_V1,
        scope: block.scope.clone(),
        height: block.height,
        round,
        validator_set_hash: set.set_hash().into(),
        proposer_id: proposer.validator_id.clone(),
        block_id: "ff".repeat(32),
        valid_round: Some(round - 1),
        public_key_hex: proposer.public_key_hex.clone(),
        signature_hex: "ff".repeat(64),
    };
    let votes = |phase| {
        set.validators()
            .values()
            .map(|v| BftVoteV1 {
                protocol_version: BFT_PROTOCOL_VERSION_V1,
                scope: block.scope.clone(),
                height: block.height,
                round,
                phase,
                validator_set_hash: set.set_hash().into(),
                value: BftVoteValueV1::Block {
                    block_id: "ff".repeat(32),
                },
                validator_id: v.validator_id.clone(),
                public_key_hex: v.public_key_hex.clone(),
                signature_hex: "ff".repeat(64),
            })
            .collect()
    };
    let qc = BftCommitCertificateV1 {
        proposal: Box::new(BftProposalV1 {
            header: header.clone(),
            block: block.clone(),
            valid_round_evidence: Some(BftValidRoundEvidenceV1 {
                proposal: header,
                prevotes: votes(BftVotePhaseV1::Prevote),
            }),
        }),
        precommits: votes(BftVotePhaseV1::Precommit),
    };
    let bytes = serde_json::to_vec(&GossipMessage::Bft {
        message: BftMessageV1::Commit(qc),
    })
    .map_err(|e| error(e.to_string()))?
    .len();
    if bytes > 65_507 {
        return Err(error(format!(
            "BFT candidate exceeds engineering UDP certificate capacity: bytes={bytes} max=65507; no signature released"
        )));
    }
    Ok(())
}

pub(super) fn validate_configuration(
    set: &BftValidatorSetV1,
    authority_epoch: u64,
) -> Result<(), NodeError> {
    let block = BftBlockV1 {
        scope: set.scope().clone(),
        height: oasis7_proto::distributed_bft::BFT_MAX_COMMIT_HISTORY_HEIGHTS_V1 as u64,
        parent_commit_ref: "ff".repeat(32),
        authority_epoch,
        manifest_hash: "ff".repeat(32),
        actions_cbor: vec![0x80],
        action_root: "ff".repeat(32),
        event_root: "ff".repeat(32),
        execution_block_hash: "ff".repeat(32),
        execution_state_root: "ff".repeat(32),
        state_root: "ff".repeat(32),
    };
    validate_capacity(&block, set)
}

#[cfg(test)]
mod tests {
    use super::{validate_capacity, validate_configuration};
    use ed25519_dalek::SigningKey;
    use oasis7_consensus::tendermint_v1::{BftValidatorSetV1, BftValidatorV1};
    use oasis7_proto::distributed_bft::{BFT_MAX_VALIDATORS_V1, BftBlockV1, BftScopeV1};

    fn scope() -> BftScopeV1 {
        BftScopeV1 {
            world_id: "capacity-world".into(),
            chain_id: "capacity-chain".into(),
            genesis_hash: "11".repeat(32),
        }
    }

    fn validator_set(ids: impl IntoIterator<Item = String>) -> BftValidatorSetV1 {
        let validators = ids
            .into_iter()
            .enumerate()
            .map(|(index, validator_id)| {
                let mut seed = [0_u8; 32];
                let ordinal = u16::try_from(index + 1).expect("test validator index fits u16");
                seed[..2].copy_from_slice(&ordinal.to_le_bytes());
                seed[31] = 0x7f;
                let key = SigningKey::from_bytes(&seed);
                BftValidatorV1 {
                    validator_id,
                    public_key_hex: hex::encode(key.verifying_key().to_bytes()),
                    voting_power: 1,
                }
            })
            .collect();
        BftValidatorSetV1::new(scope(), validators).expect("valid bounded test set")
    }

    fn block(set: &BftValidatorSetV1, action_bytes: usize) -> BftBlockV1 {
        BftBlockV1 {
            scope: set.scope().clone(),
            height: 1,
            parent_commit_ref: "22".repeat(32),
            authority_epoch: 1,
            manifest_hash: "33".repeat(32),
            actions_cbor: vec![1; action_bytes],
            action_root: "44".repeat(32),
            event_root: "55".repeat(32),
            execution_block_hash: "66".repeat(32),
            execution_state_root: "77".repeat(32),
            state_root: "88".repeat(32),
        }
    }

    fn assert_udp_capacity_error(error: crate::NodeError) {
        let message = error.to_string();
        assert!(
            message.contains("65507") && message.contains("UDP certificate capacity"),
            "expected a UDP envelope capacity rejection, got: {message}"
        );
    }

    #[test]
    fn small_static_set_configuration_fits_complete_udp_certificate() {
        let set = validator_set((0..4).map(|index| format!("validator-{index}")));

        validate_configuration(&set, 1).expect("small static set should fit UDP transport");
    }

    #[test]
    fn complete_qc_with_large_block_payload_is_rejected_before_signing() {
        let set = validator_set((0..4).map(|index| format!("validator-{index}")));

        let error = validate_capacity(&block(&set, 40_000), &set)
            .expect_err("complete JSON QC envelope must exceed UDP datagram limit");
        assert_udp_capacity_error(error);
    }

    #[test]
    fn maximum_validator_set_is_rejected_when_its_complete_qc_cannot_fit() {
        let set = validator_set((0..BFT_MAX_VALIDATORS_V1).map(|index| format!("v-{index}")));
        assert_eq!(set.validators().len(), BFT_MAX_VALIDATORS_V1);

        let error = validate_configuration(&set, 1)
            .expect_err("256-member complete certificate cannot fit one UDP datagram");
        assert_udp_capacity_error(error);
    }

    #[test]
    fn capacity_counts_json_escaping_in_validator_ids() {
        const ID_LEN: usize = 255;
        let plain = validator_set(
            (0..4).map(|index| format!("plain-{index:02}-{}", "a".repeat(ID_LEN - 9))),
        );
        let escaped = validator_set((0..4).map(|index| {
            format!(
                "{}{}",
                "\"".repeat(ID_LEN - 1),
                char::from(b'a' + index as u8)
            )
        }));
        assert!(
            plain
                .validators()
                .values()
                .all(|validator| validator.validator_id.len() == ID_LEN)
        );
        assert!(
            escaped
                .validators()
                .values()
                .all(|validator| validator.validator_id.len() == ID_LEN)
        );
        let plain_id = plain
            .validators()
            .values()
            .next()
            .expect("plain validator exists");
        let escaped_id = escaped
            .validators()
            .values()
            .next()
            .expect("escaped validator exists");
        let plain_id_json = serde_json::to_vec(&plain_id.validator_id).expect("plain id JSON");
        let escaped_id_json =
            serde_json::to_vec(&escaped_id.validator_id).expect("escaped id JSON");
        assert!(escaped_id_json.len() > plain_id_json.len() + 200);

        let mut fitting = 0;
        let mut failing = 65_508;
        while failing - fitting > 1 {
            let candidate = fitting + (failing - fitting) / 2;
            if validate_capacity(&block(&plain, candidate), &plain).is_ok() {
                fitting = candidate;
            } else {
                failing = candidate;
            }
        }
        assert!(
            fitting > 0,
            "plain four-validator envelope should have payload room"
        );
        validate_capacity(&block(&plain, fitting), &plain)
            .expect("largest plain-id payload should fit");
        let error = validate_capacity(&block(&escaped, fitting), &escaped)
            .expect_err("JSON escaping must count toward the full QC envelope");
        assert_udp_capacity_error(error);
    }
}
