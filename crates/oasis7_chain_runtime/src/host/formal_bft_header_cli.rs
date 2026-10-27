//! Explicit offline previews and private durable imports. No canonical
//! publication, service listener or validator signer is created.
#[path = "formal_bft_header_ledger_cli.rs"]
mod ledger;

#[cfg(not(test))]
use super::execution_bridge::{
    FormalBftHeaderExecutionV3, PRIVATE_HEADER_PREFIX_MAX_BYTES_V3, PrivateHeaderPrefixV3,
};
#[cfg(test)]
use super::execution_bridge_real_tests::real_execution_bridge::{
    FormalBftHeaderExecutionV3, PRIVATE_HEADER_PREFIX_MAX_BYTES_V3, PrivateHeaderPrefixV3,
};
use super::formal_bft_read_cli::{
    CasBusinessIndexReader, load_qualified_handoff_anchor, read_qualified_handoff_material,
    require_existing_directory,
};
use oasis7_formal_protocol::bft::BftProfileTargetV2;
use oasis7_proto::distributed_bft::{BFT_MAX_ACTIONS_CBOR_BYTES_V1, v3::BftBlockHeaderV3};
use oasis7_runtime::LocalCasStore;
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    schema_version: u32,
    trust_config_path: PathBuf,
    target: BftProfileTargetV2,
    handoff_material_path: PathBuf,
    cas_directory: PathBuf,
}
#[derive(Serialize)]
pub(super) struct Preview {
    schema_version: &'static str,
    header: BftBlockHeaderV3,
    execution_commitment: oasis7_formal_protocol::bft::FormalBftExecutionCommitmentV1,
}

pub(super) fn run<'a>(mut args: impl Iterator<Item = &'a str>) -> Result<(), String> {
    let action = args.next().ok_or_else(help)?;
    if matches!(action, "--help" | "-h") {
        println!("{}", help());
        return Ok(());
    }
    if matches!(action, "install" | "verify") {
        return ledger::run(action, args);
    }
    if action != "preview" {
        return Err(help());
    }
    let mut config_path = None;
    let mut actions_path = None;
    let mut history_path = None;
    while let Some(option) = args.next() {
        if matches!(option, "--help" | "-h") {
            println!("{}", help());
            return Ok(());
        }
        let slot = match option {
            "--config" => &mut config_path,
            "--actions" => &mut actions_path,
            "--history" => &mut history_path,
            _ => return Err("unsupported formal header preview option".into()),
        };
        if slot.is_some() {
            return Err("duplicate formal header preview option".into());
        }
        let path = PathBuf::from(
            args.next()
                .ok_or("preview option requires an absolute path")?,
        );
        if !path.is_absolute() {
            return Err("formal header preview paths must be absolute".into());
        }
        *slot = Some(path);
    }
    let config_path = config_path.ok_or("bft-header preview requires --config")?;
    let actions_path = actions_path.ok_or("bft-header preview requires --actions")?;
    let output = preview_from_paths(&config_path, &actions_path, history_path.as_deref())?;
    println!(
        "{}",
        serde_json::to_string(&output)
            .map_err(|_| "encode public formal header preview".to_string())?
    );
    Ok(())
}

pub(super) fn preview_from_paths(
    config_path: &Path,
    actions_path: &Path,
    history_path: Option<&Path>,
) -> Result<Preview, String> {
    let config_bytes = read_private(config_path, 2 * 1024 * 1024)?;
    let config: Config = serde_json::from_slice(&config_bytes)
        .map_err(|_| "invalid formal header preview config".to_string())?;
    if config.schema_version != 1 {
        return Err("unsupported formal header preview config schema".into());
    }
    config.target.validate()?;
    let cas = require_existing_directory(&config.cas_directory, "private preview CAS")?;
    super::controlled_live_config::require_private_owned(&cas, true)?;
    let anchor = load_qualified_handoff_anchor(&config.trust_config_path)
        .map_err(|_| "formal header preview trust rejected".to_string())?;
    oasis7_formal_protocol::bft::verify_formal_bft_handoff_v2(&anchor, config.target.clone())
        .map_err(|_| "formal header preview target is not admitted by qualified H".to_string())?;
    super::controlled_live_config::require_private_owned(&config.handoff_material_path, false)?;
    let material = read_qualified_handoff_material(&config.handoff_material_path)
        .map_err(|_| "formal header preview H material rejected".to_string())?;
    let private_actions = read_private(actions_path, BFT_MAX_ACTIONS_CBOR_BYTES_V1 as u64)?;
    let history = history_path
        .map(|path| {
            read_private(path, PRIVATE_HEADER_PREFIX_MAX_BYTES_V3 as u64)
                .and_then(|bytes| PrivateHeaderPrefixV3::decode_private_canonical(&bytes))
        })
        .transpose()?;
    let source = Arc::new(CasBusinessIndexReader {
        store: LocalCasStore::new(cas),
    });
    let mut execution = FormalBftHeaderExecutionV3::from_handoff(
        anchor,
        config.target,
        &material,
        source,
        Vec::new(),
        false,
    )
    .map_err(|_| "formal header preview could not restore the qualified H".to_string())?;
    if let Some(history) = history {
        for entry in history.entries() {
            execution
                .replay_next(&entry.certificate, &entry.actions_cbor)
                .map_err(|_| {
                    "formal header preview history failed actual sequential verification"
                        .to_string()
                })?;
        }
    }
    let candidate = execution
        .build_candidate(&private_actions)
        .map_err(|_| "formal header preview private input or execution rejected".to_string())?;
    Ok(Preview {
        schema_version: "oasis7.formal_bft_header_preview.v1",
        header: candidate.header,
        execution_commitment: candidate.commitment,
    })
}
fn read_private(path: &Path, maximum: u64) -> Result<Vec<u8>, String> {
    if !path.is_absolute() {
        return Err("formal header input path must be absolute".into());
    }
    super::controlled_live_config::require_private_owned(path, false)?;
    super::controlled_history_cli::read_bounded(path, maximum)
}
fn help() -> String {
    "Usage: oasis7_chain_runtime bft-header preview --config ABS --actions ABS [--history ABS]\nAdditional commands: bft-header install --config ABS --history ABS; bft-header verify --config ABS.\nOffline actual World execution preview from qualified H and an optional fully replayed private history. Preview output is public header/commitment only, with no durable receipt. Install/verify return a private local ledger report with a public prefix; neither grants canonical publication, validator signature or read authority.".into()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preview_help_and_relative_paths_are_explicit() {
        run(["--help"].into_iter()).unwrap();
        assert!(
            run([
                "preview",
                "--config",
                "relative",
                "--actions",
                "/private/input"
            ]
            .into_iter())
            .is_err()
        );
        assert!(run(["serve"].into_iter()).is_err());
        assert!(run(["preview", "--config", "/one", "--config", "/two"].into_iter()).is_err());
        assert!(help().contains("no durable receipt"));
    }
    #[test]
    fn preview_history_uses_the_shared_private_codec() {
        let bytes = oasis7_wasm_abi::encode_canonical_cbor(
            &serde_json::json!({"schema_version":1,"entries":[]}),
        )
        .unwrap();
        assert!(
            PrivateHeaderPrefixV3::decode_private_canonical(&bytes)
                .unwrap()
                .entries()
                .is_empty()
        );
        assert!(PrivateHeaderPrefixV3::decode_private_canonical(&[0x80]).is_err());
    }
}
