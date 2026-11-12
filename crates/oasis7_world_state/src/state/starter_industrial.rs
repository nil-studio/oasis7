use super::super::events::{IndustryStage, MaterialMarketQuote};
#[path = "starter_industrial_fixed.rs"]
mod fixed;
use super::*;
pub use fixed::{
    StarterIndustrialFactsV1, StarterIndustrialReceiptFactsV1,
    starter_industrial_feasibility_from_facts_v1,
};

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
        let facts = StarterIndustrialFactsV1 {
            time: self.time,
            smelter_factory,
            smelter_profile: self.factory_profiles.get(STARTER_SMELTER_FACTORY_ID),
            assembler_profile: self.factory_profiles.get(STARTER_ASSEMBLER_FACTORY_ID),
            smelter_recipe: self.recipe_profiles.get(STARTER_SMELTER_RECIPE_ID),
            smelter_site: smelter_factory
                .and_then(|f| self.factory_site_authorities.get(&f.site_id)),
            smelter_power: self
                .factory_construction_power_profiles
                .get(STARTER_SMELTER_FACTORY_ID),
            assembler_power: self
                .factory_construction_power_profiles
                .get(STARTER_ASSEMBLER_FACTORY_ID),
            smelter_output: smelter_factory
                .and_then(|f| self.material_ledgers.get(&f.output_ledger)),
            milestone: self.industry_progress.starter_industrial_milestone.as_ref(),
        };
        match starter_industrial_feasibility_from_facts_v1(
            facts,
            self.recipe_completion_receipts
                .values()
                .map(|r| StarterIndustrialReceiptFactsV1 {
                    factory_id: &r.factory_id,
                    recipe_id: &r.recipe_id,
                    output_ledger: &r.output_ledger,
                    accepted_batches: r.accepted_batches,
                    produce: &r.produce,
                }),
            &mut |_| Ok::<(), std::convert::Infallible>(()),
        ) {
            Ok(result) => result,
            Err(never) => match never {},
        }
    }
}
