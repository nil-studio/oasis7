use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt;
use std::sync::Arc;
mod canonical_cbor;
mod capability_v2;
mod economy;
mod recipe_substitution;
pub use canonical_cbor::encode_canonical_cbor;
pub use capability_v2::*;
pub use economy::*;
pub use recipe_substitution::*;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModuleKind {
    Reducer,
    Pure,
}
impl ModuleKind {
    pub fn entrypoint(&self) -> &'static str {
        match self {
            ModuleKind::Reducer => "reduce",
            ModuleKind::Pure => "call",
        }
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ModuleLimits {
    #[serde(default)]
    pub max_mem_bytes: u64,
    #[serde(default)]
    pub max_gas: u64,
    #[serde(default)]
    pub max_call_rate: u32,
    #[serde(default)]
    pub max_output_bytes: u64,
    #[serde(default)]
    pub max_effects: u32,
    #[serde(default)]
    pub max_emits: u32,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleArtifact {
    pub wasm_hash: String,
    pub bytes: Arc<[u8]>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ModuleArtifactIdentity {
    pub source_hash: String,
    pub build_manifest_hash: String,
    pub signer_node_id: String,
    pub signature_scheme: String,
    pub artifact_signature: String,
}
impl ModuleArtifactIdentity {
    pub const SIGNATURE_SCHEME_ED25519: &'static str = "ed25519";
    pub const SIGNATURE_PREFIX_ED25519_V1: &'static str = "modsig:ed25519:v1:";
    pub fn is_complete(&self) -> bool {
        !self.source_hash.trim().is_empty()
            && !self.build_manifest_hash.trim().is_empty()
            && !self.signer_node_id.trim().is_empty()
            && !self.signature_scheme.trim().is_empty()
            && !self.artifact_signature.trim().is_empty()
    }
    pub fn signing_payload_v1(
        wasm_hash: &str,
        source_hash: &str,
        build_manifest_hash: &str,
        signer_node_id: &str,
    ) -> Vec<u8> {
        format!(
            "modsig:ed25519:v1|{wasm_hash}|{source_hash}|{build_manifest_hash}|{signer_node_id}"
        )
        .into_bytes()
    }
    pub fn expected_signature_prefix(&self) -> Option<&'static str> {
        match self.signature_scheme.as_str() {
            Self::SIGNATURE_SCHEME_ED25519 => Some(Self::SIGNATURE_PREFIX_ED25519_V1),
            _ => None,
        }
    }
    pub fn has_unsigned_prefix(&self) -> bool {
        self.artifact_signature.starts_with("unsigned:")
    }
}
#[derive(Debug, Clone)]
pub struct BoundedLruCache<V> {
    capacity: usize,
    cache: BTreeMap<String, V>,
    recent_by_key: BTreeMap<String, u128>,
    keys_by_recent: BTreeMap<u128, String>,
    next_recent: u128,
}
impl<V: PartialEq> PartialEq for BoundedLruCache<V> {
    fn eq(&self, other: &Self) -> bool {
        self.capacity == other.capacity
            && self.cache == other.cache
            && self.lru_keys().eq(other.lru_keys())
    }
}
impl<V: Eq> Eq for BoundedLruCache<V> {}
impl<V> BoundedLruCache<V> {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            cache: BTreeMap::new(),
            recent_by_key: BTreeMap::new(),
            keys_by_recent: BTreeMap::new(),
            next_recent: 0,
        }
    }
    pub fn capacity(&self) -> usize {
        self.capacity
    }
    pub fn len(&self) -> usize {
        self.cache.len()
    }
    pub fn is_empty(&self) -> bool {
        self.cache.is_empty()
    }
    pub fn set_capacity(&mut self, capacity: usize) {
        self.capacity = capacity;
        self.prune();
    }
    pub fn insert(&mut self, key: String, value: V) {
        self.cache.insert(key.clone(), value);
        self.touch(&key);
        self.prune();
    }
    pub fn lru_keys(&self) -> impl Iterator<Item = &str> {
        self.keys_by_recent.values().map(String::as_str)
    }
    fn touch(&mut self, key: &str) {
        if !self.cache.contains_key(key) {
            return;
        }
        let recent = self.next_recent;
        self.next_recent = self.next_recent.saturating_add(1);
        if let Some(previous_recent) = self.recent_by_key.insert(key.to_string(), recent) {
            self.keys_by_recent.remove(&previous_recent);
        }
        self.keys_by_recent.insert(recent, key.to_string());
    }
    fn prune(&mut self) {
        if self.capacity == 0 {
            self.cache.clear();
            self.recent_by_key.clear();
            self.keys_by_recent.clear();
            return;
        }
        while self.cache.len() > self.capacity {
            if let Some((recent, evicted)) = self.keys_by_recent.pop_first() {
                self.cache.remove(&evicted);
                self.recent_by_key.remove(&evicted);
                debug_assert!(!self.keys_by_recent.contains_key(&recent));
            } else {
                break;
            }
        }
    }
    pub fn get_cloned(&mut self, key: &str) -> Option<V>
    where
        V: Clone,
    {
        let value = self.cache.get(key)?.clone();
        self.touch(key);
        Some(value)
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundedFifoCache<V> {
    capacity: usize,
    cache: BTreeMap<String, V>,
    insertion_order: VecDeque<String>,
}
impl<V> BoundedFifoCache<V> {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            cache: BTreeMap::new(),
            insertion_order: VecDeque::new(),
        }
    }
    pub fn len(&self) -> usize {
        self.cache.len()
    }
    pub fn is_empty(&self) -> bool {
        self.cache.is_empty()
    }
    pub fn insert(&mut self, key: String, value: V) {
        if self.capacity == 0 {
            self.cache.clear();
            self.insertion_order.clear();
            return;
        }
        if let Some(entry) = self.cache.get_mut(&key) {
            *entry = value;
            return;
        }
        while self.cache.len() >= self.capacity {
            if let Some(evicted) = self.insertion_order.pop_front() {
                self.cache.remove(&evicted);
            } else {
                break;
            }
        }

        self.insertion_order.push_back(key.clone());
        self.cache.insert(key, value);
    }

    pub fn get_cloned(&self, key: &str) -> Option<V>
    where
        V: Clone,
    {
        self.cache.get(key).cloned()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleCache {
    artifacts: BoundedLruCache<ModuleArtifact>,
}

impl ModuleCache {
    pub fn new(max_cached_modules: usize) -> Self {
        Self {
            artifacts: BoundedLruCache::new(max_cached_modules),
        }
    }

    pub fn max_cached_modules(&self) -> usize {
        self.artifacts.capacity()
    }

    pub fn len(&self) -> usize {
        self.artifacts.len()
    }

    pub fn is_empty(&self) -> bool {
        self.artifacts.is_empty()
    }

    pub fn set_max_cached_modules(&mut self, max_cached_modules: usize) {
        self.artifacts.set_capacity(max_cached_modules);
    }

    pub fn get(&mut self, wasm_hash: &str) -> Option<ModuleArtifact> {
        self.artifacts.get_cloned(wasm_hash)
    }

    pub fn insert(&mut self, artifact: ModuleArtifact) {
        let key = artifact.wasm_hash.clone();
        self.artifacts.insert(key, artifact);
    }
}

impl Default for ModuleCache {
    fn default() -> Self {
        Self::new(8)
    }
}

impl ModuleLimits {
    pub fn unbounded() -> Self {
        Self {
            max_mem_bytes: u64::MAX,
            max_gas: u64::MAX,
            max_call_rate: u32::MAX,
            max_output_bytes: u64::MAX,
            max_effects: u32::MAX,
            max_emits: u32::MAX,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModuleSubscription {
    #[serde(default)]
    pub event_kinds: Vec<String>,
    #[serde(default)]
    pub action_kinds: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stage: Option<ModuleSubscriptionStage>,
    #[serde(default)]
    pub filters: Option<JsonValue>,
}

impl ModuleSubscription {
    pub fn resolved_stage(&self) -> ModuleSubscriptionStage {
        self.stage.unwrap_or(if !self.event_kinds.is_empty() {
            ModuleSubscriptionStage::PostEvent
        } else {
            ModuleSubscriptionStage::PreAction
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ModuleSubscriptionStage {
    PreAction,
    PostAction,
    #[default]
    PostEvent,
    Tick,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ModuleRole {
    Rule,
    #[default]
    Domain,
    Gameplay,
    Body,
    AgentInternal,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum GameplayModuleKind {
    #[default]
    War,
    Governance,
    Crisis,
    Economic,
    Meta,
}

impl GameplayModuleKind {
    pub fn as_str(self) -> &'static str {
        match self {
            GameplayModuleKind::War => "war",
            GameplayModuleKind::Governance => "governance",
            GameplayModuleKind::Crisis => "crisis",
            GameplayModuleKind::Economic => "economic",
            GameplayModuleKind::Meta => "meta",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameplayContract {
    pub kind: GameplayModuleKind,
    #[serde(default)]
    pub game_modes: Vec<String>,
    #[serde(default = "default_gameplay_min_players")]
    pub min_players: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_players: Option<u16>,
}

fn default_gameplay_min_players() -> u16 {
    1
}

impl Default for GameplayContract {
    fn default() -> Self {
        Self {
            kind: GameplayModuleKind::War,
            game_modes: Vec::new(),
            min_players: default_gameplay_min_players(),
            max_players: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModuleManifest {
    pub module_id: String,
    pub name: String,
    pub version: String,
    pub kind: ModuleKind,
    #[serde(default)]
    pub role: ModuleRole,
    pub wasm_hash: String,
    pub interface_version: String,
    #[serde(default)]
    pub exports: Vec<String>,
    #[serde(default)]
    pub subscriptions: Vec<ModuleSubscription>,
    #[serde(default)]
    pub required_caps: Vec<String>,
    #[serde(default)]
    pub abi_contract: ModuleAbiContract,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_identity: Option<ModuleArtifactIdentity>,
    #[serde(default)]
    pub limits: ModuleLimits,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ModuleAbiContract {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub abi_version: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_schema: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_schema: Option<String>,
    #[serde(default)]
    pub cap_slots: BTreeMap<String, String>,
    #[serde(default)]
    pub policy_hooks: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gameplay: Option<GameplayContract>,
    #[serde(default, skip_serializing_if = "ModuleSchemaDeclarations::is_empty")]
    pub declarations: ModuleSchemaDeclarations,
}

/// The largest command payload the ABI admits before a module is invoked.
pub const MAX_MODULE_COMMAND_PAYLOAD_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ModuleSchemaDeclarations {
    #[serde(default)]
    pub commands: Vec<ModuleCommandDeclaration>,
}

impl ModuleSchemaDeclarations {
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }
}

/// Agent-facing projection of a command declared by an active module.
///
/// This is a declaration DTO rather than a capability grant: consumers may
/// use it for schema discovery, but it does not authorize invocation.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ModuleCommandCatalogEntry {
    pub module_id: String,
    pub module_version: String,
    pub namespace: String,
    pub name: String,
    pub schema_version: u32,
    pub schema_hash: String,
    pub max_payload_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModuleCommandDeclaration {
    pub namespace: String,
    pub name: String,
    pub schema_version: u32,
    pub schema_hash: String,
    pub max_payload_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModuleCommandEnvelope {
    pub namespace: String,
    pub name: String,
    pub schema_version: u32,
    pub schema_hash: String,
    pub payload: Vec<u8>,
}

impl ModuleCommandEnvelope {
    /// Encode the envelope using RFC 8949 deterministic CBOR map ordering.
    pub fn encode_canonical(&self) -> Result<Vec<u8>, ModuleCommandValidationError> {
        encode_canonical_cbor(self)
    }

    /// Decode only canonical CBOR: decoding and re-encoding must be byte exact.
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, ModuleCommandValidationError> {
        let envelope: Self = ciborium::de::from_reader(bytes)
            .map_err(|error| ModuleCommandValidationError::CanonicalDecoding(error.to_string()))?;
        let canonical = envelope.encode_canonical()?;
        if canonical != bytes {
            return Err(ModuleCommandValidationError::NonCanonicalEncoding);
        }
        Ok(envelope)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModuleCommandValidationError {
    InvalidNamespace(String),
    ReservedNamespace(String),
    InvalidName(String),
    InvalidSchemaVersion,
    InvalidSchemaHash(String),
    InvalidPayloadBound(u64),
    InvalidPayloadSize(usize),
    DuplicateDeclaration {
        namespace: String,
        name: String,
        schema_version: u32,
    },
    UnknownDeclaration {
        namespace: String,
        name: String,
        schema_version: u32,
    },
    SchemaHashMismatch,
    PayloadExceedsDeclaration {
        actual: usize,
        max: u64,
    },
    CanonicalEncoding(String),
    CanonicalDecoding(String),
    NonCanonicalEncoding,
}

impl fmt::Display for ModuleCommandValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidNamespace(value) => {
                write!(formatter, "invalid command namespace: {value}")
            }
            Self::ReservedNamespace(value) => {
                write!(formatter, "reserved command namespace: {value}")
            }
            Self::InvalidName(value) => write!(formatter, "invalid command name: {value}"),
            Self::InvalidSchemaVersion => {
                formatter.write_str("schema version must be greater than zero")
            }
            Self::InvalidSchemaHash(value) => write!(
                formatter,
                "schema hash must be lowercase SHA-256 hex: {value}"
            ),
            Self::InvalidPayloadBound(value) => write!(
                formatter,
                "payload bound is outside 1..={MAX_MODULE_COMMAND_PAYLOAD_BYTES}: {value}"
            ),
            Self::InvalidPayloadSize(value) => write!(
                formatter,
                "payload size is outside 1..={MAX_MODULE_COMMAND_PAYLOAD_BYTES}: {value}"
            ),
            Self::DuplicateDeclaration {
                namespace,
                name,
                schema_version,
            } => write!(
                formatter,
                "duplicate declaration {namespace}:{name}@{schema_version}"
            ),
            Self::UnknownDeclaration {
                namespace,
                name,
                schema_version,
            } => write!(
                formatter,
                "unknown declaration {namespace}:{name}@{schema_version}"
            ),
            Self::SchemaHashMismatch => {
                formatter.write_str("schema hash does not match declaration")
            }
            Self::PayloadExceedsDeclaration { actual, max } => write!(
                formatter,
                "payload size {actual} exceeds declaration bound {max}"
            ),
            Self::CanonicalEncoding(error) => {
                write!(formatter, "canonical CBOR encoding failed: {error}")
            }
            Self::CanonicalDecoding(error) => {
                write!(formatter, "canonical CBOR decoding failed: {error}")
            }
            Self::NonCanonicalEncoding => formatter.write_str("CBOR encoding is not canonical"),
        }
    }
}

impl std::error::Error for ModuleCommandValidationError {}

pub fn validate_module_command_declarations(
    declarations: &ModuleSchemaDeclarations,
) -> Result<(), ModuleCommandValidationError> {
    let mut seen = BTreeSet::new();
    for declaration in &declarations.commands {
        validate_namespace(&declaration.namespace)?;
        validate_name(&declaration.name)?;
        if declaration.schema_version == 0 {
            return Err(ModuleCommandValidationError::InvalidSchemaVersion);
        }
        validate_schema_hash(&declaration.schema_hash)?;
        if declaration.max_payload_bytes == 0
            || declaration.max_payload_bytes > MAX_MODULE_COMMAND_PAYLOAD_BYTES as u64
        {
            return Err(ModuleCommandValidationError::InvalidPayloadBound(
                declaration.max_payload_bytes,
            ));
        }
        let key = (
            declaration.namespace.as_str(),
            declaration.name.as_str(),
            declaration.schema_version,
        );
        if !seen.insert(key) {
            return Err(ModuleCommandValidationError::DuplicateDeclaration {
                namespace: declaration.namespace.clone(),
                name: declaration.name.clone(),
                schema_version: declaration.schema_version,
            });
        }
    }
    Ok(())
}

pub fn validate_module_command_envelope(
    envelope: &ModuleCommandEnvelope,
    declarations: &ModuleSchemaDeclarations,
) -> Result<(), ModuleCommandValidationError> {
    validate_module_command_declarations(declarations)?;
    validate_namespace(&envelope.namespace)?;
    validate_name(&envelope.name)?;
    if envelope.schema_version == 0 {
        return Err(ModuleCommandValidationError::InvalidSchemaVersion);
    }
    validate_schema_hash(&envelope.schema_hash)?;
    if envelope.payload.is_empty() {
        return Err(ModuleCommandValidationError::InvalidPayloadSize(0));
    }
    if envelope.payload.len() > MAX_MODULE_COMMAND_PAYLOAD_BYTES {
        return Err(ModuleCommandValidationError::InvalidPayloadSize(
            envelope.payload.len(),
        ));
    }

    let declaration = declarations.commands.iter().find(|declaration| {
        declaration.namespace == envelope.namespace
            && declaration.name == envelope.name
            && declaration.schema_version == envelope.schema_version
    });
    let declaration =
        declaration.ok_or_else(|| ModuleCommandValidationError::UnknownDeclaration {
            namespace: envelope.namespace.clone(),
            name: envelope.name.clone(),
            schema_version: envelope.schema_version,
        })?;
    if declaration.schema_hash != envelope.schema_hash {
        return Err(ModuleCommandValidationError::SchemaHashMismatch);
    }
    if envelope.payload.len() > declaration.max_payload_bytes as usize {
        return Err(ModuleCommandValidationError::PayloadExceedsDeclaration {
            actual: envelope.payload.len(),
            max: declaration.max_payload_bytes,
        });
    }
    Ok(())
}

fn validate_namespace(namespace: &str) -> Result<(), ModuleCommandValidationError> {
    if namespace == "core"
        || namespace.starts_with("core.")
        || namespace == "kernel"
        || namespace.starts_with("kernel.")
    {
        return Err(ModuleCommandValidationError::ReservedNamespace(
            namespace.to_string(),
        ));
    }
    if namespace.is_empty()
        || namespace
            .split('.')
            .any(|segment| !is_local_identifier(segment))
    {
        return Err(ModuleCommandValidationError::InvalidNamespace(
            namespace.to_string(),
        ));
    }
    Ok(())
}

fn validate_name(name: &str) -> Result<(), ModuleCommandValidationError> {
    if !is_local_identifier(name) {
        return Err(ModuleCommandValidationError::InvalidName(name.to_string()));
    }
    Ok(())
}

fn is_local_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(chars.next(), Some(character) if character.is_ascii_lowercase())
        && chars.all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '_'
        })
}

fn validate_schema_hash(hash: &str) -> Result<(), ModuleCommandValidationError> {
    if hash.len() != 64
        || !hash
            .bytes()
            .all(|character| character.is_ascii_digit() || (b'a'..=b'f').contains(&character))
    {
        return Err(ModuleCommandValidationError::InvalidSchemaHash(
            hash.to_string(),
        ));
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ModuleChangeSet {
    #[serde(default)]
    pub register: Vec<ModuleManifest>,
    #[serde(default)]
    pub activate: Vec<ModuleActivation>,
    #[serde(default)]
    pub deactivate: Vec<ModuleDeactivation>,
    #[serde(default)]
    pub upgrade: Vec<ModuleUpgrade>,
}

impl ModuleChangeSet {
    pub fn is_empty(&self) -> bool {
        self.register.is_empty()
            && self.activate.is_empty()
            && self.deactivate.is_empty()
            && self.upgrade.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModuleActivation {
    pub module_id: String,
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModuleDeactivation {
    pub module_id: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModuleUpgrade {
    pub module_id: String,
    pub from_version: String,
    pub to_version: String,
    pub wasm_hash: String,
    pub manifest: ModuleManifest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ModuleRegistry {
    pub records: BTreeMap<String, ModuleRecord>,
    pub active: BTreeMap<String, String>,
}

impl ModuleRegistry {
    pub fn record_key(module_id: &str, version: &str) -> String {
        format!("{module_id}@{version}")
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModuleRecord {
    pub manifest: ModuleManifest,
    pub registered_at: u64,
    pub registered_by: String,
    pub audit_event_id: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModuleEvent {
    pub proposal_id: u64,
    #[serde(flatten)]
    pub kind: ModuleEventKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum ModuleEventKind {
    RegisterModule {
        module: ModuleManifest,
        registered_by: String,
    },
    ActivateModule {
        module_id: String,
        version: String,
        activated_by: String,
    },
    DeactivateModule {
        module_id: String,
        reason: String,
        deactivated_by: String,
    },
    UpgradeModule {
        module_id: String,
        from_version: String,
        to_version: String,
        wasm_hash: String,
        manifest: ModuleManifest,
        upgraded_by: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModuleEffectIntent {
    pub kind: String,
    pub params: JsonValue,
    pub cap_ref: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cap_slot: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModuleEmit {
    pub kind: String,
    pub payload: JsonValue,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModuleOutput {
    pub new_state: Option<Vec<u8>>,
    #[serde(default)]
    pub effects: Vec<ModuleEffectIntent>,
    #[serde(default)]
    pub emits: Vec<ModuleEmit>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tick_lifecycle: Option<ModuleTickLifecycleDirective>,
    pub output_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum ModuleTickLifecycleDirective {
    WakeAfterTicks { ticks: u64 },
    Suspend,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModuleCallRequest {
    pub module_id: String,
    pub wasm_hash: String,
    pub trace_id: String,
    pub entrypoint: String,
    pub input: Vec<u8>,
    pub limits: ModuleLimits,
    #[serde(default)]
    pub wasm_bytes: Arc<[u8]>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModuleCallOrigin {
    pub kind: String,
    pub id: String,
}

/// The trusted host-side identity that initiated a module invocation.
///
/// Caller provenance is deliberately carried by [`ModuleContext`] rather than
/// [`ModuleCommandEnvelope`].  The envelope is untrusted command data and may
/// be supplied by an Agent or another module; the host must inject this value
/// at the invocation boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ModuleCallCaller {
    /// An invocation produced through a legacy host path that has no caller
    /// identity yet.  This is the serde default for pre-provenance inputs.
    #[default]
    LegacyUnspecified,
    System {
        system_id: String,
    },
    Agent {
        agent_id: String,
    },
    Module {
        module_id: String,
    },
}

/// Trusted host-side provenance for a module invocation.
///
/// This type is not part of [`ModuleCommandEnvelope`]: command payloads must
/// not be able to self-assert their caller or origin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModuleInvocationProvenance {
    pub caller: ModuleCallCaller,
    pub origin: ModuleCallOrigin,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModuleContext {
    pub v: String,
    pub module_id: String,
    pub trace_id: String,
    pub time: u64,
    pub origin: ModuleCallOrigin,
    /// Host-injected caller identity.  Missing on legacy serialized contexts
    /// decodes as `LegacyUnspecified` for wire compatibility.
    #[serde(default)]
    pub caller: ModuleCallCaller,
    pub limits: ModuleLimits,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stage: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub world_config_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manifest_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub journal_height: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub module_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub module_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub module_role: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModuleCallInput {
    pub ctx: ModuleContext,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event: Option<Vec<u8>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<Vec<u8>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModuleCallErrorCode {
    Trap,
    Timeout,
    OutOfFuel,
    Interrupted,
    OutputTooLarge,
    EffectLimitExceeded,
    EmitLimitExceeded,
    CapsDenied,
    PolicyDenied,
    SandboxUnavailable,
    InvalidOutput,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModuleCallFailure {
    pub module_id: String,
    pub trace_id: String,
    pub code: ModuleCallErrorCode,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModuleEmitEvent {
    pub module_id: String,
    pub trace_id: String,
    pub kind: String,
    pub payload: JsonValue,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModuleStateUpdate {
    pub module_id: String,
    pub trace_id: String,
    pub state: Vec<u8>,
}

pub trait ModuleSandbox {
    fn call(&mut self, request: &ModuleCallRequest) -> Result<ModuleOutput, ModuleCallFailure>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    fn artifact(hash: &str, byte: u8) -> ModuleArtifact {
        ModuleArtifact {
            wasm_hash: hash.to_string(),
            bytes: Arc::<[u8]>::from(vec![byte]),
        }
    }

    #[test]
    fn bounded_lru_cache_refreshes_hits_without_duplicate_keys() {
        let mut cache = BoundedLruCache::new(2);
        cache.insert("a".to_string(), 1u8);
        cache.insert("b".to_string(), 2u8);

        assert_eq!(cache.get_cloned("a"), Some(1));
        assert_eq!(cache.get_cloned("a"), Some(1));
        assert_eq!(cache.lru_keys().collect::<Vec<_>>(), vec!["b", "a"]);

        cache.insert("c".to_string(), 3u8);
        assert_eq!(cache.get_cloned("b"), None);
        assert_eq!(cache.get_cloned("a"), Some(1));
        assert_eq!(cache.get_cloned("c"), Some(3));
    }

    #[test]
    fn bounded_lru_cache_zero_capacity_stays_empty() {
        let mut cache = BoundedLruCache::new(0);
        cache.insert("a".to_string(), 1u8);
        assert!(cache.is_empty());
        assert_eq!(cache.get_cloned("a"), None);
        assert_eq!(cache.lru_keys().collect::<Vec<_>>(), Vec::<&str>::new());
    }

    #[test]
    fn bounded_fifo_cache_preserves_insertion_order_on_update() {
        let mut cache = BoundedFifoCache::new(2);
        cache.insert("a".to_string(), 1u8);
        cache.insert("b".to_string(), 2u8);
        cache.insert("a".to_string(), 10u8);
        cache.insert("c".to_string(), 3u8);

        assert_eq!(cache.get_cloned("a"), None);
        assert_eq!(cache.get_cloned("b"), Some(2));
        assert_eq!(cache.get_cloned("c"), Some(3));
    }

    #[test]
    fn bounded_fifo_cache_zero_capacity_stays_empty() {
        let mut cache = BoundedFifoCache::new(0);
        cache.insert("a".to_string(), 1u8);
        assert!(cache.is_empty());
        assert_eq!(cache.get_cloned("a"), None);
    }

    #[test]
    fn module_cache_evicts_lru_entry() {
        let mut cache = ModuleCache::new(2);
        cache.insert(artifact("a", 1));
        cache.insert(artifact("b", 2));
        let _ = cache.get("a");
        cache.insert(artifact("c", 3));

        assert!(cache.get("a").is_some());
        assert!(cache.get("b").is_none());
        assert!(cache.get("c").is_some());
    }

    #[test]
    fn module_cache_zero_capacity_stays_empty() {
        let mut cache = ModuleCache::new(0);
        cache.insert(artifact("a", 1));
        assert_eq!(cache.len(), 0);
        assert!(cache.get("a").is_none());
    }

    #[test]
    fn module_cache_set_max_prunes_existing_entries() {
        let mut cache = ModuleCache::new(3);
        cache.insert(artifact("a", 1));
        cache.insert(artifact("b", 2));
        cache.insert(artifact("c", 3));
        cache.set_max_cached_modules(1);

        assert_eq!(cache.len(), 1);
        assert!(cache.get("c").is_some());
        assert!(cache.get("a").is_none());
        assert!(cache.get("b").is_none());
    }

    #[test]
    fn module_cache_repeated_hot_hit_keeps_single_recent_lru_entry() {
        let mut cache = ModuleCache::new(2);
        cache.insert(artifact("a", 1));
        cache.insert(artifact("b", 2));

        assert!(cache.get("b").is_some());
        assert!(cache.get("b").is_some());
        assert_eq!(
            cache.artifacts.lru_keys().collect::<Vec<_>>(),
            vec!["a", "b"]
        );

        cache.insert(artifact("c", 3));
        assert!(cache.get("a").is_none());
        assert!(cache.get("b").is_some());
        assert!(cache.get("c").is_some());
    }

    #[test]
    fn bounded_lru_cache_equality_ignores_internal_recency_clock() {
        let mut baseline = BoundedLruCache::new(2);
        baseline.insert("a".to_string(), 1u8);
        baseline.insert("b".to_string(), 2u8);

        let mut refreshed_mru = BoundedLruCache::new(2);
        refreshed_mru.insert("a".to_string(), 1u8);
        refreshed_mru.insert("b".to_string(), 2u8);
        assert_eq!(refreshed_mru.get_cloned("b"), Some(2));

        assert_eq!(baseline, refreshed_mru);
    }

    #[test]
    fn module_change_set_is_empty_checks_all_sections() {
        let mut changes = ModuleChangeSet::default();
        assert!(changes.is_empty());

        changes.activate.push(ModuleActivation {
            module_id: "m.test".to_string(),
            version: "v1".to_string(),
        });
        assert!(!changes.is_empty());
    }

    #[test]
    fn module_registry_record_key_uses_module_and_version() {
        assert_eq!(
            ModuleRegistry::record_key("m.rule", "1.2.3"),
            "m.rule@1.2.3"
        );
    }

    #[test]
    fn module_event_kind_serialization_keeps_tag_format() {
        let kind = ModuleEventKind::DeactivateModule {
            module_id: "m.rule".to_string(),
            reason: "manual".to_string(),
            deactivated_by: "tester".to_string(),
        };

        let json = serde_json::to_value(&kind).expect("serialize module event kind");
        assert_eq!(json["type"], "DeactivateModule");
        assert_eq!(json["data"]["module_id"], "m.rule");
        assert_eq!(json["data"]["reason"], "manual");
        assert_eq!(json["data"]["deactivated_by"], "tester");
    }

    #[test]
    fn module_artifact_identity_payload_and_prefix() {
        let identity = ModuleArtifactIdentity {
            source_hash: "src-1".to_string(),
            build_manifest_hash: "build-1".to_string(),
            signer_node_id: "node-1".to_string(),
            signature_scheme: ModuleArtifactIdentity::SIGNATURE_SCHEME_ED25519.to_string(),
            artifact_signature: format!(
                "{}{}",
                ModuleArtifactIdentity::SIGNATURE_PREFIX_ED25519_V1,
                "abcd"
            ),
        };
        assert!(identity.is_complete());
        assert_eq!(
            identity.expected_signature_prefix(),
            Some(ModuleArtifactIdentity::SIGNATURE_PREFIX_ED25519_V1)
        );
        assert_eq!(
            ModuleArtifactIdentity::signing_payload_v1("hash-1", "src-1", "build-1", "node-1"),
            b"modsig:ed25519:v1|hash-1|src-1|build-1|node-1".to_vec()
        );
        assert!(!identity.has_unsigned_prefix());
    }

    #[test]
    #[ignore = "local perf probe"]
    fn perf_probe_module_cache_clone_cost_scales_with_wasm_size() {
        let sizes = [4 * 1024usize, 256 * 1024usize, 4 * 1024 * 1024usize];

        for size in sizes {
            let mut cache = ModuleCache::new(1);
            let key = format!("hash-{size}");
            cache.insert(ModuleArtifact {
                wasm_hash: key.clone(),
                bytes: vec![7_u8; size].into(),
            });
            let iterations = match size {
                0..=16_384 => 200_000u32,
                16_385..=1_048_576 => 20_000u32,
                _ => 2_000u32,
            };

            let started = Instant::now();
            let mut bytes_observed = 0usize;
            for _ in 0..iterations {
                let artifact = cache.get(&key).expect("cache hit");
                bytes_observed = bytes_observed.saturating_add(artifact.bytes.len());
            }
            let elapsed = started.elapsed();
            let avg_us = elapsed.as_secs_f64() * 1_000_000.0 / f64::from(iterations);
            let throughput_mib_s = if elapsed.as_secs_f64() == 0.0 {
                0.0
            } else {
                (bytes_observed as f64 / (1024.0 * 1024.0)) / elapsed.as_secs_f64()
            };
            eprintln!(
                "perf_probe_module_cache_clone_cost_scales_with_wasm_size: size_bytes={size} iterations={iterations} total_ms={:.3} avg_us_per_get={:.3} throughput_mib_s={:.2}",
                elapsed.as_secs_f64() * 1_000.0,
                avg_us,
                throughput_mib_s
            );
        }
    }
}
