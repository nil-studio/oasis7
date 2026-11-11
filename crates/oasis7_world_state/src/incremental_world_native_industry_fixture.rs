//! Original nonempty industry graph for native-envelope and profile consumers.
use crate::*;
use oasis7_agent_api::world_action::MaterialLedgerId;
use oasis7_wasm_abi::MaterialStack;
use std::collections::BTreeMap;
pub(crate) fn populate_world(world: &mut WorldState) {
    world.product_profiles.insert(
        "outside-product".into(),
        super::test_profiles::product_profile(),
    );
    world.recipe_profiles.insert(
        "outside-recipe".into(),
        super::test_profiles::recipe_profile(),
    );
    world.factory_profiles.insert(
        "outside-factory".into(),
        super::test_profiles::factory_profile(),
    );
    world.latest_product_validation = Some(LastProductValidationState {
        product_id: "latest-product".into(),
        tradable: false,
    });
    let receipt = ProductValidationReceiptV1 {
        job_id: 123,
        validation_index: Some(u32::MAX),
        requester_agent_id: "requester".into(),
        module_id: "validator".into(),
        stack: MaterialStack::new("material", i64::MAX),
        decision: oasis7_wasm_abi::ProductValidationDecision {
            product_id: "product".into(),
            accepted: false,
            notes: vec!["ordered".into(), "ordered".into()],
            stack_limit: u32::MAX,
            tradable: false,
            quality_levels: vec!["quality".into()],
        },
        failure_detail: Some("original failure detail".into()),
    };
    world
        .product_validation_receipts
        .insert(u64::MAX, vec![receipt.clone(), receipt]);
    world.product_validation_attempts.insert(
        u64::MAX,
        vec![ProductValidationAttemptV1 {
            job_id: 321,
            validation_index: None,
            requester_agent_id: "requester".into(),
            module_id: "validator".into(),
            stack: MaterialStack::new("material", i64::MIN),
        }],
    );
    world.factory_construction_receipts.insert(
        "outside-construction".into(),
        oasis7_agent_api::world_quote::FactoryBuildPowerObligationV1 {
            payer_agent_id: "payer".into(),
            profile_key: "inner-profile".into(),
            profile_revision: u64::MAX,
            electricity_amount: i64::MAX,
            mode: oasis7_agent_api::world_quote::FactoryConstructionPowerMode::StartOnlySink,
            material_ledger: Some(MaterialLedgerId::Agent("agent".into())),
            electricity_before: Some(i64::MAX),
            electricity_after: Some(i64::MIN),
            material_balances_before: Some(BTreeMap::new()),
            material_balances_after: Some(BTreeMap::from([("_length".into(), i64::MIN)])),
        },
    );
    world.factory_recycle_receipts.insert(
        "outside-recycle".into(),
        FactoryRecycleReceiptV1 {
            operator_agent_id: "operator".into(),
            factory_id: "inner-factory".into(),
            recycle_ledger: MaterialLedgerId::Site("site".into()),
            recovered: vec![
                MaterialStack::new("material", i64::MIN),
                MaterialStack::new("material", i64::MIN),
            ],
            durability_ppm: i64::MAX,
        },
    );
    world.industry_progress = IndustryProgressState {
        stage: crate::events::IndustryStage::Governance,
        stage_updated_at: u64::MAX,
        completed_recipe_jobs: u64::MAX,
        completed_material_transits: u64::MAX,
        starter_industrial_milestone: Some(StarterIndustrialMilestoneV1 {
            settlement_summary: Some(
                oasis7_agent_api::world_industry::StarterIndustrialSettlementSummaryV1 {
                    requester_agent_id: "requester".into(),
                    accepted_batches: u32::MAX,
                    consume: vec![MaterialStack::new("material", i64::MIN)],
                    power_required: i64::MIN,
                    produce: vec![MaterialStack::new("product", i64::MAX)],
                },
            ),
            committed_recipe_origin: Some(CommittedRecipeOrigin {
                submission: oasis7_proto::gameplay_submission_origin::GameplaySubmissionOrigin {
                    verified_player_id: "player".into(),
                    public_key: "a".repeat(64),
                    auth_nonce: u64::MAX,
                    hosted_registration_nonce: Some("hosted".into()),
                    requester_agent_id: "requester".into(),
                    factory_id: "factory".into(),
                    recipe_id: "recipe".into(),
                },
                consensus_action_id: u64::MAX,
                consensus_submitter_player_id: "player".into(),
                action_payload_hash: "b".repeat(64),
                committed_height: u64::MAX,
                action_root: "c".repeat(64),
            }),
            profile_id: "starter-industrial-smelter-to-assembler-v1".into(),
            profile_revision: u64::MAX,
            factory_id: "factory".into(),
            recipe_id: "recipe".into(),
            output_ledger: MaterialLedgerId::Factory("factory".into()),
            settlement_job_id: u64::MAX,
            settled_at: u64::MAX,
        }),
        latest_market_quotes: BTreeMap::from([(
            "outer-quote".into(),
            crate::events::MaterialMarketQuote {
                kind: "inner-material".into(),
                requested_amount: i64::MIN,
                local_available_amount: i64::MAX,
                world_available_amount: i64::MAX,
                local_deficit_amount: i64::MIN,
                transit_loss_bps: i64::MAX,
                governance_tax_bps: u16::MAX,
                effective_cost_index_ppm: i64::MIN,
            },
        )]),
    };
}
