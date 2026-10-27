//! Deleting only test-owned private metadata cannot recreate the old signer identity.
use super::*;

#[test]
fn wire3_formal_read_node_private_identity_loss_refuses_the_old_binding() {
    thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(identity_case)
        .unwrap()
        .join()
        .unwrap();
}
fn identity_case() {
    let h = qualified_handoff_v3_with_count_and_setup(1, |_| {});
    let root = Scratch::new();
    let store = cas(root.path());
    let first = driver(&h, root.path(), &store);
    let original_binding = signer_binding(&first);
    let original_digest = original_binding.digest().unwrap();
    first
        .lock()
        .unwrap()
        .configure_bft_v3_execution(&original_binding)
        .unwrap();
    let prefix = root.path().join("ledger/formal-bft-header-prefix-v1");
    let origin_path = prefix.join("origin.cbor");
    let original_origin = fs::read(&origin_path).unwrap();
    let original_h_pins = store
        .list_pin_scope_shards("formal-bft-header-prefix-origin-v1")
        .unwrap();
    assert!(!original_h_pins.is_empty());
    drop(first);

    // Only this Scratch's subordinate Node private directory and scope are
    // removed. The original H ledger, carrier identities and H pins remain.
    fs::remove_dir_all(prefix.join("node-private-v1")).unwrap();
    store
        .clear_pin_scope("formal-bft-header-node-private-v3")
        .unwrap();
    assert_eq!(fs::read(&origin_path).unwrap(), original_origin);
    assert_eq!(
        store
            .list_pin_scope_shards("formal-bft-header-prefix-origin-v1")
            .unwrap(),
        original_h_pins
    );

    let reopened = driver(&h, root.path(), &store);
    let replacement = signer_binding(&reopened);
    replacement.validate().unwrap();
    assert_ne!(
        replacement.material_identity,
        original_binding.material_identity
    );
    assert_ne!(replacement.digest().unwrap(), original_digest);
    assert!(
        reopened
            .lock()
            .unwrap()
            .configure_bft_v3_execution(&original_binding)
            .is_err(),
        "lost private identity must not reattach the old protected binding"
    );
    assert_eq!(fs::read(&origin_path).unwrap(), original_origin);
    assert_eq!(
        store
            .list_pin_scope_shards("formal-bft-header-prefix-origin-v1")
            .unwrap(),
        original_h_pins
    );
    // No provider/service/Node worker is constructed, so refusal occurs before
    // any remote lease acquisition or proposal/vote signing can be requested.
}
