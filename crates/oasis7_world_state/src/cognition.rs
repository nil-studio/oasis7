use unicode_normalization::UnicodeNormalization;
pub fn is_canonical_identifier(value: &str, max_bytes: usize) -> bool {
    !value.trim().is_empty()
        && value.len() <= max_bytes
        && !value.chars().any(char::is_control)
        && value.nfc().collect::<String>() == value
}
pub use oasis7_agent_api::world_cognition::finality_binding_is_legal;

use serde::Serialize;
const WORLD_STATE_HASH_DOMAIN_V1: &str = "oasis7.runtime.world-state.v1";
#[derive(Debug, Serialize)]
struct WorldStateHashInput<'a> {
    world_id: &'a str,
    branch_id: &'a str,
    finality_epoch: u64,
    finality_block_hash: Option<&'a str>,
    finality_status: &'a str,
    logical_tick: u64,
    state_root: &'a str,
    reorg_epoch: u64,
    runtime_manifest_hash: &'a str,
}
fn h_v1<T: Serialize>(domain: &str, payload: &T) -> String {
    let bytes = oasis7_wasm_abi::encode_canonical_cbor(&(domain, payload))
        .expect("cognition identity payload must be canonicalizable");
    format!("blake3:{}", blake3::hash(&bytes))
}
#[expect(
    clippy::too_many_arguments,
    reason = "This hash preimage is a stable replay/persistence contract; grouping fields would change call-site/API shape."
)]
pub fn world_state_binding_digest_v1(
    world_id: &str,
    branch_id: &str,
    finality_epoch: u64,
    finality_block_hash: Option<&str>,
    finality_status: &str,
    logical_tick: u64,
    state_root: &str,
    reorg_epoch: u64,
    runtime_manifest_hash: &str,
) -> String {
    h_v1(
        WORLD_STATE_HASH_DOMAIN_V1,
        &WorldStateHashInput {
            world_id,
            branch_id,
            finality_epoch,
            finality_block_hash,
            finality_status,
            logical_tick,
            state_root,
            reorg_epoch,
            runtime_manifest_hash,
        },
    )
}
