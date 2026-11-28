use super::*;
use crate::controlled_authority::formal::native_environment::{
    NativeEnvironmentChunkBytesV1, NativeEnvironmentChunkV1, NativeEnvironmentComponentV1,
    NativeEnvironmentRole, NativeExecutionEnvironmentV1,
};
use crate::controlled_authority::replicated_protocol::ArtifactObject;
use oasis7_formal_protocol::controlled_authority::formal::{
    bounded_cbor_data::{CborDataLimitsV1, decode_closed_cbor_data_v1},
    bounded_context::{FormalBoundedContextV1 as Context, FormalBoundedWorkV1 as Work},
    context_wire,
    crypto::{bytes as original_bytes, hash as original_hash, verify as original_verify},
    test_fixtures::{h, initial, key, public},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass};
use std::borrow::Borrow;

fn body(command: Command) -> RequestBody {
    RequestBody {
        schema_version: 1,
        world: FormalWorldIdentityV1 {
            world_id: "request-auth".into(),
            chain_id: "chain".into(),
            genesis_digest: h(8),
        },
        authority_id: h(1),
        instance_digest: h(2),
        challenge: h(3),
        minimum: FormalHeadRefV1 {
            position: 0,
            record_id: h(4),
        },
        command,
        archive_read_grant: None,
    }
}

fn prepare() -> Command {
    let decision = initial().2.decision;
    let candidate = SignedCanonicalCandidateV1 {
        record: decision.record,
        writer_signature_hex: decision.writer_signature_hex,
    };
    let record = &candidate.record;
    let material = ClosedRecord {
        payload_digest: record.payload_digest.clone(),
        before_state_root: record.before_state_root.clone(),
        after_state_root: record.after_state_root.clone(),
        roots: record.artifact_roots.clone(),
        objects: vec![ArtifactObject {
            content_hash: h(9),
            bytes: vec![0, 127, 128, 255],
            references: vec![h(10)],
        }],
    };
    // Authentication DATA only; this does not claim valid business material.
    Command::Prepare(Box::new(FormalCandidateMaterialV1 {
        candidate,
        frozen_material: Some(material.clone()),
        material,
        frozen: None,
        branch: FormalCompletionBranchV1::Normal,
    }))
}

fn native() -> Command {
    let bytes = [0, 127, 128, 255].repeat(16 * 1024);
    let digest = blake3::hash(&bytes).to_hex().to_string();
    let descriptor = NativeExecutionEnvironmentV1 {
        schema_version: 1,
        target: "x86_64-unknown-linux-gnu".into(),
        os: "linux".into(),
        arch: "x86_64".into(),
        components: [
            NativeEnvironmentRole::Executable,
            NativeEnvironmentRole::BuildMetadata,
        ]
        .into_iter()
        .map(|role| NativeEnvironmentComponentV1 {
            role,
            name: format!("{role:?}"),
            byte_len: bytes.len() as u64,
            content_digest: digest.clone(),
            chunks: vec![NativeEnvironmentChunkV1 {
                digest: digest.clone(),
                byte_len: bytes.len() as u32,
            }],
        })
        .collect(),
    };
    Command::StageNativeEnvironment(
        Box::new(descriptor),
        vec![NativeEnvironmentChunkBytesV1 { digest, bytes }],
    )
}

fn work() -> Work {
    Work::new(100_000_000, 1 << 34).unwrap()
}

fn signature(body: &RequestBody, domain: &str) -> String {
    hex::encode(
        key(4)
            .sign(&original_bytes(domain, body).unwrap())
            .to_bytes(),
    )
}

fn assert_cause(error: FormalError, expected: &str) {
    assert!(
        matches!(&error, FormalError::Invalid(message) if message == expected),
        "{error:?}"
    );
}

#[test]
fn prepaid_request_auth_preserves_original_preimages_digests_and_true_signatures() {
    for command in [Command::Query(h(5)), prepare(), native()] {
        let body = body(command);
        let memory = MemoryBudget::new(128 << 20, 0).unwrap();
        let work = work();
        let mut stop = || false;
        let mut ctx = Context::new(&memory, &work, &mut stop);
        for domain in [REQUEST_DOMAIN, DIGEST_DOMAIN] {
            let expected = original_bytes(domain, &body).unwrap();
            let paid = frame(domain, &body, &mut ctx).unwrap();
            assert_eq!(paid.as_ref(), expected.as_slice());
            assert!(memory.used() >= expected.len());
            drop(paid);
            assert_eq!(memory.used(), 0);
        }
        let signature = signature(&body, REQUEST_DOMAIN);
        original_verify(REQUEST_DOMAIN, &body, &signature, &public(4)).unwrap();
        verify_request(&body, &signature, &public(4), &mut ctx).unwrap();
        assert_eq!(memory.used(), 0);
        let (digest, fee) = request_digest(&body, &mut ctx).unwrap();
        assert_eq!(digest, original_hash(DIGEST_DOMAIN, &body).unwrap());
        assert!(fee.uses_budget(&memory));
        assert!(fee.bytes() >= digest.capacity());
        assert_eq!(memory.used(), fee.bytes());
        drop(digest);
        assert!(memory.used() > 0);
        drop(fee);
        assert_eq!(memory.used(), 0);
        assert!(work.remaining_steps() < 100_000_000);
        assert!(work.remaining_read_bytes() < 1 << 34);
    }
}

#[test]
fn prepaid_request_auth_admits_the_simultaneous_payload_and_preimage_peak() {
    let body = body(native());
    let payload = serde_cbor::to_vec(&body).unwrap();
    let signature = signature(&body, REQUEST_DOMAIN);
    original_verify(REQUEST_DOMAIN, &body, &signature, &public(4)).unwrap();
    let memory = MemoryBudget::new(payload.len() + 16 * 1024, 0).unwrap();
    let work = work();
    let mut stop = || false;
    let mut ctx = Context::new(&memory, &work, &mut stop);
    let paid = context_wire::encode_paid(&body, 64 << 20, &mut ctx, "wire", "encode").unwrap();
    assert_eq!(paid.as_slice(), payload.as_slice());
    drop(paid);
    assert_eq!(memory.used(), 0);
    let before = (work.remaining_steps(), work.remaining_read_bytes());
    let error = verify_request(&body, &signature, &public(4), &mut ctx).unwrap_err();
    assert_cause(error, "formal RPC request preimage admission");
    assert!(work.remaining_steps() < before.0);
    assert!(work.remaining_read_bytes() < before.1);
    assert!(!work.is_child_active());
    assert_eq!(memory.used(), 0);
}

#[test]
fn prepaid_request_auth_preserves_bad_key_signature_and_domain_rejections() {
    let body = body(Command::Query(h(5)));
    let signature = signature(&body, REQUEST_DOMAIN);
    let wrong_domain = self::signature(&body, "different.request.domain");
    let memory = MemoryBudget::new(128 << 20, 0).unwrap();
    let work = work();
    for (signature, public_key) in [
        (signature.clone(), "bad key".into()),
        ("bad signature".into(), public(4)),
        ("bad signature".into(), "bad key".into()),
        (signature.to_uppercase(), public(4)),
        (signature, public(5)),
        (wrong_domain, public(4)),
    ] {
        let original = original_verify(REQUEST_DOMAIN, &body, &signature, &public_key).unwrap_err();
        let error = verify_request(
            &body,
            &signature,
            &public_key,
            &mut Context::new(&memory, &work, &mut || false),
        )
        .unwrap_err();
        assert_eq!(error.to_string(), original.to_string());
        assert_eq!(memory.used(), 0);
        assert!(!work.is_child_active());
    }
}

#[test]
fn prepaid_request_auth_respects_live_raw_typed_owners_and_retains_digest_fee() {
    const POOL: usize = 128 << 20;
    let body = body(Command::Query(h(5)));
    let request = Request {
        signature: signature(&body, REQUEST_DOMAIN),
        body,
    };
    let memory = MemoryBudget::new(POOL, 0).unwrap();
    let work = work();
    let mut stop = || false;
    let mut ctx = Context::new(&memory, &work, &mut stop);
    let raw = context_wire::encode_paid(&request, 64 << 20, &mut ctx, "wire", "encode").unwrap();
    let paid = decode_closed_cbor_data_v1::<Request>(
        &raw,
        super::super::request_admission::layout(),
        &CborDataLimitsV1 {
            max_wire_bytes: 64 << 20,
            max_graph_bytes: 1 << 30,
            max_depth: 128,
        },
        &mut ctx,
    )
    .unwrap()
    .unwrap_or_else(|refusal| panic!("{}", refusal.message()));
    paid.validate_context(&mut ctx).unwrap();
    assert!(raw.uses_budget(&memory));
    let request: &Request = paid.borrow();
    let retained = memory.used();
    assert!(retained > raw.reserved_bytes());
    let pressure = memory
        .reserve(POOL - retained, MemoryClass::Ordinary)
        .unwrap();
    let before = work.remaining_steps();
    let error =
        verify_request(&request.body, &request.signature, &public(4), &mut ctx).unwrap_err();
    assert_cause(error, "formal RPC request crypto admission");
    assert_eq!(memory.used(), POOL);
    assert!(work.remaining_steps() < before);
    assert!(!work.is_child_active());
    drop(pressure);
    assert_eq!(memory.used(), retained);
    verify_request(&request.body, &request.signature, &public(4), &mut ctx).unwrap();
    assert_eq!(memory.used(), retained);
    let (digest, fee) = request_digest(&request.body, &mut ctx).unwrap();
    assert_eq!(digest, original_hash(DIGEST_DOMAIN, &request.body).unwrap());
    assert!(fee.uses_budget(&memory));
    assert_eq!(memory.used(), retained + fee.bytes());
    drop(raw);
    assert!(memory.used() > fee.bytes());
    drop(paid);
    assert_eq!(memory.used(), fee.bytes());
    drop(digest);
    assert!(memory.used() > 0);
    drop(fee);
    assert_eq!(memory.used(), 0);
}

#[test]
fn prepaid_request_auth_cancellation_preserves_first_cause_and_consumed_work() {
    let body = body(Command::Query(h(5)));
    let signature = signature(&body, REQUEST_DOMAIN);
    let memory = MemoryBudget::new(128 << 20, 0).unwrap();
    let work = work();
    let before = (work.remaining_steps(), work.remaining_read_bytes());
    assert_cause(
        verify_request(
            &body,
            "bad",
            "bad",
            &mut Context::new(&memory, &work, &mut || true),
        )
        .unwrap_err(),
        "bounded archive read cancelled",
    );
    assert_cause(
        request_digest(&body, &mut Context::new(&memory, &work, &mut || true))
            .err()
            .unwrap(),
        "bounded archive read cancelled",
    );
    assert_eq!(
        (work.remaining_steps(), work.remaining_read_bytes()),
        before
    );
    for digest in [false, true] {
        let before = work.remaining_steps();
        let mut stop = || work.is_child_active();
        let mut ctx = Context::new(&memory, &work, &mut stop);
        let error = if digest {
            request_digest(&body, &mut ctx).err().unwrap()
        } else {
            verify_request(&body, &signature, &public(4), &mut ctx).unwrap_err()
        };
        assert_cause(error, "bounded archive read cancelled");
        assert!(work.remaining_steps() < before);
        assert!(!work.is_child_active());
        assert_eq!(memory.used(), 0);
    }
}

#[test]
fn prepaid_request_auth_never_refills_exhausted_steps_or_bytes() {
    let body = body(Command::Query(h(5)));
    let signature = signature(&body, REQUEST_DOMAIN);
    let memory = MemoryBudget::new(128 << 20, 0).unwrap();
    for bytes in [false, true] {
        let work = work();
        let mut stop = || false;
        let mut ctx = Context::new(&memory, &work, &mut stop);
        for _ in 0..2 {
            verify_request(&body, &signature, &public(4), &mut ctx).unwrap();
            drop(request_digest(&body, &mut ctx).unwrap());
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
            assert_cause(
                verify_request(&body, &signature, &public(4), &mut ctx).unwrap_err(),
                cause,
            );
            assert_cause(request_digest(&body, &mut ctx).err().unwrap(), cause);
            assert!(work.remaining_steps() <= before.0);
            assert!(work.remaining_read_bytes() <= before.1);
            assert!(!work.is_child_active());
            assert_eq!(memory.used(), 0);
        }
    }
}
