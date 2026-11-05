use super::*;

use std::os::unix::fs::PermissionsExt;
struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let suffix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().canonicalize().unwrap().join(format!(
            "oasis7-node-private-{}-{suffix}",
            std::process::id()
        ));
        fs::create_dir(&path).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        Self(path)
    }
    fn path(&self) -> &Path {
        &self.0
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn data() -> Data {
    Data {
        schema_version: 3,
        domain: DOMAIN.into(),
        origin: true,
        ledger_origin_digest: "a".repeat(64),
        material_identity: "b".repeat(64),
        instance_id: "c".repeat(64),
        height: 3,
        value: None,
        actions_cbor: Vec::new(),
        references: Vec::new(),
        publication_reservation_ref: None,
        publication_original_data_ref: None,
        source_activation: None,
        protected_source_origin_ref: None,
    }
}

#[test]
fn formal_read_node_private_intent_and_link_cannot_republish_lost_pins() {
    for linked in [false, true] {
        let root = Scratch::new();
        let store = LocalCasStore::new(root.path().join("cas"));
        let path = root.path().join("origin.cbor");
        let intent = path.with_extension("intent.cbor");
        let original = data();
        let bytes = original.bytes().unwrap();
        files::publish(&intent, &bytes).unwrap();
        if linked {
            files::link_record(&path, &intent, &bytes).unwrap();
        }
        common_storage::put_cas_durable(&store, &bytes, MAX_RECORD).unwrap();
        store
            .replace_pin_scope_shard(SCOPE, &original.shard(), &original.retained().unwrap())
            .unwrap();
        store
            .remove_pin_scope_shard(SCOPE, &original.shard())
            .unwrap();
        assert!(publish(&store, &path, &original, &BTreeMap::new(), false, true).is_err());
        assert_eq!(fs::read(&intent).unwrap(), bytes);
        assert_eq!(path.exists(), linked);
        assert!(!path.parent().unwrap().join("ready").exists());
        assert!(store.list_pin_scope_shards(SCOPE).unwrap().is_empty());
    }
}

#[test]
fn formal_read_node_private_live_metadata_loss_is_rejected() {
    let root = Scratch::new();
    let store = LocalCasStore::new(root.path().join("cas"));
    let path = root.path().join("origin.cbor");
    let original = data();
    publish(&store, &path, &original, &BTreeMap::new(), false, true).unwrap();
    let slot = read_slot(&path).unwrap().unwrap();
    assert!(slot.complete);
    verify_original_slot(&store, &path, &slot).unwrap();
    fs::remove_file(&path).unwrap();
    assert!(verify_original_slot(&store, &path, &slot).is_err());
    assert!(!path.exists());
    verify_material(&store, &original).unwrap();
}

#[test]
fn formal_read_private_covered_origin_is_old_codec_fail_closed_and_none_bytes_unchanged() {
    #[derive(serde::Serialize, serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct LegacyData {
        schema_version: u32,
        domain: String,
        origin: bool,
        ledger_origin_digest: String,
        material_identity: String,
        instance_id: String,
        height: u64,
        value: Option<BftHeaderValueV3>,
        actions_cbor: Vec<u8>,
        references: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        publication_reservation_ref: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        publication_original_data_ref: Option<String>,
    }
    let original = data().bytes().unwrap();
    let legacy: LegacyData = serde_cbor::from_slice(&original).unwrap();
    assert_eq!(
        serde_cbor::to_vec(&legacy).unwrap(),
        original,
        "optional None preserves original schema3 bytes exactly"
    );
    let mut covered = serde_json::to_value(data()).unwrap();
    covered["schema_version"] = serde_json::json!(8);
    covered["domain"] = serde_json::json!(COVERED_SOURCE_ORIGIN_DOMAIN);
    covered["source_activation"] = serde_json::json!({"reference":"de".repeat(32),"encoded_bytes":123,"directory":{"device":1,"inode":1}});
    let covered: Data = serde_json::from_value(covered).unwrap();
    let bytes = covered.bytes().unwrap();
    assert!(
        serde_cbor::from_slice::<LegacyData>(&bytes).is_err(),
        "old decoder must reject the mandatory covered origin field"
    );
    let mut stripped = covered;
    stripped.source_activation = None;
    assert!(
        stripped.bytes().is_err(),
        "schema8 cannot downgrade by omitting activation"
    );
}

#[test]
fn completed_private_record_rejects_recreated_reservation_even_with_exact_bytes() {
    let root = Scratch::new();
    let path = root.path().join("origin.cbor");
    let bytes = data().bytes().unwrap();
    files::publish(&path, &bytes).unwrap();
    assert!(read_slot(&path).unwrap().unwrap().complete);
    let reservation = path.with_extension("reservation.cbor");
    files::publish(&reservation, &bytes).unwrap();
    assert_eq!(
        read_slot(&path).err().as_deref(),
        Some("completed Node record has foreign pending material"),
    );
    // The guard must preserve both originals for explicit investigation.
    assert_eq!(fs::read(&path).unwrap(), bytes);
    assert_eq!(fs::read(&reservation).unwrap(), bytes);
}
