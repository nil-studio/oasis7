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
    WorldState,
    incremental_field_member::*,
    incremental_field_overlay::WorldFieldOverlayContextV1,
    incremental_world_rule_effects::*,
    world_rules::{SupplyBatchV1, WorldRulesStateV1},
};
#[allow(dead_code)]
#[path = "support/incremental_inventory_fixture.rs"]
mod fixture;
use fixture::{Directory, logical};
// This whole-record fixture selects 12 fixed paths, unlike the earlier six/
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
fn batch() -> SupplyBatchV1 {
    SupplyBatchV1 {
        id: "inner-id".into(),
        authorization_id: "auth".into(),
        frozen_at: u64::MAX,
        authorization_revision: 0,
        frozen_escrow: i64::MIN,
        allocated: false,
        frozen_lineages: Default::default(),
        demands: Default::default(),
        allocation: Default::default(),
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
fn creation_outcome(source: &SupplyBatchV1, populate: bool) -> (bool, bool, bool) {
    successors::outcome(
        oasis7_runtime::incremental_world_rule_supply_root::SupplyRootSourceV1::Batch(source),
        populate,
    )
}
#[test]
fn empty_creation_rejects_nonempty_original_collections_and_poisons_prior_prefix() {
    for case in 1..8 {
        let mut source = batch();
        if case & 1 != 0 {
            source.demands.insert("member".into(), 3);
        }
        if case & 2 != 0 {
            source.allocation.insert("request".into(), -7);
        }
        if case & 4 != 0 {
            source.frozen_lineages.insert(
                "frozen".into(),
                oasis7_runtime::world_rules::SupplyLineageV1::default(),
            );
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
    let source = batch();
    let (rejected, lost, sealable) = creation_outcome(&source, true);
    assert!(!rejected);
    assert!(!lost);
    assert!(sealable);
}
