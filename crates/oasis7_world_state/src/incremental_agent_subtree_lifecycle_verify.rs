//! Full explicit created/deleted actor scope, reusing the bootstrap entry codec.
use super::*;
use oasis7_formal_protocol::{
    incremental_execution::PreparedTransition, incremental_state::StateNodeWriteV1,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass};
struct StateOverlay<'a, M> {
    base: &'a M,
    nodes: &'a [StateNodeWriteV1],
    memory: &'a MemoryBudget,
    class: MemoryClass,
}
enum Node<'a, N> {
    Borrowed(&'a [u8]),
    Base(N),
}
impl<N: AsRef<[u8]>> AsRef<[u8]> for Node<'_, N> {
    fn as_ref(&self) -> &[u8] {
        match self {
            Self::Borrowed(bytes) => bytes,
            Self::Base(node) => node.as_ref(),
        }
    }
}
impl<'a, M: MerkleNodeReader> MerkleNodeReader for StateOverlay<'a, M> {
    type Node = Node<'a, M::Node>;
    fn allocation_budget(&self) -> Option<&MemoryBudget> {
        Some(self.memory)
    }
    fn allocation_class(&self) -> Option<MemoryClass> {
        Some(self.class)
    }
    fn read_node(&self, id: &str, max: usize) -> Result<Option<Self::Node>, String> {
        if let Ok(index) = self
            .nodes
            .binary_search_by(|node| node.digest.as_str().cmp(id))
        {
            let bytes = &self.nodes[index].bytes;
            if bytes.len() > max {
                return Err("agent verified node bound".into());
            }
            return Ok(Some(Node::Borrowed(bytes)));
        }
        Ok(self.base.read_node(id, max)?.map(Node::Base))
    }
}
fn entry_owner(parsed: AgentSubtreeKeyV1<'_>) -> Result<[u8; 32], ValueTreeError> {
    match parsed {
        AgentSubtreeKeyV1::Field { owner, .. } => Ok(owner),
        AgentSubtreeKeyV1::Body { agent } | AgentSubtreeKeyV1::Mailbox { agent, .. } => {
            let key = AgentFieldKeyV1::new(agent, AgentFieldV1::Identity, 0)
                .map_err(ValueTreeError::Invalid)?;
            Ok(AgentFieldKeyV1::from_bytes(key.as_bytes())
                .map_err(ValueTreeError::Invalid)?
                .0)
        }
    }
}
/// Caller already authenticated generic replay and its retained new-node arena.
/// No operation scans an unrelated parent subtree or installs a public receipt.
pub(crate) fn verify_lifecycle_actor<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    prepared: &PreparedTransition,
    agent: &str,
    added: bool,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut AgentSubtreeScanWorkV1,
) -> Result<u64, ValueTreeError> {
    let _metadata = ctx.values.memory.reserve(128 << 10, ctx.values.class)?;
    let target = owner(agent, ctx.key_work, ctx.semantic_bytes)?;
    let roots = if added {
        &prepared.record.after
    } else {
        &prepared.record.before
    };
    let overlay = StateOverlay {
        base: ctx.state,
        nodes: &prepared.new_nodes,
        memory: ctx.values.memory,
        class: ctx.values.class,
    };
    let mut table = BTreeMap::new();
    let mut local = WorldFieldReadContextV1 {
        root: &roots.state[0],
        state: &overlay,
        values: oasis7_formal_protocol::incremental_value::ValueReadContext {
            source: ctx.values.source,
            memory: ctx.values.memory,
            class: ctx.values.class,
            io: &mut *ctx.values.io,
        },
        key_work: &mut *ctx.key_work,
        semantic_bytes: ctx.semantic_bytes,
        record_bytes: ctx.record_bytes,
        take_state_error: &mut ctx.take_state_error,
        cancelled: &mut ctx.cancelled,
    };
    for write in &prepared.record.writes {
        local.check()?;
        work.cells_remaining = work
            .cells_remaining
            .checked_sub(1)
            .ok_or(ValueTreeError::ResourceLimited("agent lifecycle writes"))?;
        let Some(parsed) = parse_agent_subtree_key_v1(write.namespace, &write.mutation.key)? else {
            continue;
        };
        if entry_owner(parsed)? != target {
            continue;
        }
        let value = if added {
            if write.mutation.before.is_some() {
                return Err(invalid());
            }
            write.mutation.after.ok_or_else(invalid)?
        } else {
            if write.mutation.after.is_some() {
                return Err(invalid());
            }
            write.mutation.before.ok_or_else(invalid)?
        };
        verify_entry(
            roots,
            EntryRef {
                namespace: write.namespace,
                key: &write.mutation.key,
                value,
            },
            &mut local,
            work,
            &mut table,
            &mut |_| Ok(()),
        )?;
    }
    let s = table.get(&target).ok_or_else(invalid)?;
    if table.len() != 1 {
        return Err(invalid());
    }
    let references = validate_stats(s)?;
    if !added {
        // Optional resources have no required mask/count: deleting a cell must
        // remove each actual optional member rather than leave an orphan.
        for ordinal in 0..2 {
            let key = key(target, AgentFieldV1::ResourceAmount, ordinal)?;
            let before = local.lookup_raw(
                roots.state_root(AgentFieldV1::ResourceAmount.namespace()),
                key.as_bytes(),
            )?;
            if let Some(before) = before {
                let found = prepared
                    .record
                    .writes
                    .binary_search_by(|write| {
                        (write.namespace, write.mutation.key.as_slice())
                            .cmp(&(AgentFieldV1::ResourceAmount.namespace(), key.as_bytes()))
                    })
                    .map_err(|_| invalid())?;
                let write = &prepared.record.writes[found];
                if write.mutation.before != Some(before) || write.mutation.after.is_some() {
                    return Err(invalid());
                }
            }
        }
    }
    local.check()?;
    Ok(references)
}
