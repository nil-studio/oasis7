use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransitionRecordV1 {
    pub parent_commit: [u8; 32],
    pub operation: [u8; 32],
    pub authority: AuthorityBinding,
    pub before: RootSetV1,
    pub after: RootSetV1,
    pub reads: Vec<ExecutionReadV1>,
    pub writes: Vec<ExecutionWriteV1>,
    pub facts: Vec<ExecutionFactRefV1>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedTransition {
    pub record: TransitionRecordV1,
    pub new_nodes: Vec<StateNodeWriteV1>,
}

pub struct TransitionVerifier<'a, R> {
    pub parent: &'a ExecutionParent,
    pub operation: [u8; 32],
    pub source: &'a R,
    pub memory: &'a MemoryBudget,
    pub class: MemoryClass,
    pub max_pending_node_bytes: usize,
}

impl TransitionRecordV1 {
    pub fn validate(&self) -> Result<(), ExecutionError> {
        self.before.validate()?;
        self.after.validate()?;
        if self.before.world_id != self.after.world_id
            || self.before.genesis_config != self.after.genesis_config
            || self.before.height.checked_add(1) != Some(self.after.height)
            || self.after.simulation_tick < self.before.simulation_tick
        {
            return Err(ExecutionError::Invalid("transition root-set boundaries"));
        }
        if self.reads.len() > EXECUTION_MAX_READS
            || self.writes.len() > EXECUTION_MAX_WRITES
            || self.facts.len() > EXECUTION_MAX_FACTS
        {
            return Err(ExecutionError::Invalid("transition sequence bounds"));
        }
        let mut previous = None;
        for read in &self.reads {
            validate_key(&read.key)?;
            let key = (read.namespace, read.key.as_slice());
            if previous.is_some_and(|old| old >= key) {
                return Err(ExecutionError::Invalid("read-set ordering/duplicates"));
            }
            previous = Some(key);
        }
        let mut previous = None;
        for write in &self.writes {
            validate_key(&write.mutation.key)?;
            let key = (write.namespace, write.mutation.key.as_slice());
            if previous.is_some_and(|old| old >= key)
                || write.mutation.before == write.mutation.after
            {
                return Err(ExecutionError::Invalid("final write-set ordering/no-op"));
            }
            previous = Some(key);
            let position = self
                .reads
                .binary_search_by(|read| (read.namespace, read.key.as_slice()).cmp(&key))
                .map_err(|_| ExecutionError::Invalid("write without parent read"))?;
            if self.reads[position].before != write.mutation.before {
                return Err(ExecutionError::Invalid("read/write precondition mismatch"));
            }
        }
        Ok(())
    }

    pub(super) fn heap_bytes(&self) -> usize {
        self.before.heap_bytes()
            + self.after.heap_bytes()
            + self.reads.capacity() * std::mem::size_of::<ExecutionReadV1>()
            + self
                .reads
                .iter()
                .map(|read| read.key.capacity())
                .sum::<usize>()
            + self.writes.capacity() * std::mem::size_of::<ExecutionWriteV1>()
            + self
                .writes
                .iter()
                .map(|write| write.mutation.key.capacity())
                .sum::<usize>()
            + self.facts.capacity() * std::mem::size_of::<ExecutionFactRefV1>()
    }
}

impl PreparedTransition {
    pub(super) fn heap_bytes(&self) -> Result<usize, ExecutionError> {
        let mut bytes = std::mem::size_of::<Self>()
            + 64
            + self.record.heap_bytes()
            + self.new_nodes.capacity() * std::mem::size_of::<StateNodeWriteV1>();
        for node in &self.new_nodes {
            bytes = bytes
                .checked_add(node.digest.capacity())
                .and_then(|bytes| bytes.checked_add(node.bytes.capacity()))
                .ok_or(ExecutionError::ResourceLimited(
                    "transition allocation size",
                ))?;
        }
        Ok(bytes)
    }
}

/// Conservative transient admission covers pending/final Merkle maps, path
/// tracking, read/write metadata and bounded codec scratch, before allocation.
pub(super) fn phase_bound(
    writes: usize,
    reads: usize,
    facts: usize,
) -> Result<usize, ExecutionError> {
    writes
        .checked_mul(512 * 1024)
        .and_then(|n| n.checked_add(reads * (2 * STATE_MAX_KEY_BYTES + 1024)))
        .and_then(|n| n.checked_add(facts * 256))
        .and_then(|n| n.checked_add(128 * 1024))
        .ok_or(ExecutionError::ResourceLimited(
            "transition workspace overflow",
        ))
}

impl<R: MerkleNodeReader> TransitionVerifier<'_, R> {
    /// Reapply all reads/writes against the authenticated before roots. It
    /// verifies integrity, not business legality or authority qualification.
    pub fn verify(
        &self,
        record: &TransitionRecordV1,
    ) -> Result<ReservedValue<PreparedTransition>, ExecutionError> {
        record.validate()?;
        if record.before != self.parent.roots
            || record.parent_commit != self.parent.commit
            || record.authority != self.parent.authority
        {
            return Err(ExecutionError::ParentConflict);
        }
        if record.operation != self.operation {
            return Err(ExecutionError::Invalid("operation mismatch"));
        }
        let mut workspace = self.memory.reserve(
            phase_bound(record.writes.len(), record.reads.len(), record.facts.len())?,
            self.class,
        )?;
        for read in &record.reads {
            let value = record
                .before
                .state_root(read.namespace)
                .get(&read.key, self.source)
                .map_err(ExecutionError::Tree)?;
            if value != read.before {
                return Err(ExecutionError::Invalid("parent read value mismatch"));
            }
        }
        let mut new_nodes = Vec::new();
        for namespace in NAMESPACES {
            let mutations = record
                .writes
                .iter()
                .filter(|write| write.namespace == namespace)
                .map(|write| write.mutation.clone())
                .collect();
            let delta = StateDeltaV1::build(
                record.before.state_root(namespace).clone(),
                mutations,
                self.source,
                self.max_pending_node_bytes,
            )
            .map_err(ExecutionError::Tree)?;
            if delta.after != *record.after.state_root(namespace) {
                return Err(ExecutionError::Invalid("recomputed after root mismatch"));
            }
            new_nodes.extend(delta.new_nodes);
        }
        new_nodes.sort_unstable_by(|a, b| a.digest.cmp(&b.digest));
        let value = PreparedTransition {
            record: record.clone(),
            new_nodes,
        };
        let charge = workspace
            .split(value.heap_bytes()?)
            .map_err(|_| ExecutionError::Invalid("verification exceeded workspace accounting"))?;
        Ok(charge.retain(value))
    }
}
