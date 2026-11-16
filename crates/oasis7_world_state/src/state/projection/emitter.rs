//! Single original field-order/skip/typed-borrow emitter. No WorldState
//! value is cloned or encoded by this visitor interface.
use super::*;

#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub enum OriginalStateFieldEmissionV1{Always,Some,NonEmpty,OriginalDefault}
impl OriginalStateFieldEmissionV1{/// Count is raw cardinality, Some=0/1, or an actual qualified non-default witness=0/1.
 pub fn emits_count(self,actual_count:usize)->bool{matches!(self,Self::Always)||actual_count>0}}
#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub struct OriginalStateFieldSlotV1{pub name:&'static str,pub emission:OriginalStateFieldEmissionV1}
#[derive(Debug,Clone,Copy)]pub enum OriginalStateFieldSlotEventV1{Declared(OriginalStateFieldSlotV1),Emitted(&'static str)}
pub(crate) struct SlotOutput<'a,O:serde::ser::SerializeStruct>{inner:O,slots:&'a mut dyn FnMut(OriginalStateFieldSlotEventV1)->Result<(),O::Error>}
impl<O:serde::ser::SerializeStruct> SlotOutput<'_,O>{
 pub(crate) fn new<'a>(inner:O,slots:&'a mut dyn FnMut(OriginalStateFieldSlotEventV1)->Result<(),O::Error>)->SlotOutput<'a,O>{SlotOutput{inner,slots}}
 pub(crate) fn declare(&mut self,name:&'static str,emission:OriginalStateFieldEmissionV1)->Result<(),O::Error>{(self.slots)(OriginalStateFieldSlotEventV1::Declared(OriginalStateFieldSlotV1{name,emission}))}
 pub(crate) fn declare_all(&mut self,fields:&[OriginalStateFieldSlotV1])->Result<(),O::Error>{for field in fields{(self.slots)(OriginalStateFieldSlotEventV1::Declared(*field))?}Ok(())}
}
impl<O:serde::ser::SerializeStruct> serde::ser::SerializeStruct for SlotOutput<'_,O>{type Ok=O::Ok;type Error=O::Error;fn serialize_field<T:Serialize+?Sized>(&mut self,key:&'static str,value:&T)->Result<(),O::Error>{(self.slots)(OriginalStateFieldSlotEventV1::Emitted(key))?;self.inner.serialize_field(key,value)}fn end(self)->Result<O::Ok,O::Error>{self.inner.end()}}
#[expect(clippy::too_many_arguments)]
pub(super) fn emit_world_state_fields_v1<O,F>(
    state: &WorldState,
    world_rule_overlay: Option<&crate::world_rules::PreparedWorldRule>,
    body_overlay: Option<&BodyOverlay>,
    command_overlay: Option<&CommandStateOverlay<'_>>,
    module_visual_entities_overlay: Option<&BTreeMap<String, ModuleVisualEntity>>,
    module_instance_overlay: Option<&module_instance_transition::PreparedModuleInstance>,
    module_release_overlay: Option<&module_release_transition::PreparedModuleRelease>,
    module_marketplace_overlay: Option<&module_marketplace_transition::PreparedModuleMarketplace>,
    governance_identity_profile_overlay: Option<&GovernanceIdentityProfileOverlay>,
    governance_registry_overlay: Option<
        &crate::world::governance_registry_publication::PreparedGovernanceRegistryEvent,
    >,
    agent_intent_overlay: Option<&crate::world::agent_intent_publication::PreparedAgentIntent>,
    economy_data_overlay: Option<&crate::world::economy_data_publication::PreparedEconomyDataEvent>,
    economic_contract_overlay: Option<
        &crate::world::economic_contract_publication::PreparedEconomicContractEvent,
    >,
    alliance_war_overlay: Option<&crate::world::alliance_war_publication::PreparedAllianceWarEvent>,
    governance_meta_overlay: Option<
        &crate::world::governance_meta_publication::PreparedGovernanceMetaEvent,
    >,
    core_policy_overlay: Option<&super::core_policy_transition::PreparedCorePolicyEvent>,
    industry_overlay: Option<&super::industry_transition::PreparedIndustryEvent>,
    power_redemption_overlay: Option<
        &crate::world::power_redemption_publication::PreparedPowerRedemptionEvent,
    >,
    node_points_settlement_overlay: Option<
        &crate::world::node_points_settlement_publication::PreparedNodePointsSettlement,
    >,
    main_token_monetary_overlay: Option<
        &crate::world::main_token_monetary_publication::PreparedMainTokenMonetaryEvent,
    >,
    main_token_governance_monetary_overlay: Option<
        &crate::world::main_token_governance_monetary_publication::PreparedMainTokenGovernanceMonetaryEvent,
    >,
    main_token_restricted_claim_overlay: Option<
        &crate::world::main_token_restricted_claim_publication::PreparedMainTokenRestrictedClaimEvent,
    >,
    starter_oc_claim_overlay: Option<
        &crate::world::starter_oc_claim_publication::PreparedStarterOcClaimed,
    >,
    agent_claim_light_lifecycle_overlay: Option<
        &crate::world::agent_claim_light_lifecycle_publication::PreparedAgentClaimLightLifecycle,
    >,
    agent_claim_economic_overlay: Option<
        &crate::world::agent_claim_economic_publication::PreparedAgentClaimEconomic,
    >,
    agent_claim_terminal_overlay: Option<
        &crate::world::agent_claim_terminal_publication::PreparedAgentClaimTerminal,
    >,
    product_validation_delivery_cursor: Option<&ProductValidationDeliveryCursor>,
    industry_history_overlay: Option<
        &super::industry_history_transition::PreparedIndustryHistoryEvent,
    >,
    create: F,
) -> Result<O::Ok, O::Error>
where
    O: serde::ser::SerializeStruct,
    F: FnOnce(usize)->Result<O,O::Error>,
{
 emit_world_state_fields_with_slots_v1(state,world_rule_overlay,body_overlay,command_overlay,module_visual_entities_overlay,module_instance_overlay,module_release_overlay,module_marketplace_overlay,governance_identity_profile_overlay,governance_registry_overlay,agent_intent_overlay,economy_data_overlay,economic_contract_overlay,alliance_war_overlay,governance_meta_overlay,core_policy_overlay,industry_overlay,power_redemption_overlay,node_points_settlement_overlay,main_token_monetary_overlay,main_token_governance_monetary_overlay,main_token_restricted_claim_overlay,starter_oc_claim_overlay,agent_claim_light_lifecycle_overlay,agent_claim_economic_overlay,agent_claim_terminal_overlay,product_validation_delivery_cursor,industry_history_overlay,create,&mut|_|Ok(()))
}

#[expect(clippy::too_many_arguments)]
pub(super) fn emit_world_state_fields_with_slots_v1<O,F>(
    state: &WorldState,
    world_rule_overlay: Option<&crate::world_rules::PreparedWorldRule>,
    body_overlay: Option<&BodyOverlay>,
    command_overlay: Option<&CommandStateOverlay<'_>>,
    module_visual_entities_overlay: Option<&BTreeMap<String, ModuleVisualEntity>>,
    module_instance_overlay: Option<&module_instance_transition::PreparedModuleInstance>,
    module_release_overlay: Option<&module_release_transition::PreparedModuleRelease>,
    module_marketplace_overlay: Option<&module_marketplace_transition::PreparedModuleMarketplace>,
    governance_identity_profile_overlay: Option<&GovernanceIdentityProfileOverlay>,
    governance_registry_overlay: Option<
        &crate::world::governance_registry_publication::PreparedGovernanceRegistryEvent,
    >,
    agent_intent_overlay: Option<&crate::world::agent_intent_publication::PreparedAgentIntent>,
    economy_data_overlay: Option<&crate::world::economy_data_publication::PreparedEconomyDataEvent>,
    economic_contract_overlay: Option<
        &crate::world::economic_contract_publication::PreparedEconomicContractEvent,
    >,
    alliance_war_overlay: Option<&crate::world::alliance_war_publication::PreparedAllianceWarEvent>,
    governance_meta_overlay: Option<
        &crate::world::governance_meta_publication::PreparedGovernanceMetaEvent,
    >,
    core_policy_overlay: Option<&super::core_policy_transition::PreparedCorePolicyEvent>,
    industry_overlay: Option<&super::industry_transition::PreparedIndustryEvent>,
    power_redemption_overlay: Option<
        &crate::world::power_redemption_publication::PreparedPowerRedemptionEvent,
    >,
    node_points_settlement_overlay: Option<
        &crate::world::node_points_settlement_publication::PreparedNodePointsSettlement,
    >,
    main_token_monetary_overlay: Option<
        &crate::world::main_token_monetary_publication::PreparedMainTokenMonetaryEvent,
    >,
    main_token_governance_monetary_overlay: Option<
        &crate::world::main_token_governance_monetary_publication::PreparedMainTokenGovernanceMonetaryEvent,
    >,
    main_token_restricted_claim_overlay: Option<
        &crate::world::main_token_restricted_claim_publication::PreparedMainTokenRestrictedClaimEvent,
    >,
    starter_oc_claim_overlay: Option<
        &crate::world::starter_oc_claim_publication::PreparedStarterOcClaimed,
    >,
    agent_claim_light_lifecycle_overlay: Option<
        &crate::world::agent_claim_light_lifecycle_publication::PreparedAgentClaimLightLifecycle,
    >,
    agent_claim_economic_overlay: Option<
        &crate::world::agent_claim_economic_publication::PreparedAgentClaimEconomic,
    >,
    agent_claim_terminal_overlay: Option<
        &crate::world::agent_claim_terminal_publication::PreparedAgentClaimTerminal,
    >,
    product_validation_delivery_cursor: Option<&ProductValidationDeliveryCursor>,
    industry_history_overlay: Option<
        &super::industry_history_transition::PreparedIndustryHistoryEvent,
    >,
    create: F,
    slots:&mut dyn FnMut(OriginalStateFieldSlotEventV1)->Result<(),O::Error>,
) -> Result<O::Ok, O::Error>
where
    O: serde::ser::SerializeStruct,
    F: FnOnce(usize)->Result<O,O::Error>,
{
    let WorldState {
        world_rules: _,
        time: _,
        agents: _,
        agent_intent_ledger: _,
        resources: _,
        materials: _,
        material_ledgers: _,
        material_profiles: _,
        logistics_routes: _,
        completed_logistics_route_ids: _,
        completed_logistics_paths: _,
        settled_logistics_transit_ids: _,
        logistics_settlement_receipts: _,
        factory_production_failure_dispositions: _,
        direct_material_transfer_receipts: _,
        product_validation_receipts: _,
        product_profiles: _,
        latest_product_validation: _,
        recipe_profiles: _,
        factory_profiles: _,
        agent_location_authorities: _,
        location_anchors: _,
        factory_site_authorities: _,
        factory_construction_power_profiles: _,
        factories: _,
        retired_factory_ids: _,
        settled_factory_build_ids: _,
        factory_construction_receipts: _,
        product_validation_attempts: _,
        product_validation_delivery_cursor: _,
        pending_factory_builds: _,
        pending_recipe_jobs: _,
        settled_recipe_job_ids: _,
        next_industry_settlement_order: _,
        industry_settlement_orders: _,
        recipe_completion_receipts: _,
        factory_recycle_receipts: _,
        pending_material_transits: _,
        industry_progress: _,
        alliances: _,
        gameplay_policy: _,
        data_access_permissions: _,
        economic_contracts: _,
        agent_claims: _,
        starter_oc_claims: _,
        authenticated_collect_data_last_nonces: _,
        agent_claim_last_processed_epoch: _,
        contract_pair_last_success_settled_at: _,
        reputation_reward_window_started_at: _,
        reputation_reward_window_accumulated: _,
        reputation_scores: _,
        wars: _,
        governance_votes: _,
        governance_proposals: _,
        governance_identity_profiles: _,
        crises: _,
        meta_progress: _,
        module_states: _,
        module_artifact_owners: _,
        module_artifact_listings: _,
        module_artifact_bids: _,
        module_instances: _,
        module_release_requests: _,
        module_release_manifest_mappings: _,
        next_module_release_request_id: _,
        module_release_role_bindings: _,
        installed_module_targets: _,
        next_module_instance_id: _,
        next_module_market_order_id: _,
        next_module_market_sale_id: _,
        main_token_config: _,
        main_token_supply: _,
        main_token_balances: _,
        restricted_starter_claim_grants: _,
        main_token_genesis_buckets: _,
        main_token_epoch_issuance_records: _,
        main_token_treasury_balances: _,
        main_token_claim_nonces: _,
        main_token_transfer_nonces: _,
        main_token_scheduled_policy_updates: _,
        main_token_node_points_bridge_records: _,
        main_token_treasury_distribution_records: _,
        restricted_starter_claim_liveops_pool_top_up_records: _,
        reward_asset_config: _,
        node_asset_balances: _,
        protocol_power_reserve: _,
        reward_mint_records: _,
        node_redeem_nonces: _,
        system_order_pool_budgets: _,
        node_identity_bindings: _,
        node_main_token_account_bindings: _,
        governance_finality_signer_registry: _,
        governance_validator_admissions: _,
        governance_finality_key_rotations: _,
        governance_bootstrap_finality_key_rotations: _,
        governance_main_token_controller_registry: _,
        reward_signature_governance_policy: _,
        ..
    } = state;

    let authorization_nonces = governance_registry_overlay
        .map_or(&state.governance_authorization_nonces, |o| {
            &o.authorization_nonces
        });
    let rotations = governance_registry_overlay
        .map_or(&state.governance_finality_key_rotations, |overlay| {
            &overlay.rotations
        });
    let bootstrap_rotations = governance_registry_overlay
        .map_or(&state.governance_bootstrap_finality_key_rotations, |o| {
            &o.bootstrap_rotations
        });
    let field_count = base_field_count(95, state, module_instance_overlay)
        + usize::from(!bootstrap_rotations.is_empty())
        + usize::from(!rotations.is_empty())
        + usize::from(!authorization_nonces.is_empty())
        + usize::from(
            !state.agent_claim_acquisition_sequences.is_empty()
                || agent_claim_economic_overlay
                    .is_some_and(|overlay| overlay.has_acquisition_sequence_update()),
        )
        + usize::from(
            state.world_rules.is_some()
                || world_rule_overlay.is_some()
                || industry_overlay
                    .and_then(|o| o.world_rules_projection())
                    .is_some(),
        )
        - usize::from(
            state.agent_intent_ledger.is_empty()
                && agent_intent_overlay.is_none_or(|overlay| overlay.ledger_updates.is_empty()),
        )
        - usize::from(
            state.latest_product_validation.is_none()
                && governance_meta_overlay.is_none_or(|v| {
                    !matches!(
                    v,
                    crate::world::governance_meta_publication::PreparedGovernanceMetaEvent::Product(
                        _
                    )
                )
                }),
        )
        - usize::from(state.starter_oc_claims.is_empty() && starter_oc_claim_overlay.is_none())
        - usize::from(
            state.authenticated_collect_data_last_nonces.is_empty()
                && economy_data_overlay.is_none_or(|overlay| !overlay.has_projected_nonces(state)),
        )
        - usize::from(
            state.product_validation_receipts.is_empty() && industry_history_overlay.is_none(),
        )
        - usize::from(
            state.factory_construction_receipts.is_empty()
                && industry_overlay.is_none_or(|v| !v.has_construction_receipt()),
        )
        - usize::from(
            state.product_validation_attempts.is_empty() && industry_history_overlay.is_none(),
        )
        - usize::from(
            state.recipe_completion_receipts.is_empty()
                && industry_overlay.is_none_or(|v| !v.has_completion_receipt()),
        )
        - usize::from(
            state.factory_recycle_receipts.is_empty()
                && industry_overlay.is_none_or(|v| !v.has_recycle_receipt()),
        )
        - usize::from(
            module_visual_entities_overlay
                .unwrap_or(&state.module_visual_entities)
                .is_empty(),
        );
    let mut output = SlotOutput{inner:create(field_count)?,slots};
    output.declare_all(&super::super::formal_retention_projection::ORIGINAL_RETENTION_FIELD_SLOTS_V1)?;
    serialize_prefix(&mut output, state)?;
    output.declare("world_rules",OriginalStateFieldEmissionV1::Some)?;
    if let Some(overlay) = world_rule_overlay {
        output.serialize_field("world_rules", &overlay.rules)?;
    } else if let Some(continuity) =
        economic_contract_overlay.and_then(|overlay| overlay.organization_continuity_projection())
        && let Some(rules) = industry_overlay
            .and_then(|overlay| overlay.world_rules_projection())
            .or_else(|| alliance_war_overlay.and_then(|overlay| overlay.world_rules_projection()))
            .or(state.world_rules.as_ref())
    {
        output.serialize_field(
            "world_rules",
            &crate::world_rules::WorldRulesContinuityProjection {
                base: rules,
                organization_continuity: continuity,
            },
        )?;
    } else if let Some(rules) = industry_overlay
        .and_then(|o| o.world_rules_projection())
        .or_else(|| alliance_war_overlay.and_then(|o| o.world_rules_projection()))
        .or(state.world_rules.as_ref())
    {
        output.serialize_field("world_rules", rules)?;
    }
    if let Some(overlay) = world_rule_overlay {
        overlay.serialize_agents(state, &mut output)?;
    } else if let Some(overlay) = agent_claim_terminal_overlay {
        overlay.serialize_agents(state, &mut output)?;
    } else if let Some(overlay) = agent_claim_economic_overlay {
        overlay.serialize_agents(state, &mut output)?;
    } else if let Some(overlay) = agent_claim_light_lifecycle_overlay {
        overlay.serialize_agents(state, &mut output)?;
    } else if let Some(overlay) = starter_oc_claim_overlay {
        overlay.serialize_agents(state, &mut output)?;
    } else if let Some(overlay) = main_token_restricted_claim_overlay {
        overlay.serialize_agents(state, &mut output)?;
    } else if let Some(overlay) = main_token_monetary_overlay {
        overlay.serialize_agents(state, &mut output)?;
    } else if let Some(overlay) = power_redemption_overlay {
        overlay.serialize_agents(state, &mut output)?;
    } else if let Some(overlay) = economy_data_overlay {
        overlay.serialize_agents(state, &mut output)?;
    } else if let Some(overlay) = core_policy_overlay {
        overlay.serialize_agents(state, &mut output)?;
    } else if let Some(overlay) = industry_overlay {
        overlay.serialize_agents(state, &mut output)?;
    } else if let Some(overlay) = industry_history_overlay {
        overlay.serialize_agents(state, &mut output)?;
    } else if let Some(overlay) = governance_meta_overlay {
        overlay.serialize_agents(state, &mut output)?;
    } else if let Some(overlay) = alliance_war_overlay {
        overlay.serialize_agents(state, &mut output)?;
    } else if let Some(overlay) = economic_contract_overlay {
        overlay.serialize_agents(state, &mut output)?;
    } else if let Some(command_overlay) = command_overlay {
        output.serialize_field(
            "agents",
            &CommandAgentMapProjection {
                agents: &state.agents,
                updates: command_overlay.agents,
            },
        )?;
    } else {
        output.serialize_field(
            "agents",
            &AgentMapProjection {
                agents: &state.agents,
                body_overlay,
            },
        )?;
    }
    output.declare("agent_intent_ledger",OriginalStateFieldEmissionV1::NonEmpty)?;
    if let Some(overlay) = agent_intent_overlay
        && (!state.agent_intent_ledger.is_empty() || !overlay.ledger_updates.is_empty())
    {
        output.serialize_field(
            "agent_intent_ledger",
            &ReleaseMapProjection {
                base: &state.agent_intent_ledger,
                updates: &overlay.ledger_updates,
            },
        )?;
    } else if !state.agent_intent_ledger.is_empty() {
        output.serialize_field("agent_intent_ledger", &state.agent_intent_ledger)?;
    }
    if let Some(command_overlay) = command_overlay {
        output.serialize_field(
            "resources",
            &CommandResourceMapProjection {
                resources: &state.resources,
                updates: command_overlay.resources,
            },
        )?;
    } else if let Some(overlay) = economic_contract_overlay {
        overlay.serialize_resources(state, &mut output)?;
    } else {
        output.serialize_field("resources", &state.resources)?;
    }
    if let Some(overlay) = world_rule_overlay {
        overlay.serialize_materials(state, &mut output)?;
    } else if let Some(overlay) = core_policy_overlay {
        overlay.serialize_materials(state, &mut output)?;
    } else if let Some(overlay) = industry_overlay {
        overlay.serialize_materials(state, &mut output)?;
    } else if let Some(overlay) = agent_claim_terminal_overlay {
        overlay.serialize_materials(state, &mut output)?;
    } else if let Some(overlay) = agent_claim_economic_overlay {
        overlay.serialize_materials(state, &mut output)?;
    } else if let Some(overlay) = agent_claim_light_lifecycle_overlay {
        overlay.serialize_materials(state, &mut output)?;
    } else if let Some(overlay) = starter_oc_claim_overlay {
        overlay.serialize_materials(state, &mut output)?;
    } else if let Some(overlay) = main_token_restricted_claim_overlay {
        overlay.serialize_materials(state, &mut output)?;
    } else if let Some(overlay) = main_token_governance_monetary_overlay {
        overlay.serialize_materials(state, &mut output)?;
    } else if let Some(overlay) = main_token_monetary_overlay {
        overlay.serialize_materials(state, &mut output)?;
    } else if let Some(overlay) = node_points_settlement_overlay {
        overlay.serialize_materials(state, &mut output)?;
    } else if let Some(overlay) = power_redemption_overlay {
        overlay.serialize_materials(state, &mut output)?;
    } else if let Some(overlay) = economy_data_overlay {
        overlay.serialize_material_fields(state, &mut output)?;
    } else if let Some(overlay) = module_instance_overlay {
        overlay.serialize_material_fields(state, &mut output)?;
    } else if let Some(overlay) = module_release_overlay {
        overlay.serialize_material_fields(state, &mut output)?;
    } else if let Some(overlay) = module_marketplace_overlay {
        overlay.serialize_material_fields(state, &mut output)?;
    } else if let Some(overlay) = alliance_war_overlay {
        overlay.serialize_materials(state, &mut output)?;
    } else if let Some(overlay) = economic_contract_overlay {
        overlay.serialize_materials(state, &mut output)?;
    } else {
        output.serialize_field("materials", &state.materials)?;
        output.serialize_field("material_ledgers", &state.material_ledgers)?;
    }
    if let Some(overlay) = core_policy_overlay {
        overlay.serialize_profile(state, &mut output)?;
    } else {
        output.serialize_field("material_profiles", &state.material_profiles)?;
    }
    if let Some(overlay) = industry_overlay {
        overlay.serialize_logistics(state, &mut output)?;
    } else {
        output.serialize_field("logistics_routes", &state.logistics_routes)?;
        output.serialize_field(
            "completed_logistics_route_ids",
            &state.completed_logistics_route_ids,
        )?;
        output.serialize_field(
            "completed_logistics_paths",
            &state.completed_logistics_paths,
        )?;
        output.serialize_field(
            "settled_logistics_transit_ids",
            &state.settled_logistics_transit_ids,
        )?;
        output.serialize_field(
            "logistics_settlement_receipts",
            &state.logistics_settlement_receipts,
        )?;
        output.serialize_field(
            "direct_material_transfer_receipts",
            &state.direct_material_transfer_receipts,
        )?;
    }
    if let Some(overlay) = industry_overlay {
        overlay.serialize_failure_dispositions(state, &mut output)?;
    } else {
        output.serialize_field(
            "factory_production_failure_dispositions",
            &state.factory_production_failure_dispositions,
        )?;
    }
    output.declare("product_validation_receipts",OriginalStateFieldEmissionV1::NonEmpty)?;
    if let Some(overlay) = industry_history_overlay {
        overlay.serialize_receipts(state, &mut output)?;
    } else if !state.product_validation_receipts.is_empty() {
        output.serialize_field(
            "product_validation_receipts",
            &state.product_validation_receipts,
        )?;
    }
    if let Some(overlay) = module_release_overlay {
        output.serialize_field(
            "product_profiles",
            &ReleaseMapProjection {
                base: &state.product_profiles,
                updates: &overlay.products,
            },
        )?;
    } else {
        output.serialize_field("product_profiles", &state.product_profiles)?;
    }
    output.declare("latest_product_validation",OriginalStateFieldEmissionV1::Some)?;
    if let Some(overlay) = governance_meta_overlay {
        overlay.serialize_product(state, &mut output)?;
    } else if let Some(overlay) = industry_history_overlay {
        overlay.serialize_latest(state, &mut output)?;
    } else if state.latest_product_validation.is_some() {
        output.serialize_field(
            "latest_product_validation",
            &state.latest_product_validation,
        )?;
    }
    if let Some(overlay) = module_release_overlay {
        output.serialize_field(
            "recipe_profiles",
            &ReleaseMapProjection {
                base: &state.recipe_profiles,
                updates: &overlay.recipes,
            },
        )?;
        output.serialize_field(
            "factory_profiles",
            &ReleaseMapProjection {
                base: &state.factory_profiles,
                updates: &overlay.factories,
            },
        )?;
    } else {
        output.serialize_field("recipe_profiles", &state.recipe_profiles)?;
        output.serialize_field("factory_profiles", &state.factory_profiles)?;
    }
    if let Some(overlay) = industry_history_overlay {
        overlay.serialize_authorities(state, &mut output)?;
    } else {
        output.serialize_field(
            "agent_location_authorities",
            &crate::world::economy_data_publication::SparseMapProjection {
                base: &state.agent_location_authorities,
                updates: &world_rule_overlay
                    .map(|o| o.locations.clone())
                    .unwrap_or_default(),
                deletion: None,
            },
        )?;
        output.serialize_field("location_anchors", &state.location_anchors)?;
        output.serialize_field("factory_site_authorities", &state.factory_site_authorities)?;
        output.serialize_field(
            "factory_construction_power_profiles",
            &state.factory_construction_power_profiles,
        )?;
    }
    if let Some(overlay) = industry_overlay {
        overlay.serialize_factory_fields(state, &mut output)?;
    } else {
        output.serialize_field("factories", &state.factories)?;
        output.serialize_field("retired_factory_ids", &state.retired_factory_ids)?;
        output.serialize_field(
            "settled_factory_build_ids",
            &state.settled_factory_build_ids,
        )?;
        output.serialize_field("pending_factory_builds", &state.pending_factory_builds)?;
        output.serialize_field("pending_recipe_jobs", &state.pending_recipe_jobs)?;
        output.serialize_field("settled_recipe_job_ids", &state.settled_recipe_job_ids)?;
    }
    output.declare("factory_construction_receipts",OriginalStateFieldEmissionV1::NonEmpty)?;
    if let Some(overlay) = industry_overlay {
        overlay.serialize_construction_receipts(state, &mut output)?;
    } else if !state.factory_construction_receipts.is_empty() {
        output.serialize_field(
            "factory_construction_receipts",
            &state.factory_construction_receipts,
        )?;
    }
    output.declare("product_validation_attempts",OriginalStateFieldEmissionV1::NonEmpty)?;
    if let Some(overlay) = industry_history_overlay {
        overlay.serialize_attempts(state, &mut output)?;
    } else if !state.product_validation_attempts.is_empty() {
        output.serialize_field(
            "product_validation_attempts",
            &state.product_validation_attempts,
        )?;
    }
    output.serialize_field(
        "product_validation_delivery_cursor",
        &product_validation_delivery_cursor.unwrap_or(&state.product_validation_delivery_cursor),
    )?;
    if let Some(overlay) = industry_overlay {
        overlay.serialize_settlement_history(state, &mut output)?;
    } else {
        output.serialize_field(
            "next_industry_settlement_order",
            &state.next_industry_settlement_order,
        )?;
        output.serialize_field(
            "industry_settlement_orders",
            &state.industry_settlement_orders,
        )?;
    }
    output.declare("recipe_completion_receipts",OriginalStateFieldEmissionV1::NonEmpty)?;
    output.declare("factory_recycle_receipts",OriginalStateFieldEmissionV1::NonEmpty)?;
    if let Some(overlay) = industry_overlay {
        overlay.serialize_terminal_receipts(state, &mut output)?;
    } else {
        if !state.recipe_completion_receipts.is_empty() {
            output.serialize_field(
                "recipe_completion_receipts",
                &state.recipe_completion_receipts,
            )?;
        }
        if !state.factory_recycle_receipts.is_empty() {
            output.serialize_field("factory_recycle_receipts", &state.factory_recycle_receipts)?;
        }
    }
    if let Some(overlay) = industry_overlay {
        overlay.serialize_pending_and_progress(state, &mut output)?;
    } else {
        output.serialize_field(
            "pending_material_transits",
            &state.pending_material_transits,
        )?;
        output.serialize_field(
            "industry_progress",
            core_policy_overlay.map_or(&state.industry_progress, |overlay| {
                overlay.projected_progress(state)
            }),
        )?;
    }
    if let Some(overlay) = world_rule_overlay {
        overlay.serialize_alliances(state, &mut output)?;
    } else if let Some(overlay) = alliance_war_overlay {
        overlay.serialize_alliances(state, &mut output)?;
    } else {
        output.serialize_field("alliances", &state.alliances)?;
    }
    output.serialize_field(
        "gameplay_policy",
        core_policy_overlay.map_or(&state.gameplay_policy, |overlay| {
            overlay.projected_policy(state)
        }),
    )?;
    if let Some(overlay) = economy_data_overlay {
        overlay.serialize_permissions(state, &mut output)?;
    } else {
        output.serialize_field("data_access_permissions", &state.data_access_permissions)?;
    }
    if let Some(overlay) = economic_contract_overlay {
        overlay.serialize_contracts(state, &mut output)?;
    } else {
        output.serialize_field("economic_contracts", &state.economic_contracts)?;
    }
    output.declare_all(&claims::ORIGINAL_CLAIM_FIELD_SLOTS_V1)?;
    claims::serialize_claim_projection(
        state,
        &mut output,
        agent_claim_terminal_overlay,
        agent_claim_economic_overlay,
        agent_claim_light_lifecycle_overlay,
        starter_oc_claim_overlay,
    )?;
    output.declare("authenticated_collect_data_last_nonces",OriginalStateFieldEmissionV1::NonEmpty)?;
    if let Some(overlay) = economy_data_overlay
        && overlay.has_projected_nonces(state)
    {
        overlay.serialize_nonces(state, &mut output)?;
    } else if !state.authenticated_collect_data_last_nonces.is_empty() {
        output.serialize_field(
            "authenticated_collect_data_last_nonces",
            &state.authenticated_collect_data_last_nonces,
        )?;
    }
    if let Some(overlay) = agent_claim_terminal_overlay {
        overlay.serialize_last_epoch(&mut output)?;
    } else if let Some(overlay) = agent_claim_economic_overlay {
        overlay.serialize_last_epoch(&mut output)?;
    } else {
        output.serialize_field(
            "agent_claim_last_processed_epoch",
            &state.agent_claim_last_processed_epoch,
        )?;
    }
    if let Some(overlay) = economic_contract_overlay {
        overlay.serialize_reputation(state, &mut output)?;
    } else {
        output.serialize_field(
            "contract_pair_last_success_settled_at",
            &state.contract_pair_last_success_settled_at,
        )?;
        output.serialize_field(
            "reputation_reward_window_started_at",
            &state.reputation_reward_window_started_at,
        )?;
        output.serialize_field(
            "reputation_reward_window_accumulated",
            &state.reputation_reward_window_accumulated,
        )?;
        if let Some(overlay) = alliance_war_overlay {
            overlay.serialize_reputation(state, &mut output)?;
        } else {
            output.serialize_field("reputation_scores", &state.reputation_scores)?;
        }
    }
    if let Some(overlay) = alliance_war_overlay {
        overlay.serialize_wars(state, &mut output)?;
    } else {
        output.serialize_field("wars", &state.wars)?;
    }
    if let Some(overlay) = governance_meta_overlay {
        overlay.serialize_governance(state, &mut output)?;
    } else {
        output.serialize_field("governance_votes", &state.governance_votes)?;
        output.serialize_field("governance_proposals", &state.governance_proposals)?;
    }
    if let Some(overlay) = governance_identity_profile_overlay {
        if !overlay.allow_insert
            && !state
                .governance_identity_profiles
                .contains_key(overlay.target_agent_id.as_str())
        {
            return Err(serde::ser::Error::custom(format!(
                "governance identity profile overlay target not found: {}",
                overlay.target_agent_id
            )));
        }
        output.serialize_field(
            "governance_identity_profiles",
            &GovernanceIdentityProfileMapProjection {
                profiles: &state.governance_identity_profiles,
                overlay,
            },
        )?;
    } else {
        output.serialize_field(
            "governance_identity_profiles",
            &state.governance_identity_profiles,
        )?;
    }
    if let Some(overlay) = governance_meta_overlay {
        overlay.serialize_crises(state, &mut output)?;
        overlay.serialize_meta(state, &mut output)?;
    } else {
        output.serialize_field("crises", &state.crises)?;
        output.serialize_field("meta_progress", &state.meta_progress)?;
    }
    if let Some(command_overlay) = command_overlay {
        output.serialize_field(
            "module_states",
            &CommandModuleStateMapProjection {
                module_states: &state.module_states,
                updates: command_overlay.module_states,
            },
        )?;
    } else {
        output.serialize_field("module_states", &state.module_states)?;
    }
    let module_visual_entities =
        module_visual_entities_overlay.unwrap_or(&state.module_visual_entities);
    output.declare("module_visual_entities",OriginalStateFieldEmissionV1::NonEmpty)?;
    if !module_visual_entities.is_empty() {
        output.serialize_field("module_visual_entities", module_visual_entities)?;
    }
    if let Some(overlay) = module_marketplace_overlay {
        overlay.serialize_market_fields(state, &mut output)?;
    } else {
        output.serialize_field("module_artifact_owners", &state.module_artifact_owners)?;
        output.serialize_field("module_artifact_listings", &state.module_artifact_listings)?;
        output.serialize_field("module_artifact_bids", &state.module_artifact_bids)?;
    }
    output.declare_all(&super::super::module_admission_freeze::ORIGINAL_MODULE_INSTANCE_FIELD_SLOTS_V1)?;
    serialize_module_instance_projection(state, module_instance_overlay, &mut output)?;
    if let Some(overlay) = module_release_overlay {
        output.serialize_field(
            "module_release_requests",
            &ReleaseMapProjection {
                base: &state.module_release_requests,
                updates: &overlay.requests,
            },
        )?;
        output.serialize_field(
            "module_release_manifest_mappings",
            &ReleaseMapProjection {
                base: &state.module_release_manifest_mappings,
                updates: &overlay.mappings,
            },
        )?;
    } else {
        output.serialize_field("module_release_requests", &state.module_release_requests)?;
        output.serialize_field(
            "module_release_manifest_mappings",
            &state.module_release_manifest_mappings,
        )?;
    }
    output.serialize_field(
        "next_module_release_request_id",
        &module_release_overlay
            .and_then(|overlay| overlay.next_request_id)
            .unwrap_or(state.next_module_release_request_id),
    )?;
    if let Some(overlay) = module_release_overlay {
        output.serialize_field(
            "module_release_role_bindings",
            &module_release_transition::ReleaseOptionalMapProjection {
                base: &state.module_release_role_bindings,
                updates: &overlay.role_bindings,
            },
        )?;
    } else {
        output.serialize_field(
            "module_release_role_bindings",
            &state.module_release_role_bindings,
        )?;
    }
    if let Some(overlay) = module_instance_overlay {
        overlay.serialize_target_fields(state, &mut output)?;
    } else {
        output.serialize_field("installed_module_targets", &state.installed_module_targets)?;
        output.serialize_field("next_module_instance_id", &state.next_module_instance_id)?;
    }
    if let Some(overlay) = module_marketplace_overlay {
        overlay.serialize_counter_fields(&mut output)?;
    } else {
        output.serialize_field(
            "next_module_market_order_id",
            &state.next_module_market_order_id,
        )?;
        output.serialize_field(
            "next_module_market_sale_id",
            &state.next_module_market_sale_id,
        )?;
    }
    output.serialize_field("main_token_config", &state.main_token_config)?;
    if let Some(overlay) = world_rule_overlay.filter(|o| o.claim.is_none()) {
        output.serialize_field("main_token_supply", &state.main_token_supply)?;
        output.serialize_field(
            "main_token_balances",
            &crate::world::economy_data_publication::SparseMapProjection {
                base: &state.main_token_balances,
                updates: &overlay.token_accounts,
                deletion: None,
            },
        )?;
    } else if let Some(overlay) = agent_claim_terminal_overlay {
        overlay.serialize_token(state, &mut output)?;
    } else if let Some(overlay) = agent_claim_economic_overlay {
        overlay.serialize_token(state, &mut output)?;
    } else if let Some(overlay) = starter_oc_claim_overlay {
        overlay.serialize_supply_balances(state, &mut output)?;
    } else if let Some(overlay) = main_token_restricted_claim_overlay {
        overlay.serialize_supply_balances(state, &mut output)?;
    } else if let Some(overlay) = main_token_governance_monetary_overlay {
        overlay.serialize_supply_balances(state, &mut output)?;
    } else if let Some(overlay) = main_token_monetary_overlay {
        overlay.serialize_supply_balances(state, &mut output)?;
    } else if let Some(overlay) = node_points_settlement_overlay {
        overlay.serialize_main_token_accounts(state, &mut output)?;
    } else {
        output.serialize_field("main_token_supply", &state.main_token_supply)?;
        output.serialize_field("main_token_balances", &state.main_token_balances)?;
    }
    if let Some(overlay) = main_token_restricted_claim_overlay {
        overlay.serialize_grants(state, &mut output)?;
    } else {
        output.serialize_field(
            "restricted_starter_claim_grants",
            &state.restricted_starter_claim_grants,
        )?;
    }
    if let Some(overlay) = main_token_monetary_overlay {
        overlay.serialize_buckets(state, &mut output)?;
        overlay.serialize_issuance(state, &mut output)?;
    } else {
        output.serialize_field(
            "main_token_genesis_buckets",
            &state.main_token_genesis_buckets,
        )?;
        output.serialize_field(
            "main_token_epoch_issuance_records",
            &state.main_token_epoch_issuance_records,
        )?;
    }
    if let Some(overlay) = agent_claim_terminal_overlay {
        overlay.serialize_treasury(state, &mut output)?;
    } else if let Some(overlay) = agent_claim_economic_overlay {
        overlay.serialize_treasury(state, &mut output)?;
    } else if let Some(overlay) = starter_oc_claim_overlay {
        overlay.serialize_treasury(state, &mut output)?;
    } else if let Some(overlay) = main_token_restricted_claim_overlay {
        overlay.serialize_treasury(state, &mut output)?;
    } else if let Some(overlay) = main_token_governance_monetary_overlay {
        overlay.serialize_treasury(state, &mut output)?;
    } else if let Some(overlay) = main_token_monetary_overlay {
        overlay.serialize_treasury(state, &mut output)?;
    } else if let Some(overlay) = node_points_settlement_overlay {
        overlay.serialize_treasury(state, &mut output)?;
    } else {
        output.serialize_field(
            "main_token_treasury_balances",
            &state.main_token_treasury_balances,
        )?;
    }
    if let Some(overlay) = main_token_monetary_overlay {
        overlay.serialize_claim_nonces(state, &mut output)?;
        overlay.serialize_transfer_nonces(state, &mut output)?;
    } else {
        output.serialize_field("main_token_claim_nonces", &state.main_token_claim_nonces)?;
        output.serialize_field(
            "main_token_transfer_nonces",
            &state.main_token_transfer_nonces,
        )?;
    }
    if let Some(overlay) = main_token_governance_monetary_overlay {
        overlay.serialize_scheduled(state, &mut output)?;
    } else {
        output.serialize_field(
            "main_token_scheduled_policy_updates",
            &state.main_token_scheduled_policy_updates,
        )?;
    }
    if let Some(overlay) = node_points_settlement_overlay {
        overlay.serialize_bridge(state, &mut output)?;
    } else {
        output.serialize_field(
            "main_token_node_points_bridge_records",
            &state.main_token_node_points_bridge_records,
        )?;
    }
    if let Some(overlay) = main_token_governance_monetary_overlay {
        overlay.serialize_distributions(state, &mut output)?;
    } else {
        output.serialize_field(
            "main_token_treasury_distribution_records",
            &state.main_token_treasury_distribution_records,
        )?;
    }
    if let Some(overlay) = main_token_restricted_claim_overlay {
        overlay.serialize_topups(state, &mut output)?;
    } else {
        output.serialize_field(
            "restricted_starter_claim_liveops_pool_top_up_records",
            &state.restricted_starter_claim_liveops_pool_top_up_records,
        )?;
    }
    output.serialize_field("reward_asset_config", &state.reward_asset_config)?;
    if let Some(overlay) = node_points_settlement_overlay {
        overlay.serialize_node_balances(state, &mut output)?;
        output.serialize_field("protocol_power_reserve", &state.protocol_power_reserve)?;
    } else if let Some(overlay) = power_redemption_overlay {
        overlay.serialize_node_balances(state, &mut output)?;
        overlay.serialize_reserve(state, &mut output)?;
    } else {
        output.serialize_field("node_asset_balances", &state.node_asset_balances)?;
        output.serialize_field("protocol_power_reserve", &state.protocol_power_reserve)?;
    }
    if let Some(overlay) = node_points_settlement_overlay {
        overlay.serialize_reward_mints(&mut output)?;
    } else {
        output.serialize_field("reward_mint_records", &state.reward_mint_records)?;
    }
    if let Some(overlay) = power_redemption_overlay {
        overlay.serialize_nonces(state, &mut output)?;
    } else {
        output.serialize_field("node_redeem_nonces", &state.node_redeem_nonces)?;
    }
    if let Some(overlay) = node_points_settlement_overlay {
        overlay.serialize_budgets(state, &mut output)?;
    } else {
        output.serialize_field(
            "system_order_pool_budgets",
            &state.system_order_pool_budgets,
        )?;
    }
    output.declare_all(&governance::ORIGINAL_GOVERNANCE_FIELD_SLOTS_V1)?;
    governance::serialize_fields(&mut output, state, governance_registry_overlay)?;
    output.end()
}

impl WorldState{
 /// Observe the original base-state fields in their exact serializer order.
 /// The adapter receives individual typed borrows and original omission
 /// decisions; this never constructs an execution state or a snapshot.
 pub fn visit_original_serialized_fields_v1<O,F>(&self,create:F)->Result<O::Ok,O::Error>
 where O:serde::ser::SerializeStruct,F:FnOnce(usize)->Result<O,O::Error>{
  emit_world_state_fields_v1(self,None,None,None,None,None,None,None,None,None,None,None,None,None,None,None,None,None,None,None,None,None,None,None,None,None,None,None,create)
 }
 pub fn visit_original_serialized_fields_with_slots_v1<O,F>(&self,create:F,slots:&mut dyn FnMut(OriginalStateFieldSlotEventV1)->Result<(),O::Error>)->Result<O::Ok,O::Error>
 where O:serde::ser::SerializeStruct,F:FnOnce(usize)->Result<O,O::Error>{
  emit_world_state_fields_with_slots_v1(self,None,None,None,None,None,None,None,None,None,None,None,None,None,None,None,None,None,None,None,None,None,None,None,None,None,None,None,create,slots)
 }

}

#[cfg(test)]
#[path="emitter_tests.rs"]
mod tests;
