use super::discovery_budget::MAX_DISCOVERY_IDS;
use crate::error::WorldError;
use serde::de::{self, Deserialize, Deserializer, SeqAccess, Visitor};

struct PeerIds(Vec<String>);
impl<'de> Deserialize<'de> for PeerIds {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Bounded;
        impl<'de> Visitor<'de> for Bounded {
            type Value = PeerIds;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("at most 128 peer identifiers")
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                if seq.size_hint().is_some_and(|n| n > MAX_DISCOVERY_IDS) {
                    return Err(de::Error::custom("discovery ID limit exceeded"));
                }
                let mut ids = Vec::with_capacity(MAX_DISCOVERY_IDS);
                while let Some(id) = seq.next_element::<String>()? {
                    if ids.len() == MAX_DISCOVERY_IDS {
                        return Err(de::Error::custom("discovery ID limit exceeded"));
                    }
                    ids.push(id);
                }
                Ok(PeerIds(ids))
            }
        }
        deserializer.deserialize_seq(Bounded)
    }
}
pub(super) fn decode(payload: &[u8]) -> Result<Vec<String>, WorldError> {
    Ok(serde_cbor::from_slice::<PeerIds>(payload)?.0)
}
