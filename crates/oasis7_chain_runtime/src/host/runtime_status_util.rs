use super::*;

pub(super) fn consensus_status_to_string(status: PosConsensusStatus) -> String {
    match status {
        PosConsensusStatus::Pending => "pending".to_string(),
        PosConsensusStatus::Committed => "committed".to_string(),
        PosConsensusStatus::Rejected => "rejected".to_string(),
    }
}

pub(super) fn now_unix_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

pub(super) fn print_runtime_ready_summary(
    options: &CliOptions,
    paths: &RuntimePaths,
    status_host: &str,
    status_port: u16,
) {
    println!(
        concat!(
            "oasis7_chain_runtime ready.\n",
            "- node_id: {}\n",
            "- world_id: {}\n",
            "- storage_profile: {}\n",
            "- role: {}\n",
            "- status: http://{}:{}/v1/chain/status\n",
            "- balances: http://{}:{}/v1/chain/balances\n",
            "- feedback_submit: http://{}:{}/v1/chain/feedback/submit\n",
            "- gameplay_submit: http://{}:{}/v1/chain/gameplay/submit\n",
            "- agent_claim_submit: http://{}:{}/v1/chain/agent-claim/submit\n",
            "- module_release_attestation_submit: http://{}:{}/v1/chain/module-release/attestation/submit\n",
            "- reward_runtime: {} ({})\n",
            "Press Ctrl+C to stop."
        ),
        options.node_id,
        options.world_id,
        options.storage_profile.as_str(),
        options.node_role.as_str(),
        status_host,
        status_port,
        status_host,
        status_port,
        status_host,
        status_port,
        status_host,
        status_port,
        status_host,
        status_port,
        status_host,
        status_port,
        if options.reward_runtime_enabled {
            "enabled"
        } else {
            "disabled"
        },
        paths.reward_runtime_report_dir.display()
    );
}
