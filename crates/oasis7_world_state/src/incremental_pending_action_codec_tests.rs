use super::*;
use crate::incremental_typed_record_test_support::{roundtrip, source_admission};

#[test]
fn pending_original_envelope_preserves_json_bits_and_large_strings() {
    let _source_admission = source_admission();
    let source = ActionEnvelope {
        governance_authorization: None,
        committed_recipe_origin: None,
        id: u64::MAX,
        action: Action::WorldServiceIntent {
            request: serde_json::json!({"negative_zero": -0.0, "wide": u64::MAX,
                "duplicate_values": ["same", "same"], "large": "x".repeat(256 << 10)}),
        },
    };
    roundtrip(&source, profile());
}

#[test]
fn pending_source_package_preserves_ordered_files_and_bytes() {
    let _source_admission = source_admission();
    let source = Action::CompileModuleArtifactFromSource {
        publisher_agent_id: "publisher".into(),
        module_id: "module".into(),
        source_package: ModuleSourcePackage {
            manifest_path: "manifest".into(),
            files: [
                ("empty".into(), vec![]),
                ("bytes".into(), vec![0x5a; 1 << 20]),
            ]
            .into_iter()
            .collect(),
        },
    };
    roundtrip(&source, profile());
}

#[test]
fn pending_blob_uses_bounded_cells_and_cold_recovers_bytes() {
    let _source_admission = source_admission();
    // The fixture caps typed cells at 100,000: a per-byte tree cannot encode this.
    let source = Action::DeployModuleArtifact {
        publisher_agent_id: "publisher".into(),
        wasm_hash: "hash".into(),
        wasm_bytes: vec![0xa5; 1 << 20],
    };
    roundtrip(&source, profile());
}
