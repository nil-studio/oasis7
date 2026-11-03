//! Actual Source alternatives and independently replayed C0 purpose matrix.
use super::*;
thread_local! { static ENABLED: Cell<bool> = const { Cell::new(false) }; }
pub(super) fn requested() -> bool {
    ENABLED.with(Cell::get)
}
struct Reset;
impl Drop for Reset {
    fn drop(&mut self) {
        ENABLED.with(|enabled| enabled.set(false));
    }
}
#[cfg(feature = "node-libp2p")]
#[test]
fn actual_source_carry_selected_local_and_unselected_received_have_distinct_purposes() {
    thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(|| {
            ENABLED.with(|enabled| assert!(!enabled.replace(true)));
            let _reset = Reset;
            install_case_with_retention(
                false,
                false,
                66,
                CandidateRetentionProfile::PublicationReserved,
            );
        })
        .unwrap()
        .join()
        .unwrap();
}

pub(super) fn exercise(
    probe: &Probe,
    driver: &mut FormalBftHeaderNodeDriverV3,
    binding: &BftSignerBindingV3,
    lease: &mut dyn BftSignerLeaseV3,
    context: &NodeBftExecutionContextV3,
    candidate: &NodeBftPrivateCandidateV3,
    deadline: Instant,
) -> Result<Summary, String> {
    let public = lease.checkpoint().map_err(|e| e.to_string())?.state;
    if context.height != probe.target
        || public.history.last().is_none_or(|certificate| {
            certificate.execution_commitment.height.checked_add(1) != Some(context.height)
        })
    {
        return Err("purpose matrix requires actual first QC prefix".into());
    }
    let signatures = probe.signatures.load(Ordering::SeqCst);
    if signatures == 0 {
        return Err("purpose matrix prefix was never genuinely signed".into());
    }
    let selected = driver.preview_received_package_for_test(context, &candidate.actions_cbor)?;
    let original_actions: Vec<oasis7_node::NodeConsensusAction> =
        serde_cbor::from_slice(&candidate.actions_cbor).map_err(|e| e.to_string())?;
    // The target may legitimately have an empty original candidate. Keep A
    // exact and execute a different legal action list at the same real parent.
    let alternative_actions = if original_actions.is_empty() {
        vec![
            oasis7_node::NodeConsensusAction::from_payload(
                34978,
                "purpose-matrix-player",
                payload("purpose-matrix-unselected-received-B"),
            )
            .map_err(|e| e.to_string())?,
        ]
    } else {
        Vec::new()
    };
    let alternative_bytes = serde_cbor::to_vec(&alternative_actions).map_err(|e| e.to_string())?;
    let alternative = driver.preview_received_package_for_test(context, &alternative_bytes)?;
    if selected.candidate != *candidate
        || alternative.candidate.public_value.block_id()? == candidate.public_value.block_id()?
    {
        return Err("purpose matrix needs distinct actual candidates at one parent".into());
    }
    let receive = |driver: &mut FormalBftHeaderNodeDriverV3,
                   lease: &mut dyn BftSignerLeaseV3,
                   package: &oasis7_node::NodeBftPrivateCandidatePackageV3|
     -> Result<(), String> {
        match driver.retain_received_bft_v3_protected_candidate(
            binding, lease, context, package, deadline,
        )? {
            oasis7_node::NodeBftProtectedCandidateAdmissionV3::Retained(_) => Ok(()),
            other => Err(format!(
                "actual purpose matrix Received retention rejected: {other:?}"
            )),
        }
    };
    receive(driver, lease, &selected)?;
    let ready = probe
        .backend
        .clone()
        .load_private_retention_current(&probe.binding)
        .map_err(|e| e.to_string())?;
    driver.retain_bft_v3_protected_candidate(binding, lease, context, candidate, deadline)?;
    let chosen = probe
        .backend
        .clone()
        .load_private_retention_current(&probe.binding)
        .map_err(|e| e.to_string())?;
    if ready.retention.as_ref().unwrap().map != chosen.retention.as_ref().unwrap().map {
        return Err("first choice changed original map/facts".into());
    }
    for (id, input) in &ready.inputs {
        if chosen
            .inputs
            .get(id)
            .is_none_or(|after| !Arc::ptr_eq(input, after))
        {
            return Err("first choice replaced original input allocation".into());
        }
    }
    receive(driver, lease, &alternative)?;
    let after = probe
        .backend
        .clone()
        .load_private_retention_current(&probe.binding)
        .map_err(|e| e.to_string())?;
    let ledger = after.retention.as_ref().unwrap();
    if ledger.selection_index != chosen.retention.as_ref().unwrap().selection_index
        || driver.pending_bft_v3_private_candidate(context)?.as_ref() != Some(candidate)
        || probe.signatures.load(Ordering::SeqCst) != signatures
    {
        return Err("alternative changed selected original or minted validator signature".into());
    }
    let target: Vec<_> = ledger
        .map
        .entries
        .iter()
        .filter(|entry| entry.key.height == context.height)
        .collect();
    if target.len() != 2
        || target
            .iter()
            .any(|entry| entry.lifecycle != BftPrivateRetentionLifecycleV1::Ready)
    {
        return Err("purpose matrix requires both distinct actual Ready originals".into());
    }
    let header = candidate.public_value.block_id()?;
    let entry = target
        .into_iter()
        .find(|entry| entry.key.header_id == header)
        .unwrap()
        .clone();
    let reference = BftPrivateRetentionAdmissionInputRefV1::from_entry(&ledger.map.source, &entry)
        .map_err(|e| e.to_string())?;
    let input = after
        .inputs
        .get(&reference.admission_digest)
        .unwrap()
        .clone();
    let selection_bytes = fs::read(
        probe
            .private
            .join(format!("selection-{:020}.cbor", context.height)),
    )
    .map_err(|e| e.to_string())?;
    Ok(Summary {
        entry,
        reference,
        input,
        state: after.signer.state,
        originals: ledger.map.entries.clone(),
        selection: BftSuccessorWindowArchiveRefV1 {
            digest: blake3_hex(&selection_bytes),
            bytes: selection_bytes.len() as u64,
        },
        validator_signatures: signatures,
    })
}
