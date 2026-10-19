use super::*;

pub const ORGANIZATION_CONTINUITY_POLICY_VERSION_V1: u32 = 1;
pub const ORGANIZATION_CONTINUITY_PERIOD_TICKS_V1: u64 = 1_440;
pub const ORGANIZATION_INACTIVITY_TICKS_V1: u64 = 30 * ORGANIZATION_CONTINUITY_PERIOD_TICKS_V1;
pub const ORGANIZATION_NOTICE_TICKS_V1: u64 = 7 * ORGANIZATION_CONTINUITY_PERIOD_TICKS_V1;
pub const ORGANIZATION_PROTECTION_TICKS_V1: u64 = 30 * ORGANIZATION_CONTINUITY_PERIOD_TICKS_V1;
pub const ORGANIZATION_CANDIDATE_TICKS_V1: u64 = 7 * ORGANIZATION_CONTINUITY_PERIOD_TICKS_V1;
pub const ORGANIZATION_GRANT_TICKS_V1: u64 = 7 * ORGANIZATION_CONTINUITY_PERIOD_TICKS_V1;
pub const ORGANIZATION_APPEAL_TICKS_V1: u64 = 14 * ORGANIZATION_CONTINUITY_PERIOD_TICKS_V1;
pub const ORGANIZATION_ACTIVITY_SOURCE_POLICY_V1: &str = "registered_atomic_exchange_v1";
pub(super) const MAX_INTERVAL_TICKS: u64 = 10_000;
pub(super) const MAX_ASSETS_PER_REGISTRATION: usize = 65_536;
pub(super) const MAX_ACTIVE_REMEDIES_PER_CASE: usize = 256;
pub(super) const MAX_ACTIVE_REMEDIES_PER_ACTOR: usize = 8;

fn default_availability_paused_reason() -> Option<String> {
    Some("availability_unknown".into())
}

fn default_activity_evidence_paused_reason() -> Option<String> {
    Some("activity_evidence_unavailable".into())
}

/// Deliberately narrow adapter for existing AllianceState.charter values.
/// Unknown charter formats remain readable but cannot enable continuity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OrganizationContinuityCharterV1 {
    pub schema: String,
    pub designated_successors: Vec<String>,
    pub neutral_maintainers: Vec<String>,
    pub appointment_authorities: BTreeSet<String>,
    pub remedy_adjudicators: BTreeSet<String>,
    pub activity_source_policy: String,
    pub residual_share_bps: BTreeMap<String, u16>,
}

/// Frozen control identity for a chartered organization role. Credential key
/// rotation on the same source preserves authority; a new controller/source
/// does not inherit it merely by reusing an Agent ID.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct OrganizationControllerBindingV1 {
    pub root_agent_id: String,
    pub player_id: String,
    pub source_id: String,
    pub generation: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrganizationActivityContractSourceV1 {
    pub contract_id: String,
    pub terms_hash: String,
    pub creator_binding: OrganizationControllerBindingV1,
    pub counterparty_binding: OrganizationControllerBindingV1,
    pub registered_at_world_tick: u64,
    pub registration_receipt: String,
    pub world_binding: String,
}

/// Founder-only read material for the exact canonical terms hash required by
/// `RegisterActivityContract`. It deliberately omits the external party's
/// identity and contract body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrganizationActivityContractCandidateV1 {
    pub organization_id: String,
    pub contract_id: String,
    pub expected_terms_hash: String,
    pub expires_at: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrganizationRegistrationV1 {
    pub organization_id: String,
    pub founder_id: String,
    #[serde(default)]
    pub policy_version: u32,
    pub enabled_at_world_tick: u64,
    #[serde(default)]
    pub last_accounted_world_tick: u64,
    /// Highest tick for which the registered AtomicExchange source catalog
    /// has a complete post-settlement journal close.
    #[serde(default)]
    pub activity_evidence_complete_through_world_tick: u64,
    /// Public lifecycle status. Missing activity data blocks dormancy rather
    /// than being interpreted as inactivity.
    #[serde(default = "default_activity_evidence_paused_reason")]
    pub activity_evidence_paused_reason: Option<String>,
    #[serde(default = "default_availability_paused_reason")]
    pub availability_paused_reason: Option<String>,
    #[serde(default)]
    pub world_binding: String,
    pub charter_hash: String,
    pub registration_receipt: String,
    pub manifest_root: String,
    pub manifest_asset_refs: BTreeSet<String>,
    pub designated_successors: Vec<String>,
    pub neutral_maintainers: Vec<String>,
    pub appointment_authorities: BTreeSet<String>,
    pub remedy_adjudicators: BTreeSet<String>,
    pub residual_share_bps: BTreeMap<String, u16>,
    #[serde(default)]
    pub controller_bindings: BTreeMap<String, OrganizationControllerBindingV1>,
    /// Controller snapshots for the active membership intervals. Join receipts
    /// extend this map without granting any charter role authority.
    #[serde(default)]
    pub membership_controller_bindings: BTreeMap<String, OrganizationControllerBindingV1>,
    #[serde(default)]
    pub activity_sources: BTreeMap<String, OrganizationActivityContractSourceV1>,
    #[serde(default)]
    pub activity_observation_started_at_world_tick: Option<u64>,
    #[serde(default)]
    pub activity_close_cursor_world_tick: u64,
    #[serde(default)]
    pub activity_close_hash: Option<String>,
    #[serde(default)]
    pub activity_last_closed_event_id: Option<u64>,
    #[serde(default)]
    pub activity_last_closed_event_hash: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrganizationAvailabilityEvidenceStatusV1 {
    #[default]
    Unknown,
    Certified,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrganizationAvailabilityIntervalV1 {
    pub from_world_tick: u64,
    pub through_world_tick: u64,
    pub world_writes_available: bool,
    pub notice_available: bool,
    pub remedy_available: bool,
    #[serde(default)]
    pub evidence_status: OrganizationAvailabilityEvidenceStatusV1,
    pub world_binding: String,
    pub registration_receipt: String,
    pub certificate_hash: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrganizationActivityKindV1 {
    FulfilledDelivery,
    ExternalServiceSettlement,
    ExternalAtomicExchangeSettlement,
    NonCircularProductionDelivery,
    AuthorizedBusinessDecision,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrganizationActivityReceiptV1 {
    pub source_ref: String,
    pub world_tick: u64,
    pub kind: OrganizationActivityKindV1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrganizationContinuityPhaseV1 {
    AwaitingNotice,
    Notice,
    Protection,
    CandidateCollection,
    Protected,
    Disposition,
    Resolved,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrganizationCaseOriginV1 {
    Dormancy,
    VoluntaryDissolution,
    VoluntaryReorganization,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrganizationDispositionStageV1 {
    None,
    Frozen,
    ExistingContracts,
    CustodyReturn,
    ClaimsAndCosts,
    BusinessTransfer,
    Residual,
    Complete,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrganizationManifestAssetV1 {
    pub asset_ref: String,
    pub source_receipt: String,
    pub organization_id: String,
    pub contributor_id: String,
    pub material: String,
    pub amount: i64,
    pub held_in_trust: bool,
    pub version: u64,
    pub disposed: bool,
    pub effect_receipt: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrganizationSuccessorCandidateV1 {
    pub candidate_id: String,
    /// Controller snapshot at candidate acceptance. Empty legacy fields fail closed.
    #[serde(default)]
    pub controller_root_id: String,
    #[serde(default)]
    pub controller_player_id: String,
    #[serde(default)]
    pub controller_source_id: String,
    #[serde(default)]
    pub controller_generation: u64,
    pub scope_refs: BTreeSet<String>,
    pub budget_ref: String,
    pub budget_snapshot: BTreeMap<String, i64>,
    pub qualification_receipts: BTreeSet<String>,
    pub conflict_disclosures: BTreeSet<String>,
    pub acceptance_receipt: String,
    pub submitted_at_clock: u64,
    pub submission_order: u32,
    pub appointed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrganizationRemedyRequestV1 {
    pub request_id: String,
    pub requester_id: String,
    pub kind: RemedyKind,
    pub scope_refs: BTreeSet<String>,
    pub fact_ref: String,
    pub evidence_refs: Vec<String>,
    pub admitted_at_clock: u64,
    pub deadline_clock: u64,
    pub outcome: Option<RemedyOutcome>,
    pub supported_scope_refs: BTreeSet<String>,
    pub adjudication_ref: Option<String>,
    pub resolved_by: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrganizationContinuityCaseV1 {
    pub case_id: String,
    pub organization_id: String,
    pub registration_receipt: String,
    pub policy_version: u32,
    pub origin: OrganizationCaseOriginV1,
    pub revision: u64,
    pub phase: OrganizationContinuityPhaseV1,
    pub disposition_stage: OrganizationDispositionStageV1,
    pub manifest_root: String,
    pub manifest_asset_refs: BTreeSet<String>,
    pub recipients: BTreeSet<String>,
    pub trigger_activity_receipt: Option<String>,
    pub opened_at_world_tick: u64,
    pub opened_at_clock: u64,
    pub notice_receipt: Option<String>,
    pub notified_at_clock: Option<u64>,
    #[serde(default)]
    pub notified_at_world_tick: Option<u64>,
    pub protection_started_at_clock: Option<u64>,
    pub protection_ends_at_clock: Option<u64>,
    pub candidate_window_starts_at_clock: Option<u64>,
    pub disposition_started_at_clock: Option<u64>,
    pub candidates: BTreeMap<String, OrganizationSuccessorCandidateV1>,
    pub remedies: BTreeMap<String, OrganizationRemedyRequestV1>,
    pub quarantined_scope_refs: BTreeSet<String>,
    pub handoff_candidate_id: Option<String>,
    pub handoff_scope_refs: BTreeSet<String>,
    pub handoff_expires_at_clock: Option<u64>,
    pub stage_receipts: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct OrganizationContinuityStateV1 {
    #[serde(default)]
    pub registrations: BTreeMap<String, OrganizationRegistrationV1>,
    #[serde(default)]
    pub availability: BTreeMap<String, Vec<OrganizationAvailabilityIntervalV1>>,
    #[serde(default)]
    pub activity_receipts: BTreeMap<String, BTreeMap<String, OrganizationActivityReceiptV1>>,
    #[serde(default)]
    pub membership_history: BTreeMap<String, BTreeMap<String, Vec<(u64, Option<u64>, String)>>>,
    #[serde(default)]
    pub assets: BTreeMap<String, OrganizationManifestAssetV1>,
    #[serde(default)]
    pub cases: BTreeMap<String, OrganizationContinuityCaseV1>,
    #[serde(default)]
    pub consumed_effects: BTreeMap<String, String>,
}

impl OrganizationContinuityStateV1 {
    /// A participant-scoped status projection. Public lifecycle gates remain
    /// visible, while financial and evidentiary bodies stay with their owners
    /// and the chartered decision makers.
    pub fn for_reader(&self, reader: Option<&str>) -> Self {
        let reader = reader.unwrap_or("");
        let mut value = self.clone();
        let authorized_orgs = self
            .registrations
            .values()
            .filter(|registration| {
                registration.founder_id == reader
                    || registration.appointment_authorities.contains(reader)
                    || registration.remedy_adjudicators.contains(reader)
                    || registration
                        .designated_successors
                        .iter()
                        .any(|id| id == reader)
                    || registration
                        .neutral_maintainers
                        .iter()
                        .any(|id| id == reader)
            })
            .map(|registration| registration.organization_id.clone())
            .collect::<BTreeSet<_>>();
        value
            .membership_history
            .retain(|organization, _| authorized_orgs.contains(organization));
        value.activity_receipts.retain(|organization, receipts| {
            let authorized = authorized_orgs.contains(organization);
            if !authorized {
                receipts.clear();
            }
            authorized
        });
        value.assets.retain(|_, asset| {
            authorized_orgs.contains(&asset.organization_id) || asset.contributor_id == reader
        });
        value.registrations.retain(|organization, registration| {
            let authorized = authorized_orgs.contains(organization);
            if !authorized {
                registration.residual_share_bps.clear();
                registration.controller_bindings.clear();
                registration.membership_controller_bindings.clear();
                registration.activity_sources.clear();
                registration.appointment_authorities.clear();
                registration.remedy_adjudicators.clear();
                registration.designated_successors.clear();
                registration.neutral_maintainers.clear();
            }
            true
        });
        for case in value.cases.values_mut() {
            let case_participant = case.recipients.contains(reader)
                || authorized_orgs.contains(&case.organization_id)
                || case.candidates.contains_key(reader);
            for candidate in case.candidates.values_mut() {
                if candidate.candidate_id != reader
                    && !authorized_orgs.contains(&case.organization_id)
                {
                    candidate.controller_root_id.clear();
                    candidate.controller_player_id.clear();
                    candidate.controller_source_id.clear();
                    candidate.controller_generation = 0;
                    candidate.budget_ref.clear();
                    candidate.budget_snapshot.clear();
                    candidate.qualification_receipts.clear();
                    candidate.conflict_disclosures.clear();
                    candidate.acceptance_receipt.clear();
                }
            }
            for remedy in case.remedies.values_mut() {
                let may_read_body = remedy.requester_id == reader
                    || authorized_orgs.contains(&case.organization_id);
                if !may_read_body {
                    remedy.fact_ref.clear();
                    remedy.evidence_refs.clear();
                    remedy.adjudication_ref = None;
                    remedy.supported_scope_refs.clear();
                }
            }
            if !case_participant {
                case.manifest_asset_refs.clear();
                case.trigger_activity_receipt = None;
                case.recipients.clear();
                case.handoff_scope_refs.clear();
            }
        }
        value
    }
}
