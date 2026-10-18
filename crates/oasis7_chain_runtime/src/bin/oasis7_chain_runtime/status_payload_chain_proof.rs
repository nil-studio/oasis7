use std::fs;
use std::path::Path;

use oasis7_proto::distributed::WorldHeadProofV1;
use oasis7_runtime::LocalCasStore;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub(crate) struct ChainProofStatus {
    pub(crate) schema_version: String,
    pub(crate) proof_contract: String,
    pub(crate) claim_boundary: String,
    pub(crate) status: String,
    pub(crate) latest_world_head_proof: Option<LatestWorldHeadProofStatus>,
    pub(crate) latest_execution_checkpoint: Option<LatestExecutionCheckpointStatus>,
    pub(crate) source_record_path: Option<String>,
    pub(crate) load_error: Option<String>,
    pub(crate) does_not_claim: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct LatestExecutionCheckpointStatus {
    pub(crate) schema_version: u32,
    pub(crate) checkpoint_id: String,
    pub(crate) height: u64,
    pub(crate) manifest_hash: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct LatestWorldHeadProofStatus {
    pub(crate) schema_version: u16,
    pub(crate) world_id: String,
    pub(crate) height: u64,
    pub(crate) execution_block_hash: String,
    pub(crate) execution_state_root: String,
    pub(crate) node_block_hash: String,
    pub(crate) action_root: String,
    pub(crate) world_head_proof_ref: String,
    pub(crate) proof_hash: String,
    pub(crate) checkpoint_ref: Option<String>,
}

pub(crate) fn build_chain_proof_status(
    execution_records_dir: Option<&Path>,
    execution_storage_root: Option<&Path>,
) -> ChainProofStatus {
    let schema_version = "oasis7.chain_proof_status.v1".to_string();
    let proof_contract = "WorldHeadProofV1".to_string();
    let claim_boundary =
        "head_execution_checkpoint_evidence_only_not_light_client_or_mainnet_readiness".to_string();
    let does_not_claim = vec![
        "public_testnet ready".to_string(),
        "ready_for_live_candidate".to_string(),
        "mainnet-grade".to_string(),
        "light-client verified".to_string(),
        "state proof verified".to_string(),
        "receipt proof verified".to_string(),
        "DA sampling verified".to_string(),
    ];
    let latest_execution_checkpoint = execution_records_dir
        .and_then(|records_dir| {
            super::super::execution_bridge::load_latest_execution_checkpoint_status_evidence(
                records_dir,
            )
            .ok()
            .flatten()
        })
        .map(|evidence| LatestExecutionCheckpointStatus {
            schema_version: evidence.schema_version,
            checkpoint_id: evidence.checkpoint_id,
            height: evidence.height,
            manifest_hash: evidence.manifest_hash,
        });
    let Some(execution_records_dir) = execution_records_dir else {
        return ChainProofStatus {
            schema_version,
            proof_contract,
            claim_boundary,
            status: "unavailable".to_string(),
            latest_world_head_proof: None,
            latest_execution_checkpoint,
            source_record_path: None,
            load_error: Some("execution_records_dir_unconfigured".to_string()),
            does_not_claim,
        };
    };
    let latest_path = execution_records_dir.join("latest.json");
    if !latest_path.exists() {
        return ChainProofStatus {
            schema_version,
            proof_contract,
            claim_boundary,
            status: "unavailable".to_string(),
            latest_world_head_proof: None,
            latest_execution_checkpoint,
            source_record_path: Some(latest_path.display().to_string()),
            load_error: Some("execution_bridge_latest_record_missing".to_string()),
            does_not_claim,
        };
    }
    let bytes = match fs::read(latest_path.as_path()) {
        Ok(bytes) => bytes,
        Err(err) => {
            return ChainProofStatus {
                schema_version,
                proof_contract,
                claim_boundary,
                status: "stale_or_invalid".to_string(),
                latest_world_head_proof: None,
                latest_execution_checkpoint,
                source_record_path: Some(latest_path.display().to_string()),
                load_error: Some(format!("read latest execution record failed: {err}")),
                does_not_claim,
            };
        }
    };
    let latest: serde_json::Value = match serde_json::from_slice(bytes.as_slice()) {
        Ok(latest) => latest,
        Err(err) => {
            return ChainProofStatus {
                schema_version,
                proof_contract,
                claim_boundary,
                status: "stale_or_invalid".to_string(),
                latest_world_head_proof: None,
                latest_execution_checkpoint,
                source_record_path: Some(latest_path.display().to_string()),
                load_error: Some(format!("parse latest execution record failed: {err}")),
                does_not_claim,
            };
        }
    };
    let record_schema_version = latest
        .get("schema_version")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    if record_schema_version < 3 {
        return ChainProofStatus {
            schema_version: "oasis7.chain_proof_status.v1".to_string(),
            proof_contract,
            claim_boundary,
            status: "stale_or_invalid".to_string(),
            latest_world_head_proof: None,
            latest_execution_checkpoint,
            source_record_path: Some(latest_path.display().to_string()),
            load_error: Some(format!(
                "execution_bridge_record_schema_v{record_schema_version}_has_no_world_head_proof"
            )),
            does_not_claim,
        };
    }

    let string_field = |name: &str| -> Result<String, String> {
        latest
            .get(name)
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .ok_or_else(|| format!("{name}_missing"))
    };
    let height = latest
        .get("height")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    if height == 0 {
        return ChainProofStatus {
            schema_version: "oasis7.chain_proof_status.v1".to_string(),
            proof_contract,
            claim_boundary,
            status: "stale_or_invalid".to_string(),
            latest_world_head_proof: None,
            latest_execution_checkpoint,
            source_record_path: Some(latest_path.display().to_string()),
            load_error: Some("height_missing".to_string()),
            does_not_claim,
        };
    }

    let latest_world_head_proof = match (
        string_field("world_id"),
        string_field("execution_block_hash"),
        string_field("execution_state_root"),
        string_field("node_block_hash"),
        string_field("action_root"),
        string_field("world_head_proof_ref"),
        string_field("world_head_proof_hash"),
    ) {
        (
            Ok(world_id),
            Ok(execution_block_hash),
            Ok(execution_state_root),
            Ok(node_block_hash),
            Ok(action_root),
            Ok(world_head_proof_ref),
            Ok(proof_hash),
        ) => LatestWorldHeadProofStatus {
            schema_version: 1,
            world_id,
            height,
            execution_block_hash,
            execution_state_root,
            node_block_hash,
            action_root,
            world_head_proof_ref,
            proof_hash,
            checkpoint_ref: latest
                .get("checkpoint_ref")
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned),
        },
        _ => {
            let missing_fields = [
                "world_id",
                "execution_block_hash",
                "execution_state_root",
                "node_block_hash",
                "action_root",
                "world_head_proof_ref",
                "world_head_proof_hash",
            ]
            .into_iter()
            .filter(|field| string_field(field).is_err())
            .collect::<Vec<_>>();
            return ChainProofStatus {
                schema_version: "oasis7.chain_proof_status.v1".to_string(),
                proof_contract,
                claim_boundary,
                status: "stale_or_invalid".to_string(),
                latest_world_head_proof: None,
                latest_execution_checkpoint,
                source_record_path: Some(latest_path.display().to_string()),
                load_error: Some(format!(
                    "latest execution record missing proof metadata: {}",
                    missing_fields.join(",")
                )),
                does_not_claim,
            };
        }
    };

    if let Err(err) = validate_latest_world_head_proof(
        execution_storage_root,
        latest_world_head_proof.world_head_proof_ref.as_str(),
        latest_world_head_proof.proof_hash.as_str(),
        latest_world_head_proof.world_id.as_str(),
        latest_world_head_proof.height,
        latest_world_head_proof.execution_block_hash.as_str(),
        latest_world_head_proof.execution_state_root.as_str(),
        latest_world_head_proof.node_block_hash.as_str(),
        latest_world_head_proof.action_root.as_str(),
        latest_world_head_proof.checkpoint_ref.as_deref(),
    ) {
        return ChainProofStatus {
            schema_version,
            proof_contract,
            claim_boundary,
            status: "stale_or_invalid".to_string(),
            latest_world_head_proof: None,
            latest_execution_checkpoint,
            source_record_path: Some(latest_path.display().to_string()),
            load_error: Some(err),
            does_not_claim,
        };
    }

    ChainProofStatus {
        schema_version,
        proof_contract,
        claim_boundary,
        status: "available".to_string(),
        latest_world_head_proof: Some(latest_world_head_proof),
        latest_execution_checkpoint,
        source_record_path: Some(latest_path.display().to_string()),
        load_error: None,
        does_not_claim,
    }
}

#[allow(clippy::too_many_arguments)]
fn validate_latest_world_head_proof(
    execution_storage_root: Option<&Path>,
    proof_ref: &str,
    expected_proof_hash: &str,
    world_id: &str,
    height: u64,
    execution_block_hash: &str,
    execution_state_root: &str,
    node_block_hash: &str,
    action_root: &str,
    checkpoint_ref: Option<&str>,
) -> Result<(), String> {
    let storage_root = execution_storage_root
        .ok_or_else(|| "execution storage root is unconfigured".to_string())?;
    let proof_bytes = LocalCasStore::new(storage_root)
        .get_verified(proof_ref)
        .map_err(|err| format!("load world head proof CAS blob failed: {err:?}"))?;
    let proof: WorldHeadProofV1 = serde_cbor::from_slice(proof_bytes.as_slice())
        .map_err(|err| format!("decode WorldHeadProofV1 failed: {err}"))?;
    proof
        .validate_contract()
        .map_err(|err| format!("validate WorldHeadProofV1 failed: {err}"))?;
    let proof_hash = proof
        .proof_hash()
        .map_err(|err| format!("hash WorldHeadProofV1 failed: {err}"))?;
    if proof_hash != expected_proof_hash {
        return Err(format!(
            "world head proof hash mismatch: record={expected_proof_hash} proof={proof_hash}"
        ));
    }
    if proof.world_id != world_id {
        return Err(format!(
            "world head proof world_id mismatch: record={world_id} proof={}",
            proof.world_id
        ));
    }
    if proof.height != height {
        return Err(format!(
            "world head proof height mismatch: record={height} proof={}",
            proof.height
        ));
    }
    if proof.execution.execution_block_hash != execution_block_hash {
        return Err(format!(
            "world head proof execution block hash mismatch: record={execution_block_hash} proof={}",
            proof.execution.execution_block_hash
        ));
    }
    if proof.execution.execution_state_root != execution_state_root {
        return Err(format!(
            "world head proof execution state root mismatch: record={execution_state_root} proof={}",
            proof.execution.execution_state_root
        ));
    }
    if proof.execution.node_block_hash != node_block_hash {
        return Err(format!(
            "world head proof node block hash mismatch: record={node_block_hash} proof={}",
            proof.execution.node_block_hash
        ));
    }
    if proof.execution.action_root != action_root {
        return Err(format!(
            "world head proof action root mismatch: record={action_root} proof={}",
            proof.execution.action_root
        ));
    }
    let proof_checkpoint_ref = proof
        .checkpoint
        .as_ref()
        .map(|checkpoint| checkpoint.manifest_ref.as_str());
    if proof_checkpoint_ref != checkpoint_ref {
        return Err(format!(
            "world head proof checkpoint ref mismatch: record={checkpoint_ref:?} proof={proof_checkpoint_ref:?}"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod checkpoint_status_tests {
    use std::fs;
    use std::path::Path;
    use std::time::{SystemTime, UNIX_EPOCH};

    use serde::Serialize;

    use super::build_chain_proof_status;

    #[derive(Serialize)]
    struct ManifestHashPayload<'a> {
        schema_version: u32,
        checkpoint_id: &'a str,
        world_id: &'a str,
        height: u64,
        execution_block_hash: &'a str,
        execution_state_root: &'a str,
        latest_state_ref: &'a str,
        snapshot_ref: Option<&'a str>,
        journal_ref: Option<&'a str>,
        pinned_refs: &'a [String],
        created_at_ms: i64,
    }

    #[derive(Serialize)]
    struct ManifestHashPayloadV2<'a> {
        schema_version: u32,
        checkpoint_id: &'a str,
        world_id: &'a str,
        height: u64,
        execution_block_hash: &'a str,
        execution_state_root: &'a str,
        predecessor_execution_block_hash: Option<&'a str>,
        latest_state_ref: &'a str,
        snapshot_ref: Option<&'a str>,
        journal_ref: Option<&'a str>,
        pinned_refs: &'a [String],
        created_at_ms: i64,
    }

    fn temp_dir(label: &str) -> std::path::PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock after epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("oasis7-{label}-{unique}"))
    }

    fn write_checkpoint_fixture(records_dir: &Path, schema_version: u32, height: u64) -> String {
        let checkpoint_id = format!("checkpoint-{height:020}-exec-block");
        let latest_state_ref = "snapshot-ref";
        let snapshot_ref = Some("snapshot-ref");
        let journal_ref = Some("journal-ref");
        let pinned_refs = vec!["journal-ref".to_string(), "snapshot-ref".to_string()];
        let created_at_ms = 1_700_000_000_000;
        let hash_bytes = match schema_version {
            1 => serde_cbor::to_vec(&ManifestHashPayload {
                schema_version,
                checkpoint_id: checkpoint_id.as_str(),
                world_id: "live-a",
                height,
                execution_block_hash: "exec-block",
                execution_state_root: "state-root",
                latest_state_ref,
                snapshot_ref,
                journal_ref,
                pinned_refs: pinned_refs.as_slice(),
                created_at_ms,
            })
            .expect("encode v1 manifest hash payload"),
            2 => serde_cbor::to_vec(&ManifestHashPayloadV2 {
                schema_version,
                checkpoint_id: checkpoint_id.as_str(),
                world_id: "live-a",
                height,
                execution_block_hash: "exec-block",
                execution_state_root: "state-root",
                predecessor_execution_block_hash: Some("prev-exec-block"),
                latest_state_ref,
                snapshot_ref,
                journal_ref,
                pinned_refs: pinned_refs.as_slice(),
                created_at_ms,
            })
            .expect("encode v2 manifest hash payload"),
            other => panic!("unsupported fixture schema {other}"),
        };
        let manifest_hash = oasis7_runtime::blake3_hex(hash_bytes.as_slice());
        let manifest_rel_path = format!("{height:020}/manifest.json");
        let manifest_path = records_dir.join("checkpoints").join(&manifest_rel_path);
        fs::create_dir_all(manifest_path.parent().expect("manifest parent"))
            .expect("create checkpoint fixture dir");
        let mut manifest = serde_json::json!({
            "schema_version": schema_version,
            "checkpoint_id": checkpoint_id,
            "world_id": "live-a",
            "height": height,
            "execution_block_hash": "exec-block",
            "execution_state_root": "state-root",
            "latest_state_ref": latest_state_ref,
            "snapshot_ref": snapshot_ref,
            "journal_ref": journal_ref,
            "pinned_refs": pinned_refs,
            "manifest_hash": manifest_hash,
            "created_at_ms": created_at_ms
        });
        if schema_version == 2 {
            manifest["predecessor_execution_block_hash"] = serde_json::json!("prev-exec-block");
        }
        fs::write(
            manifest_path,
            serde_json::to_vec_pretty(&manifest).expect("encode manifest fixture"),
        )
        .expect("write manifest fixture");
        fs::write(
            records_dir.join("checkpoints/latest.json"),
            serde_json::to_vec_pretty(&serde_json::json!({
                "schema_version": 1,
                "checkpoint_id": manifest["checkpoint_id"],
                "height": height,
                "manifest_hash": manifest_hash,
                "manifest_rel_path": manifest_rel_path,
                "updated_at_ms": created_at_ms
            }))
            .expect("encode latest pointer fixture"),
        )
        .expect("write latest pointer fixture");
        manifest_hash
    }

    #[test]
    fn chain_status_reports_no_latest_execution_checkpoint_when_none_is_available() {
        let dir = temp_dir("status-checkpoint-none");
        fs::create_dir_all(dir.as_path()).expect("create records dir");

        let status = build_chain_proof_status(Some(dir.as_path()), None);

        assert!(status.latest_execution_checkpoint.is_none());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn chain_status_reports_retained_v1_execution_checkpoint_identity() {
        let dir = temp_dir("status-checkpoint-v1");
        let manifest_hash = write_checkpoint_fixture(dir.as_path(), 1, 20);

        let status = build_chain_proof_status(Some(dir.as_path()), None);
        let checkpoint = status
            .latest_execution_checkpoint
            .expect("retained v1 checkpoint evidence");

        assert_eq!(checkpoint.schema_version, 1);
        assert_eq!(checkpoint.height, 20);
        assert_eq!(
            checkpoint.checkpoint_id,
            "checkpoint-00000000000000000020-exec-block"
        );
        assert_eq!(checkpoint.manifest_hash, manifest_hash);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn chain_status_reports_verified_v2_execution_checkpoint_identity() {
        let dir = temp_dir("status-checkpoint-v2");
        let manifest_hash = write_checkpoint_fixture(dir.as_path(), 2, 42);

        let status = build_chain_proof_status(Some(dir.as_path()), None);
        let checkpoint = status
            .latest_execution_checkpoint
            .as_ref()
            .expect("verified v2 checkpoint evidence");

        assert_eq!(checkpoint.schema_version, 2);
        assert_eq!(checkpoint.height, 42);
        assert_eq!(
            checkpoint.checkpoint_id,
            "checkpoint-00000000000000000042-exec-block"
        );
        assert_eq!(checkpoint.manifest_hash, manifest_hash);
        let status_json = serde_json::to_value(&status).expect("serialize chain proof status");
        assert_eq!(
            status_json["latest_execution_checkpoint"]["schema_version"],
            serde_json::json!(2)
        );
        assert_eq!(
            status_json["latest_execution_checkpoint"]["height"],
            serde_json::json!(42)
        );
        assert_eq!(
            status_json["latest_execution_checkpoint"]["checkpoint_id"],
            serde_json::json!("checkpoint-00000000000000000042-exec-block")
        );
        assert_eq!(
            status_json["latest_execution_checkpoint"]["manifest_hash"],
            serde_json::json!(manifest_hash)
        );
        let _ = fs::remove_dir_all(dir);
    }
}
