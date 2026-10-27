//! Serve an existing exact header cut; no import or current authority source.
use super::*;
use std::{
    net::SocketAddr,
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::Duration,
};

pub(super) fn run(config_path: &Path, shutdown: Option<Arc<AtomicBool>>) -> Result<(), String> {
    let config = load_config(config_path)?;
    let bind: SocketAddr = config
        .bind
        .as_deref()
        .ok_or("bft-header serve requires bind")?
        .parse()
        .map_err(|_| "bft-header bind must be a literal socket address")?;
    if !bind.ip().is_loopback() || bind.port() == 0 {
        return Err("bft-header bind must be a nonzero loopback address".into());
    }
    let key_path = config
        .service_signing_key_path
        .as_deref()
        .ok_or("bft-header serve requires service_signing_key_path")?;
    if !key_path.is_absolute() {
        return Err("bft-header service key path must be absolute".into());
    }
    let signer = crate::controlled_live_config::read_existing_signing_key(key_path)?;
    let records = require_existing_directory(&config.ledger_directory, "existing header ledger")?;
    let LoadedH {
        anchor,
        material,
        store,
    } = load_h(&config)?;
    let source = Arc::new(CasBusinessIndexReader {
        store: store.clone(),
    });
    let mut execution = FormalBftHeaderLedgerExecutionV3::open_from_handoff(
        &records,
        store,
        anchor.clone(),
        config.target.clone(),
        material,
        source,
        false,
    )
    .map_err(|_| "header read service could not open the original qualified ledger")?;
    let cut = execution
        .verified_cut(config.exact_height, &config.exact_header_id)
        .map_err(|_| "header read service exact durable replay rejected")?;
    use oasis7::world_service::formal_bft_header_read_service::{
        FormalBftHeaderReadPublicationInputV4, FormalBftHeaderReadPublicationV4,
    };
    let publication =
        FormalBftHeaderReadPublicationV4::new(FormalBftHeaderReadPublicationInputV4 {
            handoff_anchor: anchor,
            target_config: config.target,
            exact_target: oasis7_client_api::world_service::FormalBftHeaderReadTargetV4 {
                height: config.exact_height,
                header_id: config.exact_header_id,
            },
            certified_prefix: cut.certified_prefix(),
            committed_world: cut.world(),
            result_tree: cut.result_tree(),
        })
        .map_err(|_| "header read service actual cut differs from its public proof")?;
    let service = Arc::new(
        crate::formal_bft_header_http::FormalBftHeaderReadHttpServiceV4::new(
            publication,
            signer,
            None,
        ),
    );
    let mut server = service.serve_loopback(bind)?;
    let interrupted = if let Some(shutdown) = shutdown {
        shutdown
    } else {
        let flag = Arc::new(AtomicBool::new(false));
        let handler = flag.clone();
        ctrlc::set_handler(move || handler.store(true, Ordering::Release))
            .map_err(|e| e.to_string())?;
        flag
    };
    while !interrupted.load(Ordering::Acquire) && !server.is_finished() {
        thread::sleep(Duration::from_millis(100));
    }
    let result = server.stop();
    drop(execution);
    result
}
