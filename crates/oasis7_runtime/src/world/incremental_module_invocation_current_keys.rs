//! Bounded current-key enumeration: original SMT keys plus actual pending edits.
//! This helper grants no invocation qualification and never supplies caller IDs.
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionError, ExecutionTransaction},
    incremental_state::{MerkleNodeReader, StateNamespaceV1, StateScanBudget, StateScanContext},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};

pub(super) fn current_keys_v1<M: MerkleNodeReader>(
    tx: &mut ExecutionTransaction<'_, M>,
    state: &M,
    parent_keys: Option<&[Vec<u8>]>,
    namespace: StateNamespaceV1,
    memory: &MemoryBudget,
    scan: &mut StateScanBudget,
    max_entries: usize,
    max_key_bytes: usize,
    pending_entries: &mut usize,
    mut cancelled: impl FnMut() -> bool,
    owns: impl Fn(&[u8]) -> bool,
) -> Result<ReservedValue<Vec<Vec<u8>>>, ExecutionError> {
    tx.contribute(|tx| {
        if !tx.uses_budget(memory)
            || state.allocation_class() != Some(MemoryClass::Ordinary)
            || !state
                .allocation_budget()
                .is_some_and(|pool| pool.same_pool(memory))
        {
            return Err(ExecutionError::ResourceLimited(
                "invocation key source scope",
            ));
        }
        let size = max_entries
            .checked_mul(
                max_key_bytes
                    .checked_add(std::mem::size_of::<Vec<u8>>())
                    .ok_or(ExecutionError::ResourceLimited("invocation key bound"))?,
            )
            .ok_or(ExecutionError::ResourceLimited("invocation key bound"))?;
        let fee = memory
            .reserve(size, MemoryClass::Ordinary)
            .map_err(ExecutionError::Memory)?;
        let mut keys = Vec::with_capacity(max_entries);
        let mut add = |key: &[u8]| -> Result<(), ExecutionError> {
            if !owns(key) {
                return Ok(());
            }
            if key.len() > max_key_bytes || keys.len() == max_entries {
                return Err(ExecutionError::ResourceLimited("invocation keys"));
            }
            keys.push(key.to_vec());
            Ok(())
        };
        if let Some(parent_keys) = parent_keys {
            for key in parent_keys {
                add(key)?;
            }
        } else {
            let result = StateScanContext {
                source: state,
                memory,
                class: MemoryClass::Ordinary,
                io: scan,
            }
            .visit_entries(
                tx.parent_state_root(namespace),
                |entry| add(&entry.key),
                &mut cancelled,
            );
            result.map_err(|_| ExecutionError::ResourceLimited("invocation parent key scan"))?;
        }
        tx.visit_pending_writes(
            namespace,
            pending_entries,
            || {
                if cancelled() {
                    Err(ExecutionError::Aborted)
                } else {
                    Ok(())
                }
            },
            |key, _, _| add(key),
        )?;
        drop(add);
        keys.sort_unstable();
        keys.dedup();
        let mut retained = 0;
        for index in 0..keys.len() {
            if cancelled() {
                return Err(ExecutionError::Aborted);
            }
            if tx.read(namespace, &keys[index])?.is_some() {
                keys.swap(retained, index);
                retained += 1;
            }
        }
        keys.truncate(retained);
        keys.sort_unstable();
        Ok(fee.retain(keys))
    })
}
