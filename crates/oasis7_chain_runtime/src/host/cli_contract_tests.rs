use super::{
    build_replication_fetch_requester_allowlist, build_replication_remote_writer_allowlist,
    parse_options,
};

#[test]
fn parse_options_rejects_missing_value_without_consuming_a_switch() {
    let error = parse_options(["--node-id", "--reward-runtime-disable"].into_iter()).unwrap_err();
    assert!(error.contains("--node-id requires a value before --reward-runtime-disable"));
    let options =
        parse_options(["--node-id", "--", "--literal", "--reward-runtime-disable"].into_iter())
            .unwrap();
    assert_eq!(options.node_id, "--literal");
    assert!(!options.reward_runtime_enabled);
}

#[test]
fn parse_options_rejects_repeated_scalars_and_opposite_switches() {
    for args in [
        vec!["--node-id", "first", "--node-id", "second"],
        vec!["--reward-runtime-enable", "--reward-runtime-disable"],
        vec![
            "--p2p-detected-relay-available",
            "--p2p-detected-relay-unavailable",
        ],
        vec![
            "--p2p-detected-probe-stable",
            "--p2p-detected-probe-unstable",
        ],
        vec![
            "--p2p-detected-reachability",
            "public",
            "--p2p-clear-detected-reachability",
        ],
        vec![
            "--reward-distfs-probe-per-tick",
            "1",
            "--reward-distfs-probe-per-tick",
            "2",
        ],
    ] {
        assert!(
            parse_options(args.into_iter())
                .unwrap_err()
                .contains("repeated scalar or conflicting")
        );
    }
}

#[test]
fn formal_live_profile_is_explicit_and_exclusive() {
    let options = parse_options(["--formal-live-config", "node.json"].into_iter()).unwrap();
    assert_eq!(
        options.formal_live_config.as_deref(),
        Some(std::path::Path::new("node.json"))
    );
    assert!(options.guarded_initial_config.is_none());

    assert!(
        parse_options(
            [
                "--formal-live-config",
                "node.json",
                "--guarded-initial-config",
                "engineering.json"
            ]
            .into_iter()
        )
        .unwrap_err()
        .contains("repeated scalar or conflicting")
    );

    assert!(
        parse_options(["--formal-live-config", "node.json", "--node-id", "override"].into_iter())
            .unwrap_err()
            .contains("cannot be combined with --formal-live-config")
    );
}

#[test]
fn parse_options_rejects_rebinding_validator_identity() {
    assert!(
        parse_options(
            [
                "--node-validator",
                "alice:100",
                "--node-validator",
                "alice:200"
            ]
            .into_iter()
        )
        .unwrap_err()
        .contains("duplicate validator identity")
    );
    let first = format!("alice:{}", "a".repeat(64));
    let second = format!("alice:{}", "b".repeat(64));
    assert!(
        parse_options(
            [
                "--node-validator-signer-public-key",
                first.as_str(),
                "--node-validator-signer-public-key",
                second.as_str()
            ]
            .into_iter()
        )
        .unwrap_err()
        .contains("duplicate validator signer binding")
    );
}

#[test]
fn parse_options_legacy_requester_alias_keeps_fetch_only_permissions() {
    let options = parse_options(
        [
            "--replication-remote-writer-public-key",
            "cccc",
            "--replication-fetch-requester-public-key",
            "dddd",
        ]
        .into_iter(),
    )
    .unwrap();
    let validators = ["aaaa".to_string()];
    assert_eq!(
        build_replication_remote_writer_allowlist(validators.iter()),
        vec!["aaaa"]
    );
    assert_eq!(
        build_replication_fetch_requester_allowlist(
            validators.iter(),
            &options.replication_fetch_requester_public_keys
        ),
        vec!["aaaa", "cccc", "dddd"]
    );
}

fn dispatched_options(source: &str) -> std::collections::BTreeSet<&str> {
    source
        .lines()
        .filter_map(|line| {
            let (arm, _) = line.split_once("=>")?;
            let name = arm.trim().strip_prefix('"')?.strip_suffix('"')?;
            name.starts_with("--").then_some(name)
        })
        .collect()
}

#[test]
fn cli_help_catalog_exactly_covers_flat_assignment_dispatch() {
    use crate::cli::cli_contract::{OPTIONS, OptionKind};
    let mut dispatched = dispatched_options(include_str!("cli.rs"));
    dispatched.extend(dispatched_options(include_str!("distfs_probe_runtime.rs")));
    let names: std::collections::BTreeSet<_> = OPTIONS.iter().map(|s| s.name).collect();
    assert_eq!(
        names, dispatched,
        "metadata and field dispatch must move together"
    );
    assert_eq!(
        OPTIONS.len(),
        names.len(),
        "catalog canonical names are unique"
    );
    let help = crate::cli::cli_help::render_help();
    let mut accepted = names;
    for spec in OPTIONS {
        assert_eq!(
            spec.kind == OptionKind::Switch,
            spec.value.is_empty(),
            "{} value classification",
            spec.name
        );
        assert_eq!(
            help.matches(&format!("  {} ", spec.name)).count(),
            1,
            "{} appears exactly once in generated help",
            spec.name
        );
        if let Some(alias) = spec.alias {
            assert!(accepted.insert(alias), "alias is unique");
            assert_eq!(
                crate::cli::cli_contract::option(alias).unwrap().name,
                spec.name
            );
        }
        if let Some(group) = spec.conflict {
            assert!(OPTIONS.iter().any(|s| s.name == group));
        }
        if spec.kind != OptionKind::Switch {
            let error =
                parse_options([spec.name, "--reward-runtime-disable"].into_iter()).unwrap_err();
            assert!(
                error.contains("requires a value before"),
                "{}: {error}",
                spec.name
            );
        }
    }
}

#[test]
fn cli_help_defaults_and_hint_sources_come_from_runtime_defaults() {
    use crate::cli::{
        CliOptions,
        cli_contract::{OFFLINE_COMMANDS, OPTIONS, offline_command},
    };
    let defaults = CliOptions::default();
    let help = crate::cli::cli_help::render_help();
    for spec in OPTIONS {
        if let Some(default) = spec.default {
            let repeatable = if spec.kind == crate::cli::cli_contract::OptionKind::List {
                " (repeatable)"
            } else {
                ""
            };
            assert!(
                help.contains(&format!(
                    "{}{repeatable} (default: {})",
                    spec.description,
                    default(&defaults)
                )),
                "{} runtime default must be rendered",
                spec.name
            );
        }
    }
    assert!(help.contains("Omitted relay/probe hints use live detection"));
    assert!(!help.contains("relay detection to available (default)"));
    let dispatched_commands: std::collections::BTreeSet<_> = include_str!("identity_receipt.rs")
        .lines()
        .filter_map(|line| {
            let (arm, _) = line.split_once("=>")?;
            arm.trim().strip_prefix("CommandKind::")
        })
        .collect();
    let catalog_commands: std::collections::BTreeSet<_> = OFFLINE_COMMANDS
        .iter()
        .map(|s| format!("{:?}", s.kind))
        .collect();
    assert_eq!(
        dispatched_commands,
        catalog_commands
            .iter()
            .map(String::as_str)
            .collect::<std::collections::BTreeSet<_>>(),
        "utility dispatch and catalog must match exactly"
    );
    let mut commands = std::collections::BTreeSet::new();
    for spec in OFFLINE_COMMANDS {
        assert!(commands.insert(spec.name));
        assert_eq!(offline_command(spec.name), Some(spec.kind));
        assert!(help.contains(&format!("  {} {}", spec.name, spec.usage)));
    }
    assert_eq!(offline_command("unknown-command"), None);
}

#[test]
fn guarded_legacy_adapter_rejects_all_explicit_inapplicable_options() {
    let legacy = parse_options(
        [
            "--guarded-initial-config",
            "guarded.json",
            "--reward-runtime-disable",
        ]
        .into_iter(),
    )
    .unwrap();
    assert_eq!(
        legacy.guarded_initial_config.as_deref(),
        Some(std::path::Path::new("guarded.json"))
    );
    let defaults = crate::CliOptions::default();
    let tick_default = defaults.node_tick_ms.to_string();
    for (flag, value) in [
        ("--node-id", defaults.node_id.as_str()),
        ("--world-id", defaults.world_id.as_str()),
        ("--status-bind", defaults.status_bind.as_str()),
        ("--storage-profile", "dev_local"),
        ("--node-tick-ms", tick_default.as_str()),
        ("--node-gossip-peer", "127.0.0.1:1"),
    ] {
        assert!(
            parse_options(
                [
                    "--guarded-initial-config",
                    "guarded.json",
                    "--reward-runtime-disable",
                    flag,
                    value
                ]
                .into_iter()
            )
            .unwrap_err()
            .contains("inapplicable"),
            "{flag}"
        );
    }
    for flag in [
        "--node-no-auto-attest-all",
        "--node-auto-attest-all",
        "--reward-runtime-enable",
    ] {
        assert!(
            parse_options(["--guarded-initial-config", "guarded.json", flag].into_iter())
                .unwrap_err()
                .contains("inapplicable"),
            "{flag}"
        );
    }
}
