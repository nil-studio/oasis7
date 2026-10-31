use oasis7_formal_protocol::incremental_state::{STATE_ROOT_BYTES, StateNamespaceV1, StateRootV1};

use oasis7::simulator::ProviderMaterialOwnerBindingV1;

pub(super) const MAX_OWNER_ID_BYTES: usize = 256;
pub(super) const MAX_OPERATION_ID_BYTES: usize = 128;
pub(super) const MAX_EVENT_BYTES: usize = 1024;
pub(super) const MAX_HEAD_BYTES: usize = 1024;
pub(super) const MAX_PENDING_ATTEMPTS: u64 = 16_384;
pub(super) const MAX_EVENTS: u64 = 1_000_000;
pub(super) const MAX_PROTECTED_ROOT_PAGE: usize = 128;

const HEAD_MAGIC: &[u8] = b"oasis7.iws.provider-pending-head.v1\0";
const EVENT_MAGIC: &[u8] = b"oasis7.iws.provider-pending-event.v1\0";
const SCOPE_DOMAIN: &[u8] = b"oasis7/iws/provider-pending-owner-scope/v1\0";
const PENDING_ROOT_DOMAIN: &[u8] = b"oasis7/iws/provider-pending-protection-root/v1\0";
const EVENT_KEY_DOMAIN: &[u8] = b"oasis7.iws.provider-pending-event-key.v1\0";
const OPERATION_KEY_DOMAIN: &[u8] = b"oasis7.iws.provider-pending-operation-key.v1\0";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(super) enum EventPhase {
    RequestPrepared = 1,
    DispatchedOrUnknown = 2,
    ResponseBodyPlanned = 3,
    ResponseBodyDurable = 4,
    ResponseContextPlanned = 5,
    ResponseCaptured = 6,
    RejectionEvidencePlanned = 7,
    Committed = 8,
    Rejected = 9,
}

impl EventPhase {
    fn parse(value: u8) -> Result<Self, String> {
        match value {
            1 => Ok(Self::RequestPrepared),
            2 => Ok(Self::DispatchedOrUnknown),
            3 => Ok(Self::ResponseBodyPlanned),
            4 => Ok(Self::ResponseBodyDurable),
            5 => Ok(Self::ResponseContextPlanned),
            6 => Ok(Self::ResponseCaptured),
            7 => Ok(Self::RejectionEvidencePlanned),
            8 => Ok(Self::Committed),
            9 => Ok(Self::Rejected),
            _ => Err("provider pending event phase".into()),
        }
    }

    pub(super) fn is_pending(self) -> bool {
        !matches!(self, Self::Committed | Self::Rejected)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct OwnerHead {
    pub(super) owner: ProviderMaterialOwnerBindingV1,
    pub(super) sequence: u64,
    pub(super) previous_event_id: [u8; 32],
    pub(super) events: StateRootV1,
    pub(super) pending: StateRootV1,
    pub(super) seen: StateRootV1,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct EventRecord {
    pub(super) phase: EventPhase,
    pub(super) sequence: u64,
    pub(super) previous_event_id: [u8; 32],
    pub(super) operation_id: String,
    pub(super) transport_attempt: u64,
    pub(super) attempt_id: [u8; 32],
    pub(super) parent_root_set: [u8; 32],
    pub(super) authority_snapshot: [u8; 32],
    pub(super) request_object_id: [u8; 32],
    pub(super) request_context_object_id: [u8; 32],
    pub(super) response_object_id: Option<[u8; 32]>,
    pub(super) response_context_object_id: Option<[u8; 32]>,
    pub(super) response_status: Option<u16>,
    pub(super) terminal_receipt_id: Option<String>,
    pub(super) terminal_action_id: Option<u64>,
    pub(super) terminal_evidence_object_id: Option<[u8; 32]>,
    pub(super) terminal_disposition_digest: Option<[u8; 32]>,
}

pub(super) fn scope_digest(owner: &ProviderMaterialOwnerBindingV1) -> Result<[u8; 32], String> {
    owner.validate().map_err(str::to_owned)?;
    let world = owner.world_id.as_bytes();
    let world_len = u16::try_from(world.len()).map_err(|_| "provider world id bound")?;
    let mut hasher = blake3::Hasher::new();
    hasher.update(SCOPE_DOMAIN);
    hasher.update(&world_len.to_be_bytes());
    hasher.update(world);
    hasher.update(&owner.genesis_root);
    hasher.update(&owner.owner_id);
    Ok(*hasher.finalize().as_bytes())
}

pub(super) fn pending_root_digest(
    owner: &ProviderMaterialOwnerBindingV1,
    head: &OwnerHead,
) -> Result<[u8; 32], String> {
    if head.events.namespace != StateNamespaceV1::Effects
        || head.pending.namespace != StateNamespaceV1::Effects
        || head.seen.namespace != StateNamespaceV1::Effects
    {
        return Err("provider protection root namespace".into());
    }
    let mut hasher = blake3::Hasher::new();
    hasher.update(PENDING_ROOT_DOMAIN);
    hasher.update(&scope_digest(owner)?);
    hasher.update(&head.sequence.to_be_bytes());
    hasher.update(&head.previous_event_id);
    hasher.update(
        &head
            .events
            .canonical_bytes()
            .map_err(|error| error.to_string())?,
    );
    hasher.update(
        &head
            .pending
            .canonical_bytes()
            .map_err(|error| error.to_string())?,
    );
    hasher.update(
        &head
            .seen
            .canonical_bytes()
            .map_err(|error| error.to_string())?,
    );
    Ok(*hasher.finalize().as_bytes())
}

pub(super) fn event_key(sequence: u64) -> Vec<u8> {
    let mut key = Vec::with_capacity(EVENT_KEY_DOMAIN.len() + 8);
    key.extend_from_slice(EVENT_KEY_DOMAIN);
    key.extend_from_slice(&sequence.to_be_bytes());
    key
}

pub(super) fn operation_key(operation_id: &str) -> Result<Vec<u8>, String> {
    if operation_id.len() > MAX_OPERATION_ID_BYTES
        || !operation_id.starts_with("blake3:")
        || operation_id[7..].len() != 64
        || !operation_id[7..]
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err("provider operation identity".into());
    }
    let mut key = Vec::with_capacity(OPERATION_KEY_DOMAIN.len() + operation_id.len());
    key.extend_from_slice(OPERATION_KEY_DOMAIN);
    key.extend_from_slice(operation_id.as_bytes());
    Ok(key)
}

pub(super) fn encode_head(head: &OwnerHead) -> Result<Vec<u8>, String> {
    head.owner.validate().map_err(str::to_owned)?;
    if head.sequence > MAX_EVENTS
        || head.events.namespace != StateNamespaceV1::Effects
        || head.pending.namespace != StateNamespaceV1::Effects
        || head.seen.namespace != StateNamespaceV1::Effects
        || head.events.leaf_count != head.sequence
        || head.pending.leaf_count > MAX_PENDING_ATTEMPTS
        || head.seen.leaf_count > MAX_EVENTS
    {
        return Err("provider owner head invariants".into());
    }
    let world = head.owner.world_id.as_bytes();
    let world_len = u16::try_from(world.len()).map_err(|_| "provider world id bound")?;
    let mut bytes = Vec::with_capacity(MAX_HEAD_BYTES);
    bytes.extend_from_slice(HEAD_MAGIC);
    let length_offset = bytes.len();
    bytes.extend_from_slice(&0u32.to_be_bytes());
    bytes.extend_from_slice(&world_len.to_be_bytes());
    bytes.extend_from_slice(world);
    bytes.extend_from_slice(&head.owner.genesis_root);
    bytes.extend_from_slice(&head.owner.owner_id);
    bytes.extend_from_slice(&head.sequence.to_be_bytes());
    bytes.extend_from_slice(&head.previous_event_id);
    bytes.extend_from_slice(
        &head
            .events
            .canonical_bytes()
            .map_err(|error| error.to_string())?,
    );
    bytes.extend_from_slice(
        &head
            .pending
            .canonical_bytes()
            .map_err(|error| error.to_string())?,
    );
    bytes.extend_from_slice(
        &head
            .seen
            .canonical_bytes()
            .map_err(|error| error.to_string())?,
    );
    let body_len = bytes
        .len()
        .checked_sub(length_offset + 4)
        .ok_or("provider head length")?;
    let body_len = u32::try_from(body_len).map_err(|_| "provider head length")?;
    bytes[length_offset..length_offset + 4].copy_from_slice(&body_len.to_be_bytes());
    let checksum = blake3::hash(&bytes);
    bytes.extend_from_slice(checksum.as_bytes());
    if bytes.len() > MAX_HEAD_BYTES {
        return Err("provider owner head bound".into());
    }
    Ok(bytes)
}

pub(super) fn decode_head(bytes: &[u8]) -> Result<OwnerHead, String> {
    if bytes.len() > MAX_HEAD_BYTES || !bytes.starts_with(HEAD_MAGIC) {
        return Err("provider owner head format".into());
    }
    let checksum_start = bytes
        .len()
        .checked_sub(32)
        .ok_or("provider head checksum")?;
    if blake3::hash(&bytes[..checksum_start]).as_bytes() != &bytes[checksum_start..] {
        return Err("provider owner head checksum".into());
    }
    let mut cursor = Cursor::new(&bytes[HEAD_MAGIC.len()..checksum_start]);
    let declared = usize::try_from(cursor.u32()?).map_err(|_| "provider head length")?;
    if declared != cursor.remaining() {
        return Err("provider owner head declared length".into());
    }
    let world_len = usize::from(cursor.u16()?);
    if world_len == 0 || world_len > MAX_OWNER_ID_BYTES {
        return Err("provider owner world length".into());
    }
    let world_id = std::str::from_utf8(cursor.bytes(world_len)?)
        .map_err(|_| "provider owner world UTF-8")?
        .to_owned();
    let owner = ProviderMaterialOwnerBindingV1 {
        world_id,
        genesis_root: cursor.array()?,
        owner_id: cursor.array()?,
    };
    let sequence = cursor.u64()?;
    let previous_event_id = cursor.array()?;
    let events = StateRootV1::from_canonical_bytes(cursor.bytes(STATE_ROOT_BYTES)?)
        .map_err(|error| error.to_string())?;
    let pending = StateRootV1::from_canonical_bytes(cursor.bytes(STATE_ROOT_BYTES)?)
        .map_err(|error| error.to_string())?;
    let seen = StateRootV1::from_canonical_bytes(cursor.bytes(STATE_ROOT_BYTES)?)
        .map_err(|error| error.to_string())?;
    cursor.finish()?;
    let head = OwnerHead {
        owner,
        sequence,
        previous_event_id,
        events,
        pending,
        seen,
    };
    // Re-encoding rejects non-canonical fields and checks all counts/namespaces.
    if encode_head(&head)?.as_slice() != bytes {
        return Err("provider owner head canonical encoding".into());
    }
    Ok(head)
}

pub(super) fn encode_event(record: &EventRecord) -> Result<Vec<u8>, String> {
    validate_event(record)?;
    let operation = record.operation_id.as_bytes();
    let operation_len = u16::try_from(operation.len()).map_err(|_| "provider operation length")?;
    let mut bytes = Vec::with_capacity(MAX_EVENT_BYTES);
    bytes.extend_from_slice(EVENT_MAGIC);
    let length_offset = bytes.len();
    bytes.extend_from_slice(&0u32.to_be_bytes());
    bytes.push(record.phase as u8);
    bytes.extend_from_slice(&record.sequence.to_be_bytes());
    bytes.extend_from_slice(&record.previous_event_id);
    bytes.extend_from_slice(&operation_len.to_be_bytes());
    bytes.extend_from_slice(operation);
    bytes.extend_from_slice(&record.transport_attempt.to_be_bytes());
    bytes.extend_from_slice(&record.attempt_id);
    bytes.extend_from_slice(&record.parent_root_set);
    bytes.extend_from_slice(&record.authority_snapshot);
    bytes.extend_from_slice(&record.request_object_id);
    bytes.extend_from_slice(&record.request_context_object_id);
    optional_array(&mut bytes, record.response_object_id.as_ref());
    optional_array(&mut bytes, record.response_context_object_id.as_ref());
    optional_u16(&mut bytes, record.response_status);
    optional_string(&mut bytes, record.terminal_receipt_id.as_deref())?;
    optional_u64(&mut bytes, record.terminal_action_id);
    optional_array(&mut bytes, record.terminal_evidence_object_id.as_ref());
    optional_array(&mut bytes, record.terminal_disposition_digest.as_ref());
    let body_len = bytes
        .len()
        .checked_sub(length_offset + 4)
        .ok_or("provider event length")?;
    let body_len = u32::try_from(body_len).map_err(|_| "provider event length")?;
    bytes[length_offset..length_offset + 4].copy_from_slice(&body_len.to_be_bytes());
    let checksum = blake3::hash(&bytes);
    bytes.extend_from_slice(checksum.as_bytes());
    if bytes.len() > MAX_EVENT_BYTES {
        return Err("provider event bound".into());
    }
    Ok(bytes)
}

pub(super) fn decode_event(bytes: &[u8]) -> Result<EventRecord, String> {
    if bytes.len() > MAX_EVENT_BYTES || !bytes.starts_with(EVENT_MAGIC) {
        return Err("provider event format".into());
    }
    let checksum_start = bytes
        .len()
        .checked_sub(32)
        .ok_or("provider event checksum")?;
    if blake3::hash(&bytes[..checksum_start]).as_bytes() != &bytes[checksum_start..] {
        return Err("provider event checksum".into());
    }
    let mut cursor = Cursor::new(&bytes[EVENT_MAGIC.len()..checksum_start]);
    let declared = usize::try_from(cursor.u32()?).map_err(|_| "provider event length")?;
    if declared != cursor.remaining() {
        return Err("provider event declared length".into());
    }
    let phase = EventPhase::parse(cursor.u8()?)?;
    let sequence = cursor.u64()?;
    let previous_event_id = cursor.array()?;
    let operation_len = usize::from(cursor.u16()?);
    if operation_len == 0 || operation_len > MAX_OPERATION_ID_BYTES {
        return Err("provider operation length".into());
    }
    let operation_id = std::str::from_utf8(cursor.bytes(operation_len)?)
        .map_err(|_| "provider operation UTF-8")?
        .to_owned();
    operation_key(&operation_id)?;
    let record = EventRecord {
        phase,
        sequence,
        previous_event_id,
        operation_id,
        transport_attempt: cursor.u64()?,
        attempt_id: cursor.array()?,
        parent_root_set: cursor.array()?,
        authority_snapshot: cursor.array()?,
        request_object_id: cursor.array()?,
        request_context_object_id: cursor.array()?,
        response_object_id: cursor.optional_array()?,
        response_context_object_id: cursor.optional_array()?,
        response_status: cursor.optional_u16()?,
        terminal_receipt_id: cursor.optional_string(MAX_OWNER_ID_BYTES)?,
        terminal_action_id: cursor.optional_u64()?,
        terminal_evidence_object_id: cursor.optional_array()?,
        terminal_disposition_digest: cursor.optional_array()?,
    };
    cursor.finish()?;
    validate_event(&record)?;
    if encode_event(&record)?.as_slice() != bytes {
        return Err("provider event canonical encoding".into());
    }
    Ok(record)
}

fn validate_event(record: &EventRecord) -> Result<(), String> {
    operation_key(&record.operation_id)?;
    if record.sequence == 0 || record.sequence > MAX_EVENTS {
        return Err("provider event sequence".into());
    }
    if record
        .terminal_receipt_id
        .as_deref()
        .is_some_and(|value| value.is_empty() || value.len() > MAX_OWNER_ID_BYTES)
    {
        return Err("provider terminal receipt ID bound".into());
    }
    let no_response = record.response_object_id.is_none()
        && record.response_context_object_id.is_none()
        && record.response_status.is_none();
    let body_planned = record.response_object_id.is_some()
        && record.response_context_object_id.is_none()
        && record.response_status.is_some();
    let context_planned = record.response_object_id.is_some()
        && record.response_context_object_id.is_some()
        && record.response_status.is_some();
    let no_terminal = record.terminal_receipt_id.is_none()
        && record.terminal_action_id.is_none()
        && record.terminal_evidence_object_id.is_none()
        && record.terminal_disposition_digest.is_none();
    let rejection_evidence = record.terminal_evidence_object_id.is_some()
        && record.terminal_disposition_digest.is_some()
        && record.terminal_receipt_id.is_none()
        && record.terminal_action_id.is_none();
    match record.phase {
        EventPhase::RequestPrepared | EventPhase::DispatchedOrUnknown
            if no_response && no_terminal =>
        {
            Ok(())
        }
        EventPhase::ResponseBodyPlanned | EventPhase::ResponseBodyDurable
            if body_planned && no_terminal =>
        {
            Ok(())
        }
        EventPhase::ResponseContextPlanned | EventPhase::ResponseCaptured
            if context_planned && no_terminal =>
        {
            Ok(())
        }
        EventPhase::RejectionEvidencePlanned if rejection_evidence => Ok(()),
        EventPhase::Committed
            if context_planned
                && record.terminal_receipt_id.is_some()
                && record.terminal_action_id.is_some()
                && record.terminal_evidence_object_id.is_none()
                && record.terminal_disposition_digest.is_none() =>
        {
            Ok(())
        }
        EventPhase::Rejected if rejection_evidence => Ok(()),
        _ => Err("provider event phase fields".into()),
    }
}

fn optional_array(out: &mut Vec<u8>, value: Option<&[u8; 32]>) {
    match value {
        Some(value) => {
            out.push(1);
            out.extend_from_slice(value);
        }
        None => out.push(0),
    }
}
fn optional_u16(out: &mut Vec<u8>, value: Option<u16>) {
    match value {
        Some(value) => {
            out.push(1);
            out.extend_from_slice(&value.to_be_bytes());
        }
        None => out.push(0),
    }
}
fn optional_u64(out: &mut Vec<u8>, value: Option<u64>) {
    match value {
        Some(value) => {
            out.push(1);
            out.extend_from_slice(&value.to_be_bytes());
        }
        None => out.push(0),
    }
}
fn optional_string(out: &mut Vec<u8>, value: Option<&str>) -> Result<(), String> {
    match value {
        Some(value) => {
            if value.is_empty() || value.len() > MAX_OWNER_ID_BYTES {
                return Err("provider terminal receipt ID bound".into());
            }
            out.push(1);
            let length = u16::try_from(value.len()).map_err(|_| "provider receipt ID length")?;
            out.extend_from_slice(&length.to_be_bytes());
            out.extend_from_slice(value.as_bytes());
        }
        None => out.push(0),
    }
    Ok(())
}

struct Cursor<'a> {
    bytes: &'a [u8],
    position: usize,
}
impl<'a> Cursor<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, position: 0 }
    }
    fn remaining(&self) -> usize {
        self.bytes.len().saturating_sub(self.position)
    }
    fn bytes(&mut self, length: usize) -> Result<&'a [u8], String> {
        let end = self
            .position
            .checked_add(length)
            .filter(|end| *end <= self.bytes.len())
            .ok_or_else(|| "provider record truncated".to_owned())?;
        let value = &self.bytes[self.position..end];
        self.position = end;
        Ok(value)
    }
    fn u8(&mut self) -> Result<u8, String> {
        Ok(self.bytes(1)?[0])
    }
    fn u16(&mut self) -> Result<u16, String> {
        Ok(u16::from_be_bytes(
            self.bytes(2)?.try_into().map_err(|_| "provider u16")?,
        ))
    }
    fn u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_be_bytes(
            self.bytes(4)?.try_into().map_err(|_| "provider u32")?,
        ))
    }
    fn u64(&mut self) -> Result<u64, String> {
        Ok(u64::from_be_bytes(
            self.bytes(8)?.try_into().map_err(|_| "provider u64")?,
        ))
    }
    fn array(&mut self) -> Result<[u8; 32], String> {
        self.bytes(32)?
            .try_into()
            .map_err(|_| "provider digest".into())
    }
    fn optional_array(&mut self) -> Result<Option<[u8; 32]>, String> {
        match self.u8()? {
            0 => Ok(None),
            1 => Ok(Some(self.array()?)),
            _ => Err("provider event optional digest".into()),
        }
    }
    fn optional_u16(&mut self) -> Result<Option<u16>, String> {
        match self.u8()? {
            0 => Ok(None),
            1 => Ok(Some(self.u16()?)),
            _ => Err("provider event optional status".into()),
        }
    }
    fn optional_u64(&mut self) -> Result<Option<u64>, String> {
        match self.u8()? {
            0 => Ok(None),
            1 => Ok(Some(self.u64()?)),
            _ => Err("provider event optional action".into()),
        }
    }
    fn optional_string(&mut self, limit: usize) -> Result<Option<String>, String> {
        match self.u8()? {
            0 => Ok(None),
            1 => {
                let length = usize::from(self.u16()?);
                if length == 0 || length > limit {
                    return Err("provider event string bound".into());
                }
                Ok(Some(
                    std::str::from_utf8(self.bytes(length)?)
                        .map_err(|_| "provider event string UTF-8")?
                        .to_owned(),
                ))
            }
            _ => Err("provider event optional string".into()),
        }
    }
    fn finish(&self) -> Result<(), String> {
        if self.position == self.bytes.len() {
            Ok(())
        } else {
            Err("provider record trailing bytes".into())
        }
    }
}
