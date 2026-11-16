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
        self.canonical_bytes_with_work(memory, &mut |_, _| Ok(()))
    }
    pub fn canonical_bytes_with_work(
        &self,
        memory: &MemoryBudget,
        work: &mut dyn FnMut(u64, u64) -> Result<(), ExecutionError>,
    ) -> Result<ReservedValue<Vec<u8>>, ExecutionError> {
        work(1, self.scope.world_id.len() as u64)?;
        self.scope.validate()?;
        if self.facts.len() > crate::incremental_execution::EXECUTION_MAX_FACTS {
            return Err(ExecutionError::Invalid("history facts count"));
        }
        let mut workspace =
            memory.reserve(64 * 1024 + self.facts.len() * 128, MemoryClass::Ordinary)?;
        let work = std::cell::RefCell::new(work);
        let meter = |s, b| (work.borrow_mut())(s, b);
        let mut out = Writer::with_work(&meter);
        out.scope(&self.scope);
        out.hash(&self.operation);
        out.u16(self.facts.len() as u16);
        for fact in &self.facts {
            out.u8(fact.kind as u8);
            out.hash(&fact.id);
            out.check()?;
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
        Self::from_canonical_bytes_in_class(bytes, memory, MemoryClass::Ordinary)
    }
    /// Decode the same wire with the caller's explicit recovery admission.
    pub fn from_canonical_bytes_in_class(
        bytes: &[u8],
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<ReservedValue<Self>, ExecutionError> {
        let mut r = Reader::new(bytes, b"oasis7.iws.fact-set.v1\0", FACTS_MAX_BYTES)?;
        let mut workspace = memory.reserve(64 * 1024 + 2 * bytes.len(), class)?;
        let scope = r.scope()?;
        let operation = r.array()?;
        let count = usize::from(r.u16()?);
        if count > crate::incremental_execution::EXECUTION_MAX_FACTS {
            return Err(ExecutionError::Invalid("fact decode count"));
        }
        let original = r.take(count * 33)?;
        r.finish()?;
        let mut extra = memory.reserve(count * std::mem::size_of::<ExecutionFactRefV1>(), class)?;
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
        self.prepare_with_work(memory, &mut |_, _| Ok(()))
    }
    pub fn prepare_with_work(
        &self,
        memory: &MemoryBudget,
        work: &mut dyn FnMut(u64, u64) -> Result<(), ExecutionError>,
    ) -> Result<ReservedValue<PreparedHistoryV1>, ExecutionError> {
        work(1, 0)?;
        let _fixed = memory.reserve(64 * 1024, MemoryClass::Ordinary)?;
        let scope = self.config.scope_with_work(work)?;
        pay_roots(work, self.roots)?;
        self.roots.validate()?;
        self.catalog.validate_with_work(work)?;
        work(
            1,
            (scope.world_id.len() * 2
                + self.roots.world_id.len()
                + self.catalog.scope.world_id.len()) as u64,
        )?;
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
        let parent_commit = self.parent.map(|p| p.digest_with_work(work)).transpose()?;
        if parent_commit != self.catalog.last_commit {
            return Err(ExecutionError::ParentConflict);
        }
        if let Some(parent) = self.parent {
            parent.validate_with_work(work)?;
            work(
                1,
                (parent.scope.world_id.len() + scope.world_id.len()) as u64,
            )?;
            if parent.scope != scope || parent.height.checked_add(1) != Some(self.roots.height) {
                return Err(ExecutionError::Invalid("parent body height/scope"));
            }
            let previous = self
                .parent_history
                .ok_or(ExecutionError::Invalid("parent prefix missing"))?;
            work(
                1,
                (previous.scope.world_id.len() + scope.world_id.len()) as u64,
            )?;
            previous.validate()?;
            if previous.scope != scope
                || previous.count != self.roots.height
                || previous.digest_with_work(work)? != parent.history
            {
                return Err(ExecutionError::Invalid("parent history binding"));
            }
        } else if self.execution_environment != self.config.execution_environment {
            return Err(ExecutionError::Invalid("genesis execution environment"));
        }
        let facts = if let Some(record) = self.transition {
            record.validate_with_work(work)?;
            work(
                1,
                (record.after.world_id.len() + self.roots.world_id.len() + 12 * 64) as u64,
            )?;
            if !self.genesis_facts.is_empty()
                || record.after != *self.roots
                || record.parent_commit
                    != parent_commit.ok_or(ExecutionError::Invalid("transition parent missing"))?
                || record.operation != self.operation
                || record.authority != self.authority
                || Some(roots_digest(work, &record.before)?)
                    != self.parent.map(|parent| parent.roots)
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
        work(
            facts.len() as u64 + 1,
            (scope.world_id.len() + std::mem::size_of_val(facts)) as u64,
        )?;
        let fact_set = FactSetV1 {
            scope: scope.clone(),
            operation: self.operation,
            facts: facts.to_vec(),
        };
        let facts_bytes = fact_set.canonical_bytes_with_work(memory, work)?;
        work(1, facts_bytes.len() as u64)?;
        let facts_id = digest(&facts_bytes);
        drop(facts_bytes);
        let transition_bytes = self
            .transition
            .map(|record| record.canonical_bytes_with_work(memory, MemoryClass::Ordinary, work))
            .transpose()?;
        let transition_id = transition_bytes
            .as_ref()
            .map(|bytes| {
                work(1, bytes.len() as u64)?;
                Ok::<_, ExecutionError>(digest(bytes))
            })
            .transpose()?;
        let roots = roots_digest(work, self.roots)?;
        work(1, scope.world_id.len() as u64)?;
        let segment = HistorySegmentV1 {
            scope: scope.clone(),
            height: self.roots.height,
            parent_commit,
            operation: self.operation,
            before_roots: self
                .transition
                .map(|record| roots_digest(work, &record.before))
                .transpose()?,
            after_roots: roots,
            transition: transition_id,
            facts: facts_id,
        };
        work(1, scope.world_id.len() as u64)?;
        let prefix = HistoryPrefixV1 {
            scope: scope.clone(),
            count,
            previous: self
                .parent_history
                .map(|p| p.digest_with_work(work))
                .transpose()?,
            segment: segment.digest_with_work(work)?,
        };
        let history = prefix.digest_with_work(work)?;
        if self.checkpoint {
            work(1, scope.world_id.len() as u64)?;
        }
        let checkpoint = self.checkpoint.then(|| CheckpointDescriptorV1 {
            scope: scope.clone(),
            height: self.roots.height,
            parent_commit,
            roots,
            history,
            format_profile: self.config.format_profile,
            execution_environment: self.execution_environment,
        });
        work(
            1,
            (self.catalog.scope.world_id.len() + self.catalog.node_root.len()) as u64,
        )?;
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
                .map(|p| p.digest_with_work(work))
                .transpose()?,
            execution_environment: self.execution_environment,
        };
        body.validate_with_work(work)?;
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

// RootSet has exactly six fixed namespace headers. Count their actual stored
// roots and scope before using the one original canonical encoder/hash.
fn pay_roots(
    work: &mut dyn FnMut(u64, u64) -> Result<(), ExecutionError>,
    roots: &RootSetV1,
) -> Result<(), ExecutionError> {
    work(1, roots.world_id.len() as u64)?;
    for root in &roots.state {
        work(1, root.node_root.len() as u64)?;
    }
    Ok(())
}
fn roots_digest(
    work: &mut dyn FnMut(u64, u64) -> Result<(), ExecutionError>,
    roots: &RootSetV1,
) -> Result<[u8; 32], ExecutionError> {
    pay_roots(work, roots)?;
    let n = b"oasis7.iws.root-set.v1\0".len()
        + 4
        + 2
        + roots.world_id.len()
        + 48
        + 6 * crate::incremental_state::STATE_ROOT_BYTES;
    work(
        13,
        (n * 2 + 6 * crate::incremental_state::STATE_ROOT_BYTES) as u64,
    )?;
    roots.digest()
}
#[cfg(test)]
mod metered_tests {
    use super::*;
    struct Empty;
    impl MerkleNodeReader for Empty {
        type Node = Vec<u8>;
        fn read_node(&self, _: &str, _: usize) -> Result<Option<Vec<u8>>, String> {
            Ok(None)
        }
    }
    #[test]
    fn metered_preparation_and_catalog_keep_identical_canonical_candidates() {
        let memory = MemoryBudget::new(32 << 20, 0).unwrap();
        let config = GenesisConfigV1 {
            world_id: "世界-work".into(),
            format_profile: [1; 32],
            initial_content: [2; 32],
            activation_config: [3; 32],
            execution_environment: [4; 32],
        };
        let scope = config.scope().unwrap();
        let catalog = CatalogRootV1::empty(scope).unwrap();
        let roots = RootSetV1::empty(config.world_id.clone(), config.digest().unwrap()).unwrap();
        let facts = [ExecutionFactRefV1 {
            kind: ExecutionFactKindV1::Input,
            id: [8; 32],
        }];
        let header = HeaderPreparation {
            config: &config,
            catalog: &catalog,
            roots: &roots,
            parent: None,
            parent_history: None,
            transition: None,
            genesis_facts: &facts,
            operation: [5; 32],
            authority: AuthorityBinding {
                token: [6; 32],
                epoch: 0,
            },
            execution_environment: config.execution_environment,
            checkpoint: true,
        };
        let original = header.prepare(&memory).unwrap();
        let (mut steps, mut bytes) = (0, 0);
        let paid = header
            .prepare_with_work(&memory, &mut |s, b| {
                steps += s;
                bytes += b;
                Ok(())
            })
            .unwrap();
        assert_eq!(paid.body, original.body);
        assert_eq!(paid.facts, original.facts);
        assert_eq!(paid.prefix, original.prefix);
        assert_eq!(paid.segment, original.segment);
        assert_eq!(paid.checkpoint, original.checkpoint);
        assert!(steps > facts.len() as u64 && bytes > 0);
        let ordinary = catalog
            .extend(&original.body, &Empty, &memory, 2 << 20)
            .unwrap();
        let metered = catalog
            .extend_with_work(&paid.body, &Empty, &memory, 2 << 20, &mut |s, b| {
                steps += s;
                bytes += b;
                Ok(())
            })
            .unwrap();
        assert_eq!(*ordinary, *metered);
        assert_eq!(
            ordinary.after.canonical_bytes().unwrap(),
            metered
                .after
                .canonical_bytes_with_work(&mut |_, _| Ok(()))
                .unwrap()
        );
    }
    #[test]
    fn first_work_failure_stops_before_header_or_catalog_io() {
        let memory = MemoryBudget::new(8 << 20, 0).unwrap();
        let config = GenesisConfigV1 {
            world_id: "work".into(),
            format_profile: [1; 32],
            initial_content: [2; 32],
            activation_config: [3; 32],
            execution_environment: [4; 32],
        };
        let catalog = CatalogRootV1::empty(config.scope().unwrap()).unwrap();
        let roots = RootSetV1::empty(config.world_id.clone(), config.digest().unwrap()).unwrap();
        let header = HeaderPreparation {
            config: &config,
            catalog: &catalog,
            roots: &roots,
            parent: None,
            parent_history: None,
            transition: None,
            genesis_facts: &[],
            operation: [5; 32],
            authority: AuthorityBinding {
                token: [6; 32],
                epoch: 0,
            },
            execution_environment: config.execution_environment,
            checkpoint: false,
        };
        let mut calls = 0;
        let result = header.prepare_with_work(&memory, &mut |_, _| {
            calls += 1;
            Err(ExecutionError::Invalid("typed work first"))
        });
        assert!(matches!(
            result,
            Err(ExecutionError::Invalid("typed work first"))
        ));
        assert_eq!(calls, 1);
        assert_eq!(memory.used(), 0);
        let mut calls = 0;
        let result = catalog.lookup_with_work(CatalogQueryV1::Height(0), &Empty, &mut |_, _| {
            calls += 1;
            Err(ExecutionError::Invalid("typed tree first"))
        });
        assert!(matches!(
            result,
            Err(ExecutionError::Invalid("typed tree first"))
        ));
        assert_eq!(calls, 1);
    }
}
