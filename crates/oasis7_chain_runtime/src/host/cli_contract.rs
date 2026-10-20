//! Single metadata catalog for the flat compatibility CLI and its generated help.
use super::CliOptions;
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OptionKind {
    Scalar,
    List,
    Switch,
}

pub(crate) struct OptionSpec {
    pub name: &'static str,
    pub alias: Option<&'static str>,
    pub kind: OptionKind,
    pub conflict: Option<&'static str>,
    pub value: &'static str,
    pub description: &'static str,
    pub default: Option<fn(&CliOptions) -> String>,
}

pub(crate) const OPTIONS: &[OptionSpec] = &[
    OptionSpec {
        name: "--node-id",
        alias: None,
        kind: OptionKind::Scalar,
        conflict: None,
        value: "<id>",
        description: "node identifier",
        default: Some(|d| d.node_id.to_string()),
    },
    OptionSpec {
        name: "--world-id",
        alias: None,
        kind: OptionKind::Scalar,
        conflict: None,
        value: "<id>",
        description: "technical runtime partition id for the unified persistent world",
        default: Some(|d| d.world_id.to_string()),
    },
    OptionSpec {
        name: "--storage-profile",
        alias: None,
        kind: OptionKind::Scalar,
        conflict: None,
        value: "<name>",
        description: "dev_local|release_default|soak_forensics",
        default: Some(|d| d.storage_profile.as_str().to_string()),
    },
    OptionSpec {
        name: "--traffic-profile",
        alias: None,
        kind: OptionKind::Scalar,
        conflict: None,
        value: "<name>",
        description: "default|triad_low_traffic",
        default: Some(|d| d.traffic_profile.as_str().to_string()),
    },
    OptionSpec {
        name: "--status-bind",
        alias: None,
        kind: OptionKind::Scalar,
        conflict: None,
        value: "<host:port>",
        description: "status HTTP bind",
        default: Some(|d| d.status_bind.to_string()),
    },
    OptionSpec {
        name: "--node-role",
        alias: None,
        kind: OptionKind::Scalar,
        conflict: None,
        value: "<role>",
        description: "sequencer|storage|observer",
        default: Some(|d| {
            match d.node_role {
                oasis7_node::NodeRole::Sequencer => "sequencer",
                oasis7_node::NodeRole::Storage => "storage",
                oasis7_node::NodeRole::Observer => "observer",
            }
            .to_string()
        }),
    },
    OptionSpec {
        name: "--p2p-user-mode",
        alias: None,
        kind: OptionKind::Scalar,
        conflict: None,
        value: "<mode>",
        description: "auto_join|private_safe|public_entry",
        default: Some(|d| d.p2p_user_mode.as_str().to_string()),
    },
    OptionSpec {
        name: "--p2p-accept-public-entry",
        alias: None,
        kind: OptionKind::Switch,
        conflict: Some("--p2p-accept-public-entry"),
        value: "",
        description: "accept auto-detected public-entry recommendation",
        default: Some(|d| d.p2p_accept_public_entry.to_string()),
    },
    OptionSpec {
        name: "--p2p-reject-public-entry",
        alias: None,
        kind: OptionKind::Switch,
        conflict: Some("--p2p-accept-public-entry"),
        value: "",
        description: "force conservative fallback when auto-detect suggests public entry",
        default: None,
    },
    OptionSpec {
        name: "--p2p-detected-reachability",
        alias: None,
        kind: OptionKind::Scalar,
        conflict: Some("--p2p-detected-reachability"),
        value: "<c>",
        description: "public|hybrid|private|relay_only|validator_hidden",
        default: None,
    },
    OptionSpec {
        name: "--p2p-clear-detected-reachability",
        alias: None,
        kind: OptionKind::Switch,
        conflict: Some("--p2p-detected-reachability"),
        value: "",
        description: "clear explicit reachability hint and restore live detection",
        default: None,
    },
    OptionSpec {
        name: "--p2p-detected-hole-punch",
        alias: None,
        kind: OptionKind::Scalar,
        conflict: None,
        value: "<s>",
        description: "unknown|viable|blocked",
        default: Some(|d| d.p2p_detected_hole_punch_viability.as_str().to_string()),
    },
    OptionSpec {
        name: "--p2p-detected-relay-available",
        alias: None,
        kind: OptionKind::Switch,
        conflict: Some("--p2p-detected-relay-available"),
        value: "",
        description: "override live relay detection to available",
        default: None,
    },
    OptionSpec {
        name: "--p2p-detected-relay-unavailable",
        alias: None,
        kind: OptionKind::Switch,
        conflict: Some("--p2p-detected-relay-available"),
        value: "",
        description: "override live relay detection to unavailable",
        default: None,
    },
    OptionSpec {
        name: "--p2p-detected-probe-stable",
        alias: None,
        kind: OptionKind::Switch,
        conflict: Some("--p2p-detected-probe-stable"),
        value: "",
        description: "override live probe detection to stable",
        default: None,
    },
    OptionSpec {
        name: "--p2p-detected-probe-unstable",
        alias: None,
        kind: OptionKind::Switch,
        conflict: Some("--p2p-detected-probe-stable"),
        value: "",
        description: "override live probe detection to unstable",
        default: None,
    },
    OptionSpec {
        name: "--p2p-deployment-mode",
        alias: None,
        kind: OptionKind::Scalar,
        conflict: None,
        value: "<mode>",
        description: "public|hybrid|private|relay_only|validator_hidden",
        default: Some(|d| d.p2p_deployment_mode.as_str().to_string()),
    },
    OptionSpec {
        name: "--p2p-node-role",
        alias: None,
        kind: OptionKind::Scalar,
        conflict: None,
        value: "<role>",
        description: "validator_core|sentry|relay|full_storage|observer_light",
        default: None,
    },
    OptionSpec {
        name: "--p2p-source-operator",
        alias: None,
        kind: OptionKind::Scalar,
        conflict: None,
        value: "<label>",
        description: "canonical operator label for peer diversity policy",
        default: None,
    },
    OptionSpec {
        name: "--p2p-source-asn",
        alias: None,
        kind: OptionKind::Scalar,
        conflict: None,
        value: "<label>",
        description: "canonical ASN label for peer diversity policy",
        default: None,
    },
    OptionSpec {
        name: "--p2p-max-ipv4-subnet-active-peers",
        alias: None,
        kind: OptionKind::Scalar,
        conflict: None,
        value: "<n>",
        description: "max active peers allowed in one IPv4 /24 before blocking",
        default: None,
    },
    OptionSpec {
        name: "--node-tick-ms",
        alias: None,
        kind: OptionKind::Scalar,
        conflict: None,
        value: "<n>",
        description: "worker poll/fallback interval ms",
        default: Some(|d| d.node_tick_ms.to_string()),
    },
    OptionSpec {
        name: "--pos-slot-duration-ms",
        alias: None,
        kind: OptionKind::Scalar,
        conflict: None,
        value: "<n>",
        description: "PoS slot duration in milliseconds",
        default: Some(|d| d.pos_slot_duration_ms.to_string()),
    },
    OptionSpec {
        name: "--pos-ticks-per-slot",
        alias: None,
        kind: OptionKind::Scalar,
        conflict: None,
        value: "<n>",
        description: "logical ticks per PoS slot",
        default: Some(|d| d.pos_ticks_per_slot.to_string()),
    },
    OptionSpec {
        name: "--pos-proposal-tick-phase",
        alias: None,
        kind: OptionKind::Scalar,
        conflict: None,
        value: "<n>",
        description: "proposal trigger phase within slot tick window",
        default: Some(|d| d.pos_proposal_tick_phase.to_string()),
    },
    OptionSpec {
        name: "--pos-adaptive-tick-scheduler",
        alias: None,
        kind: OptionKind::Switch,
        conflict: Some("--pos-adaptive-tick-scheduler"),
        value: "",
        description: "enable adaptive wait to next logical tick boundary",
        default: Some(|d| d.pos_adaptive_tick_scheduler_enabled.to_string()),
    },
    OptionSpec {
        name: "--pos-no-adaptive-tick-scheduler",
        alias: None,
        kind: OptionKind::Switch,
        conflict: Some("--pos-adaptive-tick-scheduler"),
        value: "",
        description: "disable adaptive scheduler",
        default: None,
    },
    OptionSpec {
        name: "--pos-slot-clock-genesis-unix-ms",
        alias: None,
        kind: OptionKind::Scalar,
        conflict: None,
        value: "<n>",
        description: "fixed slot clock genesis unix ms",
        default: Some(|d| {
            d.pos_slot_clock_genesis_unix_ms
                .map(|v| v.to_string())
                .unwrap_or_else(|| "auto".into())
        }),
    },
    OptionSpec {
        name: "--pos-max-past-slot-lag",
        alias: None,
        kind: OptionKind::Scalar,
        conflict: None,
        value: "<n>",
        description: "max accepted inbound stale slot lag",
        default: Some(|d| d.pos_max_past_slot_lag.to_string()),
    },
    OptionSpec {
        name: "--node-validator",
        alias: None,
        kind: OptionKind::List,
        conflict: None,
        value: "<id:stake>",
        description: "add validator stake",
        default: None,
    },
    OptionSpec {
        name: "--node-validator-signer-public-key",
        alias: None,
        kind: OptionKind::List,
        conflict: None,
        value: "<id:public_key_hex>",
        description: "override validator signer public key",
        default: None,
    },
    OptionSpec {
        name: "--node-auto-attest-all",
        alias: None,
        kind: OptionKind::Switch,
        conflict: Some("--node-auto-attest-all"),
        value: "",
        description: "enable auto attesting validators",
        default: Some(|d| d.node_auto_attest_all_validators.to_string()),
    },
    OptionSpec {
        name: "--node-no-auto-attest-all",
        alias: None,
        kind: OptionKind::Switch,
        conflict: Some("--node-auto-attest-all"),
        value: "",
        description: "disable auto attesting validators",
        default: None,
    },
    OptionSpec {
        name: "--node-gossip-bind",
        alias: None,
        kind: OptionKind::Scalar,
        conflict: None,
        value: "<addr:port>",
        description: "UDP gossip bind",
        default: None,
    },
    OptionSpec {
        name: "--node-gossip-peer",
        alias: None,
        kind: OptionKind::List,
        conflict: None,
        value: "<addr:port>",
        description: "UDP gossip peer (repeatable, requires --node-gossip-bind)",
        default: None,
    },
    OptionSpec {
        name: "--replication-network-listen",
        alias: None,
        kind: OptionKind::List,
        conflict: None,
        value: "<multiaddr>",
        description: "libp2p replication listen addr",
        default: Some(|_| super::DEFAULT_REPLICATION_NETWORK_LISTEN.to_string()),
    },
    OptionSpec {
        name: "--replication-network-peer",
        alias: None,
        kind: OptionKind::List,
        conflict: None,
        value: "<multiaddr>",
        description: "libp2p replication bootstrap peer",
        default: None,
    },
    OptionSpec {
        name: "--replication-fetch-requester-public-key",
        alias: Some("--replication-remote-writer-public-key"),
        kind: OptionKind::List,
        conflict: None,
        value: "<public_key_hex>",
        description: "extra authorized replication fetch requester",
        default: None,
    },
    OptionSpec {
        name: "--network-tier-manifest",
        alias: None,
        kind: OptionKind::Scalar,
        conflict: None,
        value: "<path>",
        description: "load formal network tier manifest json and bootstrap peer ref",
        default: None,
    },
    OptionSpec {
        name: "--genesis-validator-registry",
        alias: None,
        kind: OptionKind::Scalar,
        conflict: None,
        value: "<path>",
        description: "initialize empty execution world validator registry from genesis manifest",
        default: None,
    },
    OptionSpec {
        name: "--deployment-inventory",
        alias: None,
        kind: OptionKind::Scalar,
        conflict: None,
        value: "<path>",
        description: "bind immutable deployment inventory to node-emitted status",
        default: None,
    },
    OptionSpec {
        name: "--config",
        alias: None,
        kind: OptionKind::Scalar,
        conflict: None,
        value: "<path>",
        description: "config file path for node keypair",
        default: Some(|d| d.config_path.to_string()),
    },
    OptionSpec {
        name: "--runtime-root",
        alias: None,
        kind: OptionKind::Scalar,
        conflict: None,
        value: "<path>",
        description: "override chain runtime state root directory",
        default: None,
    },
    OptionSpec {
        name: "--execution-bridge-state",
        alias: None,
        kind: OptionKind::Scalar,
        conflict: None,
        value: "<path>",
        description: "override execution bridge state file path",
        default: None,
    },
    OptionSpec {
        name: "--execution-world-dir",
        alias: None,
        kind: OptionKind::Scalar,
        conflict: None,
        value: "<path>",
        description: "override execution world directory",
        default: None,
    },
    OptionSpec {
        name: "--execution-records-dir",
        alias: None,
        kind: OptionKind::Scalar,
        conflict: None,
        value: "<path>",
        description: "override execution records directory",
        default: None,
    },
    OptionSpec {
        name: "--capture-schedule-recipe-history",
        alias: None,
        kind: OptionKind::Switch,
        conflict: None,
        value: "",
        description: "capture bounded recipe history for offline verification (no authority activation)",
        default: None,
    },
    OptionSpec {
        name: "--provider-bootstrap-authority",
        alias: None,
        kind: OptionKind::List,
        conflict: None,
        value: "<path>",
        description: "explicit JSON Runtime authority bundle; one per provider-backed agent",
        default: None,
    },
    OptionSpec {
        name: "--local-test-provider-authority",
        alias: None,
        kind: OptionKind::Scalar,
        conflict: None,
        value: "<path>",
        description: "explicit DevLocal output JSON authority bundle (opt-in setup)",
        default: None,
    },
    OptionSpec {
        name: "--local-test-provider-wasm",
        alias: None,
        kind: OptionKind::Scalar,
        conflict: None,
        value: "<path>",
        description: "real WASM artifact for the explicit DevLocal provider setup",
        default: None,
    },
    OptionSpec {
        name: "--local-test-provider-metadata",
        alias: None,
        kind: OptionKind::Scalar,
        conflict: None,
        value: "<path>",
        description: "canonical build-suite metadata JSON for the WASM artifact",
        default: None,
    },
    OptionSpec {
        name: "--local-test-provider-agent-id",
        alias: None,
        kind: OptionKind::Scalar,
        conflict: None,
        value: "<id>",
        description: "live starter agent to provision",
        default: Some(|d| d.local_test_provider_agent_id.to_string()),
    },
    OptionSpec {
        name: "--local-test-provider-owner-binding",
        alias: None,
        kind: OptionKind::Scalar,
        conflict: None,
        value: "<id>",
        description: "stable local session owner binding",
        default: Some(|d| d.local_test_provider_owner_binding.to_string()),
    },
    OptionSpec {
        name: "--local-test-provider-finality-block-hash",
        alias: None,
        kind: OptionKind::Scalar,
        conflict: None,
        value: "<hash>",
        description: "explicit local finality marker, blake3:<64 lowercase hex>",
        default: None,
    },
    OptionSpec {
        name: "--local-test-provider-session-mode",
        alias: None,
        kind: OptionKind::Scalar,
        conflict: None,
        value: "<mode>",
        description: "hosted_public_join|loopback",
        default: Some(|d| d.local_test_provider_session_mode.to_string()),
    },
    OptionSpec {
        name: "--storage-root",
        alias: None,
        kind: OptionKind::Scalar,
        conflict: None,
        value: "<path>",
        description: "override execution CAS/storage root",
        default: None,
    },
    OptionSpec {
        name: "--replication-root",
        alias: None,
        kind: OptionKind::Scalar,
        conflict: None,
        value: "<path>",
        description: "override replication root directory",
        default: None,
    },
    OptionSpec {
        name: "--reward-runtime-enable",
        alias: None,
        kind: OptionKind::Switch,
        conflict: Some("--reward-runtime-enable"),
        value: "",
        description: "enable reward runtime worker",
        default: Some(|d| d.reward_runtime_enabled.to_string()),
    },
    OptionSpec {
        name: "--reward-runtime-disable",
        alias: None,
        kind: OptionKind::Switch,
        conflict: Some("--reward-runtime-enable"),
        value: "",
        description: "disable reward runtime worker",
        default: None,
    },
    OptionSpec {
        name: "--reward-runtime-signer-node-id",
        alias: None,
        kind: OptionKind::Scalar,
        conflict: None,
        value: "<id>",
        description: "override reward runtime signer node id; omitted uses --node-id",
        default: Some(|d| {
            d.reward_runtime_signer_node_id
                .clone()
                .unwrap_or_else(|| d.node_id.clone())
        }),
    },
    OptionSpec {
        name: "--reward-runtime-epoch-duration-secs",
        alias: None,
        kind: OptionKind::Scalar,
        conflict: None,
        value: "<n>",
        description: "override reward settlement epoch duration seconds",
        default: None,
    },
    OptionSpec {
        name: "--reward-points-per-credit",
        alias: None,
        kind: OptionKind::Scalar,
        conflict: None,
        value: "<n>",
        description: "reward points per credit",
        default: Some(|d| d.reward_points_per_credit.to_string()),
    },
    OptionSpec {
        name: "--reward-runtime-auto-redeem",
        alias: None,
        kind: OptionKind::Switch,
        conflict: Some("--reward-runtime-auto-redeem"),
        value: "",
        description: "enable runtime auto redeem",
        default: Some(|d| d.reward_runtime_auto_redeem.to_string()),
    },
    OptionSpec {
        name: "--reward-runtime-no-auto-redeem",
        alias: None,
        kind: OptionKind::Switch,
        conflict: Some("--reward-runtime-auto-redeem"),
        value: "",
        description: "disable runtime auto redeem",
        default: None,
    },
    OptionSpec {
        name: "--reward-initial-reserve-power-units",
        alias: None,
        kind: OptionKind::Scalar,
        conflict: None,
        value: "<n>",
        description: "reward reserve power units",
        default: Some(|d| d.reward_initial_reserve_power_units.to_string()),
    },
    OptionSpec {
        name: "--reward-distfs-probe-per-tick",
        alias: None,
        kind: OptionKind::Scalar,
        conflict: None,
        value: "<n>",
        description: "distfs challenge probes per tick",
        default: Some(|d| d.reward_distfs_probe_config.challenges_per_tick.to_string()),
    },
    OptionSpec {
        name: "--guarded-initial-config",
        alias: None,
        kind: OptionKind::Scalar,
        conflict: Some("--formal-live-config"),
        value: "<path>",
        description: "deprecated thin guarded adapter; use dev guarded-world --config PATH; only optional --reward-runtime-disable is applicable",
        default: None,
    },
    OptionSpec {
        name: "--formal-live-config",
        alias: None,
        kind: OptionKind::Scalar,
        conflict: Some("--formal-live-config"),
        value: "<path>",
        description: "externally trusted formal single-authority runtime configuration",
        default: None,
    },
    OptionSpec {
        name: "--chain-local-standalone-test",
        alias: None,
        kind: OptionKind::Switch,
        conflict: None,
        value: "",
        description: "explicit local provider standalone admission",
        default: None,
    },
    OptionSpec {
        name: "--agent-decision-source",
        alias: None,
        kind: OptionKind::Scalar,
        conflict: None,
        value: "<value>",
        description: "local provider exact admission tuple",
        default: None,
    },
    OptionSpec {
        name: "--agent-provider-backend",
        alias: None,
        kind: OptionKind::Scalar,
        conflict: None,
        value: "<value>",
        description: "local provider exact admission tuple",
        default: None,
    },
    OptionSpec {
        name: "--agent-provider-contract",
        alias: None,
        kind: OptionKind::Scalar,
        conflict: None,
        value: "<value>",
        description: "local provider exact admission tuple",
        default: None,
    },
    OptionSpec {
        name: "--agent-provider-transport",
        alias: None,
        kind: OptionKind::Scalar,
        conflict: None,
        value: "<value>",
        description: "local provider exact admission tuple",
        default: None,
    },
    OptionSpec {
        name: "--agent-execution-lane",
        alias: None,
        kind: OptionKind::Scalar,
        conflict: None,
        value: "<value>",
        description: "local provider exact admission tuple",
        default: None,
    },
    OptionSpec {
        name: "--reward-distfs-probe-max-sample-bytes",
        alias: None,
        kind: OptionKind::Scalar,
        conflict: None,
        value: "<n>",
        description: "DistFS probe policy override",
        default: None,
    },
    OptionSpec {
        name: "--reward-distfs-probe-ttl-ms",
        alias: None,
        kind: OptionKind::Scalar,
        conflict: None,
        value: "<n>",
        description: "DistFS probe policy override",
        default: None,
    },
    OptionSpec {
        name: "--reward-distfs-probe-allowed-clock-skew-ms",
        alias: None,
        kind: OptionKind::Scalar,
        conflict: None,
        value: "<n>",
        description: "DistFS probe policy override",
        default: None,
    },
    OptionSpec {
        name: "--reward-distfs-adaptive-max-checks-per-round",
        alias: None,
        kind: OptionKind::Scalar,
        conflict: None,
        value: "<n>",
        description: "DistFS probe policy override",
        default: None,
    },
    OptionSpec {
        name: "--reward-distfs-adaptive-backoff-base-ms",
        alias: None,
        kind: OptionKind::Scalar,
        conflict: None,
        value: "<n>",
        description: "DistFS probe policy override",
        default: None,
    },
    OptionSpec {
        name: "--reward-distfs-adaptive-backoff-max-ms",
        alias: None,
        kind: OptionKind::Scalar,
        conflict: None,
        value: "<n>",
        description: "DistFS probe policy override",
        default: None,
    },
    OptionSpec {
        name: "--reward-distfs-adaptive-multiplier-hash-mismatch",
        alias: None,
        kind: OptionKind::Scalar,
        conflict: None,
        value: "<n>",
        description: "DistFS probe policy override",
        default: None,
    },
    OptionSpec {
        name: "--reward-distfs-adaptive-multiplier-missing-sample",
        alias: None,
        kind: OptionKind::Scalar,
        conflict: None,
        value: "<n>",
        description: "DistFS probe policy override",
        default: None,
    },
    OptionSpec {
        name: "--reward-distfs-adaptive-multiplier-read-io-error",
        alias: None,
        kind: OptionKind::Scalar,
        conflict: None,
        value: "<n>",
        description: "DistFS probe policy override",
        default: None,
    },
    OptionSpec {
        name: "--reward-distfs-adaptive-multiplier-signature-invalid",
        alias: None,
        kind: OptionKind::Scalar,
        conflict: None,
        value: "<n>",
        description: "DistFS probe policy override",
        default: None,
    },
    OptionSpec {
        name: "--reward-distfs-adaptive-multiplier-timeout",
        alias: None,
        kind: OptionKind::Scalar,
        conflict: None,
        value: "<n>",
        description: "DistFS probe policy override",
        default: None,
    },
    OptionSpec {
        name: "--reward-distfs-adaptive-multiplier-unknown",
        alias: None,
        kind: OptionKind::Scalar,
        conflict: None,
        value: "<n>",
        description: "DistFS probe policy override",
        default: None,
    },
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CommandKind {
    ProvisionIdentity,
    IdentityReceipt,
    RebuildProof,
    ControlledHistory,
    ControlledBootstrap,
    ValidatorMembership,
    NodeLifecycle,
    Operation,
    LegacyMigration,
    LegacyRuntime,
    DurableSetControl,
    Dev,
    Inspect,
}

pub(crate) struct CommandSpec {
    pub kind: CommandKind,
    pub name: &'static str,
    pub usage: &'static str,
}
pub(crate) const OFFLINE_COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        name: "inspect",
        kind: CommandKind::Inspect,
        usage: "bootstrap|history|rebuild-proof --help (offline existing evidence; no state change or runtime readiness)",
    },
    CommandSpec {
        name: "dev",
        kind: CommandKind::Dev,
        usage: "node|local-provider|guarded-world --config PATH; prepare-config --output PATH -- source-options (typed engineering only; no formal readiness)",
    },
    CommandSpec {
        name: "durable-set",
        kind: CommandKind::DurableSetControl,
        usage: "arm|query --config <formal.json> --owner-instance <instance.json> --operation-id <id> (exact local owner; no activation grant)",
    },
    CommandSpec {
        name: "legacy-migrate",
        kind: CommandKind::LegacyMigration,
        usage: "plan|apply|resume|status (preserving stopped compatibility Observer migration)",
    },
    CommandSpec {
        name: "legacy-run",
        kind: CommandKind::LegacyRuntime,
        usage: "--config <legacy-launch.json> (run the migrated compatibility Observer)",
    },
    CommandSpec {
        name: "op",
        kind: CommandKind::Operation,
        usage: "plan|submit|status|resume|cancel (optional local operating-domain store; no chain authority)",
    },
    CommandSpec {
        name: "node",
        kind: CommandKind::NodeLifecycle,
        usage: "init|start|status|stop (local compatibility Observer lifecycle; preserves original chain identity)",
    },
    CommandSpec {
        name: "validator",
        kind: CommandKind::ValidatorMembership,
        usage: "list|status|wait|watch (authenticated service assertion; explicit --allow-controlled-prerequisite diagnostic policy; never BFT applied)",
    },
    CommandSpec {
        name: "provision-identity",
        kind: CommandKind::ProvisionIdentity,
        usage: "--config-dir <path> --node-id <id> (explicit identity preparation)",
    },
    CommandSpec {
        name: "identity-receipt",
        kind: CommandKind::IdentityReceipt,
        usage: "--config-dir <path> --node-id <id> (existing identity receipt)",
    },
    CommandSpec {
        name: "verify-rebuild-proof",
        kind: CommandKind::RebuildProof,
        usage: "(compatibility alias for inspect rebuild-proof; --help lists offline inputs)",
    },
    CommandSpec {
        name: "verify-controlled-history",
        kind: CommandKind::ControlledHistory,
        usage: "--trusted-config <external-json> --evidence <original-history-json> (compatibility alias for inspect history; offline only)",
    },
    CommandSpec {
        name: "verify-controlled-bootstrap",
        kind: CommandKind::ControlledBootstrap,
        usage: "--trusted-config <external-json> --evidence <original-proof-json> (compatibility alias for inspect bootstrap; independent issuer policy required)",
    },
];

pub(crate) fn option(arg: &str) -> Result<&'static OptionSpec, String> {
    let spec = OPTIONS
        .iter()
        .find(|s| s.name == arg || s.alias == Some(arg))
        .ok_or_else(|| format!("unknown option: {arg}"))?;
    if spec.alias == Some(arg) {
        eprintln!(
            "deprecated {arg}: use {} (fetch permission only)",
            spec.name
        );
    }
    Ok(spec)
}

pub(crate) fn check_option_occurrence(
    spec: &OptionSpec,
    seen: &mut BTreeSet<String>,
) -> Result<(), String> {
    let key = spec.conflict.unwrap_or(spec.name);
    if spec.kind != OptionKind::List && !seen.insert(key.to_string()) {
        return Err(format!(
            "{}: repeated scalar or conflicting option ({key})",
            spec.name
        ));
    }
    Ok(())
}

pub(crate) fn check_required_value(spec: &OptionSpec, next: Option<&str>) -> Result<(), String> {
    if spec.kind == OptionKind::Switch {
        return Ok(());
    }
    let next = next
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("{} requires a non-empty value", spec.name))?;
    if next.starts_with("--") && next != "--" {
        return Err(format!(
            "{} requires a value before {next}; use {} -- <literal> for a leading -- value",
            spec.name, spec.name
        ));
    }
    Ok(())
}

pub(crate) fn parse_required_value<'a, I>(
    iter: &mut std::iter::Peekable<I>,
    flag: &str,
) -> Result<String, String>
where
    I: Iterator<Item = &'a str>,
{
    let Some(value) = iter.next() else {
        return Err(format!("{flag} requires a value"));
    };
    let value = value.trim();
    let value = if value == "--" {
        iter.next()
            .ok_or_else(|| format!("{flag} requires a value after --"))?
    } else if value.starts_with("--") {
        return Err(format!(
            "{flag} requires a value before {value}; use {flag} -- <literal> for a leading -- value"
        ));
    } else {
        value
    };
    let value = value.trim();
    if value.is_empty() {
        return Err(format!("{flag} requires a non-empty value"));
    }
    Ok(value.to_string())
}

pub(crate) fn offline_command(name: &str) -> Option<CommandKind> {
    OFFLINE_COMMANDS
        .iter()
        .find(|s| s.name == name)
        .map(|s| s.kind)
}
