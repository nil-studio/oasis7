use oasis7::geometry::GeoPos;
use oasis7::observability::emit_stderr_or_event;
use oasis7_runtime::{
    Action as RuntimeAction, EpochSettlementReport, NodeRewardMintRecord, World as RuntimeWorld,
    WorldError, reward_redeem_signature_v1,
};
use tracing::Level;

pub(super) fn build_reward_settlement_mint_records(
    reward_world: &RuntimeWorld,
    report: &EpochSettlementReport,
    signer_node_id: &str,
    signer_private_key_hex: &str,
) -> Result<Vec<NodeRewardMintRecord>, WorldError> {
    let mut preview_world = reward_world.clone();
    preview_world.apply_node_points_settlement_mint_v2(
        report,
        signer_node_id,
        signer_private_key_hex,
    )
}

pub(super) fn auto_redeem_runtime_rewards(
    reward_world: &mut RuntimeWorld,
    minted_records: &[NodeRewardMintRecord],
    signer_node_id: &str,
    signer_private_key_hex: &str,
) {
    let signer_public_key = match reward_world.node_identity_public_key(signer_node_id) {
        Some(key) => key.to_string(),
        None => {
            emit_stderr_or_event(
                Level::WARN,
                format!(
                    "reward runtime auto-redeem skipped: signer identity not bound: {}",
                    signer_node_id
                )
                .as_str(),
                "reward runtime auto-redeem signer binding missing",
            );
            return;
        }
    };

    for record in minted_records {
        let node_id = record.node_id.as_str();
        if !reward_world.state().agents.contains_key(node_id) {
            reward_world.submit_action(RuntimeAction::RegisterAgent {
                agent_id: node_id.to_string(),
                pos: GeoPos::new(0, 0, 0),
            });
            if let Err(err) = reward_world.step() {
                emit_stderr_or_event(
                    Level::WARN,
                    format!("reward runtime register auto-redeem agent failed: {err:?}").as_str(),
                    "reward runtime auto-redeem agent registration failed",
                );
                continue;
            }
        }

        let redeem_credits = reward_world.node_power_credit_balance(node_id);
        if redeem_credits == 0 {
            continue;
        }
        let nonce = reward_world
            .node_last_redeem_nonce(node_id)
            .unwrap_or(0)
            .saturating_add(1);
        let signature = match reward_redeem_signature_v1(
            node_id,
            node_id,
            redeem_credits,
            nonce,
            signer_node_id,
            signer_public_key.as_str(),
            signer_private_key_hex,
        ) {
            Ok(signature) => signature,
            Err(err) => {
                emit_stderr_or_event(
                    Level::WARN,
                    format!(
                        "reward runtime auto-redeem skipped for {}: sign failed: {}",
                        node_id, err
                    )
                    .as_str(),
                    "reward runtime auto-redeem signing failed",
                );
                continue;
            }
        };
        reward_world.submit_action(RuntimeAction::RedeemPowerSigned {
            node_id: node_id.to_string(),
            target_agent_id: node_id.to_string(),
            redeem_credits,
            nonce,
            signer_node_id: signer_node_id.to_string(),
            signature,
        });
        if let Err(err) = reward_world.step() {
            emit_stderr_or_event(
                Level::WARN,
                format!("reward runtime auto-redeem failed: {err:?}").as_str(),
                "reward runtime auto-redeem apply failed",
            );
        }
    }
}
