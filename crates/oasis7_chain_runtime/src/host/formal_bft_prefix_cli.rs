//! Explicit multiheight import/recovery and fixed-cut V3 public serving.
//! No provider-selected latest target or validator signing is installed.
use super::formal_bft_read_cli::{self as handoff_files, CasBusinessIndexReader};
use oasis7_consensus::tendermint_v2::BftProfileTargetV1;
use oasis7_formal_protocol::bft::{BFT_FORMAL_PREFIX_MAX_BYTES_V1, FormalBftCertifiedPrefixV1};
use oasis7_runtime::LocalCasStore;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::{thread, time::Duration};

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
    #[serde(default)]
    bind: Option<String>,
    #[serde(default)]
    service_signing_key_path: Option<PathBuf>,
}
#[derive(Serialize)]
struct Receipt<'a> {
    schema_version: &'static str,
    height: u64,
    block_id: &'a str,
    activation_digest: &'a str,
    durable_record_digest: &'a str,
}

pub(super) fn run<'a>(args: impl Iterator<Item = &'a str>) -> Result<(), String> {
    run_with_shutdown(args, None)
}
pub(super) fn run_with_shutdown<'a>(
    mut args: impl Iterator<Item = &'a str>,
    shutdown: Option<Arc<AtomicBool>>,
) -> Result<(), String> {
    let action = args.next().ok_or_else(help)?;
    if action == "--help" || action == "-h" {
        println!("{}", help());
        return Ok(());
    }
    if !matches!(action, "install" | "verify" | "serve") {
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
    let mut execution = Some(FormalBftPrefixExecutionV1::open_from_handoff(
        &records,
        store,
        trusted.anchor.clone(),
        config.target.clone(),
        trusted.handoff_material,
        source,
        action == "install",
    )?);
    let cut = match prefix {
        Some(prefix) => execution
            .take()
            .ok_or("prefix execution owner missing")?
            .install_prefix(
                &prefix,
                config.exact_target.height,
                &config.exact_target.block_id,
            )?,
        None => execution
            .as_mut()
            .ok_or("prefix execution owner missing")?
            .verified_cut(config.exact_target.height, &config.exact_target.block_id)?,
    };
    if action == "serve" {
        let bind: SocketAddr = config
            .bind
            .as_deref()
            .ok_or("bft-prefix serve requires bind")?
            .parse()
            .map_err(|_| "bft-prefix bind must be a literal socket address")?;
        if !bind.ip().is_loopback() || bind.port() == 0 {
            return Err("bft-prefix bind must be a nonzero loopback address".into());
        }
        let key_path = config
            .service_signing_key_path
            .as_deref()
            .ok_or("bft-prefix serve requires service_signing_key_path")?;
        if !key_path.is_absolute() {
            return Err("bft-prefix service key path must be absolute".into());
        }
        let signer = super::controlled_live_config::read_existing_signing_key(key_path)?;
        use oasis7::world_service::formal_bft_prefix_read_service::{
            FormalBftReadPublicationInputV3, FormalBftReadPublicationV3,
        };
        let publication = FormalBftReadPublicationV3::new(FormalBftReadPublicationInputV3 {
            handoff_anchor: trusted.anchor,
            target_config: config.target,
            exact_target: oasis7_client_api::world_service::FormalBftReadTargetV3 {
                height: config.exact_target.height,
                block_id: config.exact_target.block_id,
            },
            certified_prefix: cut.certified_prefix(),
            committed_world: cut.world(),
            result_tree: cut.result_tree(),
        })
        .map_err(|e| format!("build durable BFT prefix publication: {e}"))?;
        let service = Arc::new(
            super::formal_bft_prefix_http::FormalBftReadHttpServiceV3::new(
                publication,
                signer,
                None,
            ),
        );
        let mut server = service.serve_loopback(bind)?;
        let interrupted = if let Some(shutdown) = shutdown {
            shutdown
        } else {
            let interrupted = Arc::new(AtomicBool::new(false));
            let flag = interrupted.clone();
            ctrlc::set_handler(move || flag.store(true, Ordering::Release))
                .map_err(|e| e.to_string())?;
            interrupted
        };
        while !interrupted.load(Ordering::Acquire) && !server.is_finished() {
            thread::sleep(Duration::from_millis(100));
        }
        // Keep the execution owner/OS locks alive through server shutdown.
        let result = server.stop();
        drop(execution);
        return result;
    }
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
    "Usage: oasis7_chain_runtime bft-prefix install --config <prefix.json> --prefix <complete-prefix.cbor>\n       oasis7_chain_runtime bft-prefix verify --config <prefix.json>\n       oasis7_chain_runtime bft-prefix serve --config <prefix.json>\nInstall/recover only the externally pinned complete prefix. Verify reopens existing state and may finish its exact original durable intent. Serve reopens the exact existing cut on the independent V3 loopback route; private reads require a separate current authority source and remain blocked here. These commands never sign validator votes, select latest state, or grant readiness.".into()
}
