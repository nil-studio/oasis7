//! Actual local NodeRuntime engineering entry. Deployment remains deferred by
//! the user; this opt-in path supplies no formal readiness or network finality.
use super::{
    NodeConfig, NodeRole, NodeRuntime, build_feedback_submit_signer,
    controlled_live_config::GuardedAuthority, dev_cli::GuardedWorldOptions,
    execution_bridge::controlled_live::GuardedExecutionDriver, node_keypair_config,
    status_server_support,
};
use std::{
    net::TcpListener,
    sync::{Arc, Mutex},
    time::Duration,
};

pub(super) struct GuardedConnectionContext {
    pub world_id: String,
    pub config_path: std::path::PathBuf,
    pub read_authority: super::controlled_live_config::GuardedReadAuthority,
}

pub(super) fn run(options: GuardedWorldOptions) -> Result<(), String> {
    let external = options.config_path.as_path();
    let authority = GuardedAuthority::load(external)?;
    super::controlled_live_config::require_private_owned(
        &authority.config.node_keypair_directory,
        true,
    )?;
    let (keypair, key_path) = node_keypair_config::read_existing_node_keypair_in_secure_config_dir(
        &authority.config.node_keypair_directory,
    )?;
    super::controlled_live_config::require_private_owned(&key_path, false)?;
    let signer = build_feedback_submit_signer(&authority.config.node_id, &keypair)?;
    let records = authority.config.records_directory.clone();
    let bind = authority.config.status_bind;
    let node_id = authority.config.node_id.clone();
    let world_id = authority.policy.trust.world_id.clone();
    let interval = authority.config.node_tick_ms;
    // Existing local single-validator decision machinery determines the
    // decision. Its continuation is engineering recovery material, not a
    // signature proof of network PoS approval.
    let config = NodeConfig::new(&node_id, &world_id, NodeRole::Sequencer)
        .and_then(|c| c.with_tick_interval(Duration::from_millis(interval)))
        .map_err(|e| format!("guarded local node configuration: {e:?}"))?
        .with_require_execution_on_commit(true)
        .with_require_peer_execution_hashes(false)
        .with_auto_attest_all_validators(true);
    let driver = GuardedExecutionDriver::new(authority, records.clone())?;
    let bootstrap = driver.bootstrap()?;
    let context = GuardedConnectionContext {
        world_id,
        config_path: options.config_path,
        read_authority: driver.read_authority(),
    };
    let listener = TcpListener::bind(bind).map_err(|e| format!("guarded loopback ingress: {e}"))?;
    let mut node = NodeRuntime::new(config)
        .with_local_execution_bootstrap(bootstrap)
        .with_execution_hook(driver);
    node.start()
        .map_err(|e| format!("guarded NodeRuntime start: {e:?}"))?;
    let runtime = Arc::new(Mutex::new(node));
    tracing::info!(
        scope = super::controlled_live_config::ENGINEERING_SCOPE,
        "local engineering checkpoint prerequisite ingress started"
    );
    for stream in listener.incoming() {
        let stream = match stream {
            Ok(stream) => stream,
            Err(error) => {
                runtime
                    .lock()
                    .map_err(|_| "guarded runtime lock unavailable")?
                    .stop()
                    .map_err(|e| format!("guarded stop: {e:?}"))?;
                return Err(format!("guarded loopback ingress failed: {error}"));
            }
        };
        if let Err(reason) = status_server_support::handle_guarded_connection(
            stream, &runtime, &context, &records, &signer,
        ) {
            tracing::warn!(error = %reason, "guarded service request rejected");
        }
    }
    Err("guarded loopback ingress ended".into())
}
