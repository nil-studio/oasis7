use super::*;

/// Current authority-bound factory build event contract. Version zero is
/// reserved for explicitly classified historical replay only.
pub const FACTORY_BUILD_STARTED_MODERN_VERSION: u8 = 1;

/// Runtime-owned identity for a location used by site and agent authority.
///
/// The registry is intentionally keyed by this exact `location_id`; physical
/// coordinates and caller-provided labels are not substitutes for an active
/// anchor.  The serde defaults keep older snapshots readable, while admission
/// still fails closed when a new authority update has no active anchor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct LocationAnchorV1 {
    #[serde(default)]
    pub location_id: String,
    #[serde(default)]
    pub active: bool,
    #[serde(default)]
    pub authority_revision: u64,
    #[serde(default)]
    pub effective_at: WorldTime,
}

/// Canonical location assignment for an agent that can participate in
/// location-bound runtime actions. `AgentState::pos` remains a physical
/// coordinate only and is not a location authority.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AgentLocationAuthorityV1 {
    #[serde(default)]
    pub agent_id: String,
    #[serde(default)]
    pub location_id: String,
    #[serde(default)]
    pub active: bool,
    #[serde(default)]
    pub authority_revision: u64,
    #[serde(default)]
    pub effective_at: WorldTime,
}

/// Persisted site admission authority. The allowlist is normalized on update
/// so access checks and replay have one canonical order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct FactorySiteAuthorityV1 {
    #[serde(default)]
    pub site_id: String,
    #[serde(default)]
    pub location_id: String,
    #[serde(default)]
    pub owner_agent_id: String,
    #[serde(default)]
    pub authorized_agent_ids: Vec<String>,
    #[serde(default)]
    pub chunk_ready: bool,
    #[serde(default)]
    pub active: bool,
    #[serde(default)]
    pub authority_revision: u64,
    #[serde(default)]
    pub registered_at: WorldTime,
}

pub use oasis7_agent_api::world_quote::FactoryConstructionPowerMode;

/// M4-governed construction electricity source. The map key is the exact
/// `factory_id` submitted by BuildFactory; callers cannot provide an amount.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct FactoryConstructionPowerProfileV1 {
    #[serde(default)]
    pub factory_id: String,
    #[serde(default)]
    pub factory_kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_module_id: Option<String>,
    #[serde(default)]
    pub electricity_amount: i64,
    #[serde(default)]
    pub mode: FactoryConstructionPowerMode,
    #[serde(default)]
    pub authority_revision: u64,
    #[serde(default)]
    pub active: bool,
}

pub use oasis7_agent_api::world_quote::FactoryBuildPowerObligationV1;

/// Durable intent written before a product validation module call. If a
/// retry observes this intent without a settled receipt it must fail closed,
/// rather than invoke the module a second time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProductValidationAttemptV1 {
    #[serde(default)]
    pub job_id: ActionId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validation_index: Option<u32>,
    #[serde(default)]
    pub requester_agent_id: String,
    #[serde(default)]
    pub module_id: String,
    #[serde(default = "default_receipt_material_stack")]
    pub stack: MaterialStack,
}

/// Durable receipt for a product-module decision.  The job and output index
/// bind a decision to one production commitment, allowing crash recovery to
/// reuse the decision without invoking a module again.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProductValidationReceiptV1 {
    #[serde(default)]
    pub job_id: ActionId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validation_index: Option<u32>,
    #[serde(default)]
    pub requester_agent_id: String,
    #[serde(default)]
    pub module_id: String,
    #[serde(default = "default_receipt_material_stack")]
    pub stack: MaterialStack,
    #[serde(default = "default_receipt_product_validation_decision")]
    pub decision: ProductValidationDecision,
    /// Structured module trap/invalid-output context, retained with the
    /// correlated rejection and blocker for replay/audit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure_detail: Option<String>,
}

impl Default for ProductValidationReceiptV1 {
    fn default() -> Self {
        Self {
            job_id: 0,
            validation_index: None,
            requester_agent_id: String::new(),
            module_id: String::new(),
            stack: default_receipt_material_stack(),
            decision: default_receipt_product_validation_decision(),
            failure_detail: None,
        }
    }
}

fn default_receipt_material_stack() -> MaterialStack {
    MaterialStack::new(String::new(), 0)
}

fn default_receipt_product_validation_decision() -> ProductValidationDecision {
    ProductValidationDecision::rejected(String::new(), 0, false, Vec::new(), Vec::new())
}

/// Full replay identity for a settled recipe completion.  The legacy
/// settled-id set remains for old snapshots, while new completions persist
/// the payload needed to reject same-id tampering.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RecipeCompletionReceiptV1 {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub committed_recipe_origin: Option<crate::CommittedRecipeOrigin>,
    #[serde(default)]
    pub job_id: ActionId,
    #[serde(default)]
    pub requester_agent_id: String,
    #[serde(default)]
    pub factory_id: String,
    #[serde(default)]
    pub recipe_id: String,
    #[serde(default)]
    pub accepted_batches: u32,
    #[serde(default)]
    pub produce: Vec<MaterialStack>,
    #[serde(default)]
    pub byproducts: Vec<MaterialStack>,
    #[serde(default)]
    pub output_ledger: MaterialLedgerId,
    #[serde(default)]
    pub bottleneck_tags: Vec<String>,
    #[serde(default)]
    pub logistics_route_ids: Vec<String>,
    #[serde(default)]
    pub logistics_path_ids: Vec<String>,
}

/// Full replay identity for a retired factory event.  A tombstone alone is
/// insufficient to distinguish an exact duplicate from a conflicting event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct FactoryRecycleReceiptV1 {
    #[serde(default)]
    pub operator_agent_id: String,
    #[serde(default)]
    pub factory_id: String,
    #[serde(default)]
    pub recycle_ledger: MaterialLedgerId,
    #[serde(default)]
    pub recovered: Vec<MaterialStack>,
    #[serde(default)]
    pub durability_ppm: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum FactoryProductionStatus {
    #[default]
    Idle,
    Running,
    Blocked,
    Paused,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FactoryProductionState {
    #[serde(default)]
    pub status: FactoryProductionStatus,
    #[serde(default)]
    pub active_jobs: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_job_id: Option<ActionId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_recipe_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_started_at: Option<WorldTime>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_completed_at: Option<WorldTime>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_blocked_at: Option<WorldTime>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_resumed_at: Option<WorldTime>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_blocker_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_blocker_detail: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_blocker_action_id: Option<ActionId>,
    #[serde(default)]
    pub completed_jobs: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_completed_recipe_id: Option<String>,
    #[serde(default)]
    pub same_recipe_repeat_count: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_completed_canonical_snapshot: Option<FactoryProductionSnapshot>,
}

impl Default for FactoryProductionState {
    fn default() -> Self {
        Self {
            status: FactoryProductionStatus::Idle,
            active_jobs: 0,
            current_job_id: None,
            current_recipe_id: None,
            last_started_at: None,
            last_completed_at: None,
            last_blocked_at: None,
            last_resumed_at: None,
            current_blocker_kind: None,
            current_blocker_detail: None,
            current_blocker_action_id: None,
            completed_jobs: 0,
            last_completed_recipe_id: None,
            same_recipe_repeat_count: 0,
            last_completed_canonical_snapshot: None,
        }
    }
}

/// Stable prerequisite facts for the latest completed recipe on a factory.
///
/// This snapshot intentionally excludes transient execution details such as
/// duration/ETA, live balances, and market quotes.  The factory containing it
/// is the partition key for the candidate window.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct FactoryProductionSnapshot {
    #[serde(default)]
    pub recipe_id: String,
    #[serde(default)]
    pub consume: Vec<MaterialStack>,
    #[serde(default)]
    pub produce: Vec<MaterialStack>,
    #[serde(default)]
    pub byproducts: Vec<MaterialStack>,
    #[serde(default)]
    pub consume_ledger: MaterialLedgerId,
    #[serde(default)]
    pub power_required: i64,
    #[serde(default)]
    pub output_ledger: MaterialLedgerId,
    #[serde(default)]
    pub bottleneck_tags: Vec<String>,
    #[serde(default)]
    pub logistics_route_ids: Vec<String>,
    #[serde(default)]
    pub logistics_path_ids: Vec<String>,
}

impl FactoryProductionSnapshot {
    pub fn from_recipe_job(job: &RecipeJobState) -> Self {
        match Self::from_recipe_job_with_work_v1(
            job,
            &mut |_| Ok::<_, std::convert::Infallible>(()),
        ) {
            Ok(value) => value,
            Err(never) => match never {},
        }
    }
    /// Work-only extension of the original normalization/clone kernel.
    pub fn from_recipe_job_with_work_v1<E>(
        job: &RecipeJobState,
        work: &mut impl FnMut(usize) -> Result<(), E>,
    ) -> Result<Self, E> {
        work(std::mem::size_of::<Self>())?;
        let recipe_id = clone_snapshot_text(&job.recipe_id, work)?;
        let consume = normalize_material_stacks(&job.consume, work)?;
        let produce = normalize_material_stacks(&job.produce, work)?;
        let byproducts = normalize_material_stacks(&job.byproducts, work)?;
        let consume_ledger = clone_snapshot_ledger(&job.consume_ledger, work)?;
        let output_ledger = clone_snapshot_ledger(&job.output_ledger, work)?;
        let bottleneck_tags = normalize_bottleneck_tags(&job.bottleneck_tags, work)?;
        let logistics_route_ids = clone_snapshot_strings(&job.logistics_route_ids, work)?;
        let logistics_path_ids = clone_snapshot_strings(&job.logistics_path_ids, work)?;
        Ok(Self {
            recipe_id,
            consume,
            produce,
            byproducts,
            consume_ledger,
            power_required: job.power_required,
            output_ledger,
            bottleneck_tags,
            logistics_route_ids,
            logistics_path_ids,
        })
    }
    pub fn visit_readonly_comparison_bytes_v1<E>(
        &self,
        work: &mut impl FnMut(usize) -> Result<(), E>,
    ) -> Result<(), E> {
        let Self {
            recipe_id,
            consume,
            produce,
            byproducts,
            consume_ledger,
            power_required: _,
            output_ledger,
            bottleneck_tags,
            logistics_route_ids,
            logistics_path_ids,
        } = self;
        work(std::mem::size_of::<Self>())?;
        work(recipe_id.len())?;
        for stacks in [consume, produce, byproducts] {
            for stack in stacks {
                work(std::mem::size_of::<MaterialStack>().saturating_add(stack.kind.len()))?;
            }
        }
        for ledger in [consume_ledger, output_ledger] {
            work(match ledger {
                MaterialLedgerId::World => 1,
                MaterialLedgerId::Agent(v)
                | MaterialLedgerId::Site(v)
                | MaterialLedgerId::Factory(v) => v.len().saturating_add(1),
            })?;
        }
        for strings in [bottleneck_tags, logistics_route_ids, logistics_path_ids] {
            for text in strings {
                work(std::mem::size_of::<String>().saturating_add(text.len()))?;
            }
        }
        Ok(())
    }
}
fn clone_snapshot_text<E>(
    text: &str,
    work: &mut impl FnMut(usize) -> Result<(), E>,
) -> Result<String, E> {
    work(text.len().saturating_add(std::mem::size_of::<String>()))?;
    Ok(text.to_owned())
}
fn clone_snapshot_ledger<E>(
    ledger: &MaterialLedgerId,
    work: &mut impl FnMut(usize) -> Result<(), E>,
) -> Result<MaterialLedgerId, E> {
    Ok(match ledger {
        MaterialLedgerId::World => MaterialLedgerId::World,
        MaterialLedgerId::Agent(v) => MaterialLedgerId::Agent(clone_snapshot_text(v, work)?),
        MaterialLedgerId::Site(v) => MaterialLedgerId::Site(clone_snapshot_text(v, work)?),
        MaterialLedgerId::Factory(v) => MaterialLedgerId::Factory(clone_snapshot_text(v, work)?),
    })
}
fn clone_snapshot_strings<E>(
    strings: &[String],
    work: &mut impl FnMut(usize) -> Result<(), E>,
) -> Result<Vec<String>, E> {
    work(strings.len().saturating_mul(std::mem::size_of::<String>()))?;
    let mut out = Vec::with_capacity(strings.len());
    for text in strings {
        out.push(clone_snapshot_text(text, work)?);
    }
    Ok(out)
}
fn normalize_material_stacks<E>(
    stacks: &[MaterialStack],
    work: &mut impl FnMut(usize) -> Result<(), E>,
) -> Result<Vec<MaterialStack>, E> {
    let mut merged = BTreeMap::<String, i64>::new();
    for stack in stacks {
        let levels = (merged.len() + 1).ilog2() as usize + 2;
        work(
            512usize.saturating_mul(levels).saturating_add(
                stack
                    .kind
                    .len()
                    .saturating_add(1)
                    .saturating_mul(levels.saturating_mul(2).saturating_add(4)),
            ),
        )?;
        let kind = stack.kind.trim().to_ascii_lowercase();
        let amount = merged.entry(kind).or_default();
        *amount = amount.saturating_add(stack.amount);
    }
    work(
        merged
            .len()
            .saturating_mul(std::mem::size_of::<MaterialStack>()),
    )?;
    let mut out = Vec::with_capacity(merged.len());
    for (kind, amount) in merged {
        work(
            kind.len()
                .saturating_add(std::mem::size_of::<MaterialStack>()),
        )?;
        out.push(MaterialStack::new(kind, amount));
    }
    Ok(out)
}
fn normalize_bottleneck_tags<E>(
    tags: &[String],
    work: &mut impl FnMut(usize) -> Result<(), E>,
) -> Result<Vec<String>, E> {
    let mut set = BTreeSet::new();
    for tag in tags {
        let levels = (set.len() + 1usize).ilog2() as usize + 2;
        work(
            512usize.saturating_mul(levels).saturating_add(
                tag.len()
                    .saturating_add(1)
                    .saturating_mul(levels.saturating_mul(2).saturating_add(4)),
            ),
        )?;
        let tag = tag.trim().to_ascii_lowercase();
        if !tag.is_empty() {
            set.insert(tag);
        }
    }
    work(set.len().saturating_mul(std::mem::size_of::<String>()))?;
    let mut out = Vec::with_capacity(set.len());
    for tag in set {
        work(tag.len().saturating_add(std::mem::size_of::<String>()))?;
        out.push(tag);
    }
    Ok(out)
}
