//! V2 changes delta writes, never node bytes, result-root identity or old deltas.
use super::*;
use std::collections::BTreeSet;

impl BusinessIndexDeltaV1 {
    /// Construct an explicitly versioned v2 delta. Updates keep their canonical
    /// key order and v1 append-only rules. Only final reachable nodes absent
    /// from the authenticated parent tree are emitted, ordered by digest.
    ///
    /// The wire fields are shared with v1; `schema_version` selects the exact
    /// verifier. `build` continues to produce byte-identical v1 histories.
    pub fn build_v2(
        base: BusinessIndexRootV1,
        updates: Vec<BusinessIndexLeafV1>,
        reader: &impl BusinessIndexNodeReader,
    ) -> Result<Self, String> {
        base.validate()?;
        validate_updates(&updates)?;
        let input_bytes = updates.iter().try_fold(0usize, |total, leaf| {
            total
                .checked_add(leaf.canonical_value.len())
                .ok_or("business index v2 input byte overflow")
        })?;
        if input_bytes > BUSINESS_INDEX_MAX_DELTA_BYTES {
            return Err("business index v2 input byte bound".into());
        }
        let parent_nodes = parent_update_paths(&base, &updates, reader)?;
        let (new, writes) = apply_updates(&base, &updates, reader)?;
        let pending: BTreeMap<_, _> = writes
            .into_iter()
            .map(|write| (write.digest, write.bytes))
            .collect();
        let empty = empty_hashes()?;
        let mut reachable = BTreeMap::new();
        let mut stack = vec![(new.node_root.clone(), 0usize)];
        while let Some((digest, depth)) = stack.pop() {
            if digest == empty[depth] || parent_nodes.contains(&digest) {
                continue;
            }
            let Some(bytes) = pending.get(&digest) else {
                // An unchanged child subtree is already bound by the parent.
                // No CAS existence probe: unrelated stored nodes cannot alter
                // a delta's deterministic write set.
                continue;
            };
            if reachable.insert(digest.clone(), bytes.clone()).is_some() {
                continue;
            }
            match BusinessIndexNodeV1::decode(bytes)?.kind {
                BusinessIndexNodeKindV1::Branch {
                    depth: stored,
                    left_hash,
                    right_hash,
                } if usize::from(stored) == depth && depth < BUSINESS_INDEX_DEPTH => {
                    stack.push((right_hash, depth + 1));
                    stack.push((left_hash, depth + 1));
                }
                BusinessIndexNodeKindV1::Leaf(_) if depth == BUSINESS_INDEX_DEPTH => {}
                _ => return Err("business index v2 final node depth".into()),
            }
        }
        let delta = Self {
            schema_version: BUSINESS_INDEX_SCHEMA_V2,
            base_root: base.canonical_result_root()?,
            base,
            new_root: new.canonical_result_root()?,
            new,
            updates,
            node_writes: reachable
                .into_iter()
                .map(|(digest, bytes)| BusinessIndexNodeWriteV1 { digest, bytes })
                .collect(),
        };
        delta.validate_shape()?;
        Ok(delta)
    }
}

/// Every potentially regenerated parent node lies on an updated key's path.
/// Collect only those paths, not all historical leaves or arbitrary CAS nodes.
fn parent_update_paths(
    base: &BusinessIndexRootV1,
    updates: &[BusinessIndexLeafV1],
    reader: &impl BusinessIndexNodeReader,
) -> Result<BTreeSet<String>, String> {
    let empty = empty_hashes()?;
    let mut parent = BTreeSet::new();
    for leaf in updates {
        let key = digest_bytes(&leaf.key_digest)?;
        let mut current = base.node_root.clone();
        for (depth, empty_hash) in empty.iter().enumerate() {
            if current == *empty_hash {
                break;
            }
            let node = read_node(reader, &current, depth)?
                .ok_or("business index v2 parent node missing")?;
            parent.insert(current);
            match node.kind {
                BusinessIndexNodeKindV1::Branch {
                    left_hash,
                    right_hash,
                    ..
                } => {
                    current = if key_bit(&key, depth) {
                        right_hash
                    } else {
                        left_hash
                    };
                }
                BusinessIndexNodeKindV1::Leaf(existing) => {
                    if existing.key_digest != leaf.key_digest {
                        return Err("business index key digest collision".into());
                    }
                    break;
                }
            }
        }
    }
    Ok(parent)
}

#[cfg(test)]
#[path = "business_index_v2_tests.rs"]
mod tests;
