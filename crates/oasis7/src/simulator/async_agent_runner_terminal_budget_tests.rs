//! Final debit identity fences and atomic native rollback, using the real provider Wait fixture.
use super::*;
fn final_fixture() -> (
    AsyncAgentRunner,
    ContinuationProposalV1,
    RuntimeAgentContinuation,
    RuntimeAgentContinuation,
    ContinuationAuthorityContextV1,
) {
    let (mut r, mut p, c, original, _) = fixture();
    r.with_rejected_unprojected_wait_cleanup("agent-1", &p, &c, &original, |_| Ok(()))
        .unwrap();
    p.continuation_proposal_id.push_str(":final");
    p.agent_session_id.push_str(":final");
    p.agent_turn_id.push_str(":final");
    p.decision_request_id.push_str(":final");
    p.origin_turn_id = p.agent_turn_id.clone();
    p.remaining_budget.value = 1;
    p.proposal_digest = p.proposal_digest().unwrap().to_string();
    let old = admitted_projection(&p);
    r.hydrate_runtime_continuation_with_authority("agent-1", p.clone(), &c.authority, old.clone())
        .unwrap();
    let mut terminal = old.clone();
    terminal.remaining_budget.value = 0;
    terminal.status = crate::runtime::ContinuationStatusV1::Completed;
    terminal.terminal_disposition = Some("budget_exhausted".into());
    terminal.logical_tick += 1;
    terminal.refresh_status_digest();
    (r, p, old, terminal, c.authority)
}
#[test]
fn final_budget_cleanup_rolls_back_five_ledgers_then_completes() {
    let (mut r, p, old, terminal, authority) = final_fixture();
    let before = r.rejected_wait_test_ledger_digests();
    let staged = std::cell::RefCell::new(None);
    assert!(
        r.with_completed_final_budget_cleanup_observed(
            "agent-1",
            &p,
            &old,
            terminal.clone(),
            &authority,
            false,
            |hashes| {
                *staged.borrow_mut() = Some(hashes);
                Err("genuine persistence failure".into())
            }
        )
        .is_err()
    );
    assert_ne!(staged.into_inner().unwrap(), before);
    assert_eq!(r.rejected_wait_test_ledger_digests(), before);
    r.with_completed_final_budget_cleanup("agent-1", &p, &old, terminal, &authority, false, || {
        Ok(())
    })
    .unwrap();
    assert!(!r.continuations.contains_key("agent-1"));
    assert!(!r.awaiting_runtime.contains_key("agent-1"));
}
#[test]
fn final_budget_changed_local_identity_or_budget_never_persists() {
    for field in ["continuation", "wake", "sequence", "budget"] {
        let (mut r, p, old, terminal, authority) = final_fixture();
        let local = r.continuations.get_mut("agent-1").unwrap();
        match field {
            "continuation" => local.continuation_id.push_str(":other"),
            "wake" => local.wake_id.push_str(":other"),
            "sequence" => local.wake_seq += 1,
            _ => local.remaining_budget.value += 1,
        }
        let before = r.rejected_wait_test_ledger_digests();
        let persisted = std::cell::Cell::new(false);
        assert!(
            r.with_completed_final_budget_cleanup(
                "agent-1",
                &p,
                &old,
                terminal,
                &authority,
                false,
                || {
                    persisted.set(true);
                    Ok(())
                }
            )
            .is_err(),
            "{field}"
        );
        assert!(!persisted.get());
        assert_eq!(r.rejected_wait_test_ledger_digests(), before);
    }
}
#[test]
fn final_budget_changed_proposal_or_completed_delta_never_persists() {
    for field in ["proposal", "world", "wake", "budget", "origin"] {
        let (mut r, mut p, old, mut terminal, authority) = final_fixture();
        match field {
            "proposal" => {
                p.continuation_proposal_id.push_str(":other");
                p.proposal_digest = p.proposal_digest().unwrap().to_string();
            }
            "world" => terminal.world_id.push_str(":other"),
            "wake" => terminal.wake_id.push_str(":other"),
            "budget" => terminal.remaining_budget.value = 1,
            _ => terminal.origin_turn_id.push_str(":other"),
        }
        terminal.refresh_status_digest();
        let before = r.rejected_wait_test_ledger_digests();
        let persisted = std::cell::Cell::new(false);
        assert!(
            r.with_completed_final_budget_cleanup(
                "agent-1",
                &p,
                &old,
                terminal,
                &authority,
                false,
                || {
                    persisted.set(true);
                    Ok(())
                }
            )
            .is_err(),
            "{field}"
        );
        assert!(!persisted.get());
        assert_eq!(r.rejected_wait_test_ledger_digests(), before);
    }
}
