//! Operator-only installer and fixed-cut loopback reader for the first formal
//! BFT successor. Neither command creates trust from its QC or a service reply.
use std::fs;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use oasis7::world_service::client::FormalReadTrustConfigV1;
use oasis7_client_api::world_service::{
    FormalBftReadTargetV2, MAX_FORMAL_BFT_CERTIFICATE_BYTES_V2,
};
use oasis7_consensus::tendermint_v2::{
    BftProfileTargetV1, VerifiedBftHandoffTargetV1, verify_formal_bft_handoff_v1,
};
use oasis7_distfs::controlled_authority::{
    formal::{VerifiedFormalReadAnchorV1, business_index::BusinessIndexNodeReader},
    replicated_protocol::ClosedRecord,
};
use oasis7_node::BftCommitCertificateV2;
use oasis7_runtime::LocalCasStore;
use oasis7_wasm_abi::encode_canonical_cbor;
use serde::{Deserialize, Serialize, de::DeserializeOwned};

#[cfg(not(test))]
use super::execution_bridge::FormalBftReadExecutionV1;
#[cfg(test)]
use super::execution_bridge_real_tests::real_execution_bridge::FormalBftReadExecutionV1;

const CONFIG_MAX_BYTES: u64 = 2 * 1024 * 1024;
const TRUST_MAX_BYTES: u64 = 64 * 1024;
const HANDOFF_MATERIAL_MAX_BYTES: u64 = 64 * 1024 * 1024;
const BUSINESS_INDEX_NODE_MAX_BYTES: usize = 4 * 1024 * 1024 + 128;
const RECEIPT_SCHEMA: &str = "oasis7.formal_bft_read_install_receipt.v1";

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct BftReadConfigV1 {
    schema_version: u32,
    trust_config_path: PathBuf,
    target: BftProfileTargetV1,
    exact_target: FormalBftReadTargetV2,
    handoff_material_path: PathBuf,
    cas_directory: PathBuf,
    ledger_directory: PathBuf,
    bind: String,
    #[serde(default)]
    service_signing_key_path: Option<PathBuf>,
}

impl BftReadConfigV1 {
    fn validate(&self) -> Result<(), String> {
        if self.schema_version != 1 {
            return Err("unsupported bft-read config schema_version".into());
        }
        self.target.validate()?;
        self.exact_target
            .validate()
            .map_err(|error| error.to_string())?;
        for (label, path) in [
            ("trust_config_path", &self.trust_config_path),
            ("handoff_material_path", &self.handoff_material_path),
        ] {
            if !path.is_absolute() {
                return Err(format!("{label} must be an absolute path"));
            }
        }
        for (label, path) in [
            ("cas_directory", &self.cas_directory),
            ("ledger_directory", &self.ledger_directory),
        ] {
            if !path.is_absolute() {
                return Err(format!("{label} must be an absolute path"));
            }
        }
        if self
            .service_signing_key_path
            .as_ref()
            .is_some_and(|path| !path.is_absolute())
        {
            return Err("service_signing_key_path must be an absolute path".into());
        }
        let bind: SocketAddr = self
            .bind
            .parse()
            .map_err(|_| "bft-read bind must be a literal socket address".to_string())?;
        if !bind.ip().is_loopback() || bind.port() == 0 {
            return Err("bft-read bind must be a nonzero loopback address".into());
        }
        Ok(())
    }

    fn bind_address(&self) -> Result<SocketAddr, String> {
        self.bind
            .parse()
            .map_err(|_| "bft-read bind must be a literal socket address".into())
    }
}

struct LoadedInputs {
    config: BftReadConfigV1,
    anchor: VerifiedFormalReadAnchorV1,
    handoff: VerifiedBftHandoffTargetV1,
    handoff_material: ClosedRecord,
    store: LocalCasStore,
}

#[derive(Serialize)]
struct InstallReceiptV1<'a> {
    schema_version: &'static str,
    height: u64,
    block_id: &'a str,
    target_config_digest: &'a str,
    activation_digest: &'a str,
    durable_record_digest: &'a str,
}

enum Command {
    Serve {
        config: PathBuf,
    },
    Install {
        config: PathBuf,
        certificate: PathBuf,
    },
    Help,
}

pub(super) fn run<'a>(args: impl Iterator<Item = &'a str>) -> Result<(), String> {
    run_inner(args, None)
}

#[cfg(test)]
pub(crate) fn run_until_shutdown<'a>(
    args: impl Iterator<Item = &'a str>,
    shutdown: Arc<AtomicBool>,
) -> Result<(), String> {
    run_inner(args, Some(shutdown))
}

fn run_inner<'a>(
    args: impl Iterator<Item = &'a str>,
    shutdown: Option<Arc<AtomicBool>>,
) -> Result<(), String> {
    match parse_command(args)? {
        Command::Help => {
            println!("{}", help());
            Ok(())
        }
        Command::Serve { config } => serve(&config, shutdown),
        Command::Install {
            config,
            certificate,
        } => install(&config, &certificate),
    }
}

fn parse_command<'a>(mut args: impl Iterator<Item = &'a str>) -> Result<Command, String> {
    let Some(action) = args.next() else {
        return Err(help());
    };
    if action == "-h" || action == "--help" {
        return Ok(Command::Help);
    }
    let mut config = None;
    let mut certificate = None;
    while let Some(option) = args.next() {
        match option {
            "--config" => set_path(&mut config, &mut args, option)?,
            "--certificate" => set_path(&mut certificate, &mut args, option)?,
            "-h" | "--help" => return Ok(Command::Help),
            _ => return Err(format!("unknown bft-read option: {option}")),
        }
    }
    match action {
        "serve" if certificate.is_none() => Ok(Command::Serve {
            config: config.ok_or("bft-read serve requires --config")?,
        }),
        "install" => Ok(Command::Install {
            config: config.ok_or("bft-read install requires --config")?,
            certificate: certificate.ok_or("bft-read install requires --certificate")?,
        }),
        "serve" => Err("bft-read serve does not accept --certificate".into()),
        _ => Err("bft-read requires serve or install".into()),
    }
}

fn set_path<'a>(
    slot: &mut Option<PathBuf>,
    args: &mut impl Iterator<Item = &'a str>,
    option: &str,
) -> Result<(), String> {
    if slot.is_some() {
        return Err(format!("duplicate bft-read option {option}"));
    }
    let value = args
        .next()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("{option} requires a non-empty value"))?;
    *slot = Some(PathBuf::from(value));
    Ok(())
}

fn load_inputs(config_path: &Path) -> Result<LoadedInputs, String> {
    if !config_path.is_absolute() {
        return Err("bft-read config path must be absolute".into());
    }
    super::controlled_live_config::require_private_owned(config_path, false)?;
    let config_bytes = super::controlled_history_cli::read_bounded(config_path, CONFIG_MAX_BYTES)?;
    let mut config: BftReadConfigV1 = serde_json::from_slice(&config_bytes)
        .map_err(|error| format!("decode bft-read config: {error}"))?;
    config.validate()?;

    let QualifiedHandoffFilesV1 {
        anchor,
        handoff,
        handoff_material,
    } = load_qualified_handoff(
        &config.trust_config_path,
        &config.target,
        &config.handoff_material_path,
    )?;
    if config.exact_target.height != handoff.activation_height()? {
        return Err("exact target height must be the H handoff activation height".into());
    }

    let cas_directory = require_existing_directory(&config.cas_directory, "CAS directory")?;
    let ledger_directory =
        require_existing_directory(&config.ledger_directory, "ledger directory")?;
    if paths_overlap(&cas_directory, &ledger_directory) {
        return Err("CAS and formal BFT ledger directories must be separate".into());
    }
    config.cas_directory = cas_directory.clone();
    config.ledger_directory = ledger_directory;
    let store = LocalCasStore::new(cas_directory.clone());
    Ok(LoadedInputs {
        config,
        anchor,
        handoff,
        handoff_material,
        store,
    })
}

pub(super) struct QualifiedHandoffFilesV1 {
    pub(super) anchor: VerifiedFormalReadAnchorV1,
    pub(super) handoff: VerifiedBftHandoffTargetV1,
    pub(super) handoff_material: ClosedRecord,
}

pub(super) fn load_qualified_handoff(
    trust_config_path: &Path,
    target: &BftProfileTargetV1,
    handoff_material_path: &Path,
) -> Result<QualifiedHandoffFilesV1, String> {
    if !trust_config_path.is_absolute() || !handoff_material_path.is_absolute() {
        return Err("formal handoff input paths must be absolute".into());
    }
    let anchor = load_qualified_handoff_anchor(trust_config_path)?;
    let handoff = verify_formal_bft_handoff_v1(&anchor, target.clone())?;
    let handoff_material = read_qualified_handoff_material(handoff_material_path)?;
    Ok(QualifiedHandoffFilesV1 {
        anchor,
        handoff,
        handoff_material,
    })
}

/// Authenticate controlled H without choosing the successor wire version.
pub(super) fn load_qualified_handoff_anchor(
    trust_config_path: &Path,
) -> Result<VerifiedFormalReadAnchorV1, String> {
    load_handoff_trust_config(trust_config_path)?.verified_anchor()
}

pub(super) fn load_handoff_trust_config(
    trust_config_path: &Path,
) -> Result<FormalReadTrustConfigV1, String> {
    if !trust_config_path.is_absolute() {
        return Err("formal handoff input paths must be absolute".into());
    }
    super::controlled_live_config::require_private_owned(trust_config_path, false)?;
    let trust_bytes =
        super::controlled_history_cli::read_bounded(trust_config_path, TRUST_MAX_BYTES)?;
    let trust: FormalReadTrustConfigV1 = serde_json::from_slice(&trust_bytes)
        .map_err(|error| format!("decode external formal-read trust config: {error}"))?;
    Ok(trust)
}

pub(super) fn read_qualified_handoff_material(
    handoff_material_path: &Path,
) -> Result<ClosedRecord, String> {
    if !handoff_material_path.is_absolute() {
        return Err("formal handoff input paths must be absolute".into());
    }
    read_canonical_cbor(
        handoff_material_path,
        HANDOFF_MATERIAL_MAX_BYTES,
        "qualified H closed material",
    )
}

fn install(config_path: &Path, certificate_path: &Path) -> Result<(), String> {
    let input = load_inputs(config_path)?;
    if !certificate_path.is_absolute() {
        return Err("BFT certificate path must be absolute".into());
    }
    let certificate: BftCommitCertificateV2 = read_canonical_cbor(
        certificate_path,
        MAX_FORMAL_BFT_CERTIFICATE_BYTES_V2 as u64,
        "BFT commit certificate",
    )?;
    certificate.validate_wire_bounds()?;
    let block = &certificate.proposal.block;
    let block_id = block.block_id()?;
    if block.height != input.config.exact_target.height
        || block_id != input.config.exact_target.block_id
    {
        return Err("certificate differs from the independently configured exact H+1 pin".into());
    }

    let mut execution = open_execution(&input, true)?;
    let cut = execution.commit_certificate(certificate)?;
    let committed_block = &cut.certificate().proposal.block;
    let committed_block_id = committed_block.block_id()?;
    if committed_block.height != input.config.exact_target.height
        || committed_block_id != input.config.exact_target.block_id
    {
        return Err("durable formal BFT cut differs from configured exact H+1 pin".into());
    }
    let receipt = InstallReceiptV1 {
        schema_version: RECEIPT_SCHEMA,
        height: committed_block.height,
        block_id: committed_block_id.as_str(),
        target_config_digest: input.handoff.target_digest(),
        activation_digest: input.handoff.activation_digest(),
        durable_record_digest: cut.durable_record_digest(),
    };
    println!(
        "{}",
        serde_json::to_string(&receipt)
            .map_err(|error| format!("encode bounded install receipt: {error}"))?
    );
    Ok(())
}

fn serve(config_path: &Path, shutdown: Option<Arc<AtomicBool>>) -> Result<(), String> {
    let input = load_inputs(config_path)?;
    let bind_address = input.config.bind_address()?;
    let key_path = input
        .config
        .service_signing_key_path
        .as_deref()
        .ok_or("bft-read serve requires service_signing_key_path")?;
    let signer = super::controlled_live_config::read_existing_signing_key(key_path)?;

    let execution = open_execution(&input, false)?;
    let cut = execution.verified_cut(input.config.exact_target.height)?;
    let block = &cut.certificate().proposal.block;
    if block.block_id()? != input.config.exact_target.block_id {
        return Err("existing durable formal BFT cut differs from configured exact H+1 pin".into());
    }
    let publication =
        oasis7::world_service::formal_bft_read_service::FormalBftReadPublicationV2::new(
            oasis7::world_service::formal_bft_read_service::FormalBftReadPublicationInputV2 {
                handoff_anchor: input.anchor,
                target_config: input.config.target,
                exact_target: input.config.exact_target,
                certificate: cut.certificate(),
                commitment: cut.commitment(),
                committed_world: cut.world(),
                new_results: cut.new_results(),
                h_result_tree: cut.h_result_tree(),
                h_result_index_reader: cut.h_index_reader(),
            },
        )
        .map_err(|error| format!("build formal BFT fixed-cut publication: {error}"))?;
    let service = Arc::new(
        super::formal_bft_read_http::FormalBftReadHttpServiceV2::new(publication, signer, None),
    );
    let mut server = service.serve_loopback(bind_address)?;

    let interrupted = if let Some(shutdown) = shutdown {
        shutdown
    } else {
        let interrupted = Arc::new(AtomicBool::new(false));
        let interrupt_flag = Arc::clone(&interrupted);
        ctrlc::set_handler(move || interrupt_flag.store(true, Ordering::Release))
            .map_err(|error| format!("install bft-read shutdown handler: {error}"))?;
        interrupted
    };
    while !interrupted.load(Ordering::Acquire) && !server.is_finished() {
        thread::sleep(Duration::from_millis(100));
    }
    server.stop()
}

fn open_execution(
    input: &LoadedInputs,
    allow_initialize: bool,
) -> Result<FormalBftReadExecutionV1, String> {
    let index_reader = Arc::new(CasBusinessIndexReader {
        store: input.store.clone(),
    });
    FormalBftReadExecutionV1::open_from_handoff(
        input.config.ledger_directory.as_path(),
        input.store.clone(),
        input.anchor.clone(),
        input.config.target.clone(),
        input.handoff_material.clone(),
        index_reader,
        allow_initialize,
    )
}

fn read_canonical_cbor<T: DeserializeOwned + Serialize>(
    path: &Path,
    maximum: u64,
    label: &str,
) -> Result<T, String> {
    let bytes = super::controlled_history_cli::read_bounded(path, maximum)?;
    let mut decoder = serde_cbor::Deserializer::from_slice(bytes.as_slice());
    let value = T::deserialize(&mut decoder).map_err(|error| format!("decode {label}: {error}"))?;
    decoder
        .end()
        .map_err(|error| format!("trailing bytes in {label}: {error}"))?;
    let canonical = encode_canonical_cbor(&value)
        .map_err(|error| format!("canonicalize {label}: {error:?}"))?;
    if canonical != bytes {
        return Err(format!("{label} is not canonical CBOR"));
    }
    Ok(value)
}

pub(super) fn require_existing_directory(path: &Path, label: &str) -> Result<PathBuf, String> {
    if !path.is_absolute() {
        return Err(format!("{label} must be an absolute path"));
    }
    for component in path.ancestors() {
        let metadata = fs::symlink_metadata(component)
            .map_err(|error| format!("inspect {label} path {}: {error}", component.display()))?;
        if metadata.file_type().is_symlink() {
            return Err(format!("{label} path contains a symlink"));
        }
    }
    let metadata = fs::metadata(path)
        .map_err(|error| format!("inspect {label} {}: {error}", path.display()))?;
    if !metadata.is_dir() {
        return Err(format!("{label} must be an existing directory"));
    }
    fs::canonicalize(path).map_err(|error| format!("canonicalize {label}: {error}"))
}

pub(super) fn paths_overlap(left: &Path, right: &Path) -> bool {
    left == right || left.starts_with(right) || right.starts_with(left)
}

fn help() -> String {
    "Usage: oasis7_chain_runtime bft-read install --config <read.json> --certificate <qc.cbor>\n       oasis7_chain_runtime bft-read serve --config <read.json>\n\
install only replays and durably installs the independently pinned H+1 QC;\n\
serve only reopens that exact existing cut and serves public reads on loopback.".into()
}

#[derive(Clone)]
pub(super) struct CasBusinessIndexReader {
    pub(super) store: LocalCasStore,
}

impl BusinessIndexNodeReader for CasBusinessIndexReader {
    fn read_node(&self, digest: &str) -> Result<Option<Vec<u8>>, String> {
        if digest.len() != 64
            || !digest
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err("formal BFT business-index CAS digest is not canonical BLAKE3 hex".into());
        }
        match self
            .store
            .get_range(digest, 0, BUSINESS_INDEX_NODE_MAX_BYTES.saturating_add(1))
        {
            Ok((bytes, true)) if bytes.len() <= BUSINESS_INDEX_NODE_MAX_BYTES => {
                if blake3::hash(&bytes).to_hex().as_str() != digest {
                    return Err("formal BFT business-index CAS node hash mismatch".into());
                }
                Ok(Some(bytes))
            }
            Ok(_) => Err("formal BFT business-index CAS node exceeds its byte limit".into()),
            Err(oasis7_proto::world_error::WorldError::BlobNotFound { .. }) => Ok(None),
            Err(error) => Err(format!("read bounded business-index CAS node: {error:?}")),
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;
    use oasis7_formal_protocol::controlled_authority::formal::business_index::BusinessIndexNodeWriteV1;
    use oasis7_runtime::BlobStore;
    use std::io::Write;
    use std::net::TcpListener;
    use std::os::unix::fs::PermissionsExt;

    struct TestRoot(PathBuf);

    impl TestRoot {
        fn new() -> Result<Self, String> {
            let path = tempfile_dir();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o700))
                .map_err(|error| format!("protect formal BFT CLI test root: {error}"))?;
            Ok(Self(path))
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TestRoot {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    pub(crate) struct InstalledCliFixture {
        _root: TestRoot,
        pub(crate) config_path: PathBuf,
        pub(crate) bind: SocketAddr,
        pub(crate) service_public_key_hex: String,
        pub(crate) read_private_key_hex: String,
    }

    /// Create real operator inputs around a qualified test H, invoke the same
    /// install dispatch as the shipped CLI, and keep the resulting ledger for
    /// an actual HTTP/client test.
    pub(crate) fn install_handoff_for_test(
        trust: FormalReadTrustConfigV1,
        target: BftProfileTargetV1,
        exact_target: FormalBftReadTargetV2,
        handoff_material: ClosedRecord,
        certificate: BftCommitCertificateV2,
        index_nodes: Vec<BusinessIndexNodeWriteV1>,
    ) -> Result<InstalledCliFixture, String> {
        let root = TestRoot::new()?;
        let root_path = root.path();
        let cas_directory = root_path.join("cas");
        let ledger_directory = root_path.join("ledger");
        fs::create_dir(&cas_directory).map_err(|error| error.to_string())?;
        fs::create_dir(&ledger_directory).map_err(|error| error.to_string())?;
        fs::set_permissions(&cas_directory, fs::Permissions::from_mode(0o700))
            .map_err(|error| error.to_string())?;
        fs::set_permissions(&ledger_directory, fs::Permissions::from_mode(0o700))
            .map_err(|error| error.to_string())?;

        let store = LocalCasStore::new(&cas_directory);
        for node in index_nodes {
            let actual_digest = store
                .put_bytes(&node.bytes)
                .map_err(|error| format!("write formal BFT test CAS node: {error:?}"))?;
            if actual_digest != node.digest {
                return Err("formal BFT test CAS node digest changed on write".into());
            }
        }

        let block_id = certificate.proposal.block.block_id()?;
        if certificate.proposal.block.height != exact_target.height
            || block_id != exact_target.block_id
        {
            return Err("formal BFT CLI test QC differs from independent fixture pin".into());
        }
        let trust_path = root_path.join("formal-read-trust.json");
        let material_path = root_path.join("handoff-h.cbor");
        let certificate_path = root_path.join("handoff-h-plus-1.cbor");
        let service_key_path = root_path.join("service-signing-key.hex");
        let config_path = root_path.join("bft-read.json");
        write_private_file(
            &trust_path,
            &serde_json::to_vec(&trust).map_err(|error| error.to_string())?,
        )?;
        write_private_file(
            &material_path,
            &encode_canonical_cbor(&handoff_material)
                .map_err(|error| format!("encode H test material: {error:?}"))?,
        )?;
        write_private_file(
            &certificate_path,
            &encode_canonical_cbor(&certificate)
                .map_err(|error| format!("encode QC test certificate: {error:?}"))?,
        )?;

        let service_key = SigningKey::from_bytes(&[31; 32]);
        write_private_file(
            &service_key_path,
            hex::encode(service_key.to_bytes()).as_bytes(),
        )?;
        let read_key = SigningKey::from_bytes(&[32; 32]);
        let listener = TcpListener::bind("127.0.0.1:0")
            .map_err(|error| format!("reserve loopback test bind: {error}"))?;
        let bind = listener
            .local_addr()
            .map_err(|error| format!("read loopback test bind: {error}"))?;
        drop(listener);
        let config = BftReadConfigV1 {
            schema_version: 1,
            trust_config_path: trust_path,
            target: target.clone(),
            exact_target: exact_target.clone(),
            handoff_material_path: material_path,
            cas_directory,
            ledger_directory,
            bind: bind.to_string(),
            service_signing_key_path: Some(service_key_path),
        };
        config.validate()?;
        write_private_file(
            &config_path,
            &serde_json::to_vec(&config)
                .map_err(|error| format!("encode BFT read CLI test config: {error}"))?,
        )?;

        let config_arg = config_path
            .to_str()
            .ok_or("formal BFT test config path is not UTF-8")?;
        let certificate_arg = certificate_path
            .to_str()
            .ok_or("formal BFT test certificate path is not UTF-8")?;
        run([
            "install",
            "--config",
            config_arg,
            "--certificate",
            certificate_arg,
        ]
        .into_iter())?;

        Ok(InstalledCliFixture {
            _root: root,
            config_path,
            bind,
            service_public_key_hex: hex::encode(service_key.verifying_key().to_bytes()),
            read_private_key_hex: hex::encode(read_key.to_bytes()),
        })
    }

    fn write_private_file(path: &Path, bytes: &[u8]) -> Result<(), String> {
        use std::os::unix::fs::OpenOptionsExt;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)
            .map_err(|error| format!("create private CLI test input: {error}"))?;
        file.write_all(bytes)
            .map_err(|error| format!("write private CLI test input: {error}"))?;
        file.sync_all()
            .map_err(|error| format!("sync private CLI test input: {error}"))
    }

    #[test]
    fn command_parser_requires_exact_config_and_install_certificate_inputs() {
        assert!(parse_command(["serve"].into_iter()).is_err());
        assert!(parse_command(["install", "--config", "x"].into_iter()).is_err());
        assert!(parse_command(["install", "--certificate", "c"].into_iter()).is_err());
        assert!(
            parse_command(["serve", "--config", "x", "--certificate", "c"].into_iter()).is_err()
        );
        assert!(
            parse_command(
                [
                    "install",
                    "--config",
                    "x",
                    "--config",
                    "y",
                    "--certificate",
                    "c"
                ]
                .into_iter()
            )
            .is_err()
        );
        assert!(parse_command(["query", "--config", "x"].into_iter()).is_err());
        assert!(parse_command(["serve", "--config", "x", "--surprise"].into_iter()).is_err());
        assert!(matches!(
            parse_command(["serve", "--config", "x"].into_iter()).unwrap(),
            Command::Serve { .. }
        ));
        assert!(matches!(
            parse_command(["install", "--config", "x", "--certificate", "c"].into_iter()).unwrap(),
            Command::Install { .. }
        ));
    }

    #[test]
    fn canonical_cbor_reader_rejects_trailing_and_noncanonical_bytes() {
        let canonical = encode_canonical_cbor(&vec![1_u64, 2]).unwrap();
        let dir = tempfile_dir();
        let file = dir.join("canonical.cbor");
        fs::write(&file, &canonical).unwrap();
        assert_eq!(
            read_canonical_cbor::<Vec<u64>>(&file, 64, "test").unwrap(),
            vec![1, 2]
        );
        fs::write(&file, [canonical.as_slice(), &[0]].concat()).unwrap();
        assert!(read_canonical_cbor::<Vec<u64>>(&file, 64, "test").is_err());
        fs::write(&file, [0x82, 0x18, 0x01, 0x02]).unwrap();
        assert!(read_canonical_cbor::<Vec<u64>>(&file, 64, "test").is_err());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn cas_business_index_reader_bounds_and_checks_content_hashes() {
        use oasis7_runtime::BlobStore;

        let dir = tempfile_dir();
        let store = LocalCasStore::new(&dir);
        let reader = CasBusinessIndexReader {
            store: store.clone(),
        };
        let bytes = b"canonical business index fixture";
        let digest = store.put_bytes(bytes).unwrap();
        assert_eq!(
            reader.read_node(&digest).unwrap().as_deref(),
            Some(bytes.as_slice())
        );
        assert!(
            reader
                .read_node(&hex::encode([0x55; 32]))
                .unwrap()
                .is_none()
        );
        assert!(reader.read_node("../outside").is_err());

        let oversized = vec![0x5a; BUSINESS_INDEX_NODE_MAX_BYTES + 1];
        let oversized_digest = store.put_bytes(&oversized).unwrap();
        assert!(reader.read_node(&oversized_digest).is_err());

        let corrupt_bytes = b"content-addressed original";
        let corrupt_digest = store.put_bytes(corrupt_bytes).unwrap();
        fs::write(
            store.blobs_dir().join(format!("{corrupt_digest}.blob")),
            b"substituted bytes",
        )
        .unwrap();
        assert!(reader.read_node(&corrupt_digest).is_err());

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn config_and_external_trust_are_private_before_parsing() {
        let dir = tempfile_dir();
        let config_path = dir.join("read.json");
        fs::write(&config_path, b"not-json").unwrap();
        fs::set_permissions(&config_path, fs::Permissions::from_mode(0o644)).unwrap();
        let error = load_inputs(&config_path).err().unwrap();
        assert!(error.contains("owned private regular path"), "{error}");

        let trust_path = dir.join("trust.json");
        fs::write(&trust_path, b"not-json").unwrap();
        fs::set_permissions(&trust_path, fs::Permissions::from_mode(0o644)).unwrap();
        let config_json = minimal_config_json(&trust_path, &dir);
        fs::write(&config_path, serde_json::to_vec(&config_json).unwrap()).unwrap();
        fs::set_permissions(&config_path, fs::Permissions::from_mode(0o600)).unwrap();
        let error = load_inputs(&config_path).err().unwrap();
        assert!(error.contains("owned private regular path"), "{error}");
        fs::remove_dir_all(dir).unwrap();
    }

    fn minimal_config_json(trust_path: &Path, dir: &Path) -> serde_json::Value {
        let signing_key = ed25519_dalek::SigningKey::from_bytes(&[7; 32]);
        serde_json::json!({
            "schema_version": 1,
            "trust_config_path": trust_path.to_string_lossy(),
            "target": {
                "schema_version": 1,
                "profile": "bft",
                "profile_version": 1,
                "wire_version": 2,
                "scope": {
                    "world_id": "world",
                    "chain_id": "chain",
                    "genesis_hash": hex::encode([1; 32])
                },
                "authority_epoch": 2,
                "initial_validators": [{
                    "validator_id": "validator-a",
                    "public_key_hex": hex::encode(signing_key.verifying_key().to_bytes()),
                    "voting_power": 1
                }],
                "schedule_policy": {
                    "epoch_length_ticks": 100,
                    "mapping_ruleset": "epoch-multiply-minus-two-v1",
                    "profile_id": "bft"
                },
                "tick_mapping_ruleset": "formal-position-equals-runtime-tick-v1"
            },
            "exact_target": {
                "height": 1,
                "block_id": hex::encode([2; 32])
            },
            "handoff_material_path": dir.join("handoff.cbor").to_string_lossy(),
            "cas_directory": dir.join("cas").to_string_lossy(),
            "ledger_directory": dir.join("ledger").to_string_lossy(),
            "bind": "127.0.0.1:8645"
        })
    }

    fn tempfile_dir() -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "bft-read-cli-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock")
                .as_nanos()
        ));
        fs::create_dir(&path).expect("create temporary CLI test directory");
        fs::canonicalize(path).expect("canonicalize temporary CLI test directory")
    }
}
