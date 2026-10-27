//! Exact canonical CBOR admission and prefix accounting, without payload copies.
use super::error;
use crate::{NodeConsensusAction, NodeError};
use oasis7_consensus::formal_bft_payload_v3::BFT_PRIVATE_ACTIONS_MAX_COUNT_V3;
use oasis7_proto::distributed_bft::BFT_MAX_ACTIONS_CBOR_BYTES_V1 as MAX_BYTES;
struct Counter(usize);
impl std::io::Write for Counter {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        self.0 = self
            .0
            .checked_add(b.len())
            .filter(|n| *n <= MAX_BYTES)
            .ok_or_else(|| std::io::Error::other("wire-3 action byte capacity"))?;
        Ok(b.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
fn item_bytes(action: &NodeConsensusAction) -> Result<usize, NodeError> {
    if action.payload_cbor.len() > MAX_BYTES {
        return Err(error("wire-3 single action exceeds batch capacity"));
    }
    let mut count = Counter(0);
    serde_cbor::to_writer(&mut count, action)
        .map_err(|_| error("wire-3 single action exceeds batch capacity"))?;
    Ok(count.0)
}
fn array_header(count: usize) -> usize {
    match count as u64 {
        0..=23 => 1,
        24..=255 => 2,
        256..=65535 => 3,
        65536..=4294967295 => 5,
        _ => 9,
    }
}
pub(super) fn validate_single_action(action: &NodeConsensusAction) -> Result<(), NodeError> {
    if item_bytes(action)?
        .checked_add(array_header(1))
        .is_none_or(|n| n > MAX_BYTES)
    {
        return Err(error("wire-3 single action exceeds batch capacity"));
    }
    Ok(())
}
pub(super) fn select_prefix(
    mut actions: Vec<NodeConsensusAction>,
    engine_limit: usize,
) -> Result<Vec<NodeConsensusAction>, NodeError> {
    actions.sort_by_key(|a| a.action_id);
    let mut count = 0usize;
    let mut bytes = 0usize;
    for action in actions
        .iter()
        .take(engine_limit.min(BFT_PRIVATE_ACTIONS_MAX_COUNT_V3))
    {
        let size = item_bytes(action)?;
        let total = bytes
            .checked_add(size)
            .ok_or_else(|| error("wire-3 batch size overflow"))?;
        if total
            .checked_add(array_header(count + 1))
            .is_none_or(|n| n > MAX_BYTES)
        {
            break;
        }
        bytes = total;
        count += 1;
    }
    if !actions.is_empty() && count == 0 {
        validate_single_action(&actions[0])?;
    }
    actions.truncate(count);
    Ok(actions)
}
#[cfg(test)]
mod tests {
    use super::*;
    use oasis7_consensus::formal_bft_payload_v3::decode_private_actions_v3;
    fn action(id: u64, n: usize) -> NodeConsensusAction {
        NodeConsensusAction::from_payload(id, "test", vec![0; n]).unwrap()
    }
    #[test]
    fn count_and_exact_byte_prefixes_pass_independent_decoder() {
        let actions = (1..=1025).rev().map(|id| action(id, 1)).collect();
        let selected = select_prefix(actions, 4096).unwrap();
        assert_eq!(selected.len(), 1024);
        assert_eq!(
            decode_private_actions_v3(&serde_cbor::to_vec(&selected).unwrap()).unwrap(),
            selected
        );
        for count in [23, 24, 255, 256] {
            let v = (1..=count).map(|id| action(id, 1)).collect::<Vec<_>>();
            let calculated =
                v.iter().map(|a| item_bytes(a).unwrap()).sum::<usize>() + array_header(v.len());
            assert_eq!(calculated, serde_cbor::to_vec(&v).unwrap().len());
        }
        let input = (1..=12).map(|id| action(id, 100_000)).collect();
        let selected = select_prefix(input, 4096).unwrap();
        assert!(selected.len() < 12 && !selected.is_empty());
        let bytes = serde_cbor::to_vec(&selected).unwrap();
        assert!(bytes.len() <= MAX_BYTES);
        assert_eq!(decode_private_actions_v3(&bytes).unwrap(), selected);
        let mut over = selected;
        over.push(action(over.len() as u64 + 1, 100_000));
        assert!(serde_cbor::to_vec(&over).unwrap().len() > MAX_BYTES);
    }
    #[test]
    fn one_element_array_metadata_is_part_of_admission() {
        let mut low = 0;
        let mut high = MAX_BYTES;
        while low < high {
            let mid = low + (high - low).div_ceil(2);
            if validate_single_action(&action(1, mid)).is_ok() {
                low = mid;
            } else {
                high = mid - 1;
            }
        }
        let exact = action(1, low);
        assert!(validate_single_action(&exact).is_ok());
        assert!(validate_single_action(&action(1, low + 1)).is_err());
        assert_eq!(serde_cbor::to_vec(&vec![exact]).unwrap().len(), MAX_BYTES);
    }
}
