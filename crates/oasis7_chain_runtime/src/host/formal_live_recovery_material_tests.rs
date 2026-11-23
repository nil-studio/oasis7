//! CA-R2 mutates the actual retained Runtime original, then repairs that file.
use super::*;
use oasis7_distfs::controlled_authority::{
    formal::{coordinator::FormalDurabilityEndpoint, transport::RemoteFormalEndpoint},
    replicated_protocol::EndpointRole,
};

pub(super) fn run(
    phase: &str,
    config: &Path,
    pending: &Path,
    original_block: &str,
) -> Result<(), String> {
    if !matches!(phase, "pending_durable" | "candidate_saved") {
        return Ok(());
    }
    let original = std::fs::read(pending).map_err(|error| error.to_string())?;
    let validated = crate::formal_live_config::FormalLiveConfigV1::load(config)?;
    let pending_original = bridge::formal_writer::acceptance::original_frozen(
        pending.parent().ok_or("CA-R2 writer root missing")?,
        bridge::formal_writer::pending_test_position(pending)?,
    )?;
    let request = pending_original.0.body.request_digest;
    let mut endpoints = [EndpointRole::Primary, EndpointRole::Replica]
        .into_iter()
        .zip([validated.config.primary, validated.config.replica])
        .map(|(role, address)| {
            RemoteFormalEndpoint::new(
                address,
                validated.timeout,
                validated.anchor.authority().clone(),
                role,
                validated.writer_key.clone(),
                validated.anchor.head().head(),
            )
            .map_err(|error| error.to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    let observe = |endpoints: &mut [RemoteFormalEndpoint]| -> Result<Vec<Vec<u8>>, String> {
        endpoints
            .iter_mut()
            .map(|endpoint| {
                let head = endpoint.head().map_err(|error| error.to_string())?;
                let checkpoint = endpoint.checkpoint().map_err(|error| error.to_string())?;
                let original = endpoint
                    .query(&request)
                    .map_err(|error| error.to_string())?;
                serde_cbor::to_vec(&(head, checkpoint, original)).map_err(|error| error.to_string())
            })
            .collect()
    };
    let before = observe(&mut endpoints)?;
    let variants = bridge::formal_writer::acceptance::corrupted_pending(
        pending,
        phase == "candidate_saved",
        &validated.writer_key,
    )?;
    for (label, bytes) in variants {
        bridge::formal_crash_test_boundary::reset_execution_attempts(original_block);
        std::fs::write(pending, bytes).map_err(|error| error.to_string())?;
        let rejected = FormalLiveRuntime::open_with_native_history(config);
        // Repair even when the assertion fails, keeping the real failed
        // scenario inspectable without leaving its original bytes corrupted.
        std::fs::write(pending, &original).map_err(|error| error.to_string())?;
        match rejected {
            Ok(mut accepted) => {
                accepted.stop(Duration::from_secs(3))?;
                return Err(format!(
                    "CA-R2 {label}: corrupted original opened a Runtime"
                ));
            }
            Err(error) if error.trim().is_empty() => {
                return Err(format!("CA-R2 {label}: missing actual refusal diagnostic"));
            }
            Err(_) => {}
        }
        if observe(&mut endpoints)? != before
            || bridge::formal_crash_test_boundary::execution_attempts() != 0
        {
            return Err(format!(
                "CA-R2 {label}: rejected original advanced endpoint/effects"
            ));
        }
        // A successful real open after restoring the same bytes rules out an
        // unrelated unavailable endpoint, occupied lease or invalid config.
        let mut repaired = FormalLiveRuntime::open_with_native_history(config)?;
        if repaired.published.current()?.anchor.head().head() != pending_original.0.body.parent {
            return Err(format!(
                "CA-R2 {label}: repair substituted a different parent"
            ));
        }
        repaired.stop(Duration::from_secs(3))?;
        drop(repaired);
        if observe(&mut endpoints)? != before
            || bridge::formal_crash_test_boundary::execution_attempts() != 0
        {
            return Err(format!(
                "CA-R2 {label}: read-only repair changed the original decision"
            ));
        }
        std::fs::write(pending, &original).map_err(|error| error.to_string())?;
        eprintln!(
            "CA-R2 phase={phase} mutation={label} open=refused execution=0 native_head=unchanged exact_original_repair=open"
        );
    }
    Ok(())
}
