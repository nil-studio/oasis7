//! Original recipe correlation attachment. Metadata never grants authority.
//! Rule Modify supplies no original here, matching World's replacement envelope.
pub(in crate::world) fn attach_recipe_origin_with_work_v1<E>(
    body: &mut crate::WorldEventBody,
    origin: Option<&crate::CommittedRecipeOrigin>,
    before_clone: &mut impl FnMut(&crate::CommittedRecipeOrigin) -> Result<(), E>,
) -> Result<(), E> {
    if let crate::WorldEventBody::Domain(crate::DomainEvent::RecipeStarted {
        committed_recipe_origin,
        ..
    }) = body
    {
        if let Some(origin) = origin {
            before_clone(origin)?;
        }
        *committed_recipe_origin = origin.cloned();
    }
    Ok(())
}
/// Every dynamic String in the original closed correlation DTO, in wire order.
pub(super) fn origin_texts_v1(origin: &crate::CommittedRecipeOrigin) -> impl Iterator<Item = &str> {
    let submission = &origin.submission;
    [
        Some(submission.verified_player_id.as_str()),
        Some(submission.public_key.as_str()),
        submission.hosted_registration_nonce.as_deref(),
        Some(submission.requester_agent_id.as_str()),
        Some(submission.factory_id.as_str()),
        Some(submission.recipe_id.as_str()),
        Some(origin.consensus_submitter_player_id.as_str()),
        Some(origin.action_payload_hash.as_str()),
        Some(origin.action_root.as_str()),
    ]
    .into_iter()
    .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;
    use oasis7_formal_protocol::controlled_authority::formal::bounded_context::{
        FormalBoundedContextV1, FormalBoundedWorkV1,
    };
    use oasis7_proto::memory_budget::MemoryBudget;
    fn original() -> crate::CommittedRecipeOrigin {
        crate::CommittedRecipeOrigin {
            submission: oasis7_proto::gameplay_submission_origin::GameplaySubmissionOrigin {
                verified_player_id: "player".into(),
                public_key: "ab".repeat(32),
                auth_nonce: 9,
                hosted_registration_nonce: Some("hosted".into()),
                requester_agent_id: "requester".into(),
                factory_id: "factory".into(),
                recipe_id: "recipe".into(),
            },
            consensus_action_id: 7,
            consensus_submitter_player_id: "consensus".into(),
            action_payload_hash: "cd".repeat(32),
            committed_height: 2,
            action_root: "ef".repeat(32),
        }
    }
    fn event() -> crate::WorldEventBody {
        crate::WorldEventBody::Domain(crate::DomainEvent::RecipeStarted {
            emergency_power_obligation: None,
            committed_recipe_origin: None,
            job_id: 1,
            requester_agent_id: "requester".into(),
            factory_id: "factory".into(),
            recipe_id: "recipe".into(),
            accepted_batches: 1,
            consume: vec![],
            produce: vec![],
            byproducts: vec![],
            power_required: 0,
            power_owner_agent_id: Some("requester".into()),
            duration_ticks: 1,
            consume_ledger: oasis7_world_state::MaterialLedgerId::World,
            output_ledger: oasis7_world_state::MaterialLedgerId::World,
            bottleneck_tags: vec![],
            market_quotes: vec![],
            logistics_route_ids: vec![],
            logistics_path_ids: vec![],
            ready_at: 1,
        })
    }
    #[test]
    fn original_correlation_reaches_paid_job_and_modify_clears_it() {
        let origin = original();
        assert!(origin.matches_recipe("requester", "factory", "recipe"));
        let mut body = event();
        let mut seen = 0;
        attach_recipe_origin_with_work_v1(&mut body, Some(&origin), &mut |value| {
            assert_eq!(value, &origin);
            seen += 1;
            Ok::<_, ()>(())
        })
        .unwrap();
        assert_eq!(seen, 1);
        let memory = MemoryBudget::new(1 << 20, 1 << 16).unwrap();
        let work = FormalBoundedWorkV1::new(10000, 1 << 20).unwrap();
        let mut stop = || false;
        let mut ctx = FormalBoundedContextV1::new(&memory, &work, &mut stop);
        let job = super::super::record::recipe_job_from_event_v1(&body, &mut ctx)
            .unwrap()
            .unwrap();
        assert_eq!(job.committed_recipe_origin.as_ref(), Some(&origin));
        // Original closed RecipeJob codec/visitor consumes this same metadata;
        // no receipt qualification is created by this data projection test.
        let wire = oasis7_wasm_abi::encode_canonical_cbor(&*job).unwrap();
        let decoded: oasis7_world_state::RecipeJobState = serde_cbor::from_slice(&wire).unwrap();
        assert_eq!(decoded.committed_recipe_origin, Some(origin));
        drop(job);
        drop(ctx);
        assert_eq!(memory.used(), 0);
        let mut mismatched = original();
        mismatched.submission.recipe_id = "different".into();
        attach_recipe_origin_with_work_v1(&mut body, Some(&mismatched), &mut |_| Ok::<_, ()>(()))
            .unwrap();
        let expected = oasis7_world_state::industry_transition::validate_recipe_started_origin_v1(
            Some(&mismatched),
            "requester",
            "factory",
            "recipe",
        )
        .unwrap_err();
        let mut stop = || false;
        let mut ctx = FormalBoundedContextV1::new(&memory, &work, &mut stop);
        let actual = super::super::record::recipe_job_from_event_v1(&body, &mut ctx)
            .err()
            .expect("restored mismatched correlation must fail replay");
        match &actual {
            crate::world::incremental_body::BodyMutationError::Original(cause) => {
                assert_eq!(format!("{cause}"), format!("{expected:?}"))
            }
            _ => panic!("original replay error category changed: {actual:?}"),
        };
        drop(actual);
        drop(ctx);
        assert_eq!(memory.used(), 0);
        // A real Rule override builds an envelope with no origin, regardless
        // of whether the replacement happens to retain the same recipe IDs.
        attach_recipe_origin_with_work_v1(&mut body, None, &mut |_| {
            Err::<(), _>("None must not clone/charge an origin")
        })
        .unwrap();
        assert!(matches!(
            body,
            crate::WorldEventBody::Domain(crate::DomainEvent::RecipeStarted {
                committed_recipe_origin: None,
                ..
            })
        ));
    }
}
