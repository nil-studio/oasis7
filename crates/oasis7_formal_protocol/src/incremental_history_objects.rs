use super::*;
use wire::{Reader, Writer, digest};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenesisConfigV1 {
    pub world_id: String,
    pub format_profile: [u8; 32],
    pub initial_content: [u8; 32],
    pub activation_config: [u8; 32],
    pub execution_environment: [u8; 32],
}
impl GenesisConfigV1 {
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, ExecutionError> {
        if self.world_id.is_empty() || self.world_id.len() > 256 {
            return Err(ExecutionError::Invalid("genesis world ID bound"));
        }
        let mut out = Writer::new();
        out.string(&self.world_id);
        out.hash(&self.format_profile);
        out.hash(&self.initial_content);
        out.hash(&self.activation_config);
        out.hash(&self.execution_environment);
        out.finish(b"oasis7.iws.genesis-config.v1\0", HISTORY_MAX_HEADER_BYTES)
    }
    pub fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, ExecutionError> {
        let mut r = Reader::new(
            bytes,
            b"oasis7.iws.genesis-config.v1\0",
            HISTORY_MAX_HEADER_BYTES,
        )?;
        let value = Self {
            world_id: r.string()?,
            format_profile: r.array()?,
            initial_content: r.array()?,
            activation_config: r.array()?,
            execution_environment: r.array()?,
        };
        r.finish()?;
        Ok(value)
    }
    pub fn digest(&self) -> Result<[u8; 32], ExecutionError> {
        Ok(digest(&self.canonical_bytes()?))
    }
    pub fn scope(&self) -> Result<WorldScopeV1, ExecutionError> {
        Ok(WorldScopeV1 {
            world_id: self.world_id.clone(),
            genesis_config: self.digest()?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistorySegmentV1 {
    pub scope: WorldScopeV1,
    pub height: u64,
    pub parent_commit: Option<[u8; 32]>,
    pub operation: [u8; 32],
    pub before_roots: Option<[u8; 32]>,
    pub after_roots: [u8; 32],
    pub transition: Option<[u8; 32]>,
    pub facts: [u8; 32],
}
impl HistorySegmentV1 {
    pub fn validate(&self) -> Result<(), ExecutionError> {
        self.scope.validate()?;
        let genesis = self.height == 0;
        if self.parent_commit.is_none() != genesis
            || self.before_roots.is_none() != genesis
            || self.transition.is_none() != genesis
        {
            return Err(ExecutionError::Invalid(
                "segment genesis/transition boundary",
            ));
        }
        Ok(())
    }
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, ExecutionError> {
        self.validate()?;
        let mut out = Writer::new();
        out.scope(&self.scope);
        out.u64(self.height);
        out.optional(&self.parent_commit);
        out.hash(&self.operation);
        out.optional(&self.before_roots);
        out.hash(&self.after_roots);
        out.optional(&self.transition);
        out.hash(&self.facts);
        out.finish(b"oasis7.iws.history-segment.v1\0", HISTORY_MAX_HEADER_BYTES)
    }
    pub fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, ExecutionError> {
        let mut r = Reader::new(
            bytes,
            b"oasis7.iws.history-segment.v1\0",
            HISTORY_MAX_HEADER_BYTES,
        )?;
        let value = Self {
            scope: r.scope()?,
            height: r.u64()?,
            parent_commit: r.optional()?,
            operation: r.array()?,
            before_roots: r.optional()?,
            after_roots: r.array()?,
            transition: r.optional()?,
            facts: r.array()?,
        };
        r.finish()?;
        value.validate()?;
        Ok(value)
    }
    pub fn digest(&self) -> Result<[u8; 32], ExecutionError> {
        Ok(digest(&self.canonical_bytes()?))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryPrefixV1 {
    pub scope: WorldScopeV1,
    pub count: u64,
    pub previous: Option<[u8; 32]>,
    pub segment: [u8; 32],
}
impl HistoryPrefixV1 {
    pub fn validate(&self) -> Result<(), ExecutionError> {
        self.scope.validate()?;
        if self.count == 0 || self.previous.is_none() != (self.count == 1) {
            return Err(ExecutionError::Invalid("history prefix count/parent"));
        }
        Ok(())
    }
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, ExecutionError> {
        self.validate()?;
        let mut out = Writer::new();
        out.scope(&self.scope);
        out.u64(self.count);
        out.optional(&self.previous);
        out.hash(&self.segment);
        out.finish(b"oasis7.iws.history-prefix.v1\0", HISTORY_MAX_HEADER_BYTES)
    }
    pub fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, ExecutionError> {
        let mut r = Reader::new(
            bytes,
            b"oasis7.iws.history-prefix.v1\0",
            HISTORY_MAX_HEADER_BYTES,
        )?;
        let value = Self {
            scope: r.scope()?,
            count: r.u64()?,
            previous: r.optional()?,
            segment: r.array()?,
        };
        r.finish()?;
        value.validate()?;
        Ok(value)
    }
    pub fn digest(&self) -> Result<[u8; 32], ExecutionError> {
        Ok(digest(&self.canonical_bytes()?))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CheckpointPointerV1 {
    pub height: u64,
    pub descriptor: [u8; 32],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckpointDescriptorV1 {
    pub scope: WorldScopeV1,
    pub height: u64,
    pub parent_commit: Option<[u8; 32]>,
    pub roots: [u8; 32],
    pub history: [u8; 32],
    pub format_profile: [u8; 32],
    pub execution_environment: [u8; 32],
}
impl CheckpointDescriptorV1 {
    pub fn validate(&self) -> Result<(), ExecutionError> {
        self.scope.validate()?;
        if self.parent_commit.is_none() != (self.height == 0) {
            return Err(ExecutionError::Invalid("checkpoint parent boundary"));
        }
        Ok(())
    }
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, ExecutionError> {
        self.validate()?;
        let mut out = Writer::new();
        out.scope(&self.scope);
        out.u64(self.height);
        out.optional(&self.parent_commit);
        out.hash(&self.roots);
        out.hash(&self.history);
        out.hash(&self.format_profile);
        out.hash(&self.execution_environment);
        out.finish(b"oasis7.iws.checkpoint.v1\0", HISTORY_MAX_HEADER_BYTES)
    }
    pub fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, ExecutionError> {
        let mut r = Reader::new(
            bytes,
            b"oasis7.iws.checkpoint.v1\0",
            HISTORY_MAX_HEADER_BYTES,
        )?;
        let value = Self {
            scope: r.scope()?,
            height: r.u64()?,
            parent_commit: r.optional()?,
            roots: r.array()?,
            history: r.array()?,
            format_profile: r.array()?,
            execution_environment: r.array()?,
        };
        r.finish()?;
        value.validate()?;
        Ok(value)
    }
    pub fn digest(&self) -> Result<[u8; 32], ExecutionError> {
        Ok(digest(&self.canonical_bytes()?))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitBodyV1 {
    pub scope: WorldScopeV1,
    pub height: u64,
    pub operation: [u8; 32],
    pub parent_commit: Option<[u8; 32]>,
    pub authority: AuthorityBinding,
    pub catalog_before: CatalogRootV1,
    pub roots: [u8; 32],
    pub transition: Option<[u8; 32]>,
    pub history: [u8; 32],
    pub facts: [u8; 32],
    pub checkpoint: Option<[u8; 32]>,
    pub execution_environment: [u8; 32],
}
impl CommitBodyV1 {
    pub fn validate(&self) -> Result<(), ExecutionError> {
        self.scope.validate()?;
        self.catalog_before.validate()?;
        if self.scope != self.catalog_before.scope
            || self.height != self.catalog_before.count
            || self.parent_commit != self.catalog_before.last_commit
            || self.transition.is_none() != (self.height == 0)
        {
            return Err(ExecutionError::Invalid(
                "body parent/catalog/transition boundary",
            ));
        }
        Ok(())
    }
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, ExecutionError> {
        self.validate()?;
        let mut out = Writer::new();
        out.scope(&self.scope);
        out.u64(self.height);
        out.hash(&self.operation);
        out.optional(&self.parent_commit);
        out.hash(&self.authority.token);
        out.u64(self.authority.epoch);
        out.nested(&self.catalog_before.canonical_bytes()?);
        out.hash(&self.roots);
        out.optional(&self.transition);
        out.hash(&self.history);
        out.hash(&self.facts);
        out.optional(&self.checkpoint);
        out.hash(&self.execution_environment);
        out.finish(b"oasis7.iws.commit-body.v1\0", HISTORY_MAX_HEADER_BYTES)
    }
    pub fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, ExecutionError> {
        let mut r = Reader::new(
            bytes,
            b"oasis7.iws.commit-body.v1\0",
            HISTORY_MAX_HEADER_BYTES,
        )?;
        let value = Self {
            scope: r.scope()?,
            height: r.u64()?,
            operation: r.array()?,
            parent_commit: r.optional()?,
            authority: AuthorityBinding {
                token: r.array()?,
                epoch: r.u64()?,
            },
            catalog_before: CatalogRootV1::from_canonical_bytes(r.nested()?)?,
            roots: r.array()?,
            transition: r.optional()?,
            history: r.array()?,
            facts: r.array()?,
            checkpoint: r.optional()?,
            execution_environment: r.array()?,
        };
        r.finish()?;
        value.validate()?;
        Ok(value)
    }
    pub fn digest(&self) -> Result<[u8; 32], ExecutionError> {
        Ok(digest(&self.canonical_bytes()?))
    }
}
