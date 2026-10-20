use super::*;
use oasis7_distfs::controlled_authority::formal::{
    NormalDurableSetTransitionBodyV1, normal_durable_set_transition_signing_bytes,
};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::formal_read_fixtures as fixtures;

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

struct PrivateTempDirectory(PathBuf);

impl PrivateTempDirectory {
    fn new() -> Self {
        use std::os::unix::fs::PermissionsExt;

        let base = std::env::temp_dir()
            .canonicalize()
            .expect("canonical system temporary directory");
        let unique = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock after epoch")
            .as_nanos();
        let path = base.join(format!(
            "oasis7-durable-set-arm-{}-{stamp}-{unique}",
            std::process::id()
        ));
        fs::create_dir(&path).expect("create private test directory");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700))
            .expect("make private test directory");
        Self(path)
    }
}

impl Drop for PrivateTempDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn signed_transition(
    authority: &oasis7_distfs::controlled_authority::formal::VerifiedAuthorityV1,
    predecessor: &oasis7_distfs::controlled_authority::formal::FormalHeadRefV1,
    operation_id: String,
) -> SignedNormalDurableSetTransitionV1 {
    let mut successor = authority.definition().clone();
    successor.epoch += 1;
    successor.predecessor_authority_id = Some(authority.authority_id().unwrap());
    successor.valid_from_position = predecessor.position + 2;
    successor.primary.instance.authority_epoch = successor.epoch;
    successor.replica.instance.authority_epoch = successor.epoch;
    successor.replica.instance.endpoint_public_key = fixtures::public(12);
    successor.replica.instance.instance_id = fixtures::h(120);
    successor.replica.instance.storage_id = fixtures::h(121);
    successor.replica.instance.storage_fingerprint = fixtures::h(122);
    successor.replica.instance.endpoint_id = "replacement-replica".into();
    successor.replica.fault_domain_id = "replacement-replica-domain".into();
    let mut body = NormalDurableSetTransitionBodyV1 {
        schema_version: 1,
        world: authority.definition().world.clone(),
        predecessor_authority_id: authority.authority_id().unwrap(),
        predecessor: predecessor.clone(),
        boundary_position: predecessor.position + 1,
        operation_id,
        successor,
        independent_closure_root: fixtures::h(123),
    };
    body.independent_closure_root = body.copy_contract_digest().unwrap();
    body.validate(authority).unwrap();
    SignedNormalDurableSetTransitionV1 {
        writer_signature_hex: fixtures::sign(
            normal_durable_set_transition_signing_bytes(&body).unwrap(),
            4,
        ),
        body,
    }
}

#[test]
fn arm_is_private_immutable_and_exact_retry_survives_reopen() {
    let (_, _, anchor, _) = fixtures::actual_world_initial_fixture();
    let operation_id = fixtures::h(124);
    let transition = signed_transition(
        anchor.authority(),
        &anchor.head().head(),
        operation_id.clone(),
    );
    let directory = PrivateTempDirectory::new();
    let first = FormalDurableSetArmControlV1::new(directory.0.clone(), false).unwrap();
    let armed = first
        .arm(&operation_id, transition.clone(), &anchor)
        .unwrap();
    assert_eq!(armed.phase, DurableSetArmPhaseV1::Armed);
    assert_eq!(armed.boundary_position, anchor.head().record().position + 1);

    let reopened = FormalDurableSetArmControlV1::new(directory.0.clone(), false).unwrap();
    assert_eq!(
        reopened
            .arm(&operation_id, transition.clone(), &anchor)
            .unwrap(),
        armed
    );
    assert_eq!(
        reopened
            .query(&operation_id, armed.boundary_position, &anchor)
            .unwrap(),
        Some(armed)
    );

    let conflict = signed_transition(anchor.authority(), &anchor.head().head(), fixtures::h(125));
    assert!(reopened.arm(&fixtures::h(125), conflict, &anchor).is_err());
    let pending = FormalDurableSetArmControlV1::new(directory.0.clone(), true).unwrap();
    assert!(pending.arm(&operation_id, transition, &anchor).is_err());
}

#[test]
fn arm_rejects_tampered_private_record_and_signature_identity_mismatch() {
    let (_, _, anchor, _) = fixtures::actual_world_initial_fixture();
    let operation_id = fixtures::h(126);
    let transition = signed_transition(
        anchor.authority(),
        &anchor.head().head(),
        operation_id.clone(),
    );
    let directory = PrivateTempDirectory::new();
    let control = FormalDurableSetArmControlV1::new(directory.0.clone(), false).unwrap();
    let status = control
        .arm(&operation_id, transition.clone(), &anchor)
        .unwrap();

    let path = directory.0.join(format!(
        "durable-set-transition-{:020}.cbor",
        status.boundary_position
    ));
    let mut bytes = fs::read(&path).unwrap();
    let last = bytes.len() - 1;
    bytes[last] ^= 1;
    fs::write(&path, &bytes).unwrap();
    assert!(
        control
            .query(&operation_id, status.boundary_position, &anchor)
            .is_err()
    );

    let other = signed_transition(anchor.authority(), &anchor.head().head(), fixtures::h(127));
    assert!(control.arm(&operation_id, other, &anchor).is_err());
}

#[test]
fn decoder_requires_persisted_selectors_to_equal_signed_body() {
    let (_, _, anchor, _) = fixtures::actual_world_initial_fixture();
    let operation_id = fixtures::h(128);
    let intent = build_intent(
        &operation_id,
        signed_transition(
            anchor.authority(),
            &anchor.head().head(),
            operation_id.clone(),
        ),
        &anchor,
    )
    .unwrap();
    let mut persisted = PersistedDurableSetArmV1 {
        schema_version: 1,
        operation_id: intent.operation_id.clone(),
        boundary_position: intent.boundary_position,
        predecessor: intent.predecessor.clone(),
        transition_digest: intent.transition_digest.clone(),
        transition: intent.transition.clone(),
        intent_digest: arm_digest(&intent).unwrap(),
    };

    // A valid local encoding/checksum must not let duplicated selectors drift
    // away from the signed request they index.
    persisted.operation_id = fixtures::h(129);
    let bytes = to_cbor(&persisted).unwrap();
    assert!(decode_intent(&bytes).is_err());
}

#[test]
fn original_arm_reconciliation_survives_pending_and_rejects_other_same_height_cut() {
    let (_, _, anchor, _) = fixtures::actual_world_initial_fixture();
    let operation_id = fixtures::h(130);
    let transition = signed_transition(
        anchor.authority(),
        &anchor.head().head(),
        operation_id.clone(),
    );
    let directory = PrivateTempDirectory::new();
    let control = FormalDurableSetArmControlV1::new(directory.0.clone(), false).unwrap();
    control
        .arm(&operation_id, transition.clone(), &anchor)
        .unwrap();
    control.set_pending_open(true).unwrap();
    assert_eq!(
        control
            .existing_for_operation(&operation_id, &anchor)
            .unwrap(),
        Some(transition)
    );

    let mut other_predecessor = anchor.head().head();
    other_predecessor.record_id = fixtures::h(131);
    let conflicting =
        signed_transition(anchor.authority(), &other_predecessor, operation_id.clone());
    let intent = DurableSetArmIntentV1 {
        operation_id,
        boundary_position: conflicting.body.boundary_position,
        predecessor: conflicting.body.predecessor.clone(),
        transition_digest: conflicting.body.digest().unwrap(),
        transition: conflicting,
    };
    assert_eq!(
        status_for(&intent, &anchor).phase,
        DurableSetArmPhaseV1::Unknown
    );
    assert!(verify_intent_against_anchor(&intent, &anchor).is_err());
}
