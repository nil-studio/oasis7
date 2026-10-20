use super::*;
use crate::formal_read_fixtures as fixtures;
use oasis7::world_service::{
    formal_read::FormalReadVerifier, formal_read_service::FormalReadPublication,
};
use oasis7_client_api::world_service::{
    FormalReadCursorFrameV1, ReadWorldViewRequest, WorldIdentity,
};

#[test]
fn cursor_postrename_sync_failure_reports_replacement_and_unknown_durability() {
    let (world, trust, anchor, _) = fixtures::actual_world_initial_fixture();
    let verifier = FormalReadVerifier::new(&trust, anchor.clone()).unwrap();
    let request = ReadWorldViewRequest {
        observation_nonce: Some("07".repeat(32)),
        formal_anchor: Some(verifier.anchor_ref().unwrap()),
        contract_version: 1,
        world: WorldIdentity {
            world_id: anchor.head().record().world.world_id.clone(),
            genesis_digest: anchor.head().record().world.genesis_digest.clone(),
        },
        scope_id: "public".into(),
        min_commit: None,
        fixed_commit: None,
        deadline_unix_ms: None,
    };
    let reply = FormalReadPublication::new(anchor.clone(), world)
        .unwrap()
        .view_at_own_anchor(&request, &fixtures::public(7))
        .unwrap();
    let bundle = Bundle {
        schema_version: 1,
        external_checkpoint_digest: anchor.checkpoint().digest().unwrap(),
        commit: reply.version.commit.clone(),
        frames: vec![FormalReadCursorFrameV1 {
            request,
            response: reply,
        }],
    };
    bundle.validate_structure().unwrap();
    let bytes = serde_json::to_vec(&bundle).unwrap();
    let mut random = [0; 16];
    getrandom::fill(&mut random).unwrap();
    let directory = std::env::temp_dir().join(format!("cursor-io-{}", hex::encode(random)));
    fs::create_dir(&directory).unwrap();
    let directory = fs::canonicalize(directory).unwrap();
    let target = directory.join("cursor.json");
    fs::write(&target, b"old cursor sentinel").unwrap();
    // An exclusive-create failure precedes rename and preserves the old file.
    fs::write(directory.join("occupied"), b"occupied").unwrap();
    assert!(anchored_write(&directory, target.file_name().unwrap(), "occupied", &bytes).is_err());
    assert_eq!(fs::read(&target).unwrap(), b"old cursor sentinel");
    assert_eq!(fs::read(directory.join("occupied")).unwrap(), b"occupied");
    let error = anchored_write_with_parent_sync(
        &directory,
        target.file_name().unwrap(),
        "new-cursor",
        &bytes,
        |_| Err(std::io::Error::other("injected directory sync failure")),
    )
    .unwrap_err();
    assert!(error.contains("replacement executed; durability unknown"));
    assert!(error.contains(&format!(
        "new_file_blake3={}",
        blake3::hash(&bytes).to_hex()
    )));
    assert_eq!(fs::read(&target).unwrap(), bytes);
    let installed = read(&target).unwrap();
    FormalReadVerifier::restore_cursor(
        &trust,
        anchor,
        &bundle.external_checkpoint_digest,
        &installed,
        &fixtures::public(7),
    )
    .unwrap();
    fs::remove_dir_all(directory).unwrap();
}
