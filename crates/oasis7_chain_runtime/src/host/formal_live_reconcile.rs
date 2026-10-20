//! Bounded startup repair of a dual-proof-complete publication interrupted
//! between endpoint writes. Only opaque native restores authorize this repair.
use oasis7_distfs::controlled_authority::formal::{
    VerifiedAuthorityV1, VerifiedDurableSetActivationV1, VerifiedFormalCommitV1, VerifiedRecovery,
    business_index::BusinessIndexNodeReader,
    coordinator::FormalDurabilityEndpoint,
    endpoint::{FormalRequestStateV1, VerifiedNativeCheckpointRestoreV1},
    transport::RemoteFormalEndpoint,
};

pub(crate) struct DualNativeRestore {
    pub(crate) primary: (RemoteFormalEndpoint, VerifiedNativeCheckpointRestoreV1),
    pub(crate) replica: (RemoteFormalEndpoint, VerifiedNativeCheckpointRestoreV1),
}
fn same_authority(a: &VerifiedAuthorityV1, b: &VerifiedAuthorityV1) -> bool {
    a.definition() == b.definition()
        && a.policy() == b.policy()
        && a.activation_ref() == b.activation_ref()
}
fn aligned(a: &VerifiedNativeCheckpointRestoreV1, b: &VerifiedNativeCheckpointRestoreV1) -> bool {
    a.commit().head() == b.commit().head()
        && a.candidate() == b.candidate()
        && a.proof() == b.proof()
        && same_authority(a.active_authority(), b.active_authority())
}
fn repair_proof(
    child: &VerifiedNativeCheckpointRestoreV1,
    parent: &VerifiedNativeCheckpointRestoreV1,
    lagging_primary: bool,
) -> Result<(), String> {
    let next = parent
        .commit()
        .record()
        .position
        .checked_add(1)
        .ok_or("native reconciliation position overflow")?;
    let record = child.commit().record();
    if record.position != next
        || record.parent_record_id != parent.commit().head().record_id
        || record.before_state_root != parent.commit().record().after_state_root
        || record.world != parent.commit().record().world
        || !same_authority(child.active_authority(), parent.active_authority())
    {
        return Err("native reconciliation is not an exact immediate child".into());
    }
    let Some(FormalRequestStateV1::Ready {
        candidate,
        decision,
        complete: Some(complete),
        ..
    }) = parent.pending()
    else {
        return Err("native reconciliation lacks durable local proof completion".into());
    };
    let expected = if lagging_primary {
        &child.proof().primary_complete
    } else {
        &child.proof().replica_complete
    };
    if candidate != child.candidate() || decision != &child.proof().decision || complete != expected
    {
        return Err("native reconciliation proof completion mismatch".into());
    }
    Ok(())
}

pub(crate) fn restore_dual_native(
    primary: RemoteFormalEndpoint,
    replica: RemoteFormalEndpoint,
    original: &VerifiedAuthorityV1,
    recovery: Option<&VerifiedRecovery>,
    cut: Option<&VerifiedFormalCommitV1>,
    cache: &dyn BusinessIndexNodeReader,
) -> Result<DualNativeRestore, String> {
    restore_dual_native_using(primary, replica, |endpoint, role| {
        #[cfg(test)]
        let started = std::time::Instant::now();
        #[cfg(test)]
        if std::env::var_os("OASIS7_FORMAL_LOOKUP_TIMING").is_some() {
            eprintln!("formal_native_restore role={role} event=begin");
        }
        let restored = endpoint
            .restore_native_current_cached(original, recovery, cut, cache)
            .map_err(|error| format!("formal {role} native restore: {error:?}"));
        #[cfg(test)]
        if std::env::var_os("OASIS7_FORMAL_LOOKUP_TIMING").is_some() {
            eprintln!(
                "formal_native_restore role={role} event=end elapsed_ms={} success={}",
                started.elapsed().as_millis(),
                restored.is_ok()
            );
        }
        restored
    })
}

pub(crate) fn restore_dual_native_with_durable_set(
    primary: RemoteFormalEndpoint,
    replica: RemoteFormalEndpoint,
    original: &VerifiedAuthorityV1,
    activation: &VerifiedDurableSetActivationV1,
    cache: &dyn BusinessIndexNodeReader,
) -> Result<DualNativeRestore, String> {
    restore_dual_native_using(primary, replica, |endpoint, role| {
        #[cfg(test)]
        let started = std::time::Instant::now();
        #[cfg(test)]
        if std::env::var_os("OASIS7_FORMAL_LOOKUP_TIMING").is_some() {
            eprintln!("formal_native_restore role={role} event=begin mode=healthy_activation");
        }
        let restored = endpoint
            .restore_native_current_with_durable_set(original, activation, Some(cache))
            .map_err(|error| {
                format!("formal {role} healthy durable-set native restore: {error:?}")
            });
        #[cfg(test)]
        if std::env::var_os("OASIS7_FORMAL_LOOKUP_TIMING").is_some() {
            eprintln!(
                "formal_native_restore role={role} event=end elapsed_ms={} success={} mode=healthy_activation",
                started.elapsed().as_millis(),
                restored.is_ok()
            );
        }
        restored
    })
}

fn restore_dual_native_using(
    mut primary: RemoteFormalEndpoint,
    mut replica: RemoteFormalEndpoint,
    mut load: impl FnMut(
        &mut RemoteFormalEndpoint,
        &str,
    ) -> Result<VerifiedNativeCheckpointRestoreV1, String>,
) -> Result<DualNativeRestore, String> {
    let mut p = load(&mut primary, "primary")?;
    let mut r = load(&mut replica, "replica")?;
    if !aligned(&p, &r) {
        let (lagging, child, parent, lagging_primary) =
            if p.commit().record().position > r.commit().record().position {
                (&mut replica, &p, &r, false)
            } else {
                (&mut primary, &r, &p, true)
            };
        repair_proof(child, parent, lagging_primary)?;
        let ancestry_bridge = child.clone();
        let previous_primary = p.clone();
        let previous_replica = r.clone();
        let head = lagging
            .publish_qualified(child.proof().clone())
            .map_err(|error| format!("{error:?}"))?;
        if head != child.commit().head() {
            return Err("native reconciliation published unexpected head".into());
        }
        p = load(&mut primary, "primary")?
            .with_verified_ancestry_bridge(&ancestry_bridge, &previous_primary)
            .map_err(|error| format!("{error:?}"))?;
        r = load(&mut replica, "replica")?
            .with_verified_ancestry_bridge(&ancestry_bridge, &previous_replica)
            .map_err(|error| format!("{error:?}"))?;
        if !aligned(&p, &r) {
            return Err("native reconciliation fresh endpoints disagree".into());
        }
    }
    Ok(DualNativeRestore {
        primary: (primary, p),
        replica: (replica, r),
    })
}
