use std::collections::BTreeMap;

use serde::Deserialize;

use super::{ModuleArtifactIdentity, WorldError};

const IDENTITY_HASH_SIGNATURE_SCHEME: &str = "identity_hash_v1";
const IDENTITY_HASH_SIGNATURE_PREFIX: &str = "idhash:";

#[derive(Debug, Deserialize)]
struct BuiltinIdentityManifest {
    #[serde(default, rename = "schema_version")]
    _schema_version: Option<u64>,
    modules: Vec<BuiltinIdentityEntry>,
}

#[derive(Debug, Deserialize)]
struct BuiltinIdentityEntry {
    #[serde(default, rename = "source_input_rule_version")]
    _source_input_rule_version: Option<u64>,
    #[serde(default, rename = "source_inputs")]
    _source_inputs: Option<Vec<BuiltinSourceInputRecord>>,
    module_id: String,
    source_hash: String,
    build_manifest_hash: String,
    identity_hash: String,
    #[serde(default)]
    hash_tokens: Vec<String>,
    #[serde(default)]
    signer_node_id: Option<String>,
    #[serde(default)]
    signature_scheme: Option<String>,
    #[serde(default)]
    artifact_signatures: BTreeMap<String, String>,
}

// Typed raw decoding preserves duplicate-field rejection alongside Value format checks.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct BuiltinSourceInputRecord {
    #[serde(rename = "kind")]
    _kind: String,
    #[serde(rename = "path")]
    _path: String,
    #[serde(rename = "sha256")]
    _sha256: String,
}

// Runtime consumes the published binary identity, not the source workspace.
// This is format validation only; the original identity/signature/hash checks below remain authoritative.
fn validate_identity_source_format(value: &serde_json::Value) -> Result<(), String> {
    let schema = match value.get("schema_version") {
        None => 1, // Historical unversioned inputs retain their original legacy semantics.
        Some(value) => value.as_u64().ok_or("invalid builtin identity schema")?,
    };
    if !matches!(schema, 1 | 2) {
        return Err(format!("unknown builtin identity schema {schema}"));
    }
    let modules = value
        .get("modules")
        .and_then(serde_json::Value::as_array)
        .ok_or("invalid builtin identity modules")?;
    for entry in modules {
        if schema == 1 {
            if entry.get("source_input_rule_version").is_some()
                || entry.get("source_inputs").is_some()
            {
                return Err("legacy identity must not contain new-rule source evidence".into());
            }
            continue;
        }
        if entry
            .get("source_input_rule_version")
            .and_then(serde_json::Value::as_u64)
            != Some(2)
        {
            return Err("unknown or missing builtin source input rule".into());
        }
        let records = entry
            .get("source_inputs")
            .and_then(serde_json::Value::as_array)
            .filter(|values| !values.is_empty())
            .ok_or("missing builtin source input details")?;
        let mut previous: Option<(&str, &str)> = None;
        for record in records {
            let object = record
                .as_object()
                .ok_or("invalid builtin source input record")?;
            if object.len() != 3
                || !["kind", "path", "sha256"]
                    .iter()
                    .all(|key| object.contains_key(*key))
            {
                return Err("unknown builtin source input record fields".into());
            }
            let kind = record["kind"].as_str().ok_or("invalid source input kind")?;
            let path = record["path"].as_str().ok_or("invalid source input path")?;
            let digest = record["sha256"]
                .as_str()
                .ok_or("invalid source input digest")?;
            if !matches!(kind, "source-file" | "input-declaration")
                || path.contains(['\\', '\0'])
                || path.split('/').any(|part| matches!(part, "" | "." | ".."))
                || digest.len() != 64
                || !digest
                    .bytes()
                    .all(|value| value.is_ascii_digit() || (b'a'..=b'f').contains(&value))
            {
                return Err("invalid builtin source input detail".into());
            }
            let key = (path, kind);
            if previous.is_some_and(|previous| previous >= key) {
                return Err("noncanonical builtin source input order or duplicate".into());
            }
            previous = Some(key);
        }
    }
    Ok(())
}

fn parse_hash_token_value(token: &str) -> &str {
    token
        .split_once('=')
        .map(|(_, hash)| hash)
        .unwrap_or(token)
        .trim()
}

pub(crate) fn module_artifact_identity_from_manifest(
    manifest_json: &str,
    manifest_name: &str,
    module_id: &str,
    wasm_hash: &str,
) -> Result<ModuleArtifactIdentity, WorldError> {
    let value: serde_json::Value =
        serde_json::from_str(manifest_json).map_err(|error| WorldError::ModuleChangeInvalid {
            reason: format!(
                "failed to parse builtin identity manifest {}: {}",
                manifest_name, error
            ),
        })?;
    validate_identity_source_format(&value).map_err(|reason| WorldError::ModuleChangeInvalid {
        reason: format!("builtin identity manifest {manifest_name}: {reason}"),
    })?;
    let manifest: BuiltinIdentityManifest =
        serde_json::from_str(manifest_json).map_err(|error| WorldError::ModuleChangeInvalid {
            reason: format!("failed to parse builtin identity manifest {manifest_name}: {error}"),
        })?;

    let Some(entry) = manifest
        .modules
        .into_iter()
        .find(|candidate| candidate.module_id == module_id)
    else {
        return Err(WorldError::ModuleChangeInvalid {
            reason: format!(
                "builtin identity manifest {} missing module_id={}",
                manifest_name, module_id
            ),
        });
    };

    let expected_identity_hash = super::util::sha256_hex(
        format!(
            "{module_id}:{}:{}",
            entry.source_hash, entry.build_manifest_hash
        )
        .as_bytes(),
    );
    if expected_identity_hash != entry.identity_hash {
        return Err(WorldError::ModuleChangeInvalid {
            reason: format!(
                "builtin identity manifest {} has invalid identity_hash module_id={} expected={} actual={}",
                manifest_name, module_id, expected_identity_hash, entry.identity_hash
            ),
        });
    }

    if let Some(artifact_signature) = entry.artifact_signatures.get(wasm_hash).cloned() {
        let signer_node_id = entry
            .signer_node_id
            .clone()
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| WorldError::ModuleChangeInvalid {
                reason: format!(
                    "builtin identity manifest {} missing signer_node_id module_id={}",
                    manifest_name, module_id
                ),
            })?;
        let signature_scheme = entry
            .signature_scheme
            .clone()
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| WorldError::ModuleChangeInvalid {
                reason: format!(
                    "builtin identity manifest {} missing signature_scheme module_id={}",
                    manifest_name, module_id
                ),
            })?;

        return Ok(ModuleArtifactIdentity {
            source_hash: entry.source_hash,
            build_manifest_hash: entry.build_manifest_hash,
            signer_node_id,
            signature_scheme,
            artifact_signature,
        });
    }

    if !entry
        .hash_tokens
        .iter()
        .map(|token| parse_hash_token_value(token.as_str()))
        .any(|hash| hash == wasm_hash)
    {
        return Err(WorldError::ModuleChangeInvalid {
            reason: format!(
                "builtin identity manifest {} missing hash token module_id={} wasm_hash={}",
                manifest_name, module_id, wasm_hash
            ),
        });
    }

    let signer_node_id = entry
        .signer_node_id
        .unwrap_or_else(|| "builtin.module.release.signer".to_string());

    Ok(ModuleArtifactIdentity {
        source_hash: entry.source_hash,
        build_manifest_hash: entry.build_manifest_hash,
        signer_node_id,
        signature_scheme: IDENTITY_HASH_SIGNATURE_SCHEME.to_string(),
        artifact_signature: format!("{IDENTITY_HASH_SIGNATURE_PREFIX}{}", entry.identity_hash),
    })
}

#[cfg(test)]
mod source_input_format_tests {
    use super::*;
    fn legacy() -> serde_json::Value {
        serde_json::from_str(include_str!(
            "world/artifacts/m1_builtin_modules.identity.json"
        ))
        .unwrap()
    }
    fn read(value: &serde_json::Value) -> Result<ModuleArtifactIdentity, WorldError> {
        let entry = &value["modules"][0];
        let module = entry["module_id"].as_str().unwrap();
        let wasm = parse_hash_token_value(entry["hash_tokens"][0].as_str().unwrap());
        module_artifact_identity_from_manifest(&value.to_string(), "fixture", module, wasm)
    }
    fn v2() -> serde_json::Value {
        let mut value = legacy();
        value["schema_version"] = 2.into();
        for entry in value["modules"].as_array_mut().unwrap() {
            entry["source_input_rule_version"] = 2.into();
            entry["source_inputs"] = serde_json::json!([{"kind":"source-file","path":"module/src/lib.rs","sha256":"0".repeat(64)}]);
        }
        value
    }
    #[test]
    fn raw_duplicate_legacy_fields_keep_the_original_typed_rejection() {
        let value = legacy();
        let entry = &value["modules"][0];
        for field in ["modules", "source_hash"] {
            let original = if field == "modules" {
                &value[field]
            } else {
                &entry[field]
            };
            let needle = format!("\"{field}\":{original}");
            let raw = value
                .to_string()
                .replacen(&needle, &format!("{needle},{needle}"), 1);
            assert!(serde_json::from_str::<BuiltinIdentityManifest>(&raw).is_err());
            assert!(
                module_artifact_identity_from_manifest(
                    &raw,
                    "duplicate fixture",
                    entry["module_id"].as_str().unwrap(),
                    parse_hash_token_value(entry["hash_tokens"][0].as_str().unwrap())
                )
                .is_err(),
                "raw duplicate {field} must retain original typed rejection"
            );
        }
    }
    #[test]
    fn raw_duplicate_new_format_fields_fail_closed() {
        let value = v2();
        let entry = &value["modules"][0];
        for (field, original) in [
            ("schema_version", &value["schema_version"]),
            (
                "source_input_rule_version",
                &entry["source_input_rule_version"],
            ),
            ("source_inputs", &entry["source_inputs"]),
            ("kind", &entry["source_inputs"][0]["kind"]),
            ("path", &entry["source_inputs"][0]["path"]),
            ("sha256", &entry["source_inputs"][0]["sha256"]),
        ] {
            let needle = format!("\"{field}\":{original}");
            let raw = value
                .to_string()
                .replacen(&needle, &format!("{needle},{needle}"), 1);
            assert!(
                module_artifact_identity_from_manifest(
                    &raw,
                    "duplicate fixture",
                    entry["module_id"].as_str().unwrap(),
                    parse_hash_token_value(entry["hash_tokens"][0].as_str().unwrap())
                )
                .is_err(),
                "raw duplicate {field} must fail closed"
            );
        }
    }
    #[test]
    fn published_legacy_and_explicit_v2_preserve_the_core_artifact_identity() {
        let old = read(&legacy()).unwrap();
        let new = read(&v2()).unwrap();
        assert_eq!(old.source_hash, new.source_hash);
        assert_eq!(old.build_manifest_hash, new.build_manifest_hash);
        assert_eq!(old.artifact_signature, new.artifact_signature);
        assert_eq!(old.signature_scheme, new.signature_scheme);
    }
    #[test]
    fn every_published_builtin_set_retains_legacy_binary_identity_reads() {
        for text in [
            include_str!("world/artifacts/m1_builtin_modules.identity.json"),
            include_str!("world/artifacts/m4_builtin_modules.identity.json"),
            include_str!("world/artifacts/m5_builtin_modules.identity.json"),
        ] {
            let value: serde_json::Value = serde_json::from_str(text).unwrap();
            for entry in value["modules"].as_array().unwrap() {
                for token in entry["hash_tokens"].as_array().unwrap() {
                    module_artifact_identity_from_manifest(
                        text,
                        "published legacy",
                        entry["module_id"].as_str().unwrap(),
                        parse_hash_token_value(token.as_str().unwrap()),
                    )
                    .unwrap();
                }
            }
        }
    }
    #[test]
    fn explicit_v2_keeps_original_artifact_signature_selection() {
        let mut current = v2();
        let token = current["modules"][0]["hash_tokens"][0].as_str().unwrap();
        let wasm = parse_hash_token_value(token).to_string();
        current["modules"][0]["signer_node_id"] = "fixture-signer".into();
        current["modules"][0]["signature_scheme"] = "fixture-scheme".into();
        current["modules"][0]["artifact_signatures"] =
            serde_json::json!({wasm:"fixture-signature"});
        let new = read(&current).unwrap();
        let mut old = current.clone();
        old["schema_version"] = 1.into();
        for entry in old["modules"].as_array_mut().unwrap() {
            entry
                .as_object_mut()
                .unwrap()
                .remove("source_input_rule_version");
            entry.as_object_mut().unwrap().remove("source_inputs");
        }
        let old = read(&old).unwrap();
        assert_eq!(old.artifact_signature, new.artifact_signature);
        assert_eq!(new.artifact_signature, "fixture-signature");
        assert_eq!(old.signature_scheme, new.signature_scheme);
        assert_eq!(old.signer_node_id, new.signer_node_id);
    }
    #[test]
    fn unknown_identity_schema_rule_and_mixed_fields_fail_closed() {
        let mut unknown = legacy();
        unknown["schema_version"] = 99.into();
        assert!(
            read(&unknown).is_err(),
            "unknown identity schema must not be ignored"
        );
        let mut rule = v2();
        rule["modules"][0]["source_input_rule_version"] = 99.into();
        assert!(read(&rule).is_err());
        let mut missing = v2();
        missing["modules"][0]
            .as_object_mut()
            .unwrap()
            .remove("source_inputs");
        assert!(read(&missing).is_err());
        let mut mixed = legacy();
        mixed["modules"][0]["source_input_rule_version"] = 2.into();
        assert!(read(&mixed).is_err());
        let mut malformed = v2();
        malformed["modules"][0]["source_inputs"][0]["path"] = "../escape".into();
        assert!(read(&malformed).is_err());
        let mut wasm = v2();
        wasm["modules"][0]["hash_tokens"] =
            serde_json::json!(["linux-x86_64=not-the-requested-artifact"]);
        let entry = &wasm["modules"][0];
        assert!(
            module_artifact_identity_from_manifest(
                &wasm.to_string(),
                "fixture",
                entry["module_id"].as_str().unwrap(),
                "different-wasm"
            )
            .is_err()
        );
        let mut identity = v2();
        identity["modules"][0]["identity_hash"] = "0".repeat(64).into();
        assert!(read(&identity).is_err());
    }
}
