//! Paid in-place speculative overlay with one-way parent-source inheritance.
//! DATA-only commit/discard: this never authorizes a business effect or Core.
use super::*;
use crate::controlled_authority::formal::{
    FormalError,
    bounded_context::{FormalBoundedContextV1, WorkIdentityV1},
};
use std::sync::atomic::Ordering;
#[derive(Debug)]
pub enum StagedExecutionBranchErrorV1 {
    Execution(ExecutionError),
    Formal(FormalError),
}
impl std::fmt::Display for StagedExecutionBranchErrorV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Execution(e) => write!(f, "{e}"),
            Self::Formal(e) => write!(f, "{e}"),
        }
    }
}
impl std::error::Error for StagedExecutionBranchErrorV1 {}
impl From<ExecutionError> for StagedExecutionBranchErrorV1 {
    fn from(e: ExecutionError) -> Self {
        Self::Execution(e)
    }
}
impl From<FormalError> for StagedExecutionBranchErrorV1 {
    fn from(e: FormalError) -> Self {
        Self::Formal(e)
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StagedBranchDispositionV1 {
    Committed,
    Discarded,
}
/// Opaque evidence for one actual child->parent finish. It is DATA provenance,
/// never permission to publish a discarded source or skip its business checks.
pub struct StagedBranchRejoinEvidenceV1 {
    child: PreparedGuard,
    child_final_revision: u64,
    parent: PreparedGuard,
    disposition: StagedBranchDispositionV1,
    core: WorkIdentityV1,
    memory: MemoryBudget,
    _fee: MemoryReservation,
}
impl StagedBranchRejoinEvidenceV1 {
    pub fn disposition(&self) -> StagedBranchDispositionV1 {
        self.disposition
    }
    pub fn validate_parent<R: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, R>,
        common: &FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<(), StagedExecutionBranchErrorV1> {
        tx.validate_exact_transaction_identity(&self.parent)?;
        tx.validate_guard(&self.parent, live)?;
        if self.core != common.work().identity()
            || !self.memory.same_pool(common.memory())
            || !self._fee.uses_budget(common.memory())
        {
            return Err(ExecutionError::Invalid("staged rejoin Core/pool").into());
        }
        Ok(())
    }
    /// A private Runtime source owner must first validate all of its own child
    /// facts/fields. This accepts only that exact finished child's retained guard
    /// and returns the actual captured parent finish guard, never a refreshed one.
    /// The caller must separately enforce committed vs historical-discarded DATA.
    pub fn rebind_source_guard<R: MerkleNodeReader>(
        &self,
        source: &PreparedGuard,
        tx: &ExecutionTransaction<'_, R>,
        common: &FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<PreparedGuard, StagedExecutionBranchErrorV1> {
        self.validate_parent(tx, common, live)?;
        if !Arc::ptr_eq(&source.owner, &self.child.owner)
            || source.fence != self.child.fence
            || source.revision > self.child_final_revision
        {
            return Err(ExecutionError::StaleGuard.into());
        }
        Ok(PreparedGuard {
            owner: self.parent.owner.clone(),
            revision: self.parent.revision,
            // Existing branch rebind carries exact execution lineage, not
            // a claim that old readonly DATA equals the resolved parent.
            mutation_revision: None,
            fence: self.parent.fence,
        })
    }
}
/// Must be finished explicitly. Unwind, forgotten finalization, or any finish
/// failure permanently poisons the still-installed child transaction.
pub struct StagedExecutionBranchV1<'a, R: MerkleNodeReader> {
    parent: Option<ExecutionTransaction<'a, R>>,
    child: PreparedGuard,
    poison: Arc<TransactionLineageV1>,
    core: WorkIdentityV1,
    _charge: MemoryReservation,
}
impl<R: MerkleNodeReader> Drop for StagedExecutionBranchV1<'_, R> {
    fn drop(&mut self) {
        if self.parent.is_some() {
            self.poison.poison.store(true, Ordering::Release)
        }
    }
}
fn charge(
    common: &mut FormalBoundedContextV1<'_>,
    steps: usize,
    bytes: usize,
) -> Result<(), StagedExecutionBranchErrorV1> {
    common.charge(
        u64::try_from(steps).map_err(|_| ExecutionError::ResourceLimited("staged branch steps"))?,
        u64::try_from(bytes).map_err(|_| ExecutionError::ResourceLimited("staged branch bytes"))?,
    )?;
    Ok(())
}
impl<'a, R: MerkleNodeReader> ExecutionTransaction<'a, R> {
    pub fn begin_staged_branch_v1(
        &mut self,
        common: &mut FormalBoundedContextV1<'_>,
        guard: &PreparedGuard,
        live: &ExecutionFence,
    ) -> Result<StagedExecutionBranchV1<'a, R>, StagedExecutionBranchErrorV1> {
        let result = (|| {
            self.validate_guard(guard, live)?;
            if !self.uses_budget(common.memory()) {
                return Err(ExecutionError::Invalid("staged branch source/core").into());
            }
            let workspace_bytes = BASE_WORKSPACE
                .checked_add(self.parent.roots.world_id.len())
                .and_then(|n| n.checked_add(512))
                .ok_or(ExecutionError::ResourceLimited("staged branch workspace"))?;
            charge(common, 1, workspace_bytes)?;
            let workspace = self
                .memory
                .reserve(workspace_bytes, MemoryClass::Ordinary)
                .map_err(ExecutionError::Memory)?;
            let identity = self
                .memory
                .reserve(
                    owner_admission_bytes().max(GUARD_BYTES),
                    MemoryClass::Ordinary,
                )
                .map_err(ExecutionError::Memory)?;
            let token_fee = self
                .memory
                .reserve(
                    std::mem::size_of::<StagedExecutionBranchV1<R>>(),
                    MemoryClass::Ordinary,
                )
                .map_err(ExecutionError::Memory)?;
            let mut entries: [BTreeMap<Vec<u8>, Entry>; 6] =
                std::array::from_fn(|_| BTreeMap::new());
            for (target, source) in entries.iter_mut().zip(&self.entries) {
                for (key, value) in source {
                    let bytes = key
                        .len()
                        .checked_add(512)
                        .ok_or(ExecutionError::ResourceLimited("staged branch entry"))?;
                    let depth = 1 + source.len().ilog2() as usize;
                    charge(
                        common,
                        depth,
                        key.len()
                            .checked_mul(depth)
                            .and_then(|n| n.checked_add(512))
                            .ok_or(ExecutionError::ResourceLimited("staged clone comparisons"))?,
                    )?;
                    let fee = self
                        .memory
                        .reserve(bytes, MemoryClass::Ordinary)
                        .map_err(ExecutionError::Memory)?;
                    target.insert(
                        key.clone(),
                        Entry {
                            before: value.before,
                            after: value.after,
                            _charge: fee,
                        },
                    );
                }
            }
            let mut facts = BTreeMap::new();
            for (index, fact) in &self.facts {
                charge(common, 1 + self.facts.len().ilog2() as usize, 512)?;
                let fee = self
                    .memory
                    .reserve(512, MemoryClass::Ordinary)
                    .map_err(ExecutionError::Memory)?;
                facts.insert(
                    *index,
                    Fact {
                        value: fact.value,
                        _charge: fee,
                    },
                );
            }
            // No ancestor graph/query: one paid O(1) owner per actual DFS child.
            // The same domain's poison immediately reaches every active level.
            charge(
                common,
                2,
                owner_admission_bytes()
                    .max(GUARD_BYTES)
                    .checked_add(std::mem::size_of::<StagedExecutionBranchV1<R>>())
                    .ok_or(ExecutionError::ResourceLimited("staged owner metadata"))?,
            )?;
            let domain = self.owner.domain.clone();
            let start = domain
                .next
                .fetch_update(Ordering::AcqRel, Ordering::Acquire, |next| {
                    next.checked_add(1)
                })
                .map_err(|_| ExecutionError::ResourceLimited("staged lineage position"))?;
            let owner = Arc::new(TransactionOwnerV1 {
                domain: domain.clone(),
                start,
                closed_end: std::sync::atomic::AtomicU64::new(0),
                _charge: identity,
            });
            let child = Self {
                parent: self.parent.clone(),
                operation: self.operation,
                fence: self.fence,
                source: self.source,
                memory: self.memory,
                limits: self.limits,
                entries,
                facts,
                reads: self.reads,
                dirty: self.dirty,
                revision: self.revision,
                mutation_revision: self.mutation_revision,
                owner,
                aborted: false,
                branch_ancestor: Some(self.owner.clone()),
                world_event_counter_claimed: std::sync::atomic::AtomicBool::new(
                    self.world_event_counter_claimed.load(Ordering::Acquire),
                ),
                _workspace: workspace,
            };
            let parent = std::mem::replace(self, child);
            Ok(StagedExecutionBranchV1 {
                parent: Some(parent),
                child: self.guard(),
                poison: domain,
                core: common.work().identity(),
                _charge: token_fee,
            })
        })();
        if result.is_err() {
            self.aborted = true
        }
        result
    }
}
impl<'a, R: MerkleNodeReader> StagedExecutionBranchV1<'a, R> {
    pub fn validate_current_branch(
        &self,
        tx: &ExecutionTransaction<'_, R>,
        common: &FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<(), StagedExecutionBranchErrorV1> {
        tx.validate_exact_transaction_identity(&self.child)?;
        tx.validate_guard(&tx.guard(), live)?;
        if self.parent.is_none()
            || self.core != common.work().identity()
            || !tx.uses_budget(common.memory())
            || !self._charge.uses_budget(common.memory())
        {
            return Err(ExecutionError::Invalid("staged branch actual identity/core").into());
        }
        Ok(())
    }
    /// Authenticate the actual parent's retained participant guard without
    /// accepting a guard minted by the child itself.
    pub fn validate_parent_participant(
        &self,
        guard: &PreparedGuard,
    ) -> Result<(), StagedExecutionBranchErrorV1> {
        let parent = self
            .parent
            .as_ref()
            .ok_or(ExecutionError::Invalid("staged parent missing"))?;
        parent.validate_exact_transaction_identity(guard)?;
        Ok(())
    }
    /// Commit final state/facts while preserving the actual parent's identity.
    pub fn commit(
        self,
        tx: &mut ExecutionTransaction<'a, R>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<StagedBranchRejoinEvidenceV1, StagedExecutionBranchErrorV1> {
        self.finish(tx, common, live, true)
    }
    /// Discard writes/facts, retaining every genuine newly observed read. Consensus
    /// work and decoder/IO allowances belong to the caller and are never restored.
    pub fn discard(
        self,
        tx: &mut ExecutionTransaction<'a, R>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<StagedBranchRejoinEvidenceV1, StagedExecutionBranchErrorV1> {
        self.finish(tx, common, live, false)
    }
    fn finish(
        mut self,
        tx: &mut ExecutionTransaction<'a, R>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
        commit: bool,
    ) -> Result<StagedBranchRejoinEvidenceV1, StagedExecutionBranchErrorV1> {
        self.validate_current_branch(tx, common, live)?;
        let parent = self
            .parent
            .as_ref()
            .ok_or(ExecutionError::Invalid("staged branch parent"))?;
        // Validate the copied immutable fact prefix; child operations can append only.
        let prefix_depth = tx
            .facts
            .len()
            .checked_add(1)
            .ok_or(ExecutionError::ResourceLimited("staged prefix depth"))?
            .ilog2() as usize
            + 1;
        charge(
            common,
            parent
                .facts
                .len()
                .checked_mul(prefix_depth)
                .and_then(|n| n.checked_add(1))
                .ok_or(ExecutionError::ResourceLimited("staged prefix steps"))?,
            parent
                .facts
                .len()
                .checked_mul(64)
                .and_then(|n| n.checked_mul(prefix_depth))
                .ok_or(ExecutionError::ResourceLimited("staged fact prefix"))?,
        )?;
        if tx.facts.len() < parent.facts.len()
            || parent
                .facts
                .iter()
                .any(|(i, f)| tx.facts.get(i).is_none_or(|v| v.value != f.value))
        {
            return Err(ExecutionError::Invalid("staged branch fact prefix").into());
        }
        let revision = tx
            .revision
            .checked_add(1)
            .ok_or(ExecutionError::ResourceLimited("staged branch revision"))?;
        if !commit {
            for (entries, old) in tx.entries.iter().zip(&parent.entries) {
                let depth = old
                    .len()
                    .checked_add(1)
                    .ok_or(ExecutionError::ResourceLimited("staged merge depth"))?
                    .ilog2() as usize
                    + 1;
                charge(common, entries.len(), 0)?;
                for key in entries.keys() {
                    charge(
                        common,
                        depth,
                        key.len()
                            .checked_mul(depth)
                            .ok_or(ExecutionError::ResourceLimited("staged merge keys"))?,
                    )?;
                }
            }
        }
        charge(common, 2, 512)?;
        let fee = tx
            .memory
            .reserve(
                std::mem::size_of::<StagedBranchRejoinEvidenceV1>(),
                MemoryClass::Ordinary,
            )
            .map_err(ExecutionError::Memory)?;
        let evidence = StagedBranchRejoinEvidenceV1 {
            child: PreparedGuard {
                owner: self.child.owner.clone(),
                revision: self.child.revision,
                mutation_revision: self.child.mutation_revision,
                fence: self.child.fence,
            },
            child_final_revision: tx.revision,
            parent: PreparedGuard {
                owner: parent.owner.clone(),
                revision,
                mutation_revision: Some(revision),
                fence: parent.fence,
            },
            disposition: if commit {
                StagedBranchDispositionV1::Committed
            } else {
                StagedBranchDispositionV1::Discarded
            },
            core: self.core,
            memory: tx.memory.clone(),
            _fee: fee,
        };
        self.child.owner.closed_end.store(
            self.child.owner.domain.next.load(Ordering::Acquire),
            Ordering::Release,
        );
        let mut parent = self.parent.take().expect("validated opaque staged parent");
        if !commit {
            for (entries, old) in tx.entries.iter_mut().zip(&parent.entries) {
                for (key, entry) in entries {
                    entry.after = old.get(key).map_or(entry.before, |base| base.after)
                }
            }
        }
        parent.entries =
            std::mem::replace(&mut tx.entries, std::array::from_fn(|_| BTreeMap::new()));
        parent.reads = tx.reads;
        if commit {
            parent.facts = std::mem::take(&mut tx.facts);
            parent.dirty = tx.dirty;
        }
        parent.revision = revision;
        parent.mutation_revision = revision;
        *tx = parent;
        Ok(evidence)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::controlled_authority::formal::bounded_context::FormalBoundedWorkV1;
    struct Empty;
    impl MerkleNodeReader for Empty {
        type Node = Vec<u8>;
        fn read_node(&self, _: &str, _: usize) -> Result<Option<Vec<u8>>, String> {
            Ok(None)
        }
    }
    fn parent() -> ExecutionParent {
        ExecutionParent {
            roots: RootSetV1::empty("branch-fixture".into(), [8; 32]).unwrap(),
            commit: [7; 32],
            authority: AuthorityBinding {
                token: [9; 32],
                epoch: 3,
            },
        }
    }
    fn limits() -> TransactionLimits {
        TransactionLimits {
            max_reads: 32,
            max_writes: 16,
            max_facts: 16,
            max_pending_node_bytes: 1 << 20,
        }
    }
    #[test]
    fn bootstrap_read_only_checks_actual_mutations_and_branch_identity() {
        let memory = MemoryBudget::new(32 << 20, 1 << 20).unwrap();
        let pages = Empty;
        let work = FormalBoundedWorkV1::new(100_000, 16 << 20).unwrap();
        let mut stop = || false;
        let mut common = FormalBoundedContextV1::new(&memory, &work, &mut stop);
        let mut tx =
            ExecutionTransaction::begin(parent(), [1; 32], &pages, &memory, limits()).unwrap();
        let live = tx.parent_fence();
        tx.read(StateNamespaceV1::Business, b"registered").unwrap();
        tx.validate_bootstrap_read_only_v1(&live).unwrap();
        let branch = tx
            .begin_staged_branch_v1(&mut common, &tx.guard(), &live)
            .unwrap();
        assert!(tx.validate_bootstrap_read_only_v1(&live).is_err());
        branch.discard(&mut tx, &mut common, &live).unwrap();
        assert!(tx.validate_bootstrap_read_only_v1(&live).is_err());
        let mut tx =
            ExecutionTransaction::begin(parent(), [1; 32], &pages, &memory, limits()).unwrap();
        tx.write(StateNamespaceV1::Business, b"same-value", None)
            .unwrap();
        assert!(tx.validate_bootstrap_read_only_v1(&live).is_err());
        assert_eq!(tx.dirty, 0);
    }
    #[test]
    fn readonly_source_guard_keeps_reads_without_relaxing_mutation_admission() {
        let memory = MemoryBudget::new(32 << 20, 1 << 20).unwrap();
        let pages = Empty;
        let mut tx =
            ExecutionTransaction::begin(parent(), [1; 32], &pages, &memory, limits()).unwrap();
        let live = tx.parent_fence();
        let source = tx.guard();
        tx.read(StateNamespaceV1::Business, b"new-read").unwrap();
        assert!(matches!(
            tx.validate_guard(&source, &live),
            Err(ExecutionError::StaleGuard)
        ));
        tx.validate_read_source_guard(&source, &live).unwrap();
        // Even a same-value write is a mutation boundary for retained DATA.
        tx.write(StateNamespaceV1::Business, b"new-read", None)
            .unwrap();
        assert!(matches!(
            tx.validate_read_source_guard(&source, &live),
            Err(ExecutionError::StaleGuard)
        ));
    }
    #[test]
    fn readonly_source_guard_rejects_facts_foreign_parent_and_abort() {
        let memory = MemoryBudget::new(32 << 20, 1 << 20).unwrap();
        let pages = Empty;
        let mut constrained = limits();
        constrained.max_reads = 0;
        let mut tx =
            ExecutionTransaction::begin(parent(), [1; 32], &pages, &memory, constrained).unwrap();
        let live = tx.parent_fence();
        let source = tx.guard();
        let other =
            ExecutionTransaction::begin(parent(), [1; 32], &pages, &memory, limits()).unwrap();
        assert!(other.validate_read_source_guard(&source, &live).is_err());
        let mut changed = live;
        changed.authority.epoch += 1;
        assert!(matches!(
            tx.validate_read_source_guard(&source, &changed),
            Err(ExecutionError::ParentConflict)
        ));
        tx.append_fact(ExecutionFactRefV1 {
            kind: ExecutionFactKindV1::Event,
            id: [5; 32],
        })
        .unwrap();
        assert!(matches!(
            tx.validate_read_source_guard(&source, &live),
            Err(ExecutionError::StaleGuard)
        ));
        let source = tx.guard();
        assert!(tx.read(StateNamespaceV1::Business, b"").is_err());
        assert!(matches!(
            tx.validate_read_source_guard(&source, &live),
            Err(ExecutionError::Aborted)
        ));
    }
    #[test]
    fn readonly_source_guard_rejects_both_branch_resolutions() {
        for commit in [false, true] {
            let memory = MemoryBudget::new(32 << 20, 1 << 20).unwrap();
            let pages = Empty;
            let work = FormalBoundedWorkV1::new(100_000, 16 << 20).unwrap();
            let mut stop = || false;
            let mut common = FormalBoundedContextV1::new(&memory, &work, &mut stop);
            let mut tx =
                ExecutionTransaction::begin(parent(), [1; 32], &pages, &memory, limits()).unwrap();
            let live = tx.parent_fence();
            let original = tx.guard();
            let branch = tx
                .begin_staged_branch_v1(&mut common, &original, &live)
                .unwrap();
            assert!(matches!(
                tx.validate_read_source_guard(&original, &live),
                Err(ExecutionError::StaleGuard)
            ));
            tx.read(StateNamespaceV1::Business, b"child-read").unwrap();
            let child = tx.guard();
            let evidence = if commit {
                branch.commit(&mut tx, &mut common, &live).unwrap()
            } else {
                branch.discard(&mut tx, &mut common, &live).unwrap()
            };
            let rebound = evidence
                .rebind_source_guard(&child, &tx, &common, &live)
                .unwrap();
            tx.validate_guard(&rebound, &live).unwrap();
            assert!(matches!(
                tx.validate_read_source_guard(&rebound, &live),
                Err(ExecutionError::StaleGuard)
            ));
            assert!(matches!(
                tx.validate_read_source_guard(&original, &live),
                Err(ExecutionError::StaleGuard)
            ));
            tx.validate_guard(&tx.guard(), &live).unwrap();
        }
    }
    #[test]
    fn discard_keeps_real_read_dependencies_and_parent_writes_without_refunding_work() {
        let memory = MemoryBudget::new(32 << 20, 1 << 20).unwrap();
        let pages = Empty;
        let work = FormalBoundedWorkV1::new(100_000, 16 << 20).unwrap();
        let mut stop = || false;
        let mut common = FormalBoundedContextV1::new(&memory, &work, &mut stop);
        let mut tx =
            ExecutionTransaction::begin(parent(), [1; 32], &pages, &memory, limits()).unwrap();
        let live = tx.parent_fence();
        tx.write(StateNamespaceV1::Business, b"parent", Some([1; 32]))
            .unwrap();
        tx.append_fact(ExecutionFactRefV1 {
            kind: ExecutionFactKindV1::Input,
            id: [2; 32],
        })
        .unwrap();
        let original = tx.guard();
        let before = work.remaining_steps();
        let branch = tx
            .begin_staged_branch_v1(&mut common, &original, &live)
            .unwrap();
        tx.validate_transaction_identity(&original).unwrap();
        assert!(tx.validate_exact_transaction_identity(&original).is_err());
        tx.write(StateNamespaceV1::Business, b"parent", Some([3; 32]))
            .unwrap();
        tx.write(StateNamespaceV1::Business, b"only-child", Some([4; 32]))
            .unwrap();
        tx.append_fact(ExecutionFactRefV1 {
            kind: ExecutionFactKindV1::Event,
            id: [5; 32],
        })
        .unwrap();
        let child = tx.guard();
        let during = work.remaining_steps();
        let evidence = branch.discard(&mut tx, &mut common, &live).unwrap();
        assert_eq!(
            tx.read(StateNamespaceV1::Business, b"parent").unwrap(),
            Some([1; 32])
        );
        assert_eq!(
            tx.read(StateNamespaceV1::Business, b"only-child").unwrap(),
            None
        );
        assert_eq!(tx.reads, 2);
        assert_eq!(tx.dirty, 1);
        assert_eq!(tx.ordered_fact_count(), 1);
        assert!(work.remaining_steps() < during && during < before);
        tx.validate_transaction_identity(&original).unwrap();
        assert!(tx.validate_transaction_identity(&child).is_err());
        assert_eq!(evidence.disposition(), StagedBranchDispositionV1::Discarded);
        let rebound = evidence
            .rebind_source_guard(&child, &tx, &common, &live)
            .unwrap();
        tx.validate_guard(&rebound, &live).unwrap();
        assert!(
            evidence
                .rebind_source_guard(&original, &tx, &common, &live)
                .is_err()
        );
    }
    #[test]
    fn commit_moves_actual_facts_and_rejoins_only_the_exact_finished_child() {
        let memory = MemoryBudget::new(32 << 20, 1 << 20).unwrap();
        let pages = Empty;
        let work = FormalBoundedWorkV1::new(100_000, 16 << 20).unwrap();
        let mut stop = || false;
        let mut common = FormalBoundedContextV1::new(&memory, &work, &mut stop);
        let mut tx =
            ExecutionTransaction::begin(parent(), [1; 32], &pages, &memory, limits()).unwrap();
        let live = tx.parent_fence();
        let original = tx.guard();
        let branch = tx
            .begin_staged_branch_v1(&mut common, &original, &live)
            .unwrap();
        tx.write(StateNamespaceV1::Effects, b"child-effect", Some([3; 32]))
            .unwrap();
        tx.append_fact(ExecutionFactRefV1 {
            kind: ExecutionFactKindV1::Event,
            id: [4; 32],
        })
        .unwrap();
        let child = tx.guard();
        let evidence = branch.commit(&mut tx, &mut common, &live).unwrap();
        assert_eq!(evidence.disposition(), StagedBranchDispositionV1::Committed);
        assert_eq!(
            tx.read(StateNamespaceV1::Effects, b"child-effect").unwrap(),
            Some([3; 32])
        );
        assert_eq!(tx.ordered_fact(0).unwrap().id, [4; 32]);
        assert!(tx.validate_transaction_identity(&child).is_err());
        let rebound = evidence
            .rebind_source_guard(&child, &tx, &common, &live)
            .unwrap();
        tx.validate_guard(&rebound, &live).unwrap();
        tx.read(StateNamespaceV1::Business, b"new-read").unwrap();
        assert!(
            evidence
                .rebind_source_guard(&child, &tx, &common, &live)
                .is_err()
        );
    }
    #[test]
    fn abandoned_stage_and_swallowed_child_error_cannot_escape_poison() {
        let memory = MemoryBudget::new(32 << 20, 1 << 20).unwrap();
        let pages = Empty;
        let work = FormalBoundedWorkV1::new(100_000, 16 << 20).unwrap();
        let mut stop = || false;
        let mut common = FormalBoundedContextV1::new(&memory, &work, &mut stop);
        let mut tx =
            ExecutionTransaction::begin(parent(), [1; 32], &pages, &memory, limits()).unwrap();
        let live = tx.parent_fence();
        let branch = tx
            .begin_staged_branch_v1(&mut common, &tx.guard(), &live)
            .unwrap();
        drop(branch);
        assert!(tx.validate_guard(&tx.guard(), &live).is_err());
        let mut tx =
            ExecutionTransaction::begin(parent(), [1; 32], &pages, &memory, limits()).unwrap();
        let branch = tx
            .begin_staged_branch_v1(&mut common, &tx.guard(), &live)
            .unwrap();
        let _: Result<(), ExecutionError> =
            tx.contribute(|_| Err(ExecutionError::Invalid("sentinel")));
        assert!(branch.discard(&mut tx, &mut common, &live).is_err());
        assert!(tx.validate_guard(&tx.guard(), &live).is_err());
    }
    #[test]
    fn nested_commit_then_outer_discard_retains_reads_and_requires_each_rejoin() {
        let memory = MemoryBudget::new(64 << 20, 1 << 20).unwrap();
        let pages = Empty;
        let work = FormalBoundedWorkV1::new(1_000_000, 64 << 20).unwrap();
        let mut stop = || false;
        let mut common = FormalBoundedContextV1::new(&memory, &work, &mut stop);
        let mut tx =
            ExecutionTransaction::begin(parent(), [1; 32], &pages, &memory, limits()).unwrap();
        let live = tx.parent_fence();
        tx.write(StateNamespaceV1::Business, b"root", Some([1; 32]))
            .unwrap();
        let root = tx.guard();
        let outer = tx
            .begin_staged_branch_v1(&mut common, &root, &live)
            .unwrap();
        tx.write(StateNamespaceV1::Business, b"outer", Some([2; 32]))
            .unwrap();
        let outer_guard = tx.guard();
        let inner = tx
            .begin_staged_branch_v1(&mut common, &outer_guard, &live)
            .unwrap();
        tx.validate_transaction_identity(&root).unwrap();
        tx.validate_transaction_identity(&outer_guard).unwrap();
        assert!(tx.validate_exact_transaction_identity(&root).is_err());
        assert!(tx.validate_guard(&outer_guard, &live).is_err());
        assert!(tx.validate_read_source_guard(&root, &live).is_err());
        tx.read(StateNamespaceV1::Business, b"inner-read").unwrap();
        tx.write(StateNamespaceV1::Business, b"inner", Some([3; 32]))
            .unwrap();
        tx.append_fact(ExecutionFactRefV1 {
            kind: ExecutionFactKindV1::Event,
            id: [4; 32],
        })
        .unwrap();
        let child = tx.guard();
        let before = work.remaining_steps();
        let inner_receipt = inner.commit(&mut tx, &mut common, &live).unwrap();
        let rebound = inner_receipt
            .rebind_source_guard(&child, &tx, &common, &live)
            .unwrap();
        tx.validate_exact_transaction_identity(&rebound).unwrap();
        assert!(tx.validate_transaction_identity(&child).is_err());
        let outer_receipt = outer.discard(&mut tx, &mut common, &live).unwrap();
        assert!(inner_receipt.validate_parent(&tx, &common, &live).is_err());
        let final_guard = outer_receipt
            .rebind_source_guard(&rebound, &tx, &common, &live)
            .unwrap();
        tx.validate_guard(&final_guard, &live).unwrap();
        assert!(
            outer_receipt
                .rebind_source_guard(&child, &tx, &common, &live)
                .is_err()
        );
        assert_eq!(
            tx.read(StateNamespaceV1::Business, b"root").unwrap(),
            Some([1; 32])
        );
        assert_eq!(tx.read(StateNamespaceV1::Business, b"outer").unwrap(), None);
        assert_eq!(tx.read(StateNamespaceV1::Business, b"inner").unwrap(), None);
        assert_eq!(tx.ordered_fact_count(), 0);
        assert_eq!(tx.reads, 4);
        assert_eq!(tx.dirty, 1);
        assert!(work.remaining_steps() < before);
    }
    #[test]
    fn abandoning_or_resolving_outer_with_live_inner_poison_all_descendants() {
        for abandon in [true, false] {
            let memory = MemoryBudget::new(64 << 20, 1 << 20).unwrap();
            let pages = Empty;
            let work = FormalBoundedWorkV1::new(1_000_000, 64 << 20).unwrap();
            let mut stop = || false;
            let mut common = FormalBoundedContextV1::new(&memory, &work, &mut stop);
            let mut tx =
                ExecutionTransaction::begin(parent(), [1; 32], &pages, &memory, limits()).unwrap();
            let live = tx.parent_fence();
            let outer = tx
                .begin_staged_branch_v1(&mut common, &tx.guard(), &live)
                .unwrap();
            let inner = tx
                .begin_staged_branch_v1(&mut common, &tx.guard(), &live)
                .unwrap();
            if abandon {
                drop(outer)
            } else {
                assert!(outer.discard(&mut tx, &mut common, &live).is_err())
            }
            assert!(matches!(
                tx.validate_guard(&tx.guard(), &live),
                Err(ExecutionError::Aborted)
            ));
            assert!(inner.commit(&mut tx, &mut common, &live).is_err());
            assert!(
                tx.read(StateNamespaceV1::Business, b"cannot-escape")
                    .is_err()
            );
        }
    }
    #[test]
    fn inner_discard_then_outer_commit_keeps_only_outer_facts() {
        let memory = MemoryBudget::new(64 << 20, 1 << 20).unwrap();
        let pages = Empty;
        let work = FormalBoundedWorkV1::new(1_000_000, 64 << 20).unwrap();
        let mut stop = || false;
        let mut common = FormalBoundedContextV1::new(&memory, &work, &mut stop);
        let mut tx =
            ExecutionTransaction::begin(parent(), [1; 32], &pages, &memory, limits()).unwrap();
        let live = tx.parent_fence();
        let outer = tx
            .begin_staged_branch_v1(&mut common, &tx.guard(), &live)
            .unwrap();
        tx.append_fact(ExecutionFactRefV1 {
            kind: ExecutionFactKindV1::Event,
            id: [2; 32],
        })
        .unwrap();
        let inner = tx
            .begin_staged_branch_v1(&mut common, &tx.guard(), &live)
            .unwrap();
        tx.write(StateNamespaceV1::Business, b"only-inner", Some([3; 32]))
            .unwrap();
        tx.append_fact(ExecutionFactRefV1 {
            kind: ExecutionFactKindV1::Event,
            id: [4; 32],
        })
        .unwrap();
        inner.discard(&mut tx, &mut common, &live).unwrap();
        outer.commit(&mut tx, &mut common, &live).unwrap();
        assert_eq!(tx.ordered_fact_count(), 1);
        assert_eq!(tx.ordered_fact(0).unwrap().id, [2; 32]);
        assert_eq!(
            tx.read(StateNamespaceV1::Business, b"only-inner").unwrap(),
            None
        );
        assert_eq!(tx.reads, 1);
    }

    #[test]
    fn nested_unwind_and_swallowed_inner_abort_never_restore_live_outer() {
        for unwind in [true, false] {
            let memory = MemoryBudget::new(64 << 20, 1 << 20).unwrap();
            let pages = Empty;
            let work = FormalBoundedWorkV1::new(1_000_000, 64 << 20).unwrap();
            let mut stop = || false;
            let mut common = FormalBoundedContextV1::new(&memory, &work, &mut stop);
            let mut tx =
                ExecutionTransaction::begin(parent(), [1; 32], &pages, &memory, limits()).unwrap();
            let live = tx.parent_fence();
            let outer = tx
                .begin_staged_branch_v1(&mut common, &tx.guard(), &live)
                .unwrap();
            if unwind {
                let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let _inner = tx
                        .begin_staged_branch_v1(&mut common, &tx.guard(), &live)
                        .unwrap();
                    panic!("nested staged fixture unwind");
                }));
                assert!(caught.is_err());
            } else {
                let inner = tx
                    .begin_staged_branch_v1(&mut common, &tx.guard(), &live)
                    .unwrap();
                let _: Result<(), ExecutionError> =
                    tx.contribute(|_| Err(ExecutionError::Invalid("nested sentinel")));
                assert!(inner.discard(&mut tx, &mut common, &live).is_err());
            }
            assert!(matches!(
                tx.validate_guard(&tx.guard(), &live),
                Err(ExecutionError::Aborted)
            ));
            assert!(outer.commit(&mut tx, &mut common, &live).is_err());
        }
    }
    #[test]
    fn constant_time_lineage_rejects_closed_siblings_descendants_and_foreign_domain() {
        let memory = MemoryBudget::new(64 << 20, 1 << 20).unwrap();
        let pages = Empty;
        let work = FormalBoundedWorkV1::new(1_000_000, 64 << 20).unwrap();
        let mut stop = || false;
        let mut common = FormalBoundedContextV1::new(&memory, &work, &mut stop);
        let mut tx =
            ExecutionTransaction::begin(parent(), [1; 32], &pages, &memory, limits()).unwrap();
        let live = tx.parent_fence();
        let foreign =
            ExecutionTransaction::begin(parent(), [1; 32], &pages, &memory, limits()).unwrap();
        let foreign_guard = foreign.guard();
        let root = tx.guard();
        let outer = tx
            .begin_staged_branch_v1(&mut common, &tx.guard(), &live)
            .unwrap();
        let outer_guard = tx.guard();
        let first = tx
            .begin_staged_branch_v1(&mut common, &tx.guard(), &live)
            .unwrap();
        let closed = tx.guard();
        tx.validate_transaction_identity(&root).unwrap();
        tx.validate_transaction_identity(&outer_guard).unwrap();
        assert!(tx.validate_transaction_identity(&foreign_guard).is_err());
        first.discard(&mut tx, &mut common, &live).unwrap();
        assert_ne!(closed.owner.closed_end.load(Ordering::Acquire), 0);
        // A resolved descendant cannot be read in its restored parent.
        assert!(tx.validate_transaction_identity(&closed).is_err());
        let sibling = tx
            .begin_staged_branch_v1(&mut common, &tx.guard(), &live)
            .unwrap();
        assert!(closed.owner.start < tx.owner.start);
        assert!(Arc::ptr_eq(&closed.owner.domain, &tx.owner.domain));
        // Merely sharing a domain and being older cannot admit a closed sibling.
        assert!(tx.validate_transaction_identity(&closed).is_err());
        assert!(
            tx.validate_exact_transaction_identity(&outer_guard)
                .is_err()
        );
        assert!(tx.validate_guard(&outer_guard, &live).is_err());
        assert!(tx.validate_read_source_guard(&outer_guard, &live).is_err());
        tx.validate_transaction_identity(&root).unwrap();
        tx.validate_transaction_identity(&outer_guard).unwrap();
        sibling.commit(&mut tx, &mut common, &live).unwrap();
        outer.commit(&mut tx, &mut common, &live).unwrap();
        assert!(tx.validate_transaction_identity(&outer_guard).is_err());
        assert!(tx.validate_transaction_identity(&closed).is_err());
        tx.validate_transaction_identity(&root).unwrap();
    }
}
