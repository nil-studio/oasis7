//! Fresh DevLocal technical preview only. No governed activation or verified finality claim.
#[cfg(not(test))]
use super::execution_bridge as bridge;
#[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
use super::execution_bridge_real_tests::real_execution_bridge as bridge;
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use oasis7::viewer::viewer_bootstrap_formal_release_runtime_world;
use oasis7::world_service::WorldIdentity;
use oasis7_node::{NodeConfig, NodeRole, NodeRuntime};
use oasis7_proto::storage_profile::StorageProfileConfig;
use oasis7_runtime::{ChainResourceDerivationContext, ReleaseSecurityPolicy, blake3_hex};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    net::SocketAddr,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
#[path = "preview_world_init_archive.rs"]
mod archive;
#[path = "preview_world_init_fs.rs"]
mod files;

const SCHEMA: &str = "oasis7.devlocal.preview-world-init/v1";
const GENESIS_DOMAIN: &str = "oasis7.devlocal.preview-genesis/v1";
const CLAIM: &str =
    "fresh_devlocal_technical_preview_not_governed_activation_or_formal_network_finality";
const LEGACY_RECEIPT_SCHEMA: &str = "v1:world_id,node_id,genesis_nonce,genesis_digest,service_public_key,reader_public_key,status_bind,initial_height,commit,artifact_hashes,relative_paths,source_hash,claim,node_scoped_signature";
const RECEIPT_SCHEMA: &str = "v1:world_id,node_id,genesis_nonce,genesis_digest,service_public_key,reader_public_key,status_bind,initial_height,commit,artifact_hashes,initial_publication_proof_hash,relative_paths,source_hash,claim,node_scoped_signature";
const SIGNATURE_DOMAIN: &[u8] = b"oasis7.devlocal.preview-init-receipt/v1\0";
fn source_hash() -> String {
    blake3_hex(
        &[
            include_bytes!("preview_world_init.rs").as_slice(),
            include_bytes!("preview_world_init_fs.rs").as_slice(),
            include_bytes!("preview_world_init_archive.rs").as_slice(),
            include_bytes!("execution_bridge/world_service_read.rs").as_slice(),
        ]
        .concat(),
    )
}

#[derive(Clone)]
struct Options {
    root: PathBuf,
    key_dir: PathBuf,
    world_id: String,
    node_id: String,
    genesis_nonce: String,
    status_bind: SocketAddr,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    schema: String,
    claim: String,
    storage_profile: String,
    world: WorldIdentity,
    node_id: String,
    genesis_nonce: String,
    service_public_key: String,
    reader_public_key: String,
    status_bind: SocketAddr,
    initial_height: u64,
    published_commit: Value,
    genesis_artifact_hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    initial_publication_proof_hash: Option<String>,
    consumer_public_config_hash: String,
    initializer_source_hash: String,
    receipt_schema_hash: String,
    relative_paths: Value,
    node_scoped_signature: String,
}
fn receipt_payload(receipt: &Receipt) -> Result<Vec<u8>, String> {
    let mut value = json!(receipt);
    value
        .as_object_mut()
        .ok_or("invalid preview receipt object")?
        .remove("node_scoped_signature");
    Ok([SIGNATURE_DOMAIN, encode(value)?.as_slice()].concat())
}
fn relative_paths() -> Value {
    json!({"world":"world","records":"records","store":"store","execution_state":"state.json","initial_artifact":"genesis/initial-artifact.json","initial_publication_proof":"genesis/initial-publication/manifest.json","viewer_public":"viewer.public.json","viewer_environment":"private/viewer.env"})
}

pub(super) fn run<'a>(args: impl Iterator<Item = &'a str>) -> Result<(), String> {
    let options = parse(args)?;
    let receipt = initialize(&options)?;
    println!(
        "{}",
        serde_json::to_string(&receipt).map_err(|_| "preview receipt serialization failed")?
    );
    Ok(())
}
fn parse<'a>(args: impl Iterator<Item = &'a str>) -> Result<Options, String> {
    let mut values = std::collections::BTreeMap::new();
    let mut args = args.peekable();
    while let Some(name) = args.next() {
        if name == "--help" || name == "-h" {
            return Err(help().into());
        }
        if ![
            "--root",
            "--key-dir",
            "--world-id",
            "--node-id",
            "--genesis-nonce",
            "--status-bind",
        ]
        .contains(&name)
        {
            return Err("unknown initialize-preview-world argument".into());
        }
        let value = args
            .next()
            .filter(|v| !v.is_empty())
            .ok_or("missing preview argument value")?;
        if values.insert(name, value).is_some() {
            return Err("duplicate preview argument".into());
        }
    }
    let required = |key| {
        values
            .get(key)
            .copied()
            .ok_or_else(|| format!("missing {key}"))
    };
    let options = Options {
        root: PathBuf::from(required("--root")?),
        key_dir: PathBuf::from(required("--key-dir")?),
        world_id: required("--world-id")?.into(),
        node_id: required("--node-id")?.into(),
        genesis_nonce: required("--genesis-nonce")?.into(),
        status_bind: required("--status-bind")?
            .parse()
            .map_err(|_| "invalid preview status bind")?,
    };
    for id in [&options.world_id, &options.node_id] {
        if id.is_empty()
            || id.len() > 128
            || !id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
        {
            return Err(
                "preview world and node identifiers must be exact canonical identifiers".into(),
            );
        }
    }
    if options.genesis_nonce.len() != 64
        || !options
            .genesis_nonce
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err("genesis nonce must be 32-byte lowercase hex; it is an initialization nonce, not a gameplay seed".into());
    }
    if !options.status_bind.ip().is_loopback() || options.status_bind.port() == 0 {
        return Err("preview status bind must be a nonzero loopback socket".into());
    }
    files::normalized(&options.root)?;
    files::normalized(&options.key_dir)?;
    Ok(options)
}
fn help() -> &'static str {
    "Usage: oasis7_chain_runtime initialize-preview-world --root <fresh-absolute-private-root> --world-id <exact-id> --node-id <exact-id> --key-dir <existing-0700-node-config> --genesis-nonce <64-lowercase-hex> --status-bind <loopback-ip:port>\nUses only genuine Runtime and NodeRuntime local commits; no formal activation/finality, provider funding or test grants. Existing complete matching roots are verified read-only; partial or conflicting roots are rejected."
}
fn canonical(value: Value) -> Value {
    match value {
        Value::Object(map) => {
            let sorted: std::collections::BTreeMap<_, _> = map.into_iter().collect();
            Value::Object(sorted.into_iter().map(|(k, v)| (k, canonical(v))).collect())
        }
        Value::Array(values) => Value::Array(values.into_iter().map(canonical).collect()),
        other => other,
    }
}
fn encode(value: Value) -> Result<Vec<u8>, String> {
    serde_json::to_vec(&canonical(value))
        .map_err(|_| "canonical preview artifact encoding failed".into())
}
fn key_owner(path: &Path) -> Result<u32, String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        Ok(std::fs::metadata(path)
            .map_err(|_| "preview owner metadata unavailable")?
            .uid())
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Err("preview initializer requires Linux or macOS".into())
    }
}
#[derive(Serialize, Deserialize)]
struct InitialCommitEnvelope {
    version: u8,
    world_id: String,
    node_id: String,
    record: oasis7_distfs::FileReplicationRecord,
    payload: Vec<u8>,
    public_key_hex: Option<String>,
    signature_hex: Option<String>,
}
#[derive(Serialize)]
struct InitialCommitSigningPayload<'a> {
    version: u8,
    world_id: &'a str,
    node_id: &'a str,
    record: &'a oasis7_distfs::FileReplicationRecord,
    payload: &'a [u8],
    public_key_hex: Option<&'a str>,
}
fn verify_signed_initial_commit(
    options: &Options,
    height: u64,
    record_bytes: &[u8],
    envelope_bytes: &[u8],
) -> Result<(), String> {
    let (key, _) = super::node_keypair_config::read_existing_node_keypair_in_secure_config_dir(
        &options.key_dir,
    )?;
    let expected = super::derive_node_consensus_signer_keypair(&options.node_id, &key)?;
    let envelope: InitialCommitEnvelope =
        serde_json::from_slice(envelope_bytes).map_err(|_| "initial consensus envelope invalid")?;
    let message: Value = serde_json::from_slice(&envelope.payload)
        .map_err(|_| "initial consensus commit invalid")?;
    if envelope.version != 1
        || envelope.world_id != options.world_id
        || envelope.node_id != options.node_id
        || message["world_id"] != options.world_id
        || message["node_id"] != options.node_id
        || message["height"] != height
        || envelope.public_key_hex.as_deref() != Some(expected.public_key_hex.as_str())
        || envelope.record.writer_id != expected.public_key_hex
    {
        return Err("initial consensus signer or world binding rejected".into());
    }
    let record: Value =
        serde_json::from_slice(record_bytes).map_err(|_| "initial execution record invalid")?;
    if envelope.record.world_id != options.world_id
        || envelope.record.content_hash != blake3_hex(&envelope.payload)
        || envelope.record.size_bytes != envelope.payload.len() as u64
        || message["execution_block_hash"] != record["execution_block_hash"]
        || message["execution_state_root"] != record["execution_state_root"]
        || message["block_hash"] != record["node_block_hash"]
        || message["action_root"] != record["action_root"]
    {
        return Err("initial signed consensus commit differs from pinned execution record".into());
    }
    let signed = InitialCommitSigningPayload {
        version: envelope.version,
        world_id: &envelope.world_id,
        node_id: &envelope.node_id,
        record: &envelope.record,
        payload: &envelope.payload,
        public_key_hex: envelope.public_key_hex.as_deref(),
    };
    let public: [u8; 32] = hex::decode(&expected.public_key_hex)
        .map_err(|_| "initial consensus key invalid")?
        .try_into()
        .map_err(|_| "initial consensus key invalid")?;
    let signature = Signature::from_slice(
        &hex::decode(
            envelope
                .signature_hex
                .as_deref()
                .ok_or("initial consensus signature missing")?,
        )
        .map_err(|_| "initial consensus signature invalid")?,
    )
    .map_err(|_| "initial consensus signature invalid")?;
    VerifyingKey::from_bytes(&public)
        .map_err(|_| "initial consensus key invalid")?
        .verify_strict(
            &serde_json::to_vec(&signed).map_err(|_| "initial consensus encoding invalid")?,
            &signature,
        )
        .map_err(|_| "initial consensus signature rejected".into())
}
fn preview_node_config(
    options: &Options,
    key: &super::node_keypair_config::NodeKeypairConfig,
    replication_root: &Path,
) -> Result<NodeConfig, String> {
    let mut config = NodeConfig::new(&options.node_id, &options.world_id, NodeRole::Sequencer)
        .and_then(|c| c.with_tick_interval(Duration::from_millis(50)))
        .map_err(|_| "preview node configuration failed")?;
    let bindings = super::build_validator_signer_public_keys(
        &config.pos_config.validators,
        key,
        &std::collections::BTreeMap::new(),
    )?;
    config.pos_config = config
        .pos_config
        .with_validator_signer_public_keys(bindings)
        .map_err(|_| "preview provisioned validator signer binding failed")?;
    let replication = super::build_node_replication_config(
        &options.node_id,
        replication_root,
        key,
        &StorageProfileConfig::default(),
        &[],
        &[],
        false,
    )?;
    Ok(config.with_replication(replication))
}
fn initialize(options: &Options) -> Result<Receipt, String> {
    if !cfg!(any(target_os = "linux", target_os = "macos")) {
        return Err(
            "preview initialization requires Linux or macOS; no storage was initialized".into(),
        );
    }
    let (node_key, _) =
        super::node_keypair_config::read_existing_node_keypair_in_secure_config_dir(
            &options.key_dir,
        )?;
    let service = super::build_feedback_submit_signer(&options.node_id, &node_key)?;
    let owner = key_owner(&options.key_dir)?;
    files::verify_private(&options.key_dir, true, owner)?;
    files::verify_private(&options.key_dir.join("node-keypair.toml"), false, owner)?;
    if options
        .root
        .try_exists()
        .map_err(|_| "preview root existence check failed")?
    {
        return verify_existing(options, &service.public_key_hex, owner);
    }
    let stage = files::Staging::new(&options.root)?;
    if key_owner(&stage.path)? != owner {
        return Err("preview initializer must run as the node identity owner".into());
    }
    for name in ["world", "records", "store", "private", "genesis"] {
        files::private_dir(&stage.path.join(name))?;
    }
    let (mut world, config) = viewer_bootstrap_formal_release_runtime_world()?;
    let artifact = encode(json!({
        "domain":GENESIS_DOMAIN,"world_id":options.world_id,"node_id":options.node_id,
        "genesis_nonce":options.genesis_nonce,"service_public_key":service.public_key_hex,
        "preset":"formal_release_runtime_world","configuration":config,
        "initial_snapshot":world.snapshot(),"initial_journal":world.journal(),
    }))?;
    let artifact_hash = blake3_hex(&artifact);
    let identity = WorldIdentity {
        world_id: options.world_id.clone(),
        genesis_digest: format!("blake3:{artifact_hash}"),
    };
    let initial_height = world.state().time;
    if initial_height == 0 {
        return Err("canonical preview preset must have a real initial Runtime boundary".into());
    }
    world
        .bind_cognition_runtime(&options.world_id, "main", 0, None, "pending", 0)
        .map_err(|_| "preview Runtime binding failed")?;
    let manifest_hash = world
        .current_manifest_hash()
        .map_err(|_| "preview manifest unavailable")?;
    world
        .save_to_dir_with_chain_resource_context(
            stage.path.join("world"),
            ChainResourceDerivationContext {
                world_id: &options.world_id,
                chain_id: &options.world_id,
                genesis_ref: Some(&identity.genesis_digest),
                created_at_height: initial_height,
                manifest_height: initial_height,
                commit_block_hash: None,
                tick: initial_height,
            },
            manifest_hash,
            "devlocal-preview-initialization-v1",
        )
        .map_err(|_| "preview initial world persistence failed")?;
    files::write_new(&stage.path.join("genesis/initial-artifact.json"), &artifact)?;
    files::write_new(
        &stage.path.join("world/world-service-identity.json"),
        &encode(json!(&identity))?,
    )?;
    let local_boundary_hash = blake3_hex(&encode(
        json!({"domain":"oasis7.devlocal.preview-setup-boundary/v1","genesis":identity,"height":initial_height}),
    )?);
    let bootstrap = bridge::derive_local_execution_bootstrap(
        &stage.path.join("world"),
        &stage.path.join("records"),
        &options.world_id,
        initial_height,
        &local_boundary_hash,
        &ReleaseSecurityPolicy::default(),
    )?;
    let driver = bridge::NodeRuntimeExecutionDriver::new_with_local_bootstrap(
        stage.path.join("state.json"),
        stage.path.join("world"),
        stage.path.join("records"),
        stage.path.join("store"),
        &StorageProfileConfig::default(),
        bootstrap.clone(),
    )?;
    let config = preview_node_config(options, &node_key, &stage.path.join("replication"))?;
    let mut node = NodeRuntime::new(config)
        .with_execution_hook(driver)
        .with_local_execution_bootstrap(bootstrap);
    node.start()
        .map_err(|_| "preview local node start failed")?;
    let until = Instant::now() + Duration::from_secs(10);
    let mut committed = false;
    while Instant::now() < until {
        let snapshot = node.snapshot();
        if snapshot.last_error.is_some() {
            break;
        }
        if snapshot.consensus.committed_height > initial_height {
            committed = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    node.stop().map_err(|_| "preview local node stop failed")?;
    if !committed {
        return Err("preview real local commit did not complete within ten seconds".into());
    }
    let pinned = bridge::world_service_read::pin(
        &stage.path.join("records"),
        &stage.path.join("store"),
        &identity,
        None,
    )?;
    if pinned.commit.position <= initial_height {
        return Err("preview CAS publication did not advance real commit".into());
    }
    let initial_publication_proof_hash =
        archive::publish(&stage.path, options, &identity, &pinned.commit)?;
    let _ = bridge::derive_service_execution_bootstrap(
        &stage.path.join("world"),
        &stage.path.join("records"),
        &stage.path.join("store"),
        &options.world_id,
    )?;
    let mut reader = [0; 32];
    getrandom::fill(&mut reader).map_err(|_| "preview reader entropy unavailable")?;
    let reader_key = ed25519_dalek::SigningKey::from_bytes(&reader);
    let reader_public = hex::encode(reader_key.verifying_key().to_bytes());
    let public_config = encode(
        json!({"schema":"oasis7.devlocal.preview-viewer/v1","endpoint":format!("http://{}",options.status_bind),"world":identity,"scope":"public","service_public_key":service.public_key_hex,"reader_public_key":reader_public,"read_private_key_file":"private/public-reader.key","claim":CLAIM}),
    )?;
    files::write_new(&stage.path.join("private/public-reader.key"), &reader)?;
    let env = format!(
        "OASIS7_WORLD_SERVICE_ENDPOINT=http://{}\nOASIS7_WORLD_SERVICE_PUBLIC_KEY={}\nOASIS7_WORLD_SERVICE_WORLD_ID={}\nOASIS7_WORLD_SERVICE_GENESIS_DIGEST={}\nOASIS7_WORLD_SERVICE_SCOPE=public\nOASIS7_WORLD_SERVICE_READ_PRIVATE_KEY={}\n",
        options.status_bind,
        service.public_key_hex,
        options.world_id,
        identity.genesis_digest,
        hex::encode(reader)
    );
    files::write_new(&stage.path.join("private/viewer.env"), env.as_bytes())?;
    files::write_new(&stage.path.join("viewer.public.json"), &public_config)?;
    let mut receipt = Receipt {
        schema: SCHEMA.into(),
        claim: CLAIM.into(),
        storage_profile: "dev_local".into(),
        world: identity,
        node_id: options.node_id.clone(),
        genesis_nonce: options.genesis_nonce.clone(),
        service_public_key: service.public_key_hex,
        reader_public_key: reader_public,
        status_bind: options.status_bind,
        initial_height,
        published_commit: json!(pinned.commit),
        genesis_artifact_hash: artifact_hash,
        initial_publication_proof_hash: Some(initial_publication_proof_hash),
        consumer_public_config_hash: blake3_hex(&public_config),
        initializer_source_hash: source_hash(),
        receipt_schema_hash: blake3_hex(RECEIPT_SCHEMA.as_bytes()),
        relative_paths: relative_paths(),
        node_scoped_signature: String::new(),
    };
    let private: [u8; 32] = hex::decode(&service.private_key_hex)
        .map_err(|_| "invalid node-scoped preview signer")?
        .try_into()
        .map_err(|_| "invalid node-scoped preview signer")?;
    receipt.node_scoped_signature = hex::encode(
        SigningKey::from_bytes(&private)
            .sign(&receipt_payload(&receipt)?)
            .to_bytes(),
    );
    files::write_new(
        &stage.path.join("initialization-receipt.json"),
        &encode(json!(&receipt))?,
    )?;
    let _ = verify_at(&stage.path, options, &receipt.service_public_key, owner)?;
    stage.publish()?;
    Ok(receipt)
}
fn verify_existing(options: &Options, service_public: &str, owner: u32) -> Result<Receipt, String> {
    verify_at(&options.root, options, service_public, owner)
}
fn verify_at(
    root: &Path,
    options: &Options,
    service_public: &str,
    owner: u32,
) -> Result<Receipt, String> {
    files::normalized(root)?;
    files::verify_private(root, true, owner)?;
    let receipt: Receipt = serde_json::from_slice(&files::read_bounded(
        &root.join("initialization-receipt.json"),
        256 * 1024,
    )?)
    .map_err(|_| "complete preview receipt required; partial root cannot be repaired")?;
    if receipt.schema != SCHEMA
        || receipt.claim != CLAIM
        || receipt.storage_profile != "dev_local"
        || receipt.world.world_id != options.world_id
        || receipt.node_id != options.node_id
        || receipt.genesis_nonce != options.genesis_nonce
        || receipt.status_bind != options.status_bind
        || receipt.service_public_key != service_public
        || (receipt.receipt_schema_hash != blake3_hex(RECEIPT_SCHEMA.as_bytes())
            && receipt.receipt_schema_hash != blake3_hex(LEGACY_RECEIPT_SCHEMA.as_bytes()))
    {
        return Err(
            "existing preview receipt does not match exact requested initialization".into(),
        );
    }
    let public: [u8; 32] = hex::decode(service_public)
        .map_err(|_| "invalid preview service pin")?
        .try_into()
        .map_err(|_| "invalid preview service pin")?;
    let signature = Signature::from_slice(
        &hex::decode(&receipt.node_scoped_signature)
            .map_err(|_| "invalid preview receipt signature")?,
    )
    .map_err(|_| "invalid preview receipt signature")?;
    VerifyingKey::from_bytes(&public)
        .map_err(|_| "invalid preview service pin")?
        .verify_strict(&receipt_payload(&receipt)?, &signature)
        .map_err(|_| "node-scoped preview receipt signature rejected")?;
    let original: oasis7::world_service::CommitRef =
        serde_json::from_value(receipt.published_commit.clone())
            .map_err(|_| "preview original commit invalid")?;
    if receipt.initial_publication_proof_hash.is_none() {
        return Err(
            "existing preview world has no archived initial publication proof; restore from a verified backup; no repair performed".into(),
        );
    }
    archive::verify(root, options, &receipt, &original, owner)?;
    if receipt.initializer_source_hash != source_hash()
        || receipt.relative_paths != relative_paths()
    {
        return Err(
            "existing preview receipt does not match exact initializer source and paths".into(),
        );
    }
    let artifact = files::read_bounded(
        &root.join("genesis/initial-artifact.json"),
        16 * 1024 * 1024,
    )?;
    let value: Value =
        serde_json::from_slice(&artifact).map_err(|_| "preview genesis artifact invalid")?;
    if artifact != encode(value.clone())?
        || blake3_hex(&artifact) != receipt.genesis_artifact_hash
        || receipt.world.genesis_digest != format!("blake3:{}", receipt.genesis_artifact_hash)
        || value["domain"] != GENESIS_DOMAIN
        || value["world_id"] != options.world_id
        || value["node_id"] != options.node_id
        || value["genesis_nonce"] != options.genesis_nonce
        || value["service_public_key"] != service_public
    {
        return Err("preview genesis digest or scope mismatch".into());
    }
    let identity = bridge::world_service_read::identity(&root.join("world"), &options.world_id)?;
    if identity != receipt.world {
        return Err("preview identity differs from admitted genesis".into());
    }
    let current = bridge::world_service_read::pin(
        &root.join("records"),
        &root.join("store"),
        &identity,
        None,
    )?;
    if current.commit.position < original.position {
        return Err("preview committed head regressed".into());
    }
    let _ = bridge::derive_service_execution_bootstrap(
        &root.join("world"),
        &root.join("records"),
        &root.join("store"),
        &options.world_id,
    )?;
    files::verify_private(&root.join("private"), true, owner)?;
    files::verify_private(&root.join("private/public-reader.key"), false, owner)?;
    files::verify_private(&root.join("private/viewer.env"), false, owner)?;
    let seed: [u8; 32] = files::read_bounded(&root.join("private/public-reader.key"), 32)?
        .try_into()
        .map_err(|_| "invalid private preview reader")?;
    if hex::encode(
        ed25519_dalek::SigningKey::from_bytes(&seed)
            .verifying_key()
            .to_bytes(),
    ) != receipt.reader_public_key
    {
        return Err("preview reader public pin mismatch".into());
    }
    let public = files::read_bounded(&root.join("viewer.public.json"), 64 * 1024)?;
    let expected_public = encode(
        json!({"schema":"oasis7.devlocal.preview-viewer/v1","endpoint":format!("http://{}",options.status_bind),"world":identity,"scope":"public","service_public_key":service_public,"reader_public_key":receipt.reader_public_key,"read_private_key_file":"private/public-reader.key","claim":CLAIM}),
    )?;
    if blake3_hex(&public) != receipt.consumer_public_config_hash || public != expected_public {
        return Err("preview consumer configuration hash or scope mismatch".into());
    }
    let expected_env = format!(
        "OASIS7_WORLD_SERVICE_ENDPOINT=http://{}\nOASIS7_WORLD_SERVICE_PUBLIC_KEY={}\nOASIS7_WORLD_SERVICE_WORLD_ID={}\nOASIS7_WORLD_SERVICE_GENESIS_DIGEST={}\nOASIS7_WORLD_SERVICE_SCOPE=public\nOASIS7_WORLD_SERVICE_READ_PRIVATE_KEY={}\n",
        options.status_bind,
        service_public,
        options.world_id,
        identity.genesis_digest,
        hex::encode(seed)
    );
    if files::read_bounded(&root.join("private/viewer.env"), 8192)? != expected_env.as_bytes() {
        return Err("private preview consumer configuration mismatch".into());
    }
    Ok(receipt)
}

#[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
#[path = "preview_world_init_tests.rs"]
mod tests;
