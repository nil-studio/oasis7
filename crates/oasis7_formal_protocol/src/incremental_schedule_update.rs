use super::*;
use std::collections::BTreeMap;

impl ScheduleIndexV1 {
    /// Update one pair using the actual old scalar wake read from the same
    /// transaction. The qualified parent must already prove index/scalar
    /// bijection: this operation cannot infer absence at every other time.
    /// Only final pages absent from the read parent closure are emitted.
    pub fn update(
        &self,
        instance: &str,
        old_wake: Option<u64>,
        new_wake: Option<u64>,
        source: &impl MerkleNodeReader,
        memory: &MemoryBudget,
        max_pending: usize,
    ) -> Result<ReservedValue<ScheduleDeltaV1>, ExecutionError> {
        // One mutation visits at most two time and two instance paths. Bound
        // both pending maps even if a caller supplies an enormous allowance.
        let mut phase = memory.reserve(2 * 1024 * 1024, MemoryClass::Ordinary)?;
        if source.allocation_class() != Some(MemoryClass::Ordinary)
            || !source
                .allocation_budget()
                .is_some_and(|p| p.same_pool(memory))
        {
            return Err(ExecutionError::Invalid("schedule producer pool/class"));
        }
        self.validate()?;
        let key = instance_key(instance).map_err(ExecutionError::Tree)?;
        let mut times = Updater::<ScheduleCodec<1>, _>::new(
            source,
            empty::<1>().map_err(ExecutionError::Tree)?,
            max_pending.min(256 * 1024),
        )
        .track_parent_nodes();
        let mut instances = Updater::<ScheduleCodec<2>, _>::new(
            source,
            empty::<2>().map_err(ExecutionError::Tree)?,
            max_pending.min(256 * 1024),
        )
        .track_parent_nodes();
        let mut after = self.clone();
        let mut final_buckets = Vec::with_capacity(2);
        if let Some(wake) = old_wake {
            let (count, root) = bucket(&mut times, &after.node_root, wake)?
                .ok_or(ExecutionError::Invalid("schedule old bucket absent"))?;
            match instances
                .lookup(&root, &key)
                .map_err(ExecutionError::Tree)?
            {
                Some(Leaf::Instance(id)) if id == instance => {}
                _ => {
                    return Err(ExecutionError::Invalid(
                        "schedule old instance absent/collision",
                    ));
                }
            }
            if old_wake != new_wake {
                let next = instances
                    .replace(&root, 0, &key, None)
                    .map_err(ExecutionError::Tree)?
                    .0;
                let remaining = count
                    .checked_sub(1)
                    .ok_or(ExecutionError::Invalid("schedule bucket underflow"))?;
                let leaf = if remaining == 0 {
                    if next != empty::<2>().map_err(ExecutionError::Tree)?[0] {
                        return Err(ExecutionError::Invalid("schedule bucket count mismatch"));
                    }
                    after.buckets = after
                        .buckets
                        .checked_sub(1)
                        .ok_or(ExecutionError::Invalid("schedule buckets underflow"))?;
                    None
                } else {
                    final_buckets.push(next.clone());
                    Some(Leaf::Time {
                        wake,
                        count: remaining,
                        root: next,
                    })
                };
                after.node_root = times
                    .replace(&after.node_root, 0, &time_key(wake), leaf.as_ref())
                    .map_err(ExecutionError::Tree)?
                    .0;
                after.entries = after
                    .entries
                    .checked_sub(1)
                    .ok_or(ExecutionError::Invalid("schedule entries underflow"))?;
            }
        }
        if old_wake != new_wake
            && let Some(wake) = new_wake
        {
            let previous = bucket(&mut times, &after.node_root, wake)?;
            let (count, root) = previous
                .clone()
                .unwrap_or((0, empty::<2>().map_err(ExecutionError::Tree)?[0].clone()));
            if instances
                .lookup(&root, &key)
                .map_err(ExecutionError::Tree)?
                .is_some()
            {
                return Err(ExecutionError::Invalid(
                    "schedule new instance already present/collision",
                ));
            }
            let root = instances
                .insert(&root, 0, &Leaf::Instance(instance.to_owned()))
                .map_err(ExecutionError::Tree)?
                .0;
            final_buckets.push(root.clone());
            let leaf = Leaf::Time {
                wake,
                count: count
                    .checked_add(1)
                    .ok_or(ExecutionError::Invalid("schedule bucket overflow"))?,
                root,
            };
            after.node_root = times
                .insert(&after.node_root, 0, &leaf)
                .map_err(ExecutionError::Tree)?
                .0;
            if previous.is_none() {
                after.buckets = after
                    .buckets
                    .checked_add(1)
                    .ok_or(ExecutionError::Invalid("schedule buckets overflow"))?;
            }
            after.entries = after
                .entries
                .checked_add(1)
                .ok_or(ExecutionError::Invalid("schedule entries overflow"))?;
        }
        after.validate()?;
        let mut nodes = BTreeMap::new();
        for root in final_buckets {
            for (digest, bytes) in instances
                .final_writes(&root)
                .map_err(ExecutionError::Tree)?
            {
                nodes.insert(digest, (SchedulePageKindV1::Instances, bytes));
            }
        }
        for (digest, bytes) in times
            .final_writes(&after.node_root)
            .map_err(ExecutionError::Tree)?
        {
            if nodes
                .insert(digest, (SchedulePageKindV1::Times, bytes))
                .is_some()
            {
                return Err(ExecutionError::Invalid("schedule kind digest collision"));
            }
        }
        let delta = ScheduleDeltaV1 {
            before: self.clone(),
            after,
            instance: instance.to_owned(),
            old_wake,
            new_wake,
            new_nodes: nodes
                .into_iter()
                .map(|(digest, (kind, bytes))| SchedulePageWriteV1 {
                    kind,
                    digest,
                    bytes,
                })
                .collect(),
        };
        let size = 1024
            + delta.instance.capacity()
            + delta.new_nodes.capacity() * std::mem::size_of::<SchedulePageWriteV1>()
            + delta
                .new_nodes
                .iter()
                .map(|n| n.digest.capacity() + n.bytes.capacity())
                .sum::<usize>();
        let charge = phase
            .split(size)
            .map_err(|_| ExecutionError::Invalid("schedule delta admission"))?;
        Ok(charge.retain(delta))
    }
}
fn bucket<R: MerkleNodeReader>(
    view: &mut Updater<'_, ScheduleCodec<1>, R>,
    root: &str,
    wake: u64,
) -> Result<Option<(u64, String)>, ExecutionError> {
    match view
        .lookup(root, &time_key(wake))
        .map_err(ExecutionError::Tree)?
    {
        None => Ok(None),
        Some(Leaf::Time {
            wake: found,
            count,
            root,
        }) if found == wake => Ok(Some((count, root))),
        _ => Err(ExecutionError::Invalid("schedule bucket key/type")),
    }
}
