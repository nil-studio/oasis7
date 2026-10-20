use super::local_io as io;
use oasis7::network_tier_manifest::LoadedNetworkTierManifest;
use serde::{Deserialize, Serialize};
use std::fs;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

pub(super) const CONFIG_FILE: &str = "node.json";
pub(super) const PROFILE: &str = "compatibility_observer_v1";
pub(super) const FORMAL_OBSERVER_PROFILE: &str = "formal_public_observer_v1";
const API_V1: &str = "oasis7.local-node/v1";
const API_V2: &str = "oasis7.local-node/v2";
const API_V3: &str = "oasis7.local-node/v3";
pub(super) struct ChainConfig;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CompatibilityChainConfigV1 {
    pub api_version: String,
    pub runtime_profile: String,
    pub world_id: String,
    pub chain_id: String,
    pub genesis_sha256: String,
    pub network_tier_manifest: PathBuf,
    pub manifest_sha256: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PinnedFile {
    pub path: PathBuf,
    pub sha256: String,
}
impl PinnedFile {
    fn capture(path: PathBuf) -> Result<Self, String> {
        Ok(Self {
            sha256: io::sha256(&path)?,
            path: io::regular(&path)?,
        })
    }
    fn verify(&self) -> Result<(), String> {
        if io::sha256(&self.path)? != self.sha256 {
            return Err(format!(
                "pinned chain material changed: {}",
                self.path.display()
            ));
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CompatibilityChainAnchor {
    pub runtime_profile: String,
    pub world_id: String,
    pub chain_id: String,
    pub manifest: PinnedFile,
    pub genesis: PinnedFile,
    pub registry: PinnedFile,
    pub release_bundle: PinnedFile,
    pub bootstrap_peers: PinnedFile,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FormalPublicObserverChainConfigV2 {
    api_version: String,
    runtime_profile: String,
    formal_observer: super::formal_observer::FormalObserverConfigV1,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FormalPublicObserverChainAnchorV2 {
    runtime_profile: String,
    world_id: String,
    chain_id: String,
    formal_observer: super::formal_observer::FormalObserverConfigV1,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FormalFullStateSyncChainConfigV3 {
    api_version: String,
    runtime_profile: String,
    formal_full_sync: super::formal_full_sync::FormalFullSyncConfigV1,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FormalFullStateSyncChainAnchorV3 {
    runtime_profile: String,
    world_id: String,
    chain_id: String,
    formal_full_sync: super::formal_full_sync::FormalFullSyncConfigV1,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub(super) enum ChainAnchor {
    Compatibility(Box<CompatibilityChainAnchor>),
    FormalPublicObserver(Box<FormalPublicObserverChainAnchorV2>),
    FormalFullStateSync(Box<FormalFullStateSyncChainAnchorV3>),
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct IdentityBinding {
    pub api_version: String,
    pub chain: ChainAnchor,
    pub data_dir: PathBuf,
    pub node_id: String,
    pub root_public_key: String,
    pub peer_id: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct NodeConfig {
    pub api_version: String,
    pub identity: IdentityBinding,
    pub role: String,
    pub rpc_bind: String,
    pub additional_peers: Vec<String>,
}
impl ChainConfig {
    pub(super) fn load(path: &Path) -> Result<ChainAnchor, String> {
        let path = io::regular(path)?;
        let bytes = fs::read(&path).map_err(|error| error.to_string())?;
        Self::load_bytes(&path, &bytes)
    }

    /// Parse an already captured chain-config document while resolving any
    /// relative references against its original canonical path. Callers that
    /// pin a digest can therefore validate the exact bytes they approved
    /// without reopening a mutable source path between hashing and parsing.
    pub(super) fn load_bytes(path: &Path, bytes: &[u8]) -> Result<ChainAnchor, String> {
        let envelope: serde_json::Value =
            serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
        match envelope["api_version"].as_str() {
            Some("oasis7.chain-config/v1") => Self::load_compatibility(path, bytes),
            Some("oasis7.chain-config/v2") => Self::load_formal_public_observer(bytes),
            Some("oasis7.chain-config/v3") => Self::load_formal_full_state_sync(bytes),
            _ => Err("unsupported chain configuration schema".into()),
        }
    }
    fn load_compatibility(path: &Path, bytes: &[u8]) -> Result<ChainAnchor, String> {
        let mut source: CompatibilityChainConfigV1 =
            serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
        if source.api_version != "oasis7.chain-config/v1" || source.runtime_profile != PROFILE {
            return Err("unsupported chain/runtime profile; formal and guarded activation are not available through this compatibility observer adapter".into());
        }
        if !source.network_tier_manifest.is_absolute() {
            source.network_tier_manifest = path
                .parent()
                .ok_or("chain config parent missing")?
                .join(&source.network_tier_manifest);
        }
        let manifest = PinnedFile::capture(source.network_tier_manifest)?;
        if manifest.sha256 != source.manifest_sha256 {
            return Err("trusted manifest SHA256 mismatch".into());
        }
        let loaded = LoadedNetworkTierManifest::load(&manifest.path)?;
        loaded.validate_runtime_support()?;
        if loaded.manifest.tier != "public_testnet"
            || !loaded.manifest.validator_policy.allow_observer_nodes
        {
            return Err(
                "local observer adapter requires public_testnet allow_observer_nodes=true".into(),
            );
        }
        let genesis = PinnedFile::capture(resolve_ref(
            &manifest.path,
            &loaded.manifest.runtime_refs.genesis_ref,
        ))?;
        if genesis.sha256 != source.genesis_sha256 {
            return Err("trusted genesis SHA256 mismatch".into());
        }
        let document: serde_json::Value = io::read(&genesis.path)?;
        if document["world_id"].as_str() != Some(source.world_id.as_str())
            || document["chain_id"].as_str() != Some(source.chain_id.as_str())
            || loaded.manifest.chain_id != source.chain_id
            || source.world_id != source.chain_id
        {
            return Err("trusted world/chain/genesis identity mismatch".into());
        }
        let manifest_document: serde_json::Value = io::read(&manifest.path)?;
        let registry_ref = manifest_document["deployment_validator_registry"]["ref"]
            .as_str()
            .ok_or("manifest-bound deployment registry is required")?;
        let registry_name = Path::new(registry_ref)
            .file_name()
            .ok_or("registry name missing")?;
        let registry = PinnedFile::capture(
            manifest
                .path
                .parent()
                .ok_or("manifest parent missing")?
                .join(registry_name),
        )?;
        let release_bundle = PinnedFile::capture(resolve_ref(
            &manifest.path,
            &loaded.manifest.runtime_refs.release_candidate_bundle_ref,
        ))?;
        let bootstrap_peers = PinnedFile::capture(resolve_ref(
            &manifest.path,
            &loaded.manifest.runtime_refs.bootstrap_peer_ref,
        ))?;
        let anchor = CompatibilityChainAnchor {
            runtime_profile: PROFILE.into(),
            world_id: source.world_id,
            chain_id: source.chain_id,
            manifest,
            genesis,
            registry,
            release_bundle,
            bootstrap_peers,
        };
        let registry_document = &manifest_document["deployment_validator_registry"];
        if registry_document["sha256"].as_str() != Some(anchor.registry.sha256.as_str()) {
            return Err("manifest registry SHA256 mismatch".into());
        }
        let registry = super::super::governance_registry::load_genesis_finality_registry(
            &anchor.registry.path,
        )?;
        if registry.threshold == 0
            || usize::from(registry.threshold) > registry.signer_bindings.len()
            || registry.signer_bindings.len()
                != loaded.manifest.validator_policy.target_validator_count as usize
        {
            return Err("manifest observer registry threshold/count mismatch".into());
        }
        let canonical =
            super::super::runtime_authority::canonicalize_registry_for_authority(registry);
        let semantic = super::super::runtime_authority::semantic_registry_sha256(&canonical)?;
        if registry_document["semantic_sha256"].as_str() != Some(semantic.as_str()) {
            return Err("manifest registry semantic SHA256 mismatch".into());
        }
        Ok(ChainAnchor::Compatibility(Box::new(anchor)))
    }
    fn load_formal_public_observer(bytes: &[u8]) -> Result<ChainAnchor, String> {
        let source: FormalPublicObserverChainConfigV2 =
            serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
        if source.api_version != "oasis7.chain-config/v2"
            || source.runtime_profile != FORMAL_OBSERVER_PROFILE
        {
            return Err("unsupported chain/runtime profile".into());
        }
        super::formal_observer::validate(&source.formal_observer)?;
        Ok(ChainAnchor::FormalPublicObserver(Box::new(
            FormalPublicObserverChainAnchorV2 {
                runtime_profile: source.runtime_profile,
                world_id: source.formal_observer.world_id.clone(),
                chain_id: source.formal_observer.chain_id.clone(),
                formal_observer: source.formal_observer,
            },
        )))
    }
    fn load_formal_full_state_sync(bytes: &[u8]) -> Result<ChainAnchor, String> {
        let source: FormalFullStateSyncChainConfigV3 =
            serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
        if source.api_version != "oasis7.chain-config/v3"
            || source.runtime_profile != super::formal_full_sync::PROFILE
            || source.formal_full_sync.grant.is_some()
        {
            return Err("full-state sync chain config must use its profile and receive its grant after local key creation".into());
        }
        source.formal_full_sync.validate()?;
        Ok(ChainAnchor::FormalFullStateSync(Box::new(
            FormalFullStateSyncChainAnchorV3 {
                runtime_profile: source.runtime_profile,
                world_id: source.formal_full_sync.world_id.clone(),
                chain_id: source.formal_full_sync.chain_id.clone(),
                formal_full_sync: source.formal_full_sync,
            },
        )))
    }
}
impl ChainAnchor {
    pub(super) fn verify(&self) -> Result<(), String> {
        match self {
            Self::Compatibility(anchor) => {
                if anchor.runtime_profile != PROFILE {
                    return Err("unsupported runtime profile".into());
                }
                for file in [
                    &anchor.manifest,
                    &anchor.genesis,
                    &anchor.registry,
                    &anchor.release_bundle,
                    &anchor.bootstrap_peers,
                ] {
                    file.verify()?;
                }
                let loaded = LoadedNetworkTierManifest::load(&anchor.manifest.path)?;
                loaded.validate_runtime_support()?;
                if loaded.manifest.chain_id != anchor.chain_id || anchor.world_id != anchor.chain_id
                {
                    return Err("pinned chain identity mismatch".into());
                }
            }
            Self::FormalPublicObserver(anchor) => {
                if anchor.runtime_profile != FORMAL_OBSERVER_PROFILE {
                    return Err("unsupported runtime profile".into());
                }
                super::formal_observer::validate(&anchor.formal_observer)?;
                if anchor.world_id != anchor.formal_observer.world_id
                    || anchor.chain_id != anchor.formal_observer.chain_id
                {
                    return Err("formal observer identity differs from fixed trust".into());
                }
            }
            Self::FormalFullStateSync(anchor) => {
                if anchor.runtime_profile != super::formal_full_sync::PROFILE {
                    return Err("unsupported runtime profile".into());
                }
                anchor.formal_full_sync.validate()?;
                if anchor.world_id != anchor.formal_full_sync.world_id
                    || anchor.chain_id != anchor.formal_full_sync.chain_id
                {
                    return Err("full-state sync identity differs from fixed trust".into());
                }
            }
        }
        Ok(())
    }
    pub(super) fn runtime_profile(&self) -> &str {
        match self {
            Self::Compatibility(anchor) => &anchor.runtime_profile,
            Self::FormalPublicObserver(anchor) => &anchor.runtime_profile,
            Self::FormalFullStateSync(anchor) => &anchor.runtime_profile,
        }
    }
    pub(super) fn world_id(&self) -> &str {
        match self {
            Self::Compatibility(anchor) => &anchor.world_id,
            Self::FormalPublicObserver(anchor) => &anchor.world_id,
            Self::FormalFullStateSync(anchor) => &anchor.world_id,
        }
    }
    #[cfg(test)]
    pub(super) fn genesis_sha256(&self) -> Result<&str, String> {
        match self {
            Self::Compatibility(anchor) => Ok(&anchor.genesis.sha256),
            Self::FormalPublicObserver(_) => {
                Err("formal public observer has no pinned local genesis file".into())
            }
            Self::FormalFullStateSync(_) => {
                Err("formal full-state sync has no pinned local genesis file".into())
            }
        }
    }
    pub(super) fn chain_id(&self) -> &str {
        match self {
            Self::Compatibility(anchor) => &anchor.chain_id,
            Self::FormalPublicObserver(anchor) => &anchor.chain_id,
            Self::FormalFullStateSync(anchor) => &anchor.chain_id,
        }
    }
    fn local_api_version(&self) -> &'static str {
        match self {
            Self::Compatibility(_) => API_V1,
            Self::FormalPublicObserver(_) => API_V2,
            Self::FormalFullStateSync(_) => API_V3,
        }
    }
    pub(super) fn local_role(&self) -> &'static str {
        match self {
            Self::FormalFullStateSync(_) => "full_state_sync",
            Self::Compatibility(_) | Self::FormalPublicObserver(_) => "observer",
        }
    }
    pub(super) fn formal_observer(
        &self,
    ) -> Option<&super::formal_observer::FormalObserverConfigV1> {
        match self {
            Self::FormalPublicObserver(anchor) => Some(&anchor.formal_observer),
            Self::Compatibility(_) | Self::FormalFullStateSync(_) => None,
        }
    }
    pub(super) fn formal_full_sync(
        &self,
    ) -> Option<&super::formal_full_sync::FormalFullSyncConfigV1> {
        match self {
            Self::FormalFullStateSync(anchor) => Some(&anchor.formal_full_sync),
            Self::Compatibility(_) | Self::FormalPublicObserver(_) => None,
        }
    }
    pub(super) fn formal_full_sync_mut(
        &mut self,
    ) -> Option<&mut super::formal_full_sync::FormalFullSyncConfigV1> {
        match self {
            Self::FormalFullStateSync(anchor) => Some(&mut anchor.formal_full_sync),
            Self::Compatibility(_) | Self::FormalPublicObserver(_) => None,
        }
    }
    fn same_immutable_identity(&self, other: &Self) -> bool {
        let mut left = self.clone();
        let mut right = other.clone();
        if let Some(config) = left.formal_full_sync_mut() {
            config.grant = None;
        }
        if let Some(config) = right.formal_full_sync_mut() {
            config.grant = None;
        }
        left == right
    }
    pub(super) fn compatibility(&self) -> Result<&CompatibilityChainAnchor, String> {
        match self {
            Self::Compatibility(anchor) => Ok(anchor),
            Self::FormalPublicObserver(_) => {
                Err("formal public observer has no local full-runtime chain bundle".into())
            }
            Self::FormalFullStateSync(_) => {
                Err("formal full-state sync has no compatibility runtime bundle".into())
            }
        }
    }
}
pub(super) fn initialize(
    chain: ChainAnchor,
    data: &Path,
    requested_id: Option<&str>,
    bind: &str,
) -> Result<NodeConfig, String> {
    validate_bind(bind)?;
    chain.verify()?;
    if !data.is_absolute() {
        return Err("data directory must be absolute".into());
    }
    io::reject_symlinks(data)?;
    io::private_dir(data)?;
    let data = fs::canonicalize(data).map_err(|e| e.to_string())?;
    if !data.join(CONFIG_FILE).exists() && !data.join("initialization.json").exists() {
        let foreign = fs::read_dir(&data)
            .map_err(|e| e.to_string())?
            .any(|e| e.map_or(true, |e| e.file_name() != "initialization.lock"));
        if foreign {
            return Err(
                "existing unbound node data is not empty; explicit migration is required".into(),
            );
        }
    }
    let lock = io::private_file(&data.join("initialization.lock"))?;
    lock.try_lock()
        .map_err(|_| "node initialization is already running".to_string())?;
    let config_path = data.join(CONFIG_FILE);
    if config_path.exists() {
        let existing = NodeConfig::load(&config_path)?;
        if !existing.identity.chain.same_immutable_identity(&chain)
            || requested_id.is_some_and(|id| id != existing.identity.node_id)
            || existing.rpc_bind != bind
        {
            return Err(
                "node init conflicts with existing chain, identity or local configuration".into(),
            );
        }
        existing.verify_identity()?;
        return Ok(existing);
    }
    let intent_path = data.join("initialization.json");
    #[derive(Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Intent {
        chain: ChainAnchor,
        node_id: String,
        rpc_bind: String,
    }
    let intent: Intent = if intent_path.exists() {
        let intent: Intent = io::read(&intent_path)?;
        if intent.chain != chain
            || intent.rpc_bind != bind
            || requested_id.is_some_and(|id| id != intent.node_id)
        {
            return Err("incomplete initialization identity conflict".into());
        }
        intent
    } else {
        let unexpected = fs::read_dir(&data)
            .map_err(|e| e.to_string())?
            .any(|e| e.map_or(true, |e| e.file_name() != "initialization.lock"));
        if unexpected {
            return Err(
                "existing unbound node data is not empty; explicit migration is required".into(),
            );
        }
        let node_id = requested_id
            .map(str::to_string)
            .unwrap_or(format!("observer-{}", io::random_id()?));
        if node_id.trim().is_empty() || node_id.len() > 256 || node_id.starts_with("triad-testnet-")
        {
            return Err(
                "local observer label is empty, oversized or a reserved managed identity".into(),
            );
        }
        let intent = Intent {
            chain: chain.clone(),
            node_id,
            rpc_bind: bind.into(),
        };
        io::write(&intent_path, &intent)?;
        intent
    };
    let keys = data.join("keys");
    let (keypair, _) = if data.join("node-binding.json").exists() {
        super::super::node_keypair_config::read_existing_node_keypair_in_secure_config_dir(&keys)?
    } else {
        io::private_dir(&keys)?;
        super::super::node_keypair_config::ensure_node_keypair_in_secure_config_dir(&keys)?
    };
    let derived =
        super::super::derive_node_libp2p_identity_keypair_config(&intent.node_id, &keypair)?;
    let peer_id = oasis7_node::derive_libp2p_identity_keypair(&derived.private_key_hex)
        .map_err(|e| format!("{e:?}"))?
        .public()
        .to_peer_id()
        .to_string();
    let identity = IdentityBinding {
        api_version: chain.local_api_version().into(),
        chain,
        data_dir: data.clone(),
        node_id: intent.node_id,
        root_public_key: keypair.public_key_hex,
        peer_id,
    };
    let role = identity.chain.local_role().to_owned();
    let binding_path = data.join("node-binding.json");
    if binding_path.exists() && io::read::<IdentityBinding>(&binding_path)? != identity {
        return Err("existing local identity binding differs; explicit migration required".into());
    }
    io::write(&binding_path, &identity)?;
    let config = NodeConfig {
        api_version: identity.api_version.clone(),
        identity,
        role,
        rpc_bind: intent.rpc_bind,
        additional_peers: vec![],
    };
    io::private_file(&data.join("process.lock"))?
        .sync_all()
        .map_err(|e| e.to_string())?;
    io::write(&config_path, &config)?;
    Ok(config)
}
fn validate_bind(bind: &str) -> Result<(), String> {
    let addr: SocketAddr = bind
        .parse()
        .map_err(|_| "RPC bind must be a numeric loopback IP:port".to_string())?;
    if !addr.ip().is_loopback() || addr.port() == 0 {
        return Err("local observer RPC must use an explicit loopback port".into());
    }
    Ok(())
}
impl NodeConfig {
    pub(super) fn load(path: &Path) -> Result<Self, String> {
        let config: Self = io::read(path)?;
        super::super::controlled_live_config::require_private_owned(path, false)?;
        let expected_api = config.identity.chain.local_api_version();
        if config.api_version != expected_api
            || config.identity.api_version != expected_api
            || config.role != config.identity.chain.local_role()
        {
            return Err("unsupported local node schema/role".into());
        }
        validate_bind(&config.rpc_bind)?;
        io::reject_symlinks(&config.identity.data_dir)?;
        super::super::controlled_live_config::require_private_owned(
            &config.identity.data_dir,
            true,
        )?;
        if fs::canonicalize(path).map_err(|e| e.to_string())?
            != config.identity.data_dir.join(CONFIG_FILE)
        {
            return Err("node config must remain inside its bound data directory".into());
        }
        let binding_path = config.identity.data_dir.join("node-binding.json");
        super::super::controlled_live_config::require_private_owned(&binding_path, false)?;
        let binding: IdentityBinding = io::read(&binding_path)?;
        let mut bound_identity = binding;
        let mut configured_identity = config.identity.clone();
        if let Some(full_sync) = bound_identity.chain.formal_full_sync_mut() {
            full_sync.grant = None;
        }
        if let Some(full_sync) = configured_identity.chain.formal_full_sync_mut() {
            full_sync.grant = None;
        }
        if bound_identity != configured_identity {
            return Err("local node config changed fixed identity/world binding".into());
        }
        if let Some(full_sync) = config.identity.chain.formal_full_sync()
            && let Some(grant) = full_sync.grant.as_ref()
        {
            let (keypair, _) =
                super::super::node_keypair_config::read_existing_node_keypair_in_secure_config_dir(
                    &config.identity.data_dir.join("keys"),
                )?;
            super::formal_full_sync::validate_grant_for_node(
                full_sync,
                grant,
                &keypair.public_key_hex,
                grant.body.not_before_unix_ms,
            )?;
        }
        Ok(config)
    }
    pub(super) fn verify_identity(&self) -> Result<(), String> {
        let (keypair, _) =
            super::super::node_keypair_config::read_existing_node_keypair_in_secure_config_dir(
                &self.identity.data_dir.join("keys"),
            )?;
        let derived = super::super::derive_node_libp2p_identity_keypair_config(
            &self.identity.node_id,
            &keypair,
        )?;
        let peer = oasis7_node::derive_libp2p_identity_keypair(&derived.private_key_hex)
            .map_err(|e| format!("{e:?}"))?
            .public()
            .to_peer_id()
            .to_string();
        if keypair.public_key_hex != self.identity.root_public_key || peer != self.identity.peer_id
        {
            return Err("existing local key/peer identity changed".into());
        }
        Ok(())
    }
    pub(super) fn runtime_args(&self) -> Result<Vec<String>, String> {
        self.verify_identity()?;
        self.identity.chain.verify()?;
        let args = self.arguments()?;
        // Reuse all accepted runtime validation (including artifact identity)
        // before spawning; no directory/key/world creation by this parse call.
        super::super::parse_options(args.iter().map(String::as_str))?;
        Ok(args)
    }
    pub(super) fn arguments(&self) -> Result<Vec<String>, String> {
        let chain = self.identity.chain.compatibility()?;
        let data = &self.identity.data_dir;
        let state = data.join("state");
        let mut args = vec![
            "--node-id".into(),
            self.identity.node_id.clone(),
            "--world-id".into(),
            chain.world_id.clone(),
            "--node-role".into(),
            "observer".into(),
            "--storage-profile".into(),
            "release_default".into(),
            "--status-bind".into(),
            self.rpc_bind.clone(),
            "--node-no-auto-attest-all".into(),
            "--reward-runtime-disable".into(),
            "--config".into(),
            data.join("keys/node-keypair.toml").display().to_string(),
            "--network-tier-manifest".into(),
            chain.manifest.path.display().to_string(),
            "--genesis-validator-registry".into(),
            chain.registry.path.display().to_string(),
        ];
        for (flag, path) in [
            ("--runtime-root", state.join("runtime")),
            ("--execution-world-dir", state.join("world")),
            ("--execution-records-dir", state.join("records")),
            ("--storage-root", state.join("cas")),
            ("--replication-root", state.join("replication")),
        ] {
            args.push(flag.into());
            args.push(path.display().to_string());
        }
        for peer in &self.additional_peers {
            args.push("--replication-network-peer".into());
            args.push(peer.clone());
        }
        Ok(args)
    }
    pub(super) fn paths(&self) -> serde_json::Value {
        let data = &self.identity.data_dir;
        let artifact_refs = self
            .identity
            .chain
            .compatibility()
            .ok()
            .and_then(|chain| {
                io::read::<serde_json::Value>(&chain.release_bundle.path)
                    .ok()
                    .map(|bundle| {
                        serde_json::json!({"generated_world_sidecar":bundle["generated_world_sidecar"]["resolved_path"],"world_generation_provenance":bundle["world_generation_provenance"]["resolved_path"]})
                    })
            });
        serde_json::json!({"actions":data.join("actions"),"management_request_bindings":data.join("management-requests"),"external_chain_artifact_refs":artifact_refs,"config":data.join(CONFIG_FILE),"identity":data.join("keys/node-keypair.toml"),"world":data.join("state/world"),"records":data.join("state/records"),"cas":data.join("state/cas"),"runtime":data.join("state/runtime"),"replication":data.join("state/replication"),"formal_full_sync_cas":data.join("state/full-sync/blobs"),"formal_full_sync_history_head":data.join("state/full-sync/formal-full-sync-history-head.json"),"external_chain_materials":self.identity.chain,"logs":data.join("logs")})
    }
}

fn resolve_ref(manifest: &Path, reference: &str) -> PathBuf {
    manifest.parent().unwrap_or(Path::new(".")).join(reference)
}
