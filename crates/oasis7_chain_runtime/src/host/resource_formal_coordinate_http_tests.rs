//! Actual TCP Create/Goal/Bind + actual private Owner proposal and installed
//! Formal archive. Endpoint/issuer doubles are component scope, not CF96 paid.
use super::*;
use oasis7::world_service::private_goal_store::PrivateGoalStore;
use oasis7_client_api::world_service::{self as ws, resource_basis as wire};
pub(super) fn prepare_actual_goal(
    store: &PrivateGoalStore,
    c: &GameTaskControlV1,
    owner: &str,
) -> String {
    let content = GoalContentV1 {
        goal_text: "Verify the actual Resource observation coordinates".into(),
        soft_constraints: vec![],
    };
    let prepare = GameGoalPrepareV1 {
        api_rev: 1,
        world_id: c.world_id.clone(),
        genesis_digest: c.genesis_digest.clone(),
        task_id: c.task_id.clone(),
        agent_id: c.agent_id.clone(),
        owner_public_key: c.owner_public_key.clone(),
        owner_binding: c.owner_binding.clone(),
        owner_generation: c.owner_generation,
        branch_id: c.branch_id.clone(),
        reorg_epoch: c.reorg_epoch,
        expected_task_revision: c.task_revision,
        expected_latest_goal_revision: c.latest_goal_revision,
        goal_revision: DecimalU64(1),
        client_operation_id: "prepare".into(),
        client_nonce: "71".repeat(32),
        goal_digest: game_goal_version_digest(&c.task_id, DecimalU64(1), &content).unwrap(),
        content,
    };
    let proof = ws::private_goal::sign_goal_prepare(prepare, owner).unwrap();
    proof.validate_against(c).unwrap();
    let receipt = store.prepare(&proof).unwrap();
    assert!(receipt.owner_proof == proof);
    receipt.goal_ref
}
pub(super) fn verify_actual_formal_coordinate(
    service: &FormalLiveService,
    cut: &PublishedReadCut,
    archive: &oasis7::world_service::formal_read_service::VerifiedResourceGoalArchiveV2,
    store: &PrivateGoalStore,
    owner_key: &str,
) {
    let control = actual_task(&cut.world).unwrap();
    let canonical =
        oasis7_runtime::service_admission::game_task::active_publish(&cut.world, "agent")
            .unwrap()
            .unwrap();
    let key = correlation::key_digest(&canonical.request.correlation.key).unwrap();
    assert!(archive.exact_result(&key) == Some(&canonical));
    let oasis7_client_api::legacy_world_service::WorldServicePayloadV1::GameGoalPublish(publish) =
        &canonical.request.signed_payload
    else {
        panic!("actual Goal required")
    };
    let receipt = store.get_prepared(&publish.request).unwrap();
    receipt.validate_publish(&publish.request).unwrap();
    assert_eq!(
        receipt.owner_proof.subject_public_key,
        control.owner_public_key
    );
    let current_publication =
        FormalReadPublication::new(cut.anchor.clone(), cut.world.clone()).unwrap();
    let view_request = ws::ReadWorldViewRequest {
        observation_nonce: Some("74".repeat(32)),
        formal_anchor: Some(anchor_ref(&cut.anchor).unwrap()),
        contract_version: 1,
        world: identity(&cut.anchor),
        scope_id: "public".into(),
        min_commit: None,
        fixed_commit: None,
        deadline_unix_ms: Some(now_unix_ms() + 5000),
    };
    let current_commit = current_publication
        .view_at_own_anchor(&view_request, "")
        .unwrap()
        .version
        .commit;
    let owner = oasis7_runtime::service_admission::agent_chat::owner_view(&cut.world, "agent")
        .unwrap()
        .unwrap();
    let delegation = cut
        .world
        .capability_revocation_state()
        .agent_signer_delegations
        .get("agent")
        .unwrap();
    let signer = wire::ResourceSignerRecordV1 {
        world_id: delegation.world_id.clone(),
        branch_id: delegation.branch_id.clone(),
        agent_id: delegation.agent_id.clone(),
        owner_binding: delegation.owner_binding.clone(),
        agent_identity_generation: DecimalU64(delegation.agent_identity_generation),
        generation: DecimalU64(delegation.generation),
        delegate_public_key: delegation.delegate_public_key.clone(),
        revoked: delegation.revoked,
        nonce: DecimalU64(delegation.nonce),
    };
    let request = wire::OwnerSetupFinalResourceReadV2 {
        api_version: 2,
        setup_id: "coordinate-setup".into(),
        provision_id: "coordinate-provision".into(),
        plan_digest: "72".repeat(32),
        world: (&current_commit.world).into(),
        expected_fence: GameTaskFenceV1::from_control(&control),
        expected_signer_delegation: signer.clone(),
        goal_reference: receipt.goal_ref.clone(),
        selection: wire::ResourceBasisSelectionV1::Build {
            site: "site".into(),
            factory: "factory".into(),
            factory_kind: "factory.smelter.mk1".into(),
        },
        observation_nonce: "73".repeat(32),
    };
    request.selection.validate().unwrap();
    let signed: wire::SignedResourceReadV1<_> =
        ws::sign_read_request(wire::RESOURCE_GOAL_PATH_V2, request, owner_key)
            .unwrap()
            .into();
    wire::verify_resource_goal_request(&signed).unwrap();
    let owner = wire::ResourceOwnerStateV1 {
        schema_revision: 1,
        world: (&current_commit.world).into(),
        read_commit: (&current_commit).into(),
        logical_tick: DecimalU64(cut.world.state().time),
        owner: wire::ResourceOwnerFenceV1 {
            agent_id: owner.agent_id,
            player_id: owner.player_id,
            public_key: owner.public_key,
            world_id: owner.world_id,
            reorg_epoch: DecimalU64(owner.reorg_epoch),
            branch_id: owner.canonical_authority.branch_id,
            agent_identity_generation: DecimalU64(
                owner.canonical_authority.agent_identity_generation,
            ),
            current_intent_id: owner.current_intent_id,
        },
        control: control.clone(),
        signer_delegation: Some(signer),
    };
    owner
        .validate(
            &signed.request.expected_fence,
            &signed.subject_public_key,
            &signed.request.expected_signer_delegation,
        )
        .unwrap();
    assert_ne!(
        archive.activation_commit().position,
        canonical.committed_height,
        "the actual Formal and Runtime coordinates must genuinely differ"
    );
    let current_goal = wire::ResourceCurrentGoalV2 {
        activation_profile: Some(wire::ResourceGoalActivationProfileV2::FormalQualifiedV2 {
            committed_runtime_height: DecimalU64(canonical.committed_height),
        }),
        schema_revision: 1,
        control,
        read_commit: (&current_commit).into(),
        activation_commit: Some(archive.activation_commit().into()),
        active_goal: Some(receipt),
    };
    #[derive(Serialize)]
    struct OldResourceCurrentGoal<'a> {
        schema_revision: u32,
        control: &'a GameTaskControlV1,
        read_commit: &'a wire::ResourceCommitRefV1,
        activation_commit: &'a Option<wire::ResourceCommitRefV1>,
        active_goal: &'a Option<ws::private_goal::PreparedGoalReceiptV1>,
    }
    let mut old_none = current_goal.clone();
    old_none.activation_profile = None;
    let old_shape = OldResourceCurrentGoal {
        schema_revision: old_none.schema_revision,
        control: &old_none.control,
        read_commit: &old_none.read_commit,
        activation_commit: &old_none.activation_commit,
        active_goal: &old_none.active_goal,
    };
    assert_eq!(
        oasis7_wasm_abi::encode_canonical_cbor(&old_shape).unwrap(),
        oasis7_wasm_abi::encode_canonical_cbor(&old_none).unwrap(),
        "None must retain exact old Resource wire bytes"
    );
    let mut unknown = serde_json::to_value(&current_goal).unwrap();
    unknown["activation_profile"]["unknown_field"] = serde_json::json!(true);
    assert!(serde_json::from_value::<wire::ResourceCurrentGoalV2>(unknown).is_err());
    let payload = wire::OwnerSetupFinalResourceObservationV2 {
        api_version: 2,
        owner,
        current_goal,
        selection: signed.request.selection.clone(),
        resource: wire::ResourceBasisResolutionV1::Blocked {
            reason: wire::ResourceBasisBlockerV1::LedgerOrObjectUnavailable,
        },
    };
    let request_digest = ws::request_digest(wire::RESOURCE_GOAL_PATH_V2, &signed).unwrap();
    let response = ws::sign_service_response(
        wire::RESOURCE_GOAL_PATH_V2,
        request_digest.clone(),
        payload,
        &hex::encode(service.signer.to_bytes()),
    )
    .unwrap();
    let node_public = hex::encode(service.signer.verifying_key().to_bytes());
    ws::verify_service_response(
        wire::RESOURCE_GOAL_PATH_V2,
        &request_digest,
        &response,
        &node_public,
    )
    .unwrap();
    let response: wire::SignedResourceResponseV1<wire::OwnerSetupFinalResourceObservationV2> =
        response.into();
    assert!(
        wire::verify_resource_goal_response(&signed, &response, &node_public).is_ok(),
        "actual Formal Goal/strong Owner proposal/Node response must reach the Resource-specific coordinate consumer"
    );
    // Each wrong payload is genuinely re-signed by the same configured Node;
    // failure must come from the Resource scope/coordinate consumer.
    for case in 0..6 {
        let mut wrong = response.payload.clone();
        match case {
            0 => {
                wrong.current_goal.activation_profile =
                    Some(wire::ResourceGoalActivationProfileV2::FormalQualifiedV2 {
                        committed_runtime_height: DecimalU64(
                            canonical.committed_height.checked_add(1).unwrap(),
                        ),
                    })
            }
            1 => {
                wrong
                    .current_goal
                    .activation_commit
                    .as_mut()
                    .unwrap()
                    .binding
                    .branch_id = "foreign-branch".into()
            }
            2 => wrong.current_goal.activation_profile = None,
            3 => {
                let v = &mut wrong
                    .current_goal
                    .activation_commit
                    .as_mut()
                    .unwrap()
                    .binding
                    .authority_generation;
                v.0 = v.0.checked_add(1).unwrap();
            }
            4 => {
                wrong
                    .current_goal
                    .activation_commit
                    .as_mut()
                    .unwrap()
                    .binding
                    .governing_manifest_ref = "foreign-governing-manifest".into()
            }
            5 => {
                let mut conflicting = wrong.current_goal.read_commit.clone();
                conflicting.execution_block_hash =
                    if conflicting.execution_block_hash == "ab".repeat(32) {
                        "cd".repeat(32)
                    } else {
                        "ab".repeat(32)
                    };
                wrong.current_goal.activation_commit = Some(conflicting);
            }
            _ => unreachable!(),
        }
        if let Some(activation) = &wrong.current_goal.activation_commit {
            let activation: ws::CommitRef = activation.clone().into();
            activation.validate().unwrap();
        }
        let signed_wrong = ws::sign_service_response(
            wire::RESOURCE_GOAL_PATH_V2,
            request_digest.clone(),
            wrong,
            &hex::encode(service.signer.to_bytes()),
        )
        .unwrap();
        ws::verify_service_response(
            wire::RESOURCE_GOAL_PATH_V2,
            &request_digest,
            &signed_wrong,
            &node_public,
        )
        .unwrap();
        let wrong: wire::SignedResourceResponseV1<wire::OwnerSetupFinalResourceObservationV2> =
            signed_wrong.into();
        assert!(wire::verify_resource_goal_response(&signed, &wrong, &node_public).is_err());
    }
}
