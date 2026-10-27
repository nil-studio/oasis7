//! Private action material checked against the independent public V3 identity.
use std::fmt;

use oasis7_formal_protocol::bft::BoundFormalBftHeaderV3;
use serde::{
    Deserialize, Deserializer,
    de::{self, SeqAccess, Visitor},
};

use crate::node_consensus_action::{NodeConsensusAction, compute_consensus_action_root};

pub const BFT_PRIVATE_ACTIONS_MAX_COUNT_V3: usize = 1024;

/// Does not certify execution, permission, quorum or handoff admission.
pub struct ValidatedBftPrivatePayloadV3 {
    bound: BoundFormalBftHeaderV3,
    actions: Vec<NodeConsensusAction>,
}

impl fmt::Debug for ValidatedBftPrivatePayloadV3 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ValidatedBftPrivatePayloadV3")
            .field("header", self.bound.header())
            .field("action_count", &self.actions.len())
            .finish()
    }
}

impl ValidatedBftPrivatePayloadV3 {
    pub fn decode(bound: BoundFormalBftHeaderV3, bytes: &[u8]) -> Result<Self, String> {
        bound.header().verify_private_payload(bytes)?;
        let actions: BoundedActions = serde_cbor::from_slice(bytes)
            .map_err(|_| "invalid bounded BFT private actions".to_string())?;
        let actions = actions.0;
        let encoded =
            serde_cbor::to_vec(&actions).map_err(|_| "encode BFT private actions".to_string())?;
        if encoded != bytes {
            return Err("noncanonical BFT private actions".into());
        }
        let root = compute_consensus_action_root(&actions)
            .map_err(|_| "invalid BFT private action ordering or payload".to_string())?;
        if root != bound.commitment().action_root() {
            return Err("BFT private action root mismatch".into());
        }
        Ok(Self { bound, actions })
    }

    pub fn bound_header(&self) -> &BoundFormalBftHeaderV3 {
        &self.bound
    }

    pub fn actions(&self) -> &[NodeConsensusAction] {
        &self.actions
    }
}

struct BoundedActions(Vec<NodeConsensusAction>);

impl<'de> Deserialize<'de> for BoundedActions {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ActionsVisitor;
        impl<'de> Visitor<'de> for ActionsVisitor {
            type Value = BoundedActions;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a bounded action sequence")
            }

            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                if seq
                    .size_hint()
                    .is_some_and(|size| size > BFT_PRIVATE_ACTIONS_MAX_COUNT_V3)
                {
                    return Err(de::Error::custom("too many BFT private actions"));
                }
                let mut actions = Vec::new();
                while let Some(action) = seq.next_element()? {
                    if actions.len() == BFT_PRIVATE_ACTIONS_MAX_COUNT_V3 {
                        return Err(de::Error::custom("too many BFT private actions"));
                    }
                    actions.push(action);
                }
                Ok(BoundedActions(actions))
            }
        }
        deserializer.deserialize_seq(ActionsVisitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_sequence_rejects_oversized_declaration_before_elements() {
        // Definite CBOR array length 1025, with deliberately absent elements.
        let error = serde_cbor::from_slice::<BoundedActions>(&[0x99, 0x04, 0x01])
            .err()
            .expect("oversized declaration");
        assert!(error.to_string().contains("too many BFT private actions"));
        assert!(serde_cbor::from_slice::<BoundedActions>(&[0xa0]).is_err());
        assert!(
            serde_cbor::from_slice::<BoundedActions>(&[0x80])
                .unwrap()
                .0
                .is_empty()
        );
    }
}
