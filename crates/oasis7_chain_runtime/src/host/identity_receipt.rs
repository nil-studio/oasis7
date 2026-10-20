use std::fs;
use std::path::{Path, PathBuf};

use oasis7_node::derive_libp2p_identity_keypair;
use serde::Serialize;
use sha2::{Digest, Sha256};

use super::derive_node_libp2p_identity_keypair_config;
use super::node_keypair_config;

pub(super) fn dispatch<'a>(mut args: impl Iterator<Item = &'a str>) -> Option<Result<(), String>> {
    let command = args.next()?;
    if command == "initialize-preview-world" {
        return Some(super::preview_world_init::run(args));
    }
    use super::cli::cli_contract::{CommandKind, offline_command};
    match offline_command(command)? {
        CommandKind::Inspect => Some(super::inspect_cli::run(args)),
        CommandKind::Dev => Some(super::dev_cli::run(args)),
        CommandKind::ProvisionIdentity => Some(super::identity_provision::run(args)),
        CommandKind::IdentityReceipt => Some(run(args)),
        CommandKind::RebuildProof => {
            Some(super::inspect_cli::run_compatibility("rebuild-proof", args))
        }
        CommandKind::ControlledHistory => {
            Some(super::inspect_cli::run_compatibility("history", args))
        }
        CommandKind::ControlledBootstrap => {
            Some(super::inspect_cli::run_compatibility("bootstrap", args))
        }
        CommandKind::ValidatorMembership => Some(super::membership_cli::run(args)),
        CommandKind::NodeLifecycle => Some(super::node_lifecycle::run(args)),
        CommandKind::Operation => Some(super::node_operations::run(args)),
        CommandKind::LegacyMigration => Some(super::node_lifecycle::legacy_migration::run(args)),
        CommandKind::LegacyRuntime => {
            Some(super::node_lifecycle::legacy_migration::run_legacy(args))
        }
        CommandKind::DurableSetControl => Some(super::formal_durable_set_cli::run(args)),
    }
}

#[derive(Debug, Serialize)]
pub(super) struct IdentityReceipt {
    pub(super) schema_version: &'static str,
    pub(super) node_id: String,
    pub(super) peer_id: String,
    pub(super) key_path: String,
    pub(super) key_sha256: String,
    pub(super) key_size_bytes: u64,
    pub(super) key_mode: u32,
    pub(super) key_uid: u32,
    pub(super) key_gid: u32,
}

pub(super) fn run<'a>(args: impl Iterator<Item = &'a str>) -> Result<(), String> {
    let (config_dir, node_id) = parse_args(args)?;
    let receipt = build_identity_receipt(config_dir.as_path(), node_id.as_str())?;
    println!(
        "{}",
        serde_json::to_string(&receipt)
            .map_err(|err| format!("serialize identity receipt failed: {err}"))?
    );
    Ok(())
}

pub(super) fn build_identity_receipt(
    config_dir: &Path,
    node_id: &str,
) -> Result<IdentityReceipt, String> {
    let (root_keypair, key_path) =
        node_keypair_config::read_existing_node_keypair_in_secure_config_dir(config_dir)?;
    let libp2p_keypair = derive_node_libp2p_identity_keypair_config(node_id, &root_keypair)?;
    let peer_id = derive_libp2p_identity_keypair(libp2p_keypair.private_key_hex.as_str())
        .map_err(|err| format!("derive libp2p identity peer id failed: {err:?}"))?
        .public()
        .to_peer_id()
        .to_string();
    let metadata = fs::metadata(&key_path)
        .map_err(|err| format!("read key metadata {} failed: {err}", key_path.display()))?;
    let bytes = fs::read(&key_path)
        .map_err(|err| format!("read key bytes {} failed: {err}", key_path.display()))?;
    let digest = Sha256::digest(bytes.as_slice());
    #[cfg(unix)]
    let (key_mode, key_uid, key_gid) = {
        use std::os::unix::fs::MetadataExt;
        (metadata.mode() & 0o777, metadata.uid(), metadata.gid())
    };
    #[cfg(not(unix))]
    let (key_mode, key_uid, key_gid) = (0, 0, 0);
    Ok(IdentityReceipt {
        schema_version: "oasis7.identity_receipt.v1",
        node_id: node_id.trim().to_string(),
        peer_id,
        key_path: key_path.display().to_string(),
        key_sha256: hex::encode(digest),
        key_size_bytes: metadata.len(),
        key_mode,
        key_uid,
        key_gid,
    })
}

fn parse_args<'a>(args: impl Iterator<Item = &'a str>) -> Result<(PathBuf, String), String> {
    let mut config_dir = None;
    let mut node_id = None;
    let mut args = args.peekable();
    while let Some(arg) = args.next() {
        match arg {
            "--config-dir" => config_dir = Some(required_value(&mut args, "--config-dir")?),
            "--node-id" => node_id = Some(required_value(&mut args, "--node-id")?),
            "-h" | "--help" => return Err(help()),
            _ => return Err(format!("unknown identity-receipt option: {arg}")),
        }
    }
    let config_dir =
        config_dir.ok_or_else(|| "identity-receipt requires --config-dir".to_string())?;
    let node_id = node_id.ok_or_else(|| "identity-receipt requires --node-id".to_string())?;
    if node_id.trim().is_empty() {
        return Err("identity-receipt requires a non-empty --node-id".to_string());
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

fn help() -> String {
    "Usage: oasis7_chain_runtime identity-receipt --config-dir <absolute path> --node-id <exact non-empty id>".to_string()
}
