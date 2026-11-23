//! An already admitted handoff stops only after the original owner freezes the
//! exact complete pending set that the verified successor plan committed.
use super::*;
use std::time::Instant;

pub(super) fn freeze_expected_originals(
    plan: &ProtectedSignerHandoffPlanV1,
    before: &FormalRuntimeOwnerSnapshotV1,
    deadline: Instant,
) -> Result<(), String> {
    if io::sha256(&plan.successor_config_file)? != plan.successor_config_digest {
        return Err(
            "FENCING_UNPROVEN: successor configuration changed before pending freeze".into(),
        );
    }
    let successor = FormalLiveConfigV1::load(&plan.successor_config_file)?;
    if recovery_permission_digest(&successor)?.as_ref() != Some(&plan.recovery_permission_digest) {
        return Err("FENCING_UNPROVEN: successor recovery changed before pending freeze".into());
    }
    let recovery = successor
        .recovery
        .as_ref()
        .ok_or("successor recovery missing")?;
    let Some(recovery_plan) = recovery.recovery().recovery_plan() else {
        if recovery.recovery().body().schema_version >= 3 {
            return Err("FENCING_UNPROVEN: successor pending plan is missing".into());
        }
        return Ok(());
    };
    let pending = formal_live_runtime::control_freeze_pending_handoff(
        &plan.predecessor_config_file,
        &before.instance,
        deadline
            .saturating_duration_since(Instant::now())
            .min(Duration::from_secs(60)),
    )?;
    if pending.root().map_err(|error| error.to_string())? != recovery_plan.body.pending_handoff_root
    {
        return Err(
            "FENCING_UNPROVEN: complete original owner pending set differs from the recovery plan"
                .into(),
        );
    }
    Ok(())
}
