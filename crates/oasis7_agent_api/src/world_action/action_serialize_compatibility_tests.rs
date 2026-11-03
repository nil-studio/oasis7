use super::Action;
use super::action_serialize_call_trace_test_support::record_public_serde_calls;
use serde_json::Value;
use sha2::{Digest, Sha256};

const LEGACY_MANIFEST: &[u8] = include_bytes!(
    "test_data/action_serde_legacy_v1/action-legacy-derived-oracle-v1.manifest.json"
);
const ALL_VARIANTS_JSON: &[u8] =
    include_bytes!("test_data/action_serde_legacy_v1/action-all-variants-v1.json");
const ALL_VARIANTS_CBOR: &[u8] =
    include_bytes!("test_data/action_serde_legacy_v1/action-all-variants-v1.cbor");
const ALL_VARIANTS_PACKED_CBOR: &[u8] =
    include_bytes!("test_data/action_serde_legacy_v1/action-all-variants-v1.packed.cbor");
const ALL_VARIANTS_TRACE: &[u8] =
    include_bytes!("test_data/action_serde_legacy_v1/action-all-variants-v1.trace.json");
const OPTIONAL_PRESENT_JSON: &[u8] =
    include_bytes!("test_data/action_serde_legacy_v1/action-optional-present-v1.json");
const OPTIONAL_PRESENT_CBOR: &[u8] =
    include_bytes!("test_data/action_serde_legacy_v1/action-optional-present-v1.cbor");
const OPTIONAL_PRESENT_PACKED_CBOR: &[u8] =
    include_bytes!("test_data/action_serde_legacy_v1/action-optional-present-v1.packed.cbor");
const OPTIONAL_PRESENT_TRACE: &[u8] =
    include_bytes!("test_data/action_serde_legacy_v1/action-optional-present-v1.trace.json");
const LEGACY_ACTION_SOURCE_COMMIT: &str = "163fffef9102121b04f0016652d14b05a15c79ec";
const LEGACY_ACTION_SOURCE_SHA256: &str =
    "6b34890c7ea73ba8ce7d9b5ad4fb0ace017b8532db9572a3ad390b207b70af40";

struct FixtureSet {
    name: &'static str,
    manifest_key: &'static str,
    trace_manifest_key: &'static str,
    json: &'static [u8],
    cbor: &'static [u8],
    packed_cbor: &'static [u8],
    trace: &'static [u8],
    expected_actions: usize,
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn assert_manifest_digest(entry: &Value, expected_file: &str, bytes: &[u8]) {
    assert_eq!(
        entry["file"], expected_file,
        "fixture path in provenance manifest"
    );
    assert_eq!(
        entry["byte_len"].as_u64(),
        Some(bytes.len() as u64),
        "fixture byte length: {expected_file}"
    );
    assert_eq!(
        entry["sha256"].as_str(),
        Some(sha256_hex(bytes).as_str()),
        "fixture SHA-256: {expected_file}"
    );
}

fn assert_fixture_set(manifest: &Value, fixture: FixtureSet) {
    let json_field = "json";
    let cbor_field = "ordinary_cbor";
    let packed_field = "packed_cbor";
    assert_manifest_digest(
        &manifest["fixtures"][fixture.manifest_key][json_field],
        match fixture.manifest_key {
            "all_variants" => "action-all-variants-v1.json",
            _ => "action-optional-present-v1.json",
        },
        fixture.json,
    );
    assert_manifest_digest(
        &manifest["fixtures"][fixture.manifest_key][cbor_field],
        match fixture.manifest_key {
            "all_variants" => "action-all-variants-v1.cbor",
            _ => "action-optional-present-v1.cbor",
        },
        fixture.cbor,
    );
    assert_manifest_digest(
        &manifest["fixtures"][fixture.manifest_key][packed_field],
        match fixture.manifest_key {
            "all_variants" => "action-all-variants-v1.packed.cbor",
            _ => "action-optional-present-v1.packed.cbor",
        },
        fixture.packed_cbor,
    );
    assert_manifest_digest(
        &manifest["fixtures"][fixture.trace_manifest_key],
        match fixture.trace_manifest_key {
            "all_variants_public_serde_trace" => "action-all-variants-v1.trace.json",
            _ => "action-optional-present-v1.trace.json",
        },
        fixture.trace,
    );

    let actions: Vec<Action> = serde_json::from_slice(fixture.json).unwrap_or_else(|error| {
        panic!(
            "{} legacy JSON fixture must deserialize: {error}",
            fixture.name
        )
    });
    assert_eq!(
        actions.len(),
        fixture.expected_actions,
        "{} action count",
        fixture.name
    );

    assert_eq!(
        serde_json::to_vec(&actions).expect("Action JSON serialization succeeds"),
        fixture.json,
        "{} legacy JSON bytes",
        fixture.name
    );
    assert_eq!(
        serde_cbor::to_vec(&actions).expect("Action CBOR serialization succeeds"),
        fixture.cbor,
        "{} legacy ordinary-CBOR bytes",
        fixture.name
    );
    assert_eq!(
        serde_cbor::ser::to_vec_packed(&actions)
            .expect("Action packed-CBOR serialization succeeds"),
        fixture.packed_cbor,
        "{} legacy packed-CBOR bytes",
        fixture.name
    );

    let traces = actions
        .iter()
        .map(record_public_serde_calls)
        .collect::<Result<Vec<_>, _>>()
        .unwrap_or_else(|error| panic!("{} public Serde trace succeeds: {error}", fixture.name));
    let actual_trace =
        serde_json::to_vec_pretty(&traces).expect("trace JSON serialization succeeds");
    assert_eq!(
        actual_trace, fixture.trace,
        "{} legacy public Serde trace",
        fixture.name
    );
}

#[test]
fn action_serialize_remains_compatible_with_legacy_derive_fixtures() {
    let manifest: Value = serde_json::from_slice(LEGACY_MANIFEST)
        .expect("legacy Action fixture provenance manifest is valid JSON");
    assert_eq!(manifest["baseline"], "old_derived_serialize");
    assert_eq!(manifest["source"]["commit"], LEGACY_ACTION_SOURCE_COMMIT);
    assert_eq!(manifest["source"]["sha256"], LEGACY_ACTION_SOURCE_SHA256);
    assert_eq!(manifest["action_count"], 90);

    assert_fixture_set(
        &manifest,
        FixtureSet {
            name: "all 90 Action variants",
            manifest_key: "all_variants",
            trace_manifest_key: "all_variants_public_serde_trace",
            json: ALL_VARIANTS_JSON,
            cbor: ALL_VARIANTS_CBOR,
            packed_cbor: ALL_VARIANTS_PACKED_CBOR,
            trace: ALL_VARIANTS_TRACE,
            expected_actions: 90,
        },
    );
    assert_fixture_set(
        &manifest,
        FixtureSet {
            name: "four optional-present Action cases",
            manifest_key: "optional_present",
            trace_manifest_key: "optional_present_public_serde_trace",
            json: OPTIONAL_PRESENT_JSON,
            cbor: OPTIONAL_PRESENT_CBOR,
            packed_cbor: OPTIONAL_PRESENT_PACKED_CBOR,
            trace: OPTIONAL_PRESENT_TRACE,
            expected_actions: 4,
        },
    );
}
