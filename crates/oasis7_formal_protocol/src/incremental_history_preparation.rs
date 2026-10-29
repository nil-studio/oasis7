use super::*;
use wire::{Reader, Writer, digest};

const FACTS_MAX_BYTES: usize = 256 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FactSetV1 {
    pub scope: WorldScopeV1,
    pub operation: [u8; 32],
    pub facts: Vec<ExecutionFactRefV1>,
}
impl FactSetV1 {
    pub fn canonical_bytes(
        &self,
        memory: &MemoryBudget,
    ) -> Result<ReservedValue<Vec<u8>>, ExecutionError> {
        self.scope.validate()?;
        if self.facts.len() > crate::incremental_execution::EXECUTION_MAX_FACTS {
            return Err(ExecutionError::Invalid("history facts count"));
        }
        let mut workspace =
            memory.reserve(64 * 1024 + self.facts.len() * 128, MemoryClass::Ordinary)?;
        let mut out = Writer::new();
        out.scope(&self.scope);
        out.hash(&self.operation);
        out.u16(self.facts.len() as u16);
        for fact in &self.facts {
            out.u8(fact.kind as u8);
            out.hash(&fact.id);
        }
        let bytes = out.finish(b"oasis7.iws.fact-set.v1\0", FACTS_MAX_BYTES)?;
        let charge = workspace
            .split(bytes.capacity())
            .map_err(|_| ExecutionError::Invalid("fact encoding accounting"))?;
        Ok(charge.retain(bytes))
    }
    pub fn from_canonical_bytes(
        bytes: &[u8],
        memory: &MemoryBudget,
    ) -> Result<ReservedValue<Self>, ExecutionError> {
        let mut r = Reader::new(bytes, b"oasis7.iws.fact-set.v1\0", FACTS_MAX_BYTES)?;
        let mut workspace = memory.reserve(64 * 1024 + 2 * bytes.len(), MemoryClass::Ordinary)?;
        let scope = r.scope()?;
        let operation = r.array()?;
        let count = usize::from(r.u16()?);
        if count > crate::incremental_execution::EXECUTION_MAX_FACTS {
            return Err(ExecutionError::Invalid("fact decode count"));
        }
        let original = r.take(count * 33)?;
        r.finish()?;
        let mut extra = memory.reserve(
            count * std::mem::size_of::<ExecutionFactRefV1>(),
            MemoryClass::Ordinary,
        )?;
        workspace
            .merge(&mut extra)
            .map_err(|_| ExecutionError::Invalid("fact decode pool"))?;
        let mut facts = Vec::with_capacity(count);
        for entry in original.chunks_exact(33) {
            let kind = match entry[0] {
                1 => ExecutionFactKindV1::Input,
                2 => ExecutionFactKindV1::Result,
                3 => ExecutionFactKindV1::Event,
                _ => return Err(ExecutionError::Invalid("fact decode type")),
            };
            facts.push(ExecutionFactRefV1 {
                kind,
                id: entry[1..]
                    .try_into()
                    .map_err(|_| ExecutionError::Invalid("fact digest width"))?,
            });
        }
        let value = Self {
            scope,
            operation,
            facts,
        };
        let bytes = std::mem::size_of::<Self>()
            + value.scope.world_id.capacity()
            + value.facts.capacity() * std::mem::size_of::<ExecutionFactRefV1>()
            + 64;
        let charge = workspace
            .split(bytes)
            .map_err(|_| ExecutionError::Invalid("fact decode accounting"))?;
        Ok(charge.retain(value))
    }
}

/// Parent objects and catalog must already be authenticated by the original
/// authority/root owner. This prepares content, never an acceptance receipt.
pub struct HeaderPreparation<'a> {
    pub config: &'a GenesisConfigV1,
    pub catalog: &'a CatalogRootV1,
    pub roots: &'a RootSetV1,
    pub parent: Option<&'a CommitBodyV1>,
    pub parent_history: Option<&'a HistoryPrefixV1>,
    pub transition: Option<&'a TransitionRecordV1>,
    pub genesis_facts: &'a [ExecutionFactRefV1],
    pub operation: [u8; 32],
    pub authority: AuthorityBinding,
    pub execution_environment: [u8; 32],
    pub checkpoint: bool,
}

#[derive(Debug)]
pub struct PreparedHistoryV1 {
    pub facts: FactSetV1,
    pub segment: HistorySegmentV1,
    pub prefix: HistoryPrefixV1,
    pub checkpoint: Option<CheckpointDescriptorV1>,
    pub body: CommitBodyV1,
    pub transition_bytes: Option<ReservedValue<Vec<u8>>>,
}

impl HeaderPreparation<'_> {
    pub fn prepare(
        &self,
        memory: &MemoryBudget,
    ) -> Result<ReservedValue<PreparedHistoryV1>, ExecutionError> {
        let _fixed = memory.reserve(64 * 1024, MemoryClass::Ordinary)?;
        let scope = self.config.scope()?;
        self.roots.validate()?;
        self.catalog.validate()?;
        if !scope.matches(self.roots)
            || self.catalog.scope != scope
            || self.catalog.count != self.roots.height
        {
            return Err(ExecutionError::Invalid(
                "header world/genesis/height binding",
            ));
        }
        let count = self
            .catalog
            .count
            .checked_add(1)
            .ok_or(ExecutionError::ResourceLimited("history count overflow"))?;
        let genesis = self.roots.height == 0;
        if self.parent.is_none() != genesis
            || self.parent_history.is_none() != genesis
            || self.transition.is_none() != genesis
        {
            return Err(ExecutionError::Invalid("header genesis/parent materials"));
        }
        let parent_commit = self.parent.map(CommitBodyV1::digest).transpose()?;
        if parent_commit != self.catalog.last_commit {
            return Err(ExecutionError::ParentConflict);
        }
        if let Some(parent) = self.parent {
            parent.validate()?;
            if parent.scope != scope || parent.height.checked_add(1) != Some(self.roots.height) {
                return Err(ExecutionError::Invalid("parent body height/scope"));
            }
            let previous = self
                .parent_history
                .ok_or(ExecutionError::Invalid("parent prefix missing"))?;
            previous.validate()?;
            if previous.scope != scope
                || previous.count != self.roots.height
                || previous.digest()? != parent.history
            {
                return Err(ExecutionError::Invalid("parent history binding"));
            }
        } else if self.execution_environment != self.config.execution_environment {
            return Err(ExecutionError::Invalid("genesis execution environment"));
        }
        let facts = if let Some(record) = self.transition {
            record.validate()?;
            if !self.genesis_facts.is_empty()
                || record.after != *self.roots
                || record.parent_commit
                    != parent_commit.ok_or(ExecutionError::Invalid("transition parent missing"))?
                || record.operation != self.operation
                || record.authority != self.authority
                || Some(record.before.digest()?) != self.parent.map(|parent| parent.roots)
            {
                return Err(ExecutionError::Invalid("transition/header binding"));
            }
            &record.facts[..]
        } else {
            self.genesis_facts
        };
        if facts.len() > crate::incremental_execution::EXECUTION_MAX_FACTS {
            return Err(ExecutionError::Invalid("header fact bound"));
        }
        let mut workspace = memory.reserve(64 * 1024 + facts.len() * 128, MemoryClass::Ordinary)?;
        let fact_set = FactSetV1 {
            scope: scope.clone(),
            operation: self.operation,
            facts: facts.to_vec(),
        };
        let facts_id = digest(&fact_set.canonical_bytes(memory)?);
        let transition_bytes = self
            .transition
            .map(|record| record.canonical_bytes(memory, MemoryClass::Ordinary))
            .transpose()?;
        let transition_id = transition_bytes.as_ref().map(|bytes| digest(bytes));
        let roots = self.roots.digest()?;
        let segment = HistorySegmentV1 {
            scope: scope.clone(),
            height: self.roots.height,
            parent_commit,
            operation: self.operation,
            before_roots: self
                .transition
                .map(|record| record.before.digest())
                .transpose()?,
            after_roots: roots,
            transition: transition_id,
            facts: facts_id,
        };
        let prefix = HistoryPrefixV1 {
            scope: scope.clone(),
            count,
            previous: self
                .parent_history
                .map(HistoryPrefixV1::digest)
                .transpose()?,
            segment: segment.digest()?,
        };
        let history = prefix.digest()?;
        let checkpoint = self.checkpoint.then(|| CheckpointDescriptorV1 {
            scope: scope.clone(),
            height: self.roots.height,
            parent_commit,
            roots,
            history,
            format_profile: self.config.format_profile,
            execution_environment: self.execution_environment,
        });
        let body = CommitBodyV1 {
            scope,
            height: self.roots.height,
            operation: self.operation,
            parent_commit,
            authority: self.authority,
            catalog_before: self.catalog.clone(),
            roots,
            transition: transition_id,
            history,
            facts: facts_id,
            checkpoint: checkpoint
                .as_ref()
                .map(CheckpointDescriptorV1::digest)
                .transpose()?,
            execution_environment: self.execution_environment,
        };
        body.validate()?;
        let value = PreparedHistoryV1 {
            facts: fact_set,
            segment,
            prefix,
            checkpoint,
            body,
            transition_bytes,
        };
        let retained = std::mem::size_of::<PreparedHistoryV1>()
            + 64
            + value.facts.scope.world_id.capacity()
            + value.facts.facts.capacity() * std::mem::size_of::<ExecutionFactRefV1>()
            + value.segment.scope.world_id.capacity()
            + value.prefix.scope.world_id.capacity()
            + value.body.scope.world_id.capacity()
            + value.body.catalog_before.scope.world_id.capacity()
            + value.body.catalog_before.node_root.capacity()
            + value
                .checkpoint
                .as_ref()
                .map_or(0, |cp| cp.scope.world_id.capacity());
        let charge = workspace
            .split(retained)
            .map_err(|_| ExecutionError::Invalid("header retained accounting"))?;
        Ok(charge.retain(value))
    }
}
