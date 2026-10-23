//! Explicit local authentication recovery. Business IDs and role keys never rotate.
use super::*;
use crate::identity_client::IdentityProgress;
const MAX_REFRESHES: usize = 64;
#[derive(Serialize, Deserialize)]
struct Candidate {
    previous: String,
    operation: String,
}
pub(super) struct Selection {
    pub active: String,
    pub pending: Option<String>,
    next: usize,
}
pub(super) fn selection(
    store: &OwnerControlStore,
    immutable: &ImmutablePlan,
) -> Result<Selection, String> {
    let mut active = immutable.operations[1].clone();
    let mut seen = immutable
        .operations
        .iter()
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();
    for index in 0..MAX_REFRESHES {
        let slot = format!("fresh-login-{index}");
        let candidate = store.read_original::<Candidate>(SCOPE, &slot, "candidate")?;
        let Some(candidate) = candidate else {
            return Ok(Selection {
                active,
                pending: None,
                next: index,
            });
        };
        if candidate.previous != active
            || !valid_operation(&candidate.operation)
            || !seen.insert(candidate.operation.clone())
        {
            return Err("cold entry fresh login history binding conflict".into());
        }
        if store.read_original::<bool>(SCOPE, &slot, "confirmed")? == Some(true) {
            active = candidate.operation;
        } else if store.read_original::<bool>(SCOPE, &slot, "unusable")? != Some(true) {
            return Ok(Selection {
                active,
                pending: Some(candidate.operation),
                next: index,
            });
        }
    }
    Err(
        "action_required: explicit login recovery history bound reached; preserve original vaults"
            .into(),
    )
}
fn valid_operation(operation: &str) -> bool {
    URL_SAFE_NO_PAD
        .decode(operation)
        .is_ok_and(|b| b.len() == 32 && URL_SAFE_NO_PAD.encode(b) == operation)
}
fn progress(operation: &str, state: &str, next: &str) -> Value {
    json!({"stage":"login_refresh","state":state,"operation_id":operation,"next_action":next,"business_operations_replaced":false})
}
fn known_unusable(error: &str) -> bool {
    error.starts_with("authentication_failed;") || error.starts_with("ceremony_expired;")
}
fn confirm(
    store: &OwnerControlStore,
    slot: &str,
    result: IdentityProgress,
    expected: &str,
) -> Result<Value, String> {
    let candidate: Candidate = store
        .read_original(SCOPE, slot, "candidate")?
        .ok_or("original fresh login candidate missing")?;
    if result.operation_id != candidate.operation {
        return Err("fresh login response differs from original candidate".into());
    }
    if result.state == "confirmed" {
        if result.account_id.as_deref() != Some(expected) {
            return Err(
                "action_required: fresh login account differs from original registration".into(),
            );
        }
        store.put_original(SCOPE, slot, "confirmed", &true)?;
        return Ok(progress(
            &result.operation_id,
            "confirmed",
            "explicit_cold_entry_run",
        ));
    }
    if result.state == "action_required" && result.next_action == "explicit_fresh_login" {
        store.put_original(SCOPE, slot, "unusable", &true)?;
    }
    Ok(progress(
        &result.operation_id,
        &result.state,
        &result.next_action,
    ))
}
pub(super) fn fresh_login(
    store: &OwnerControlStore,
    immutable: &ImmutablePlan,
    password: &[u8],
    operation: &str,
) -> Result<Value, String> {
    if !valid_operation(operation) {
        return Err("fresh login requires explicit canonical 32-byte operation ID".into());
    }
    preflight(&immutable.plan)?;
    execution::verify_keys(store, &immutable.plan, password)?;
    let expected: String = store
        .read_original(SCOPE, PLAN, "registered-account")?
        .ok_or("confirm original registration before explicit fresh login")?;
    let selected = selection(store, immutable)?;
    let client = IdentityClient::new(immutable.plan.entry.identity.clone(), password)?;
    if selected.active == operation {
        return Ok(progress(
            operation,
            "action_required",
            "selected_login_already_current_use_cold_entry_run",
        ));
    }
    if immutable.operations.iter().any(|old| old == operation) {
        return Err("fresh login must not reuse another original operation identity".into());
    }
    let slot = format!("fresh-login-{}", selected.next);
    if let Some(pending) = &selected.pending {
        if pending != operation {
            return Err("action_required: reconcile exact pending fresh login before choosing another operation".into());
        }
    } else {
        // Reconcile the old login before making another authentication original.
        // Network ambiguity keeps the old phase; only known consumed/expired
        // authentication may be replaced by this explicit local command.
        match client.login(&selected.active) {
            Ok(p) if p.state == "confirmed" => {
                if p.account_id.as_deref() != Some(&expected) {
                    return Err("original login account changed".into());
                }
                return Ok(progress(
                    &selected.active,
                    "confirmed",
                    "current_login_valid_use_cold_entry_run",
                ));
            }
            Ok(p) if p.state == "action_required" && p.next_action == "explicit_fresh_login" => (),
            Err(e) if known_unusable(&e) => (),
            _ => {
                return Ok(progress(
                    &selected.active,
                    "unknown",
                    "reconcile_original_login_before_explicit_fresh_login",
                ));
            }
        }
        // Reused historical attempts cannot be repurposed or silently skipped.
        for index in 0..selected.next {
            if store
                .read_original::<Candidate>(SCOPE, &format!("fresh-login-{index}"), "candidate")?
                .is_some_and(|v| v.operation == operation)
            {
                return Err("action_required: fresh login identity already consumed; choose another explicit ID".into());
            }
        }
        store.put_original(
            SCOPE,
            &slot,
            "candidate",
            &Candidate {
                previous: selected.active,
                operation: operation.into(),
            },
        )?;
    }
    match client.login(operation) {
        Ok(result) => confirm(store, &slot, result, &expected),
        Err(e) if known_unusable(&e) => {
            store.put_original(SCOPE, &slot, "unusable", &true)?;
            Ok(progress(
                operation,
                "action_required",
                "explicit_new_fresh_login_operation_required",
            ))
        }
        Err(_) => Ok(progress(
            operation,
            "unknown",
            "explicit_fresh_login_same_operation",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn refresh_history_never_changes_business_operations_or_selects_unknown_account() {
        let (_directory, plan) = super::super::tests::fixture("https://identity.example".into());
        let store =
            OwnerControlStore::create(&plan.orchestration_directory, super::super::tests::PASSWORD)
                .unwrap();
        let immutable = ImmutablePlan {
            plan,
            operations: (0..STAGES.len())
                .map(|_| random_operation().unwrap())
                .collect(),
        };
        let original = immutable.operations.clone();
        let fresh = random_operation().unwrap();
        let slot = "fresh-login-0";
        store
            .put_original(
                SCOPE,
                slot,
                "candidate",
                &Candidate {
                    previous: original[1].clone(),
                    operation: fresh.clone(),
                },
            )
            .unwrap();
        let unresolved = selection(&store, &immutable).unwrap();
        assert_eq!(unresolved.active, original[1]);
        assert_eq!(unresolved.pending, Some(fresh.clone()));
        let result = IdentityProgress {
            stage: "identity_login".into(),
            state: "confirmed".into(),
            operation_id: fresh.clone(),
            account_id: Some("other-account".into()),
            next_action: "world_admission".into(),
        };
        assert!(confirm(&store, slot, result, "original-account").is_err());
        assert!(selection(&store, &immutable).unwrap().pending.is_some());
        let result = IdentityProgress {
            stage: "identity_login".into(),
            state: "confirmed".into(),
            operation_id: fresh.clone(),
            account_id: Some("original-account".into()),
            next_action: "world_admission".into(),
        };
        confirm(&store, slot, result, "original-account").unwrap();
        let confirmed = selection(&store, &immutable).unwrap();
        assert_eq!(confirmed.active, fresh);
        assert_eq!(confirmed.pending, None);
        assert_eq!(immutable.operations, original);
    }
}
