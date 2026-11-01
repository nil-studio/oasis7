//! Closed local observation builder for the mounted Node adapter.
//! The Account paid TermWide consumer remains separate; this module performs no I/O.
use crate::formal_read_service::LocalStopPublication;
use oasis7_agent_api::game::{DecimalU64, GameTaskFenceV1};
use oasis7_client_api::world_service::{task_stop_record_control_v2::*, *};
use oasis7_runtime::World;

/// Reads one exact original under the already qualified current Snapshot.
/// No pagination, secondary query, remote storage or Current token construction.
fn observe(
    publication: &LocalStopPublication,
    q: &StopRecordControlRequestV2,
    received_at_ms: u64,
) -> Result<(StopRecordFactProjectionV2, StopControlProjectionV2), String> {
    q.validate()?;
    let world = publication.world();
    let tip = publication.commit().clone();
    let resident = world.resident_guarded_task_history()?;
    let grant = q
        .original
        .original
        .request
        .observation_grant
        .as_ref()
        .ok_or("Owner combined observation grant missing")?;
    if received_at_ms >= grant.historical_read_until_ms() {
        return Err("Stop historical query expired".into());
    }
    tip.validate().map_err(|e| e.to_string())?;
    if tip.world != q.original.expected_world {
        return Err("Stop query World mismatch".into());
    }
    let digest = q.original.original_digest()?;
    let mut fact = StopRecordFactProjectionV2 {
        api_revision: 2,
        original_digest: digest.clone(),
        source: q.source.clone(),
        observed_at_ms: DecimalU64(received_at_ms),
        fact: StopRecordFactV2::Unknown,
    };
    let mut current = StopControlProjectionV2 {
        api_revision: 2,
        original_digest: digest,
        source: q.source.clone(),
        delivered_at_ms: DecimalU64(received_at_ms),
        current: StopControlDeliveryV2::Unavailable,
    };
    let Some(record) = world.guarded_task_stop_v2(&q.original.original)? else {
        return Ok((fact, current));
    };
    // Both heights are Runtime heights in the same qualified Snapshot. Formal
    // CommitRef.position is deliberately not used as a Runtime height.
    if record.committed_height > world.state().time {
        return Err("Stop record is ahead of qualified Runtime Snapshot".into());
    }
    fact.fact = StopRecordFactV2::Known(Box::new(StopRecordWitnessFactV2 {
        record,
        witnessed_in_commit: tip.clone(),
    }));
    fact.validate(q)?;
    current.current = if received_at_ms < grant.control_read_until_ms() {
        exact_current(world, &tip, &resident, q, &fact)?
    } else {
        StopControlDeliveryV2::Drift
    };
    current.validate(q, &fact)?;
    Ok((fact, current))
}
fn exact_current(
    world: &World,
    tip: &CommitRef,
    resident: &oasis7_runtime::service_admission::game_task::ResidentTaskHistory<'_>,
    q: &StopRecordControlRequestV2,
    fact: &StopRecordFactProjectionV2,
) -> Result<StopControlDeliveryV2, String> {
    let p = &q.original.original.request;
    let f = &p.initial_fence;
    let owner =
        oasis7_runtime::service_admission::agent_authority::owner_public_key(world, &f.agent_id);
    let identity = world.current_agent_identity(&f.agent_id);
    if owner.as_ref().ok() != Some(&f.owner_public_key)
        || identity.as_ref().is_none_or(|v| {
            v.owner_binding != f.owner_binding || v.generation != f.owner_generation.0
        })
    {
        return Ok(StopControlDeliveryV2::Drift);
    }
    let control = resident.current_control(&f.agent_id);
    let Ok(Some(control)) = control else {
        return Ok(StopControlDeliveryV2::Drift);
    };
    let Some(delegation) = world
        .capability_revocation_state()
        .agent_signer_delegations
        .get(&f.agent_id)
    else {
        return Ok(StopControlDeliveryV2::Drift);
    };
    let candidate = StopControlDeliveryV2::Exact(Box::new(StopExactControlV2 {
        tip: tip.clone(),
        fence: GameTaskFenceV1::from_control(&control),
        signer_delegation: delegation.clone(),
    }));
    let projection = StopControlProjectionV2 {
        api_revision: 2,
        original_digest: q.original.original_digest()?,
        source: q.source.clone(),
        delivered_at_ms: fact.observed_at_ms,
        current: candidate.clone(),
    };
    // Scope drift returns no replacement target/goal/Executor information.
    Ok(if projection.validate(q, fact).is_ok() {
        candidate
    } else {
        StopControlDeliveryV2::Drift
    })
}
/// A second *actual* complete pin, not the public first tip echoed by a caller.
/// A changed delivery tip invalidates Current only; historical fact remains.
fn before_delivery(
    current: &mut StopControlProjectionV2,
    actual: &CommitRef,
    q: &StopRecordControlRequestV2,
    delivered_at_ms: u64,
) -> Result<(), String> {
    actual.validate().map_err(|e| e.to_string())?;
    if delivered_at_ms < current.delivered_at_ms.0 {
        return Err("Stop delivery clock regressed".into());
    }
    current.delivered_at_ms = DecimalU64(delivered_at_ms);
    let grant = q
        .original
        .original
        .request
        .observation_grant
        .as_ref()
        .ok_or("Owner combined observation grant missing")?;
    if let StopControlDeliveryV2::Exact(v) = &current.current {
        if &v.tip != actual || delivered_at_ms >= grant.control_read_until_ms() {
            current.current = StopControlDeliveryV2::Drift;
        }
    }
    Ok(())
}
/// Prepared observation retains no dispatch permission. Node delivery takes a
/// second actual local cut, while Account freshness/admit remains separate.
pub struct PreparedStopRecordObservation {
    q: StopRecordControlRequestV2,
    fact: StopRecordFactProjectionV2,
    current: StopControlProjectionV2,
}
/// Builds only from the actual qualified local cut and resident history.
pub fn prepare(
    raw: &[u8],
    publication: &LocalStopPublication,
    received_at_ms: u64,
) -> Result<PreparedStopRecordObservation, String> {
    if raw.len() > 64 * 1024 {
        return Err("Stop query exceeds bound".into());
    }
    let q: StopRecordControlRequestV2 = serde_json::from_slice(raw).map_err(|e| e.to_string())?;
    let (fact, current) = observe(publication, &q, received_at_ms)?;
    Ok(PreparedStopRecordObservation { q, fact, current })
}
impl PreparedStopRecordObservation {
    /// Caller supplies a second actual local publication taken after prepare.
    pub fn deliver(
        mut self,
        delivery: &LocalStopPublication,
        configured_node_signing_key: &str,
        delivered_at_ms: u64,
    ) -> Result<StopRecordControlResponseV2, String> {
        before_delivery(
            &mut self.current,
            delivery.commit(),
            &self.q,
            delivered_at_ms,
        )?;
        self.current.validate(&self.q, &self.fact)?;
        let digest = self.q.digest()?;
        let response = StopRecordControlResponseV2 {
            fact: sign_service_response(
                STOP_RECORD_WITNESS_DOMAIN_V2,
                digest.clone(),
                self.fact,
                configured_node_signing_key,
            )?,
            current: sign_service_response(
                STOP_RECORD_CONTROL_CURRENT_DOMAIN_V2,
                digest,
                self.current,
                configured_node_signing_key,
            )?,
        };
        if serde_json::to_vec(&response)
            .map_err(|e| e.to_string())?
            .len()
            > 64 * 1024
        {
            return Err("Stop combined response exceeds bound".into());
        }
        Ok(response)
    }
}
