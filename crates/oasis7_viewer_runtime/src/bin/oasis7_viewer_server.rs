use std::env;
use std::process;

use oasis7_observability::init_tracing;
use oasis7_viewer_runtime::{ViewerServer, ViewerServerConfig};
use tracing::{error, info};

fn main() {
    init_tracing("oasis7_viewer_server");
    let mut args = env::args().skip(1);
    let world_dir = args.next().unwrap_or_else(|| ".".to_string());
    let bind_addr = args.next().unwrap_or_else(|| "127.0.0.1:5010".to_string());

    let config = ViewerServerConfig::from_dir(world_dir).with_bind_addr(bind_addr);
    info!(
        bind_addr = %config.bind_addr,
        snapshot_path = %config.snapshot_path.display(),
        journal_path = %config.journal_path.display(),
        world_id = %config.world_id,
        "starting viewer playback server"
    );

    let server = match ViewerServer::load(config) {
        Ok(server) => server,
        Err(err) => {
            error!(error = ?err, "failed to load viewer server data");
            process::exit(1);
        }
    };

    if let Err(err) = server.run() {
        error!(error = ?err, "viewer server failed");
        process::exit(1);
    }
}
