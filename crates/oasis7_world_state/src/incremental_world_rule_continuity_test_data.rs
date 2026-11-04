//! Explicit, nonempty original DTO fixtures; map IDs deliberately differ from inner IDs.
use std::collections::{BTreeMap, BTreeSet};

#[allow(non_snake_case)]
pub(crate) fn fixture_OrganizationControllerBindingV1()
-> crate::world_rules::OrganizationControllerBindingV1 {
    crate::world_rules::OrganizationControllerBindingV1 {
        root_agent_id: " inner identity ".into(),
        player_id: " inner identity ".into(),
        source_id: " inner identity ".into(),
        generation: 17,
    }
}

#[allow(non_snake_case)]
pub(crate) fn fixture_OrganizationActivityContractSourceV1()
-> crate::world_rules::OrganizationActivityContractSourceV1 {
    crate::world_rules::OrganizationActivityContractSourceV1 {
        contract_id: " inner identity ".into(),
        terms_hash: " inner identity ".into(),
        creator_binding: fixture_OrganizationControllerBindingV1(),
        counterparty_binding: fixture_OrganizationControllerBindingV1(),
        registered_at_world_tick: 17,
        registration_receipt: " inner identity ".into(),
        world_binding: " inner identity ".into(),
    }
}

#[allow(non_snake_case)]
pub(crate) fn fixture_OrganizationRegistrationV1() -> crate::world_rules::OrganizationRegistrationV1
{
    crate::world_rules::OrganizationRegistrationV1 {
        organization_id: " inner identity ".into(),
        founder_id: " inner identity ".into(),
        policy_version: 17,
        enabled_at_world_tick: 17,
        last_accounted_world_tick: 17,
        activity_evidence_complete_through_world_tick: 17,
        activity_evidence_paused_reason: Some(" inner identity ".into()),
        availability_paused_reason: Some(" inner identity ".into()),
        world_binding: " inner identity ".into(),
        charter_hash: " inner identity ".into(),
        registration_receipt: " inner identity ".into(),
        manifest_root: " inner identity ".into(),
        manifest_asset_refs: BTreeSet::from([" inner identity ".into()]),
        designated_successors: vec![" inner identity ".into()],
        neutral_maintainers: vec![" inner identity ".into()],
        appointment_authorities: BTreeSet::from([" inner identity ".into()]),
        remedy_adjudicators: BTreeSet::from([" inner identity ".into()]),
        residual_share_bps: BTreeMap::from([(" outer-key ".into(), 17)]),
        controller_bindings: BTreeMap::from([(
            " outer-key ".into(),
            fixture_OrganizationControllerBindingV1(),
        )]),
        membership_controller_bindings: BTreeMap::from([(
            " outer-key ".into(),
            fixture_OrganizationControllerBindingV1(),
        )]),
        activity_sources: BTreeMap::from([(
            " outer-key ".into(),
            fixture_OrganizationActivityContractSourceV1(),
        )]),
        activity_observation_started_at_world_tick: Some(17),
        activity_close_cursor_world_tick: 17,
        activity_close_hash: Some(" inner identity ".into()),
        activity_last_closed_event_id: Some(17),
        activity_last_closed_event_hash: Some(" inner identity ".into()),
    }
}

#[allow(non_snake_case)]
pub(crate) fn fixture_OrganizationAvailabilityIntervalV1()
-> crate::world_rules::OrganizationAvailabilityIntervalV1 {
    crate::world_rules::OrganizationAvailabilityIntervalV1 {
        from_world_tick: 17,
        through_world_tick: 17,
        world_writes_available: true,
        notice_available: true,
        remedy_available: true,
        evidence_status: crate::world_rules::OrganizationAvailabilityEvidenceStatusV1::Certified,
        world_binding: " inner identity ".into(),
        registration_receipt: " inner identity ".into(),
        certificate_hash: " inner identity ".into(),
    }
}

#[allow(non_snake_case)]
pub(crate) fn fixture_OrganizationActivityReceiptV1()
-> crate::world_rules::OrganizationActivityReceiptV1 {
    crate::world_rules::OrganizationActivityReceiptV1 {
        source_ref: " inner identity ".into(),
        world_tick: 17,
        kind: crate::world_rules::OrganizationActivityKindV1::AuthorizedBusinessDecision,
    }
}

#[allow(non_snake_case)]
pub(crate) fn fixture_OrganizationManifestAssetV1()
-> crate::world_rules::OrganizationManifestAssetV1 {
    crate::world_rules::OrganizationManifestAssetV1 {
        asset_ref: " inner identity ".into(),
        source_receipt: " inner identity ".into(),
        organization_id: " inner identity ".into(),
        contributor_id: " inner identity ".into(),
        material: " inner identity ".into(),
        amount: -19,
        held_in_trust: true,
        version: 17,
        disposed: true,
        effect_receipt: Some(" inner identity ".into()),
    }
}

#[allow(non_snake_case)]
pub(crate) fn fixture_OrganizationSuccessorCandidateV1()
-> crate::world_rules::OrganizationSuccessorCandidateV1 {
    crate::world_rules::OrganizationSuccessorCandidateV1 {
        candidate_id: " inner identity ".into(),
        controller_root_id: " inner identity ".into(),
        controller_player_id: " inner identity ".into(),
        controller_source_id: " inner identity ".into(),
        controller_generation: 17,
        scope_refs: BTreeSet::from([" inner identity ".into()]),
        budget_ref: " inner identity ".into(),
        budget_snapshot: BTreeMap::from([(" outer-key ".into(), -19)]),
        qualification_receipts: BTreeSet::from([" inner identity ".into()]),
        conflict_disclosures: BTreeSet::from([" inner identity ".into()]),
        acceptance_receipt: " inner identity ".into(),
        submitted_at_clock: 17,
        submission_order: 17,
        appointed: true,
    }
}

#[allow(non_snake_case)]
pub(crate) fn fixture_OrganizationRemedyRequestV1()
-> crate::world_rules::OrganizationRemedyRequestV1 {
    crate::world_rules::OrganizationRemedyRequestV1 {
        request_id: " inner identity ".into(),
        requester_id: " inner identity ".into(),
        kind: oasis7_agent_api::world_action::OrganizationRemedyKindV1::Appeal,
        scope_refs: BTreeSet::from([" inner identity ".into()]),
        fact_ref: " inner identity ".into(),
        evidence_refs: vec![" inner identity ".into()],
        admitted_at_clock: 17,
        deadline_clock: 17,
        outcome: Some(
            oasis7_agent_api::world_action::OrganizationRemedyOutcomeV1::PartiallySupported,
        ),
        supported_scope_refs: BTreeSet::from([" inner identity ".into()]),
        adjudication_ref: Some(" inner identity ".into()),
        resolved_by: Some(" inner identity ".into()),
    }
}

#[allow(non_snake_case)]
pub(crate) fn fixture_OrganizationContinuityCaseV1()
-> crate::world_rules::OrganizationContinuityCaseV1 {
    crate::world_rules::OrganizationContinuityCaseV1 {
        case_id: " inner identity ".into(),
        organization_id: " inner identity ".into(),
        registration_receipt: " inner identity ".into(),
        policy_version: 17,
        origin: crate::world_rules::OrganizationCaseOriginV1::VoluntaryReorganization,
        revision: 17,
        phase: crate::world_rules::OrganizationContinuityPhaseV1::Disposition,
        disposition_stage: crate::world_rules::OrganizationDispositionStageV1::ClaimsAndCosts,
        manifest_root: " inner identity ".into(),
        manifest_asset_refs: BTreeSet::from([" inner identity ".into()]),
        recipients: BTreeSet::from([" inner identity ".into()]),
        trigger_activity_receipt: Some(" inner identity ".into()),
        opened_at_world_tick: 17,
        opened_at_clock: 17,
        notice_receipt: Some(" inner identity ".into()),
        notified_at_clock: Some(17),
        notified_at_world_tick: Some(17),
        protection_started_at_clock: Some(17),
        protection_ends_at_clock: Some(17),
        candidate_window_starts_at_clock: Some(17),
        disposition_started_at_clock: Some(17),
        candidates: BTreeMap::from([(
            " outer-key ".into(),
            fixture_OrganizationSuccessorCandidateV1(),
        )]),
        remedies: BTreeMap::from([(" outer-key ".into(), fixture_OrganizationRemedyRequestV1())]),
        quarantined_scope_refs: BTreeSet::from([" inner identity ".into()]),
        handoff_candidate_id: Some(" inner identity ".into()),
        handoff_scope_refs: BTreeSet::from([" inner identity ".into()]),
        handoff_expires_at_clock: Some(17),
        stage_receipts: vec![" inner identity ".into()],
    }
}

#[allow(non_snake_case)]
pub(crate) fn fixture_OrganizationContinuityStateV1()
-> crate::world_rules::OrganizationContinuityStateV1 {
    crate::world_rules::OrganizationContinuityStateV1 {
        registrations: BTreeMap::from([(
            " outer-key ".into(),
            fixture_OrganizationRegistrationV1(),
        )]),
        availability: BTreeMap::from([(
            " outer-key ".into(),
            vec![fixture_OrganizationAvailabilityIntervalV1()],
        )]),
        activity_receipts: BTreeMap::from([(
            " outer-key ".into(),
            BTreeMap::from([(
                " outer-key ".into(),
                fixture_OrganizationActivityReceiptV1(),
            )]),
        )]),
        membership_history: BTreeMap::from([(
            " outer-key ".into(),
            BTreeMap::from([(
                " outer-key ".into(),
                vec![(17, Some(17), " inner identity ".into())],
            )]),
        )]),
        assets: BTreeMap::from([(" outer-key ".into(), fixture_OrganizationManifestAssetV1())]),
        cases: BTreeMap::from([(" outer-key ".into(), fixture_OrganizationContinuityCaseV1())]),
        consumed_effects: BTreeMap::from([(" outer-key ".into(), " inner identity ".into())]),
    }
}

#[allow(non_snake_case)]
pub(crate) fn fixture_EmergencySupplyStateV1() -> crate::world_rules::EmergencySupplyStateV1 {
    crate::world_rules::EmergencySupplyStateV1 {
        world_id: Some(" inner identity ".into()),
        minimum_services: BTreeMap::from([(
            " outer-key ".into(),
            fixture_MinimumServiceContractV1(),
        )]),
        region_lineages: BTreeMap::from([(" outer-key ".into(), " inner identity ".into())]),
        observations: BTreeMap::from([(" outer-key ".into(), fixture_EmergencyObservationV1())]),
        observation_interval_coverage: BTreeMap::from([(
            " outer-key ".into(),
            BTreeMap::from([(" outer-key ".into(), -19)]),
        )]),
        overflowed_windows: BTreeSet::from([(" inner identity ".into(), 17)]),
        services: BTreeMap::from([(
            " outer-key ".into(),
            fixture_EmergencyServiceRegistrationV1(),
        )]),
        evidence: BTreeMap::from([(" outer-key ".into(), fixture_EmergencyEvidenceV1())]),
        authorizations: BTreeMap::from([(
            " outer-key ".into(),
            fixture_EmergencyAuthorizationV1(),
        )]),
        batches: BTreeMap::from([(" outer-key ".into(), fixture_EmergencyBatchV1())]),
        needs: BTreeMap::from([(" outer-key ".into(), fixture_EmergencyNeedLedgerV1())]),
        reviews: BTreeMap::from([(" outer-key ".into(), fixture_EmergencyReviewV1())]),
        restricted_power: BTreeMap::from([(
            " outer-key ".into(),
            fixture_EmergencyRestrictedPowerV1(),
        )]),
    }
}

#[allow(non_snake_case)]
pub(crate) fn fixture_EmergencyAuthorizationV1() -> crate::world_rules::EmergencyAuthorizationV1 {
    crate::world_rules::EmergencyAuthorizationV1 {
        id: " inner identity ".into(),
        evidence_id: " inner identity ".into(),
        provider_id: " inner identity ".into(),
        reviewer_id: " inner identity ".into(),
        reviewer_controller_id: " inner identity ".into(),
        reviewer_owner_source_id: " inner identity ".into(),
        provider_controller_id: " inner identity ".into(),
        region_id: " inner identity ".into(),
        starts_at: 17,
        expires_at: 17,
        remaining: -19,
        active: true,
        requested: BTreeSet::from([" inner identity ".into()]),
    }
}

#[allow(non_snake_case)]
pub(crate) fn fixture_EmergencyBatchV1() -> crate::world_rules::EmergencyBatchV1 {
    crate::world_rules::EmergencyBatchV1 {
        authorization_id: " inner identity ".into(),
        frozen_at: 17,
        frozen_pool: -19,
        evidence_id: " inner identity ".into(),
        demands: BTreeMap::from([(" outer-key ".into(), -19)]),
        frozen_ledger: BTreeMap::from([(" outer-key ".into(), fixture_EmergencyNeedLedgerV1())]),
        allocation: BTreeMap::from([(" outer-key ".into(), -19)]),
        allocated: true,
        committed_at: Some(17),
    }
}

#[allow(non_snake_case)]
pub(crate) fn fixture_EmergencyNeedLedgerV1() -> crate::world_rules::EmergencyNeedLedgerV1 {
    crate::world_rules::EmergencyNeedLedgerV1 {
        obligation_id: " inner identity ".into(),
        controller_id: " inner identity ".into(),
        location_id: " inner identity ".into(),
        beneficiary_id: " inner identity ".into(),
        interval_start: 17,
        minimum: -19,
        normal_coverage: -19,
        fulfilled: -19,
        reserved: -19,
        public_critical: true,
    }
}

#[allow(non_snake_case)]
pub(crate) fn fixture_EmergencyRestrictedPowerV1() -> crate::world_rules::EmergencyRestrictedPowerV1
{
    crate::world_rules::EmergencyRestrictedPowerV1 {
        beneficiary_id: " inner identity ".into(),
        obligation_id: " inner identity ".into(),
        interval_start: 17,
        remaining: -19,
        delivered: -19,
    }
}

#[allow(non_snake_case)]
pub(crate) fn fixture_EmergencyReviewV1() -> crate::world_rules::EmergencyReviewV1 {
    crate::world_rules::EmergencyReviewV1 {
        batch_id: " inner identity ".into(),
        need_id: " inner identity ".into(),
        requester_id: " inner identity ".into(),
        reason_ref: " inner identity ".into(),
        opened_at: 17,
        deadline: 17,
        decision: Some(true),
    }
}

#[allow(non_snake_case)]
pub(crate) fn fixture_MinimumServiceContractV1() -> crate::world_rules::MinimumServiceContractV1 {
    crate::world_rules::MinimumServiceContractV1 {
        registration: fixture_EmergencyServiceRegistrationV1(),
        owner_source_id: " inner identity ".into(),
        operator_agent_id: " inner identity ".into(),
        region_budget_lineage: " inner identity ".into(),
        registered_at: 17,
        certificate: fixture_GovernanceFinalityCertificate(),
    }
}

#[allow(non_snake_case)]
pub(crate) fn fixture_EmergencyServiceRegistrationV1()
-> oasis7_agent_api::world_action::EmergencyServiceRegistrationV1 {
    oasis7_agent_api::world_action::EmergencyServiceRegistrationV1 {
        obligation_id: " inner identity ".into(),
        beneficiary_id: " inner identity ".into(),
        controller_id: " inner identity ".into(),
        region_id: " inner identity ".into(),
        location_id: " inner identity ".into(),
        registered_at: 17,
        electricity_per_interval: -19,
        public_critical: true,
        recipe_ids: BTreeSet::from([" inner identity ".into()]),
        source_ref: " inner identity ".into(),
    }
}

#[allow(non_snake_case)]
pub(crate) fn fixture_EmergencyObservationV1()
-> oasis7_agent_api::world_action::EmergencyObservationV1 {
    oasis7_agent_api::world_action::EmergencyObservationV1 {
        window_start: 17,
        world_root: " inner identity ".into(),
        obligation_ids: BTreeSet::from([" inner identity ".into()]),
        normal_coverage: BTreeMap::from([(" outer-key ".into(), -19)]),
    }
}

#[allow(non_snake_case)]
pub(crate) fn fixture_EmergencyEvidenceV1() -> oasis7_agent_api::world_action::EmergencyEvidenceV1 {
    oasis7_agent_api::world_action::EmergencyEvidenceV1 {
        region_id: " inner identity ".into(),
        region_budget_lineage: " inner identity ".into(),
        observations: vec![fixture_EmergencyObservationV1()],
        source_ref: " inner identity ".into(),
    }
}

#[allow(non_snake_case)]
pub(crate) fn fixture_GovernanceFinalityCertificate()
-> oasis7_agent_api::world_action::GovernanceFinalityCertificate {
    oasis7_agent_api::world_action::GovernanceFinalityCertificate {
        proposal_id: 17,
        manifest_hash: " inner identity ".into(),
        consensus_height: 17,
        epoch_id: 17,
        validator_set_hash: " inner identity ".into(),
        stake_root: " inner identity ".into(),
        threshold_bps: 17,
        min_unique_signers: 17,
        threshold: 17,
        signatures: BTreeMap::from([(" outer-key ".into(), " inner identity ".into())]),
    }
}
