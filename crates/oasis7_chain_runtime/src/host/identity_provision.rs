use std::path::PathBuf;

use oasis7_node::derive_libp2p_identity_keypair;
use serde::Serialize;

use super::node_keypair_config;
use super::{derive_node_consensus_signer_keypair, derive_node_libp2p_identity_keypair_config};

#[derive(Serialize)]
struct IdentityProvisionReceipt {
    schema_version: &'static str,
    node_id: String,
    root_public_key: String,
    finality_public_key: String,
    libp2p_peer_id: String,
    node_keypair_config_path: String,
    node_keypair_config_exists: bool,
    node_keypair_config_mode: &'static str,
}

pub(super) fn run<'a>(args: impl Iterator<Item = &'a str>) -> Result<(), String> {
    let (config_dir, node_id) = parse_args(args)?;
    let (root_keypair, key_path) =
        node_keypair_config::ensure_node_keypair_in_secure_config_dir(config_dir.as_path())?;
    let finality_keypair = derive_node_consensus_signer_keypair(node_id.as_str(), &root_keypair)?;
    let libp2p_keypair =
        derive_node_libp2p_identity_keypair_config(node_id.as_str(), &root_keypair)?;
    let libp2p_peer_id = derive_libp2p_identity_keypair(libp2p_keypair.private_key_hex.as_str())
        .map_err(|err| format!("derive libp2p identity peer id failed: {err:?}"))?
        .public()
        .to_peer_id()
        .to_string();
    let receipt = IdentityProvisionReceipt {
        schema_version: "oasis7.identity_provision.v1",
        node_id,
        root_public_key: root_keypair.public_key_hex,
        finality_public_key: finality_keypair.public_key_hex,
        libp2p_peer_id,
        node_keypair_config_path: key_path.display().to_string(),
        node_keypair_config_exists: true,
        node_keypair_config_mode: "0600",
    };
    println!(
        "{}",
        serde_json::to_string(&receipt)
            .map_err(|err| format!("serialize identity receipt failed: {err}"))?
    );
    Ok(())
}

fn parse_args<'a>(args: impl Iterator<Item = &'a str>) -> Result<(PathBuf, String), String> {
    let mut config_dir = None;
    let mut node_id = None;
    let mut args = args.peekable();
    while let Some(arg) = args.next() {
        match arg {
            "--config-dir" => config_dir = Some(required_value(&mut args, "--config-dir")?),
            "--node-id" => node_id = Some(required_value(&mut args, "--node-id")?),
            "-h" | "--help" => return Err(provision_help()),
            _ => return Err(format!("unknown provision-identity option: {arg}")),
        }
    }
    let config_dir =
        config_dir.ok_or_else(|| "provision-identity requires --config-dir".to_string())?;
    let node_id = node_id.ok_or_else(|| "provision-identity requires --node-id".to_string())?;
    if node_id.trim().is_empty() {
        return Err("--node-id requires a non-empty value".to_string());
    }
    Ok((PathBuf::from(config_dir), node_id))
}

fn required_value<'a>(
    args: &mut std::iter::Peekable<impl Iterator<Item = &'a str>>,
    option: &str,
) -> Result<String, String> {
    args.next()
        .map(str::to_owned)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("{option} requires a non-empty value"))
}

fn provision_help() -> String {
    "Usage: oasis7_chain_runtime provision-identity --config-dir <absolute path> --node-id <exact non-empty id>".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_requires_both_stable_identity_inputs() {
        assert!(parse_args(["--config-dir", "/tmp/identity"].into_iter()).is_err());
        assert!(parse_args(["--node-id", "validator-a"].into_iter()).is_err());
        assert!(
            parse_args(["--config-dir", "/tmp/identity", "--node-id", "  "].into_iter()).is_err()
        );
    }
}
