//! Original digest projections only. Lifecycle, accepted occurrence, index and
//! cross-record head/sequence relationships remain the History owner's checks.
use super::*;
use crate::incremental_cognition_history::CognitionHistoryFieldV1;
use crate::incremental_cognition_history::CognitionOriginalIdentityStatusV1;

#[derive(Debug)]
pub struct CognitionOriginalSemanticV1 {
    pub(super) id: [u8; 32],
    pub(super) bytes: u64,
    pub(super) class: MemoryClass,
    pub(super) field: CognitionHistoryFieldV1,
    pub(super) identity: CognitionOriginalIdentityStatusV1,
    pub(super) sequence: Option<u64>,
    pub(super) parent: Option<Option<[u8; 32]>>,
    pub(super) journal_head: Option<Option<[u8; 32]>>,
}
impl CognitionOriginalSemanticV1 {
    pub fn id(&self) -> &[u8; 32] {
        &self.id
    }
    pub fn bytes(&self) -> u64 {
        self.bytes
    }
    pub fn class(&self) -> MemoryClass {
        self.class
    }
    pub fn field(&self) -> CognitionHistoryFieldV1 {
        self.field
    }
    pub fn identity_status(&self) -> CognitionOriginalIdentityStatusV1 {
        self.identity
    }
    pub fn sequence(&self) -> Option<u64> {
        self.sequence
    }
    /// Outer None means missing; inner None is the original empty first parent.
    pub fn parent(&self) -> Option<Option<[u8; 32]>> {
        self.parent
    }
    pub fn journal_head(&self) -> Option<Option<[u8; 32]>> {
        self.journal_head
    }
}
pub(super) struct Schema {
    field: CognitionHistoryFieldV1,
    event: [blake3::Hasher; 2],
    payload: [blake3::Hasher; 5],
    artifact: blake3::Hasher,
    active: [bool; 3],
    removed: usize,
    event_removed: usize,
    artifact_available: bool,
    capture: Option<&'static str>,
    captured: [u8; 512],
    captured_len: usize,
    declared: Option<[u8; 32]>,
    payload_declared: Option<[u8; 32]>,
    sequence: Option<u64>,
    parent: Option<Option<[u8; 32]>>,
    journal_head: Option<Option<[u8; 32]>>,
}
fn prefix(domain: &str) -> Result<blake3::Hasher, ValueTreeError> {
    let mut hash = blake3::Hasher::new();
    hash.update(&[0x82]); // Original canonical tuple (domain,payload).
    hash.update(&oasis7_wasm_abi::encode_canonical_cbor(&domain).map_err(|_| invalid())?);
    Ok(hash)
}
pub(super) fn header(major: u8, value: u64) -> ([u8; 9], usize) {
    let mut bytes = [0; 9];
    let width = if value < 24 {
        0
    } else if value <= 255 {
        1
    } else if value <= 65535 {
        2
    } else if value <= u32::MAX as u64 {
        4
    } else {
        8
    };
    bytes[0] = major << 5
        | match width {
            0 => value as u8,
            1 => 24,
            2 => 25,
            4 => 26,
            _ => 27,
        };
    if width != 0 {
        bytes[1..=width].copy_from_slice(&value.to_be_bytes()[8 - width..]);
    }
    (bytes, width + 1)
}
pub(super) fn identity(value: &str) -> Result<Option<[u8; 32]>, ValueTreeError> {
    if value.is_empty() {
        return Ok(None);
    }
    let hex = value.strip_prefix("blake3:").ok_or_else(invalid)?;
    if hex.len() != 64
        || !hex
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(invalid());
    }
    Ok(Some(
        *blake3::Hash::from_hex(hex)
            .map_err(|_| invalid())?
            .as_bytes(),
    ))
}
impl Schema {
    pub(super) fn new(field: CognitionHistoryFieldV1) -> Result<Self, ValueTreeError> {
        let event = prefix(field.domain())?;
        let payload = prefix("oasis7.cognition.event-payload.v1")?;
        Ok(Self {
            field,
            event: std::array::from_fn(|_| event.clone()),
            payload: std::array::from_fn(|_| payload.clone()),
            artifact: prefix(field.domain())?,
            active: [false; 3],
            removed: 0,
            event_removed: 0,
            artifact_available: false,
            capture: None,
            captured: [0; 512],
            captured_len: 0,
            declared: None,
            payload_declared: None,
            sequence: None,
            parent: None,
            journal_head: None,
        })
    }
    pub(super) fn start(&mut self, count: u64) {
        // The count of absent/omitted keys is unknown until the end. Keep a
        // fixed finite set of header hypotheses, never buffer/reorder a map.
        for (removed, hash) in self.event.iter_mut().enumerate() {
            let (bytes, n) = header(5, count.saturating_sub(removed as u64));
            hash.update(&bytes[..n]);
        }
        for (removed, hash) in self.payload.iter_mut().enumerate() {
            let (bytes, n) = header(5, count.saturating_sub(removed as u64));
            hash.update(&bytes[..n]);
        }
    }
    pub(super) fn stop(&mut self) {
        self.active = [false; 3];
        self.capture = None;
    }
    pub(super) fn key(&mut self, key: &[u8]) -> Result<(), ValueTreeError> {
        let response = self.field == CognitionHistoryFieldV1::Responses;
        let event_omitted = key == b"event_digest";
        let payload_omitted = [
            b"journal_seq".as_slice(),
            b"parent_event_digest",
            b"payload_digest",
            b"event_digest",
        ]
        .contains(&key);
        self.event_removed += usize::from(event_omitted);
        self.removed += usize::from(payload_omitted);
        self.active = [
            !response && !event_omitted,
            self.field == CognitionHistoryFieldV1::JournalEvents && !payload_omitted,
            false,
        ];
        let (bytes, n) = header(3, key.len() as u64);
        self.feed(&bytes[..n])?;
        self.feed(key)?;
        self.active[2] = response && key == b"response_artifact";
        self.artifact_available |= self.active[2];
        self.capture = match key {
            b"event_digest" if !response => Some("identity"),
            b"response_digest" if response => Some("identity"),
            b"payload_digest" if self.field == CognitionHistoryFieldV1::JournalEvents => {
                Some("payload")
            }
            b"journal_seq" if !response => Some("sequence"),
            b"parent_event_digest" if !response => Some("parent"),
            b"journal_head" if response => Some("head"),
            _ => None,
        };
        self.captured_len = 0;
        Ok(())
    }
    pub(super) fn first(&mut self, initial: u8) {
        if self.active[2] && initial == 0xf6 {
            self.artifact_available = false;
        }
    }
    pub(super) fn feed(&mut self, bytes: &[u8]) -> Result<(), ValueTreeError> {
        if self.active[0] {
            for hash in &mut self.event {
                hash.update(bytes);
            }
        }
        if self.active[1] {
            for hash in &mut self.payload {
                hash.update(bytes);
            }
        }
        if self.active[2] {
            self.artifact.update(bytes);
        }
        if self.capture.is_some() {
            let end = self
                .captured_len
                .checked_add(bytes.len())
                .ok_or_else(invalid)?;
            if end > self.captured.len() {
                return Err(ValueTreeError::ResourceLimited("Cognition semantic scalar"));
            }
            self.captured[self.captured_len..end].copy_from_slice(bytes);
            self.captured_len = end;
        }
        Ok(())
    }
    pub(super) fn end_value(&mut self) -> Result<(), ValueTreeError> {
        if let Some(name) = self.capture.take() {
            // At most 512 bytes of designated scalar metadata, never a record,
            // response artifact, arbitrary payload or long JSON field.
            let value: serde_json::Value =
                serde_cbor::from_slice(&self.captured[..self.captured_len])
                    .map_err(|_| invalid())?;
            match name {
                "sequence" => self.sequence = Some(value.as_u64().ok_or_else(invalid)?),
                "identity" => self.declared = identity(value.as_str().ok_or_else(invalid)?)?,
                "payload" => {
                    self.payload_declared =
                        Some(identity(value.as_str().ok_or_else(invalid)?)?.ok_or_else(invalid)?)
                }
                "parent" => self.parent = Some(identity(value.as_str().ok_or_else(invalid)?)?),
                "head" => self.journal_head = Some(identity(value.as_str().ok_or_else(invalid)?)?),
                _ => return Err(invalid()),
            }
        }
        Ok(())
    }
    pub(super) fn finish(
        &self,
        id: [u8; 32],
        bytes: u64,
        class: MemoryClass,
    ) -> Result<CognitionOriginalSemanticV1, ValueTreeError> {
        let actual = if self.field == CognitionHistoryFieldV1::Responses {
            self.artifact.finalize()
        } else {
            self.event[self.event_removed].finalize()
        };
        let status = match self.declared {
            None => CognitionOriginalIdentityStatusV1::MissingIdentity,
            Some(_)
                if self.field == CognitionHistoryFieldV1::Responses && !self.artifact_available =>
            {
                CognitionOriginalIdentityStatusV1::ArtifactUnavailable
            }
            Some(expected) => {
                if actual.as_bytes() != &expected {
                    return Err(ValueTreeError::Invalid(
                        "Cognition original semantic digest",
                    ));
                }
                CognitionOriginalIdentityStatusV1::DigestMatched(expected)
            }
        };
        if let Some(expected) = self.payload_declared
            && self.payload[self.removed].finalize().as_bytes() != &expected
        {
            return Err(ValueTreeError::Invalid("Cognition original payload digest"));
        }
        Ok(CognitionOriginalSemanticV1 {
            id,
            bytes,
            class,
            field: self.field,
            identity: status,
            sequence: self.sequence,
            parent: self.parent,
            journal_head: self.journal_head,
        })
    }
}
