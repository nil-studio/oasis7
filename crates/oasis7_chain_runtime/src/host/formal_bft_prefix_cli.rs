//! Explicit multiheight import/recovery. This command does not sign, serve an
//! HTTP read version, or select a latest target supplied by a provider.
use super::formal_bft_read_cli::{self as handoff_files, CasBusinessIndexReader};
use oasis7_consensus::tendermint_v2::BftProfileTargetV1;
use oasis7_formal_protocol::bft::{BFT_FORMAL_PREFIX_MAX_BYTES_V1, FormalBftCertifiedPrefixV1};
use oasis7_runtime::LocalCasStore;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[cfg(not(test))]
use super::execution_bridge::FormalBftPrefixExecutionV1;
#[cfg(test)]
use super::execution_bridge_real_tests::real_execution_bridge::FormalBftPrefixExecutionV1;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExactCut {
    height: u64,
    block_id: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    schema_version: u32,
    trust_config_path: PathBuf,
    target: BftProfileTargetV1,
    exact_target: ExactCut,
    handoff_material_path: PathBuf,
    cas_directory: PathBuf,
    ledger_directory: PathBuf,
}
#[derive(Serialize)]
struct Receipt<'a> {
    schema_version: &'static str,
    height: u64,
    block_id: &'a str,
    activation_digest: &'a str,
    durable_record_digest: &'a str,
}

pub(super) fn run<'a>(mut args: impl Iterator<Item = &'a str>) -> Result<(), String> {
    let action = args.next().ok_or_else(help)?;
    if action == "--help" || action == "-h" {
        println!("{}", help());
        return Ok(());
    }
    if !matches!(action, "install" | "verify") {
        return Err(help());
    }
    let mut config_path = None;
    let mut prefix_path = None;
    while let Some(option) = args.next() {
        let destination = match option {
            "--help" | "-h" => {
                println!("{}", help());
                return Ok(());
            }
            "--config" => &mut config_path,
            "--prefix" if action == "install" => &mut prefix_path,
            _ => return Err(format!("unsupported bft-prefix option {option}")),
        };
        if destination.is_some() {
            return Err(format!("duplicate bft-prefix option {option}"));
        }
        let value = args
            .next()
            .ok_or_else(|| format!("{option} requires an absolute path"))?;
        let path = PathBuf::from(value);
        if !path.is_absolute() {
            return Err(format!("{option} requires an absolute path"));
        }
        *destination = Some(path);
    }
    let config_path = config_path.ok_or("bft-prefix requires --config")?;
    super::controlled_live_config::require_private_owned(&config_path, false)?;
    let bytes = super::controlled_history_cli::read_bounded(&config_path, 2 * 1024 * 1024)?;
    let config: Config =
        serde_json::from_slice(&bytes).map_err(|e| format!("decode bft-prefix config: {e}"))?;
    config.target.validate()?;
    if config.schema_version != 1
        || config.exact_target.height == 0
        || config.exact_target.block_id.len() != 64
        || !config
            .exact_target
            .block_id
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err("unsupported bft-prefix config schema or exact cut".into());
    }
    super::controlled_live_config::require_private_owned(&config.handoff_material_path, false)?;
    let trusted = handoff_files::load_qualified_handoff(
        &config.trust_config_path,
        &config.target,
        &config.handoff_material_path,
    )?;
    let cas = handoff_files::require_existing_directory(&config.cas_directory, "prefix CAS")?;
    let records =
        handoff_files::require_existing_directory(&config.ledger_directory, "prefix ledger")?;
    super::controlled_live_config::require_private_owned(&cas, true)?;
    super::controlled_live_config::require_private_owned(&records, true)?;
    if handoff_files::paths_overlap(&cas, &records) {
        return Err("formal prefix CAS and ledger directories must be separate".into());
    }
    let prefix = if action == "install" {
        let path = prefix_path.ok_or("bft-prefix install requires --prefix")?;
        let bytes = read_prefix(&path)?;
        let prefix = FormalBftCertifiedPrefixV1::decode_canonical(&bytes)?;
        let proof = prefix.verify(&trusted.handoff)?;
        if proof.certificate().block().height != config.exact_target.height
            || proof.certificate().block_id() != config.exact_target.block_id
        {
            return Err("prefix differs from independently configured exact cut".into());
        }
        Some(prefix)
    } else {
        None
    };
    let store = LocalCasStore::new(cas);
    let source = Arc::new(CasBusinessIndexReader {
        store: store.clone(),
    });
    let mut execution = FormalBftPrefixExecutionV1::open_from_handoff(
        &records,
        store,
        trusted.anchor,
        config.target,
        trusted.handoff_material,
        source,
        action == "install",
    )?;
    let cut = match prefix {
        Some(prefix) => execution.install_prefix(
            &prefix,
            config.exact_target.height,
            &config.exact_target.block_id,
        )?,
        None => {
            execution.verified_cut(config.exact_target.height, &config.exact_target.block_id)?
        }
    };
    let receipt = Receipt {
        schema_version: "oasis7.formal_bft_prefix_install_receipt.v1",
        height: cut.proof().certificate().block().height,
        block_id: cut.proof().certificate().block_id(),
        activation_digest: cut.proof().handoff().activation_digest(),
        durable_record_digest: cut.durable_record_digest(),
    };
    println!(
        "{}",
        serde_json::to_string(&receipt).map_err(|e| e.to_string())?
    );
    Ok(())
}

fn read_prefix(path: &Path) -> Result<Vec<u8>, String> {
    super::controlled_live_config::require_private_owned(path, false)?;
    super::controlled_history_cli::read_bounded(path, BFT_FORMAL_PREFIX_MAX_BYTES_V1 as u64)
}
fn help() -> String {
    "Usage: oasis7_chain_runtime bft-prefix install --config <prefix.json> --prefix <complete-prefix.cbor>\n       oasis7_chain_runtime bft-prefix verify --config <prefix.json>\nInstall/recover only the externally pinned complete prefix. Verify reopens existing state and may finish its exact original durable intent. Neither command signs, serves latest reads, or grants validator readiness.".into()
}
