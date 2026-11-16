use super::*;

fn read(map: &BTreeMap<String, Option<String>>) -> Result<String, &'static str> {
    load(|name| map.get(name).cloned().flatten())
}

#[test]
fn deployment_parts_preserve_exact_bytes_and_clear_obsolete_representation() {
    for raw in [
        "small".to_owned(),
        "私有".repeat(10_000),
        "x".repeat(MAX_BYTES),
    ] {
        let bindings = encode(&raw).unwrap();
        assert_eq!(read(&bindings).unwrap(), raw);
        assert!(bindings.values().flatten().all(|v| v.len() <= PART_BYTES));
        assert_eq!(bindings.len(), MAX_PARTS + 2);
        assert_eq!(bindings[INLINE].is_some(), raw.len() <= PART_BYTES);
        assert_eq!(bindings[MANIFEST].is_some(), raw.len() > PART_BYTES);
    }
    assert!(encode("").is_err());
    assert!(encode(&"x".repeat(MAX_BYTES + 1)).is_err());
}

#[test]
fn incomplete_conflicting_or_mixed_parts_never_fall_back() {
    let raw = "a".repeat(6000);
    let valid = encode(&raw).unwrap();
    let mut missing = valid.clone();
    missing.remove(&part_name(2));
    assert!(read(&missing).is_err());
    for replacement in [String::new(), "b".repeat(1904), "a".repeat(PART_BYTES + 1)] {
        let mut changed = valid.clone();
        changed.insert(part_name(2), Some(replacement));
        assert!(read(&changed).is_err());
    }
    let mut mixed = valid.clone();
    mixed.insert(INLINE.into(), Some("previous policy".into()));
    assert!(read(&mixed).is_err());
    let mut reordered = valid.clone();
    let first = reordered[&part_name(1)].clone();
    reordered.insert(part_name(1), reordered[&part_name(2)].clone());
    reordered.insert(part_name(2), first);
    // Equal repeated characters still form the same bytes; use unequal parts below.
    assert_eq!(read(&reordered).unwrap(), raw);
    let unequal = encode(&("a".repeat(PART_BYTES) + &"b".repeat(1904))).unwrap();
    let mut reordered = unequal.clone();
    reordered.insert(part_name(1), unequal[&part_name(2)].clone());
    reordered.insert(part_name(2), unequal[&part_name(1)].clone());
    assert!(read(&reordered).is_err());
}

#[test]
fn malformed_manifest_and_aggregate_bounds_reject_before_policy_use() {
    let valid = encode(&"x".repeat(6000)).unwrap();
    for (field, value) in [
        ("version", serde_json::json!(2)),
        ("parts", serde_json::json!(0)),
        ("parts", serde_json::json!(17)),
        ("blake3", serde_json::json!("0".repeat(64))),
        ("extra", serde_json::json!(true)),
    ] {
        let mut manifest: serde_json::Value =
            serde_json::from_str(valid[MANIFEST].as_ref().unwrap()).unwrap();
        manifest[field] = value;
        let mut changed = valid.clone();
        changed.insert(MANIFEST.into(), Some(manifest.to_string()));
        assert!(read(&changed).is_err());
    }
    let mut duplicate = valid.clone();
    duplicate.insert(
        MANIFEST.into(),
        Some(
            valid[MANIFEST]
                .as_ref()
                .unwrap()
                .replacen('{', "{\"version\":1,", 1),
        ),
    );
    assert!(read(&duplicate).is_err());
    duplicate.insert(MANIFEST.into(), Some(" ".repeat(257)));
    assert!(read(&duplicate).is_err());
    let mut oversized = encode(&"x".repeat(MAX_BYTES)).unwrap();
    let manifest = Manifest {
        version: 1,
        parts: 16,
        blake3: blake3::hash(&vec![b'x'; 16 * PART_BYTES])
            .to_hex()
            .to_string(),
    };
    oversized.insert(
        MANIFEST.into(),
        Some(serde_json::to_string(&manifest).unwrap()),
    );
    oversized.insert(part_name(16), Some("x".repeat(PART_BYTES)));
    assert!(read(&oversized).is_err());
    assert!(load(|_| None).is_err());
    assert!(load(|name| (name == INLINE).then(|| "x".repeat(5121))).is_err());
}

#[test]
fn public_operator_output_covers_a_realistic_two_account_deployment() {
    let input = include_str!("../../tests/fixtures/policy-large-input.json");
    let expected: BTreeMap<String, Option<String>> = serde_json::from_str(include_str!(
        "../../tests/fixtures/policy-large-bindings.json"
    ))
    .unwrap();
    let generated = crate::prepare_operator_policy_bindings(input).unwrap();
    assert_eq!(generated, expected);
    let raw = read(&generated).unwrap();
    assert!(
        raw.len() > 5120,
        "must exceed Cloudflare's single-Secret limit"
    );
    assert_eq!(raw, crate::prepare_operator_policy(input).unwrap());
    let policy = crate::config::Config::parse(&raw).unwrap();
    assert_eq!(policy.accounts.len(), 2);
    assert_eq!(policy.grants.len(), 2);
}

#[test]
fn policy_fingerprint_is_identical_across_inline_and_chunk_transport() {
    let raw = include_str!("../../tests/fixtures/policy.json");
    let inline = BTreeMap::from([(INLINE.into(), Some(raw.into()))]);
    let split = BTreeMap::from([
        (
            MANIFEST.into(),
            Some(
                serde_json::to_string(&Manifest {
                    version: 1,
                    parts: 2,
                    blake3: blake3::hash(raw.as_bytes()).to_hex().to_string(),
                })
                .unwrap(),
            ),
        ),
        (part_name(1), Some(raw[..100].into())),
        (part_name(2), Some(raw[100..].into())),
    ]);
    assert_eq!(read(&inline).unwrap(), read(&split).unwrap());
    assert_eq!(
        blake3::hash(read(&inline).unwrap().as_bytes()),
        blake3::hash(read(&split).unwrap().as_bytes())
    );
}
