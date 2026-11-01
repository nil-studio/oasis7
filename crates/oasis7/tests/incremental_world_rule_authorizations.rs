#![cfg(all(unix, not(target_arch = "wasm32")))]
use oasis7_distfs::incremental_page_store::{
    IncrementalPageStore, PageCompression, PageIoBudget, PageStoreError,
};
use oasis7_formal_protocol::{
    incremental_execution::{
        AuthorityBinding, ExecutionParent, ExecutionTransaction, ExecutionWriteV1, RootSetV1,
        TransactionLimits,
    },
    incremental_state::{
        StateDeltaV1, StateNamespaceV1 as Ns, StatePageWriter, StateRootV1, StateWriteV1,
    },
    incremental_value::{ValueReadContext, ValueTreeError, write_value},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use oasis7_runtime::{
    CrisisState, CrisisStatus, WorldState,
    incremental_field_member::*,
    incremental_field_overlay::WorldFieldOverlayContextV1,
    incremental_world_rule_effects::*,
    world_rules::{SupplyAuthorizationV1, WorldRulesStateV1},
};
#[allow(dead_code)]
#[path = "support/incremental_inventory_fixture.rs"]
mod fixture;
use fixture::{Directory, logical};
// This whole-record fixture selects 31 fixed paths, unlike the earlier six/
// ten-path records. Keep finite initial per-operation limits and original bytes.
fn physical() -> PageIoBudget {
    PageIoBudget {
        reads_remaining: 1_000_000,
        ..fixture::physical()
    }
}
#[allow(dead_code)]
#[path = "support/incremental_world_rules_fixture.rs"]
mod rules_fixture;
use rules_fixture::*;
fn authorization() -> SupplyAuthorizationV1 {
    SupplyAuthorizationV1 {
        id: "inner-id".into(),
        provider_id: "provider".into(),
        crisis_id: "outer-crisis".into(),
        crisis_evidence: CrisisState {
            crisis_id: "inner-crisis".into(),
            kind: String::new(),
            severity: u32::MAX,
            status: CrisisStatus::TimedOut,
            opened_at: 4,
            expires_at: 5,
            resolver_agent_id: Some(String::new()),
            strategy: None,
            success: Some(false),
            impact: i64::MAX,
            resolved_at: Some(0),
        },
        material: "material".into(),
        beneficiaries: Default::default(),
        policy_version: u32::MAX,
        starts_at: 1,
        expires_at: 2,
        review_proposal: "review".into(),
        remaining_escrow: i64::MIN,
        requests: Default::default(),
        revision: 3,
        active: false,
    }
}
fn parent(root: &StateRootV1) -> ExecutionParent {
    let mut roots = RootSetV1::empty("rule-authorizations".into(), [3; 32]).unwrap();
    *roots
        .state
        .iter_mut()
        .find(|r| r.namespace == Ns::Authorization)
        .unwrap() = root.clone();
    ExecutionParent {
        commit: [4; 32],
        roots,
        authority: AuthorityBinding {
            token: [5; 32],
            epoch: 7,
        },
    }
}
fn txn_limits() -> TransactionLimits {
    TransactionLimits {
        max_reads: 128,
        max_writes: 64,
        max_facts: 4,
        max_pending_node_bytes: 4 << 20,
    }
}
#[path = "support/incremental_world_rule_supply_create_successors.rs"]
mod successors;
fn creation_outcome(source: &SupplyAuthorizationV1, populate: bool) -> (bool, bool, bool) {
    successors::outcome(
        oasis7_runtime::incremental_world_rule_supply_root::SupplyRootSourceV1::Authorization(
            source,
        ),
        populate,
    )
}
#[test]
fn empty_creation_rejects_nonempty_original_collections_and_poisons_prior_prefix() {
    for case in 0..3 {
        let mut source = authorization();
        if case != 1 {
            source.beneficiaries.insert("member".into());
        }
        if case != 0 {
            source.requests.insert("request".into(), -7);
        }
        let (rejected, lost, sealable) = creation_outcome(&source, false);
        assert!(
            rejected,
            "case {case}: original members lost={lost}, sealable={sealable}"
        );
        assert!(!sealable, "case {case}");
    }
}

#[test]
fn empty_creation_can_add_members_and_update_nonempty_scalar_projection_in_one_transaction() {
    let source = authorization();
    let (rejected, lost, sealable) = creation_outcome(&source, true);
    assert!(!rejected);
    assert!(!lost);
    assert!(sealable);
}
