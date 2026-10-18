//! World adapter for the shared formal projection commitment protocol.
//! Audience authorization and committed formatting stay with the World.
pub use oasis7_client_api::formal_projection::{
    MAX_PROJECTION_SCOPES, ProjectionAudienceV1, ProjectionCommitmentsV1, ProjectionInclusionV1,
    projection_leaf,
};

/// Build from the same committed formatter used by the signed read route.
/// Only current owner/delegate audiences get private leaves; missing or
/// revoked authorization never gains a leaf from a local node registry.
pub fn commitments_for_world(
    world: &oasis7_runtime::World,
) -> Result<ProjectionCommitmentsV1, String> {
    let public = ProjectionAudienceV1::public();
    let projection =
        super::projection::WorldServiceProjection::from_formal_committed_world(world, None, "")?;
    let mut leaves = vec![(public.clone(), projection_leaf(&public, &projection)?)];
    let mut audiences = std::collections::BTreeSet::new();
    for agent in world.capability_revocation_state().agent_identities.keys() {
        let Ok(owner) = super::agent_authority::owner_public_key(world, agent) else {
            continue;
        };
        audiences.insert(ProjectionAudienceV1::agent(agent, owner)?);
        if let Some(delegate) = world
            .capability_revocation_state()
            .agent_signer_delegations
            .get(agent)
            && super::agent_authority::validate_agent_signer(
                world,
                agent,
                &delegate.delegate_public_key,
                delegate.generation,
            )
            .is_ok()
        {
            audiences.insert(ProjectionAudienceV1::agent(
                agent,
                delegate.delegate_public_key.clone(),
            )?);
        }
        if audiences.len() + 1 > MAX_PROJECTION_SCOPES {
            return Err("formal projection audience budget".into());
        }
    }
    for audience in audiences {
        let agent = audience
            .scope_id
            .strip_prefix("agent:")
            .expect("private audience");
        let projection = super::projection::WorldServiceProjection::from_formal_committed_world(
            world,
            Some(agent),
            audience
                .reader_public_key
                .as_deref()
                .expect("private reader"),
        )?;
        let leaf = projection_leaf(&audience, &projection)?;
        leaves.push((audience, leaf));
    }
    ProjectionCommitmentsV1::from_leaves(leaves)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn formal_world_projection_binds_permission_header_and_legacy_omits_it() {
        let world = oasis7_runtime::World::new();
        let legacy = super::super::projection::WorldServiceProjection::from_committed_world(
            &world, None, "",
        )
        .unwrap();
        assert!(
            !serde_json::to_value(&legacy)
                .unwrap()
                .as_object()
                .unwrap()
                .contains_key("formal_commit_header")
        );
        let mut formal =
            super::super::projection::WorldServiceProjection::from_formal_committed_world(
                &world, None, "",
            )
            .unwrap();
        let tree = crate::formal_projection::commitments_for_world(&world).unwrap();
        let audience = ProjectionAudienceV1::public();
        let proof = tree.inclusion(&audience).unwrap();
        proof
            .verify(&audience, &formal, &tree.root().unwrap())
            .unwrap();
        formal
            .formal_commit_header
            .as_mut()
            .unwrap()
            .permission_generation += 1;
        assert!(
            proof
                .verify(&audience, &formal, &tree.root().unwrap())
                .is_err()
        );
    }
}
