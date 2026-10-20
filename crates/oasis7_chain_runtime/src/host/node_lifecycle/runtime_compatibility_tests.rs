use super::{highest_declared_record_height, io, loaded_executable_identity, read_minimum_cut};
use oasis7_client_api::{
    world_service::WorldIdentity, world_service_full_sync::FormalFullSyncCutV1,
};
use sha2::{Digest, Sha256};
use std::{fs, path::PathBuf, time::SystemTime};

fn records_dir(label: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let path = fs::canonicalize(std::env::temp_dir())
        .unwrap()
        .join(format!(
            "oasis7-runtime-compatibility-{label}-{}-{nonce}",
            std::process::id()
        ));
    fs::create_dir_all(&path).expect("create records fixture");
    path
}

#[test]
fn compatibility_probe_uses_highest_declared_record_even_if_it_is_unreadable() {
    let dir = records_dir("highest-record");
    fs::write(dir.join("00000000000000000001.json"), b"{}").expect("write low record");
    fs::write(dir.join("00000000000000000002.json"), b"not-json")
        .expect("write unreadable high record");
    fs::write(dir.join("latest.json"), b"{}").expect("write latest pointer");

    assert_eq!(highest_declared_record_height(&dir).unwrap(), Some(2));
    fs::remove_dir_all(dir).expect("remove records fixture");
}

#[test]
fn compatibility_probe_rejects_malformed_higher_record_name() {
    let dir = records_dir("malformed-name");
    fs::write(dir.join("00000000000000000001.json"), b"{}").expect("write low record");
    fs::write(dir.join("00000000000000000002x.json"), b"{}").expect("write malformed high record");

    assert!(highest_declared_record_height(&dir).is_err());
    fs::remove_dir_all(dir).expect("remove records fixture");
}

#[test]
fn compatibility_probe_rejects_noncanonical_numbered_record_name() {
    let dir = records_dir("noncanonical-name");
    fs::write(dir.join("2.json"), b"{}").expect("write noncanonical record");

    assert!(highest_declared_record_height(&dir).is_err());
    fs::remove_dir_all(dir).expect("remove records fixture");
}

#[test]
fn compatibility_descriptor_identity_is_bound_to_the_loaded_image() {
    use std::os::unix::fs::MetadataExt;

    let identity = loaded_executable_identity().expect("inspect loaded image");
    let image = crate::executable_image::open_loaded_image().expect("open loaded image");
    let metadata = image.metadata().expect("read loaded image metadata");
    let named = fs::metadata(&identity.path).expect("stat loaded image path");

    assert_eq!(identity.device, metadata.dev());
    assert_eq!(identity.inode, metadata.ino());
    assert_eq!(named.dev(), metadata.dev());
    assert_eq!(named.ino(), metadata.ino());
    assert_eq!(identity.sha256, io::sha256(&identity.path).unwrap());
}

fn minimum_cut(position: u64, seed: u8) -> FormalFullSyncCutV1 {
    FormalFullSyncCutV1 {
        world: WorldIdentity {
            world_id: "world-a".into(),
            genesis_digest: hex::encode([1; 32]),
        },
        chain_id: "chain-a".into(),
        position,
        record_id: hex::encode([seed; 32]),
        after_state_root: hex::encode([2; 32]),
        active_authority_id: hex::encode([3; 32]),
        finalizing_authority_id: hex::encode([4; 32]),
        authority_epoch: 1,
    }
}

#[test]
fn runtime_compatibility_minimum_cut_requires_exact_canonical_bytes() {
    let dir = records_dir("minimum-cut");
    let path = dir.join("minimum-cut.json");
    let cut = minimum_cut(7, 7);
    let canonical = serde_json::to_vec_pretty(&cut).expect("encode canonical cut");
    let digest = hex::encode(Sha256::digest(&canonical));
    fs::write(&path, &canonical).expect("write minimum cut");

    assert_eq!(
        read_minimum_cut(Some(&path), Some(&digest)).expect("read exact cut"),
        (Some(cut.clone()), Some(digest.clone()))
    );

    let substituted = serde_json::to_vec_pretty(&minimum_cut(8, 8)).unwrap();
    fs::write(&path, substituted).expect("replace minimum cut");
    assert!(
        read_minimum_cut(Some(&path), Some(&digest)).is_err(),
        "a changed cut cannot reuse the immutable stage digest"
    );

    let mut noncanonical = canonical;
    noncanonical.push(b'\n');
    let noncanonical_digest = hex::encode(Sha256::digest(&noncanonical));
    fs::write(&path, &noncanonical).expect("write noncanonical cut");
    assert!(
        read_minimum_cut(Some(&path), Some(&noncanonical_digest)).is_err(),
        "the cut must remain in canonical JSON even with its recomputed digest"
    );
    fs::remove_dir_all(dir).expect("remove minimum-cut fixture");
}
