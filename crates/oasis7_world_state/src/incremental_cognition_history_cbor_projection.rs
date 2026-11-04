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
}
impl CognitionJournalLifecycleProjectionV1 {
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
    keys: [CognitionOriginalTextProjectionV1; 5],
    selected: Option<usize>,
    active: Option<(usize, u64)>,
    hash: blake3::Hasher,
}
impl Projection {
    pub(super) fn new(enabled: bool) -> Self {
        Self {
            enabled,
            keys: [CognitionOriginalTextProjectionV1::Missing; 5],
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
        } else {
            None
        };
        Ok(())
    }
    pub(super) fn begin(&mut self, depth: usize, initial: u8) {
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
            keys: self.keys,
            transport_attempt: attempt,
        })
    }
}
