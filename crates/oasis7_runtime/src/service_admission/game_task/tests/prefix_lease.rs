//! A prepared lifecycle prefix may survive ordinary progress, never authority drift.
use super::*;

fn prepared() -> (World, String, SchedulerIntentV1) {
    let (mut world, owner, delegate, initial) = boot();
    let accepted = execute(
        &world,
        executor_request(
            &owner,
            &initial,
            "lease-bind",
            bind(&delegate, 1, "lease-executor"),
        ),
        &[],
        3,
    );
    world.record_world_service_result(accepted).unwrap();
    let fence = GameTaskFenceV1::from_control(&current_control(&world, "agent").unwrap().unwrap());
    let authority = world.canonical_native_task_authority(&fence).unwrap();
    let intent = SchedulerIntentV1 {
        agent_id: "agent".into(),
        request_id: "lease-decision".into(),
        delegation_generation: 1,
        task_prefix_binding_revision: 1,
        captured_base_binding: world.current_cognition_base_binding().unwrap(),
        operation: SchedulerOperationV1::TaskProviderPrefix {
            context: TaskContextIdentityV1 {
                authority_mode: Some(
                    oasis7_agent_api::game::TaskContextAuthorityModeV1::NativeExecutorV1,
                ),
                native_authorization_root: Some(authority.native_authorization_root),
                schema_revision: 2,
                fence,
                agent_session_id: "lease-session".into(),
                agent_turn_id: "lease-turn".into(),
                decision_request_id: "lease-decision".into(),
                retry_seq: 1,
                transport_attempt: 1,
                request_commitment: format!("blake3:{}", "11".repeat(32)),
                context_commitment: format!("blake3:{}", "22".repeat(32)),
                provider_invocation_commitment: format!("blake3:{}", "33".repeat(32)),
                valid_until_tick: DecimalU64(authority.valid_until_tick),
            },
        },
    };
    (world, delegate, intent)
}

fn advance_to(world: &mut World, tick: u64) {
    while world.state().time < tick {
        world.step().unwrap();
    }
}

fn reject_unchanged(world: &mut World, private: &str, intent: SchedulerIntentV1) {
    let before = world.snapshot();
    let error = world
        .apply_authenticated_scheduler(sign_read_request("scheduler", intent, private).unwrap());
    assert!(error.is_err(), "invalid prefix unexpectedly accepted");
    assert_eq!(world.snapshot(), before, "rejection must be transactional");
}

#[test]
fn task_prefix_lease_accepts_progress_and_original_expiry_boundary() {
    for elapsed in [1, 16] {
        let (mut world, private, intent) = prepared();
        let captured = intent.captured_base_binding.base_tick;
        advance_to(&mut world, captured + elapsed);
        assert_ne!(
            world.current_cognition_base_binding().unwrap(),
            intent.captured_base_binding
        );
        world
            .apply_authenticated_scheduler(
                sign_read_request("scheduler", intent, &private).unwrap(),
            )
            .expect("ordinary progress inside the original lease must admit the prefix");
    }
}

#[test]
fn task_prefix_lease_rejects_expired_and_future_capture_without_mutation() {
    let (mut world, private, intent) = prepared();
    advance_to(&mut world, intent.captured_base_binding.base_tick + 17);
    reject_unchanged(&mut world, &private, intent);
    let (mut world, private, mut intent) = prepared();
    intent.captured_base_binding.base_tick += 1;
    reject_unchanged(&mut world, &private, intent);
}

#[test]
fn task_prefix_lease_rejects_authority_drift_without_mutation() {
    for field in [
        "world",
        "branch",
        "finality_epoch",
        "finality_hash",
        "finality_status",
        "reorg",
        "manifest",
    ] {
        let (mut world, private, mut intent) = prepared();
        advance_to(&mut world, intent.captured_base_binding.base_tick + 1);
        let base = &mut intent.captured_base_binding;
        match field {
            "world" => base.world_id = "other-world".into(),
            "branch" => base.branch_id = "other-branch".into(),
            "finality_epoch" => base.finality_epoch += 1,
            "finality_hash" => {
                base.finality_block_hash = Some(format!("blake3:{}", "44".repeat(32)))
            }
            "finality_status" => base.finality_status = "suspended".into(),
            "reorg" => base.reorg_epoch += 1,
            "manifest" => base.runtime_manifest_hash = format!("blake3:{}", "55".repeat(32)),
            _ => unreachable!(),
        }
        reject_unchanged(&mut world, &private, intent);
    }
}

#[test]
fn task_prefix_lease_rejects_same_tick_hash_and_invalid_binding() {
    for invalid_digest in [false, true] {
        let (mut world, private, mut intent) = prepared();
        intent.captured_base_binding.base_world_hash = if invalid_digest {
            "invalid-digest".into()
        } else {
            format!("blake3:{}", "77".repeat(32))
        };
        if invalid_digest {
            advance_to(&mut world, intent.captured_base_binding.base_tick + 1);
        }
        reject_unchanged(&mut world, &private, intent);
    }
}

#[test]
fn task_prefix_lease_rejects_extension_and_overflow() {
    for overflow in [false, true] {
        let (mut world, private, mut intent) = prepared();
        let SchedulerOperationV1::TaskProviderPrefix { context } = &mut intent.operation else {
            unreachable!()
        };
        if overflow {
            intent.captured_base_binding.base_tick = u64::MAX;
            context.valid_until_tick = DecimalU64(u64::MAX);
        } else {
            context.valid_until_tick.0 += 1;
            advance_to(&mut world, intent.captured_base_binding.base_tick + 1);
        }
        reject_unchanged(&mut world, &private, intent);
    }
}

#[test]
fn task_prefix_lease_rejects_changed_fence_and_native_root_without_mutation() {
    for change_root in [false, true] {
        let (mut world, private, mut intent) = prepared();
        advance_to(&mut world, intent.captured_base_binding.base_tick + 1);
        let SchedulerOperationV1::TaskProviderPrefix { context } = &mut intent.operation else {
            unreachable!()
        };
        if change_root {
            context.native_authorization_root = Some(format!("blake3:{}", "66".repeat(32)));
        } else {
            context.fence.executor_epoch.0 += 1;
        }
        reject_unchanged(&mut world, &private, intent);
    }
}

#[test]
fn non_task_prefix_scheduler_keeps_exact_base_check() {
    let (mut world, private, mut intent) = prepared();
    intent.task_prefix_binding_revision = 0;
    intent.operation = SchedulerOperationV1::ReleaseLease {
        lease_id: "missing-lease".into(),
    };
    advance_to(&mut world, intent.captured_base_binding.base_tick + 1);
    let before = world.snapshot();
    let error = world
        .apply_authenticated_scheduler(sign_read_request("scheduler", intent, &private).unwrap())
        .unwrap_err();
    assert_eq!(error, "scheduler base binding changed");
    assert_eq!(world.snapshot(), before);
}

#[test]
fn legacy_task_prefix_retains_exact_base_and_omitted_revision_decodes_as_zero() {
    let (mut world, private, mut intent) = prepared();
    intent.task_prefix_binding_revision = 0;
    let encoded = serde_json::to_value(&intent).unwrap();
    assert!(encoded.get("task_prefix_binding_revision").is_none());
    let decoded: SchedulerIntentV1 = serde_json::from_value(encoded).unwrap();
    assert_eq!(decoded.task_prefix_binding_revision, 0);
    assert_eq!(decoded, intent);
    advance_to(&mut world, intent.captured_base_binding.base_tick + 1);
    let before = world.snapshot();
    let error = world
        .apply_authenticated_scheduler(sign_read_request("scheduler", decoded, &private).unwrap())
        .unwrap_err();
    assert_eq!(error, "scheduler base binding changed");
    assert_eq!(world.snapshot(), before);
}

#[test]
fn task_prefix_lease_does_not_replace_an_active_context() {
    let (mut world, private, intent) = prepared();
    let signed = sign_read_request("scheduler", intent.clone(), &private).unwrap();
    let receipt = world.apply_authenticated_scheduler(signed.clone()).unwrap();
    let payload = WorldServicePayloadV1::Scheduler(signed);
    world
        .record_world_service_result(CanonicalIntentResultV1 {
            request: SubmitIntentRequest {
                contract_version: 1,
                correlation: derive_correlation(
                    WorldIdentity {
                        world_id: "world".into(),
                        genesis_digest: "fixture".into(),
                    },
                    &payload,
                )
                .unwrap(),
                deadline_unix_ms: None,
                signed_payload: payload,
            },
            action_id: 4,
            committed_height: 4,
            receipt,
            rejected: None,
        })
        .unwrap();
    let mut next = intent;
    next.request_id = "competing-decision".into();
    let SchedulerOperationV1::TaskProviderPrefix { context } = &mut next.operation else {
        unreachable!()
    };
    context.decision_request_id = next.request_id.clone();
    context.agent_turn_id = "competing-turn".into();
    advance_to(&mut world, next.captured_base_binding.base_tick + 1);
    let before = world.snapshot();
    let error = world
        .apply_authenticated_scheduler(sign_read_request("scheduler", next, &private).unwrap())
        .unwrap_err();
    assert_eq!(error, "task already has an active context");
    assert_eq!(world.snapshot(), before);
}

#[test]
fn task_prefix_revision_rejects_unknown_and_non_prefix_opt_in() {
    for (revision, prefix) in [(2, true), (u16::MAX, true), (1, false)] {
        let (mut world, private, mut intent) = prepared();
        intent.task_prefix_binding_revision = revision;
        if !prefix {
            intent.operation = SchedulerOperationV1::ReleaseLease {
                lease_id: "missing".into(),
            };
        }
        let before = world.snapshot();
        let error = world
            .apply_authenticated_scheduler(
                sign_read_request("scheduler", intent, &private).unwrap(),
            )
            .unwrap_err();
        assert_eq!(error, "unsupported task prefix binding revision");
        assert_eq!(world.snapshot(), before);
    }
}
