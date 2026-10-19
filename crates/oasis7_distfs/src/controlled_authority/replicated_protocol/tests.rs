use super::*;
use ed25519_dalek::SigningKey;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use storage::{Fault, Phase};

pub(super) fn key(n: u8) -> SigningKey {
    SigningKey::from_bytes(&[n; 32])
}
pub(super) fn trust() -> FixedTrust {
    FixedTrust {
        world_id: "world-test".into(),
        chain_id: "chain-test".into(),
        genesis_digest: "a".repeat(64),
        authority_epoch: 4,
        writer_key: hex::encode(key(1).verifying_key().to_bytes()),
        primary_id: "primary".into(),
        primary_key: hex::encode(key(2).verifying_key().to_bytes()),
        replica_id: "replica".into(),
        replica_key: hex::encode(key(3).verifying_key().to_bytes()),
    }
}
pub(super) fn request(id: &str) -> LocalRequestIdentity {
    LocalRequestIdentity {
        verified_subject: "player-test".into(),
        operation_domain: "recipe".into(),
        nonce_scope: "session1".into(),
        request_id: id.into(),
    }
}
pub(super) fn record() -> ClosedRecord {
    let roles = [
        ArtifactRole::Input,
        ArtifactRole::Result,
        ArtifactRole::Snapshot,
        ArtifactRole::Journal,
        ArtifactRole::NonceIndex,
        ArtifactRole::EffectOutbox,
        ArtifactRole::ExecutionManifest,
        ArtifactRole::Wasm,
        ArtifactRole::Rules,
    ];
    let mut roots = BTreeMap::new();
    let mut objects = Vec::new();
    for (i, role) in roles.into_iter().enumerate() {
        let bytes = vec![i as u8; 17];
        let content_hash = blake3::hash(&bytes).to_hex().to_string();
        roots.insert(role, content_hash.clone());
        objects.push(ArtifactObject {
            content_hash,
            bytes,
            references: Vec::new(),
        });
    }
    objects.sort_by(|a, b| a.content_hash.cmp(&b.content_hash));
    ClosedRecord {
        payload_digest: roots[&ArtifactRole::Input].clone(),
        before_state_root: "b".repeat(64),
        after_state_root: "c".repeat(64),
        roots,
        objects,
    }
}
pub(super) fn proposal(id: &str, parent: &HeadAnchor) -> SignedProposal {
    crypto::sign_proposal(
        ProposalBody {
            schema_version: 1,
            scope: EVIDENCE_SCOPE.into(),
            trust: trust(),
            position: parent.position + 1,
            parent_hash: parent.decision_hash.clone(),
            request: request(id),
            record: record(),
            frozen_input_digest: None,
        },
        &key(1),
    )
    .unwrap()
}
static SERIAL: AtomicU64 = AtomicU64::new(0);
pub(super) fn root() -> PathBuf {
    loop {
        let p = std::env::temp_dir().join(format!(
            "oasis7-replicated-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        match std::fs::create_dir(&p) {
            Ok(()) => return p,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => panic!("{e}"),
        }
    }
}
pub(super) fn endpoints(
    root: &std::path::Path,
    p_min: &HeadAnchor,
    r_min: &HeadAnchor,
) -> (FileEndpoint, FileEndpoint) {
    (
        FileEndpoint::open(
            root.join("p"),
            trust(),
            EndpointRole::Primary,
            key(2),
            p_min,
        )
        .unwrap(),
        FileEndpoint::open(
            root.join("r"),
            trust(),
            EndpointRole::Replica,
            key(3),
            r_min,
        )
        .unwrap(),
    )
}
pub(super) fn coordinator(root: &std::path::Path) -> ReplicatedCoordinator {
    let genesis = trust().genesis_anchor().unwrap();
    let (p, r) = endpoints(root, &genesis, &genesis);
    ReplicatedCoordinator::new(p, r, key(1)).unwrap()
}
pub(super) fn qualified(o: ProtocolOutcome) -> DurabilityEvidence {
    match o {
        ProtocolOutcome::DurabilityQualified(e) => *e,
        other => panic!("not qualified: {other:?}"),
    }
}

#[test]
#[cfg(unix)]
fn real_two_endpoint_idempotency_and_offline_evidence() {
    let root = root();
    let mut c = coordinator(&root);
    let genesis = trust().genesis_anchor().unwrap();
    let e = qualified(c.submit(4, &genesis, request("1"), record()).unwrap());
    let head = verify_evidence(&e, &trust(), &genesis).unwrap();
    assert!(head.qualified);
    assert_eq!(
        qualified(c.submit(4, &genesis, request("1"), record()).unwrap()),
        e
    );
    assert_eq!(qualified(c.lookup(&request("1")).unwrap()), e);
    let mut changed = record();
    changed.after_state_root = "d".repeat(64);
    assert!(matches!(
        c.submit(4, &head, request("1"), changed),
        Err(ProtocolError::Invalid(_))
    ));
    for epoch in [0, 3, 5] {
        assert!(c.submit(epoch, &head, request("2"), record()).is_err());
    }
    assert!(c.submit(4, &genesis, request("2"), record()).is_err());
    let second = qualified(c.submit(4, &head, request("2"), record()).unwrap());
    let second_head = verify_evidence(&second, &trust(), &head).unwrap();
    let (p, r) = c.into_endpoints();
    drop((p, r));
    let (mut p, mut r) = endpoints(&root, &second_head, &second_head);
    assert!(matches!(
        p.lookup(&request("2")).unwrap(),
        EndpointStatus::Qualified(_)
    ));
    assert!(matches!(
        r.lookup(&request("2")).unwrap(),
        EndpointStatus::Qualified(_)
    ));
    drop((p, r));
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn rejects_incomplete_closure_and_resource_capacity() {
    let good = record();
    good.validate().unwrap();
    let mut bad = good.clone();
    bad.objects.pop();
    assert!(bad.validate().is_err());
    let mut bad = good.clone();
    bad.objects[0].bytes[0] ^= 1;
    assert!(bad.validate().is_err());
    let mut bad = good.clone();
    bad.objects[0].references.push("d".repeat(64));
    assert!(bad.validate().is_err());
    let mut bad = good.clone();
    bad.roots.remove(&ArtifactRole::Wasm);
    assert!(bad.validate().is_err());
    let mut bad = good.clone();
    bad.objects.push(bad.objects[0].clone());
    assert!(bad.validate().is_err());
    let mut bad = good;
    bad.objects[0].bytes = vec![0; package::MAX_RECORD_BYTES + 1];
    assert!(bad.validate().is_err());
    // Hit ArtifactObject.bytes; wrong outer shape rejection is not capacity proof.
    let mut bytes = vec![0xa3];
    bytes.extend(serde_cbor::to_vec(&"content_hash").unwrap());
    bytes.extend(serde_cbor::to_vec(&"a".repeat(64)).unwrap());
    bytes.extend(serde_cbor::to_vec(&"bytes").unwrap());
    bytes.push(0x9b);
    bytes.extend_from_slice(&u64::MAX.to_be_bytes());
    let error = serde_cbor::from_slice::<ArtifactObject>(&bytes).unwrap_err();
    assert!(error.to_string().contains("capacity limit"));
}
#[test]
#[cfg(unix)]
fn prepare_only_does_not_qualify_and_competing_parent_has_one_decision() {
    let root = root();
    let genesis = trust().genesis_anchor().unwrap();
    let (mut p, mut r) = endpoints(&root, &genesis, &genesis);
    let a = proposal("a", &genesis);
    let b = proposal("b", &genesis);
    p.prepare(&a).unwrap();
    let ra = r.prepare(&a).unwrap();
    assert!(p.prepare(&b).is_err());
    assert!(r.prepare(&b).is_err());
    let rb = ra.clone();
    assert!(matches!(
        p.lookup(&request("a")).unwrap(),
        EndpointStatus::Prepared(_)
    ));
    let pr = p.decide_primary(&a, &ra).unwrap();
    assert!(p.decide_primary(&b, &rb).is_err());
    assert!(matches!(
        p.lookup(&request("a")).unwrap(),
        EndpointStatus::DecisionUnqualified(_)
    ));
    let rr = r.decide_replica(&a, &ra, &pr).unwrap();
    let e = DurabilityEvidence {
        frozen_certificate: None,
        proposal: a.clone(),
        replica_prepare: ra.clone(),
        primary_receipt: pr,
        replica_receipt: rr,
    };
    let mut premature = e.clone();
    premature.replica_receipt = ra;
    assert!(verify_evidence(&premature, &trust(), &genesis).is_err());
    p.finalize(&e).unwrap();
    r.finalize(&e).unwrap();
    assert!(r.decide_replica(&b, &rb, &e.primary_receipt).is_err());
    assert!(matches!(
        r.lookup(&request("b")).unwrap(),
        EndpointStatus::NotRecorded
    ));
    drop((p, r));
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
#[cfg(unix)]
fn forgery_cross_world_wrong_writer_and_distinct_role_trust() {
    let root = root();
    let mut c = coordinator(&root);
    let genesis = trust().genesis_anchor().unwrap();
    let e = qualified(c.submit(4, &genesis, request("x"), record()).unwrap());
    let mut bad = e.clone();
    bad.replica_receipt.signature_hex = bad.primary_receipt.signature_hex.clone();
    assert!(verify_evidence(&bad, &trust(), &genesis).is_err());
    let mut bad = e.clone();
    bad.replica_receipt.signature_hex = bad.replica_receipt.signature_hex.to_uppercase();
    assert!(verify_evidence(&bad, &trust(), &genesis).is_err());
    let mut bad = e.clone();
    bad.proposal.body.record.after_state_root = "e".repeat(64);
    assert!(verify_evidence(&bad, &trust(), &genesis).is_err());
    let mut other = trust();
    other.world_id = "another".into();
    assert!(verify_evidence(&e, &other, &genesis).is_err());
    let mut other = trust();
    other.replica_key = other.primary_key.clone();
    assert!(other.validate().is_err());
    let mut other = trust();
    other.replica_id = other.primary_id.clone();
    assert!(other.validate().is_err());
    let mut forged = proposal("y", &c.heads().0);
    forged.signature_hex = crypto::sign_proposal(forged.body.clone(), &key(9))
        .unwrap()
        .signature_hex;
    assert!(c.endpoints_mut().0.prepare(&forged).is_err());
    drop(c);
    std::fs::remove_dir_all(root).unwrap();
}

const PHASES: [Phase; 5] = [
    Phase::TempCreated,
    Phase::Written,
    Phase::Synced,
    Phase::Renamed,
    Phase::DirectorySynced,
];
#[path = "process_tests.rs"]
mod process_tests;
#[path = "read_failure_tests.rs"]
mod read_failure_tests;
#[test]
#[cfg(unix)]
fn every_persist_phase_and_logical_stage_returns_unknown_then_resumes_original() {
    if crate::test_process::run_isolated(
        concat!(
            module_path!(),
            "::every_persist_phase_and_logical_stage_returns_unknown_then_resumes_original"
        ),
        std::time::Duration::from_secs(180),
    ) {
        return;
    }
    for endpoint in [EndpointRole::Primary, EndpointRole::Replica] {
        for writes in 1..=5 {
            for phase in PHASES {
                let root = root();
                let mut c = coordinator(&root);
                let genesis = trust().genesis_anchor().unwrap();
                let (p, r) = c.endpoints_mut();
                (if endpoint == EndpointRole::Primary {
                    p
                } else {
                    r
                })
                .inject_after(writes, Fault::Error(phase));
                assert!(
                    matches!(c.submit(4,&genesis,request("original"),record()).unwrap(), ProtocolOutcome::Unknown { request: id } if id == request("original"))
                );
                assert!(matches!(
                    c.submit(4, &genesis, request("different"), record())
                        .unwrap(),
                    ProtocolOutcome::Unknown { .. }
                ));
                drop(c);
                let mut recovered = coordinator(&root);
                qualified(
                    recovered
                        .submit(4, &genesis, request("original"), record())
                        .unwrap(),
                );
                drop(recovered);
                std::fs::remove_dir_all(root).unwrap();
            }
        }
    }
}
#[test]
#[cfg(unix)]
fn either_endpoint_loss_recovers_from_complete_evidence_and_minimum_anchor() {
    for role in [EndpointRole::Primary, EndpointRole::Replica] {
        let root = root();
        let mut c = coordinator(&root);
        let genesis = trust().genesis_anchor().unwrap();
        let e = qualified(c.submit(4, &genesis, request("1"), record()).unwrap());
        let head = verify_evidence(&e, &trust(), &genesis).unwrap();
        let (p, r) = c.into_endpoints();
        let history = if role == EndpointRole::Primary {
            r.export_evidence()
        } else {
            p.export_evidence()
        };
        drop((p, r));
        let path = root.join(if role == EndpointRole::Primary {
            "p"
        } else {
            "r"
        });
        std::fs::remove_dir_all(&path).unwrap();
        let n = if role == EndpointRole::Primary { 2 } else { 3 };
        assert!(FileEndpoint::open(&path, trust(), role, key(n), &head).is_err());
        let restored =
            FileEndpoint::restore(&path, trust(), role, key(n), &head, &history).unwrap();
        assert_eq!(restored.head(), head);
        drop(restored);
        let (p, r) = endpoints(&root, &head, &head);
        let mut recovered = ReplicatedCoordinator::new(p, r, key(1)).unwrap();
        assert_eq!(qualified(recovered.lookup(&request("1")).unwrap()), e);
        drop(recovered);
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[path = "trust_disagreement_tests.rs"]
mod trust_disagreement_tests;
