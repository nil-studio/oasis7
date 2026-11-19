//! Independent Cold requests against the same actual accepted C<T history.
//! Faults remove a real T page or reduce an existing allowance; no verifier
//! result, Host source, admission flag or replacement quota is fabricated.
use super::*;
use crate::iws_host_runtime::cold_recovery::{
    HostColdRecoveryContextV1, HostColdReopenInputV1, reopen_host_bounded_v1,
};
use oasis7_formal_protocol::incremental_state::StateNamespaceV1;
use std::path::PathBuf;

pub(super) fn business_page_bounded(
    host: &IwsHostRuntimeV1,
    memory: &MemoryBudget,
    archive: &FormalArchiveReadWorkV1,
    cancelled: &AtomicBool,
) -> Result<[u8; 32], String> {
    let current = host.current.current().ok_or("actual suffix Host missing")?;
    current
        .source
        .with_parent_read_view_bounded(
            memory,
            archive,
            &mut || cancelled.load(Ordering::Acquire),
            |parent| -> Result<_, IwsHostRuntimeErrorV1> {
                archive.charge(1, 64, &mut || cancelled.load(Ordering::Acquire))?;
                let mut page = [0; 32];
                hex::decode_to_slice(
                    &parent
                        .roots()
                        .state_root(StateNamespaceV1::Business)
                        .node_root,
                    &mut page,
                )
                .map_err(|_| FormalError::Invalid("actual suffix Business root codec".into()))?;
                Ok(page)
            },
        )
        .map_err(diagnostic)
}

struct WithheldPage {
    original: PathBuf,
    withheld: PathBuf,
    active: bool,
}
impl WithheldPage {
    fn new(pages: &Path, page: &[u8; 32]) -> Result<Self, String> {
        let original = pages.join("recipes").join(hex::encode(page));
        let withheld = pages.with_extension("withheld-suffix-target-page");
        if !std::fs::metadata(&original).map_err(diagnostic)?.is_file()
            || withheld.try_exists().map_err(diagnostic)?
        {
            return Err("actual suffix page fault path invalid".into());
        }
        std::fs::rename(&original, &withheld).map_err(diagnostic)?;
        Ok(Self {
            original,
            withheld,
            active: true,
        })
    }
    fn restore(mut self) -> Result<(), String> {
        std::fs::rename(&self.withheld, &self.original).map_err(diagnostic)?;
        self.active = false;
        Ok(())
    }
}
impl Drop for WithheldPage {
    fn drop(&mut self) {
        if self.active {
            // Preserve the fixture's original page even if its consumer unwinds.
            let _ = std::fs::rename(&self.withheld, &self.original);
        }
    }
}

#[derive(Clone, Copy)]
enum Refusal {
    MissingTargetPage,
    ExhaustedSuffixRows,
    CancelledColdRequest,
}
impl Refusal {
    fn name(self) -> &'static str {
        match self {
            Self::MissingTargetPage => "missing_target_page",
            Self::ExhaustedSuffixRows => "exhausted_suffix_rows",
            Self::CancelledColdRequest => "cancelled_cold_request",
        }
    }
}

pub(super) fn run_refusals(
    input: HostColdReopenInputV1<'_>,
    mailbox_hints: &BTreeMap<[u8; 32], FactLocationV1>,
    target_page: [u8; 32],
) -> Result<serde_json::Value, String> {
    if input.endpoint.checkpoint.is_none() {
        return Err("suffix refusal cases require the actual earlier checkpoint selection".into());
    }
    let mut receipts = Vec::new();
    for refusal in [
        Refusal::MissingTargetPage,
        Refusal::ExhaustedSuffixRows,
        Refusal::CancelledColdRequest,
    ] {
        // Each is a new external Cold request after all prior live owners have
        // dropped. None is a retry or a continuation of a previous request.
        let memory = MemoryBudget::new(2usize << 30, 32 << 20).map_err(diagnostic)?;
        let receipt = (|| -> Result<_, String> {
            let archive =
                FormalArchiveReadWorkV1::new(1_000_000_000, 16u64 << 30).map_err(diagnostic)?;
            let mut work = ColdFixtureWorkV1::new_bounded(
                &input.endpoint.current_limits,
                &mut archive.bounded_context(&memory, &mut || false),
            )
            .map_err(diagnostic)?;
            if matches!(refusal, Refusal::ExhaustedSuffixRows) {
                work.source.runtime.rows_remaining = 0;
            }
            let withheld = if matches!(refusal, Refusal::MissingTargetPage) {
                Some(WithheldPage::new(input.endpoint.pages_path, &target_page)?)
            } else {
                None
            };
            let compiler = RuntimeSourceCompiler::default();
            let cancelled = AtomicBool::new(matches!(refusal, Refusal::CancelledColdRequest));
            let before = [archive.remaining_steps(), archive.remaining_read_bytes()];
            let result = reopen_host_bounded_v1(
                HostColdReopenInputV1 {
                    endpoint: input.endpoint,
                    authorization_hint_path: input.authorization_hint_path,
                    projection_limits: input.projection_limits,
                    process: input.process,
                    configuration: input.configuration,
                    worker: input.worker,
                    transaction: input.transaction,
                    facts: input.facts,
                },
                &mut work,
                &mut |id| {
                    mailbox_hints
                        .get(id)
                        .copied()
                        .ok_or_else(|| "actual mailbox original location not supplied".into())
                },
                HostColdRecoveryContextV1 {
                    memory: &memory,
                    archive: &archive,
                    cancelled: &cancelled,
                    compiler: &compiler,
                },
            );
            if let Some(withheld) = withheld {
                withheld.restore()?;
            }
            let error = match result {
                Ok(_) => return Err(format!("{} unexpectedly installed a Host", refusal.name())),
                Err(error) => error,
            };
            let error_text = error.to_string();
            let actual = match refusal {
                Refusal::MissingTargetPage => {
                    error_text.contains(&format!("Missing({target_page:?})"))
                }
                Refusal::ExhaustedSuffixRows => {
                    error_text.contains("suffix incremental Runtime History")
                        && error_text.contains("Runtime History candidate retained delta")
                        && work.source.runtime.rows_remaining == 0
                }
                Refusal::CancelledColdRequest => error_text.to_ascii_lowercase().contains("cancel"),
            };
            if !actual {
                return Err(format!(
                    "{} failed at another gate: {error_text}",
                    refusal.name()
                ));
            }
            let after = [archive.remaining_steps(), archive.remaining_read_bytes()];
            if after
                .into_iter()
                .zip(before)
                .any(|(after, before)| after > before)
            {
                return Err("failed suffix attempt replenished its original Common".into());
            }
            Ok(serde_json::json!({
                "case": refusal.name(), "host_created": false, "error": error_text,
                "common_before": before, "common_after": after,
                "suffix_rows_after": work.source.runtime.rows_remaining,
            }))
        })()?;
        if memory.used() != 0 {
            return Err(format!(
                "{} retained {} bytes after all failed owners dropped",
                refusal.name(),
                memory.used()
            ));
        }
        receipts.push(receipt);
    }
    Ok(
        serde_json::json!({"attempts": receipts, "pool_used_after_each_drop": 0, "target_page": hex::encode(target_page)}),
    )
}
