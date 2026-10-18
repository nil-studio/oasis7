use super::{LlmDecisionPayload, LlmSocialStakePayload, parse_owner_spec, parse_resource_kind};
use oasis7_simulator::{Action, ModuleInstallTarget, PPM_BASE, PowerOrderSide, ResourceOwner};
use oasis7_simulator::{SocialAdjudicationDecision, SocialStake};
use std::collections::{BTreeMap, BTreeSet};

const GOVERNANCE_MIN_VOTING_WINDOW_TICKS: u64 = 1;
const GOVERNANCE_MAX_VOTING_WINDOW_TICKS: u64 = 1_440;
const GOVERNANCE_MIN_PASS_THRESHOLD_BPS: u16 = 5_000;
const GOVERNANCE_MAX_PASS_THRESHOLD_BPS: u16 = 10_000;
const GOVERNANCE_MAX_VOTE_WEIGHT: u32 = 100;
const WAR_MAX_INTENSITY: u32 = 10;

pub(super) fn parse_market_or_social_action(
    decision: &str,
    parsed: &LlmDecisionPayload,
    agent_id: &str,
) -> Option<Result<Action, String>> {
    match decision {
        "buy_power" => Some(parse_buy_power(parsed, agent_id)),
        "sell_power" => Some(parse_sell_power(parsed, agent_id)),
        "place_power_order" => Some(parse_place_power_order(parsed, agent_id)),
        "cancel_power_order" => Some(parse_cancel_power_order(parsed, agent_id)),
        "compile_module_artifact_from_source" => {
            Some(parse_compile_module_artifact_from_source(parsed, agent_id))
        }
        "deploy_module_artifact" => Some(parse_deploy_module_artifact(parsed, agent_id)),
        "install_module_from_artifact" => {
            Some(parse_install_module_from_artifact(parsed, agent_id))
        }
        "install_module_to_target_from_artifact" => Some(
            parse_install_module_to_target_from_artifact(parsed, agent_id),
        ),
        "list_module_artifact_for_sale" => {
            Some(parse_list_module_artifact_for_sale(parsed, agent_id))
        }
        "buy_module_artifact" => Some(parse_buy_module_artifact(parsed, agent_id)),
        "delist_module_artifact" => Some(parse_delist_module_artifact(parsed, agent_id)),
        "destroy_module_artifact" => Some(parse_destroy_module_artifact(parsed, agent_id)),
        "place_module_artifact_bid" => Some(parse_place_module_artifact_bid(parsed, agent_id)),
        "cancel_module_artifact_bid" => Some(parse_cancel_module_artifact_bid(parsed, agent_id)),
        "publish_social_fact" => Some(parse_publish_social_fact(parsed, agent_id)),
        "challenge_social_fact" => Some(parse_challenge_social_fact(parsed, agent_id)),
        "adjudicate_social_fact" => Some(parse_adjudicate_social_fact(parsed, agent_id)),
        "revoke_social_fact" => Some(parse_revoke_social_fact(parsed, agent_id)),
        "declare_social_edge" => Some(parse_declare_social_edge(parsed, agent_id)),
        "form_alliance" => Some(parse_form_alliance(parsed, agent_id)),
        "join_alliance" => Some(parse_join_alliance(parsed, agent_id)),
        "leave_alliance" => Some(parse_leave_alliance(parsed, agent_id)),
        "dissolve_alliance" => Some(parse_dissolve_alliance(parsed, agent_id)),
        "declare_war" => Some(parse_declare_war(parsed, agent_id)),
        "open_governance_proposal" => Some(parse_open_governance_proposal(parsed, agent_id)),
        "cast_governance_vote" => Some(parse_cast_governance_vote(parsed, agent_id)),
        "resolve_crisis" => Some(parse_resolve_crisis(parsed, agent_id)),
        "grant_meta_progress" => Some(parse_grant_meta_progress(parsed, agent_id)),
        "open_economic_contract" => Some(parse_open_economic_contract(parsed, agent_id)),
        "accept_economic_contract" => Some(parse_accept_economic_contract(parsed, agent_id)),
        "settle_economic_contract" => Some(parse_settle_economic_contract(parsed, agent_id)),
        _ => None,
    }
}

fn parse_buy_power(parsed: &LlmDecisionPayload, agent_id: &str) -> Result<Action, String> {
    let buyer = parse_required_owner(parsed.buyer.as_deref(), "buy_power", "buyer", agent_id)?;
    let seller = parse_required_owner(parsed.seller.as_deref(), "buy_power", "seller", agent_id)?;
    let amount = parse_positive_i64(parsed.amount, "buy_power", "amount")?;
    let price_per_pu =
        parse_non_negative_i64_with_default(parsed.price_per_pu, "buy_power", "price_per_pu", 0)?;
    Ok(Action::BuyPower {
        buyer,
        seller,
        amount,
        price_per_pu,
    })
}

fn parse_sell_power(parsed: &LlmDecisionPayload, agent_id: &str) -> Result<Action, String> {
    let seller = parse_required_owner(parsed.seller.as_deref(), "sell_power", "seller", agent_id)?;
    let buyer = parse_required_owner(parsed.buyer.as_deref(), "sell_power", "buyer", agent_id)?;
    let amount = parse_positive_i64(parsed.amount, "sell_power", "amount")?;
    let price_per_pu =
        parse_non_negative_i64_with_default(parsed.price_per_pu, "sell_power", "price_per_pu", 0)?;
    Ok(Action::SellPower {
        seller,
        buyer,
        amount,
        price_per_pu,
    })
}

fn parse_place_power_order(parsed: &LlmDecisionPayload, agent_id: &str) -> Result<Action, String> {
    let owner = parse_owner_or_self(parsed.owner.as_deref(), agent_id)?;
    let side_raw = parse_required_text(parsed.side.as_deref(), "place_power_order", "side")?;
    let side = parse_power_order_side(side_raw.as_str())
        .ok_or_else(|| format!("place_power_order invalid side: {side_raw}"))?;
    let amount = parse_positive_i64(parsed.amount, "place_power_order", "amount")?;
    let limit_price_per_pu = parse_non_negative_i64_with_default(
        parsed.limit_price_per_pu,
        "place_power_order",
        "limit_price_per_pu",
        0,
    )?;
    Ok(Action::PlacePowerOrder {
        owner,
        side,
        amount,
        limit_price_per_pu,
    })
}

fn parse_cancel_power_order(parsed: &LlmDecisionPayload, agent_id: &str) -> Result<Action, String> {
    let owner = parse_owner_or_self(parsed.owner.as_deref(), agent_id)?;
    let order_id = parse_positive_u64(parsed.order_id, "cancel_power_order", "order_id")?;
    Ok(Action::CancelPowerOrder { owner, order_id })
}

fn parse_compile_module_artifact_from_source(
    parsed: &LlmDecisionPayload,
    agent_id: &str,
) -> Result<Action, String> {
    let publisher_agent_id = parse_agent_identity_or_self(
        parsed.publisher.as_deref(),
        "compile_module_artifact_from_source",
        "publisher",
        agent_id,
    )?;
    let module_id = parse_required_text(
        parsed.module_id.as_deref(),
        "compile_module_artifact_from_source",
        "module_id",
    )?;
    let manifest_path = parse_required_text(
        parsed.manifest_path.as_deref(),
        "compile_module_artifact_from_source",
        "manifest_path",
    )?;
    let source_files = parse_source_files(
        parsed.source_files.as_ref(),
        "compile_module_artifact_from_source",
        "source_files",
    )?;
    Ok(Action::CompileModuleArtifactFromSource {
        publisher_agent_id,
        module_id,
        manifest_path,
        source_files,
    })
}

fn parse_deploy_module_artifact(
    parsed: &LlmDecisionPayload,
    agent_id: &str,
) -> Result<Action, String> {
    let publisher_agent_id = parse_agent_identity_or_self(
        parsed.publisher.as_deref(),
        "deploy_module_artifact",
        "publisher",
        agent_id,
    )?;
    let wasm_hash = parse_required_text(
        parsed.wasm_hash.as_deref(),
        "deploy_module_artifact",
        "wasm_hash",
    )?;
    let wasm_bytes = parse_hex_bytes(
        parsed.wasm_bytes_hex.as_deref(),
        "deploy_module_artifact",
        "wasm_bytes_hex",
    )?;
    let module_id_hint = parsed
        .module_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    Ok(Action::DeployModuleArtifact {
        publisher_agent_id,
        wasm_hash,
        wasm_bytes,
        module_id_hint,
    })
}

fn parse_install_module_from_artifact(
    parsed: &LlmDecisionPayload,
    agent_id: &str,
) -> Result<Action, String> {
    let installer_agent_id = parse_agent_identity_or_self(
        parsed.installer.as_deref(),
        "install_module_from_artifact",
        "installer",
        agent_id,
    )?;
    let module_id = parse_required_text(
        parsed.module_id.as_deref(),
        "install_module_from_artifact",
        "module_id",
    )?;
    let module_version = parsed
        .module_version
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("0.1.0")
        .to_string();
    let wasm_hash = parse_required_text(
        parsed.wasm_hash.as_deref(),
        "install_module_from_artifact",
        "wasm_hash",
    )?;
    let activate = parsed.activate.unwrap_or(true);
    Ok(Action::InstallModuleFromArtifact {
        installer_agent_id,
        module_id,
        module_version,
        wasm_hash,
        activate,
    })
}

fn parse_install_module_to_target_from_artifact(
    parsed: &LlmDecisionPayload,
    agent_id: &str,
) -> Result<Action, String> {
    let installer_agent_id = parse_agent_identity_or_self(
        parsed.installer.as_deref(),
        "install_module_to_target_from_artifact",
        "installer",
        agent_id,
    )?;
    let module_id = parse_required_text(
        parsed.module_id.as_deref(),
        "install_module_to_target_from_artifact",
        "module_id",
    )?;
    let module_version = parsed
        .module_version
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("0.1.0")
        .to_string();
    let wasm_hash = parse_required_text(
        parsed.wasm_hash.as_deref(),
        "install_module_to_target_from_artifact",
        "wasm_hash",
    )?;
    let install_target = parse_install_target(
        parsed.install_target_type.as_deref(),
        parsed.install_target_location_id.as_deref(),
    )?;
    let activate = parsed.activate.unwrap_or(true);
    Ok(Action::InstallModuleToTargetFromArtifact {
        installer_agent_id,
        module_id,
        module_version,
        wasm_hash,
        activate,
        install_target,
    })
}

fn parse_install_target(
    target_type: Option<&str>,
    target_location_id: Option<&str>,
) -> Result<ModuleInstallTarget, String> {
    let normalized_type = target_type
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("self_agent");
    match normalized_type {
        "self_agent" => {
            if target_location_id
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .is_some()
            {
                return Err(
                    "install_module_to_target_from_artifact install_target_location_id must be empty when install_target_type=self_agent".to_string(),
                );
            }
            Ok(ModuleInstallTarget::SelfAgent)
        }
        "location_infrastructure" => {
            let location_id = parse_required_text(
                target_location_id,
                "install_module_to_target_from_artifact",
                "install_target_location_id",
            )?;
            Ok(ModuleInstallTarget::LocationInfrastructure { location_id })
        }
        other => Err(format!(
            "install_module_to_target_from_artifact invalid install_target_type: {other}"
        )),
    }
}

fn parse_list_module_artifact_for_sale(
    parsed: &LlmDecisionPayload,
    agent_id: &str,
) -> Result<Action, String> {
    let seller_agent_id = parse_agent_identity_or_self(
        parsed.seller.as_deref(),
        "list_module_artifact_for_sale",
        "seller",
        agent_id,
    )?;
    let wasm_hash = parse_required_text(
        parsed.wasm_hash.as_deref(),
        "list_module_artifact_for_sale",
        "wasm_hash",
    )?;
    let price_kind = parse_resource_kind(
        parse_required_text(
            parsed.price_kind.as_deref(),
            "list_module_artifact_for_sale",
            "price_kind",
        )?
        .as_str(),
    )
    .ok_or_else(|| "list_module_artifact_for_sale invalid price_kind".to_string())?;
    let price_amount = parse_positive_i64(
        parsed.price_amount,
        "list_module_artifact_for_sale",
        "price_amount",
    )?;
    Ok(Action::ListModuleArtifactForSale {
        seller_agent_id,
        wasm_hash,
        price_kind,
        price_amount,
    })
}

fn parse_buy_module_artifact(
    parsed: &LlmDecisionPayload,
    agent_id: &str,
) -> Result<Action, String> {
    let buyer_agent_id = parse_agent_identity_or_self(
        parsed.buyer.as_deref(),
        "buy_module_artifact",
        "buyer",
        agent_id,
    )?;
    let wasm_hash = parse_required_text(
        parsed.wasm_hash.as_deref(),
        "buy_module_artifact",
        "wasm_hash",
    )?;
    Ok(Action::BuyModuleArtifact {
        buyer_agent_id,
        wasm_hash,
    })
}

fn parse_delist_module_artifact(
    parsed: &LlmDecisionPayload,
    agent_id: &str,
) -> Result<Action, String> {
    let seller_agent_id = parse_agent_identity_or_self(
        parsed.seller.as_deref(),
        "delist_module_artifact",
        "seller",
        agent_id,
    )?;
    let wasm_hash = parse_required_text(
        parsed.wasm_hash.as_deref(),
        "delist_module_artifact",
        "wasm_hash",
    )?;
    Ok(Action::DelistModuleArtifact {
        seller_agent_id,
        wasm_hash,
    })
}

fn parse_destroy_module_artifact(
    parsed: &LlmDecisionPayload,
    agent_id: &str,
) -> Result<Action, String> {
    let owner_agent_id = parse_agent_identity_or_self(
        parsed.owner.as_deref(),
        "destroy_module_artifact",
        "owner",
        agent_id,
    )?;
    let wasm_hash = parse_required_text(
        parsed.wasm_hash.as_deref(),
        "destroy_module_artifact",
        "wasm_hash",
    )?;
    let reason = parse_required_text(
        parsed.reason.as_deref(),
        "destroy_module_artifact",
        "reason",
    )?;
    Ok(Action::DestroyModuleArtifact {
        owner_agent_id,
        wasm_hash,
        reason,
    })
}

fn parse_place_module_artifact_bid(
    parsed: &LlmDecisionPayload,
    agent_id: &str,
) -> Result<Action, String> {
    let bidder_agent_id = parse_agent_identity_or_self(
        parsed.bidder.as_deref(),
        "place_module_artifact_bid",
        "bidder",
        agent_id,
    )?;
    let wasm_hash = parse_required_text(
        parsed.wasm_hash.as_deref(),
        "place_module_artifact_bid",
        "wasm_hash",
    )?;
    let price_kind = parse_resource_kind(
        parse_required_text(
            parsed.price_kind.as_deref(),
            "place_module_artifact_bid",
            "price_kind",
        )?
        .as_str(),
    )
    .ok_or_else(|| "place_module_artifact_bid invalid price_kind".to_string())?;
    let price_amount = parse_positive_i64(
        parsed.price_amount,
        "place_module_artifact_bid",
        "price_amount",
    )?;
    Ok(Action::PlaceModuleArtifactBid {
        bidder_agent_id,
        wasm_hash,
        price_kind,
        price_amount,
    })
}

fn parse_cancel_module_artifact_bid(
    parsed: &LlmDecisionPayload,
    agent_id: &str,
) -> Result<Action, String> {
    let bidder_agent_id = parse_agent_identity_or_self(
        parsed.bidder.as_deref(),
        "cancel_module_artifact_bid",
        "bidder",
        agent_id,
    )?;
    let wasm_hash = parse_required_text(
        parsed.wasm_hash.as_deref(),
        "cancel_module_artifact_bid",
        "wasm_hash",
    )?;
    let bid_order_id = parse_positive_u64(
        parsed.bid_order_id,
        "cancel_module_artifact_bid",
        "bid_order_id",
    )?;
    Ok(Action::CancelModuleArtifactBid {
        bidder_agent_id,
        wasm_hash,
        bid_order_id,
    })
}

fn parse_publish_social_fact(
    parsed: &LlmDecisionPayload,
    agent_id: &str,
) -> Result<Action, String> {
    let actor = parse_owner_or_self(parsed.actor.as_deref(), agent_id)?;
    let schema_id = parse_required_text(
        parsed.schema_id.as_deref(),
        "publish_social_fact",
        "schema_id",
    )?;
    let subject = parse_required_owner(
        parsed.subject.as_deref(),
        "publish_social_fact",
        "subject",
        agent_id,
    )?;
    let object = match parsed.object.as_deref() {
        Some(owner) => Some(parse_owner_spec(owner, agent_id)?),
        None => None,
    };
    let claim = parse_required_text(parsed.claim.as_deref(), "publish_social_fact", "claim")?;
    let confidence_ppm = parsed.confidence_ppm.unwrap_or(PPM_BASE);
    if !(1..=PPM_BASE).contains(&confidence_ppm) {
        return Err(format!(
            "publish_social_fact confidence_ppm out of range: {confidence_ppm} (expected 1..={PPM_BASE})"
        ));
    }
    let evidence_event_ids = parse_positive_u64_list(
        parsed.evidence_event_ids.as_deref(),
        "publish_social_fact",
        "evidence_event_ids",
    )?;
    let ttl_ticks =
        parse_optional_positive_u64(parsed.ttl_ticks, "publish_social_fact", "ttl_ticks")?;
    let stake = parse_social_stake(parsed.stake.as_ref(), "publish_social_fact")?;
    Ok(Action::PublishSocialFact {
        actor,
        schema_id,
        subject,
        object,
        claim,
        confidence_ppm,
        evidence_event_ids,
        ttl_ticks,
        stake,
    })
}

fn parse_challenge_social_fact(
    parsed: &LlmDecisionPayload,
    agent_id: &str,
) -> Result<Action, String> {
    let challenger = parse_owner_or_self(parsed.challenger.as_deref(), agent_id)?;
    let fact_id = parse_positive_u64(parsed.fact_id, "challenge_social_fact", "fact_id")?;
    let reason = parse_required_text(parsed.reason.as_deref(), "challenge_social_fact", "reason")?;
    let stake = parse_social_stake(parsed.stake.as_ref(), "challenge_social_fact")?;
    Ok(Action::ChallengeSocialFact {
        challenger,
        fact_id,
        reason,
        stake,
    })
}

fn parse_adjudicate_social_fact(
    parsed: &LlmDecisionPayload,
    agent_id: &str,
) -> Result<Action, String> {
    let adjudicator = parse_owner_or_self(parsed.adjudicator.as_deref(), agent_id)?;
    let fact_id = parse_positive_u64(parsed.fact_id, "adjudicate_social_fact", "fact_id")?;
    let adjudication_raw = parse_required_text(
        parsed.adjudication.as_deref(),
        "adjudicate_social_fact",
        "adjudication",
    )?;
    let decision =
        parse_social_adjudication_decision(adjudication_raw.as_str()).ok_or_else(|| {
            format!("adjudicate_social_fact invalid adjudication: {adjudication_raw}")
        })?;
    let notes = parse_required_text(parsed.notes.as_deref(), "adjudicate_social_fact", "notes")?;
    Ok(Action::AdjudicateSocialFact {
        adjudicator,
        fact_id,
        decision,
        notes,
    })
}

fn parse_revoke_social_fact(parsed: &LlmDecisionPayload, agent_id: &str) -> Result<Action, String> {
    let actor = parse_owner_or_self(parsed.actor.as_deref(), agent_id)?;
    let fact_id = parse_positive_u64(parsed.fact_id, "revoke_social_fact", "fact_id")?;
    let reason = parse_required_text(parsed.reason.as_deref(), "revoke_social_fact", "reason")?;
    Ok(Action::RevokeSocialFact {
        actor,
        fact_id,
        reason,
    })
}

fn parse_declare_social_edge(
    parsed: &LlmDecisionPayload,
    agent_id: &str,
) -> Result<Action, String> {
    let declarer = parse_owner_or_self(parsed.declarer.as_deref(), agent_id)?;
    let schema_id = parse_required_text(
        parsed.schema_id.as_deref(),
        "declare_social_edge",
        "schema_id",
    )?;
    let relation_kind = parse_required_text(
        parsed.relation_kind.as_deref(),
        "declare_social_edge",
        "relation_kind",
    )?;
    let from = parse_required_owner(
        parsed.from.as_deref(),
        "declare_social_edge",
        "from",
        agent_id,
    )?;
    let to = parse_required_owner(parsed.to.as_deref(), "declare_social_edge", "to", agent_id)?;
    let weight_bps = parsed.weight_bps.unwrap_or(0);
    if !(-10_000..=10_000).contains(&weight_bps) {
        return Err(format!(
            "declare_social_edge weight_bps out of range: {weight_bps} (expected -10000..=10000)"
        ));
    }
    let backing_fact_ids = parse_positive_u64_list(
        parsed.backing_fact_ids.as_deref(),
        "declare_social_edge",
        "backing_fact_ids",
    )?;
    let ttl_ticks =
        parse_optional_positive_u64(parsed.ttl_ticks, "declare_social_edge", "ttl_ticks")?;
    Ok(Action::DeclareSocialEdge {
        declarer,
        schema_id,
        relation_kind,
        from,
        to,
        weight_bps,
        backing_fact_ids,
        ttl_ticks,
    })
}

fn parse_form_alliance(parsed: &LlmDecisionPayload, agent_id: &str) -> Result<Action, String> {
    let proposer_agent_id = parse_agent_identity_or_self(
        parsed.proposer_agent_id.as_deref(),
        "form_alliance",
        "proposer_agent_id",
        agent_id,
    )?;
    let alliance_id = parse_required_text(
        parsed.alliance_id.as_deref(),
        "form_alliance",
        "alliance_id",
    )?;
    let charter = parse_required_text(parsed.charter.as_deref(), "form_alliance", "charter")?;
    let members = parse_required_agent_identity_list(
        parsed.members.as_deref(),
        "form_alliance",
        "members",
        agent_id,
    )?;

    let mut normalized_members = BTreeSet::new();
    normalized_members.insert(proposer_agent_id.clone());
    normalized_members.extend(members);
    if normalized_members.len() < 2 {
        return Err("form_alliance requires at least 2 unique members".to_string());
    }

    Ok(Action::FormAlliance {
        proposer_agent_id,
        alliance_id,
        members: normalized_members.into_iter().collect(),
        charter,
    })
}

fn parse_join_alliance(parsed: &LlmDecisionPayload, agent_id: &str) -> Result<Action, String> {
    let operator_agent_id = parse_agent_identity_or_self(
        parsed.operator_agent_id.as_deref(),
        "join_alliance",
        "operator_agent_id",
        agent_id,
    )?;
    let alliance_id = parse_required_text(
        parsed.alliance_id.as_deref(),
        "join_alliance",
        "alliance_id",
    )?;
    let member_agent_id = parse_agent_identity_or_self(
        parsed
            .member_agent_id
            .as_deref()
            .or(parsed.operator_agent_id.as_deref()),
        "join_alliance",
        "member_agent_id",
        agent_id,
    )?;

    Ok(Action::JoinAlliance {
        operator_agent_id,
        alliance_id,
        member_agent_id,
    })
}

fn parse_leave_alliance(parsed: &LlmDecisionPayload, agent_id: &str) -> Result<Action, String> {
    let operator_agent_id = parse_agent_identity_or_self(
        parsed.operator_agent_id.as_deref(),
        "leave_alliance",
        "operator_agent_id",
        agent_id,
    )?;
    let alliance_id = parse_required_text(
        parsed.alliance_id.as_deref(),
        "leave_alliance",
        "alliance_id",
    )?;
    let member_agent_id = parse_agent_identity_or_self(
        parsed
            .member_agent_id
            .as_deref()
            .or(parsed.operator_agent_id.as_deref()),
        "leave_alliance",
        "member_agent_id",
        agent_id,
    )?;

    Ok(Action::LeaveAlliance {
        operator_agent_id,
        alliance_id,
        member_agent_id,
    })
}

fn parse_dissolve_alliance(parsed: &LlmDecisionPayload, agent_id: &str) -> Result<Action, String> {
    let operator_agent_id = parse_agent_identity_or_self(
        parsed.operator_agent_id.as_deref(),
        "dissolve_alliance",
        "operator_agent_id",
        agent_id,
    )?;
    let alliance_id = parse_required_text(
        parsed.alliance_id.as_deref(),
        "dissolve_alliance",
        "alliance_id",
    )?;
    let reason = parse_required_text(parsed.reason.as_deref(), "dissolve_alliance", "reason")?;
    Ok(Action::DissolveAlliance {
        operator_agent_id,
        alliance_id,
        reason,
    })
}

fn parse_declare_war(parsed: &LlmDecisionPayload, agent_id: &str) -> Result<Action, String> {
    let initiator_agent_id = parse_agent_identity_or_self(
        parsed.initiator_agent_id.as_deref(),
        "declare_war",
        "initiator_agent_id",
        agent_id,
    )?;
    let war_id = parse_required_text(parsed.war_id.as_deref(), "declare_war", "war_id")?;
    let aggressor_alliance_id = parse_required_text(
        parsed.aggressor_alliance_id.as_deref(),
        "declare_war",
        "aggressor_alliance_id",
    )?;
    let defender_alliance_id = parse_required_text(
        parsed.defender_alliance_id.as_deref(),
        "declare_war",
        "defender_alliance_id",
    )?;
    let objective = parse_required_text(parsed.objective.as_deref(), "declare_war", "objective")?;
    let intensity = parsed.intensity.unwrap_or(1);
    if intensity == 0 {
        return Err("declare_war intensity must be > 0".to_string());
    }
    if intensity > WAR_MAX_INTENSITY {
        return Err(format!(
            "declare_war intensity exceeds max {}",
            WAR_MAX_INTENSITY
        ));
    }

    Ok(Action::DeclareWar {
        initiator_agent_id,
        war_id,
        aggressor_alliance_id,
        defender_alliance_id,
        objective,
        intensity,
    })
}

fn parse_open_governance_proposal(
    parsed: &LlmDecisionPayload,
    agent_id: &str,
) -> Result<Action, String> {
    let proposer_agent_id = parse_agent_identity_or_self(
        parsed.proposer_agent_id.as_deref(),
        "open_governance_proposal",
        "proposer_agent_id",
        agent_id,
    )?;
    let proposal_key = parse_required_text(
        parsed.proposal_key.as_deref(),
        "open_governance_proposal",
        "proposal_key",
    )?;
    let title = parse_required_text(parsed.title.as_deref(), "open_governance_proposal", "title")?;
    let description = parse_required_text(
        parsed.description.as_deref(),
        "open_governance_proposal",
        "description",
    )?;
    let options = parse_required_text_list(
        parsed.options.as_deref(),
        "open_governance_proposal",
        "options",
    )?;
    let voting_window_ticks = parsed.voting_window_ticks.unwrap_or(32);
    if !(GOVERNANCE_MIN_VOTING_WINDOW_TICKS..=GOVERNANCE_MAX_VOTING_WINDOW_TICKS)
        .contains(&voting_window_ticks)
    {
        return Err(format!(
            "open_governance_proposal voting_window_ticks must be within {}..={}",
            GOVERNANCE_MIN_VOTING_WINDOW_TICKS, GOVERNANCE_MAX_VOTING_WINDOW_TICKS
        ));
    }
    let quorum_weight = parsed.quorum_weight.unwrap_or(1);
    if quorum_weight == 0 {
        return Err("open_governance_proposal quorum_weight must be > 0".to_string());
    }
    let pass_threshold_bps = parsed.pass_threshold_bps.unwrap_or(6_000);
    if !(GOVERNANCE_MIN_PASS_THRESHOLD_BPS..=GOVERNANCE_MAX_PASS_THRESHOLD_BPS)
        .contains(&pass_threshold_bps)
    {
        return Err(format!(
            "open_governance_proposal pass_threshold_bps must be within {}..={}",
            GOVERNANCE_MIN_PASS_THRESHOLD_BPS, GOVERNANCE_MAX_PASS_THRESHOLD_BPS
        ));
    }

    let mut unique_options = BTreeSet::new();
    for option in options {
        unique_options.insert(option);
    }
    if unique_options.len() < 2 {
        return Err("open_governance_proposal requires at least 2 unique options".to_string());
    }

    Ok(Action::OpenGovernanceProposal {
        proposer_agent_id,
        proposal_key,
        title,
        description,
        options: unique_options.into_iter().collect(),
        voting_window_ticks,
        quorum_weight,
        pass_threshold_bps,
    })
}

fn parse_cast_governance_vote(
    parsed: &LlmDecisionPayload,
    agent_id: &str,
) -> Result<Action, String> {
    let voter_agent_id = parse_agent_identity_or_self(
        parsed.voter_agent_id.as_deref(),
        "cast_governance_vote",
        "voter_agent_id",
        agent_id,
    )?;
    let proposal_key = parse_required_text(
        parsed.proposal_key.as_deref(),
        "cast_governance_vote",
        "proposal_key",
    )?;
    let option = parse_required_text(parsed.option.as_deref(), "cast_governance_vote", "option")?;
    let weight = parsed.weight.unwrap_or(1);
    if weight == 0 {
        return Err("cast_governance_vote weight must be > 0".to_string());
    }
    if weight > GOVERNANCE_MAX_VOTE_WEIGHT {
        return Err(format!(
            "cast_governance_vote weight must be within 1..={}",
            GOVERNANCE_MAX_VOTE_WEIGHT
        ));
    }

    Ok(Action::CastGovernanceVote {
        voter_agent_id,
        proposal_key,
        option,
        weight,
    })
}

fn parse_resolve_crisis(parsed: &LlmDecisionPayload, agent_id: &str) -> Result<Action, String> {
    let resolver_agent_id = parse_agent_identity_or_self(
        parsed.resolver_agent_id.as_deref(),
        "resolve_crisis",
        "resolver_agent_id",
        agent_id,
    )?;
    let crisis_id =
        parse_required_text(parsed.crisis_id.as_deref(), "resolve_crisis", "crisis_id")?;
    let strategy = parse_required_text(parsed.strategy.as_deref(), "resolve_crisis", "strategy")?;
    let success = parsed.success.unwrap_or(true);

    Ok(Action::ResolveCrisis {
        resolver_agent_id,
        crisis_id,
        strategy,
        success,
    })
}

fn parse_grant_meta_progress(
    parsed: &LlmDecisionPayload,
    agent_id: &str,
) -> Result<Action, String> {
    let operator_agent_id = parse_agent_identity_or_self(
        parsed.operator_agent_id.as_deref(),
        "grant_meta_progress",
        "operator_agent_id",
        agent_id,
    )?;
    let target_agent_id = parse_agent_identity_or_self(
        parsed.target_agent_id.as_deref(),
        "grant_meta_progress",
        "target_agent_id",
        agent_id,
    )?;
    let track = parse_required_text(parsed.track.as_deref(), "grant_meta_progress", "track")?;
    let points = parsed
        .points
        .ok_or_else(|| "grant_meta_progress missing `points`".to_string())?;
    if points == 0 {
        return Err("grant_meta_progress points must be non-zero".to_string());
    }
    let achievement_id = parse_optional_non_empty_text(
        parsed.achievement_id.as_deref(),
        "grant_meta_progress",
        "achievement_id",
    )?;

    Ok(Action::GrantMetaProgress {
        operator_agent_id,
        target_agent_id,
        track,
        points,
        achievement_id,
    })
}

fn parse_open_economic_contract(
    parsed: &LlmDecisionPayload,
    agent_id: &str,
) -> Result<Action, String> {
    let creator_agent_id = parse_agent_identity_or_self(
        parsed.creator_agent_id.as_deref(),
        "open_economic_contract",
        "creator_agent_id",
        agent_id,
    )?;
    let contract_id = parse_required_text(
        parsed.contract_id.as_deref(),
        "open_economic_contract",
        "contract_id",
    )?;
    let counterparty_agent_id = parse_agent_identity_or_self(
        parsed.counterparty_agent_id.as_deref(),
        "open_economic_contract",
        "counterparty_agent_id",
        agent_id,
    )?;
    let settlement_kind_raw = parse_required_text(
        parsed.settlement_kind.as_deref(),
        "open_economic_contract",
        "settlement_kind",
    )?;
    let settlement_kind = parse_resource_kind(settlement_kind_raw.as_str()).ok_or_else(|| {
        format!("open_economic_contract invalid settlement_kind: {settlement_kind_raw}")
    })?;
    let settlement_amount = parse_positive_i64(
        parsed.settlement_amount,
        "open_economic_contract",
        "settlement_amount",
    )?;
    let reputation_stake = parse_positive_i64(
        parsed.reputation_stake,
        "open_economic_contract",
        "reputation_stake",
    )?;
    let expires_at = parse_positive_u64(parsed.expires_at, "open_economic_contract", "expires_at")?;
    let description = parse_required_text(
        parsed.description.as_deref(),
        "open_economic_contract",
        "description",
    )?;

    Ok(Action::OpenEconomicContract {
        creator_agent_id,
        contract_id,
        counterparty_agent_id,
        settlement_kind,
        settlement_amount,
        reputation_stake,
        expires_at,
        description,
    })
}
