//! Closed reference to a schema-owned record's state subtree.
//!
//! The containing value must first be authenticated by its owning reader.
//! Parsing and binding expose an integrity edge, not a schema qualification,
//! business authorization, durable receipt, or accepted head.
use crate::incremental_state::{STATE_ROOT_BYTES, StateNamespaceV1, StateRootV1};

const MAGIC: &[u8] = b"oasis7.iws.record-root.v1\0";
const BODY_BYTES: usize = 64 + STATE_ROOT_BYTES;
pub const RECORD_ROOT_BYTES: usize = MAGIC.len() + 4 + BODY_BYTES;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordRootRefV1 {
    /// The owner defines a frozen schema identity, including its record kind.
    pub schema: [u8; 32],
    /// Original semantic-key digest authenticated by the enclosing record.
    pub owner_key: [u8; 32],
    root: StateRootV1,
}

impl RecordRootRefV1 {
    pub fn new(schema: [u8; 32], owner_key: [u8; 32], root: StateRootV1) -> Result<Self, String> {
        root.validate()?;
        Ok(Self {
            schema,
            owner_key,
            root,
        })
    }

    /// Fixed-size encoding; no input-controlled allocation or opaque tail.
    pub fn canonical_bytes(&self) -> Result<[u8; RECORD_ROOT_BYTES], String> {
        let child = self.root.canonical_bytes()?;
        let mut bytes = [0; RECORD_ROOT_BYTES];
        let start = MAGIC.len();
        bytes[..start].copy_from_slice(MAGIC);
        bytes[start..start + 4].copy_from_slice(&(BODY_BYTES as u32).to_be_bytes());
        bytes[start + 4..start + 36].copy_from_slice(&self.schema);
        bytes[start + 36..start + 68].copy_from_slice(&self.owner_key);
        bytes[start + 68..].copy_from_slice(&child);
        Ok(bytes)
    }

    pub fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() != RECORD_ROOT_BYTES || !bytes.starts_with(MAGIC) {
            return Err("record root format/length".into());
        }
        let body = &bytes[MAGIC.len()..];
        if body[..4] != (BODY_BYTES as u32).to_be_bytes() {
            return Err("record root declared length".into());
        }
        Ok(Self {
            schema: body[4..36].try_into().map_err(|_| "record root schema")?,
            owner_key: body[36..68]
                .try_into()
                .map_err(|_| "record root owner key")?,
            root: StateRootV1::from_canonical_bytes(&body[68..])?,
        })
    }

    /// Expose the typed child edge only after checking the containing owner's
    /// expected schema, original semantic-key digest, and namespace. Transfer
    /// and GC must visit this StateRoot as well as the enclosing value pages.
    /// The owner must separately verify the child's complete schema closure.
    pub fn validate_binding(
        &self,
        schema: &[u8; 32],
        owner_key: &[u8; 32],
        namespace: StateNamespaceV1,
    ) -> Result<&StateRootV1, String> {
        if &self.schema != schema
            || &self.owner_key != owner_key
            || self.root.namespace != namespace
        {
            return Err("record root owner/schema/namespace binding".into());
        }
        self.root.validate()?;
        Ok(&self.root)
    }
}

#[cfg(test)]
#[path = "incremental_record_root_tests.rs"]
mod tests;
