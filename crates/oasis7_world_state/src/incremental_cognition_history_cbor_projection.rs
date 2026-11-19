//! Fixed lookup commitments to the ORIGINAL five UTF8 strings. These are not
//! replacement business IDs, membership/absence proofs or a lifecycle grant.
use super::*;
use crate::incremental_cognition_history_lifecycle::COGNITION_LIFECYCLE_KEY_FIELDS_V1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CognitionOriginalTextProjectionV1 {
    Missing,
    Null,
    Text { bytes: u64, utf8_digest: [u8; 32] },
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CognitionJournalLifecycleProjectionV1 {
    keys: [CognitionOriginalTextProjectionV1; 5],
    transport_attempt: u64,
    origin_request_digest: CognitionOriginalTextProjectionV1,
    registration_kind: CognitionOriginalTextProjectionV1,
    admission_keys: [CognitionOriginalTextProjectionV1; 4],
}
impl CognitionJournalLifecycleProjectionV1 {
    /// Exact original top-level kind alias used by turn registration.
    /// Exact original V2 admission details, outside the unchanged lifecycle key.
    pub fn admission_keys_v2(&self) -> &[CognitionOriginalTextProjectionV1; 4] {
        &self.admission_keys
    }
    pub fn registration_kind(&self) -> CognitionOriginalTextProjectionV1 {
        self.registration_kind
    }
    /// Original optional alias; it is not part of the unchanged five-key lifecycle index.
    pub fn origin_request_digest(&self) -> CognitionOriginalTextProjectionV1 {
        self.origin_request_digest
    }
    pub fn keys(&self) -> &[CognitionOriginalTextProjectionV1; 5] {
        &self.keys
    }
    pub fn transport_attempt(&self) -> u64 {
        self.transport_attempt
    }
    /// Exact old sparse gate: no trimming, normalization or new length limit.
    pub fn is_complete(&self) -> bool {
        self.keys.iter().all(|key| matches!(key, CognitionOriginalTextProjectionV1::Text { bytes, .. } if *bytes != 0))
    }
}
pub(super) struct Projection {
    enabled: bool,
    keys: [CognitionOriginalTextProjectionV1; 11],
    selected: Option<usize>,
    active: Option<(usize, u64)>,
    hash: blake3::Hasher,
}
impl Projection {
    pub(super) fn new(enabled: bool) -> Self {
        Self {
            enabled,
            keys: [CognitionOriginalTextProjectionV1::Missing; 11],
            selected: None,
            active: None,
            hash: blake3::Hasher::new(),
        }
    }
    pub(super) fn key(&mut self, key: &[u8]) -> Result<(), ValueTreeError> {
        if self.active.is_some() {
            return Err(invalid());
        }
        self.selected = if self.enabled {
            COGNITION_LIFECYCLE_KEY_FIELDS_V1
                .iter()
                .position(|name| name.as_bytes() == key)
                .or_else(|| (key == b"origin_request_digest").then_some(5))
                .or_else(|| (key == b"kind").then_some(6))
                .or_else(|| {
                    [
                        b"continuation_id".as_slice(),
                        b"wake_id".as_slice(),
                        b"proposal_digest".as_slice(),
                        b"context_core_digest".as_slice(),
                    ]
                    .iter()
                    .position(|name| *name == key)
                    .map(|i| i + 7)
                })
        } else {
            None
        };
        Ok(())
    }
    pub(super) fn begin(&mut self, depth: usize, initial: u8) {
        // The optional alias follows the original JSON as_str semantics:
        // non-text values are not an alias and do not reject an otherwise
        // valid event. Existing five-key lifecycle projection is unchanged.
        if depth == 1
            && let Some(index @ 5..=10) = self.selected
        {
            self.keys[index] = CognitionOriginalTextProjectionV1::Missing;
        }
        if depth == 1
            && initial == 0xf6
            && let Some(index) = self.selected
        {
            self.keys[index] = CognitionOriginalTextProjectionV1::Null;
        }
    }
    pub(super) fn text(&mut self, depth: usize, length: u64) {
        if depth == 1
            && let Some(index) = self.selected
        {
            self.hash.reset();
            self.keys[index] = CognitionOriginalTextProjectionV1::Text {
                bytes: length,
                utf8_digest: [0; 32],
            };
            self.active = Some((index, length));
        }
    }
    pub(super) fn feed(&mut self, bytes: &[u8]) -> Result<(), ValueTreeError> {
        if let Some((_, remaining)) = &mut self.active {
            *remaining = remaining
                .checked_sub(bytes.len() as u64)
                .ok_or_else(invalid)?;
            self.hash.update(bytes);
        }
        Ok(())
    }
    pub(super) fn end(&mut self, depth: usize) -> Result<(), ValueTreeError> {
        if depth == 1
            && let Some((index, remaining)) = self.active.take()
        {
            if remaining != 0 {
                return Err(invalid());
            }
            let CognitionOriginalTextProjectionV1::Text { utf8_digest, .. } = &mut self.keys[index]
            else {
                return Err(invalid());
            };
            *utf8_digest = *self.hash.finalize().as_bytes();
        }
        Ok(())
    }
    pub(super) fn finish(
        &self,
        attempt: u64,
    ) -> Result<CognitionJournalLifecycleProjectionV1, ValueTreeError> {
        if !self.enabled || self.active.is_some() {
            return Err(invalid());
        }
        Ok(CognitionJournalLifecycleProjectionV1 {
            keys: [
                self.keys[0],
                self.keys[1],
                self.keys[2],
                self.keys[3],
                self.keys[4],
            ],
            transport_attempt: attempt,
            origin_request_digest: self.keys[5],
            registration_kind: self.keys[6],
            admission_keys: [self.keys[7], self.keys[8], self.keys[9], self.keys[10]],
        })
    }
}
