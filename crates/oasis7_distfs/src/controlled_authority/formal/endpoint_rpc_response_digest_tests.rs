use super::*;
use crate::controlled_authority::replicated_protocol::{ArtifactObject, ClosedRecord, HeadAnchor};
use oasis7_formal_protocol::controlled_authority::formal::{
    bounded_context::{FormalBoundedContextV1 as Context, FormalBoundedWorkV1 as Work},
    context_wire,
    crypto::{bytes as original_bytes, hash as original_hash},
    test_fixtures::{h, initial},
};
use oasis7_proto::memory_budget::MemoryBudget;
use std::mem::size_of;

fn body() -> ResponseBody {
    let (_, authority, proof, commit) = initial();
    let record = &proof.decision.record;
    let candidate = FormalCandidateMaterialV1 {
        candidate: SignedCanonicalCandidateV1 {
            record: record.clone(),
            writer_signature_hex: proof.decision.writer_signature_hex.clone(),
        },
        material: ClosedRecord {
            payload_digest: record.payload_digest.clone(),
            before_state_root: record.before_state_root.clone(),
            after_state_root: record.after_state_root.clone(),
            roots: record.artifact_roots.clone(),
            objects: vec![ArtifactObject {
                content_hash: h(9),
                bytes: vec![0, 127, 128, 255],
                references: vec![h(10)],
            }],
        },
        frozen: None,
        frozen_material: None,
        branch: FormalCompletionBranchV1::Normal,
    };
    let head = commit.head();
    let checkpoint = fencing::EndpointCheckpointBindingV1 {
        revision: 1,
        commitment: h(11),
        head: HeadAnchor {
            position: head.position,
            decision_hash: head.record_id.clone(),
            qualified: true,
        },
    };
    let instance_digest = authority.definition().primary.instance.digest().unwrap();
    // Closed encoding DATA only: these bytes assert neither ancestry nor GEN.
    ResponseBody {
        schema_version: 1,
        request_digest: record.request_digest.clone(),
        challenge: h(3),
        world: authority.definition().world.clone(),
        authority_id: authority.authority_id().unwrap(),
        instance_digest: instance_digest.clone(),
        minimum: authority.definition().world.genesis_head().unwrap(),
        head,
        checkpoint: checkpoint.clone(),
        checkpoint_bytes: vec![0, 127, 128, 255],
        ancestry: vec![[0, 127, 128, 255].repeat(4096), vec![255, 0]],
        provider: fencing::MonotonicProviderEvidenceV1 {
            statement: fencing::MonotonicProviderStatementV1 {
                schema_version: 1,
                capability: fencing::ProviderCapabilityV1::ControlledLocalTest,
                resource_binding: h(12),
                instance_digest,
                checkpoint,
                epoch_floor: 1,
                seal_digest: None,
            },
            signature: h(13),
        },
        result: Ok(Value::Query(Box::new(FormalQueryOutcomeV1::Recorded(
            Box::new(FormalRequestStateV1::Qualified { candidate, proof }),
        )))),
    }
}

fn work() -> Work {
    Work::new(100_000_000, 1 << 34).unwrap()
}

fn assert_cause(error: FormalError, expected: &str) {
    assert!(
        matches!(&error, FormalError::Invalid(message) if message == expected),
        "{error:?}"
    );
}

#[test]
fn paid_response_digest_preserves_original_nonempty_query_and_error_bytes() {
    let mut body = body();
    for kind in 0..3 {
        if kind == 1 {
            body.result = Ok(Value::Query(Box::new(FormalQueryOutcomeV1::NotRecorded)));
        } else if kind == 2 {
            body.result = Err(FormalError::Conflict("original remote error".into()));
        }
        assert!(!body.checkpoint_bytes.is_empty());
        assert!(body.ancestry.iter().all(|node| !node.is_empty()));
        assert!(serde_cbor::to_vec(&body).unwrap().len() > 8192);
        let memory = MemoryBudget::new(4 << 20, 0).unwrap();
        let other = MemoryBudget::new(4 << 20, 0).unwrap();
        let work = work();
        let digest =
            paid_response_digest(&body, &mut Context::new(&memory, &work, &mut || false)).unwrap();
        let expected = original_hash(RESPONSE_DIGEST_DOMAIN, &body).unwrap();
        assert_eq!(digest.as_str(), expected);
        assert_eq!(
            original_bytes(RESPONSE_DOMAIN, &digest.as_str()).unwrap(),
            original_bytes(RESPONSE_DOMAIN, &expected).unwrap()
        );
        assert!(digest.uses_budget(&memory));
        assert!(!digest.uses_budget(&other));
        assert_eq!(digest.reserved_bytes(), 64 + size_of::<String>());
        assert_eq!(memory.used(), digest.reserved_bytes());
        drop(digest);
        assert_eq!(memory.used(), 0);
        assert!(work.remaining_steps() < 100_000_000);
        assert!(work.remaining_read_bytes() < 1 << 34);
        assert!(!work.is_child_active());
    }
}

#[test]
fn paid_response_digest_admits_payload_and_digest_simultaneously() {
    let body = body();
    let payload = serde_cbor::to_vec(&body).unwrap();
    let peak = payload.len() + size_of::<Vec<u8>>() + 64 + size_of::<String>();
    let work = work();
    for limit in [0, peak - 1, peak] {
        let memory = MemoryBudget::new(limit, 0).unwrap();
        let mut stop = || false;
        let mut ctx = Context::new(&memory, &work, &mut stop);
        if limit > 0 {
            let encoded =
                context_wire::encode_paid(&body, 64 << 20, &mut ctx, "wire", "encode").unwrap();
            assert_eq!(encoded.as_slice(), payload.as_slice());
            drop(encoded);
            assert_eq!(memory.used(), 0);
        }
        let before = (work.remaining_steps(), work.remaining_read_bytes());
        match paid_response_digest(&body, &mut ctx) {
            Ok(digest) => {
                assert_eq!(limit, peak);
                assert!(digest.uses_budget(&memory));
                assert_eq!(
                    digest.as_str(),
                    original_hash(RESPONSE_DIGEST_DOMAIN, &body).unwrap()
                );
            }
            Err(error) if limit == 0 => assert!(
                matches!(&error, FormalError::Invalid(message) if message.starts_with("formal RPC response body encoding:") && message.contains("Memory")),
                "{error:?}"
            ),
            Err(error) => {
                assert_eq!(limit, peak - 1);
                assert_cause(error, "formal RPC response digest admission");
            }
        }
        assert!(work.remaining_steps() < before.0);
        assert!(work.remaining_read_bytes() < before.1);
        assert!(!work.is_child_active());
        assert_eq!(memory.used(), 0);
    }
}

#[test]
fn paid_response_digest_cancellation_keeps_first_cause_and_closes_child() {
    let body = body();
    let memory = MemoryBudget::new(0, 0).unwrap();
    let empty = Work::new(0, 0).unwrap();
    assert_cause(
        paid_response_digest(&body, &mut Context::new(&memory, &empty, &mut || true)).unwrap_err(),
        "bounded archive read cancelled",
    );
    assert_eq!(
        (empty.remaining_steps(), empty.remaining_read_bytes()),
        (0, 0)
    );
    assert!(!empty.is_child_active());
    let work = work();
    let mut calls = 0;
    let mut stop = || {
        if work.is_child_active() {
            calls += 1;
        }
        calls >= 5
    };
    assert_cause(
        paid_response_digest(&body, &mut Context::new(&memory, &work, &mut stop)).unwrap_err(),
        "bounded archive read cancelled",
    );
    assert!(work.remaining_steps() < 100_000_000);
    assert!(work.remaining_read_bytes() < 1 << 34);
    assert!(!work.is_child_active());
    assert_eq!(memory.used(), 0);
}

#[test]
fn paid_response_digest_never_refills_exhausted_steps_or_bytes() {
    let body = body();
    let memory = MemoryBudget::new(4 << 20, 0).unwrap();
    for bytes in [false, true] {
        let work = work();
        let mut stop = || false;
        let mut ctx = Context::new(&memory, &work, &mut stop);
        for _ in 0..2 {
            drop(paid_response_digest(&body, &mut ctx).unwrap());
        }
        let cause = if bytes {
            ctx.consume_read_bytes(work.remaining_read_bytes()).unwrap();
            "bounded archive read bytes"
        } else {
            ctx.consume_steps(work.remaining_steps()).unwrap();
            "bounded archive read steps"
        };
        for _ in 0..2 {
            let before = (work.remaining_steps(), work.remaining_read_bytes());
            assert_cause(paid_response_digest(&body, &mut ctx).unwrap_err(), cause);
            assert!(work.remaining_steps() <= before.0);
            assert!(work.remaining_read_bytes() <= before.1);
            assert!(!work.is_child_active());
            assert_eq!(memory.used(), 0);
        }
    }
}
