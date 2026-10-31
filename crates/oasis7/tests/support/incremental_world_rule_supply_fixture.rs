use super::*;
pub(super) fn authorization() -> SupplyAuthorizationV1 {
    SupplyAuthorizationV1 {
        id: "inner-auth".into(),
        provider_id: "provider".into(),
        crisis_id: "crisis".into(),
        crisis_evidence: CrisisState {
            crisis_id: "evidence".into(),
            kind: String::new(),
            severity: 0,
            status: CrisisStatus::TimedOut,
            opened_at: 0,
            expires_at: 1,
            resolver_agent_id: None,
            strategy: None,
            success: Some(false),
            impact: 0,
            resolved_at: Some(0),
        },
        material: String::new(),
        beneficiaries: [OLD.into()].into(),
        policy_version: 0,
        starts_at: 0,
        expires_at: 1,
        review_proposal: String::new(),
        remaining_escrow: 0,
        requests: [(OLD.into(), i64::MIN)].into(),
        revision: 0,
        active: false,
    }
}
pub(super) fn batch() -> SupplyBatchV1 {
    SupplyBatchV1 {
        id: "inner-batch".into(),
        authorization_id: "inner-auth".into(),
        frozen_at: 0,
        authorization_revision: 0,
        frozen_escrow: 0,
        allocated: false,
        frozen_lineages: [(
            OLD.into(),
            SupplyLineageV1 {
                target: -7,
                fulfilled: 5,
                reserved: 3,
            },
        )]
        .into(),
        demands: [(OLD.into(), -9)].into(),
        allocation: [(OLD.into(), 0)].into(),
    }
}
