use super::*;
use crate::WorldState;
use oasis7_agent_api::world_action::GeoPos;
use oasis7_client_api::{
    safe_alternative::FineGrainedMoveRequestV1, world_service::sign_read_request,
};
fn fixture() -> (World, String) {
    let private = hex::encode([71u8; 32]);
    let public = sign_read_request("fixture", (), &private)
        .unwrap()
        .subject_public_key;
    let mut w = World::new_with_state(WorldState::default());
    w.submit_action(Action::RegisterAgent {
        agent_id: "agent".into(),
        pos: GeoPos::new(0, 0, 0),
    });
    w.step().unwrap();
    w.submit_action(Action::ClaimStarterOc {
        agent_id: "agent".into(),
        player_id: "owner".into(),
        public_key: Some(public),
    });
    w.step().unwrap();
    w.install_capability_agent_identity("agent", "owner", 1)
        .unwrap();
    w.chain_resource_manifest = w
        .snapshot_with_chain_resource_context(
            crate::ChainResourceDerivationContext {
                world_id: "world",
                chain_id: "fixture-chain",
                genesis_ref: Some("fixture"),
                created_at_height: 0,
                manifest_height: 0,
                commit_block_hash: None,
                tick: w.state.time,
            },
            "fixture-config",
            "fixture-generation",
        )
        .chain_resource_manifest;
    w.bind_cognition_runtime("world", "main", 0, None, "pending", 0)
        .unwrap();
    (w, private)
}
fn signed(private: &str, op: Op) -> SignedReadRequest<SafeAlternativeCommandV1> {
    let c = SafeAlternativeCommandV1 {
        api_rev: 1,
        client_operation_id: format!("{}-{}", op.signing_domain(), digest(&json!(op))),
        world_id: "world".into(),
        genesis_digest: "fixture".into(),
        branch_id: "main".into(),
        reorg_epoch: 0,
        agent_id: "agent".into(),
        owner_binding: "owner".into(),
        owner_generation: 1,
        operation: op,
    };
    sign_read_request(c.signing_domain(), c.clone(), private).unwrap()
}
fn proposal(private: &str, id: &str) -> SignedReadRequest<SafeAlternativeCommandV1> {
    signed(
        private,
        Op::Propose {
            request_id: id.into(),
            original: FineGrainedMoveRequestV1 {
                goal: "Take path by waypoint with precise arrival".into(),
                waypoints: vec![GeoPos::new(20, 0, 0), GeoPos::new(40, 0, 0)],
                arrival_tick: Some(10),
            },
            valid_until_tick: 20,
            predecessor_request_id: None,
        },
    )
}
fn confirm(w: &mut World, private: &str, r: &Value, candidate: usize) -> Value {
    w.apply_authenticated_safe_alternative(&signed(
        private,
        Op::Confirm {
            request_id: r["request_id"].as_str().unwrap().into(),
            generation: r["generation"].as_u64().unwrap(),
            candidate_id: r["candidates"][candidate]["candidate_id"]
                .as_str()
                .unwrap()
                .into(),
            candidate_digest: r["candidates"][candidate]["candidate_digest"]
                .as_str()
                .unwrap()
                .into(),
            confirmation_id: "confirm".into(),
            valid_until_tick: 15,
        },
    ))
    .unwrap()
}
fn execute(private: &str, r: &Value) -> SignedReadRequest<SafeAlternativeCommandV1> {
    signed(
        private,
        Op::Execute {
            request_id: r["request_id"].as_str().unwrap().into(),
            authorization_id: r["authorization"]["authorization_id"]
                .as_str()
                .unwrap()
                .into(),
            execution_id: "execution".into(),
        },
    )
}
#[test]
fn preview_independent_confirmation_exact_retry_restore_and_unique_effect() {
    let (mut w, key) = fixture();
    let before = w.state().clone();
    let queued = w.pending_actions_len();
    let p = proposal(&key, "request");
    let r = w.apply_authenticated_safe_alternative(&p).unwrap();
    assert_eq!(w.state(), &before);
    assert_eq!(w.pending_actions_len(), queued);
    assert_eq!(r["candidates"].as_array().unwrap().len(), 2);
    let unconfirmed = signed(
        &key,
        Op::Execute {
            request_id: "request".into(),
            authorization_id: "fake".into(),
            execution_id: "execution".into(),
        },
    );
    assert!(
        w.apply_authenticated_safe_alternative(&unconfirmed)
            .is_err()
    );
    let r = confirm(&mut w, &key, &r, 0);
    let call = execute(&key, &r);
    let bound = w.apply_authenticated_safe_alternative(&call).unwrap();
    assert_eq!(bound["state"], "execution_bound");
    assert_eq!(w.pending_actions_len(), 1);
    assert_eq!(
        w.apply_authenticated_safe_alternative(&call).unwrap(),
        bound
    );
    assert_eq!(w.pending_actions_len(), 1);
    let mut restored = World::from_snapshot(w.snapshot(), w.journal().clone()).unwrap();
    restored.step().unwrap();
    let read = signed(
        &key,
        Op::Read {
            request_id: "request".into(),
        },
    );
    let done = restored
        .apply_authenticated_safe_alternative(&read)
        .unwrap();
    assert_eq!(done["state"], "applied");
    assert_eq!(done["authorization"]["state"], "consumed");
    assert_eq!(
        restored.state().agents["agent"].state.pos,
        GeoPos::new(20, 0, 0)
    );
    assert_eq!(
        restored
            .apply_authenticated_safe_alternative(&call)
            .unwrap(),
        done
    );
    assert_eq!(restored.pending_actions_len(), 0);
    let again = World::from_snapshot(restored.snapshot(), restored.journal().clone()).unwrap();
    assert_eq!(
        again.capability_revocation_state.safe_alternatives,
        restored.capability_revocation_state.safe_alternatives
    );
}
#[test]
fn cancel_and_authority_change_reject_without_effect_and_never_rebind() {
    let (mut w, key) = fixture();
    let p = proposal(&key, "request");
    let r = w.apply_authenticated_safe_alternative(&p).unwrap();
    let r = confirm(&mut w, &key, &r, 0);
    let call = execute(&key, &r);
    w.apply_authenticated_safe_alternative(&call).unwrap();
    w.apply_authenticated_safe_alternative(&signed(
        &key,
        Op::Cancel {
            request_id: "request".into(),
        },
    ))
    .unwrap();
    w.step().unwrap();
    assert_eq!(w.state().agents["agent"].state.pos, GeoPos::new(0, 0, 0));
    let r = w.apply_authenticated_safe_alternative(&call).unwrap();
    assert_eq!(r["outcome"]["status"], "rejected");
    assert_eq!(w.pending_actions_len(), 0);
    assert!(
        w.apply_authenticated_safe_alternative(&signed(
            &key,
            Op::Reevaluate {
                request_id: "request".into(),
                valid_until_tick: 30
            }
        ))
        .is_err()
    );
    let mut next = proposal(&key, "next").request;
    if let Op::Propose {
        predecessor_request_id,
        ..
    } = &mut next.operation
    {
        *predecessor_request_id = Some("request".into());
    }
    let next = sign_read_request(next.signing_domain(), next.clone(), &key).unwrap();
    assert!(w.apply_authenticated_safe_alternative(&next).is_ok());
}
#[test]
fn stale_generation_wrong_digest_owner_change_and_snapshot_root_fence() {
    let (mut w, key) = fixture();
    let p = proposal(&key, "request");
    let r = w.apply_authenticated_safe_alternative(&p).unwrap();
    let c = signed(
        &key,
        Op::Confirm {
            request_id: "request".into(),
            generation: 1,
            candidate_id: "endpoint-1".into(),
            candidate_digest: "tampered".into(),
            confirmation_id: "confirm".into(),
            valid_until_tick: 15,
        },
    );
    assert!(w.apply_authenticated_safe_alternative(&c).is_err());
    let reevaluate = signed(
        &key,
        Op::Reevaluate {
            request_id: "request".into(),
            valid_until_tick: 25,
        },
    );
    let updated = w.apply_authenticated_safe_alternative(&reevaluate).unwrap();
    assert_eq!(updated["generation"], 2);
    assert_eq!(updated["history"][0]["candidates"], r["candidates"]);
    assert_eq!(
        w.apply_authenticated_safe_alternative(&reevaluate).unwrap(),
        updated
    );
    assert!(w.apply_authenticated_safe_alternative(&c).is_err());
    let mut snapshot = w.snapshot();
    snapshot
        .capability_revocation_state
        .safe_alternatives
        .clear();
    assert!(World::from_snapshot(snapshot, w.journal().clone()).is_err());
    let _ = r;
}

#[test]
fn expiry_policy_owner_module_fences_and_journal_replay() {
    for change in 0..5 {
        let (mut w, key) = fixture();
        let checkpoint = w.snapshot();
        let r = w
            .apply_authenticated_safe_alternative(&proposal(&key, "request"))
            .unwrap();
        let r = confirm(&mut w, &key, &r, 0);
        let call = execute(&key, &r);
        w.apply_authenticated_safe_alternative(&call).unwrap();
        match change {
            0 => w.state.time = 15,
            1 => w
                .state
                .gameplay_policy
                .forbidden_location_ids
                .push("20:0:0".into()),
            2 => {
                w.install_capability_agent_identity("agent", "other-owner", 2)
                    .unwrap();
            }
            3 => {
                w.state.module_instances.insert(
                    "changed".into(),
                    crate::state::ModuleInstanceState {
                        instance_id: "changed".into(),
                        module_id: "changed".into(),
                        module_version: "1".into(),
                        wasm_hash: String::new(),
                        owner_agent_id: "agent".into(),
                        install_target: Default::default(),
                        active: true,
                        installed_at: w.state.time,
                    },
                );
            }
            _ => {
                w.module_registry
                    .active
                    .insert("changed".into(), "1".into());
            }
        }
        w.step().unwrap();
        assert_eq!(w.state().agents["agent"].state.pos, GeoPos::new(0, 0, 0));
        let record = w
            .capability_revocation_state
            .safe_alternatives
            .values()
            .next()
            .unwrap();
        assert_eq!(record["outcome"]["status"], "rejected");
        assert_eq!(record["authorization"]["state"], "revoked");
        let _ = checkpoint;
    }
    let (mut w, key) = fixture();
    let checkpoint = w.snapshot();
    let r = w
        .apply_authenticated_safe_alternative(&proposal(&key, "request"))
        .unwrap();
    let r = confirm(&mut w, &key, &r, 0);
    w.apply_authenticated_safe_alternative(&execute(&key, &r))
        .unwrap();
    w.step().unwrap();
    let restored = World::from_snapshot(checkpoint, w.journal().clone()).unwrap();
    assert_eq!(
        restored.state().agents["agent"].state.pos,
        w.state().agents["agent"].state.pos
    );
    assert_eq!(restored.pending_actions_len(), 0);
    assert_eq!(
        restored.capability_revocation_state.safe_alternatives,
        w.capability_revocation_state.safe_alternatives
    );
}
#[test]
fn no_safe_candidate_competing_selection_and_execution_identity_conflict() {
    let (mut w, key) = fixture();
    w.state.gameplay_policy.forbidden_location_ids = vec!["20:0:0".into(), "40:0:0".into()];
    let none = w
        .apply_authenticated_safe_alternative(&proposal(&key, "none"))
        .unwrap();
    assert_eq!(none["state"], "no_safe_alternative");
    assert_eq!(w.pending_actions_len(), 0);
    w.state.gameplay_policy.forbidden_location_ids.clear();
    let r = w
        .apply_authenticated_safe_alternative(&proposal(&key, "request"))
        .unwrap();
    let selected = confirm(&mut w, &key, &r, 0);
    for (id, index) in [("different", 1), ("confirm", 1)] {
        let candidate = &r["candidates"][index];
        let c = signed(
            &key,
            Op::Confirm {
                request_id: "request".into(),
                generation: 1,
                candidate_id: candidate["candidate_id"].as_str().unwrap().into(),
                candidate_digest: candidate["candidate_digest"].as_str().unwrap().into(),
                confirmation_id: id.into(),
                valid_until_tick: 15,
            },
        );
        assert!(w.apply_authenticated_safe_alternative(&c).is_err());
    }
    let call = execute(&key, &selected);
    w.apply_authenticated_safe_alternative(&call).unwrap();
    let mut competing = call.request.clone();
    if let Op::Execute { execution_id, .. } = &mut competing.operation {
        *execution_id = "another".into();
    }
    let competing = sign_read_request(competing.signing_domain(), competing.clone(), &key).unwrap();
    assert!(w.apply_authenticated_safe_alternative(&competing).is_err());
    assert_eq!(w.pending_actions_len(), 1);
}
