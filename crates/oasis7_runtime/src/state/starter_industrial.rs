use super::super::events::{IndustryStage, MaterialMarketQuote};
use super::super::util::hash_json;
use super::*;

/// Lightweight observability state for industry progression and market
/// snapshots. The durable starter milestone lives alongside the progression
/// counters, while its receipt/history backing remains bounded independently.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct IndustryProgressState {
    #[serde(default)]
    pub stage: IndustryStage,
    #[serde(default)]
    pub stage_updated_at: WorldTime,
    #[serde(default)]
    pub completed_recipe_jobs: u64,
    #[serde(default)]
    pub completed_material_transits: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub starter_industrial_milestone: Option<StarterIndustrialMilestoneV1>,
    #[serde(default)]
    pub latest_market_quotes: BTreeMap<String, MaterialMarketQuote>,
}

/// Canonical product profile for the first industrial capability transition.
/// The profile identity/revision is shared by runtime and Viewer readiness;
/// it is not a second progression tree or a persisted task type.
pub const STARTER_INDUSTRIAL_PROFILE_ID: &str = "starter-industrial-smelter-to-assembler-v1";
pub const STARTER_INDUSTRIAL_PROFILE_REVISION: u64 = 1;
pub const STARTER_SMELTER_FACTORY_ID: &str = "factory.smelter.mk1";
pub const STARTER_SMELTER_RECIPE_ID: &str = "recipe.smelter.iron_ingot";
pub const STARTER_ASSEMBLER_FACTORY_ID: &str = "factory.assembler.mk1";
pub const STARTER_INDUSTRIAL_COMPLETION_BOUNDARY: &str =
    "starter Smelter first settled iron_ingot production";

pub use oasis7_agent_api::world_industry::StarterIndustrialSettlementSummaryV1;

/// Durable identity of the first starter-chain production settlement. The
/// profile and output-ledger bindings prevent a later recipe or a replacement
/// profile from inheriting this milestone accidentally.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StarterIndustrialMilestoneV1 {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settlement_summary: Option<StarterIndustrialSettlementSummaryV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub committed_recipe_origin: Option<crate::CommittedRecipeOrigin>,
    pub profile_id: String,
    pub profile_revision: u64,
    pub factory_id: String,
    pub recipe_id: String,
    pub output_ledger: MaterialLedgerId,
    pub settlement_job_id: ActionId,
    pub settled_at: WorldTime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StarterIndustrialFeasibilityStatus {
    CandidateAvailable,
    NoSafeStarterChain,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StarterIndustrialFeasibilityResult {
    pub profile_id: String,
    pub profile_revision: u64,
    /// Digest of the authority facts read for this evaluation. It is
    /// recomputed on every call so callers cannot carry a stale result across
    /// a world change.
    pub authority_snapshot: String,
    pub status: StarterIndustrialFeasibilityStatus,
    pub evidence_class: String,
    pub completion_boundary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocker: Option<String>,
    pub next_action: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_recheck: Option<WorldTime>,
    pub progression_effect: String,
}

impl StarterIndustrialFeasibilityResult {
    pub fn candidate_available(&self) -> bool {
        self.status == StarterIndustrialFeasibilityStatus::CandidateAvailable
    }

    /// Translate the canonical result into the action-facing reason used by
    /// both runtime admission and the player gameplay snapshot.
    pub fn disabled_reason(&self) -> Option<String> {
        self.blocker.as_ref().map(|blocker| {
            format!(
                "{blocker} [profile={} revision={} authority_snapshot={} completion_boundary={} evidence={} progression_effect={} next_action={} next_recheck={}]",
                self.profile_id,
                self.profile_revision,
                self.authority_snapshot,
                self.completion_boundary,
                self.evidence_class,
                self.progression_effect,
                self.next_action,
                self.next_recheck
                    .map_or_else(|| "on_authority_change".to_string(), |tick| tick.to_string()),
            )
        })
    }
}

impl WorldState {
    /// Evaluate the canonical starter industrial profile from one fresh set
    /// of world-owned authority facts. This is intentionally a pure read: the
    /// result does not reserve resources, enqueue work, or mutate the durable
    /// milestone. Runtime admission and Viewer readiness both call this
    /// method so bounded receipt history cannot become a second progression
    /// authority.
    pub fn starter_industrial_feasibility(&self) -> StarterIndustrialFeasibilityResult {
        let smelter_factory = self.factories.get(STARTER_SMELTER_FACTORY_ID);
        let smelter_output_ledger = smelter_factory.map(|factory| factory.output_ledger.clone());
        let authority_snapshot = hash_json(&(
            &self.time,
            smelter_factory,
            self.factory_profiles.get(STARTER_SMELTER_FACTORY_ID),
            self.factory_profiles.get(STARTER_ASSEMBLER_FACTORY_ID),
            self.recipe_profiles.get(STARTER_SMELTER_RECIPE_ID),
            smelter_factory
                .and_then(|factory| self.factory_site_authorities.get(factory.site_id.as_str())),
            self.factory_construction_power_profiles
                .get(STARTER_SMELTER_FACTORY_ID),
            self.factory_construction_power_profiles
                .get(STARTER_ASSEMBLER_FACTORY_ID),
            smelter_output_ledger
                .as_ref()
                .and_then(|ledger| self.material_ledgers.get(ledger)),
            &self.industry_progress.starter_industrial_milestone,
        ))
        .unwrap_or_else(|_| format!("unavailable-at-{}", self.time));

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
                next_recheck: Some(self.time.saturating_add(1)),
                progression_effect: progression_effect.to_string(),
            }
        };

        let milestone_matches = self
            .industry_progress
            .starter_industrial_milestone
            .as_ref()
            .is_some_and(|milestone| {
                milestone.profile_id == STARTER_INDUSTRIAL_PROFILE_ID
                    && milestone.profile_revision == STARTER_INDUSTRIAL_PROFILE_REVISION
                    && milestone.factory_id == STARTER_SMELTER_FACTORY_ID
                    && milestone.recipe_id == STARTER_SMELTER_RECIPE_ID
            });
        if milestone_matches {
            let assembler_profile_ready = self
                .factory_profiles
                .get(STARTER_ASSEMBLER_FACTORY_ID)
                .is_some_and(|profile| {
                    profile.factory_id == STARTER_ASSEMBLER_FACTORY_ID
                        && profile.tags.iter().any(|tag| tag == "assembler")
                });
            let assembler_power_profile_ready = self
                .factory_construction_power_profiles
                .get(STARTER_ASSEMBLER_FACTORY_ID)
                .is_some_and(|profile| {
                    profile.factory_id == STARTER_ASSEMBLER_FACTORY_ID
                        && profile.active
                        && profile.authority_revision > 0
                });
            if !assembler_profile_ready || !assembler_power_profile_ready {
                return base(
                    StarterIndustrialFeasibilityStatus::NoSafeStarterChain,
                    "unknown/not_tracked",
                    Some(format!(
                        "starter assembler authority is incomplete after milestone settlement: assembler_profile={assembler_profile_ready} assembler_power_profile={assembler_power_profile_ready}"
                    )),
                    "refresh_starter_assembler_authority",
                    "none",
                );
            }
            return base(
                StarterIndustrialFeasibilityStatus::CandidateAvailable,
                "durable-milestone-backed",
                None,
                "build_factory_assembler_mk1",
                "open_assembler_candidate_only",
            );
        }

        let Some(smelter_factory) = smelter_factory else {
            return base(
                StarterIndustrialFeasibilityStatus::NoSafeStarterChain,
                "unknown/not_tracked",
                Some(format!(
                    "starter industrial profile requires {STARTER_SMELTER_FACTORY_ID} before building {STARTER_ASSEMBLER_FACTORY_ID}"
                )),
                "build_factory_smelter_mk1",
                "none",
            );
        };

        let smelter_profile_ready = self
            .factory_profiles
            .get(STARTER_SMELTER_FACTORY_ID)
            .is_some_and(|profile| {
                profile.factory_id == STARTER_SMELTER_FACTORY_ID
                    && profile.tags.iter().any(|tag| tag == "smelter")
            });
        let assembler_profile_ready = self
            .factory_profiles
            .get(STARTER_ASSEMBLER_FACTORY_ID)
            .is_some_and(|profile| {
                profile.factory_id == STARTER_ASSEMBLER_FACTORY_ID
                    && profile.tags.iter().any(|tag| tag == "assembler")
            });
        let site_authority_ready = self
            .factory_site_authorities
            .get(smelter_factory.site_id.as_str())
            .is_some_and(|authority| {
                authority.site_id == smelter_factory.site_id
                    && authority.active
                    && authority.chunk_ready
                    && authority.authority_revision > 0
                    && !authority.location_id.trim().is_empty()
            });
        let smelter_power_profile_ready = self
            .factory_construction_power_profiles
            .get(STARTER_SMELTER_FACTORY_ID)
            .is_some_and(|profile| {
                profile.factory_id == STARTER_SMELTER_FACTORY_ID
                    && profile.active
                    && profile.authority_revision > 0
            });
        let assembler_power_profile_ready = self
            .factory_construction_power_profiles
            .get(STARTER_ASSEMBLER_FACTORY_ID)
            .is_some_and(|profile| {
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
            return base(
                StarterIndustrialFeasibilityStatus::NoSafeStarterChain,
                "unknown/not_tracked",
                Some(format!(
                    "starter industrial profile authority is incomplete: smelter_profile={smelter_profile_ready} assembler_profile={assembler_profile_ready} site_authority={site_authority_ready} smelter_power_profile={smelter_power_profile_ready} assembler_power_profile={assembler_power_profile_ready}"
                )),
                "refresh_starter_industrial_authority",
                "none",
            );
        }

        let receipt_matches = self.recipe_completion_receipts.values().any(|receipt| {
            receipt.factory_id == STARTER_SMELTER_FACTORY_ID
                && receipt.recipe_id == STARTER_SMELTER_RECIPE_ID
                && smelter_output_ledger.as_ref() == Some(&receipt.output_ledger)
                && receipt.accepted_batches > 0
                && receipt
                    .produce
                    .iter()
                    .any(|stack| stack.kind == "iron_ingot" && stack.amount > 0)
        });
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
                        && smelter_output_ledger.as_ref() == Some(&snapshot.output_ledger)
                        && snapshot
                            .produce
                            .iter()
                            .any(|stack| stack.kind == "iron_ingot" && stack.amount > 0)
                });
        if !receipt_matches && !legacy_snapshot_matches {
            return base(
                StarterIndustrialFeasibilityStatus::NoSafeStarterChain,
                "unknown/not_tracked",
                Some(format!(
                    "starter assembler requires a completed starter Smelter production receipt: complete {STARTER_SMELTER_RECIPE_ID} on {STARTER_SMELTER_FACTORY_ID} before building {STARTER_ASSEMBLER_FACTORY_ID}"
                )),
                "schedule_recipe_smelter_iron_ingot_and_wait_for_settlement",
                "none",
            );
        }

        base(
            StarterIndustrialFeasibilityStatus::CandidateAvailable,
            "current-evidence-backed",
            None,
            "build_factory_assembler_mk1",
            "open_assembler_candidate_only",
        )
    }
}
