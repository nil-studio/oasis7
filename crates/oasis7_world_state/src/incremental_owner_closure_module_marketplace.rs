use super::{OwnerKeyKind, OwnerSemanticKeyProbe, WorldStateOwnerError};
use crate::{
    incremental_field_member::{
        MemberPayloadCodecV1, MemberRecordLimitsV1, WorldFieldKeyV1, read_member_record_v1,
    },
    incremental_module_marketplace::ModuleMarketplaceFieldV1,
    incremental_module_marketplace::bid_membership::GroupKeyV1,
    incremental_projection::{FieldDescriptor, FieldDestination, FieldOwner, FieldShape},
    incremental_semantic_key::{
        SemanticKeyBudget, SemanticKeyError, SemanticKeyRefV1, SemanticKeyV1, hash_semantic_key_v1,
        read_semantic_key_v1,
    },
    incremental_world_field_key::{
        WORLD_FIELD_KEY_MAX_BYTES, WorldFieldKeyRefV1, WorldFieldRecordKindV1,
    },
};
use oasis7_formal_protocol::{
    incremental_record_root::{RECORD_ROOT_BYTES, RecordRootRefV1},
    incremental_state::{
        MerkleNodeReader, StateEntryV1, StateNamespaceV1, StateRootV1, StateScanError,
        StateScanReader,
    },
    incremental_value::{ValueNodeReader, ValueReadContext, ValueTreeError},
};
use std::{
    cell::RefCell,
    io::{self, Write},
};

#[path = "incremental_owner_closure_module_marketplace_bids.rs"]
mod bids;

pub(super) const FIELD_COUNT: u8 = ModuleMarketplaceFieldV1::ALL.len() as u8;

const CHILD_SEMANTIC_CAPACITY: usize = 128;
const CHILD_ENTRY_KEY_WORKSPACE: usize = 512;

pub(super) fn field_from_descriptor(field: FieldDescriptor) -> Option<ModuleMarketplaceFieldV1> {
    ModuleMarketplaceFieldV1::from_name(field.name)
}

pub(super) fn valid_descriptor(field: FieldDescriptor) -> bool {
    field.owner == FieldOwner::WorldState
        && field.shape == FieldShape::Map
        && field.destination == FieldDestination::State(StateNamespaceV1::Business)
        && ModuleMarketplaceFieldV1::from_name(field.name)
            .is_some_and(|candidate| candidate.descriptor().ok() == Some(field))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ChildSemantic {
    Field(&'static str),
    Bid { bid_id: u64, field: &'static str },
    BidGroup([u8; 32]),
}

impl ChildSemantic {
    fn field(self) -> &'static str {
        match self {
            Self::Field(field) | Self::Bid { field, .. } => field,
            Self::BidGroup(_) => "group_root_reference",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ChildPayloadKind {
    Unit,
    Bool,
    U64,
    I64,
    Text,
    FixedU64x7,
    RecordRoot,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ChildValue {
    Unit,
    Bool(bool),
    U64(u64),
    I64(i64),
    Text,
    TextPairDigest([u8; 32]),
    BidIndexNode(crate::incremental_module_marketplace::bid_index::BidIndexNodeV1),
    BidGroupRoot([u8; RECORD_ROOT_BYTES]),
}

fn child_payload_kind(field: &str) -> Option<ChildPayloadKind> {
    Some(match field {
        "present" => ChildPayloadKind::Unit,
        "label_present" => ChildPayloadKind::Bool,
        "anchor_kind" | "length" | "next_bid_id" | "sequence_root_id" | "order_id"
        | "price_kind" | "listed_at" | "bid_at" => ChildPayloadKind::U64,
        "anchor_x_cm" | "anchor_y_cm" | "anchor_z_cm" | "price_amount" => ChildPayloadKind::I64,
        "entity_id" | "module_id" | "kind" | "label_text" | "anchor_agent_id"
        | "anchor_location_id" | "seller_agent_id" | "bidder_agent_id" => ChildPayloadKind::Text,
        "index_node" => ChildPayloadKind::FixedU64x7,
        "group_root_reference" => ChildPayloadKind::RecordRoot,
        _ => return None,
    })
}

fn child_codec(kind: ChildPayloadKind) -> MemberPayloadCodecV1 {
    match kind {
        ChildPayloadKind::Unit => MemberPayloadCodecV1::Unit,
        ChildPayloadKind::Bool => MemberPayloadCodecV1::Bool,
        ChildPayloadKind::U64 => MemberPayloadCodecV1::U64,
        ChildPayloadKind::I64 => MemberPayloadCodecV1::I64,
        ChildPayloadKind::Text => MemberPayloadCodecV1::Text,
        ChildPayloadKind::FixedU64x7 => MemberPayloadCodecV1::FixedU64x7,
        ChildPayloadKind::RecordRoot => MemberPayloadCodecV1::RecordRoot,
    }
}

fn static_field(name: &str) -> Option<&'static str> {
    child_payload_kind(name).map(|_| match name {
        "present" => "present",
        "label_present" => "label_present",
        "anchor_kind" => "anchor_kind",
        "length" => "length",
        "next_bid_id" => "next_bid_id",
        "sequence_root_id" => "sequence_root_id",
        "index_node" => "index_node",
        "order_id" => "order_id",
        "price_kind" => "price_kind",
        "listed_at" => "listed_at",
        "bid_at" => "bid_at",
        "anchor_x_cm" => "anchor_x_cm",
        "anchor_y_cm" => "anchor_y_cm",
        "anchor_z_cm" => "anchor_z_cm",
        "price_amount" => "price_amount",
        "entity_id" => "entity_id",
        "module_id" => "module_id",
        "kind" => "kind",
        "label_text" => "label_text",
        "anchor_agent_id" => "anchor_agent_id",
        "anchor_location_id" => "anchor_location_id",
        "seller_agent_id" => "seller_agent_id",
        "bidder_agent_id" => "bidder_agent_id",
        _ => unreachable!(),
    })
}

fn classify_semantic(bytes: &[u8]) -> Option<ChildSemantic> {
    match read_semantic_key_v1(bytes).ok()? {
        SemanticKeyRefV1::Text(key) => {
            if let Some((digest, _)) = GroupKeyV1::from_text(key) {
                Some(ChildSemantic::BidGroup(digest))
            } else {
                Some(ChildSemantic::Field(static_field(key)?))
            }
        }
        SemanticKeyRefV1::Pair { left, right } => match (left.decode().ok()?, right.decode().ok()?)
        {
            (
                SemanticKeyRefV1::Pair {
                    left: bid_name,
                    right: bid_id,
                },
                SemanticKeyRefV1::Text(field),
            ) if bid_name.decode().ok()? == SemanticKeyRefV1::Text("bid") => {
                let SemanticKeyRefV1::Unsigned(bid_id) = bid_id.decode().ok()? else {
                    return None;
                };
                Some(ChildSemantic::Bid {
                    bid_id,
                    field: static_field(field)?,
                })
            }
            _ => None,
        },
        _ => None,
    }
}

struct ChildProbe {
    semantic_bytes: [u8; CHILD_SEMANTIC_CAPACITY],
    semantic_len: usize,
    semantic: Option<ChildSemantic>,
    fixed: [u8; crate::incremental_module_marketplace::bid_index::BID_INDEX_NODE_BYTES],
    fixed_len: usize,
    record_root: [u8; RECORD_ROOT_BYTES],
    record_root_len: usize,
    pair_order_id: Option<u64>,
    pair_hasher: Option<blake3::Hasher>,
    payload_bytes_seen: u64,
    utf8_pending: [u8; 4],
    utf8_pending_len: usize,
    work_limited: bool,
    invalid: bool,
}

impl Default for ChildProbe {
    fn default() -> Self {
        Self {
            semantic_bytes: [0; CHILD_SEMANTIC_CAPACITY],
            semantic_len: 0,
            semantic: None,
            fixed: [0; crate::incremental_module_marketplace::bid_index::BID_INDEX_NODE_BYTES],
            fixed_len: 0,
            record_root: [0; RECORD_ROOT_BYTES],
            record_root_len: 0,
            pair_order_id: None,
            pair_hasher: None,
            payload_bytes_seen: 0,
            utf8_pending: [0; 4],
            utf8_pending_len: 0,
            work_limited: false,
            invalid: false,
        }
    }
}

impl ChildProbe {
    fn for_pair(order_id: u64) -> Self {
        Self {
            pair_order_id: Some(order_id),
            ..Self::default()
        }
    }

    fn is_pair_text(&self) -> bool {
        self.pair_order_id.is_some()
            && matches!(
                self.semantic,
                Some(
                    ChildSemantic::Field("bidder_agent_id")
                        | ChildSemantic::Bid {
                            field: "bidder_agent_id",
                            ..
                        }
                )
            )
    }

    fn start_pair_hash(&mut self) {
        let Some(order_id) = self.pair_order_id else {
            return;
        };
        let mut hasher = blake3::Hasher::new();
        hasher.update(crate::incremental_module_marketplace::bid_membership::PAIR_DIGEST_DOMAIN);
        hasher.update(&order_id.to_be_bytes());
        self.pair_hasher = Some(hasher);
    }

    fn push_utf8(&mut self, mut bytes: &[u8]) -> bool {
        if self.utf8_pending_len != 0 {
            let width = match self.utf8_pending[0] {
                0xc2..=0xdf => 2,
                0xe0..=0xef => 3,
                0xf0..=0xf4 => 4,
                _ => return false,
            };
            let take = (width - self.utf8_pending_len).min(bytes.len());
            self.utf8_pending[self.utf8_pending_len..self.utf8_pending_len + take]
                .copy_from_slice(&bytes[..take]);
            self.utf8_pending_len += take;
            bytes = &bytes[take..];
            if self.utf8_pending_len < width {
                return true;
            }
            if std::str::from_utf8(&self.utf8_pending[..width]).is_err() {
                return false;
            }
            self.utf8_pending_len = 0;
        }
        match std::str::from_utf8(bytes) {
            Ok(_) => true,
            Err(error) if error.error_len().is_none() => {
                let tail = &bytes[error.valid_up_to()..];
                if tail.len() > 3 {
                    return false;
                }
                self.utf8_pending[..tail.len()].copy_from_slice(tail);
                self.utf8_pending_len = tail.len();
                true
            }
            Err(_) => false,
        }
    }

    fn value(
        &self,
        info: crate::incremental_field_member::MemberRecordInfoV1,
    ) -> Option<ChildValue> {
        let semantic = self.semantic?;
        let kind = child_payload_kind(semantic.field())?;
        if info.codec != child_codec(kind)
            || info.semantic_bytes != self.semantic_len as u64
            || info.payload_bytes != self.payload_bytes_seen
        {
            return None;
        }
        Some(match kind {
            ChildPayloadKind::Unit if info.payload_bytes == 0 && self.fixed_len == 0 => {
                ChildValue::Unit
            }
            ChildPayloadKind::Bool
                if info.payload_bytes == 1 && self.fixed_len == 1 && self.fixed[0] <= 1 =>
            {
                ChildValue::Bool(self.fixed[0] != 0)
            }
            ChildPayloadKind::U64 if info.payload_bytes == 8 && self.fixed_len == 8 => {
                let bytes: [u8; 8] = self.fixed[..8].try_into().ok()?;
                ChildValue::U64(u64::from_be_bytes(bytes))
            }
            ChildPayloadKind::I64 if info.payload_bytes == 8 && self.fixed_len == 8 => {
                let bytes: [u8; 8] = self.fixed[..8].try_into().ok()?;
                ChildValue::I64(i64::from_be_bytes(bytes))
            }
            ChildPayloadKind::Text if info.payload_bytes > 0 || self.utf8_pending_len == 0 => {
                if self.utf8_pending_len != 0 {
                    return None;
                }
                match &self.pair_hasher {
                    Some(hasher) => ChildValue::TextPairDigest(*hasher.finalize().as_bytes()),
                    None => ChildValue::Text,
                }
            }
            ChildPayloadKind::FixedU64x7
                if info.payload_bytes
                    == crate::incremental_module_marketplace::bid_index::BID_INDEX_NODE_BYTES as u64
                    && self.fixed_len
                        == crate::incremental_module_marketplace::bid_index::BID_INDEX_NODE_BYTES =>
            {
                let bytes: &[u8; crate::incremental_module_marketplace::bid_index::BID_INDEX_NODE_BYTES] =
                    self.fixed.as_slice().try_into().ok()?;
                ChildValue::BidIndexNode(
                    crate::incremental_module_marketplace::bid_index::BidIndexNodeV1::decode(bytes),
                )
            }
            ChildPayloadKind::RecordRoot
                if info.payload_bytes == RECORD_ROOT_BYTES as u64
                    && self.record_root_len == RECORD_ROOT_BYTES =>
            {
                let ChildSemantic::BidGroup(_) = semantic else {
                    return None;
                };
                ChildValue::BidGroupRoot(self.record_root)
            }
            _ => return None,
        })
    }
}

struct ChildKeyOutput<'a>(&'a RefCell<ChildProbe>);
impl Write for ChildKeyOutput<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let mut probe = self.0.borrow_mut();
        let Some(end) = probe.semantic_len.checked_add(bytes.len()) else {
            probe.invalid = true;
            return Err(io::Error::other("market child semantic length"));
        };
        if end > probe.semantic_bytes.len() {
            probe.invalid = true;
            return Err(io::Error::other("market child semantic bound"));
        }
        let start = probe.semantic_len;
        probe.semantic_bytes[start..end].copy_from_slice(bytes);
        probe.semantic_len = end;
        probe.semantic = classify_semantic(&probe.semantic_bytes[..end]);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

struct ChildPayloadOutput<'a> {
    probe: &'a RefCell<ChildProbe>,
    pair_key_work: Option<&'a mut SemanticKeyBudget>,
}
impl Write for ChildPayloadOutput<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let (pair_text, needs_prefix) = {
            let probe = self.probe.borrow();
            (
                probe.is_pair_text(),
                probe.is_pair_text() && probe.pair_hasher.is_none(),
            )
        };
        if pair_text {
            let budget = self
                .pair_key_work
                .as_deref_mut()
                .ok_or_else(|| io::Error::other("marketplace pair hash budget missing"))?;
            let payload = u64::try_from(bytes.len())
                .map_err(|_| io::Error::other("marketplace pair hash work overflow"))?;
            let fixed = if needs_prefix {
                crate::incremental_module_marketplace::bid_membership::PAIR_DIGEST_DOMAIN
                    .len()
                    .checked_add(8)
                    .and_then(|length| u64::try_from(length).ok())
                    .ok_or_else(|| io::Error::other("marketplace pair hash work overflow"))?
            } else {
                0
            };
            let charge = fixed
                .checked_add(payload)
                .ok_or_else(|| io::Error::other("marketplace pair hash work overflow"))?;
            if budget.bytes_remaining < charge {
                self.probe.borrow_mut().work_limited = true;
                return Err(io::Error::other("marketplace pair hash work exhausted"));
            }
            budget.bytes_remaining -= charge;
        }
        let mut probe = self.probe.borrow_mut();
        if pair_text && needs_prefix {
            probe.start_pair_hash();
        }
        let Some(semantic) = probe.semantic else {
            probe.invalid = true;
            return Err(io::Error::other("market child semantic profile"));
        };
        let Some(kind) = child_payload_kind(semantic.field()) else {
            probe.invalid = true;
            return Err(io::Error::other("market child field profile"));
        };
        let Some(payload_bytes_seen) = probe.payload_bytes_seen.checked_add(bytes.len() as u64)
        else {
            probe.invalid = true;
            return Err(io::Error::other("market child payload length"));
        };
        probe.payload_bytes_seen = payload_bytes_seen;
        match kind {
            ChildPayloadKind::Unit => {
                probe.invalid = true;
                return Err(io::Error::other("market child unit payload"));
            }
            ChildPayloadKind::Text => {
                if !probe.push_utf8(bytes) {
                    probe.invalid = true;
                    return Err(io::Error::other("market child text UTF-8"));
                }
                if pair_text && let Some(hasher) = &mut probe.pair_hasher {
                    hasher.update(bytes);
                }
            }
            ChildPayloadKind::Bool
            | ChildPayloadKind::U64
            | ChildPayloadKind::I64
            | ChildPayloadKind::FixedU64x7 => {
                let Some(end) = probe.fixed_len.checked_add(bytes.len()) else {
                    probe.invalid = true;
                    return Err(io::Error::other("market child fixed payload length"));
                };
                if end > probe.fixed.len() {
                    probe.invalid = true;
                    return Err(io::Error::other("market child fixed payload length"));
                }
                let start = probe.fixed_len;
                probe.fixed[start..end].copy_from_slice(bytes);
                probe.fixed_len = end;
            }
            ChildPayloadKind::RecordRoot => {
                let Some(end) = probe.record_root_len.checked_add(bytes.len()) else {
                    probe.invalid = true;
                    return Err(io::Error::other("market child record root length"));
                };
                if end > probe.record_root.len() {
                    probe.invalid = true;
                    return Err(io::Error::other("market child record root length"));
                }
                let start = probe.record_root_len;
                probe.record_root[start..end].copy_from_slice(bytes);
                probe.record_root_len = end;
            }
        }
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn read_child_member<R: ValueNodeReader>(
    entry: &StateEntryV1,
    field: FieldDescriptor,
    values: &mut ValueReadContext<'_, R>,
    max_value_bytes: u64,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<(ChildSemantic, ChildValue), WorldStateOwnerError> {
    read_child_member_inner(entry, field, values, max_value_bytes, cancelled, None, None)
}

fn read_child_member_with_pair<R: ValueNodeReader>(
    entry: &StateEntryV1,
    field: FieldDescriptor,
    order_id: u64,
    key_work: &mut SemanticKeyBudget,
    values: &mut ValueReadContext<'_, R>,
    max_value_bytes: u64,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<(ChildSemantic, ChildValue), WorldStateOwnerError> {
    read_child_member_inner(
        entry,
        field,
        values,
        max_value_bytes,
        cancelled,
        Some(order_id),
        Some(key_work),
    )
}

fn read_child_member_inner<R: ValueNodeReader>(
    entry: &StateEntryV1,
    field: FieldDescriptor,
    values: &mut ValueReadContext<'_, R>,
    max_value_bytes: u64,
    cancelled: &mut impl FnMut() -> bool,
    pair_order_id: Option<u64>,
    mut pair_key_work: Option<&mut SemanticKeyBudget>,
) -> Result<(ChildSemantic, ChildValue), WorldStateOwnerError> {
    let parsed = WorldFieldKeyV1::from_bytes(&entry.key).map_err(WorldStateOwnerError::Invalid)?;
    if parsed.owner != FieldOwner::WorldState
        || parsed.namespace != StateNamespaceV1::Business
        || parsed.shape != FieldShape::Map
        || parsed.kind != WorldFieldRecordKindV1::Entry
        || parsed.name != field.name
        || entry.namespace != StateNamespaceV1::Business
    {
        return Err(WorldStateOwnerError::Invalid(
            "module marketplace child descriptor binding",
        ));
    }
    let key = WorldFieldKeyV1::new(field, parsed.kind, parsed.semantic_key)
        .map_err(WorldStateOwnerError::Invalid)?;
    if key.as_bytes() != entry.key.as_slice() {
        return Err(WorldStateOwnerError::Invalid(
            "module marketplace child key mismatch",
        ));
    }
    let probe = RefCell::new(pair_order_id.map_or_else(ChildProbe::default, ChildProbe::for_pair));
    let info = read_member_record_v1(
        &entry.value,
        &key,
        values,
        MemberRecordLimitsV1 {
            semantic_bytes: max_value_bytes,
            payload_bytes: max_value_bytes,
            record_bytes: max_value_bytes,
        },
        &mut ChildKeyOutput(&probe),
        &mut ChildPayloadOutput {
            probe: &probe,
            pair_key_work: pair_key_work.as_deref_mut(),
        },
        &mut *cancelled,
    );
    let mut probe = probe.into_inner();
    if probe.work_limited {
        return Err(WorldStateOwnerError::ResourceLimited(
            "module marketplace pair hash work",
        ));
    }
    if probe.is_pair_text() && probe.pair_hasher.is_none() {
        let fixed = crate::incremental_module_marketplace::bid_membership::PAIR_DIGEST_DOMAIN
            .len()
            .checked_add(8)
            .and_then(|bytes| u64::try_from(bytes).ok())
            .ok_or(WorldStateOwnerError::ResourceLimited(
                "module marketplace pair hash work",
            ))?;
        let work = match &mut pair_key_work {
            Some(work) => &mut **work,
            None => {
                return Err(WorldStateOwnerError::Invalid(
                    "module marketplace pair hash budget",
                ));
            }
        };
        if work.bytes_remaining < fixed {
            return Err(WorldStateOwnerError::ResourceLimited(
                "module marketplace pair hash work",
            ));
        }
        work.bytes_remaining -= fixed;
        probe.start_pair_hash();
    }
    if probe.invalid {
        return Err(WorldStateOwnerError::Invalid(
            "module marketplace child member framing",
        ));
    }
    let info = info.map_err(WorldStateOwnerError::Value)?;
    let semantic = probe.semantic.ok_or(WorldStateOwnerError::Invalid(
        "module marketplace child semantic key",
    ))?;
    let value = probe.value(info).ok_or(WorldStateOwnerError::Invalid(
        "module marketplace child payload profile",
    ))?;
    Ok((semantic, value))
}

fn map_scan_error(error: StateScanError<WorldStateOwnerError>) -> WorldStateOwnerError {
    match error {
        StateScanError::Visitor(error) => error,
        StateScanError::Tree(error) => WorldStateOwnerError::Scan(error),
        StateScanError::Memory(error) => WorldStateOwnerError::Memory(error),
        StateScanError::Invalid(error) => WorldStateOwnerError::Invalid(error),
        StateScanError::ResourceLimited => {
            WorldStateOwnerError::ResourceLimited("module marketplace child scan")
        }
        StateScanError::Cancelled => WorldStateOwnerError::Value(ValueTreeError::Cancelled),
    }
}

fn map_scan_lookup_error(error: StateScanError<String>) -> WorldStateOwnerError {
    match error {
        StateScanError::Visitor(error) | StateScanError::Tree(error) => {
            WorldStateOwnerError::Scan(error)
        }
        StateScanError::Memory(error) => WorldStateOwnerError::Memory(error),
        StateScanError::Invalid(error) => WorldStateOwnerError::Invalid(error),
        StateScanError::ResourceLimited => {
            WorldStateOwnerError::ResourceLimited("module marketplace child lookup")
        }
        StateScanError::Cancelled => WorldStateOwnerError::Value(ValueTreeError::Cancelled),
    }
}

fn make_semantic_key<K: SemanticKeyV1 + ?Sized, R: ValueNodeReader>(
    field: FieldDescriptor,
    semantic: &K,
    values: &ValueReadContext<'_, R>,
    key_work: &mut SemanticKeyBudget,
    max_value_bytes: u64,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<WorldFieldKeyV1, WorldStateOwnerError> {
    let key = hash_semantic_key_v1(
        semantic,
        values.memory,
        values.class,
        key_work,
        &mut *cancelled,
    )
    .map_err(|error| match error {
        SemanticKeyError::Memory(error) => WorldStateOwnerError::Memory(error),
        SemanticKeyError::ResourceLimited => {
            WorldStateOwnerError::ResourceLimited("module marketplace semantic key")
        }
        SemanticKeyError::Cancelled => WorldStateOwnerError::Value(ValueTreeError::Cancelled),
        SemanticKeyError::Invalid(reason) => WorldStateOwnerError::Invalid(reason),
    })?;
    if key.encoded_bytes > max_value_bytes {
        return Err(WorldStateOwnerError::ResourceLimited(
            "module marketplace semantic key",
        ));
    }
    WorldFieldKeyV1::new(field, WorldFieldRecordKindV1::Entry, Some(key.digest))
        .map_err(WorldStateOwnerError::Invalid)
}

fn make_text_key<R: ValueNodeReader>(
    field: FieldDescriptor,
    text: &'static str,
    values: &ValueReadContext<'_, R>,
    key_work: &mut SemanticKeyBudget,
    max_value_bytes: u64,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<WorldFieldKeyV1, WorldStateOwnerError> {
    make_semantic_key(field, text, values, key_work, max_value_bytes, cancelled)
}

#[expect(
    clippy::too_many_arguments,
    reason = "Reads a bid member through the caller's shared scan, value and cancellation budgets."
)]
fn read_bid_member<M, R, C>(
    child: &StateRootV1,
    state_reader: &StateScanReader<'_, M, C>,
    field: FieldDescriptor,
    bid_id: u64,
    name: &'static str,
    values: &mut ValueReadContext<'_, R>,
    key_work: &mut SemanticKeyBudget,
    max_value_bytes: u64,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<ChildValue, WorldStateOwnerError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    C: FnMut() -> bool,
{
    let semantic = (("bid", bid_id), name);
    let key = make_semantic_key(
        field,
        &semantic,
        values,
        key_work,
        max_value_bytes,
        cancelled,
    )?;
    read_bid_value_at_key(
        child,
        state_reader,
        field,
        &key,
        ChildSemantic::Bid {
            bid_id,
            field: name,
        },
        key_work,
        values,
        max_value_bytes,
        cancelled,
    )
}

#[expect(
    clippy::too_many_arguments,
    reason = "Reads a scalar child while preserving the active owner's shared budgets and cancellation."
)]
fn read_bid_field<M, R, C>(
    child: &StateRootV1,
    state_reader: &StateScanReader<'_, M, C>,
    field: FieldDescriptor,
    name: &'static str,
    values: &mut ValueReadContext<'_, R>,
    key_work: &mut SemanticKeyBudget,
    max_value_bytes: u64,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<ChildValue, WorldStateOwnerError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    C: FnMut() -> bool,
{
    let key = make_text_key(field, name, values, key_work, max_value_bytes, cancelled)?;
    read_bid_value_at_key(
        child,
        state_reader,
        field,
        &key,
        ChildSemantic::Field(name),
        key_work,
        values,
        max_value_bytes,
        cancelled,
    )
}

#[expect(
    clippy::too_many_arguments,
    reason = "Authenticates one typed bid value with the caller's scan, value and cancellation context."
)]
fn read_bid_value_at_key<M, R, C>(
    child: &StateRootV1,
    state_reader: &StateScanReader<'_, M, C>,
    field: FieldDescriptor,
    key: &WorldFieldKeyV1,
    expected: ChildSemantic,
    key_work: &mut SemanticKeyBudget,
    values: &mut ValueReadContext<'_, R>,
    max_value_bytes: u64,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<ChildValue, WorldStateOwnerError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    C: FnMut() -> bool,
{
    read_value_at_key(
        child,
        state_reader,
        field,
        key,
        expected,
        None,
        key_work,
        values,
        max_value_bytes,
        cancelled,
    )
}

#[expect(
    clippy::too_many_arguments,
    reason = "Authenticates one identity text through the active scan with the pair-order digest prefix."
)]
fn read_bid_text_digest<M, R, C>(
    child: &StateRootV1,
    state_reader: &StateScanReader<'_, M, C>,
    field: FieldDescriptor,
    key: &impl SemanticKeyV1,
    expected: ChildSemantic,
    order_id: u64,
    values: &mut ValueReadContext<'_, R>,
    key_work: &mut SemanticKeyBudget,
    max_value_bytes: u64,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<[u8; 32], WorldStateOwnerError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    C: FnMut() -> bool,
{
    let key = make_semantic_key(field, key, values, key_work, max_value_bytes, cancelled)?;
    match read_value_at_key(
        child,
        state_reader,
        field,
        &key,
        expected,
        Some(order_id),
        key_work,
        values,
        max_value_bytes,
        cancelled,
    )? {
        ChildValue::TextPairDigest(digest) => Ok(digest),
        _ => Err(WorldStateOwnerError::Invalid(
            "module bid pair text profile",
        )),
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "Authenticates one child key under shared state, value, memory and cancellation budgets."
)]
fn read_value_at_key<M, R, C>(
    child: &StateRootV1,
    state_reader: &StateScanReader<'_, M, C>,
    field: FieldDescriptor,
    key: &WorldFieldKeyV1,
    expected: ChildSemantic,
    pair_order_id: Option<u64>,
    key_work: &mut SemanticKeyBudget,
    values: &mut ValueReadContext<'_, R>,
    max_value_bytes: u64,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<ChildValue, WorldStateOwnerError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    C: FnMut() -> bool,
{
    let Some(value) = state_reader
        .get(child, key.as_bytes())
        .map_err(map_scan_lookup_error)?
    else {
        return Err(WorldStateOwnerError::Invalid(
            "module marketplace bid member missing",
        ));
    };
    let _entry_key = values
        .memory
        .reserve(CHILD_ENTRY_KEY_WORKSPACE, values.class)
        .map_err(WorldStateOwnerError::Memory)?;
    if key.as_bytes().len() > WORLD_FIELD_KEY_MAX_BYTES {
        return Err(WorldStateOwnerError::Invalid(
            "module marketplace bid key length",
        ));
    }
    let mut owned_key = Vec::new();
    owned_key
        .try_reserve_exact(key.as_bytes().len())
        .map_err(|_| WorldStateOwnerError::ResourceLimited("module marketplace bid key"))?;
    if owned_key.capacity() > CHILD_ENTRY_KEY_WORKSPACE {
        return Err(WorldStateOwnerError::Invalid(
            "module marketplace bid key allocation",
        ));
    }
    owned_key.extend_from_slice(key.as_bytes());
    let entry = StateEntryV1 {
        namespace: child.namespace,
        key: owned_key,
        value,
    };
    let (actual_semantic, value) = match pair_order_id {
        Some(order_id) => read_child_member_with_pair(
            &entry,
            field,
            order_id,
            key_work,
            values,
            max_value_bytes,
            cancelled,
        )?,
        None => read_child_member(&entry, field, values, max_value_bytes, cancelled)?,
    };
    if actual_semantic != expected {
        return Err(WorldStateOwnerError::Invalid(
            "module marketplace bid member identity",
        ));
    }
    Ok(value)
}

fn validate_child_entries<M, V, C, F>(
    child: &StateRootV1,
    state_reader: &StateScanReader<'_, M, C>,
    descriptor: FieldDescriptor,
    values: &mut ValueReadContext<'_, V>,
    max_value_bytes: u64,
    cancelled: &mut impl FnMut() -> bool,
    mut visit: F,
) -> Result<u64, WorldStateOwnerError>
where
    M: MerkleNodeReader,
    V: ValueNodeReader,
    C: FnMut() -> bool,
    F: FnMut(ChildSemantic, ChildValue) -> Result<(), WorldStateOwnerError>,
{
    state_reader
        .visit_entries(child, |entry| {
            let (semantic, value) =
                read_child_member(entry, descriptor, values, max_value_bytes, cancelled)?;
            visit(semantic, value)
        })
        .map_err(map_scan_error)
}

fn read_outer_child<R: ValueNodeReader>(
    owner: ModuleMarketplaceFieldV1,
    field: FieldDescriptor,
    entry: &StateEntryV1,
    parsed: WorldFieldKeyRefV1<'_>,
    values: &mut ValueReadContext<'_, R>,
    max_value_bytes: u64,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<StateRootV1, WorldStateOwnerError> {
    let key = WorldFieldKeyV1::new(field, parsed.kind, parsed.semantic_key)
        .map_err(WorldStateOwnerError::Invalid)?;
    if key.as_bytes() != entry.key.as_slice() {
        return Err(WorldStateOwnerError::Invalid(
            "module marketplace outer field key mismatch",
        ));
    }
    let mut outer_key = OwnerSemanticKeyProbe::new(OwnerKeyKind::Text, max_value_bytes);
    let mut bytes = [0; RECORD_ROOT_BYTES];
    let info = read_member_record_v1(
        &entry.value,
        &key,
        values,
        MemberRecordLimitsV1 {
            semantic_bytes: max_value_bytes,
            payload_bytes: RECORD_ROOT_BYTES as u64,
            record_bytes: max_value_bytes,
        },
        &mut outer_key,
        &mut &mut bytes[..],
        &mut *cancelled,
    );
    if outer_key.invalid {
        return Err(WorldStateOwnerError::Invalid(
            "module marketplace outer semantic key",
        ));
    }
    let info = info.map_err(WorldStateOwnerError::Value)?;
    if !outer_key.finish() {
        return Err(WorldStateOwnerError::Invalid(
            "module marketplace outer semantic key framing",
        ));
    }
    if info.codec != MemberPayloadCodecV1::RecordRoot
        || info.payload_bytes != RECORD_ROOT_BYTES as u64
    {
        return Err(WorldStateOwnerError::Invalid(
            "module marketplace outer record-root profile",
        ));
    }
    let reference = RecordRootRefV1::from_canonical_bytes(&bytes)
        .map_err(|_| WorldStateOwnerError::Invalid("module marketplace record root"))?;
    let digest = parsed.semantic_key.ok_or(WorldStateOwnerError::Invalid(
        "module marketplace owner digest missing",
    ))?;
    let child = reference
        .validate_binding(&owner.schema(), &digest, StateNamespaceV1::Business)
        .map_err(|_| WorldStateOwnerError::Invalid("module marketplace root binding"))?
        .clone();
    Ok(child)
}

fn validate_visual<M, V, C>(
    child: &StateRootV1,
    reader: &StateScanReader<'_, M, C>,
    field: FieldDescriptor,
    values: &mut ValueReadContext<'_, V>,
    max_value_bytes: u64,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<(), WorldStateOwnerError>
where
    M: MerkleNodeReader,
    V: ValueNodeReader,
    C: FnMut() -> bool,
{
    const ENTITY: u16 = 1 << 0;
    const MODULE: u16 = 1 << 1;
    const KIND: u16 = 1 << 2;
    const LABEL_PRESENT: u16 = 1 << 3;
    const ANCHOR_KIND: u16 = 1 << 4;
    const LABEL_TEXT: u16 = 1 << 5;
    const AGENT_ID: u16 = 1 << 6;
    const LOCATION_ID: u16 = 1 << 7;
    const X: u16 = 1 << 8;
    const Y: u16 = 1 << 9;
    const Z: u16 = 1 << 10;

    let mut seen = 0u16;
    let mut label_present = None;
    let mut anchor_kind = None;
    validate_child_entries(
        child,
        reader,
        field,
        values,
        max_value_bytes,
        cancelled,
        |semantic, value| {
            let ChildSemantic::Field(name) = semantic else {
                return Err(WorldStateOwnerError::Invalid(
                    "module visual unexpected pair key",
                ));
            };
            let (bit, expected_kind) = match name {
                "entity_id" => (ENTITY, ChildPayloadKind::Text),
                "module_id" => (MODULE, ChildPayloadKind::Text),
                "kind" => (KIND, ChildPayloadKind::Text),
                "label_present" => (LABEL_PRESENT, ChildPayloadKind::Bool),
                "anchor_kind" => (ANCHOR_KIND, ChildPayloadKind::U64),
                "label_text" => (LABEL_TEXT, ChildPayloadKind::Text),
                "anchor_agent_id" => (AGENT_ID, ChildPayloadKind::Text),
                "anchor_location_id" => (LOCATION_ID, ChildPayloadKind::Text),
                "anchor_x_cm" => (X, ChildPayloadKind::I64),
                "anchor_y_cm" => (Y, ChildPayloadKind::I64),
                "anchor_z_cm" => (Z, ChildPayloadKind::I64),
                _ => {
                    return Err(WorldStateOwnerError::Invalid(
                        "module visual unknown child field",
                    ));
                }
            };
            if seen & bit != 0 {
                return Err(WorldStateOwnerError::Invalid(
                    "module visual duplicate child field",
                ));
            }
            if child_payload_kind(name) != Some(expected_kind) {
                return Err(WorldStateOwnerError::Invalid(
                    "module visual field codec table",
                ));
            }
            match (name, value) {
                ("label_present", ChildValue::Bool(value)) => label_present = Some(value),
                ("anchor_kind", ChildValue::U64(value)) if value <= 2 => anchor_kind = Some(value),
                ("anchor_kind", _) => {
                    return Err(WorldStateOwnerError::Invalid("module visual anchor kind"));
                }
                (
                    "entity_id" | "module_id" | "kind" | "label_text" | "anchor_agent_id"
                    | "anchor_location_id",
                    ChildValue::Text,
                )
                | ("anchor_x_cm" | "anchor_y_cm" | "anchor_z_cm", ChildValue::I64(_)) => {}
                _ => {
                    return Err(WorldStateOwnerError::Invalid(
                        "module visual child value kind",
                    ));
                }
            }
            seen |= bit;
            Ok(())
        },
    )?;
    let mut expected = ENTITY | MODULE | KIND | LABEL_PRESENT | ANCHOR_KIND;
    if label_present == Some(true) {
        expected |= LABEL_TEXT;
    }
    match anchor_kind {
        Some(0) => expected |= AGENT_ID,
        Some(1) => expected |= LOCATION_ID,
        Some(2) => expected |= X | Y | Z,
        _ => {
            return Err(WorldStateOwnerError::Invalid(
                "module visual required anchor kind missing",
            ));
        }
    }
    if label_present.is_none() || seen != expected {
        return Err(WorldStateOwnerError::Invalid("module visual child closure"));
    }
    Ok(())
}

fn validate_listing<M, V, C>(
    child: &StateRootV1,
    reader: &StateScanReader<'_, M, C>,
    field: FieldDescriptor,
    values: &mut ValueReadContext<'_, V>,
    max_value_bytes: u64,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<(), WorldStateOwnerError>
where
    M: MerkleNodeReader,
    V: ValueNodeReader,
    C: FnMut() -> bool,
{
    let mut seen = 0u8;
    validate_child_entries(
        child,
        reader,
        field,
        values,
        max_value_bytes,
        cancelled,
        |semantic, value| {
            let ChildSemantic::Field(name) = semantic else {
                return Err(WorldStateOwnerError::Invalid(
                    "module listing unexpected pair key",
                ));
            };
            let (bit, expected_kind) = match name {
                "order_id" => (1 << 0, ChildPayloadKind::U64),
                "seller_agent_id" => (1 << 1, ChildPayloadKind::Text),
                "price_kind" => (1 << 2, ChildPayloadKind::U64),
                "price_amount" => (1 << 3, ChildPayloadKind::I64),
                "listed_at" => (1 << 4, ChildPayloadKind::U64),
                _ => {
                    return Err(WorldStateOwnerError::Invalid(
                        "module listing unknown child field",
                    ));
                }
            };
            if seen & bit != 0 || child_payload_kind(name) != Some(expected_kind) {
                return Err(WorldStateOwnerError::Invalid(
                    "module listing duplicate/profile",
                ));
            }
            let valid_value = match (name, value) {
                ("order_id" | "listed_at", ChildValue::U64(_))
                | ("seller_agent_id", ChildValue::Text)
                | ("price_amount", ChildValue::I64(_)) => true,
                ("price_kind", ChildValue::U64(value)) => value <= 1,
                _ => false,
            };
            if !valid_value {
                return Err(WorldStateOwnerError::Invalid("module listing child value"));
            }
            seen |= bit;
            Ok(())
        },
    )?;
    if seen != 0b1_1111 {
        return Err(WorldStateOwnerError::Invalid(
            "module listing child closure",
        ));
    }
    Ok(())
}

pub(super) fn validate_member<M, R, C>(
    field: FieldDescriptor,
    entry: &StateEntryV1,
    parsed: WorldFieldKeyRefV1<'_>,
    state_reader: &StateScanReader<'_, M, C>,
    values: &mut ValueReadContext<'_, R>,
    max_value_bytes: u64,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<(), WorldStateOwnerError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    C: FnMut() -> bool,
{
    let Some(owner) = field_from_descriptor(field) else {
        return Err(WorldStateOwnerError::Invalid(
            "module marketplace field descriptor",
        ));
    };
    if !valid_descriptor(field)
        || parsed.owner != FieldOwner::WorldState
        || parsed.namespace != StateNamespaceV1::Business
        || parsed.shape != FieldShape::Map
        || parsed.kind != WorldFieldRecordKindV1::Entry
        || entry.namespace != StateNamespaceV1::Business
    {
        return Err(WorldStateOwnerError::Invalid(
            "module marketplace descriptor/member binding",
        ));
    }
    if !state_reader.source_matches(values.memory, values.class) {
        return Err(WorldStateOwnerError::Invalid(
            "module marketplace state/value pool binding",
        ));
    }
    let child = read_outer_child(
        owner,
        field,
        entry,
        parsed,
        values,
        max_value_bytes,
        cancelled,
    )?;
    let mut key_work = SemanticKeyBudget {
        bytes_remaining: max_value_bytes,
    };
    match owner {
        ModuleMarketplaceFieldV1::VisualEntities => validate_visual(
            &child,
            state_reader,
            field,
            values,
            max_value_bytes,
            cancelled,
        ),
        ModuleMarketplaceFieldV1::ArtifactListings => validate_listing(
            &child,
            state_reader,
            field,
            values,
            max_value_bytes,
            cancelled,
        ),
        ModuleMarketplaceFieldV1::ArtifactBids => bids::validate_bids(
            &child,
            state_reader,
            field,
            values,
            &mut key_work,
            max_value_bytes,
            cancelled,
        ),
    }
}
