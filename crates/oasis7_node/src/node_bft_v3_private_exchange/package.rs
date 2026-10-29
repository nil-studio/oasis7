use super::wire::{MAX_PACKAGE, encode, scan};
use super::*;
use serde::de::{self, SeqAccess, Visitor};
const DOMAIN: &str = "oasis7.bft.private-candidate.package.v1";
#[derive(Serialize)]
struct Out<'a> {
    domain: &'static str,
    schema_version: u32,
    public_value: &'a BftHeaderValueV3,
    #[serde(serialize_with = "bytes")]
    actions_cbor: &'a [u8],
    dependency_nodes: Vec<OutNode<'a>>,
}
#[derive(Serialize)]
struct OutNode<'a> {
    digest: &'a str,
    #[serde(serialize_with = "bytes")]
    bytes: &'a [u8],
}
fn bytes<S: serde::Serializer>(v: &&[u8], s: S) -> Result<S::Ok, S::Error> {
    s.serialize_bytes(v)
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct In {
    domain: String,
    schema_version: u32,
    public_value: BftHeaderValueV3,
    #[serde(deserialize_with = "actions", serialize_with = "vec_bytes")]
    actions_cbor: Vec<u8>,
    #[serde(deserialize_with = "nodes")]
    dependency_nodes: Vec<InNode>,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct InNode {
    digest: String,
    #[serde(deserialize_with = "node_bytes", serialize_with = "vec_bytes")]
    bytes: Vec<u8>,
}
fn vec_bytes<S: serde::Serializer>(v: &[u8], s: S) -> Result<S::Ok, S::Error> {
    s.serialize_bytes(v)
}
fn actions<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
    read_bytes(d, 1024 * 1024)
}
fn node_bytes<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
    read_bytes(d, 4 * 1024 * 1024 + 128)
}
fn read_bytes<'de, D: serde::Deserializer<'de>>(d: D, max: usize) -> Result<Vec<u8>, D::Error> {
    struct Bound(usize);
    impl Visitor<'_> for Bound {
        type Value = Vec<u8>;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("bounded compact private bytes")
        }
        fn visit_bytes<E: de::Error>(self, v: &[u8]) -> Result<Vec<u8>, E> {
            if v.len() > self.0 {
                return Err(E::custom("private bytes capacity"));
            }
            Ok(v.to_vec())
        }
    }
    d.deserialize_bytes(Bound(max))
}
fn nodes<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<InNode>, D::Error> {
    struct Nodes;
    impl<'de> Visitor<'de> for Nodes {
        type Value = Vec<InNode>;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("at most4096private nodes")
        }
        fn visit_seq<A: SeqAccess<'de>>(self, mut a: A) -> Result<Self::Value, A::Error> {
            if a.size_hint().is_some_and(|n| n > 4096) {
                return Err(de::Error::custom("private nodes declared count"));
            }
            let mut nodes = Vec::new();
            let mut total = 0usize;
            while let Some(n) = a.next_element::<InNode>()? {
                if nodes.len() == 4096 {
                    return Err(de::Error::custom("private nodes count"));
                }
                total = total
                    .checked_add(n.bytes.len())
                    .filter(|n| *n <= 64 * 1024 * 1024)
                    .ok_or_else(|| de::Error::custom("private dependency bytes"))?;
                nodes.push(n);
            }
            Ok(nodes)
        }
    }
    d.deserialize_seq(Nodes)
}
pub(super) fn validate(p: &NodeBftPrivateCandidatePackageV3) -> Result<(), String> {
    oasis7_consensus::formal_bft_payload_v3::ValidatedBftPrivatePayloadV3::decode(
        p.candidate.public_value.validate_binding()?,
        &p.candidate.actions_cbor,
    )?;
    if p.dependency_nodes.len() > 4096 || !p.dependency_nodes.windows(2).all(|n| n[0].0 < n[1].0) {
        return Err("private dependency count/order".into());
    }
    let mut total = 0usize;
    for (d, b) in &p.dependency_nodes {
        wire::hash(d)?;
        if b.len() > 4 * 1024 * 1024 + 128 || blake3::hash(b).to_hex().as_str() != d {
            return Err("private dependency digest/bytes".into());
        }
        oasis7_distfs::controlled_authority::formal::business_index::BusinessIndexNodeV1::decode(b)
            .map_err(|_| "private dependency node schema")?;
        total = total
            .checked_add(b.len())
            .filter(|n| *n <= 64 * 1024 * 1024)
            .ok_or("private closure byte capacity")?;
    }
    Ok(())
}
pub(super) fn encode_package(p: &NodeBftPrivateCandidatePackageV3) -> Result<Vec<u8>, String> {
    validate(p)?;
    let out = Out {
        domain: DOMAIN,
        schema_version: 1,
        public_value: &p.candidate.public_value,
        actions_cbor: &p.candidate.actions_cbor,
        dependency_nodes: p
            .dependency_nodes
            .iter()
            .map(|(digest, bytes)| OutNode { digest, bytes })
            .collect(),
    };
    let b = encode(&out)?;
    if b.len() > MAX_PACKAGE {
        return Err("private package encoded capacity".into());
    }
    Ok(b)
}
pub(super) fn decode_package(b: &[u8]) -> Result<NodeBftPrivateCandidatePackageV3, String> {
    if b.len() > MAX_PACKAGE {
        return Err("private package capacity".into());
    }
    scan(b, true)?;
    let p: In = serde_cbor::from_slice(b).map_err(|_| "private package decode")?;
    if p.domain != DOMAIN || p.schema_version != 1 || encode(&p)? != b {
        return Err("private package canonical domain/schema".into());
    }
    let p = NodeBftPrivateCandidatePackageV3 {
        candidate: crate::NodeBftPrivateCandidateV3 {
            public_value: p.public_value,
            actions_cbor: p.actions_cbor,
        },
        dependency_nodes: p
            .dependency_nodes
            .into_iter()
            .map(|n| (n.digest, n.bytes))
            .collect(),
    };
    validate(&p)?;
    Ok(p)
}
pub(super) fn package_digest(b: &[u8]) -> String {
    wire::digest(DOMAIN.as_bytes(), b)
}
