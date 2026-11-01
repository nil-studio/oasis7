//! Fixed allocator and stable mailbox positions for new-format event lowering.
//! Queue positions never change when the head advances.
pub const EVENT_SEQUENCE_KEY_V1: &[u8] = b"oasis7.iws.event-sequence-key.v1\0";
const SEQUENCE: &[u8] = b"oasis7.iws.event-sequence.v1\0";
const FRONTIER: &[u8] = b"oasis7.iws.mailbox-frontier.v1\0";
pub const MAILBOX_KEY_DOMAIN_V1: &[u8] = b"oasis7.iws.mailbox-key.v1\0";
const KEY: &[u8] = MAILBOX_KEY_DOMAIN_V1;
pub const EVENT_SEQUENCE_BYTES: usize = SEQUENCE.len() + 16;
pub const MAILBOX_FRONTIER_BYTES: usize = FRONTIER.len() + 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EventSequenceV1 {
    pub next: u64,
    pub era: u64,
}
impl EventSequenceV1 {
    pub fn canonical_bytes(self) -> [u8; EVENT_SEQUENCE_BYTES] {
        let mut bytes = [0; EVENT_SEQUENCE_BYTES];
        bytes[..SEQUENCE.len()].copy_from_slice(SEQUENCE);
        bytes[SEQUENCE.len()..SEQUENCE.len() + 8].copy_from_slice(&self.next.to_be_bytes());
        bytes[SEQUENCE.len() + 8..].copy_from_slice(&self.era.to_be_bytes());
        bytes
    }
    pub fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, &'static str> {
        if bytes.len() != EVENT_SEQUENCE_BYTES || !bytes.starts_with(SEQUENCE) {
            return Err("event sequence domain/length");
        }
        Ok(Self {
            next: u64::from_be_bytes(
                bytes[SEQUENCE.len()..SEQUENCE.len() + 8]
                    .try_into()
                    .map_err(|_| "sequence next")?,
            ),
            era: u64::from_be_bytes(
                bytes[SEQUENCE.len() + 8..]
                    .try_into()
                    .map_err(|_| "sequence era")?,
            ),
        })
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MailboxFrontierV1 {
    pub head: u64,
    pub tail: u64,
}
impl MailboxFrontierV1 {
    pub fn canonical_bytes(self) -> Result<[u8; MAILBOX_FRONTIER_BYTES], &'static str> {
        if self.head > self.tail {
            return Err("mailbox head exceeds tail");
        }
        let mut bytes = [0; MAILBOX_FRONTIER_BYTES];
        bytes[..FRONTIER.len()].copy_from_slice(FRONTIER);
        bytes[FRONTIER.len()..FRONTIER.len() + 8].copy_from_slice(&self.head.to_be_bytes());
        bytes[FRONTIER.len() + 8..].copy_from_slice(&self.tail.to_be_bytes());
        Ok(bytes)
    }
    pub fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, &'static str> {
        if bytes.len() != MAILBOX_FRONTIER_BYTES || !bytes.starts_with(FRONTIER) {
            return Err("mailbox frontier domain/length");
        }
        let value = Self {
            head: u64::from_be_bytes(
                bytes[FRONTIER.len()..FRONTIER.len() + 8]
                    .try_into()
                    .map_err(|_| "mailbox head")?,
            ),
            tail: u64::from_be_bytes(
                bytes[FRONTIER.len() + 8..]
                    .try_into()
                    .map_err(|_| "mailbox tail")?,
            ),
        };
        value.canonical_bytes()?;
        Ok(value)
    }
}
/// kind 0 is frontier, kind 1 is one stable ordinal entry. IDs are framed.
pub struct MailboxKeyV1 {
    bytes: [u8; 1024],
    length: usize,
}
impl MailboxKeyV1 {
    pub fn new(agent_id: &str, ordinal: Option<u64>) -> Result<Self, &'static str> {
        let length = KEY.len() + 3 + agent_id.len() + usize::from(ordinal.is_some()) * 8;
        if agent_id.is_empty() || length > 1024 {
            return Err("mailbox key length");
        }
        let mut key = Self {
            bytes: [0; 1024],
            length,
        };
        key.bytes[..KEY.len()].copy_from_slice(KEY);
        key.bytes[KEY.len()] = u8::from(ordinal.is_some());
        key.bytes[KEY.len() + 1..KEY.len() + 3]
            .copy_from_slice(&(agent_id.len() as u16).to_be_bytes());
        key.bytes[KEY.len() + 3..KEY.len() + 3 + agent_id.len()]
            .copy_from_slice(agent_id.as_bytes());
        if let Some(ordinal) = ordinal {
            key.bytes[length - 8..length].copy_from_slice(&ordinal.to_be_bytes());
        }
        Ok(key)
    }
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes[..self.length]
    }
    /// Original agent/position framing, independent of any history qualification.
    pub fn from_bytes(bytes: &[u8]) -> Result<(&str, Option<u64>), &'static str> {
        let payload = bytes.strip_prefix(KEY).ok_or("mailbox key domain")?;
        let header = payload.get(..3).ok_or("mailbox key framing")?;
        let indexed = match header[0] {
            0 => false,
            1 => true,
            _ => return Err("mailbox key kind"),
        };
        let length = usize::from(u16::from_be_bytes(header[1..3].try_into().unwrap()));
        if payload.len() != 3 + length + usize::from(indexed) * 8 {
            return Err("mailbox key length");
        }
        let id = std::str::from_utf8(&payload[3..3 + length]).map_err(|_| "mailbox key UTF-8")?;
        let ordinal =
            indexed.then(|| u64::from_be_bytes(payload[3 + length..].try_into().unwrap()));
        if Self::new(id, ordinal)?.as_bytes() != bytes {
            return Err("mailbox key canonical");
        }
        Ok((id, ordinal))
    }
}
/// Mailbox items reference original WorldEvent facts; delivery reads its Domain
/// payload. The complete original is retained/verified by the history owner.
const MAILBOX_EVENT_DOMAIN_V1: &[u8] = b"oasis7.iws.mailbox-event.v1\0";
pub const MAILBOX_EVENT_REFERENCE_BYTES: usize = MAILBOX_EVENT_DOMAIN_V1.len() + 32;
pub fn mailbox_event_ref_v1(id: [u8; 32]) -> [u8; MAILBOX_EVENT_REFERENCE_BYTES] {
    let mut bytes = [0; MAILBOX_EVENT_REFERENCE_BYTES];
    bytes[..MAILBOX_EVENT_DOMAIN_V1.len()].copy_from_slice(MAILBOX_EVENT_DOMAIN_V1);
    bytes[MAILBOX_EVENT_DOMAIN_V1.len()..].copy_from_slice(&id);
    bytes
}

pub fn mailbox_event_id_v1(bytes: &[u8]) -> Result<[u8; 32], &'static str> {
    if bytes.len() != MAILBOX_EVENT_REFERENCE_BYTES || !bytes.starts_with(MAILBOX_EVENT_DOMAIN_V1) {
        return Err("mailbox event reference domain/length");
    }
    bytes[MAILBOX_EVENT_DOMAIN_V1.len()..]
        .try_into()
        .map_err(|_| "mailbox event reference ID")
}
