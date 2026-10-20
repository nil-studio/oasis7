#![cfg(all(not(target_arch = "wasm32"), feature = "node-libp2p"))]

use oasis7_chain_runtime::{RuntimeConfig, run_cli};
use std::path::Path;

#[test]
fn parsed_configuration_exposes_resolved_paths() {
    let config = RuntimeConfig::parse(
        [
            "--node-id",
            "library-node",
            "--runtime-root",
            "custom/runtime",
        ]
        .into_iter(),
    )
    .unwrap();
    let paths = config.paths();
    assert_eq!(paths.runtime_root, Path::new("custom/runtime"));
    assert_eq!(paths.storage_root, Path::new("custom/runtime/store"));
    assert_eq!(
        paths.replication_root,
        Path::new("output/node-distfs/library-node")
    );
}

#[test]
fn cli_errors_return_without_terminating_the_library_caller() {
    assert!(run_cli(vec!["--unknown-library-option".into()]).is_err());
    assert!(RuntimeConfig::parse(["--unknown-library-option"].into_iter()).is_err());
    assert!(run_cli(vec!["--help".into()]).is_ok());
}
