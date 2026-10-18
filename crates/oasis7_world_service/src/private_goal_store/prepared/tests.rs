use super::*;
use crate::{private_goal_prepare::sign_goal_prepare, sign_read_request};
use oasis7_agent_api::game::{DecimalU64, GoalContentV1, game_goal_version_digest};
use std::{
    fs,
    os::unix::fs::{PermissionsExt, symlink},
};

struct Fixture(std::path::PathBuf);
impl Fixture {
    fn new() -> Self {
        let mut id = [0; 16];
        getrandom::fill(&mut id).unwrap();
        let root = fs::canonicalize(std::env::temp_dir())
            .unwrap()
            .join(format!("oasis7-prepared-goal-{}", hex::encode(id)));
        fs::create_dir(&root).unwrap();
        Self(root)
    }
    fn path(&self) -> std::path::PathBuf {
        self.0.join("private")
    }
    fn open(&self) -> PrivateGoalStore {
        PrivateGoalStore::open(&self.path(), [7; 32]).unwrap()
    }
    fn record(&self) -> std::path::PathBuf {
        fs::read_dir(self.path())
            .unwrap()
            .map(|e| e.unwrap().path())
            .find(|p| {
                p.file_name()
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .starts_with("prepared-")
            })
            .unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn proof() -> SignedGoalPrepareV1 {
    let private = hex::encode([9; 32]);
    let owner = sign_read_request("fixture", (), &private)
        .unwrap()
        .subject_public_key;
    let content = GoalContentV1 {
        goal_text: "private complete 正文 encrypted".into(),
        soft_constraints: vec!["first".into(), "second".into()],
    };
    sign_goal_prepare(
        GameGoalPrepareV1 {
            api_rev: 1,
            world_id: "world".into(),
            genesis_digest: "genesis".into(),
            task_id: "task".into(),
            agent_id: "agent".into(),
            owner_public_key: owner,
            owner_binding: "owner".into(),
            owner_generation: DecimalU64(1),
            branch_id: "main".into(),
            reorg_epoch: DecimalU64(0),
            expected_task_revision: DecimalU64(1),
            expected_latest_goal_revision: DecimalU64(0),
            goal_revision: DecimalU64(1),
            client_operation_id: "prepare-original".into(),
            client_nonce: "ab".repeat(32),
            goal_digest: game_goal_version_digest("task", DecimalU64(1), &content).unwrap(),
            content,
        },
        &private,
    )
    .unwrap()
}
fn publication(receipt: &PreparedGoalReceiptV1) -> GameGoalPublishV1 {
    let r = &receipt.owner_proof.request;
    GameGoalPublishV1 {
        api_rev: 1,
        world_id: r.world_id.clone(),
        genesis_digest: r.genesis_digest.clone(),
        task_id: r.task_id.clone(),
        agent_id: r.agent_id.clone(),
        owner_public_key: r.owner_public_key.clone(),
        owner_binding: r.owner_binding.clone(),
        owner_generation: r.owner_generation,
        branch_id: r.branch_id.clone(),
        reorg_epoch: r.reorg_epoch,
        expected_task_revision: r.expected_task_revision,
        expected_latest_goal_revision: r.expected_latest_goal_revision,
        goal_revision: r.goal_revision,
        client_operation_id: "publish-original".into(),
        prepare_operation_id: r.client_operation_id.clone(),
        goal_ref: receipt.goal_ref.clone(),
    }
}
#[test]
fn prepared_goal_single_durable_record_replay_restart_and_conflict() {
    let f = Fixture::new();
    let p = proof();
    let store = f.open();
    let first = store.prepare(&p).unwrap();
    first.validate_against(&p).unwrap();
    let disk = fs::read(f.record()).unwrap();
    let retry = store.prepare(&p).unwrap();
    assert!(first == retry);
    assert_eq!(fs::read(f.record()).unwrap(), disk);
    assert_eq!(fs::read_dir(f.path()).unwrap().count(), 2); // key marker + sole authority
    for secret in [
        &p.request.content.goal_text,
        &p.signature_hex,
        &p.request.client_operation_id,
        &p.request.goal_digest.0,
        &first.reference_salt,
    ] {
        assert!(
            !disk
                .windows(secret.len())
                .any(|bytes| bytes == secret.as_bytes())
        );
    }
    drop(store);
    let store = f.open();
    assert!(store.get_prepared(&publication(&first)).unwrap() == first);
    let mut changed = p.request.clone();
    changed.content.goal_text.push('x');
    changed.goal_digest =
        game_goal_version_digest(&changed.task_id, changed.goal_revision, &changed.content)
            .unwrap();
    let changed = sign_goal_prepare(changed, &hex::encode([9; 32])).unwrap();
    assert_eq!(
        store.prepare(&changed).err(),
        Some(PreparedGoalError::Conflict)
    );
    let mut changed = p.request.clone();
    changed.expected_task_revision.0 += 1;
    changed.expected_latest_goal_revision.0 += 1;
    changed.goal_revision.0 += 1;
    changed.goal_digest =
        game_goal_version_digest(&changed.task_id, changed.goal_revision, &changed.content)
            .unwrap();
    let changed = sign_goal_prepare(changed, &hex::encode([9; 32])).unwrap();
    assert_eq!(
        store.prepare(&changed).err(),
        Some(PreparedGoalError::Conflict)
    );
    assert_eq!(fs::read(f.record()).unwrap(), disk);
}
#[test]
fn prepared_goal_exact_body_commitment_and_publication_fences() {
    let f = Fixture::new();
    let store = f.open();
    let p = proof();
    let record = store.prepare(&p).unwrap();
    let publish = publication(&record);
    for field in 0..14 {
        let mut invalid = publish.clone();
        match field {
            0 => invalid.world_id.push('x'),
            1 => invalid.genesis_digest.push('x'),
            2 => invalid.task_id.push('x'),
            3 => invalid.agent_id.push('x'),
            4 => invalid.owner_public_key = "cd".repeat(32),
            5 => invalid.owner_binding.push('x'),
            6 => invalid.owner_generation.0 += 1,
            7 => invalid.branch_id.push('x'),
            8 => invalid.reorg_epoch.0 += 1,
            9 => invalid.expected_task_revision.0 += 1,
            10 => {
                invalid.expected_latest_goal_revision.0 += 1;
                invalid.goal_revision.0 += 1;
            }
            11 => invalid.prepare_operation_id.push('x'),
            12 => invalid.goal_ref = "cd".repeat(32),
            _ => invalid.goal_revision.0 += 1,
        }
        assert!(store.get_prepared(&invalid).is_err());
    }
    // A malicious service holding another genuine owner-signed proposal cannot
    // replace the body behind the reference signed in the public publication.
    let mut altered = record.clone();
    let mut r = p.request.clone();
    r.content.goal_text.push('x');
    r.goal_digest = game_goal_version_digest(&r.task_id, r.goal_revision, &r.content).unwrap();
    altered.owner_proof = sign_goal_prepare(r, &hex::encode([9; 32])).unwrap();
    assert!(altered.validate().is_err());
    altered.goal_ref = commitment(
        &hex::decode(&altered.reference_salt)
            .unwrap()
            .try_into()
            .unwrap(),
        &altered.owner_proof,
    )
    .unwrap();
    altered.validate().unwrap();
    assert!(altered.validate_publish(&publish).is_err());
    assert!(altered.validate_against(&p).is_err());
    let other_fixture = Fixture::new();
    let other = other_fixture.open().prepare(&p).unwrap();
    assert_ne!(record.goal_ref, other.goal_ref);
}
#[test]
fn prepared_goal_concurrent_same_operation_has_one_winner() {
    let f = Fixture::new();
    let store = std::sync::Arc::new(f.open());
    let p = proof();
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(4));
    let handles: Vec<_> = (0..4)
        .map(|_| {
            let s = store.clone();
            let p = p.clone();
            let b = barrier.clone();
            std::thread::spawn(move || {
                b.wait();
                s.prepare(&p)
            })
        })
        .collect();
    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    // A reader can see the short-lived hard-link publication before unlink;
    // such a read fails closed and is retried with the original operation.
    let winner = store.prepare(&p).unwrap();
    for result in results {
        match result {
            Ok(record) => assert!(record == winner),
            Err(error) => assert_eq!(error, PreparedGoalError::Unavailable),
        }
    }
    assert_eq!(fs::read_dir(f.path()).unwrap().count(), 2);
}
#[test]
fn prepared_goal_missing_corrupt_relocated_or_linked_records_fail_closed() {
    let f = Fixture::new();
    let store = f.open();
    let p = proof();
    let record = store.prepare(&p).unwrap();
    let publish = publication(&record);
    let path = f.record();
    let original = fs::read(&path).unwrap();
    let mut damaged = original.clone();
    let end = damaged.len() - 1;
    damaged[end] ^= 1;
    fs::write(&path, &damaged).unwrap();
    assert_eq!(
        store.get_prepared(&publish).err(),
        Some(PreparedGoalError::Unavailable)
    );
    fs::write(&path, &original).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
    assert!(store.get_prepared(&publish).is_err());
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    let backup = path.with_extension("backup");
    fs::rename(&path, &backup).unwrap();
    assert_eq!(
        store.get_prepared(&publish).err(),
        Some(PreparedGoalError::Unavailable)
    );
    symlink(&backup, &path).unwrap();
    assert!(store.get_prepared(&publish).is_err());
    fs::remove_file(&path).unwrap();
    fs::hard_link(&backup, &path).unwrap();
    assert!(store.get_prepared(&publish).is_err());
    fs::remove_file(&backup).unwrap();
    assert!(store.get_prepared(&publish).is_ok());
    fs::remove_file(&path).unwrap();
    assert!(store.lookup_prepared(&p).unwrap().is_none());
}

#[test]
fn prepared_goal_owner_client_checks_original_proof_and_service_trust() {
    use crate::{private_goal_read::*, request_digest, sign_service_response};
    let fixture = Fixture::new();
    let p = proof();
    let receipt = fixture.open().prepare(&p).unwrap();
    let secret = hex::encode([11; 32]);
    let trust = sign_read_request("fixture", (), &secret)
        .unwrap()
        .subject_public_key;
    let digest = request_digest(GOAL_PREPARE_PATH, &p).unwrap();
    let signed =
        sign_service_response(GOAL_PREPARE_PATH, digest, receipt.clone(), &secret).unwrap();
    verify_prepare_response(&p, &signed, &trust).unwrap();
    let public = publication(&p, &receipt, "public-operation".into()).unwrap();
    let json = serde_json::to_string(&public).unwrap();
    for private in [
        &p.request.content.goal_text,
        &receipt.reference_salt,
        &p.signature_hex,
        &p.request.goal_digest.0,
    ] {
        assert!(!json.contains(private));
    }
    let mut modified = signed.clone();
    modified.payload.reference_salt = "cd".repeat(32);
    assert!(verify_prepare_response(&p, &modified, &trust).is_err());
    assert!(verify_prepare_response(&p, &signed, &"ab".repeat(32)).is_err());
    let mut changed = p.request.clone();
    changed.client_operation_id.push('x');
    let changed = sign_goal_prepare(changed, &hex::encode([9; 32])).unwrap();
    assert!(verify_prepare_response(&changed, &signed, &trust).is_err());
    assert!(publication(&changed, &receipt, "public-operation".into()).is_err());
    let identity = "01".to_owned() + &"00".repeat(31);
    let forged = crate::SignedServiceResponse {
        request_digest: signed.request_digest,
        payload: receipt,
        signature_hex: identity.clone() + &"00".repeat(32),
    };
    assert!(verify_prepare_response(&p, &forged, &identity).is_err());
}
