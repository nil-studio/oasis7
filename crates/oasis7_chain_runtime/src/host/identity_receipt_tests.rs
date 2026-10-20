use std::fs;

use super::identity_receipt::{IdentityReceipt, build_identity_receipt};
use ed25519_dalek::SigningKey;

#[cfg(unix)]
#[test]
fn identity_receipt_reads_existing_key_without_mutating_or_leaking_bytes() {
    let root = tempfile_dir("identity-receipt");
    let key_path = root.join("node-keypair.toml");
    let signing_key = SigningKey::from_bytes(&[1_u8; 32]);
    let key_bytes = format!(
        "[node]\nprivate_key = \"{}\"\npublic_key = \"{}\"\n",
        hex::encode(signing_key.to_bytes()),
        hex::encode(signing_key.verifying_key().to_bytes())
    );
    fs::write(&key_path, key_bytes.as_bytes()).expect("write fixture");
    fs::set_permissions(&root, std::os::unix::fs::PermissionsExt::from_mode(0o700))
        .expect("secure root");
    fs::set_permissions(
        &key_path,
        std::os::unix::fs::PermissionsExt::from_mode(0o600),
    )
    .expect("secure fixture");
    let before = fs::read(&key_path).expect("read before");

    let result = build_identity_receipt(root.as_path(), "validator-a").expect("receipt");
    assert_eq!(result.schema_version, "oasis7.identity_receipt.v1");
    assert!(!result.peer_id.is_empty());
    assert_eq!(result.key_size_bytes, before.len() as u64);
    assert_eq!(result.key_mode, 0o600);
    assert_eq!(fs::read(&key_path).expect("read after"), before);
    let receipt_json = serde_json::to_string(&result).expect("receipt json");
    assert!(!receipt_json.contains(&hex::encode([1_u8; 32])));
    let _type_check: Option<IdentityReceipt> = Some(result);
    fs::remove_dir_all(root).expect("cleanup fixture");
}

fn tempfile_dir(label: &str) -> std::path::PathBuf {
    let path = std::env::current_dir()
        .expect("current worktree")
        .join(".tmp")
        .join(format!(
            "oasis7-{label}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
    fs::create_dir_all(&path).expect("create fixture");
    path
}
