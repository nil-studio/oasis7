//! Exact-parent execution memo; durable bytes are rechecked by the caller.
use super::*;

#[derive(PartialEq, Eq)]
pub(super) struct ParentMemoKey {
    identity: String,
}
impl ParentMemoKey {
    pub(super) fn new(
        material_identity: &str,
        origin_digest: &str,
        material_pin_digest: &str,
        cursor: u64,
        records: Vec<String>,
        origin_nodes: &[(String, Vec<u8>)],
    ) -> Result<Self, String> {
        let bytes = serde_cbor::to_vec(&(
            "oasis7.formal.bft.exact_parent_memo.v3",
            material_identity,
            origin_digest,
            material_pin_digest,
            cursor,
            records,
            origin_nodes,
        ))
        .map_err(|_| "encode exact Node parent memo identity")?;
        Ok(Self {
            identity: blake3_hex(&bytes),
        })
    }
}

pub(super) struct ParentMemo {
    pub(super) key: ParentMemoKey,
    pub(super) parent: FormalBftHeaderExecutionV3,
}
