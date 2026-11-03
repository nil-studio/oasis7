use super::super::super::checkpoint;

#[path = "world_service_private_task_context_phases.rs"]
mod phases;
use super::private_goal_tests::{consumer, http, http_with_hook};
use super::*;
use oasis7::world_service::{
    private_goal_prepare::sign_goal_prepare, private_goal_read::publication,
    private_goal_store::PrivateGoalStore, private_task_context::*,
};
use oasis7_agent_api::game::{
    DecimalU64, GAME_EXECUTOR_CONTROL_DOMAIN, GAME_GOAL_PUBLISH_DOMAIN, GameExecutorControlV1,
    GameExecutorOperationV1, GameGoalPrepareV1, GameTaskFenceV1, GoalContentV1,
    game_goal_version_digest,
};
use phases::*;
use std::sync::{Arc, Mutex};

#[path = "world_service_private_task_context_initial.rs"]
mod initial;
#[path = "world_service_private_task_context_stack.rs"]
mod stack;
use initial::{
    InitialTaskContextFixture, PrepareAndReopenTaskContextInputs, RecoveredTaskContextFixture,
    prepare_and_reopen_task_context, prepare_initial_task_context,
};
use stack::{
    boxed_context_driver_fixture, boxed_context_driver_restart, boxed_context_value,
    commit_empty_context_block, run_context_stage,
};

#[path = "world_service_task_authority_compat.rs"]
mod authority_compat;
fn verify_live_task_evidence_retention(
    dir: &std::path::Path,
    driver: &mut NodeRuntimeExecutionDriver,
    world: &WorldIdentity,
    create: &SubmitIntentRequest<WorldServicePayloadV1>,
) -> LocalCasStore {
    // Active goal evidence survives both the normal 32-block window and full reconciliation.
    for height in 7..40 {
        commit(driver, height, create.clone());
    }
    checkpoint::task_evidence::reset_protected_height_validation_count();
    commit_empty_context_block(driver, 40);
    assert_eq!(
        checkpoint::task_evidence::protected_height_validation_count(),
        1,
        "one retention transaction must fully validate existing task-pin shards once"
    );
    let cas = LocalCasStore::new(dir.join("store"));
    assert_eq!(
        cas.list_pin_scope_shards("execution_bridge_live_tasks_v1")
            .unwrap(),
        vec![format!("record-{:020}", 4)],
        "published task evidence shards must equal the current live dependency set"
    );
    // An unchanged, fully verified task dependency must not be re-published.
    // A historical mtime distinguishes reuse from atomic rewriting without
    // depending on filesystem timestamp granularity or execution speed.
    let shard = dir
        .join("store/pin_scopes/execution_bridge_live_tasks_v1/record-00000000000000000004.json");
    let sentinel = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1);
    std::fs::File::options()
        .write(true)
        .open(&shard)
        .unwrap()
        .set_times(std::fs::FileTimes::new().set_modified(sentinel))
        .unwrap();
    let retained_bytes = std::fs::read(&shard).unwrap();
    for _ in 0..3 {
        checkpoint::refresh_live_task_read_evidence(
            &dir.join("records"),
            &cas,
            &driver.execution_world,
            2,
        )
        .unwrap();
        assert_eq!(std::fs::read(&shard).unwrap(), retained_bytes);
        assert_eq!(
            std::fs::metadata(&shard).unwrap().modified().unwrap(),
            sentinel,
            "unchanged validated task shard was rewritten"
        );
    }
    // Reuse must still reject a parseable shard with the wrong semantic set.
    std::fs::write(&shard, br#"{"pins":[]}"#).unwrap();
    assert!(
        checkpoint::refresh_live_task_read_evidence(
            &dir.join("records"),
            &cas,
            &driver.execution_world,
            2,
        )
        .is_err()
    );
    let latest_record_path = checkpoint::execution_bridge_record_path(&dir.join("records"), 40);
    let latest_record =
        checkpoint::load_execution_bridge_record(latest_record_path.as_path()).unwrap();
    assert!(
        checkpoint::run_execution_bridge_incremental_retention_maintenance(
            &dir.join("records"),
            &cas,
            &latest_record,
            2,
            32,
            3,
        )
        .is_err(),
        "standalone incremental maintenance must revalidate a corrupt task-pin shard"
    );
    assert!(
        checkpoint::run_execution_bridge_retention_maintenance(&dir.join("records"), &cas, 2,)
            .is_err(),
        "full reconciliation must revalidate a corrupt task-pin shard"
    );
    std::fs::write(&shard, &retained_bytes).unwrap();
    let validated = checkpoint::refresh_live_task_read_evidence(
        &dir.join("records"),
        &cas,
        &driver.execution_world,
        2,
    )
    .unwrap();
    let mut replacement_record = latest_record.clone();
    replacement_record.node_block_hash = Some("replacement-head-hash".into());
    assert!(
        checkpoint::run_execution_bridge_incremental_retention_maintenance_after_task_refresh(
            &dir.join("records"),
            &cas,
            &replacement_record,
            validated,
            2,
            32,
            3,
        )
        .is_err(),
        "validated task-pin set must not be reused for a replaced same-height record"
    );
    checkpoint::run_execution_bridge_retention_maintenance(&dir.join("records"), &cas, 2).unwrap();
    assert!(
        super::super::super::world_service_read::pin_at_height(
            &dir.join("records"),
            &dir.join("store"),
            world,
            4
        )
        .is_ok()
    );
    // The live-task cache is derived: remove it and restart from the real current head.
    cas.clear_pin_scope("execution_bridge_live_tasks_v1")
        .unwrap();
    cas
}

fn complete_context_scenario_with_revision(
    authority_mode: Option<oasis7_agent_api::game::TaskContextAuthorityModeV1>,
    other_provider: bool,
    prefix_revision: u16,
) {
    const AGE: u64 = 35;
    let (dir, mut driver, world, owner, owner_public) =
        boxed_context_driver_fixture(authority_mode.is_some(), other_provider);
    stack::assert_fixture_authorities(&driver, authority_mode.is_some(), other_provider);
    let InitialTaskContextFixture {
        private_goal_dir: private,
        create,
        store,
        goal,
        proof,
        delegate,
        delegate_public,
        signer,
        runtime,
    } = prepare_initial_task_context(
        &dir,
        driver.as_mut(),
        &world,
        &owner,
        &owner_public,
        authority_mode,
    );
    let cas = verify_live_task_evidence_retention(&dir, driver.as_mut(), &world, &create);
    drop(driver);
    let bootstrap = super::super::super::local_bootstrap::derive_service_execution_bootstrap(
        &dir.join("world"),
        &dir.join("records"),
        &dir.join("store"),
        "w1",
    )
    .unwrap();
    let mut driver = boxed_context_driver_restart(&dir, bootstrap);
    let RecoveredTaskContextFixture {
        store,
        prepared,
        original_context_file,
        get,
    } = prepare_and_reopen_task_context(PrepareAndReopenTaskContextInputs {
        dir: &dir,
        private: &private,
        runtime: &runtime,
        signer: &signer,
        store,
        world: &world,
        proof: &proof,
        goal: &goal,
        delegate: &delegate,
        authority_mode,
        other_provider,
    });
    let ReplacementTaskContext {
        goal: _g2,
        prepared: g2_context,
        prepare: g2_prepare,
        fence,
        ..
    } = verify_initial_context_capture(InitialCaptureInputs {
        driver: driver.as_mut(),
        dir: &dir,
        world: &world,
        owner: &owner,
        authority_mode,
        runtime: &runtime,
        signer: &signer,
        store: &store,
        goal: &goal,
        create: &create,
        proof: &proof,
        delegate: &delegate,
        prepared: &prepared,
        original_context_file: &original_context_file,
        get: &get,
        cas: &cas,
        prefix_revision,
        age: AGE,
    });
    // A context lease expires before the still-valid grant. Cleanup is entirely public.
    let mut expiry_request = g2_prepare.request.clone();
    expiry_request.client_operation_id = "expiry-context".into();
    expiry_request.client_nonce = "90".repeat(32);
    let expiry_proof =
        sign_read_request(TASK_CONTEXT_PREPARE_PATH, expiry_request, &delegate).unwrap();
    let expiry = consumer(&dir, &runtime, &signer, &store, &world, |client| {
        client.prepare_task_context(&expiry_proof).unwrap()
    });
    assert_eq!(
        expiry.valid_until_tick.0,
        expiry.snapshot.captured_base_binding.base_tick + TASK_CONTEXT_LEASE_TICKS
    );
    assert!(expiry.valid_until_tick.0 <= expiry.snapshot.authority_valid_until_tick().unwrap());
    let expiry_prefix = request(
        &world,
        WorldServicePayloadV1::Scheduler(
            sign_read_request("scheduler", public_prefix(&expiry).unwrap(), &delegate).unwrap(),
        ),
    );
    commit(&mut driver, AGE + 13, expiry_prefix.clone());
    assert!(
        game_task_tests::recorded(&driver, &expiry_prefix)
            .rejected
            .is_none()
    );
    assert_expiry_retention_matches_canonical(
        driver.as_ref(),
        &expiry_prefix,
        &expiry,
        &delegate_public,
        &owner_public,
    );
    run_context_stage(|| {
        assert_stop_and_revocation_reject_context(
            driver.as_ref(),
            &expiry,
            &expiry_prefix,
            &world,
            &owner,
            &delegate_public,
        );
    });
    let expiry_get = sign_read_request(
        TASK_CONTEXT_READ_PATH,
        TaskContextGetV1 {
            schema_revision: 1,
            context: expiry.identity.clone(),
            prepare_operation_id: expiry_proof.request.client_operation_id.clone(),
        },
        &delegate,
    )
    .unwrap();
    let expiry_height =
        advance_context_until_expired(driver.as_mut(), &expiry, AGE + 13, &delegate_public);
    assert_eq!(
        http(
            &dir,
            &runtime,
            &signer,
            Some(&store),
            TASK_CONTEXT_READ_PATH,
            &expiry_get,
            ""
        )
        .0,
        409
    );
    let failure = public_outcome_intent(
        &expiry.identity,
        driver
            .execution_world
            .current_cognition_base_binding()
            .unwrap(),
        "context_expired",
    )
    .unwrap();
    let failure = request(
        &world,
        WorldServicePayloadV1::Scheduler(
            sign_read_request("scheduler", failure, &delegate).unwrap(),
        ),
    );
    commit(&mut driver, expiry_height + 1, failure.clone());
    assert!(
        game_task_tests::recorded(&driver, &failure)
            .rejected
            .is_none()
    );
    let mut fresh_request = g2_prepare.request.clone();
    fresh_request.client_operation_id = "after-expiry-context".into();
    fresh_request.client_nonce = "91".repeat(32);
    let fresh_proof =
        sign_read_request(TASK_CONTEXT_PREPARE_PATH, fresh_request, &delegate).unwrap();
    let fresh = consumer(&dir, &runtime, &signer, &store, &world, |client| {
        client.prepare_task_context(&fresh_proof).unwrap()
    });
    assert_eq!(
        fresh.identity.fence.executor_binding,
        expiry.identity.fence.executor_binding
    );
    let fresh_prefix = request(
        &world,
        WorldServicePayloadV1::Scheduler(
            sign_read_request("scheduler", public_prefix(&fresh).unwrap(), &delegate).unwrap(),
        ),
    );
    commit(&mut driver, expiry_height + 2, fresh_prefix.clone());
    assert!(
        game_task_tests::recorded(&driver, &fresh_prefix)
            .rejected
            .is_none()
    );
    let fresh_get = sign_read_request(
        TASK_CONTEXT_READ_PATH,
        TaskContextGetV1 {
            schema_revision: 1,
            context: fresh.identity.clone(),
            prepare_operation_id: fresh_proof.request.client_operation_id.clone(),
        },
        &delegate,
    )
    .unwrap();
    let fresh_captured = consumer(&dir, &runtime, &signer, &store, &world, |client| {
        client.read_task_context(&fresh_get).unwrap()
    });
    let fresh_action = request(
        &world,
        WorldServicePayloadV1::Cognition(
            sign_read_request(
                "cognition",
                cognition_intent(
                    &fresh_captured,
                    Action::MoveAgent {
                        agent_id: "agent-a".into(),
                        to: oasis7_agent_api::world_action::GeoPos::new(3, 0, 0),
                    },
                )
                .unwrap(),
                &delegate,
            )
            .unwrap(),
        ),
    );
    commit(&mut driver, expiry_height + 3, fresh_action.clone());
    assert!(
        game_task_tests::recorded(&driver, &fresh_action)
            .rejected
            .is_none()
    );
    assert_eq!(
        driver.execution_world.state().agents["agent-a"].state.pos,
        oasis7_agent_api::world_action::GeoPos::new(3, 0, 0)
    );
    verify_retained_evidence_and_fail_closed_on_missing_history(
        RetainedEvidenceVerificationInputs {
            dir: &dir,
            cas: &cas,
            world: &world,
            expiry_height,
            g2_context: &g2_context,
            authority_mode,
            fence: &fence,
            delegate: &delegate,
            signer: &signer,
            store: &store,
            runtime: &runtime,
            owner: &owner,
            age: AGE,
        },
    );
    verify_stop_fences_delivery_and_cleanup(StopDeliveryContext {
        dir,
        private,
        driver,
        world,
        runtime,
        signer,
        store,
        g2_prepare,
        fence,
        delegate,
        owner,
        goal,
        cas,
        expiry_height,
        age: AGE,
        create,
    });
}
