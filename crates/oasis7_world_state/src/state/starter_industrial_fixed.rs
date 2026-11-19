//! Fixed starter-industrial predicates over the original complete facts.
//! The view is DATA, not qualification. Current callers retain each genuine
//! selected owner and a complete receipt directory while borrowing this view.
use super::*;
use sha2::{Digest, Sha256};
use std::io::{self, Write};
/// Borrowed receipt projection DATA; it never grants receipt or source authority.
pub struct StarterIndustrialReceiptFactsV1<'a> {
    pub factory_id: &'a str,
    pub recipe_id: &'a str,
    pub output_ledger: &'a MaterialLedgerId,
    pub accepted_batches: u32,
    pub produce: &'a [MaterialStack],
}
pub struct StarterIndustrialFactsV1<'a> {
    pub time: u64,
    pub smelter_factory: Option<&'a FactoryState>,
    pub smelter_profile: Option<&'a oasis7_wasm_abi::FactoryProfileV1>,
    pub assembler_profile: Option<&'a oasis7_wasm_abi::FactoryProfileV1>,
    pub smelter_recipe: Option<&'a oasis7_wasm_abi::RecipeProfileV1>,
    pub smelter_site: Option<&'a FactorySiteAuthorityV1>,
    pub smelter_power: Option<&'a FactoryConstructionPowerProfileV1>,
    pub assembler_power: Option<&'a FactoryConstructionPowerProfileV1>,
    pub smelter_output: Option<&'a BTreeMap<String, i64>>,
    pub milestone: Option<&'a StarterIndustrialMilestoneV1>,
}
struct SnapshotWriter<'a, E, F: FnMut(usize) -> Result<(), E>> {
    hash: Sha256,
    work: &'a mut F,
    first: Option<E>,
}
impl<E, F: FnMut(usize) -> Result<(), E>> Write for SnapshotWriter<'_, E, F> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if let Err(error) = (self.work)(bytes.len()) {
            if self.first.is_none() {
                self.first = Some(error);
            }
            return Err(io::Error::other("starter snapshot work"));
        }
        self.hash.update(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn snapshot<T: serde::Serialize, E, F: FnMut(usize) -> Result<(), E>>(
    value: &T,
    work: &mut F,
) -> Result<Option<String>, E> {
    work(64)?;
    let mut writer = SnapshotWriter {
        hash: Sha256::new(),
        work,
        first: None,
    };
    // The original tuple contains only closed derived serializers. Stream exactly
    // the original JSON grammar into the original SHA-256 algorithm.
    let result = serde_json::to_writer(&mut writer, value);
    if let Some(error) = writer.first {
        return Err(error);
    }
    if result.is_err() {
        return Ok(None);
    }
    Ok(Some(hex::encode(writer.hash.finalize())))
}
pub fn starter_industrial_feasibility_from_facts_v1<'a, E>(
    facts: StarterIndustrialFactsV1<'a>,
    receipts: impl Iterator<Item = StarterIndustrialReceiptFactsV1<'a>>,
    work: &mut impl FnMut(usize) -> Result<(), E>,
) -> Result<StarterIndustrialFeasibilityResult, E> {
    // 1201 bytes is the sum of every literal in the fixed result/notes
    // templates below, including branches not simultaneously live. Two string
    // growth buffers plus u64 decimal (20), booleans (5 each), and 64 hex bytes
    // cover the result, intermediate formatting, and snapshot copy at peak.
    work(
        2 * (1201 + 20 + 8 * 5 + 3 * 64)
            + std::mem::size_of::<StarterIndustrialFeasibilityResult>(),
    )?;
    let smelter_factory = facts.smelter_factory;
    let smelter_output_ledger = smelter_factory.map(|factory| &factory.output_ledger);
    let authority_snapshot = snapshot(
        &(
            &facts.time,
            smelter_factory,
            facts.smelter_profile,
            facts.assembler_profile,
            facts.smelter_recipe,
            facts.smelter_site,
            facts.smelter_power,
            facts.assembler_power,
            facts.smelter_output,
            &facts.milestone,
        ),
        work,
    )?
    .unwrap_or_else(|| format!("unavailable-at-{}", facts.time));

    let base = |status: StarterIndustrialFeasibilityStatus,
                evidence_class: &str,
                blocker: Option<String>,
                next_action: &str,
                progression_effect: &str|
     -> StarterIndustrialFeasibilityResult {
        StarterIndustrialFeasibilityResult {
            profile_id: STARTER_INDUSTRIAL_PROFILE_ID.to_string(),
            profile_revision: STARTER_INDUSTRIAL_PROFILE_REVISION,
            authority_snapshot: authority_snapshot.clone(),
            status,
            evidence_class: evidence_class.to_string(),
            completion_boundary: STARTER_INDUSTRIAL_COMPLETION_BOUNDARY.to_string(),
            blocker,
            next_action: next_action.to_string(),
            next_recheck: Some(facts.time.saturating_add(1)),
            progression_effect: progression_effect.to_string(),
        }
    };

    let milestone_matches = facts.milestone.is_some_and(|milestone| {
        milestone.profile_id == STARTER_INDUSTRIAL_PROFILE_ID
            && milestone.profile_revision == STARTER_INDUSTRIAL_PROFILE_REVISION
            && milestone.factory_id == STARTER_SMELTER_FACTORY_ID
            && milestone.recipe_id == STARTER_SMELTER_RECIPE_ID
    });
    if milestone_matches {
        let assembler_profile_ready = facts.assembler_profile.is_some_and(|profile| {
            profile.factory_id == STARTER_ASSEMBLER_FACTORY_ID
                && profile.tags.iter().any(|tag| tag == "assembler")
        });
        let assembler_power_profile_ready = facts.assembler_power.is_some_and(|profile| {
            profile.factory_id == STARTER_ASSEMBLER_FACTORY_ID
                && profile.active
                && profile.authority_revision > 0
        });
        if !assembler_profile_ready || !assembler_power_profile_ready {
            return Ok(base(
                StarterIndustrialFeasibilityStatus::NoSafeStarterChain,
                "unknown/not_tracked",
                Some(format!(
                    "starter assembler authority is incomplete after milestone settlement: assembler_profile={assembler_profile_ready} assembler_power_profile={assembler_power_profile_ready}"
                )),
                "refresh_starter_assembler_authority",
                "none",
            ));
        }
        return Ok(base(
            StarterIndustrialFeasibilityStatus::CandidateAvailable,
            "durable-milestone-backed",
            None,
            "build_factory_assembler_mk1",
            "open_assembler_candidate_only",
        ));
    }

    let Some(smelter_factory) = smelter_factory else {
        return Ok(base(
            StarterIndustrialFeasibilityStatus::NoSafeStarterChain,
            "unknown/not_tracked",
            Some(format!(
                "starter industrial profile requires {STARTER_SMELTER_FACTORY_ID} before building {STARTER_ASSEMBLER_FACTORY_ID}"
            )),
            "build_factory_smelter_mk1",
            "none",
        ));
    };

    let smelter_profile_ready = facts.smelter_profile.is_some_and(|profile| {
        profile.factory_id == STARTER_SMELTER_FACTORY_ID
            && profile.tags.iter().any(|tag| tag == "smelter")
    });
    let assembler_profile_ready = facts.assembler_profile.is_some_and(|profile| {
        profile.factory_id == STARTER_ASSEMBLER_FACTORY_ID
            && profile.tags.iter().any(|tag| tag == "assembler")
    });
    let site_authority_ready = facts.smelter_site.is_some_and(|authority| {
        authority.site_id == smelter_factory.site_id
            && authority.active
            && authority.chunk_ready
            && authority.authority_revision > 0
            && !authority.location_id.trim().is_empty()
    });
    let smelter_power_profile_ready = facts.smelter_power.is_some_and(|profile| {
        profile.factory_id == STARTER_SMELTER_FACTORY_ID
            && profile.active
            && profile.authority_revision > 0
    });
    let assembler_power_profile_ready = facts.assembler_power.is_some_and(|profile| {
        profile.factory_id == STARTER_ASSEMBLER_FACTORY_ID
            && profile.active
            && profile.authority_revision > 0
    });
    if !smelter_profile_ready
        || !assembler_profile_ready
        || !site_authority_ready
        || !smelter_power_profile_ready
        || !assembler_power_profile_ready
    {
        return Ok(base(
            StarterIndustrialFeasibilityStatus::NoSafeStarterChain,
            "unknown/not_tracked",
            Some(format!(
                "starter industrial profile authority is incomplete: smelter_profile={smelter_profile_ready} assembler_profile={assembler_profile_ready} site_authority={site_authority_ready} smelter_power_profile={smelter_power_profile_ready} assembler_power_profile={assembler_power_profile_ready}"
            )),
            "refresh_starter_industrial_authority",
            "none",
        ));
    }

    let mut receipt_matches = false;
    for receipt in receipts {
        work(receipt.factory_id.len())?;
        work(receipt.recipe_id.len())?;
        match receipt.output_ledger {
            MaterialLedgerId::World => {}
            MaterialLedgerId::Agent(v)
            | MaterialLedgerId::Site(v)
            | MaterialLedgerId::Factory(v) => work(v.len())?,
        }
        for stack in receipt.produce {
            work(stack.kind.len())?;
        }
        if receipt.factory_id == STARTER_SMELTER_FACTORY_ID
            && receipt.recipe_id == STARTER_SMELTER_RECIPE_ID
            && smelter_output_ledger == Some(receipt.output_ledger)
            && receipt.accepted_batches > 0
            && receipt
                .produce
                .iter()
                .any(|stack| stack.kind == "iron_ingot" && stack.amount > 0)
        {
            receipt_matches = true;
            break;
        }
    }

    let legacy_snapshot_matches = smelter_factory
        .production
        .last_completed_recipe_id
        .as_deref()
        == Some(STARTER_SMELTER_RECIPE_ID)
        && smelter_factory.production.completed_jobs > 0
        && smelter_factory
            .production
            .last_completed_canonical_snapshot
            .as_ref()
            .is_some_and(|snapshot| {
                snapshot.recipe_id == STARTER_SMELTER_RECIPE_ID
                    && smelter_output_ledger == Some(&snapshot.output_ledger)
                    && snapshot
                        .produce
                        .iter()
                        .any(|stack| stack.kind == "iron_ingot" && stack.amount > 0)
            });
    if !receipt_matches && !legacy_snapshot_matches {
        return Ok(base(
            StarterIndustrialFeasibilityStatus::NoSafeStarterChain,
            "unknown/not_tracked",
            Some(format!(
                "starter assembler requires a completed starter Smelter production receipt: complete {STARTER_SMELTER_RECIPE_ID} on {STARTER_SMELTER_FACTORY_ID} before building {STARTER_ASSEMBLER_FACTORY_ID}"
            )),
            "schedule_recipe_smelter_iron_ingot_and_wait_for_settlement",
            "none",
        ));
    }

    Ok(base(
        StarterIndustrialFeasibilityStatus::CandidateAvailable,
        "current-evidence-backed",
        None,
        "build_factory_assembler_mk1",
        "open_assembler_candidate_only",
    ))
}
