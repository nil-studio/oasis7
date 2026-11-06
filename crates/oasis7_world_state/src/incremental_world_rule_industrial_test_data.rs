use oasis7_agent_api::world_action::*;
use oasis7_wasm_abi::*;
use std::collections::{BTreeMap, BTreeSet};

fn reference(id: &str) -> IndustrialRefV1 {
    IndustrialRefV1 {
        world_binding: "world".into(),
        domain: "industrial".into(),
        schema: "v1".into(),
        id: id.into(),
        revision: u64::MAX,
        digest: "ab".repeat(32),
    }
}
fn profile(id: &str) -> SubstitutionProfileRefV1 {
    SubstitutionProfileRefV1 {
        id: id.into(),
        version: "v1".into(),
        digest: "cd".repeat(32),
    }
}
fn parent() -> IndustrialParentV1 {
    IndustrialParentV1 {
        lot_ref: reference("parent-lot"),
        settlement_ref: reference("settlement"),
        quantity: u64::MAX,
    }
}
fn disposition() -> DispositionPlanV1 {
    DispositionPlanV1 {
        root_ref: reference("root"),
        plan_ref: reference("plan"),
        expected_causal_digest: "causal".into(),
        disposition: IndustrialDispositionV1::Terminate,
    }
}
fn operation() -> RootOperationV1 {
    RootOperationV1 {
        owner_binding_digest: "owner-binding".into(),
        allowed_dispositions: vec![
            IndustrialDispositionV1::Terminate,
            IndustrialDispositionV1::Hold,
            IndustrialDispositionV1::Terminate,
        ],
        output_ledger: MaterialLedgerId::Factory("factory-ledger".into()),
        terminal_capacities: BTreeMap::from([("_length".into(), 0), ("iron".into(), u64::MAX)]),
        started_job_id: Some(0),
        root_ref: reference("inner-root"),
        owner_id: "owner".into(),
        factory_id: "factory".into(),
        pinned_recipe_id: "recipe".into(),
        accepted_batches: u32::MAX,
        pinned_profile_digest: "pinned".into(),
        causal_digest: "causal".into(),
        work: u64::MAX,
        first_sink: false,
        slots: BTreeMap::from([(
            "slot-outer".into(),
            IndustrialInputSlotV1 {
                material_profile: profile("input"),
                material: "铁-é".into(),
                unit: "kg".into(),
                spec: "spec".into(),
                quality: "quality".into(),
                custody: "custody".into(),
                quantity: 0,
                binding_revision: u64::MAX,
                parents: vec![parent(), parent()],
                consumed: true,
            },
        )]),
    }
}
pub(super) fn industrial() -> IndustrialWorkStateV1 {
    let lot = |ledger| IndustrialLotV1 {
        lot_ref: reference("inner-lot"),
        owner_id: "owner".into(),
        source_ledger: ledger,
        material: "iron".into(),
        unit: "kg".into(),
        spec: "spec".into(),
        quality: "quality".into(),
        custody: "custody".into(),
        provenance_ref: reference("provenance"),
        quantity: u64::MAX,
        remaining: 0,
    };
    IndustrialWorkStateV1 {
        roots: BTreeMap::from([("_length".into(), operation())]),
        lots: BTreeMap::from([
            ("world".into(), lot(MaterialLedgerId::World)),
            (
                "agent".into(),
                lot(MaterialLedgerId::Agent("agent-inner".into())),
            ),
            (
                "site".into(),
                lot(MaterialLedgerId::Site("site-inner".into())),
            ),
        ]),
        imported_lot_authorities: BTreeMap::from([(
            "authority-outer".into(),
            IndustrialLotAuthorityV1 {
                owner_binding_digest: "binding".into(),
                expires_at: u64::MAX,
                escrow_capacity: 0,
                material_profile: profile("authority-profile"),
            },
        )]),
        receipts: BTreeMap::from([(
            "receipt-outer".into(),
            IndustrialReceiptV1 {
                receipt_ref: reference("receipt-inner"),
                root_ref: reference("root"),
                child_ref: reference("child"),
                effect_role: "role".into(),
                payload_digest: "payload".into(),
                input: vec![parent(), parent()],
                output: vec![reference("output"), reference("output")],
            },
        )]),
        sink_keys: BTreeSet::from(["_length".into(), "世界\0sink".into()]),
        bindings: vec![ParentBindingRevisionV1 {
            root_ref: reference("binding-root"),
            slot_id: "slot".into(),
            expected_revision: u64::MAX,
            decision_ref: reference("decision"),
            parents: vec![parent(), parent()],
        }],
        dispositions: BTreeMap::from([("disposition-outer".into(), disposition())]),
    }
}
fn impacts() -> SubstitutionImpactV1 {
    SubstitutionImpactV1 {
        outputs: profile("outputs"),
        quality: profile("quality"),
        power: profile("power"),
        time: profile("time"),
        byproducts: profile("byproducts"),
        terminal: profile("terminal"),
        value_class: profile("value-class"),
        logistics: profile("logistics"),
        buffer: profile("buffer"),
    }
}
fn authority(selection: SubstitutionSelectionV1) -> SubstitutionExecutionAuthorityV1 {
    SubstitutionExecutionAuthorityV1 {
        authority_ref: reference("authority-inner"),
        owner_id: "owner".into(),
        owner_binding_digest: "binding".into(),
        factory_digest: "factory".into(),
        root_ref: reference("root"),
        slot_id: "slot".into(),
        expected_binding_revision: u64::MAX,
        profile_digest: "profile".into(),
        selection,
        target_recipe: "recipe".into(),
        baseline_impacts: impacts(),
        candidate_impacts: impacts(),
        output_material: "iron".into(),
        output_unit: "kg".into(),
        output_spec: "spec".into(),
        output_quality: "quality".into(),
        output_custody: "custody".into(),
        owner_ledger_path_policy: profile("ledger-path"),
        input_state: "active".into(),
        costs: BTreeMap::from([("_length".into(), u64::MAX)]),
        power_cost: 0,
        duration_ticks: u64::MAX,
        terminal_capacity: 0,
        valid_until: u64::MAX,
        valuation: SubstitutionValuationV1 {
            unit: "unit".into(),
            held: u64::MAX,
            consumed: 0,
            expected_output: u64::MAX,
        },
        risks: vec!["risk".into(), "risk".into()],
        next_recheck: u64::MAX,
    }
}
fn rule() -> SubstitutionRuleV1 {
    SubstitutionRuleV1 {
        rule: profile("rule"),
        recipe: profile("recipe"),
        input_edge: "edge".into(),
        required_material: profile("required"),
        allowed_materials: vec![profile("allowed"), profile("allowed")],
        accepted_specs: vec!["spec".into()],
        accepted_quality: vec![],
        accepted_custody: vec!["custody".into()],
        owner_ledger_path_policy: profile("ledger"),
        source_unit: "source".into(),
        target_unit: "target".into(),
        ratio_numerator: u64::MAX,
        ratio_denominator: 1,
        executable_target_quantum: 1,
        rounding: SubstitutionRoundingV1::Ceil,
        residual: SubstitutionResidualV1::Retain,
        aggregation_domain: profile("aggregate"),
        source_loss_numerator: 0,
        source_loss_denominator: 1,
        impacts: impacts(),
        conversion: SubstitutionConversionV1::Required {
            converter: profile("converter"),
        },
        applicable_input_states: vec!["active".into(), "active".into()],
        disposition_policy: profile("disposition"),
        recheck_policy: profile("recheck"),
    }
}
fn candidate() -> SubstitutionCandidateV1 {
    SubstitutionCandidateV1 {
        candidate_digest: "candidate".into(),
        authority: authority(SubstitutionSelectionV1::ProfileRule {
            rule: profile("rule"),
            source_lot: reference("source-lot"),
            source_material: profile("source-material"),
        }),
        rule: Some(rule()),
        kind: SubstitutionCandidateKindV1::ProfileSubstitute,
        plan: Some(SubstitutionQuantityPlanV1 {
            rule: profile("rule"),
            domain: profile("domain"),
            domain_key: "domain-key".into(),
            source_unit: "source".into(),
            target_unit: "target".into(),
            source_required: u64::MAX,
            source_loss: 0,
            target_executable: 0,
            residual_numerator: u64::MAX,
            residual_denominator: 1,
            unmet_target: u64::MAX,
        }),
        native_source_quantity: Some(0),
        source_material_kind: Some("iron".into()),
        candidate_root: Some(operation()),
        cutover_plan: Some(disposition()),
        cutover_plan_digest: Some("cutover".into()),
        recipe_plan: Some(RecipeExecutionPlan {
            accepted_batches: u32::MAX,
            consume: vec![MaterialStack {
                kind: "iron".into(),
                amount: i64::MIN,
            }],
            produce: vec![MaterialStack {
                kind: "steel".into(),
                amount: i64::MAX,
            }],
            byproducts: vec![],
            power_required: i64::MIN,
            duration_ticks: u32::MAX,
            reject_reason: Some("".into()),
        }),
        causal_equivalent: false,
        conversion_required: true,
        source_remaining: u64::MAX,
        binding_revision: 0,
        resource_balances: BTreeMap::from([("_length".into(), i64::MIN), ("铁".into(), i64::MAX)]),
    }
}
pub(super) fn substitution() -> SubstitutionStateV1 {
    let selected = candidate();
    let preview = SubstitutionPreviewV1 {
        preview_digest: "preview".into(),
        status: SubstitutionPreviewStatusV1::Available,
        blocker: Some("".into()),
        next_action: "next".into(),
        next_recheck: u64::MAX,
        request: SubstitutionPreviewRequestV1 {
            root_ref: reference("root"),
            slot_id: "slot".into(),
            authority_refs: vec![reference("a"), reference("a")],
        },
        candidates: vec![selected.clone()],
        evaluated_at: u64::MAX,
    };
    SubstitutionStateV1 {
        authorities: BTreeMap::from([
            (
                "_length".into(),
                authority(SubstitutionSelectionV1::SameMaterialSource {
                    source_lot: reference("lot"),
                    source_material: profile("material"),
                }),
            ),
            (
                "new-recipe".into(),
                authority(SubstitutionSelectionV1::NewRecipe {
                    candidate_root: reference("new-root"),
                }),
            ),
        ]),
        decisions: BTreeMap::from([(
            "decision-outer".into(),
            SubstitutionDecisionV1 {
                decision_ref: reference("decision-inner"),
                owner_id: "owner".into(),
                confirmation_key: "confirm".into(),
                intent: ConfirmSubstitutionV1 {
                    preview,
                    candidate_digest: "candidate".into(),
                    scope: SubstitutionScopeV1::DecisionAndExactConversion,
                    maximum_source: u64::MAX,
                    maximum_costs: BTreeMap::from([("power".into(), 0)]),
                    maximum_power: 0,
                    cutover_plan_digest: Some("".into()),
                },
                candidate: selected,
                confirmed_at: u64::MAX,
            },
        )]),
        settlements: BTreeMap::from([(
            "settlement-outer".into(),
            ConversionSettlementV1 {
                decision_ref: reference("decision"),
                conversion_ref: reference("conversion"),
                receipt_ref: reference("receipt"),
                output_lot: reference("output-lot"),
                source_consumed: u64::MAX,
                source_loss: 0,
                residual_numerator: 0,
                residual_denominator: 1,
                committed_at: u64::MAX,
            },
        )]),
        conversion_domains: BTreeSet::from(["_length".into(), "domain".into()]),
        terminal_used: BTreeMap::from([("terminal".into(), u64::MAX)]),
        retained_residuals: BTreeMap::from([(
            "residual-outer".into(),
            SubstitutionResidualBalanceV1 {
                owner_id: "owner".into(),
                domain_key: "domain".into(),
                source_lot: reference("source"),
                settlement_ref: reference("settlement"),
                unit: "unit".into(),
                numerator: u64::MAX,
                denominator: 1,
                next_action: "next".into(),
            },
        )]),
    }
}
