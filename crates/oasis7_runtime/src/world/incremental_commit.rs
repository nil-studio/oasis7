//! Runtime driver preparation, not head selection or authority qualification.
//! One fixed parent and one transaction produce the complete candidate content.
use super::World;
use oasis7_formal_protocol::{
    incremental_execution::{
        AuthorityBinding, ExecutionError, ExecutionFence, ExecutionParent, ExecutionTransaction,
        PreparedTransition, RootSetV1, TransactionLimits,
    },
    incremental_history::{
        CatalogDeltaV1, CatalogQueryV1, CatalogRootV1, CheckpointPointerV1, CommitBodyV1,
        GenesisConfigV1, HeaderPreparation, HistoryPrefixV1, PreparedHistoryV1,
    },
    incremental_state::MerkleNodeReader,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};

/// The authority/root owner supplies authenticated original parent materials.
/// Structural validation below cannot turn arbitrary bytes into that authority.
pub struct ParentCommitView<'a> {
    pub config: &'a GenesisConfigV1,
    pub roots: &'a RootSetV1,
    pub body: &'a CommitBodyV1,
    pub history: &'a HistoryPrefixV1,
    pub catalog: &'a CatalogRootV1,
    pub authority: AuthorityBinding,
}
pub struct CommitPreparationRequest<'a> {
    pub parent: ParentCommitView<'a>,
    pub operation: [u8; 32],
    pub next_tick: u64,
    pub execution_environment: [u8; 32],
    pub checkpoint: bool,
    pub limits: TransactionLimits,
}

/// Immutable candidate. No certificate, accepted receipt, selected head or
/// cache-install permission is issued by constructing or persisting this value.
#[derive(Debug)]
pub struct PreparedRuntimeCommit {
    transition: ReservedValue<PreparedTransition>,
    history: ReservedValue<PreparedHistoryV1>,
    catalog: ReservedValue<CatalogDeltaV1>,
    root_set_bytes: ReservedValue<Vec<u8>>,
    catalog_bytes: ReservedValue<Vec<u8>>,
    fence: ExecutionFence,
}
impl PreparedRuntimeCommit {
    pub fn transition(&self) -> &ReservedValue<PreparedTransition> {
        &self.transition
    }
    pub fn history(&self) -> &ReservedValue<PreparedHistoryV1> {
        &self.history
    }
    pub fn catalog(&self) -> &ReservedValue<CatalogDeltaV1> {
        &self.catalog
    }
    pub fn root_set_bytes(&self) -> &ReservedValue<Vec<u8>> {
        &self.root_set_bytes
    }
    pub fn catalog_bytes(&self) -> &ReservedValue<Vec<u8>> {
        &self.catalog_bytes
    }
    pub fn commit_id(&self) -> [u8; 32] {
        self.catalog.entry.commit
    }
    pub fn expected_fence(&self) -> ExecutionFence {
        self.fence
    }
    /// Recheck immediately at each later protocol/publication boundary too.
    pub fn validate_live_fence(&self, live: &ExecutionFence) -> Result<(), ExecutionError> {
        if self.fence == *live {
            Ok(())
        } else {
            Err(ExecutionError::ParentConflict)
        }
    }
}

impl ParentCommitView<'_> {
    fn validate(
        &self,
        source: &impl MerkleNodeReader,
        memory: &MemoryBudget,
        max_pending_bytes: usize,
    ) -> Result<[u8; 32], ExecutionError> {
        let scope = self.config.scope()?;
        self.roots.validate()?;
        self.body.validate()?;
        self.history.validate()?;
        self.catalog.validate()?;
        let commit = self.body.digest()?;
        if self.body.scope != scope
            || self.catalog.scope != scope
            || self.history.scope != scope
            || !scope.matches(self.roots)
            || self.body.height != self.roots.height
            || self.body.roots != self.roots.digest()?
            || self.history.digest()? != self.body.history
            || self.roots.height.checked_add(1) != Some(self.catalog.count)
            || self.history.count != self.catalog.count
            || self.catalog.last_commit != Some(commit)
        {
            return Err(ExecutionError::Invalid("driver parent material binding"));
        }
        // The parent certificate binds catalog-before, not an arbitrary local
        // catalog-after. Reproduce its exact extension before any runtime work.
        let extension =
            self.body
                .catalog_before
                .extend(self.body, source, memory, max_pending_bytes)?;
        if extension.after != *self.catalog {
            return Err(ExecutionError::Invalid("driver parent catalog extension"));
        }
        drop(extension);
        let tail = self
            .catalog
            .lookup(CatalogQueryV1::Height(self.roots.height), source)?
            .ok_or(ExecutionError::Invalid("driver parent catalog missing"))?;
        if tail.commit != commit
            || tail.parent_commit != self.body.parent_commit
            || tail.checkpoint
                != self.body.checkpoint.map(|descriptor| CheckpointPointerV1 {
                    height: self.body.height,
                    descriptor,
                })
            || tail.latest_checkpoint != self.catalog.last_checkpoint
        {
            return Err(ExecutionError::Invalid("driver parent catalog binding"));
        }
        Ok(commit)
    }
}

impl World {
    /// All nested runtime entrypoints in apply borrow this same transaction.
    /// Facts persist through their actual owner; this never clones World or
    /// reads the hot journal. The live fence comes from the original adapter.
    pub fn prepare_iws_commit<R: MerkleNodeReader, E: From<ExecutionError>>(
        &self,
        request: &CommitPreparationRequest<'_>,
        source: &R,
        memory: &MemoryBudget,
        mut live_fence: impl FnMut() -> Result<ExecutionFence, E>,
        apply: impl FnOnce(&World, &mut ExecutionTransaction<'_, R>) -> Result<(), E>,
    ) -> Result<ReservedValue<PreparedRuntimeCommit>, E> {
        let mut workspace = memory
            .reserve(64 * 1024, MemoryClass::Ordinary)
            .map_err(ExecutionError::Memory)?;
        if source
            .allocation_budget()
            .is_some_and(|pool| !pool.same_pool(memory))
        {
            return Err(ExecutionError::Invalid("driver source memory pool").into());
        }
        let parent_commit =
            request
                .parent
                .validate(source, memory, request.limits.max_pending_node_bytes)?;
        let parent = ExecutionParent {
            commit: parent_commit,
            roots: request.parent.roots.clone(),
            authority: request.parent.authority,
        };
        let mut txn =
            ExecutionTransaction::begin(parent, request.operation, source, memory, request.limits)?;
        let fence = txn.parent_fence();
        if fence != live_fence()? {
            return Err(ExecutionError::ParentConflict.into());
        }
        txn.contribute(|txn| apply(self, txn))?;
        let guard = txn.guard();
        let transition = txn.seal(&guard, &live_fence()?, request.next_tick)?;
        let history = HeaderPreparation {
            config: request.parent.config,
            catalog: request.parent.catalog,
            roots: &transition.record.after,
            parent: Some(request.parent.body),
            parent_history: Some(request.parent.history),
            transition: Some(&transition.record),
            genesis_facts: &[],
            operation: request.operation,
            authority: request.parent.authority,
            execution_environment: request.execution_environment,
            checkpoint: request.checkpoint,
        }
        .prepare(memory)?;
        let catalog = request.parent.catalog.extend(
            &history.body,
            source,
            memory,
            request.limits.max_pending_node_bytes,
        )?;
        let mut encoding = memory
            .reserve(4096, MemoryClass::Ordinary)
            .map_err(ExecutionError::Memory)?;
        let roots = transition.record.after.canonical_bytes()?;
        let root_charge = encoding
            .split(roots.capacity())
            .map_err(ExecutionError::Invalid)?;
        let root_set_bytes = root_charge.retain(roots);
        let bytes = catalog.after.canonical_bytes()?;
        let catalog_charge = encoding
            .split(bytes.capacity())
            .map_err(ExecutionError::Invalid)?;
        let catalog_bytes = catalog_charge.retain(bytes);
        let prepared = PreparedRuntimeCommit {
            transition,
            history,
            catalog,
            root_set_bytes,
            catalog_bytes,
            fence,
        };
        prepared.validate_live_fence(&live_fence()?)?;
        let charge = workspace
            .split(std::mem::size_of::<PreparedRuntimeCommit>() + 64)
            .map_err(ExecutionError::Invalid)?;
        Ok(charge.retain(prepared))
    }
}
