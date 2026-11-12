//! Private accepted-parent canonical cache owner. It authenticates projection
//! DATA only: no business admission, physical closure or WorldReady permission.
use super::read::{AuthorizationRootSourceReadWorkV1, SourceError};
use super::*;
use oasis7_formal_protocol::{
    controlled_authority::formal::VerifiedFormalCommitV1,
    incremental_execution::{ExecutionFactKindV1, ExecutionFence, PreparedGuard, RootSetV1},
    incremental_state::MerkleNodeReader,
    incremental_value::ValueNodeReader,
};
use oasis7_world_state::{
    incremental_field_overlay::WorldFieldOverlayContextV1,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_runtime_history_source::AcceptedRuntimeHistorySourceV1,
    incremental_runtime_revocation::RuntimeRevocationCurrentV1,
    incremental_runtime_typed_complete::RuntimeTypedCompleteQualificationV1,
};
pub(in crate::world) struct AcceptedAuthorizationProjectionV1 {
    data: ProvisionalAuthorizationProjectionV1,
    head: [u8; 32],
    body: [u8; 32],
    roots: [u8; 32],
    revocation_body: ReservedValue<oasis7_formal_protocol::incremental_state::StateRootV1>,
    expected: [u8; 64],
    fee: ReservedValue<()>,
}
#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn qualify_accepted_parent<M, R, E, C, V, ER, CR>(
    data: ProvisionalAuthorizationProjectionV1,
    head: &VerifiedFormalCommitV1,
    history: &ReservedValue<AcceptedRuntimeHistorySourceV1>,
    qualified: &ReservedValue<RuntimeTypedCompleteQualificationV1>,
    revocation: &RuntimeRevocationCurrentV1,
    roots: &RootSetV1,
    overlay: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, M, V, ER, CR>,
    work: &mut AuthorizationRootSourceReadWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<AcceptedAuthorizationProjectionV1, SourceError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    V: ValueNodeReader,
    ER: FnMut() -> Option<ValueTreeError>,
    CR: FnMut() -> bool,
{
    let result = (|| {
        common.consume_steps(1)?;
        let tx = &*overlay.transaction;
        let fact = history.occurrence();
        let fence = tx.parent_fence();
        if !history.uses_budget(common.memory())
            || history.allocation_class() != MemoryClass::Ordinary
            || !tx.uses_budget(common.memory())
            || fact.kind() != ExecutionFactKindV1::Result
            || fact.head_record_id() != head.verified_record_id()
            || fact.head_body_id() != fact.fact_body_id()
            || fact.head_body_id() != fence.parent_commit
            || fact.roots_id() != fence.parent_roots
            || history.manifest().roots != fence.parent_roots
        {
            return Err(invalid("authorization accepted parent provenance").into());
        }
        let record = fact.head_record_id();
        let body = fence.parent_commit;
        let roots_id = fence.parent_roots;
        read::metadata(roots, common)?;
        if roots.digest().map_err(SourceError::Execution)? != roots_id {
            return Err(invalid("authorization exact parent RootSet").into());
        }
        qualify::compare_seven_current(
            &data, qualified, revocation, roots, overlay, parent, work, common, live,
        )?;
        // The original committed root is exactly a 64-byte lowercase SHA256 text.
        // A hint content address is not accepted as the expected source root.
        let mut original = [0u8; 64];
        let length = read::paid(roots, overlay, parent, work, common, |_, parent, _| {
            qualified.authorization_root().validate_binding(roots)?;
            oasis7_world_state::incremental_runtime_authorization_root::read_authorization_root_value_v1(parent,64,&mut &mut original[..]).map_err(SourceError::from)
        })?;
        if length != 64
            || !original
                .iter()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(b))
        {
            return Err(invalid("authorization original SHA256 text").into());
        }
        let actual = data.hash(common)?;
        if actual.as_bytes() != original {
            return Err(invalid("authorization original root mismatch").into());
        }
        let guard = overlay.transaction.guard();
        overlay
            .transaction
            .validate_guard(&guard, live)
            .map_err(SourceError::Execution)?;
        let revocation_body = paid_body(revocation.body(), common)?;
        let fee = common
            .memory()
            .reserve(
                std::mem::size_of::<AcceptedAuthorizationProjectionV1>(),
                MemoryClass::Ordinary,
            )?
            .retain(());
        common.consume_steps(0)?;
        Ok(AcceptedAuthorizationProjectionV1 {
            data,
            head: record,
            body,
            roots: roots_id,
            revocation_body,
            expected: original,
            fee,
        })
    })();
    if result.is_err() {
        let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
            overlay.transaction.contribute(|_| {
                Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
            });
    }
    result
}

/// A sole mutable transaction-local DATA stage. It borrows the retained
/// accepted parent; callers cannot replace its parent or inject another root.
pub(in crate::world) struct CurrentAuthorizationProjectionV1<'a> {
    parent: &'a AcceptedAuthorizationProjectionV1,
    revocation_body: ReservedValue<oasis7_formal_protocol::incremental_state::StateRootV1>,
    expected: [u8; 64],
    data: ProvisionalAuthorizationProjectionV1,
    registered: Option<tree::TreeRef>,
    stage_identity: std::sync::Arc<ReservedValue<()>>,
    identity: PreparedGuard,
    core: oasis7_formal_protocol::controlled_authority::formal::bounded_context::WorkIdentityV1,
    fee: ReservedValue<()>,
}
impl AcceptedAuthorizationProjectionV1 {
    pub(in crate::world) fn begin_current<'a, M: MerkleNodeReader>(
        &'a self,
        head: &VerifiedFormalCommitV1,
        tx: &oasis7_formal_protocol::incremental_execution::ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<CurrentAuthorizationProjectionV1<'a>, SourceError> {
        common.consume_steps(1)?;
        self.data.check_pool(common)?;
        let fence = tx.parent_fence();
        if self.head != head.verified_record_id()
            || self.body != fence.parent_commit
            || self.roots != fence.parent_roots
            || !self.fee.uses_budget(common.memory())
            || !tx.uses_budget(common.memory())
        {
            return Err(invalid("authorization retained accepted parent binding").into());
        }
        let identity = tx.guard();
        tx.validate_guard(&identity, live)
            .map_err(SourceError::Execution)?;
        let fee = common
            .memory()
            .reserve(
                std::mem::size_of::<CurrentAuthorizationProjectionV1<'_>>(),
                MemoryClass::Ordinary,
            )?
            .retain(());
        // The paid persistent graph stays shared. Only changed AVL paths will
        // allocate; there is no whole seven-map clone or cold requalification.
        let identity_layout = std::mem::size_of::<ReservedValue<()>>()
            .checked_add(2 * std::mem::size_of::<usize>())
            .and_then(|n| n.checked_add(4 * std::mem::align_of::<ReservedValue<()>>()))
            .ok_or_else(limited)?;
        let stage_identity = std::sync::Arc::new(
            common
                .memory()
                .reserve(identity_layout, MemoryClass::Ordinary)?
                .retain(()),
        );
        let revocation_body = paid_body(&self.revocation_body, common)?;
        let data = ProvisionalAuthorizationProjectionV1 {
            root: self.data.root.clone(),
            class: self.data.class,
            limits: self.data.limits,
        };
        Ok(CurrentAuthorizationProjectionV1 {
            parent: self,
            data,
            revocation_body,
            expected: self.expected,
            registered: None,
            stage_identity,
            identity,
            core: common.work().identity(),
            fee,
        })
    }
}
impl CurrentAuthorizationProjectionV1<'_> {
    fn validate<M: MerkleNodeReader>(
        &self,
        tx: &oasis7_formal_protocol::incremental_execution::ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), SourceError> {
        common.consume_steps(1)?;
        tx.validate_transaction_identity(&self.identity)
            .map_err(SourceError::Execution)?;
        if self.core != common.work().identity()
            || !self.fee.uses_budget(common.memory())
            || self.parent.body != tx.parent_fence().parent_commit
            || self.parent.roots != tx.parent_fence().parent_roots
        {
            return Err(invalid("authorization Current stage identity").into());
        }
        self.data.check_pool(common)?;
        Ok(())
    }
}

#[path = "incremental_authorization_root_current_warm.rs"]
mod warm;

#[path = "incremental_authorization_root_current_refresh.rs"]
mod refresh;
pub(in crate::world) use refresh::AuthorizationProjectionRefreshV1;

fn paid_body(
    body: &oasis7_formal_protocol::incremental_state::StateRootV1,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<ReservedValue<oasis7_formal_protocol::incremental_state::StateRootV1>, SourceError> {
    if body.node_root.len() > 64 {
        return Err(invalid("authorization tracked body digest bound").into());
    }
    common.charge(1, body.node_root.len() as u64)?;
    let bytes = std::mem::size_of::<oasis7_formal_protocol::incremental_state::StateRootV1>()
        .checked_add(body.node_root.len())
        .ok_or_else(limited)?;
    let fee = common.memory().reserve(bytes, MemoryClass::Ordinary)?;
    let out = body.clone();
    if out.node_root.capacity() > body.node_root.len() {
        return Err(limited().into());
    }
    Ok(fee.retain(out))
}

pub(in crate::world) use warm::{OwnedProjectedOutcomeV1, WarmError};

#[path = "incremental_authorization_root_current_writes.rs"]
mod writes;

#[path = "incremental_authorization_root_current_publish.rs"]
mod publish;

/// A unique borrow prevents any further stage refresh until Root's fixed child
/// publisher either commits the real Text CAS or aborts this transaction.
pub(in crate::world) struct FinalAuthorizationProjectionV1<'stage, 'parent> {
    stage: &'stage mut CurrentAuthorizationProjectionV1<'parent>,
    outcome: OwnedProjectedOutcomeV1,
    expected: [u8; 64],
    after: [u8; 64],
    guard: PreparedGuard,
    core: oasis7_formal_protocol::controlled_authority::formal::bounded_context::WorkIdentityV1,
    fee: ReservedValue<()>,
}
impl<'parent> CurrentAuthorizationProjectionV1<'parent> {
    pub(in crate::world) fn finish_for_publication<'stage, M: MerkleNodeReader>(
        &'stage mut self,
        outcome: OwnedProjectedOutcomeV1,
        current: &RuntimeRevocationCurrentV1,
        tx: &mut oasis7_formal_protocol::incremental_execution::ExecutionTransaction<'_, M>,
        remaining: &mut usize,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<FinalAuthorizationProjectionV1<'stage, 'parent>, WarmError> {
        let result = (|| {
            self.validate(tx, common)?;
            outcome.validate(tx, common, live)?;
            if !std::sync::Arc::ptr_eq(&self.stage_identity, &outcome.stage_identity)
                || &*self.revocation_body != current.body()
            {
                return Err(WarmError::from(invalid(
                    "authorization final stage/authority binding",
                )));
            }
            self.validate_registered_writes(tx, remaining, common)?;
            let hash = self.data.hash(common)?;
            let mut after = [0u8; 64];
            after.copy_from_slice(hash.as_bytes());
            let fee = common
                .memory()
                .reserve(
                    std::mem::size_of::<FinalAuthorizationProjectionV1<'_, '_>>(),
                    MemoryClass::Ordinary,
                )
                .map_err(SourceError::from)?
                .retain(());
            let guard = tx.guard();
            tx.validate_guard(&guard, live)
                .map_err(SourceError::Execution)?;
            common.consume_steps(0)?;
            Ok((outcome, after, guard, fee))
        })();
        let (outcome, after, guard, fee) = match result {
            Ok(v) => v,
            Err(error) => {
                let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
                    tx.contribute(|_| {
                        Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
                    });
                return Err(error);
            }
        };
        Ok(FinalAuthorizationProjectionV1 {
            expected: self.expected,
            stage: self,
            outcome,
            after,
            guard,
            core: common.work().identity(),
            fee,
        })
    }
}

pub(in crate::world) use publish::RootPublishedOutcomeV1;
