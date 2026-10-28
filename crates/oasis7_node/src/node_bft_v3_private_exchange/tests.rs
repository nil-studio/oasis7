//! Data-codec vectors only; real authorization/World evidence belongs to the
//! independent multi-Node native tests. No fake private-exchange capability.
use super::*;
use oasis7_consensus::tendermint_v3::{BftHeaderValueV3, FormalBftExecutionCommitmentV1};
use oasis7_proto::distributed_bft::{
    v2::BftScopeV2,
    v3::{BftBlockHeaderV3, actions_cbor_digest_v3},
};
fn h(n: u8) -> String {
    format!("{n:02x}").repeat(32)
}
fn package() -> NodeBftPrivateCandidatePackageV3 {
    let c = FormalBftExecutionCommitmentV1 {
        domain: "oasis7.formal.bft.execution.v1".into(),
        schema_version: 1,
        profile: "bft".into(),
        profile_version: 1,
        activation_digest: h(1),
        scope: BftScopeV2 {
            world_id: "private-codec".into(),
            chain_id: "chain".into(),
            genesis_hash: h(2),
        },
        height: 9,
        parent_commit_ref: h(3),
        parent_tick: 8,
        parent_semantic_state_root: h(4),
        authority_epoch: 1,
        active_validator_set_hash: h(5),
        schedule_policy_hash: h(6),
        manifest_hash: h(7),
        post_manifest_hash: h(7),
        action_root: crate::compute_consensus_action_root(&[]).unwrap(),
        event_root: h(8),
        execution_state_root: h(9),
        state_root: h(10),
        projection_root: h(11),
        result_root: h(12),
        transition: None,
        transition_ref: None,
    };
    NodeBftPrivateCandidatePackageV3 {
        candidate: crate::NodeBftPrivateCandidateV3 {
            public_value: BftHeaderValueV3 {
                header: BftBlockHeaderV3 {
                    protocol_version: 3,
                    execution_commitment_hash: c.digest().unwrap(),
                    actions_cbor_digest: actions_cbor_digest_v3(&[0x80]).unwrap(),
                },
                execution_commitment: c,
            },
            actions_cbor: vec![0x80],
        },
        dependency_nodes: vec![],
    }
}
#[test]
fn private_package_compact_bytes_roundtrip_binds_exact_input() {
    let p = package();
    let b = super::package::encode_package(&p).unwrap();
    assert_eq!(super::package::decode_package(&b).unwrap(), p);
    let mut bad = p.clone();
    bad.candidate.actions_cbor.push(0);
    assert!(super::package::encode_package(&bad).is_err());
    let mut trailing = b;
    trailing.push(0);
    assert!(super::package::decode_package(&trailing).is_err());
}
#[test]
fn private_decoder_declared_containers_are_checked_before_reservation() {
    for b in [
        vec![0x9b, 255, 255, 255, 255, 255, 255, 255, 255],
        vec![0xbb, 255, 255, 255, 255, 255, 255, 255, 255],
        vec![0x5b, 255, 255, 255, 255, 255, 255, 255, 255],
        vec![0x9f, 0xff],
    ] {
        assert!(wire::scan(&b, false).is_err());
        assert!(wire::scan(&b, true).is_err());
    }
    assert!(wire::decode_frame::<wire::Response>(&vec![0; 65537]).is_err());
    assert!(super::package::decode_package(&vec![0; wire::MAX_PACKAGE + 1]).is_err());
}
#[test]
fn private_manifest_reply_is_not_package_data_or_unbounded_frame() {
    let manifest = wire::Manifest {
        package_digest: h(1),
        total_bytes: 16385,
        chunk_size: 16384,
        total_chunks: 2,
    };
    assert_eq!(manifest.operation().chunk_range(1).unwrap(), (16384, 1));
    assert!(manifest.operation().chunk_range(2).is_err());
    let reply = wire::Response {
        domain: wire::DOMAIN.into(),
        request_digest: h(2),
        manifest,
        chunk_index: None,
        chunk_hex: None,
    };
    let b = wire::encode_frame(&reply).unwrap();
    let decoded: wire::Response = wire::decode_frame(&b).unwrap();
    assert!(decoded.chunk_hex.is_none());
    let mut reply = reply;
    reply.chunk_hex = Some("00".repeat(65536));
    assert!(wire::encode_frame(&reply).is_err());
}
