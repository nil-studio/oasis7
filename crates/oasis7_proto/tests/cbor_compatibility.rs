//! Fixed outputs captured with serde_cbor 0.11.2 before migrating to the Kanidm fork.
//! The fixture is an independent oracle: never regenerate it with the replacement codec.
use ed25519_dalek::SigningKey;
use oasis7_proto::distributed::WorldBlock;
use oasis7_proto::distributed_bft::{BftScopeV1, BftVotePhaseV1, BftVoteV1, BftVoteValueV1};
use serde::Serialize;
use serde_cbor::Value;
use std::collections::BTreeMap;

fn samples() -> BTreeMap<String, Vec<u8>> {
    let mut samples = BTreeMap::new();
    let mut record = |name: &str, bytes: Vec<u8>| {
        samples.insert(name.to_owned(), bytes);
    };
    record(
        "integers",
        serde_cbor::to_vec(&[
            i128::from(i64::MIN),
            -257,
            -256,
            -25,
            -24,
            -1,
            0,
            23,
            24,
            255,
            256,
            65535,
            65536,
            i128::from(u32::MAX),
            i128::from(u64::MAX),
        ])
        .unwrap(),
    );
    record(
        "options",
        serde_cbor::to_vec(&((), None::<()>, Some(()), true, false)).unwrap(),
    );
    record(
        "floats",
        serde_cbor::to_vec(&[
            0.0_f64,
            -0.0,
            1.5,
            65504.0,
            f64::from(f32::MIN_POSITIVE),
            f64::MIN_POSITIVE,
            f64::MAX,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NAN,
            std::f64::consts::PI,
        ])
        .unwrap(),
    );
    let map = Value::Map(BTreeMap::from([
        (
            Value::Text("long-key".into()),
            Value::Integer(i128::from(u64::MAX)),
        ),
        (Value::Text("a".into()), Value::Bytes(vec![0, 128, 255])),
        (Value::Integer(-1), Value::Null),
        (Value::Integer(24), Value::Text("世界".into())),
        (Value::Bool(false), Value::Array(vec![Value::Integer(1)])),
    ]));
    record("canonical_map", serde_cbor::to_vec(&map).unwrap());
    let mut described = Vec::new();
    let mut serializer = serde_cbor::ser::Serializer::new(&mut described);
    serializer.self_describe().unwrap();
    map.serialize(&mut serializer).unwrap();
    record("self_described_map", described);
    let vote = BftVoteV1::sign(
        BftScopeV1 {
            world_id: "world-a".into(),
            chain_id: "chain-a".into(),
            genesis_hash: "11".repeat(32),
        },
        1,
        0,
        BftVotePhaseV1::Prevote,
        "22".repeat(32),
        BftVoteValueV1::Nil,
        "validator-a".into(),
        &SigningKey::from_bytes(&[7; 32]),
    )
    .unwrap();
    record("bft_vote_preimage", vote.signing_payload().unwrap());
    record("bft_vote", serde_cbor::to_vec(&vote).unwrap());
    record(
        "finality_vote_preimage",
        oasis7_proto::distributed_finality::world_finality_vote_signing_payload(
            "validator-a",
            u64::MAX,
            "block",
            "state",
            "set",
            "round",
        )
        .unwrap(),
    );
    let block = WorldBlock {
        world_id: "world-a".into(),
        height: u64::MAX,
        prev_block_hash: "11".repeat(32),
        action_root: "22".repeat(32),
        event_root: "33".repeat(32),
        state_root: "44".repeat(32),
        journal_ref: "journal".into(),
        snapshot_ref: "snapshot".into(),
        receipts_root: "55".repeat(32),
        proposer_id: "validator-a".into(),
        timestamp_ms: i64::MIN,
        signature: "signature".into(),
    };
    record("persisted_block", serde_cbor::to_vec(&block).unwrap());
    samples
}

#[test]
fn legacy_cbor_bytes_and_signed_payloads_remain_compatible() {
    let expected: BTreeMap<String, String> =
        serde_json::from_str(include_str!("fixtures/serde_cbor_0_11_2.json")).unwrap();
    let actual: BTreeMap<_, _> = samples()
        .into_iter()
        .map(|(name, bytes)| (name, hex::encode(bytes)))
        .collect();
    assert_eq!(actual, expected);
    let vote_bytes = hex::decode(&expected["bft_vote"]).unwrap();
    let vote: BftVoteV1 = serde_cbor::from_slice(&vote_bytes).unwrap();
    vote.verify_signature().unwrap();
    assert_eq!(
        hex::encode(vote.signing_payload().unwrap()),
        expected["bft_vote_preimage"]
    );
    let block_bytes = hex::decode(&expected["persisted_block"]).unwrap();
    let block: WorldBlock = serde_cbor::from_slice(&block_bytes).unwrap();
    assert_eq!(block.height, u64::MAX);
    assert_eq!(block.timestamp_ms, i64::MIN);
    assert_eq!(serde_cbor::to_vec(&block).unwrap(), block_bytes);
    for name in ["canonical_map", "self_described_map"] {
        let bytes = hex::decode(&expected[name]).unwrap();
        let value: Value = serde_cbor::from_slice(&bytes).unwrap();
        assert_eq!(
            serde_cbor::to_vec(&value).unwrap(),
            hex::decode(&expected["canonical_map"]).unwrap()
        );
    }
}

#[test]
fn stream_boundaries_and_invalid_inputs_preserve_decoder_contract() {
    use serde::Deserialize;
    let bytes = [0x01, 0x02];
    assert!(serde_cbor::from_slice::<u64>(&bytes).is_err());
    let mut decoder = serde_cbor::Deserializer::from_slice(&bytes);
    assert_eq!(u64::deserialize(&mut decoder).unwrap(), 1);
    assert_eq!(decoder.byte_offset(), 1);
    assert_eq!(u64::deserialize(&mut decoder).unwrap(), 2);
    decoder.end().unwrap();
    let mut trailing = serde_cbor::Deserializer::from_slice(&bytes);
    u64::deserialize(&mut trailing).unwrap();
    assert!(trailing.end().is_err());
    for invalid in [vec![0x1b, 0], vec![0x5f, 0x01, 0xff], vec![0x81; 256]] {
        assert!(serde_cbor::from_slice::<Value>(&invalid).is_err());
    }
}
