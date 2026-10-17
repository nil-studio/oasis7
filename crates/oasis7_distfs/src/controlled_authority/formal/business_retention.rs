//! Retention transitions derive roots from hash-closed snapshot bytes, never
//! from a writer's unbound root assertion. Unknown snapshot fields are skipped.
#[path = "retained_history_material.rs"]
mod tick_material;
use super::business_index::{BusinessIndexDeltaV1, BusinessIndexNodeReader, BusinessIndexRootV1};
use super::*;
use crate::controlled_authority::replicated_protocol::ClosedRecord;
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RetainedRoot {
    schema_version: u32,
    index_root: BusinessIndexRootV1,
    root_digest: String,
}
#[derive(Deserialize)]
struct SnapshotDescriptor {
    state: StateDescriptor,
}
#[derive(Deserialize)]
struct StateDescriptor {
    #[serde(default)]
    retained_history_root: Option<RetainedRoot>,
}
fn descriptor(material: &ClosedRecord) -> Result<Option<RetainedRoot>, FormalError> {
    let bytes = material
        .snapshot_bytes()
        .map_err(|e| invalid(e.to_string()))?;
    // Abstract protocol fixtures predate the actual World snapshot contract.
    // Once a root exists, malformed/missing successor state is rejected below.
    let root = if bytes.first().is_some_and(|byte| byte >> 5 == 5) {
        serde_cbor::from_slice::<SnapshotDescriptor>(&bytes)
            .map_err(|error| invalid(format!("retention snapshot descriptor: {error}")))?
            .state
            .retained_history_root
    } else {
        None
    };
    if let Some(root) = root.as_ref() {
        root.index_root.validate().map_err(invalid)?;
        let value = serde_cbor::value::to_value((
            "oasis7.formal.retained-history.root.v1",
            root.schema_version,
            &root.index_root,
        ))
        .map_err(|error| invalid(error.to_string()))?;
        let bytes = serde_cbor::to_vec(&value).map_err(|error| invalid(error.to_string()))?;
        if root.schema_version != 1 || blake3::hash(&bytes).to_hex().as_str() != root.root_digest {
            return Err(invalid("retention snapshot descriptor canonical digest"));
        }
    }
    Ok(root)
}
pub(super) fn verify_retained_transition(
    initial: bool,
    material: &ClosedRecord,
    before: Option<&ClosedRecord>,
    delta: Option<&BusinessIndexDeltaV1>,
    reader: &impl BusinessIndexNodeReader,
) -> Result<(), FormalError> {
    let before_material = before;
    let before = before.map(descriptor).transpose()?.flatten();
    let after = descriptor(material)?;
    if after.is_some() {
        let snapshot_bytes = |material: &ClosedRecord| -> Result<Vec<u8>, FormalError> {
            material
                .snapshot_bytes()
                .map_err(|e| invalid(e.to_string()))
        };
        let prior_bytes = before_material.map(snapshot_bytes).transpose()?;
        tick_material::verify_tick_prefix_material(
            prior_bytes.as_deref(),
            &snapshot_bytes(material)?,
            after.as_ref().map(|r| &r.index_root),
            delta,
            reader,
        )
        .map_err(invalid)?;
    }
    let empty = BusinessIndexRootV1::empty().map_err(invalid)?;
    if initial && after.as_ref().is_some_and(|root| root.index_root != empty) {
        return Err(invalid("fresh initial retained index must be empty"));
    }
    match (before.as_ref(), after.as_ref(), delta) {
        (None, Some(new), None) if initial && new.index_root == empty => Ok(()),
        (None, None, None) => Ok(()),
        (Some(old), Some(new), None) if old == new => {
            if old.index_root.leaf_count > 0 {
                let bytes = reader
                    .read_node(&old.index_root.node_root)
                    .map_err(invalid)?
                    .ok_or_else(|| invalid("retention unchanged referenced root missing"))?;
                if blake3::hash(&bytes).to_hex().as_str() != old.index_root.node_root {
                    return Err(invalid("retention unchanged root checksum"));
                }
                let node =
                    super::business_index::BusinessIndexNodeV1::decode(&bytes).map_err(invalid)?;
                if !matches!(
                    node.kind,
                    super::business_index::BusinessIndexNodeKindV1::Branch { depth: 0, .. }
                ) {
                    return Err(invalid("retention unchanged root branch shape"));
                }
            }
            Ok(())
        }
        (_, Some(new), Some(delta)) => {
            let base = before
                .as_ref()
                .map(|root| root.index_root.clone())
                .unwrap_or(BusinessIndexRootV1::empty().map_err(invalid)?);
            if delta.base != base || delta.new != new.index_root {
                return Err(invalid(
                    "retention delta exact hash-bound before/after snapshot descriptor",
                ));
            }
            delta
                .verify(&base.canonical_result_root().map_err(invalid)?, reader)
                .map_err(invalid)?;
            Ok(())
        }
        _ => Err(invalid(
            "retention root removed/changed without exact closed delta",
        )),
    }
}

#[cfg(test)]
#[path = "business_retention_tests.rs"]
mod tests;
