//! Converts the shared typed engineering contract into validated runtime
//! settings. The configuration never accepts a network-tier manifest or argv.
use super::{
    CliOptions,
    cli::{TrafficProfile, default_p2p_node_role, validate_effective_options},
};
use oasis7::development_node_config::*;
use oasis7_node::{NodeHolePunchViability, NodeRole, NodeUserMode, PosValidator};
use std::{
    collections::BTreeSet,
    io::Read,
    path::{Path, PathBuf},
};

pub(super) const CONFIG_API: &str = "oasis7.development-node/v1";

pub(super) fn read_config(path: &Path) -> Result<DevelopmentNodeConfigV1, String> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(|e| format!("read engineering config: {e}"))?
        .take(1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 1024 * 1024 {
        return Err("engineering config exceeds 1 MiB".into());
    }
    serde_json::from_slice(&bytes).map_err(|e| format!("typed engineering config: {e}"))
}
fn path_at(cwd: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.into()
    } else {
        cwd.join(path)
    }
}

pub(super) fn into_options(
    config: DevelopmentNodeConfigV1,
    require_provider: bool,
) -> Result<CliOptions, String> {
    if config.api_version != CONFIG_API
        || !config.working_directory.is_absolute()
        || !config.working_directory.is_dir()
    {
        return Err("engineering configuration schema/absolute working directory invalid".into());
    }
    if require_provider && config.local_provider.is_none() {
        return Err("dev local-provider requires typed local_provider setup".into());
    }
    let cwd = &config.working_directory;
    let r = config.runtime;
    for path in [
        &r.config_path,
        &r.runtime_root,
        &r.execution_bridge_state_path,
        &r.execution_world_dir,
        &r.execution_records_dir,
        &r.storage_root,
        &r.replication_root,
    ]
    .into_iter()
    .flatten()
    .chain(r.provider_backed_bootstrap_authority_paths.iter())
    {
        if path.as_os_str().is_empty() {
            return Err("engineering runtime path must not be empty".into());
        }
    }
    let mut o = CliOptions {
        node_id: r.node_id,
        world_id: r.world_id,
        ..CliOptions::default()
    };
    if let Some(value) = r.storage_profile {
        o.storage_profile = value;
    }
    if let Some(value) = r.p2p_accept_public_entry {
        o.p2p_accept_public_entry = value;
    }
    o.p2p_source_operator = r
        .p2p_source_operator
        .map(|v| super::cli::normalize_p2p_metadata_label(&v, "p2p_source_operator"))
        .transpose()?;
    o.p2p_source_asn = r
        .p2p_source_asn
        .map(|v| super::cli::normalize_p2p_metadata_label(&v, "p2p_source_asn"))
        .transpose()?;
    if let Some(value) = r.node_tick_ms {
        o.node_tick_ms = value;
    }
    if let Some(value) = r.pos_slot_duration_ms {
        o.pos_slot_duration_ms = value;
    }
    if let Some(value) = r.pos_ticks_per_slot {
        o.pos_ticks_per_slot = value;
    }
    if let Some(value) = r.pos_proposal_tick_phase {
        o.pos_proposal_tick_phase = value;
    }
    if let Some(value) = r.pos_adaptive_tick_scheduler_enabled {
        o.pos_adaptive_tick_scheduler_enabled = value;
    }
    if let Some(value) = r.pos_max_past_slot_lag {
        o.pos_max_past_slot_lag = value;
    }
    if let Some(value) = r.node_auto_attest_all_validators {
        o.node_auto_attest_all_validators = value;
    }
    if let Some(value) = r.capture_schedule_recipe_history {
        o.capture_schedule_recipe_history = value;
    }
    if let Some(value) = r.chain_local_standalone_test {
        o.chain_local_standalone_test = value;
    }
    if let Some(value) = r.reward_runtime_enabled {
        o.reward_runtime_enabled = value;
    }
    if let Some(value) = r.reward_points_per_credit {
        o.reward_points_per_credit = value;
    }
    if let Some(value) = r.reward_runtime_auto_redeem {
        o.reward_runtime_auto_redeem = value;
    }
    if let Some(value) = r.reward_initial_reserve_power_units {
        o.reward_initial_reserve_power_units = value;
    }
    if let Some(value) = r.status_bind {
        o.status_bind = value.to_string();
    }
    if let Some(value) = r.node_role {
        o.node_role = match value {
            DevelopmentNodeRoleV1::Sequencer => NodeRole::Sequencer,
            DevelopmentNodeRoleV1::Storage => NodeRole::Storage,
            DevelopmentNodeRoleV1::Observer => NodeRole::Observer,
        };
        o.p2p_node_role = default_p2p_node_role(o.node_role);
    }
    if let Some(value) = r.p2p_user_mode {
        o.p2p_user_mode = match value {
            DevelopmentUserModeV1::AutoJoin => NodeUserMode::AutoJoin,
            DevelopmentUserModeV1::PrivateSafe => NodeUserMode::PrivateSafe,
            DevelopmentUserModeV1::PublicEntry => NodeUserMode::PublicEntry,
        };
    }
    if let Some(value) = r.traffic_profile {
        o.traffic_profile = match value {
            DevelopmentTrafficProfileV1::Default => TrafficProfile::Default,
            DevelopmentTrafficProfileV1::TriadLowTraffic => TrafficProfile::TriadLowTraffic,
        };
    }
    if let Some(value) = r.p2p_detected_hole_punch_viability {
        o.p2p_detected_hole_punch_viability = match value {
            DevelopmentHolePunchV1::Unknown => NodeHolePunchViability::Unknown,
            DevelopmentHolePunchV1::Viable => NodeHolePunchViability::Viable,
            DevelopmentHolePunchV1::Blocked => NodeHolePunchViability::Blocked,
        };
        o.p2p_detected_hole_punch_viability_explicit = true;
    }
    o.p2p_detected_reachability = r.p2p_detected_reachability;
    o.p2p_detected_reachability_explicit = o.p2p_detected_reachability.is_some();
    if let Some(value) = r.p2p_detected_relay_available {
        o.p2p_detected_relay_available = value;
        o.p2p_detected_relay_available_explicit = true;
    }
    if let Some(value) = r.p2p_detected_probe_stable {
        o.p2p_detected_probe_stable = value;
        o.p2p_detected_probe_stable_explicit = true;
    }
    if let Some(value) = r.p2p_deployment_mode {
        o.p2p_deployment_mode = value;
        o.p2p_deployment_mode_explicit = true;
    }
    if let Some(value) = r.p2p_node_role {
        o.p2p_node_role = value;
        o.p2p_node_role_explicit = true;
    }
    o.p2p_max_ipv4_subnet_active_peers = r.p2p_max_ipv4_subnet_active_peers;
    o.pos_slot_clock_genesis_unix_ms = r.pos_slot_clock_genesis_unix_ms;
    o.reward_runtime_signer_node_id = r.reward_runtime_signer_node_id;
    o.reward_runtime_epoch_duration_secs = r.reward_runtime_epoch_duration_secs;
    o.node_gossip_bind = r.node_gossip_bind;
    if let Some(value) = r.config_path {
        o.config_path = path_at(cwd, &value).to_string_lossy().into_owned();
    } else {
        o.config_path = path_at(cwd, Path::new(&o.config_path))
            .to_string_lossy()
            .into_owned();
    }
    o.runtime_root = r.runtime_root.map(|p| path_at(cwd, &p));
    o.execution_bridge_state_path = r.execution_bridge_state_path.map(|p| path_at(cwd, &p));
    o.execution_world_dir = r.execution_world_dir.map(|p| path_at(cwd, &p));
    o.execution_records_dir = r.execution_records_dir.map(|p| path_at(cwd, &p));
    o.storage_root = r.storage_root.map(|p| path_at(cwd, &p));
    o.replication_root = r.replication_root.map(|p| path_at(cwd, &p));
    o.node_gossip_peers = r.node_gossip_peers;
    o.replication_network_listen_addrs = r.replication_network_listen_addrs;
    o.replication_network_bootstrap_peers = r.replication_network_bootstrap_peers;
    o.replication_fetch_requester_public_keys = r.replication_fetch_requester_public_keys;
    o.provider_backed_bootstrap_authority_paths = r
        .provider_backed_bootstrap_authority_paths
        .into_iter()
        .map(|p| path_at(cwd, &p))
        .collect();
    let mut validator_ids = BTreeSet::new();
    for validator in r.node_validators {
        if validator.validator_id.trim().is_empty()
            || validator.stake == 0
            || !validator_ids.insert(validator.validator_id.clone())
        {
            return Err("engineering validator identity/stake is empty or rebound".into());
        }
        o.node_validators.push(PosValidator {
            validator_id: validator.validator_id,
            stake: validator.stake,
        });
    }
    for signer in r.node_validator_signer_public_keys {
        if signer.validator_id.trim().is_empty()
            || signer.public_key.len() != 64
            || !signer.public_key.bytes().all(|b| b.is_ascii_hexdigit())
            || o.node_validator_signer_public_keys
                .insert(signer.validator_id, signer.public_key.to_ascii_lowercase())
                .is_some()
        {
            return Err("engineering validator signer identity is invalid or rebound".into());
        }
    }
    if let Some(provider) = config.local_provider {
        if [
            &provider.authority_path,
            &provider.wasm_path,
            &provider.metadata_path,
        ]
        .iter()
        .any(|p| p.as_os_str().is_empty())
        {
            return Err("local provider paths must not be empty".into());
        }
        if o.storage_profile != oasis7_proto::storage_profile::StorageProfile::DevLocal {
            return Err("local provider setup requires dev_local storage".into());
        }
        o.local_test_provider_authority_path = Some(path_at(cwd, &provider.authority_path));
        o.local_test_provider_wasm_path = Some(path_at(cwd, &provider.wasm_path));
        o.local_test_provider_metadata_path = Some(path_at(cwd, &provider.metadata_path));
        o.local_test_provider_agent_id = provider.agent_id;
        o.local_test_provider_owner_binding = provider.owner_binding;
        o.local_test_provider_finality_block_hash = Some(provider.finality_block_hash);
        o.local_test_provider_session_mode = match provider.session_mode {
            DevelopmentProviderSessionV1::HostedPublicJoin => "hosted_public_join",
            DevelopmentProviderSessionV1::Loopback => "loopback",
        }
        .into();
        if provider.funding_lane == Some(DevelopmentFundingLaneV1::HostedLocalMock) {
            o.agent_decision_source = Some("provider_backed".into());
            o.agent_provider_backend = Some("provider_local_mock".into());
            o.agent_provider_contract = Some("worldsim_provider_v1".into());
            o.agent_provider_transport = Some("loopback_http".into());
            o.agent_execution_lane = Some("player_parity".into());
        }
    }
    if let Some(probe) = r.reward_distfs_probe {
        if let Some(v) = probe.max_sample_bytes {
            o.reward_distfs_probe_config.max_sample_bytes = v;
        }
        if let Some(v) = probe.challenges_per_tick {
            o.reward_distfs_probe_config.challenges_per_tick = v;
        }
        if let Some(v) = probe.challenge_ttl_ms {
            o.reward_distfs_probe_config.challenge_ttl_ms = v;
        }
        if let Some(v) = probe.allowed_clock_skew_ms {
            o.reward_distfs_probe_config.allowed_clock_skew_ms = v;
        }
        if let Some(v) = probe.max_checks_per_round {
            o.reward_distfs_probe_config
                .adaptive_policy
                .max_checks_per_round = v;
        }
        if let Some(v) = probe.failure_backoff_base_ms {
            o.reward_distfs_probe_config
                .adaptive_policy
                .failure_backoff_base_ms = v;
        }
        if let Some(v) = probe.failure_backoff_max_ms {
            o.reward_distfs_probe_config
                .adaptive_policy
                .failure_backoff_max_ms = v;
        }
        if let Some(v) = probe.backoff_multiplier_hash_mismatch {
            o.reward_distfs_probe_config
                .adaptive_policy
                .backoff_multiplier_hash_mismatch = v;
        }
        if let Some(v) = probe.backoff_multiplier_missing_sample {
            o.reward_distfs_probe_config
                .adaptive_policy
                .backoff_multiplier_missing_sample = v;
        }
        if let Some(v) = probe.backoff_multiplier_timeout {
            o.reward_distfs_probe_config
                .adaptive_policy
                .backoff_multiplier_timeout = v;
        }
        if let Some(v) = probe.backoff_multiplier_read_io_error {
            o.reward_distfs_probe_config
                .adaptive_policy
                .backoff_multiplier_read_io_error = v;
        }
        if let Some(v) = probe.backoff_multiplier_signature_invalid {
            o.reward_distfs_probe_config
                .adaptive_policy
                .backoff_multiplier_signature_invalid = v;
        }
        if let Some(v) = probe.backoff_multiplier_unknown {
            o.reward_distfs_probe_config
                .adaptive_policy
                .backoff_multiplier_unknown = v;
        }
    }
    let p = &o.reward_distfs_probe_config;
    let a = &p.adaptive_policy;
    if p.max_sample_bytes == 0
        || p.challenges_per_tick == 0
        || p.challenge_ttl_ms <= 0
        || p.allowed_clock_skew_ms < 0
        || a.max_checks_per_round == 0
        || a.failure_backoff_base_ms < 0
        || a.failure_backoff_max_ms < 0
        || [
            a.backoff_multiplier_hash_mismatch,
            a.backoff_multiplier_missing_sample,
            a.backoff_multiplier_timeout,
            a.backoff_multiplier_read_io_error,
            a.backoff_multiplier_signature_invalid,
            a.backoff_multiplier_unknown,
        ]
        .contains(&0)
    {
        return Err("engineering DistFS probe budget/backoff is invalid".into());
    }
    // Freeze default path derivation against declared cwd as well. Relative
    // JSON paths never inherit a different launcher/operator process cwd.
    let paths = crate::resolve_runtime_paths(&o);
    o.runtime_root = Some(path_at(cwd, &paths.runtime_root));
    o.execution_bridge_state_path = Some(path_at(cwd, &paths.execution_bridge_state_path));
    o.execution_world_dir = Some(path_at(cwd, &paths.execution_world_dir));
    o.execution_records_dir = Some(path_at(cwd, &paths.execution_records_dir));
    o.storage_root = Some(path_at(cwd, &paths.storage_root));
    o.replication_root = Some(path_at(cwd, &paths.replication_root));
    validate_effective_options(&o)?;
    o.development_runtime = true;
    Ok(o)
}

pub(super) fn from_options(o: &CliOptions) -> Result<DevelopmentNodeConfigV1, String> {
    if o.network_tier_manifest_path.is_some()
        || o.loaded_network_tier_manifest.is_some()
        || o.genesis_validator_registry_path.is_some()
        || o.deployment_inventory_path.is_some()
        || o.formal_live_config.is_some()
        || o.guarded_initial_config.is_some()
    {
        return Err("engineering typed runtime cannot consume manifest/formal/guarded/deployment authority inputs".into());
    }
    validate_effective_options(o)?;
    let mut r = DevelopmentRuntimeConfigV1 {
        node_id: o.node_id.clone(),
        world_id: o.world_id.clone(),
        ..Default::default()
    };
    r.storage_profile = Some(o.storage_profile);
    r.p2p_accept_public_entry = Some(o.p2p_accept_public_entry);
    r.node_tick_ms = Some(o.node_tick_ms);
    r.pos_slot_duration_ms = Some(o.pos_slot_duration_ms);
    r.pos_ticks_per_slot = Some(o.pos_ticks_per_slot);
    r.pos_proposal_tick_phase = Some(o.pos_proposal_tick_phase);
    r.pos_adaptive_tick_scheduler_enabled = Some(o.pos_adaptive_tick_scheduler_enabled);
    r.pos_max_past_slot_lag = Some(o.pos_max_past_slot_lag);
    r.node_auto_attest_all_validators = Some(o.node_auto_attest_all_validators);
    r.capture_schedule_recipe_history = Some(o.capture_schedule_recipe_history);
    r.chain_local_standalone_test = Some(o.chain_local_standalone_test);
    r.reward_runtime_enabled = Some(o.reward_runtime_enabled);
    r.reward_points_per_credit = Some(o.reward_points_per_credit);
    r.reward_runtime_auto_redeem = Some(o.reward_runtime_auto_redeem);
    r.reward_initial_reserve_power_units = Some(o.reward_initial_reserve_power_units);
    r.p2p_source_operator = o.p2p_source_operator.clone();
    r.p2p_source_asn = o.p2p_source_asn.clone();
    r.p2p_max_ipv4_subnet_active_peers = o.p2p_max_ipv4_subnet_active_peers;
    r.pos_slot_clock_genesis_unix_ms = o.pos_slot_clock_genesis_unix_ms;
    r.reward_runtime_signer_node_id = o.reward_runtime_signer_node_id.clone();
    r.reward_runtime_epoch_duration_secs = o.reward_runtime_epoch_duration_secs;
    r.node_gossip_bind = o.node_gossip_bind;
    r.runtime_root = o.runtime_root.clone();
    r.execution_bridge_state_path = o.execution_bridge_state_path.clone();
    r.execution_world_dir = o.execution_world_dir.clone();
    r.execution_records_dir = o.execution_records_dir.clone();
    r.storage_root = o.storage_root.clone();
    r.replication_root = o.replication_root.clone();
    r.node_gossip_peers = o.node_gossip_peers.clone();
    r.replication_network_listen_addrs = o.replication_network_listen_addrs.clone();
    r.replication_network_bootstrap_peers = o.replication_network_bootstrap_peers.clone();
    r.replication_fetch_requester_public_keys = o.replication_fetch_requester_public_keys.clone();
    r.provider_backed_bootstrap_authority_paths =
        o.provider_backed_bootstrap_authority_paths.clone();
    r.config_path = Some(o.config_path.clone().into());
    r.status_bind = Some(
        o.status_bind
            .parse()
            .map_err(|_| "typed engineering status_bind requires an IP socket address")?,
    );
    r.node_role = Some(match o.node_role {
        NodeRole::Sequencer => DevelopmentNodeRoleV1::Sequencer,
        NodeRole::Storage => DevelopmentNodeRoleV1::Storage,
        NodeRole::Observer => DevelopmentNodeRoleV1::Observer,
    });
    r.p2p_user_mode = Some(match o.p2p_user_mode {
        NodeUserMode::AutoJoin => DevelopmentUserModeV1::AutoJoin,
        NodeUserMode::PrivateSafe => DevelopmentUserModeV1::PrivateSafe,
        NodeUserMode::PublicEntry => DevelopmentUserModeV1::PublicEntry,
    });
    r.traffic_profile = Some(match o.traffic_profile {
        TrafficProfile::Default => DevelopmentTrafficProfileV1::Default,
        TrafficProfile::TriadLowTraffic => DevelopmentTrafficProfileV1::TriadLowTraffic,
    });
    if o.p2p_detected_reachability_explicit {
        r.p2p_detected_reachability = o.p2p_detected_reachability;
    }
    if o.p2p_detected_hole_punch_viability_explicit {
        r.p2p_detected_hole_punch_viability = Some(match o.p2p_detected_hole_punch_viability {
            NodeHolePunchViability::Unknown => DevelopmentHolePunchV1::Unknown,
            NodeHolePunchViability::Viable => DevelopmentHolePunchV1::Viable,
            NodeHolePunchViability::Blocked => DevelopmentHolePunchV1::Blocked,
        });
    }
    if o.p2p_detected_relay_available_explicit {
        r.p2p_detected_relay_available = Some(o.p2p_detected_relay_available);
    }
    if o.p2p_detected_probe_stable_explicit {
        r.p2p_detected_probe_stable = Some(o.p2p_detected_probe_stable);
    }
    if o.p2p_deployment_mode_explicit {
        r.p2p_deployment_mode = Some(o.p2p_deployment_mode);
    }
    if o.p2p_node_role_explicit {
        r.p2p_node_role = Some(o.p2p_node_role);
    }
    r.node_validators = o
        .node_validators
        .iter()
        .map(|v| DevelopmentValidatorV1 {
            validator_id: v.validator_id.clone(),
            stake: v.stake,
        })
        .collect();
    r.node_validator_signer_public_keys = o
        .node_validator_signer_public_keys
        .iter()
        .map(|(id, key)| DevelopmentValidatorSignerV1 {
            validator_id: id.clone(),
            public_key: key.clone(),
        })
        .collect();
    let p = &o.reward_distfs_probe_config;
    let a = &p.adaptive_policy;
    r.reward_distfs_probe = Some(DevelopmentDistfsProbeV1 {
        max_sample_bytes: Some(p.max_sample_bytes),
        challenges_per_tick: Some(p.challenges_per_tick),
        challenge_ttl_ms: Some(p.challenge_ttl_ms),
        allowed_clock_skew_ms: Some(p.allowed_clock_skew_ms),
        max_checks_per_round: Some(a.max_checks_per_round),
        failure_backoff_base_ms: Some(a.failure_backoff_base_ms),
        failure_backoff_max_ms: Some(a.failure_backoff_max_ms),
        backoff_multiplier_hash_mismatch: Some(a.backoff_multiplier_hash_mismatch),
        backoff_multiplier_missing_sample: Some(a.backoff_multiplier_missing_sample),
        backoff_multiplier_timeout: Some(a.backoff_multiplier_timeout),
        backoff_multiplier_read_io_error: Some(a.backoff_multiplier_read_io_error),
        backoff_multiplier_signature_invalid: Some(a.backoff_multiplier_signature_invalid),
        backoff_multiplier_unknown: Some(a.backoff_multiplier_unknown),
    });
    let local_provider = if let Some(path) = &o.local_test_provider_authority_path {
        Some(DevelopmentLocalProviderV1 {
            authority_path: path.clone(),
            wasm_path: o
                .local_test_provider_wasm_path
                .clone()
                .ok_or("provider WASM missing")?,
            metadata_path: o
                .local_test_provider_metadata_path
                .clone()
                .ok_or("provider metadata missing")?,
            agent_id: o.local_test_provider_agent_id.clone(),
            owner_binding: o.local_test_provider_owner_binding.clone(),
            finality_block_hash: o
                .local_test_provider_finality_block_hash
                .clone()
                .ok_or("provider finality missing")?,
            session_mode: match o.local_test_provider_session_mode.as_str() {
                "hosted_public_join" => DevelopmentProviderSessionV1::HostedPublicJoin,
                "loopback" => DevelopmentProviderSessionV1::Loopback,
                _ => return Err("provider session invalid".into()),
            },
            funding_lane: if o.agent_decision_source.is_some() {
                Some(DevelopmentFundingLaneV1::HostedLocalMock)
            } else {
                None
            },
        })
    } else {
        None
    };
    Ok(DevelopmentNodeConfigV1 {
        api_version: CONFIG_API.into(),
        working_directory: std::env::current_dir().map_err(|e| e.to_string())?,
        runtime: r,
        local_provider,
    })
}

pub(super) fn run(config: DevelopmentNodeConfigV1, require_provider: bool) -> Result<(), String> {
    crate::run_chain_runtime(into_options(config, require_provider)?)
}

pub(super) fn prepare_config<'a>(mut args: impl Iterator<Item = &'a str>) -> Result<(), String> {
    if args.next() != Some("--output") {
        return Err(
            "dev prepare-config requires --output PATH -- legacy-engineering-options".into(),
        );
    }
    let output = args
        .next()
        .filter(|v| !v.is_empty() && !v.starts_with("--"))
        .ok_or("--output requires a path")?;
    if args.next() != Some("--") {
        return Err(
            "dev prepare-config requires explicit -- before source engineering options".into(),
        );
    }
    let options = super::cli::parse_options(args)?;
    let config = from_options(&options)?;
    into_options(config.clone(), false)?; // all validation before config file effect
    let output = Path::new(output);
    if let Some(parent) = output.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    // Configuration contains public settings/paths only, never secret bytes.
    let bytes = serde_json::to_vec_pretty(&config).map_err(|e| e.to_string())?;
    let mut temporary = output.to_path_buf();
    temporary.set_extension(format!("dev-config-{}.tmp", std::process::id()));
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|e| e.to_string())?;
    file.write_all(&bytes)
        .and_then(|_| file.sync_all())
        .map_err(|e| e.to_string())?;
    std::fs::rename(&temporary, output).map_err(|e| e.to_string())?;
    println!("{}", output.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn engineering_config_roundtrip_preserves_defaults_and_live_hints() {
        let original = CliOptions::default();
        let typed = from_options(&original).unwrap();
        let decoded: DevelopmentNodeConfigV1 =
            serde_json::from_slice(&serde_json::to_vec(&typed).unwrap()).unwrap();
        let result = into_options(decoded, false).unwrap();
        assert_eq!(result.node_id, original.node_id);
        assert_eq!(result.world_id, original.world_id);
        assert_eq!(result.node_tick_ms, original.node_tick_ms);
        assert_eq!(
            result.node_auto_attest_all_validators,
            original.node_auto_attest_all_validators
        );
        assert_eq!(
            result.p2p_detected_reachability_explicit,
            original.p2p_detected_reachability_explicit
        );
        assert!(result.development_runtime);
        assert!(into_options(typed, true).is_err());
    }
    #[test]
    fn typed_config_rejects_unknown_fields_at_each_boundary() {
        let typed = from_options(&CliOptions::default()).unwrap();
        let base = serde_json::to_value(&typed).unwrap();
        for field in [
            "argv",
            "network_tier_manifest_path",
            "formal_initial_config",
        ] {
            let mut value = base.clone();
            value
                .as_object_mut()
                .unwrap()
                .insert(field.into(), serde_json::json!("forbidden"));
            assert!(serde_json::from_value::<DevelopmentNodeConfigV1>(value).is_err());
        }
        let mut value = base.clone();
        value["runtime"]["identity_override"] = serde_json::json!("forbidden");
        assert!(serde_json::from_value::<DevelopmentNodeConfigV1>(value).is_err());
        let mut value = base;
        value["runtime"]["node_role"] = serde_json::json!("authority");
        assert!(serde_json::from_value::<DevelopmentNodeConfigV1>(value).is_err());
    }
}
