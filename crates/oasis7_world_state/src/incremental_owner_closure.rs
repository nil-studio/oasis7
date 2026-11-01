//! Fail-closed WorldState owner coverage for profile-3 endpoint verification.
//!
//! Generic state/value verification proves the six trees and all referenced
//! value bytes. This layer binds structural keys to the frozen WorldState
//! registry and verifies the currently implemented scalar, set, number-map,
//! text/byte-map, installed-target, world-balance, and module-instance owner
//! families.
//! Other WorldState, Runtime, and Cognition owners remain explicit gaps; this
//! report is never a receipt or an activation token.

use crate::{
    incremental_field_member::{
        MemberPayloadCodecV1, MemberReadProfileV1, MemberRecordLimitsV1, WorldFieldKeyV1,
        WorldFieldRecordKindV1, read_member_record_profile_v1, read_member_record_v1,
    },
    incremental_field_presence::{PresenceRecordLimitsV1, read_field_presence_v1},
    incremental_ledger_value::Utf8Output,
    incremental_projection::{
        FieldDescriptor, FieldDestination, FieldOwner, FieldShape, WORLD_STATE_FIELDS,
    },
    incremental_semantic_key::{SEMANTIC_KEY_DOMAIN, SemanticKeyRefV1, read_semantic_key_v1},
    incremental_world_id_sets::WorldIdSetFieldV1,
    incremental_world_number_maps::WorldNumberMapFieldV1,
    incremental_world_u64::WorldU64FieldV1,
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionError, RootSetV1},
    incremental_state::{
        MerkleNodeReader, STATE_ROOT_BYTES, StateNamespaceV1, StateScanBudget, StateScanContext,
        StateScanError,
    },
    incremental_value::{ValueNodeReader, ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::ReservedValue;
use std::{cell::RefCell, io, io::Write};

#[path = "incremental_owner_closure_module_instances.rs"]
mod module_instances;
#[path = "incremental_owner_closure_module_marketplace.rs"]
mod module_marketplace;
#[path = "incremental_owner_closure_record_families.rs"]
mod record_families;
#[path = "incremental_owner_closure_typed_maps.rs"]
mod typed_maps;

const OWNER_SCAN_METADATA_BYTES: usize = 4096;
const U64_SEMANTIC_BYTES: u64 = (SEMANTIC_KEY_DOMAIN.len() + 1) as u64;
const ROOT_SET_CANONICAL_MAX_BYTES: usize =
    b"oasis7.iws.root-set.v1\0".len() + 4 + 2 + 256 + 48 + 6 * STATE_ROOT_BYTES;

#[derive(Debug, Clone, Copy, Default)]
struct SeenField {
    metadata: u8,
    entries: u8,
}

/// Bounded diagnostics for a state whose generic tree/value closure passed but
/// whose typed owner inventory is not yet complete. Keeping the fields charged
/// prevents the diagnostic from escaping the endpoint's shared pool.
#[derive(Debug)]
pub struct UnsupportedWorldStateOwnersV1 {
    root_set_digest: [u8; 32],
    unsupported_fields: ReservedValue<Vec<&'static str>>,
    runtime_registry_missing: bool,
    cognition_registry_missing: bool,
    checked_u64_fields: u8,
    checked_id_set_fields: u8,
    checked_number_map_fields: u8,
    checked_text_byte_fields: u8,
    checked_installed_target_fields: u8,
    checked_world_balance_fields: u8,
    checked_module_instance_fields: u8,
    checked_module_marketplace_fields: u8,
    checked_governance_fields: u8,
    checked_factory_authority_fields: u8,
    checked_main_token_policy_fields: u8,
    checked_material_fields: u8,
    checked_logistics_fields: u8,
    checked_fixed_economy_fields: u8,
    checked_agent_authority_fields: u8,
    checked_reward_fields: u8,
}

impl UnsupportedWorldStateOwnersV1 {
    /// Digest of the exact canonical RootSet scanned by this diagnostic.
    /// This does not qualify the roots or their authority.
    pub fn root_set_digest(&self) -> [u8; 32] {
        self.root_set_digest
    }

    pub fn unsupported_fields(&self) -> &[&'static str] {
        self.unsupported_fields.as_slice()
    }

    pub fn runtime_registry_missing(&self) -> bool {
        self.runtime_registry_missing
    }

    pub fn cognition_registry_missing(&self) -> bool {
        self.cognition_registry_missing
    }

    pub fn checked_u64_fields(&self) -> u8 {
        self.checked_u64_fields
    }

    pub fn checked_id_set_fields(&self) -> u8 {
        self.checked_id_set_fields
    }

    pub fn checked_number_map_fields(&self) -> u8 {
        self.checked_number_map_fields
    }

    pub fn checked_text_byte_fields(&self) -> u8 {
        self.checked_text_byte_fields
    }

    pub fn checked_installed_target_fields(&self) -> u8 {
        self.checked_installed_target_fields
    }

    pub fn checked_world_balance_fields(&self) -> u8 {
        self.checked_world_balance_fields
    }

    pub fn checked_module_instance_fields(&self) -> u8 {
        self.checked_module_instance_fields
    }

    pub fn checked_module_marketplace_fields(&self) -> u8 {
        self.checked_module_marketplace_fields
    }

    pub fn checked_governance_fields(&self) -> u8 {
        self.checked_governance_fields
    }

    pub fn checked_factory_authority_fields(&self) -> u8 {
        self.checked_factory_authority_fields
    }

    pub fn checked_main_token_policy_fields(&self) -> u8 {
        self.checked_main_token_policy_fields
    }

    pub fn checked_material_fields(&self) -> u8 {
        self.checked_material_fields
    }

    pub fn checked_logistics_fields(&self) -> u8 {
        self.checked_logistics_fields
    }

    pub fn checked_fixed_economy_fields(&self) -> u8 {
        self.checked_fixed_economy_fields
    }

    pub fn checked_agent_authority_fields(&self) -> u8 {
        self.checked_agent_authority_fields
    }

    pub fn checked_reward_fields(&self) -> u8 {
        self.checked_reward_fields
    }
}

#[derive(Debug)]
pub enum WorldStateOwnerError {
    Scan(String),
    RootSet(ExecutionError),
    Value(ValueTreeError),
    Memory(oasis7_proto::memory_budget::MemoryLimitExceeded),
    ResourceLimited(&'static str),
    Invalid(&'static str),
}
impl std::fmt::Display for WorldStateOwnerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Scan(error) => write!(f, "WorldState owner scan: {error}"),
            Self::RootSet(error) => write!(f, "WorldState owner RootSet: {error}"),
            Self::Value(error) => error.fmt(f),
            Self::Memory(error) => error.fmt(f),
            Self::ResourceLimited(error) => write!(f, "WorldState owner resource limited: {error}"),
            Self::Invalid(error) => write!(f, "WorldState owner closure: {error}"),
        }
    }
}
impl std::error::Error for WorldStateOwnerError {}

fn registered_descriptor(
    owner: FieldOwner,
    namespace: StateNamespaceV1,
    shape: FieldShape,
    name: &str,
) -> Result<Option<(usize, FieldDescriptor)>, WorldStateOwnerError> {
    let mut found = None;
    for (index, field) in WORLD_STATE_FIELDS.iter().copied().enumerate() {
        if field.owner == owner
            && field.name == name
            && field.shape == shape
            && field.destination == FieldDestination::State(namespace)
        {
            if found.is_some() {
                return Err(WorldStateOwnerError::Invalid(
                    "ambiguous WorldState descriptor registration",
                ));
            }
            found = Some((index, field));
        }
    }
    Ok(found)
}

fn u64_field(field: FieldDescriptor) -> Option<WorldU64FieldV1> {
    WorldU64FieldV1::ALL
        .into_iter()
        .find(|candidate| candidate.descriptor().ok() == Some(field))
}

fn id_set_field(field: FieldDescriptor) -> Option<WorldIdSetFieldV1> {
    WorldIdSetFieldV1::ALL
        .into_iter()
        .find(|candidate| candidate.name() == field.name)
}

fn number_map_field(field: FieldDescriptor) -> Option<WorldNumberMapFieldV1> {
    WorldNumberMapFieldV1::ALL
        .into_iter()
        .find(|candidate| candidate.name() == field.name)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TextByteOwnerField {
    ModuleStates,
    ModuleArtifactOwners,
    NodeIdentityBindings,
    NodeMainTokenAccountBindings,
}

impl TextByteOwnerField {
    fn from_descriptor(field: FieldDescriptor) -> Option<Self> {
        match field.name {
            "module_states" => Some(Self::ModuleStates),
            "module_artifact_owners" => Some(Self::ModuleArtifactOwners),
            "node_identity_bindings" => Some(Self::NodeIdentityBindings),
            "node_main_token_account_bindings" => Some(Self::NodeMainTokenAccountBindings),
            _ => None,
        }
    }

    fn descriptor_matches(self, field: FieldDescriptor) -> bool {
        let (name, shape, namespace) = match self {
            Self::ModuleStates => (
                "module_states",
                FieldShape::ByteMap,
                StateNamespaceV1::Business,
            ),
            Self::ModuleArtifactOwners => (
                "module_artifact_owners",
                FieldShape::Map,
                StateNamespaceV1::Business,
            ),
            Self::NodeIdentityBindings => (
                "node_identity_bindings",
                FieldShape::Map,
                StateNamespaceV1::Authorization,
            ),
            Self::NodeMainTokenAccountBindings => (
                "node_main_token_account_bindings",
                FieldShape::Map,
                StateNamespaceV1::Authorization,
            ),
        };
        field.owner == FieldOwner::WorldState
            && field.name == name
            && field.shape == shape
            && field.destination == FieldDestination::State(namespace)
    }

    fn record_kind(self) -> WorldFieldRecordKindV1 {
        if self == Self::ModuleStates {
            WorldFieldRecordKindV1::Bytes
        } else {
            WorldFieldRecordKindV1::Entry
        }
    }

    fn payload_codec(self) -> MemberPayloadCodecV1 {
        if self == Self::ModuleStates {
            MemberPayloadCodecV1::Bytes
        } else {
            MemberPayloadCodecV1::Text
        }
    }

    fn payload_is_text(self) -> bool {
        self != Self::ModuleStates
    }
}

fn text_byte_field(field: FieldDescriptor) -> Option<TextByteOwnerField> {
    TextByteOwnerField::from_descriptor(field)
}

#[derive(Clone, Copy)]
pub(super) enum OwnerKeyKind {
    Text,
    Action,
}

/// Validates closed text/action semantic-key tokens without materializing
/// arbitrarily long original keys. The record reader separately hashes the
/// full stream and binds it to the structural field-key digest.
pub(super) struct OwnerSemanticKeyProbe {
    expected: OwnerKeyKind,
    prefix: [u8; SEMANTIC_KEY_DOMAIN.len() + 9],
    prefix_len: usize,
    expected_total: Option<u64>,
    bytes_seen: u64,
    max_bytes: u64,
    utf8_pending: [u8; 4],
    utf8_pending_len: usize,
    invalid: bool,
}

impl OwnerSemanticKeyProbe {
    pub(super) fn new(expected: OwnerKeyKind, max_bytes: u64) -> Self {
        Self {
            expected,
            prefix: [0; SEMANTIC_KEY_DOMAIN.len() + 9],
            prefix_len: 0,
            expected_total: None,
            bytes_seen: 0,
            max_bytes,
            utf8_pending: [0; 4],
            utf8_pending_len: 0,
            invalid: false,
        }
    }

    fn fail(&mut self) -> io::Error {
        self.invalid = true;
        io::Error::other("WorldState owner semantic key")
    }

    fn parse_prefix(&mut self) -> io::Result<()> {
        let domain_len = SEMANTIC_KEY_DOMAIN.len();
        if &self.prefix[..domain_len] != SEMANTIC_KEY_DOMAIN {
            return Err(self.fail());
        }
        match (self.expected, self.prefix[domain_len]) {
            (OwnerKeyKind::Text, 1) => {
                let length = u64::from_be_bytes(
                    self.prefix[domain_len + 1..domain_len + 9]
                        .try_into()
                        .map_err(|_| self.fail())?,
                );
                self.expected_total = Some(
                    (self.prefix.len() as u64)
                        .checked_add(length)
                        .filter(|total| *total <= self.max_bytes)
                        .ok_or_else(|| self.fail())?,
                );
            }
            (OwnerKeyKind::Action, 2) => {
                if self.prefix.len() as u64 > self.max_bytes {
                    return Err(self.fail());
                }
                self.expected_total = Some(self.prefix.len() as u64);
            }
            _ => return Err(self.fail()),
        }
        Ok(())
    }

    fn validate_text(&mut self, mut bytes: &[u8]) -> io::Result<()> {
        if self.utf8_pending_len != 0 {
            let width = match self.utf8_pending[0] {
                0xc2..=0xdf => 2,
                0xe0..=0xef => 3,
                0xf0..=0xf4 => 4,
                _ => return Err(self.fail()),
            };
            let take = (width - self.utf8_pending_len).min(bytes.len());
            self.utf8_pending[self.utf8_pending_len..self.utf8_pending_len + take]
                .copy_from_slice(&bytes[..take]);
            self.utf8_pending_len += take;
            bytes = &bytes[take..];
            if self.utf8_pending_len < width {
                return Ok(());
            }
            if std::str::from_utf8(&self.utf8_pending[..width]).is_err() {
                return Err(self.fail());
            }
            self.utf8_pending_len = 0;
        }
        match std::str::from_utf8(bytes) {
            Ok(_) => Ok(()),
            Err(error) if error.error_len().is_none() => {
                let tail = &bytes[error.valid_up_to()..];
                if tail.len() > 3 {
                    return Err(self.fail());
                }
                self.utf8_pending[..tail.len()].copy_from_slice(tail);
                self.utf8_pending_len = tail.len();
                Ok(())
            }
            Err(_) => Err(self.fail()),
        }
    }

    pub(super) fn finish(&self) -> bool {
        self.expected_total == Some(self.bytes_seen)
            && self.prefix_len == self.prefix.len()
            && (matches!(self.expected, OwnerKeyKind::Action) || self.utf8_pending_len == 0)
            && !self.invalid
    }
}

impl Write for OwnerSemanticKeyProbe {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.invalid {
            return Err(io::Error::other("WorldState owner semantic key"));
        }
        let Some(seen) = self.bytes_seen.checked_add(bytes.len() as u64) else {
            return Err(self.fail());
        };
        if seen > self.max_bytes {
            return Err(self.fail());
        }
        self.bytes_seen = seen;
        let mut offset = 0;
        if self.prefix_len < self.prefix.len() {
            let take = (self.prefix.len() - self.prefix_len).min(bytes.len());
            self.prefix[self.prefix_len..self.prefix_len + take].copy_from_slice(&bytes[..take]);
            self.prefix_len += take;
            offset = take;
            if self.prefix_len == self.prefix.len() {
                self.parse_prefix()?;
            } else {
                return Ok(bytes.len());
            }
        }
        let expected_total = self.expected_total.ok_or_else(|| self.fail())?;
        if self.bytes_seen > expected_total {
            return Err(self.fail());
        }
        let tail = &bytes[offset..];
        match self.expected {
            OwnerKeyKind::Text => self.validate_text(tail)?,
            OwnerKeyKind::Action if !tail.is_empty() => return Err(self.fail()),
            OwnerKeyKind::Action => {}
        }
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        if self.invalid {
            Err(io::Error::other("WorldState owner semantic key"))
        } else {
            Ok(())
        }
    }
}

fn read_typed_member<R: ValueNodeReader>(
    value: &[u8; 32],
    field_key: &WorldFieldKeyV1,
    profile: OwnerMemberProfile,
    values: &mut ValueReadContext<'_, R>,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<(), WorldStateOwnerError> {
    let mut key_probe = OwnerSemanticKeyProbe::new(profile.key_kind, profile.max_record_bytes);
    let mut payload = [0u8; 8];
    let result = read_member_record_v1(
        value,
        field_key,
        values,
        MemberRecordLimitsV1 {
            semantic_bytes: profile.max_record_bytes,
            payload_bytes: payload.len() as u64,
            record_bytes: profile.max_record_bytes,
        },
        &mut key_probe,
        &mut &mut payload[..],
        cancelled,
    );
    if key_probe.invalid {
        return Err(WorldStateOwnerError::Invalid(
            "typed owner semantic key profile",
        ));
    }
    let info = result.map_err(WorldStateOwnerError::Value)?;
    if !key_probe.finish() {
        return Err(WorldStateOwnerError::Invalid(
            "typed owner semantic key framing",
        ));
    }
    if info.codec != profile.codec || info.payload_bytes != profile.payload_bytes {
        return Err(WorldStateOwnerError::Invalid("typed owner payload profile"));
    }
    if profile.codec == MemberPayloadCodecV1::U64 {
        let _value = u64::from_be_bytes(payload);
    } else if profile.codec == MemberPayloadCodecV1::I64 {
        let _value = i64::from_be_bytes(payload);
    }
    Ok(())
}

fn read_text_byte_member<R: ValueNodeReader>(
    owner: TextByteOwnerField,
    field: FieldDescriptor,
    key: &WorldFieldKeyV1,
    value: &[u8; 32],
    values: &mut ValueReadContext<'_, R>,
    max_value_bytes: u64,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<(), WorldStateOwnerError> {
    if !owner.descriptor_matches(field) {
        return Err(WorldStateOwnerError::Invalid(
            "text/bytes owner descriptor mismatch",
        ));
    }
    let parsed =
        WorldFieldKeyV1::from_bytes(key.as_bytes()).map_err(WorldStateOwnerError::Invalid)?;
    if parsed.kind != owner.record_kind() {
        return Err(WorldStateOwnerError::Invalid(
            "text/bytes owner record kind",
        ));
    }

    let mut key_probe = OwnerSemanticKeyProbe::new(OwnerKeyKind::Text, max_value_bytes);
    let mut discarded = io::sink();
    let limits = MemberRecordLimitsV1 {
        semantic_bytes: max_value_bytes,
        payload_bytes: max_value_bytes,
        record_bytes: max_value_bytes,
    };
    let info = if owner.payload_is_text() {
        let mut payload_probe = Utf8Output::new(&mut discarded, "typed owner text payload UTF-8");
        let result = read_member_record_v1(
            value,
            key,
            values,
            limits,
            &mut key_probe,
            &mut payload_probe,
            cancelled,
        );
        if key_probe.invalid {
            return Err(WorldStateOwnerError::Invalid(
                "typed owner semantic key profile",
            ));
        }
        if payload_probe.is_invalid() {
            return Err(WorldStateOwnerError::Invalid(
                "typed owner text payload UTF-8",
            ));
        }
        let info = result.map_err(WorldStateOwnerError::Value)?;
        if !payload_probe.is_complete() {
            return Err(WorldStateOwnerError::Invalid(
                "typed owner text payload UTF-8 framing",
            ));
        }
        info
    } else {
        let result = read_member_record_v1(
            value,
            key,
            values,
            limits,
            &mut key_probe,
            &mut discarded,
            cancelled,
        );
        if key_probe.invalid {
            return Err(WorldStateOwnerError::Invalid(
                "typed owner semantic key profile",
            ));
        }
        result.map_err(WorldStateOwnerError::Value)?
    };

    if !key_probe.finish() {
        return Err(WorldStateOwnerError::Invalid(
            "typed owner semantic key framing",
        ));
    }
    if info.codec != owner.payload_codec() {
        return Err(WorldStateOwnerError::Invalid(
            "typed owner text/bytes payload profile",
        ));
    }
    Ok(())
}

#[derive(Clone, Copy)]
struct OwnerMemberProfile {
    key_kind: OwnerKeyKind,
    codec: MemberPayloadCodecV1,
    payload_bytes: u64,
    max_record_bytes: u64,
}

fn id_set_key_kind(field: WorldIdSetFieldV1) -> OwnerKeyKind {
    match field {
        WorldIdSetFieldV1::CompletedLogisticsRoutes | WorldIdSetFieldV1::RetiredFactories => {
            OwnerKeyKind::Text
        }
        WorldIdSetFieldV1::SettledLogisticsTransits
        | WorldIdSetFieldV1::SettledFactoryBuilds
        | WorldIdSetFieldV1::SettledRecipeJobs => OwnerKeyKind::Action,
    }
}

fn number_map_profile(field: WorldNumberMapFieldV1) -> (OwnerKeyKind, MemberPayloadCodecV1) {
    let key = if field == WorldNumberMapFieldV1::IndustrySettlementOrders {
        OwnerKeyKind::Action
    } else {
        OwnerKeyKind::Text
    };
    let codec = if matches!(
        field,
        WorldNumberMapFieldV1::ReputationWindowAccumulated
            | WorldNumberMapFieldV1::ReputationScores
    ) {
        MemberPayloadCodecV1::I64
    } else {
        MemberPayloadCodecV1::U64
    };
    (key, codec)
}

fn read_u64_entry<R: ValueNodeReader>(
    scalar: WorldU64FieldV1,
    field: FieldDescriptor,
    key: WorldFieldKeyV1,
    value: &[u8; 32],
    values: &mut ValueReadContext<'_, R>,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<(), WorldStateOwnerError> {
    if scalar.descriptor().ok() != Some(field) {
        return Err(WorldStateOwnerError::Invalid("u64 owner registration"));
    }
    let semantic_len = usize::try_from(U64_SEMANTIC_BYTES)
        .map_err(|_| WorldStateOwnerError::Invalid("u64 semantic key length"))?;
    let mut semantic = [0u8; 64];
    let mut semantic_output = &mut semantic[..semantic_len];
    let payload_len = 8usize;
    let mut payload = [0u8; 8];
    let mut payload_output = &mut payload[..];
    let record_limit = crate::incremental_field_member::FIELD_MEMBER_HEADER_BYTES
        .checked_add(key.as_bytes().len())
        .and_then(|size| size.checked_add(semantic_len))
        .and_then(|size| size.checked_add(payload_len))
        .and_then(|size| u64::try_from(size).ok())
        .ok_or(WorldStateOwnerError::Invalid("u64 record limit overflow"))?;
    let profile = MemberReadProfileV1 {
        codec: MemberPayloadCodecV1::U64,
        semantic_bytes: U64_SEMANTIC_BYTES,
        limits: MemberRecordLimitsV1 {
            semantic_bytes: U64_SEMANTIC_BYTES,
            payload_bytes: 8,
            record_bytes: record_limit,
        },
    };
    let info = read_member_record_profile_v1(
        value,
        &key,
        values,
        profile,
        &mut semantic_output,
        &mut payload_output,
        cancelled,
    )
    .map_err(WorldStateOwnerError::Value)?;
    if info.codec != MemberPayloadCodecV1::U64
        || info.semantic_bytes != U64_SEMANTIC_BYTES
        || info.payload_bytes != 8
        || !semantic_output.is_empty()
        || !payload_output.is_empty()
        || read_semantic_key_v1(&semantic[..semantic_len]) != Ok(SemanticKeyRefV1::Unit)
    {
        return Err(WorldStateOwnerError::Invalid("u64 owner member profile"));
    }
    let _value = u64::from_be_bytes(payload);
    Ok(())
}

/// Scan all six namespaces and bind every encountered structural key to the
/// frozen WorldState descriptor table. `verify_state_values` must already have
/// established every value-tree closure. This function adds owner typing for
/// the implemented scalar-u64, ID-set, number-map, text-map and byte-map
/// descriptors and checks required metadata/member records for the closed
/// owner families connected here. Family qualifiers run only when their
/// format markers are present; absent families remain explicit unsupported
/// fields. Partial or malformed marked families fail closed.
/// The roots scanned are taken directly from `root_set`; the returned
/// diagnostic binds their complete world/genesis/height/tick/root identity.
/// Callers must reject the incomplete report before issuing a profile-3 receipt.
pub fn verify_world_state_owner_coverage<S: MerkleNodeReader, V: ValueNodeReader>(
    root_set: &RootSetV1,
    state: &S,
    values: &mut ValueReadContext<'_, V>,
    state_io: &mut StateScanBudget,
    max_value_bytes: u64,
    cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<UnsupportedWorldStateOwnersV1>, WorldStateOwnerError> {
    if state.allocation_class() != Some(values.class)
        || !state
            .allocation_budget()
            .is_some_and(|pool| pool.same_pool(values.memory))
    {
        return Err(WorldStateOwnerError::Invalid("owner source pool/class"));
    }
    let memory = values.memory;
    let root_set_charge = memory
        .reserve(ROOT_SET_CANONICAL_MAX_BYTES, values.class)
        .map_err(WorldStateOwnerError::Memory)?;
    let root_set_digest = root_set.digest().map_err(WorldStateOwnerError::RootSet)?;
    drop(root_set_charge);

    let seen_bytes = WORLD_STATE_FIELDS
        .len()
        .checked_mul(std::mem::size_of::<SeenField>())
        .ok_or(WorldStateOwnerError::Invalid("owner schema size overflow"))?;
    let schema_charge = memory
        .reserve(OWNER_SCAN_METADATA_BYTES.max(seen_bytes), values.class)
        .map_err(WorldStateOwnerError::Memory)?;
    let cancelled = RefCell::new(cancelled);
    let mut seen = Vec::with_capacity(WORLD_STATE_FIELDS.len());
    seen.resize(WORLD_STATE_FIELDS.len(), SeenField::default());
    if seen.capacity() * std::mem::size_of::<SeenField>() > schema_charge.bytes() {
        return Err(WorldStateOwnerError::Invalid("owner schema allocation"));
    }
    let mut module_instance_coverage = module_instances::Coverage::default();
    let mut unknown_owner = false;
    for root in &root_set.state {
        let mut scan = StateScanContext {
            source: state,
            memory,
            class: values.class,
            io: state_io,
        };
        let result = scan.visit_entries_with_reader(
            root,
            |entry, scan_reader| {
                if (cancelled.borrow_mut())() {
                    return Err(WorldStateOwnerError::Value(ValueTreeError::Cancelled));
                }
                if entry.namespace != root.namespace {
                    return Err(WorldStateOwnerError::Invalid(
                        "entry namespace/root mismatch",
                    ));
                }
                let parsed =
                    crate::incremental_world_field_key::WorldFieldKeyV1::from_bytes(&entry.key)
                        .map_err(WorldStateOwnerError::Invalid)?;
                if module_instances::handles_name(parsed.name)
                    && module_instances::visit_entry(
                        &mut module_instance_coverage,
                        root,
                        entry,
                        scan_reader,
                        values,
                        max_value_bytes,
                        &mut || (cancelled.borrow_mut())(),
                    )?
                {
                    return Ok(());
                }
                let Some((index, field)) = registered_descriptor(
                    parsed.owner,
                    parsed.namespace,
                    parsed.shape,
                    parsed.name,
                )?
                else {
                    unknown_owner = true;
                    return Ok(());
                };
                let key = WorldFieldKeyV1::new(field, parsed.kind, parsed.semantic_key)
                    .map_err(WorldStateOwnerError::Invalid)?;
                if key.as_bytes() != entry.key.as_slice() {
                    return Err(WorldStateOwnerError::Invalid(
                        "registered field key mismatch",
                    ));
                }
                match parsed.kind {
                    WorldFieldRecordKindV1::Metadata => {
                        seen[index].metadata = seen[index].metadata.saturating_add(1);
                        read_field_presence_v1(
                            &entry.value,
                            &key,
                            values,
                            PresenceRecordLimitsV1 {
                                semantic_bytes: 0,
                                record_bytes: 512,
                            },
                            &mut io::sink(),
                            || (cancelled.borrow_mut())(),
                        )
                        .map_err(WorldStateOwnerError::Value)?;
                    }
                    WorldFieldRecordKindV1::Entry if let Some(scalar) = u64_field(field) => {
                        seen[index].entries = seen[index].entries.saturating_add(1);
                        let mut cancel_now = || (cancelled.borrow_mut())();
                        read_u64_entry(scalar, field, key, &entry.value, values, &mut cancel_now)?;
                    }
                    WorldFieldRecordKindV1::Entry if let Some(set) = id_set_field(field) => {
                        if field.owner != FieldOwner::WorldState
                            || field.shape != FieldShape::Set
                            || field.destination
                                != FieldDestination::State(StateNamespaceV1::Authorization)
                        {
                            return Err(WorldStateOwnerError::Invalid(
                                "world ID set descriptor registration",
                            ));
                        }
                        let mut cancel_now = || (cancelled.borrow_mut())();
                        read_typed_member(
                            &entry.value,
                            &key,
                            OwnerMemberProfile {
                                key_kind: id_set_key_kind(set),
                                codec: MemberPayloadCodecV1::Unit,
                                payload_bytes: 0,
                                max_record_bytes: max_value_bytes,
                            },
                            values,
                            &mut cancel_now,
                        )?;
                    }
                    WorldFieldRecordKindV1::Entry if let Some(map) = number_map_field(field) => {
                        if field.owner != FieldOwner::WorldState
                            || field.shape != FieldShape::Map
                            || field.destination != FieldDestination::State(map.namespace())
                        {
                            return Err(WorldStateOwnerError::Invalid(
                                "world number map descriptor registration",
                            ));
                        }
                        let (key_kind, codec) = number_map_profile(map);
                        let mut cancel_now = || (cancelled.borrow_mut())();
                        read_typed_member(
                            &entry.value,
                            &key,
                            OwnerMemberProfile {
                                key_kind,
                                codec,
                                payload_bytes: 8,
                                max_record_bytes: max_value_bytes,
                            },
                            values,
                            &mut cancel_now,
                        )?;
                    }
                    WorldFieldRecordKindV1::Entry | WorldFieldRecordKindV1::Bytes
                        if let Some(owner) = text_byte_field(field) =>
                    {
                        seen[index].entries = seen[index].entries.saturating_add(1);
                        let mut cancel_now = || (cancelled.borrow_mut())();
                        read_text_byte_member(
                            owner,
                            field,
                            &key,
                            &entry.value,
                            values,
                            max_value_bytes,
                            &mut cancel_now,
                        )?;
                    }
                    WorldFieldRecordKindV1::Entry if typed_maps::is_installed_target(field) => {
                        seen[index].entries = seen[index].entries.saturating_add(1);
                        let mut cancel_now = || (cancelled.borrow_mut())();
                        typed_maps::validate_installed_target_member(
                            field,
                            entry,
                            parsed,
                            values,
                            max_value_bytes,
                            &mut cancel_now,
                        )?;
                    }
                    WorldFieldRecordKindV1::Entry if typed_maps::is_world_balance(field) => {
                        seen[index].entries = seen[index].entries.saturating_add(1);
                        let mut cancel_now = || (cancelled.borrow_mut())();
                        typed_maps::validate_world_balance_member(
                            root,
                            field,
                            entry,
                            parsed,
                            values,
                            max_value_bytes,
                            &mut cancel_now,
                        )?;
                    }
                    WorldFieldRecordKindV1::Entry
                        if module_marketplace::field_from_descriptor(field).is_some() =>
                    {
                        seen[index].entries = seen[index].entries.saturating_add(1);
                        let mut cancel_now = || (cancelled.borrow_mut())();
                        module_marketplace::validate_member(
                            field,
                            entry,
                            parsed,
                            scan_reader,
                            values,
                            max_value_bytes,
                            &mut cancel_now,
                        )?;
                    }
                    WorldFieldRecordKindV1::Entry
                        if record_families::family_for_descriptor(field).is_some() =>
                    {
                        seen[index].entries = seen[index].entries.saturating_add(1);
                    }
                    _ if u64_field(field).is_some()
                        || id_set_field(field).is_some()
                        || number_map_field(field).is_some()
                        || text_byte_field(field).is_some()
                        || typed_maps::is_installed_target(field)
                        || typed_maps::is_world_balance(field)
                        || module_marketplace::field_from_descriptor(field).is_some()
                        || record_families::family_for_descriptor(field).is_some() =>
                    {
                        return Err(WorldStateOwnerError::Invalid(
                            "unexpected typed owner record kind",
                        ));
                    }
                    _ => {
                        // Other owner records have bounded generic value
                        // closure, but remain untyped and are listed as
                        // unsupported below.
                    }
                }
                Ok(())
            },
            || (cancelled.borrow_mut())(),
        );
        match result {
            Ok(_) => {}
            Err(StateScanError::Visitor(error)) => return Err(error),
            Err(StateScanError::Cancelled) => {
                return Err(WorldStateOwnerError::Value(ValueTreeError::Cancelled));
            }
            Err(StateScanError::Memory(error)) => return Err(WorldStateOwnerError::Memory(error)),
            Err(StateScanError::Tree(error)) => {
                return Err(WorldStateOwnerError::Scan(error));
            }
            Err(StateScanError::Invalid(error)) => {
                return Err(WorldStateOwnerError::Invalid(error));
            }
            Err(StateScanError::ResourceLimited) => {
                return Err(WorldStateOwnerError::ResourceLimited("scan allowance"));
            }
        }
    }
    module_instances::finish(&module_instance_coverage)?;
    if (cancelled.borrow_mut())() {
        return Err(WorldStateOwnerError::Value(ValueTreeError::Cancelled));
    }
    let family_coverage = record_families::verify_present_families(
        root_set,
        &root_set_digest,
        state,
        values,
        state_io,
        max_value_bytes,
        &cancelled,
        &seen,
        WORLD_STATE_FIELDS,
    )?;
    if (cancelled.borrow_mut())() {
        return Err(WorldStateOwnerError::Value(ValueTreeError::Cancelled));
    }

    let unsupported_capacity =
        WORLD_STATE_FIELDS
            .len()
            .checked_add(3)
            .ok_or(WorldStateOwnerError::Invalid(
                "owner diagnostic size overflow",
            ))?;
    let fields_bytes = unsupported_capacity
        .checked_mul(std::mem::size_of::<&'static str>())
        .ok_or(WorldStateOwnerError::Invalid(
            "owner diagnostic size overflow",
        ))?;
    let fields_charge = memory
        .reserve(fields_bytes, values.class)
        .map_err(WorldStateOwnerError::Memory)?;
    let mut unsupported = Vec::with_capacity(unsupported_capacity);
    if unsupported.capacity() * std::mem::size_of::<&'static str>() > fields_charge.bytes() {
        return Err(WorldStateOwnerError::Invalid("owner diagnostic allocation"));
    }

    let mut checked_u64_fields = 0u8;
    let mut checked_id_set_fields = 0u8;
    let mut checked_number_map_fields = 0u8;
    let mut checked_text_byte_fields = 0u8;
    let mut checked_installed_target_fields = 0u8;
    let mut checked_world_balance_fields = 0u8;
    let mut checked_module_marketplace_fields = 0u8;
    let mut checked_governance_fields = 0u8;
    let mut checked_factory_authority_fields = 0u8;
    let mut checked_main_token_policy_fields = 0u8;
    let mut checked_material_fields = 0u8;
    let mut checked_logistics_fields = 0u8;
    let mut checked_fixed_economy_fields = 0u8;
    let mut checked_agent_authority_fields = 0u8;
    let mut checked_reward_fields = 0u8;
    for (index, field) in WORLD_STATE_FIELDS.iter().copied().enumerate() {
        let FieldDestination::State(_) = field.destination else {
            continue;
        };
        if let Some(scalar) = u64_field(field) {
            if seen[index].metadata != 1 || seen[index].entries != 1 {
                return Err(WorldStateOwnerError::Invalid(
                    "required scalar-u64 metadata/entry missing or duplicate",
                ));
            }
            if scalar.descriptor().ok() != Some(field) {
                return Err(WorldStateOwnerError::Invalid("scalar-u64 descriptor"));
            }
            checked_u64_fields = checked_u64_fields.saturating_add(1);
        } else if let Some(set) = id_set_field(field) {
            if seen[index].metadata != 1 {
                return Err(WorldStateOwnerError::Invalid(
                    "required ID-set metadata missing or duplicate",
                ));
            }
            if field.owner != FieldOwner::WorldState
                || field.shape != FieldShape::Set
                || field.destination != FieldDestination::State(StateNamespaceV1::Authorization)
                || set.name() != field.name
            {
                return Err(WorldStateOwnerError::Invalid("ID-set descriptor"));
            }
            checked_id_set_fields = checked_id_set_fields.saturating_add(1);
        } else if let Some(map) = number_map_field(field) {
            if seen[index].metadata != 1 {
                return Err(WorldStateOwnerError::Invalid(
                    "required number-map metadata missing or duplicate",
                ));
            }
            if field.owner != FieldOwner::WorldState
                || field.shape != FieldShape::Map
                || field.destination != FieldDestination::State(map.namespace())
                || map.name() != field.name
            {
                return Err(WorldStateOwnerError::Invalid("number-map descriptor"));
            }
            checked_number_map_fields = checked_number_map_fields.saturating_add(1);
        } else if let Some(owner) = text_byte_field(field) {
            if seen[index].metadata != 1 {
                return Err(WorldStateOwnerError::Invalid(
                    "required text/bytes metadata missing or duplicate",
                ));
            }
            if !owner.descriptor_matches(field) {
                return Err(WorldStateOwnerError::Invalid("text/bytes owner descriptor"));
            }
            checked_text_byte_fields = checked_text_byte_fields.saturating_add(1);
        } else if typed_maps::is_installed_target(field) {
            if seen[index].metadata != 1 {
                return Err(WorldStateOwnerError::Invalid(
                    "required installed-target metadata missing or duplicate",
                ));
            }
            if !typed_maps::valid_installed_target_descriptor(field) {
                return Err(WorldStateOwnerError::Invalid(
                    "installed-target descriptor registration",
                ));
            }
            checked_installed_target_fields = checked_installed_target_fields.saturating_add(1);
        } else if typed_maps::is_world_balance(field) {
            if seen[index].metadata != 1 {
                return Err(WorldStateOwnerError::Invalid(
                    "required world-balance metadata missing or duplicate",
                ));
            }
            if !typed_maps::valid_world_balance_descriptor(field) {
                return Err(WorldStateOwnerError::Invalid(
                    "world-balance descriptor registration",
                ));
            }
            checked_world_balance_fields = checked_world_balance_fields.saturating_add(1);
        } else if module_marketplace::field_from_descriptor(field).is_some() {
            if seen[index].metadata != 1 {
                return Err(WorldStateOwnerError::Invalid(
                    "required module marketplace metadata missing or duplicate",
                ));
            }
            if !module_marketplace::valid_descriptor(field) {
                return Err(WorldStateOwnerError::Invalid(
                    "module marketplace descriptor registration",
                ));
            }
            checked_module_marketplace_fields = checked_module_marketplace_fields.saturating_add(1);
        } else if let Some(family) = record_families::family_for_descriptor(field) {
            if seen[index].metadata != 1 {
                return Err(WorldStateOwnerError::Invalid(
                    "required record-family metadata missing or duplicate",
                ));
            }
            match family {
                record_families::RecordFamily::Governance if family_coverage.governance => {
                    checked_governance_fields = checked_governance_fields.saturating_add(1);
                }
                record_families::RecordFamily::FactoryAuthority
                    if family_coverage.factory_authority =>
                {
                    checked_factory_authority_fields =
                        checked_factory_authority_fields.saturating_add(1);
                }
                record_families::RecordFamily::MainTokenPolicy
                    if family_coverage.main_token_policy =>
                {
                    checked_main_token_policy_fields =
                        checked_main_token_policy_fields.saturating_add(1);
                }
                record_families::RecordFamily::Material if family_coverage.material => {
                    checked_material_fields = checked_material_fields.saturating_add(1);
                }
                record_families::RecordFamily::Logistics if family_coverage.logistics => {
                    checked_logistics_fields = checked_logistics_fields.saturating_add(1);
                }
                record_families::RecordFamily::FixedEconomy if family_coverage.fixed_economy => {
                    checked_fixed_economy_fields = checked_fixed_economy_fields.saturating_add(1);
                }
                record_families::RecordFamily::AgentAuthority
                    if family_coverage.agent_authority =>
                {
                    checked_agent_authority_fields =
                        checked_agent_authority_fields.saturating_add(1);
                }
                record_families::RecordFamily::Reward if family_coverage.reward => {
                    checked_reward_fields = checked_reward_fields.saturating_add(1);
                }
                _ => unsupported.push(field.name),
            }
        } else {
            if seen[index].metadata != 1 {
                return Err(WorldStateOwnerError::Invalid(
                    "required field metadata missing or duplicate",
                ));
            }
            unsupported.push(field.name);
        }
    }
    if checked_installed_target_fields != typed_maps::INSTALLED_TARGET_FIELD_COUNT
        || checked_world_balance_fields != typed_maps::WORLD_BALANCE_FIELD_COUNT
        || checked_module_marketplace_fields != module_marketplace::FIELD_COUNT
        || (family_coverage.governance
            && checked_governance_fields != record_families::GOVERNANCE_FIELD_COUNT)
        || (family_coverage.factory_authority
            && checked_factory_authority_fields != record_families::FACTORY_AUTHORITY_FIELD_COUNT)
        || (family_coverage.main_token_policy
            && checked_main_token_policy_fields != record_families::MAIN_TOKEN_POLICY_FIELD_COUNT)
        || (family_coverage.material
            && checked_material_fields != record_families::MATERIAL_FIELD_COUNT)
        || (family_coverage.logistics
            && checked_logistics_fields != record_families::LOGISTICS_FIELD_COUNT)
        || (family_coverage.fixed_economy
            && checked_fixed_economy_fields != record_families::FIXED_ECONOMY_FIELD_COUNT)
        || (family_coverage.agent_authority
            && checked_agent_authority_fields != record_families::AGENT_AUTHORITY_FIELD_COUNT)
        || (family_coverage.reward && checked_reward_fields != record_families::REWARD_FIELD_COUNT)
    {
        return Err(WorldStateOwnerError::Invalid(
            "typed map owner descriptor coverage",
        ));
    }
    if unknown_owner {
        unsupported.push("unregistered-owner-key");
    }
    let checked_module_instance_fields = module_instances::FIELD_COUNT;
    // These registries are not yet joined to the WorldState descriptor table;
    // keep them explicit even if a particular root happens to contain no such
    // field. Absence is not proof of a complete Runtime/Cognition schema.
    unsupported.push("Runtime-owner-registry");
    unsupported.push("Cognition-owner-registry");
    if unsupported.len() > unsupported.capacity() {
        return Err(WorldStateOwnerError::Invalid("owner diagnostic capacity"));
    }
    let report_charge = memory
        .reserve(
            std::mem::size_of::<UnsupportedWorldStateOwnersV1>(),
            values.class,
        )
        .map_err(WorldStateOwnerError::Memory)?;
    let report = UnsupportedWorldStateOwnersV1 {
        root_set_digest,
        unsupported_fields: fields_charge.retain(unsupported),
        runtime_registry_missing: true,
        cognition_registry_missing: true,
        checked_u64_fields,
        checked_id_set_fields,
        checked_number_map_fields,
        checked_text_byte_fields,
        checked_installed_target_fields,
        checked_world_balance_fields,
        checked_module_instance_fields,
        checked_module_marketplace_fields,
        checked_governance_fields,
        checked_factory_authority_fields,
        checked_main_token_policy_fields,
        checked_material_fields,
        checked_logistics_fields,
        checked_fixed_economy_fields,
        checked_agent_authority_fields,
        checked_reward_fields,
    };
    drop(schema_charge);
    Ok(report_charge.retain(report))
}

#[cfg(test)]
#[path = "incremental_owner_closure_tests.rs"]
mod tests;
