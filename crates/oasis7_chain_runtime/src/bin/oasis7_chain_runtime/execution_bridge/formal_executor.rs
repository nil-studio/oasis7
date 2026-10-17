//! Actual private runtime execution, gated by dual durable formal Frozen input.
use super::{ExecutionBridgeState, checkpoint, controlled_capture as capture, formal_package};
use ed25519_dalek::SigningKey;
use oasis7::runtime::World;
use oasis7_distfs::controlled_authority::formal::{
    coordinator::ExecutableFormalFrozenV1, endpoint::FormalCandidateMaterialV1, *,
};
use oasis7_node::NodeExecutionHook;
use std::path::Path;

pub(super) fn execute_normal(
    frozen: &ExecutableFormalFrozenV1,
    authority: &VerifiedAuthorityV1,
    parent: &VerifiedFormalCommitV1,
    private_root: &Path,
    writer: &SigningKey,
) -> Result<FormalCandidateMaterialV1, String> {
    execute_normal_with_business_reader(frozen, authority, parent, private_root, writer, None, None)
}
pub(super) fn execute_normal_with_business_reader(
    frozen: &ExecutableFormalFrozenV1,
    authority: &VerifiedAuthorityV1,
    parent: &VerifiedFormalCommitV1,
    private_root: &Path,
    writer: &SigningKey,
    business_reader: Option<&super::formal_business_package::SharedBusinessReader>,
    read_anchor: Option<&VerifiedFormalReadAnchorV1>,
) -> Result<FormalCandidateMaterialV1, String> {
    execute_with_business_reader(
        frozen,
        authority,
        parent,
        private_root,
        writer,
        business_reader,
        read_anchor,
        None,
    )
}

/// Original frozen input is restored under its original authority; only the
/// verified recovery successor may sign a newly replayed candidate.
pub(super) fn execute_recovery_replay(
    frozen: &ExecutableFormalFrozenV1,
    recovery: &VerifiedRecovery,
    parent: &VerifiedFormalCommitV1,
    private_root: &Path,
    writer: &SigningKey,
    business_reader: Option<&super::formal_business_package::SharedBusinessReader>,
    read_anchor: Option<&VerifiedFormalReadAnchorV1>,
) -> Result<FormalCandidateMaterialV1, String> {
    if parent.head() != recovery.body().sealed_cut
        || hex::encode(writer.verifying_key().as_bytes())
            != recovery.successor().definition().writer_public_key
    {
        return Err("formal replay requires exact sealed cut and successor writer".into());
    }
    execute_with_business_reader(
        frozen,
        recovery.original_authority(),
        parent,
        private_root,
        writer,
        business_reader,
        read_anchor,
        Some(recovery),
    )
}

#[expect(
    clippy::too_many_arguments,
    reason = "Execution binds each verified authority, parent, storage, and read capability explicitly."
)]
fn execute_with_business_reader(
    frozen: &ExecutableFormalFrozenV1,
    authority: &VerifiedAuthorityV1,
    parent: &VerifiedFormalCommitV1,
    private_root: &Path,
    writer: &SigningKey,
    business_reader: Option<&super::formal_business_package::SharedBusinessReader>,
    read_anchor: Option<&VerifiedFormalReadAnchorV1>,
    recovery: Option<&VerifiedRecovery>,
) -> Result<FormalCandidateMaterialV1, String> {
    let verified_snapshot = formal_package::VerifiedFrozenSnapshot::new(frozen, authority, parent)?;
    let (mut world, original) = formal_package::restore_executable(
        frozen,
        authority,
        parent,
        None,
        read_anchor,
        business_reader,
        &verified_snapshot,
    )?;
    let formal_retention = authority.definition().profile_version == 2;
    if formal_retention {
        let anchor = read_anchor.ok_or("formal retention lacks qualified parent anchor")?;
        if anchor.head().record() != parent.record() {
            return Err("formal retention parent anchor mismatch".into());
        }
        let reader = business_reader.ok_or("formal retention CAS reader unavailable")?;
        let bytes = super::to_cbor(world.snapshot())?;
        let retained =
            oasis7::runtime::VerifiedFormalRetainedHistoryParentV1::verify(anchor, &bytes)?;
        world.attach_verified_formal_history(retained, reader.0.clone())?;
    }
    // Create a new private attempt. Existing partial attempts are evidence,
    // never a mutable cache whose contents can replace the original prestate.
    super::controlled_live::private_directory(private_root)?;
    let mut nonce = [0u8; 32];
    use std::io::Read;
    std::fs::File::open("/dev/urandom")
        .and_then(|mut source| source.read_exact(&mut nonce))
        .map_err(|e| e.to_string())?;
    let attempt = private_root.join(format!("formal-attempt-{}", hex::encode(nonce)));
    if attempt.exists() {
        return Err("formal attempt identity collision".into());
    }
    super::controlled_live::private_directory(&attempt)?;
    let _attempt_pin = super::formal_attempt_lifecycle::active(&attempt, frozen)?;
    let world_dir = attempt.join("world");
    super::controlled_live::private_directory(&world_dir)?;
    let identity = oasis7::world_service::WorldIdentity {
        world_id: authority.definition().world.world_id.clone(),
        genesis_digest: authority.definition().world.genesis_digest.clone(),
    };
    super::durable_transaction::write_private_file_durable(
        &world_dir.join("world-service-identity.json"),
        &serde_json::to_vec(&identity).map_err(|e| e.to_string())?,
    )?;
    let state = ExecutionBridgeState {
        last_applied_committed_height: original.predecessor_execution_height,
        last_execution_block_hash: original.predecessor_execution_block_hash.clone(),
        last_execution_state_root: original.predecessor_execution_state_root.clone(),
        last_node_block_hash: original.predecessor_node_block_hash.clone(),
    };
    let policy = crate::controlled_execution_policy::GuardedExecutionPolicy::current();
    let mut executor = super::driver::NodeRuntimeExecutionDriver::new_with_sandbox(
        attempt.join("state.json"),
        world_dir,
        attempt.join("records"),
        attempt.join("storage"),
        state,
        world,
        Box::new(policy.executor()?),
        policy.hot_head_heights,
        policy.checkpoint_interval,
        policy.checkpoint_keep,
    );
    executor.resource_identity = Some(super::driver::ExecutionResourceIdentity {
        chain_id: authority.definition().world.chain_id.clone(),
        genesis_ref: authority.definition().world.genesis_digest.clone(),
    });
    executor.formal_retention_enabled = formal_retention;
    #[cfg(all(test, unix))]
    super::formal_crash_test_boundary::mark_execution_started(&original.context.node_block_hash);
    let execution_result = executor.on_commit(original.context.clone())?;
    #[cfg(all(test, unix))]
    super::formal_crash_test_boundary::stop_after("execution_completed")?;
    let mut record = checkpoint::load_execution_bridge_record(
        &checkpoint::execution_bridge_record_path(&executor.records_dir, original.context.height),
    )
    .map_err(|error| {
        let entries = std::fs::read_dir(&executor.records_dir)
            .map(|entries| {
                entries
                    .filter_map(Result::ok)
                    .take(8)
                    .map(|entry| entry.file_name().to_string_lossy().into_owned())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        format!(
            "formal executor result height={} expected={} missing authoritative record; entries={entries:?}: {error}",
            execution_result.execution_height, original.context.height,
        )
    })?;
    record.commit_log_ref = None;
    record.checkpoint_ref = None;
    record.world_head_proof_ref = None;
    record.world_head_proof_hash = None;
    let snapshot = executor
        .execution_store
        .get_verified(
            record
                .snapshot_ref
                .as_deref()
                .ok_or("formal output snapshot missing")?,
        )
        .map_err(|e| format!("{e:?}"))?;
    let journal = executor
        .execution_store
        .get_verified(
            record
                .journal_ref
                .as_deref()
                .ok_or("formal output journal missing")?,
        )
        .map_err(|e| format!("{e:?}"))?;
    let effect = executor
        .execution_store
        .get_verified(
            record
                .external_effect_ref
                .as_deref()
                .ok_or("formal output effect missing")?,
        )
        .map_err(|e| format!("{e:?}"))?;
    let output_snapshot: oasis7::runtime::Snapshot = capture::decode_snapshot(&snapshot)?;
    let output_journal: oasis7::runtime::Journal = capture::decode_generic(&journal)?;
    let after = if formal_retention {
        executor
            .execution_world
            .restore_candidate_snapshot_with_attached_formal_history(
                output_snapshot,
                output_journal,
            )
            .map_err(|e| format!("{e:?}"))?
    } else {
        World::from_snapshot(output_snapshot, output_journal).map_err(|e| format!("{e:?}"))?
    };
    let reader = business_reader.cloned().unwrap_or_else(|| {
        super::formal_business_package::SharedBusinessReader(std::sync::Arc::new(
            super::formal_business_package::EmptyBusinessReader,
        ))
    });
    let retained_outputs = if formal_retention {
        Some(formal_package::FormalRetainedExecutionOutputs {
            new_results: executor.formal_new_results.clone(),
            event_root: executor
                .formal_event_root
                .clone()
                .ok_or("formal actual event root missing")?,
            delta: executor.formal_retention_delta.clone(),
            unpruned_journal: executor
                .formal_unpruned_journal
                .clone()
                .ok_or("formal unpruned execution journal missing")?,
            journal_cursors: executor.formal_journal_cursors.clone(),
        })
    } else {
        None
    };
    let candidate = formal_package::candidate_with_business_reader(
        frozen,
        authority,
        parent,
        &after,
        &record,
        &capture::decode_generic(&effect)?,
        writer,
        &reader,
        retained_outputs.as_ref(),
        recovery,
        &verified_snapshot,
    )?;
    // The local executor has no serving reader; drop its private store handles
    // before publishing the immutable duplicate-output delete list.
    let outputs = [
        record.snapshot_ref,
        record.journal_ref,
        record.external_effect_ref,
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>();
    drop(executor);
    super::formal_attempt_lifecycle::awaiting(&attempt, &candidate, &outputs)?;
    Ok(candidate)
}
