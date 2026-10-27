//! Singleton actual-World Node consumer of an independently provisioned wire-3 provider.
use super::*;
#[cfg(not(test))]
use crate::execution_bridge::FormalBftHeaderNodeDriverV3;
#[cfg(test)]
use crate::execution_bridge_real_tests::real_execution_bridge::FormalBftHeaderNodeDriverV3;
use oasis7_consensus::tendermint_v1::BftTimeoutsV1;
use oasis7_distfs::protected_bft_v3::{
    BftSignerBindingV3, BftSignerKeyIdentityV3, BftSignerTrustV3, RemoteBftSignerProviderV3,
};
use oasis7_node::{NodeConfig, NodeReplicationConfig, NodeRole, NodeRuntime};
use std::{
    net::SocketAddr,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    schema_version: u32,
    trust_config_path: PathBuf,
    target: BftProfileTargetV2,
    handoff_material_path: PathBuf,
    cas_directory: PathBuf,
    records_directory: PathBuf,
    replication_directory: PathBuf,
    validator_id: String,
    player_id: String,
    operator_instance_id: String,
    provisioned_signing_keys: Vec<BftSignerKeyIdentityV3>,
    timeouts: BftTimeoutsV1,
    provider_address: SocketAddr,
    provider_trust: Option<BftSignerTrustV3>,
    requester_signing_key_path: PathBuf,
    provider_timeout_ms: u64,
    completion_timeout_ms: u64,
}

/// Public operator descriptor only. A provider must independently admit H;
/// serializing this report cannot mint the opaque binding capability.
#[derive(Serialize)]
pub(crate) struct BindingReport {
    pub(crate) schema_version: &'static str,
    pub(crate) binding: BftSignerBindingV3,
    pub(crate) binding_digest: String,
}
struct Prepared {
    driver: FormalBftHeaderNodeDriverV3,
    binding: BftSignerBindingV3,
    requester: ed25519_dalek::SigningKey,
}

#[derive(Serialize)]
struct Receipt {
    height: u64,
    header_id: String,
    action_root: String,
    action_count: usize,
}
#[derive(Serialize)]
struct Report {
    schema_version: &'static str,
    receipts: Vec<Receipt>,
}

pub(super) fn run<'a>(
    mut args: impl Iterator<Item = &'a str>,
    shutdown: Option<Arc<AtomicBool>>,
) -> Result<(), String> {
    let mut config = None;
    let mut actions = None;
    let mut print_binding = false;
    while let Some(option) = args.next() {
        if option == "--print-binding" {
            if print_binding {
                return Err("duplicate header Node option".into());
            }
            print_binding = true;
            continue;
        }
        let slot = match option {
            "--config" => &mut config,
            "--actions" => &mut actions,
            _ => return Err("unsupported header Node option".into()),
        };
        if slot.is_some() {
            return Err("duplicate header Node option".into());
        }
        let path = PathBuf::from(
            args.next()
                .ok_or("header Node option requires an absolute path")?,
        );
        if !path.is_absolute() {
            return Err("header Node paths must be absolute".into());
        }
        *slot = Some(path);
    }
    let config = config.ok_or("bft-header node requires --config")?;
    if print_binding {
        if actions.is_some() {
            return Err("header Node --print-binding cannot be combined with --actions".into());
        }
        let report = super::prepare_node_binding_report(&config)?;
        println!(
            "{}",
            serde_json::to_string(&report).map_err(|_| "encode public header Node binding")?
        );
        return Ok(());
    }
    let report = from_paths(
        &config,
        &actions.ok_or("bft-header node requires --actions")?,
        shutdown,
    )?;
    println!(
        "{}",
        serde_json::to_string(&report).map_err(|_| "encode public header Node receipt")?
    );
    Ok(())
}

fn from_paths(
    config_path: &Path,
    actions_path: &Path,
    shutdown: Option<Arc<AtomicBool>>,
) -> Result<Report, String> {
    let config = load_config(config_path)?;
    let trust = config
        .provider_trust
        .clone()
        .ok_or("header Node actions require explicit pinned provider_trust")?;
    let input = read_private(actions_path, BFT_MAX_ACTIONS_CBOR_BYTES_V1 as u64)?;
    let actions = oasis7_consensus::formal_bft_payload_v3::decode_private_actions_v3(&input)
        .map_err(|_| "header Node canonical private actions rejected")?;
    if actions.is_empty()
        || actions
            .iter()
            .any(|a| a.submitter_player_id != config.player_id)
    {
        return Err("header Node requires nonempty actions for its configured player".into());
    }
    let Prepared {
        driver,
        binding,
        requester,
    } = prepare(&config)?;
    let provider = Arc::new(
        RemoteBftSignerProviderV3::new(
            config.provider_address,
            Duration::from_millis(config.provider_timeout_ms),
            binding.clone(),
            requester,
            trust,
        )
        .map_err(|_| "header Node explicit remote signer trust rejected")?,
    );
    let node_config = NodeConfig::new(
        config.validator_id,
        binding.scope.world_id.clone(),
        NodeRole::Sequencer,
    )
    .and_then(|c| c.with_player_id(config.player_id))
    .map_err(|_| "header Node identity rejected")?
    .with_replication(
        NodeReplicationConfig::new(config.replication_directory)
            .map_err(|_| "header Node replication storage rejected")?,
    );
    let mut runtime = NodeRuntime::new(node_config)
        .with_replication_network_consensus_enabled(false)
        .with_execution_hook(driver)
        .with_tendermint_v3(binding, provider)
        .map_err(|_| "header Node runtime configuration rejected")?;
    runtime
        .start()
        .map_err(|_| "header Node actual startup recovery rejected")?;
    let result = submit_and_wait(
        &runtime,
        &actions,
        Duration::from_millis(config.completion_timeout_ms),
        shutdown,
    );
    let stopped = runtime
        .stop()
        .map_err(|_| "header Node could not stop cleanly");
    let report = result?;
    stopped?;
    Ok(report)
}

fn load_config(config_path: &Path) -> Result<Config, String> {
    let config: Config = serde_json::from_slice(&read_private(config_path, 2 * 1024 * 1024)?)
        .map_err(|_| "invalid header Node config")?;
    if config.schema_version != 1
        || !(1..=60_000).contains(&config.provider_timeout_ms)
        || !(1..=600_000).contains(&config.completion_timeout_ms)
        || config.provider_address.port() == 0
        || config.provider_address.ip().is_unspecified()
    {
        return Err("unsupported header Node schema, endpoint or timeout".into());
    }
    config.target.validate()?;
    Ok(config)
}

pub(super) fn prepare_binding_report(config_path: &Path) -> Result<BindingReport, String> {
    let config = load_config(config_path)?;
    let Prepared { binding, .. } = prepare(&config)?;
    let binding_digest = binding
        .digest()
        .map_err(|_| "header Node binding digest rejected")?;
    Ok(BindingReport {
        schema_version: "oasis7.formal_bft_header_node_binding.v3",
        binding,
        binding_digest,
    })
}

fn prepare(config: &Config) -> Result<Prepared, String> {
    // All private inputs and existing directories are checked before creating Node state.
    let cas = require_existing_directory(&config.cas_directory, "private Node CAS")?;
    crate::controlled_live_config::require_private_owned(&cas, true)?;
    for path in [&config.records_directory, &config.replication_directory] {
        let path = require_existing_directory(path, "private Node state")?;
        crate::controlled_live_config::require_private_owned(&path, true)?;
    }
    let directories = [
        &cas,
        &config.records_directory,
        &config.replication_directory,
    ]
    .into_iter()
    .map(std::fs::canonicalize)
    .collect::<Result<Vec<_>, _>>()
    .map_err(|_| "header Node state directory resolution rejected")?;
    for (index, left) in directories.iter().enumerate() {
        if directories
            .iter()
            .skip(index + 1)
            .any(|right| left.starts_with(right) || right.starts_with(left))
        {
            return Err(
                "header Node CAS, driver and replication directories must be disjoint".into(),
            );
        }
    }
    if !config.requester_signing_key_path.is_absolute() {
        return Err("header Node requester key path must be absolute".into());
    }
    let requester = crate::controlled_live_config::read_existing_signing_key(
        &config.requester_signing_key_path,
    )
    .map_err(|_| "header Node requester key rejected")?;
    let anchor = load_qualified_handoff_anchor(&config.trust_config_path)
        .map_err(|_| "header Node qualified H trust rejected")?;
    if !config.handoff_material_path.is_absolute() {
        return Err("header Node H material path must be absolute".into());
    }
    crate::controlled_live_config::require_private_owned(&config.handoff_material_path, false)?;
    let material = read_qualified_handoff_material(&config.handoff_material_path)
        .map_err(|_| "header Node qualified H material rejected")?;
    let store = LocalCasStore::new(cas);
    let source = Arc::new(CasBusinessIndexReader {
        store: store.clone(),
    });
    let driver = FormalBftHeaderNodeDriverV3::open_from_handoff(
        &config.records_directory,
        store,
        anchor,
        config.target.clone(),
        material,
        source,
    )
    .map_err(|_| "header Node actual qualified H driver recovery rejected")?;
    let binding = BftSignerBindingV3::new(
        driver.handoff().clone(),
        driver.initial_schedule().clone(),
        driver.material_identity().into(),
        config.validator_id.clone(),
        config.provisioned_signing_keys.clone(),
        config.timeouts,
        config.operator_instance_id.clone(),
        hex::encode(requester.verifying_key().to_bytes()),
    )
    .map_err(|_| "header Node trusted signer binding rejected")?;
    Ok(Prepared {
        driver,
        binding,
        requester,
    })
}

fn submit_and_wait(
    runtime: &NodeRuntime,
    actions: &[oasis7_node::NodeConsensusAction],
    timeout: Duration,
    shutdown: Option<Arc<AtomicBool>>,
) -> Result<Report, String> {
    let deadline = Instant::now() + timeout;
    for action in actions {
        runtime
            .submit_consensus_action_payload(action.action_id, action.payload_cbor.clone())
            .map_err(|_| "header Node private action admission rejected")?;
    }
    let mut remaining = actions.to_vec();
    let mut receipts = Vec::new();
    let signal = runtime.committed_action_batches_handle();
    loop {
        for batch in runtime.drain_committed_action_batches() {
            if batch.actions.is_empty() {
                continue;
            }
            if batch.actions.iter().any(|committed| {
                actions.iter().any(|requested| {
                    requested.action_id == committed.action_id && requested != committed
                })
            }) {
                return Err(
                    "header Node committed identity conflicts with submitted private input".into(),
                );
            }
            // Recovery must finish an earlier original private reservation
            // before this invocation can propose its newly submitted actions.
            // Those unrelated batches are real results, but cannot complete
            // any action in this invocation.
            if !batch
                .actions
                .iter()
                .any(|action| remaining.contains(action))
            {
                continue;
            }
            remaining.retain(|a| !batch.actions.contains(a));
            // The V3 worker emits this only after actual durable World install and
            // exact protected-provider applied confirmation. Never serialize actions.
            receipts.push(Receipt {
                height: batch.height,
                header_id: batch.block_hash,
                action_root: batch.action_root,
                action_count: batch.actions.len(),
            });
        }
        if remaining.is_empty() {
            return Ok(Report {
                schema_version: "oasis7.formal_bft_header_node_receipt.v3",
                receipts,
            });
        }
        if shutdown.as_ref().is_some_and(|s| s.load(Ordering::Acquire)) {
            return Err("header Node interrupted before complete applied receipt".into());
        }
        if runtime
            .bft_v3_status()
            .is_some_and(|s| s.blocked_reason.is_some())
        {
            return Err("header Node blocked before complete applied receipt".into());
        }
        if Instant::now() >= deadline {
            return Err("header Node timed out before complete applied receipt".into());
        }
        signal.wait_for_batches(Duration::from_millis(25));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn node_options_reject_ambiguous_or_relative_inputs_before_loading() {
        assert!(
            run(
                ["--config", "relative", "--actions", "/private/actions"].into_iter(),
                None
            )
            .is_err()
        );
        assert!(run(["--config", "/one", "--config", "/two"].into_iter(), None).is_err());
        assert!(
            run(
                ["--config", "/one", "--history", "/private/actions"].into_iter(),
                None
            )
            .is_err()
        );
        assert!(run(["--actions", "/private/actions"].into_iter(), None).is_err());
    }
    #[test]
    fn binding_mode_rejects_actions_and_duplicate_mode_before_loading() {
        assert!(
            run(
                [
                    "--config",
                    "/missing",
                    "--print-binding",
                    "--actions",
                    "/missing-actions"
                ]
                .into_iter(),
                None
            )
            .unwrap_err()
            .contains("cannot be combined")
        );
        assert!(
            run(
                ["--config", "/missing", "--print-binding", "--print-binding"].into_iter(),
                None
            )
            .unwrap_err()
            .contains("duplicate")
        );
    }
    #[test]
    fn public_receipt_omits_private_payloads() {
        let report = Report {
            schema_version: "oasis7.formal_bft_header_node_receipt.v3",
            receipts: vec![Receipt {
                height: 9,
                header_id: "a".repeat(64),
                action_root: "b".repeat(64),
                action_count: 2,
            }],
        };
        let value = serde_json::to_value(report).unwrap();
        assert_eq!(value["receipts"][0]["height"], 9);
        assert_eq!(value["receipts"][0].as_object().unwrap().len(), 4);
        assert!(value["receipts"][0].get("actions").is_none());
    }
}
