use super::*;

#[test]
fn legacy_organization_mutation_routes_require_continuity_upgrade() {
    use oasis7_agent_api::world_action::{
        OrganizationEstateRightV1 as Right, OrganizationEstateScopeV1 as Scope,
    };

    let state = organization_fixture();
    let scope = Scope {
        object_id: "estate".into(),
        right: Right::ReturnTrust,
    };
    let commands = [
        WorldRuleCommandV1::OpenOrganizationProtection {
            organization_id: "org".into(),
        },
        WorldRuleCommandV1::BeginOrganizationDissolution {
            organization_id: "org".into(),
        },
        WorldRuleCommandV1::AdvanceOrganization {
            case_id: "case".into(),
            expected_revision: 1,
        },
        WorldRuleCommandV1::GrantOrganizationScope {
            case_id: "case".into(),
            expected_revision: 1,
            proposal_key: "grant".into(),
            delegate_id: "recipient".into(),
            scopes: [scope].into_iter().collect(),
            expires_at: 100,
            replace_grant_id: None,
        },
        WorldRuleCommandV1::RevokeOrganizationScope {
            case_id: "case".into(),
            expected_revision: 1,
            grant_id: "grant".into(),
            proposal_key: "revoke".into(),
        },
        WorldRuleCommandV1::RequestOrganizationRecovery {
            case_id: "case".into(),
            expected_revision: 1,
            appeal: true,
        },
        WorldRuleCommandV1::ResolveOrganizationRecovery {
            case_id: "case".into(),
            expected_revision: 1,
            recovery_request_id: "request".into(),
            approved: true,
            proposal_key: "resolve".into(),
        },
        WorldRuleCommandV1::DisposeOrganizationEstate {
            case_id: "case".into(),
            expected_revision: 1,
            grant_id: "grant".into(),
            grant_revision: 1,
            deposit_id: "estate".into(),
        },
    ];

    for (index, command) in commands.into_iter().enumerate() {
        let request = format!("legacy-{index}");
        let error = PreparedWorldRule::prepare(&state, &event("owner", &request, command), 10)
            .expect_err("legacy mutation routes must not create a second policy path");
        assert!(
            format!("{error:?}").contains("organization_continuity_upgrade_required"),
            "legacy request {index} failed for the wrong reason: {error:?}"
        );
    }
}

#[test]
fn exact_legacy_receipt_replay_remains_idempotent() {
    let command = WorldRuleCommandV1::OpenOrganizationProtection {
        organization_id: "org".into(),
    };
    let mut state = organization_fixture();
    let request_id = "legacy-replay";
    let key = request_key("owner", request_id);
    let mut rules = WorldRulesStateV1::default();
    rules.requests.insert(
        key.clone(),
        WorldRuleReceiptV1 {
            request_id: key.clone(),
            actor_id: "owner".into(),
            command: command.clone(),
            committed_at: 1,
            result_id: key,
        },
    );
    state.world_rules = Some(rules);

    assert!(PreparedWorldRule::prepare(&state, &event("owner", request_id, command), 10).is_ok());
}
