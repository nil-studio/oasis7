//! Explicit engineering command boundary. Runtime identities and all storage
//! settings come from the typed guarded configuration, never global argv.
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GuardedWorldOptions {
    pub config_path: PathBuf,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum DevCommandOptions {
    GuardedWorld(GuardedWorldOptions),
}

pub(super) fn parse<'a>(
    args: impl Iterator<Item = &'a str>,
) -> Result<Option<DevCommandOptions>, String> {
    let mut args = args.peekable();
    match args.next() {
        None | Some("--help") if args.peek().is_none() => return Ok(None),
        Some("guarded-world") => {}
        Some(command) => {
            return Err(format!(
                "unsupported dev command {command}; expected guarded-world"
            ));
        }
        None => return Ok(None),
    }
    if args.peek() == Some(&"--help") {
        args.next();
        if args.peek().is_none() {
            return Ok(None);
        }
        return Err("dev guarded-world --help accepts no additional arguments".into());
    }
    let mut config_path = None;
    while let Some(flag) = args.next() {
        if flag != "--config" {
            return Err(format!("{flag} is inapplicable to dev guarded-world"));
        }
        if config_path.is_some() {
            return Err("dev guarded-world repeated scalar --config".into());
        }
        let mut value = args
            .next()
            .ok_or("dev guarded-world --config requires a value")?;
        if value == "--" {
            value = args.next().ok_or("--config escape requires a value")?;
        } else if value.starts_with("--") {
            return Err("dev guarded-world --config requires a value".into());
        }
        if value.trim().is_empty() {
            return Err("dev guarded-world --config requires a non-empty value".into());
        }
        config_path = Some(PathBuf::from(value));
    }
    Ok(Some(DevCommandOptions::GuardedWorld(GuardedWorldOptions {
        config_path: config_path.ok_or("dev guarded-world requires --config PATH")?,
    })))
}

pub(super) fn run<'a>(args: impl Iterator<Item = &'a str>) -> Result<(), String> {
    let args: Vec<&str> = args.collect();
    if args.as_slice() == ["prepare-config", "--help"] {
        println!(
            "dev prepare-config --output PATH -- legacy-engineering-options (validate typed engineering configuration before writing; no node startup)"
        );
        return Ok(());
    }
    if args.first() == Some(&"prepare-config") {
        return super::development_runtime::prepare_config(args.into_iter().skip(1));
    }
    if matches!(args.first(), Some(&"node") | Some(&"local-provider")) {
        let require_provider = args[0] == "local-provider";
        let mut tail = args.into_iter().skip(1);
        if tail.clone().collect::<Vec<_>>() == ["--help"] {
            println!(
                "dev {} --config PATH (strict typed engineering configuration; no network-tier manifest)",
                if require_provider {
                    "local-provider"
                } else {
                    "node"
                }
            );
            return Ok(());
        }
        if tail.next() != Some("--config") {
            return Err("dev node/local-provider requires only --config PATH".into());
        }
        let path = tail
            .next()
            .filter(|s| !s.trim().is_empty() && !s.starts_with("--"))
            .ok_or("--config requires a path")?;
        if tail.next().is_some() {
            return Err("dev node/local-provider accepts only one --config PATH".into());
        }
        let config = super::development_runtime::read_config(std::path::Path::new(path))?;
        return super::development_runtime::run(config, require_provider);
    }
    match parse(args.into_iter())? {
        None => {
            println!(
                "dev node --config PATH\ndev local-provider --config PATH\ndev guarded-world --config PATH\ndev prepare-config --output PATH -- source-options\nEngineering configurations only. Synthetic counting does not establish independent consensus or formal readiness."
            );
            Ok(())
        }
        Some(DevCommandOptions::GuardedWorld(options)) => {
            super::controlled_live_runtime::run(options)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dev_help_and_invalid_inputs_have_no_runtime_effects() {
        for args in [
            vec!["--help"],
            vec!["node", "--help"],
            vec!["local-provider", "--help"],
            vec!["prepare-config", "--help"],
        ] {
            run(args.into_iter()).unwrap();
        }
        for args in [
            vec!["node", "--config", "--node-id"],
            vec![
                "local-provider",
                "--config",
                "missing.json",
                "--node-id",
                "x",
            ],
            vec!["node", "--config", "a", "--config", "b"],
        ] {
            assert!(run(args.into_iter()).is_err());
        }
    }
    #[test]
    fn guarded_typed_command_has_one_config_and_no_identity_overrides() {
        assert_eq!(
            parse(["guarded-world", "--config", "guarded.json"].into_iter()).unwrap(),
            Some(DevCommandOptions::GuardedWorld(GuardedWorldOptions {
                config_path: "guarded.json".into()
            }))
        );
        for extra in [
            "--node-id",
            "--world-id",
            "--reward-runtime-disable",
            "--node-no-auto-attest-all",
            "--status-bind",
            "--node-tick-ms",
        ] {
            assert!(
                parse(
                    [
                        "guarded-world",
                        "--config",
                        "guarded.json",
                        extra,
                        "default"
                    ]
                    .into_iter()
                )
                .is_err()
            );
        }
        assert!(parse(["guarded-world", "--config", "--node-id"].into_iter()).is_err());
        assert!(parse(["guarded-world", "--config", "a", "--config", "b"].into_iter()).is_err());
        assert!(
            parse(["guarded-world", "--help"].into_iter())
                .unwrap()
                .is_none()
        );
        assert!(parse(["guarded-world", "--help", "--config", "a"].into_iter()).is_err());
    }

    #[test]
    fn node_and_local_provider_reject_invalid_typed_network_budget_at_the_real_cli_entry() {
        // Both public commands must fail during typed --config decoding,
        // before constructing a NodeRuntime, local provider or network.
        let config =
            super::super::development_runtime::from_options(&super::super::CliOptions::default())
                .unwrap();
        let mut wire = serde_json::to_value(config).unwrap();
        let mut nonce = [0_u8; 16];
        getrandom::fill(&mut nonce).unwrap();
        let path = std::env::temp_dir().join(format!(
            "oasis7-dev-budget-cli-{}.json",
            hex::encode(nonce)
        ));
        struct Cleanup(std::path::PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = std::fs::remove_file(&self.0);
            }
        }
        let _cleanup = Cleanup(path.clone());
        let path = path.to_str().unwrap();

        let base = serde_json::json!({
            "profile": "v1-discovery-resource-budget-1",
            "connections_total": 3,
            "pending_connections_total": 2,
            "pending_inbound_unknown": 1,
            "pending_outbound": 2,
            "connections_per_peer": 1,
            "handshake_timeout_ms": 2000,
            "bytes_in_per_window": 1024,
            "bytes_out_per_window": 2048,
            "bytes_total_per_window": 3072
        });
        let mut missing = base.clone();
        missing.as_object_mut().unwrap().remove("pending_inbound_unknown");
        let mut zero = base.clone();
        zero["bytes_out_per_window"] = serde_json::json!(0);
        let mut unknown = base;
        unknown["profile"] = serde_json::json!("unsupported-budget-v2");
        for invalid in [missing, zero, unknown] {
            wire["runtime"]["p2p_resource_budget"] = invalid;
            std::fs::write(path, serde_json::to_vec(&wire).unwrap()).unwrap();
            for command in ["node", "local-provider"] {
                let error = run([command, "--config", path].into_iter()).unwrap_err();
                assert!(
                    error.starts_with("typed engineering config:"),
                    "{command} must reject the network budget before provider/runtime admission: {error}"
                );
            }
        }
    }
}
