//! Modeled progress tests; endpoint authentication is covered by real RPC tests.
use super::*;
use oasis7_distfs::controlled_authority::formal::{
    NativeEnvironmentVerificationProgressV1, fencing::EndpointInstanceBindingV1,
};
use oasis7_distfs::controlled_authority::replicated_protocol::EndpointRole;
fn progress(index: usize, complete: bool, chunks: u64) -> NativeEnvironmentVerificationProgressV1 {
    NativeEnvironmentVerificationProgressV1 {
        complete,
        verified_chunks: chunks,
        descriptor_digest: "a".repeat(64),
        endpoint_instance: EndpointInstanceBindingV1 {
            schema_version: 1,
            world_id: "modeled-world".into(),
            chain_id: "chain".into(),
            genesis_digest: "b".repeat(64),
            authority_epoch: 1,
            endpoint_role: if index == 0 {
                EndpointRole::Primary
            } else {
                EndpointRole::Replica
            },
            endpoint_id: format!("endpoint-{index}"),
            endpoint_public_key: "c".repeat(64),
            instance_id: format!("{index:064x}"),
            storage_id: "d".repeat(64),
            storage_fingerprint: "e".repeat(64),
        },
    }
}
#[test]
fn native_round_robin_renews_completed_peer_while_other_endpoint_advances() {
    let mut calls = Vec::new();
    let mut counts = [0, 0];
    verify_progress(&"a".repeat(64), 2, |index| {
        calls.push(index);
        counts[index] += 1;
        Ok(if index == 0 || counts[index] == 2 {
            progress(index, true, 2)
        } else {
            progress(index, false, 1)
        })
    })
    .unwrap();
    assert_eq!(calls, [0, 1, 0, 1]);
}
#[test]
fn native_round_robin_rejects_replaced_completed_job_and_stalled_progress() {
    for replaced in [true, false] {
        let mut counts = [0, 0];
        let result = verify_progress(&"a".repeat(64), 2, |index| {
            counts[index] += 1;
            let mut value = progress(index, index == 0, if index == 0 { 2 } else { 1 });
            if replaced && index == 0 && counts[index] == 2 {
                value.endpoint_instance.instance_id = "f".repeat(64);
            }
            Ok(value)
        });
        assert!(result.is_err());
        assert!(counts.into_iter().all(|count| count <= 3));
    }
    assert!(verify_progress(&"a".repeat(64), 2, |index| Ok(progress(index, true, 1))).is_err());
}
