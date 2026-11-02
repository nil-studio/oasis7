//! Local selection metadata is never a canonical rejection or permission.
use super::*;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ParkedV2 {
    OrdinaryBatchPending,
    WaitingTurn,
    Expired,
    ClockUnavailable,
    PreflightUnavailable,
    InvalidCatalog,
}
pub(super) struct Selection {
    pub selected: Vec<NodeConsensusAction>,
    pub deferred: Vec<NodeConsensusAction>,
    pub parked: BTreeMap<u64, ParkedV2>,
}
pub(super) fn select(
    actions: Vec<NodeConsensusAction>,
    now_ms: i64,
    hook: Option<&dyn NodeExecutionHook>,
) -> Selection {
    let mut ordinary = Vec::new();
    let mut closed = Vec::new();
    let mut parked = BTreeMap::new();
    for action in actions {
        match crate::node_runtime_core::closed_task_deadline(&action.payload_cbor) {
            Ok(Some(deadline)) => closed.push((action, Some(deadline))),
            Ok(None) => ordinary.push(action),
            Err(_) => {
                parked.insert(action.action_id, ParkedV2::InvalidCatalog);
                closed.push((action, None));
            }
        }
    }
    // Never defer an eligible height-bound ordinary input across its height.
    // Pure ordinary batches retain their exact existing ordered bytes.
    if !ordinary.is_empty() {
        for (action, _) in &closed {
            parked
                .entry(action.action_id)
                .or_insert(ParkedV2::OrdinaryBatchPending);
        }
        return Selection {
            selected: ordinary,
            deferred: closed.into_iter().map(|(a, _)| a).collect(),
            parked,
        };
    }
    let mut selected = Vec::new();
    let mut deferred = Vec::new();
    for (action, deadline) in closed {
        let reason=match deadline {
            None=>Some(ParkedV2::InvalidCatalog),
            Some(_) if now_ms<=0=>Some(ParkedV2::ClockUnavailable),
            Some((kind,until)) if (now_ms as u64)>until || (kind==oasis7_client_api::world_service::task_mutation_admission_v2::TaskMutationKindV2::GuardedStop && now_ms as u64==until)=>Some(ParkedV2::Expired),
            Some(_) if !selected.is_empty()=>Some(ParkedV2::WaitingTurn),
            Some(_)=>{
                if hook.is_some_and(|hook| hook.guarded_local_execution() && crate::node_runtime_core::preflight_with_execution_hook(hook,&action.payload_cbor).is_ok()) {None} else {Some(ParkedV2::PreflightUnavailable)}
            }
        };
        if let Some(reason) = reason {
            parked.insert(action.action_id, reason);
            deferred.push(action);
        } else {
            selected.push(action);
        }
    }
    Selection {
        selected,
        deferred,
        parked,
    }
}

#[cfg(test)]
#[path = "node_engine_closed_catalog_v2_tests.rs"]
mod tests;

impl PosNodeEngine {
    // A failed proposal has no canonical effect. Retain exact new-catalog
    // originals and their existing byte reservation; ordinary failures retain
    // their previous release semantics. A successful immediate decision is not
    // requeued by this path.
    pub(super) fn retain_failed_closed_selection(
        &mut self,
        closed: &[NodeConsensusAction],
        total: usize,
    ) {
        let retained = total_action_payload_bytes(closed.iter());
        for action in closed {
            self.pending_consensus_actions
                .insert(action.action_id, action.clone());
            self.closed_v2_parked
                .insert(action.action_id, ParkedV2::PreflightUnavailable);
        }
        release_action_payload_bytes(
            &self.pending_consensus_action_queue_bytes,
            total.saturating_sub(retained),
        );
    }
}
