use crate::social::{
    SocialAdjudicationDecision, SocialChallengeState, SocialEdgeLifecycleState, SocialEdgeState,
    SocialFactImpactQuote, SocialFactLifecycleState, SocialFactState, SocialStake,
};

use super::super::types::{PPM_BASE, ResourceOwner, WorldEventId, WorldTime};
use super::WorldKernel;
use super::types::{RejectReason, WorldEventKind};

mod stake_returns;

const EDGE_EXPIRE_REASON_TTL: &str = "ttl_expired";
const EDGE_EXPIRE_REASON_BACKING_FACT_INACTIVE: &str = "backing_fact_inactive";

impl WorldKernel {
    #[expect(clippy::too_many_arguments, reason = "stable social seam")]
    pub fn quote_publish_social_fact(
        &self,
        actor: &ResourceOwner,
        schema_id: &str,
        subject: &ResourceOwner,
        object: Option<&ResourceOwner>,
        claim: &str,
        confidence_ppm: i64,
        evidence_event_ids: &[WorldEventId],
        ttl_ticks: Option<u64>,
        stake: Option<&SocialStake>,
    ) -> Result<SocialFactImpactQuote, RejectReason> {
        self.ensure_owner_exists(actor)?;
        self.ensure_owner_exists(subject)?;
        if let Some(owner) = object {
            self.ensure_owner_exists(owner)?;
        }

        let schema_id = schema_id.trim();
        if schema_id.is_empty() {
            return social_rule_denied("social schema_id cannot be empty");
        }
        let claim = claim.trim();
        if claim.is_empty() {
            return social_rule_denied("social claim cannot be empty");
        }
        if !(1..=PPM_BASE).contains(&confidence_ppm) {
            return Err(RejectReason::InvalidAmount {
                amount: confidence_ppm,
            });
        }
        if evidence_event_ids.is_empty() {
            return social_rule_denied("social evidence_event_ids cannot be empty");
        }
        for event_id in evidence_event_ids {
            if !self.has_journal_event(*event_id) {
                return social_rule_denied(format!("social evidence event missing: {event_id}"));
            }
        }
        if ttl_ticks.is_some_and(|ticks| ticks == 0) {
            return Err(RejectReason::InvalidAmount { amount: 0 });
        }
        self.ensure_social_stake_available(actor, stake)?;

        Ok(SocialFactImpactQuote {
            actor_id: social_owner_quote_id(actor),
            action_kind: "publish_social_fact".to_string(),
            schema_id: schema_id.to_string(),
            subject_id: Some(social_owner_quote_id(subject)),
            object_id: object.map(social_owner_quote_id),
            claim_summary: summarize_social_text(claim),
            confidence_ppm: Some(confidence_ppm),
            stake_at_risk: stake.map(|stake| stake.amount).unwrap_or(0),
            ttl_ticks,
            affected_relationships: social_affected_relationships(schema_id, subject, object),
            affected_social_surfaces: social_affected_surfaces(schema_id),
            cooperation_opportunity_delta: "positive".to_string(),
            blacklist_or_dispute_risk: if stake.is_some() {
                "stake_at_risk"
            } else {
                "challengeable_claim"
            }
            .to_string(),
            governance_or_claim_relevance: "evidence_backed_claim".to_string(),
            recommended_social_action: "publish_fact".to_string(),
            why_this_action_matters: format!(
                "Publishing this claim makes {} visible for cooperation, dispute, and governance decisions.",
                social_owner_quote_id(subject)
            ),
        })
    }

    pub fn quote_challenge_social_fact(
        &self,
        challenger: &ResourceOwner,
        fact_id: u64,
        reason: &str,
        stake: Option<&SocialStake>,
    ) -> Result<SocialFactImpactQuote, RejectReason> {
        self.ensure_owner_exists(challenger)?;
        let reason = reason.trim();
        if reason.is_empty() {
            return social_rule_denied("social challenge reason cannot be empty");
        }
        self.ensure_social_stake_available(challenger, stake)?;

        let Some(fact) = self.model.social_facts.get(&fact_id) else {
            return social_rule_denied(format!("social fact not found: {fact_id}"));
        };
        if !matches!(
            fact.lifecycle,
            SocialFactLifecycleState::Active | SocialFactLifecycleState::Confirmed
        ) {
            return social_rule_denied(format!(
                "social fact {fact_id} cannot be challenged in state {:?}",
                fact.lifecycle
            ));
        }
        if fact.challenge.is_some() {
            return social_rule_denied(format!("social fact {fact_id} already challenged"));
        }

        Ok(SocialFactImpactQuote {
            actor_id: social_owner_quote_id(challenger),
            action_kind: "challenge_social_fact".to_string(),
            schema_id: fact.schema_id.clone(),
            subject_id: Some(social_owner_quote_id(&fact.subject)),
            object_id: fact.object.as_ref().map(social_owner_quote_id),
            claim_summary: summarize_social_text(reason),
            confidence_ppm: Some(fact.confidence_ppm),
            stake_at_risk: stake.map(|stake| stake.amount).unwrap_or(0),
            ttl_ticks: fact.ttl_ticks,
            affected_relationships: social_affected_relationships(
                &fact.schema_id,
                &fact.subject,
                fact.object.as_ref(),
            ),
            affected_social_surfaces: social_affected_surfaces(&fact.schema_id),
            cooperation_opportunity_delta: "contested".to_string(),
            blacklist_or_dispute_risk: "opens_dispute".to_string(),
            governance_or_claim_relevance: "adjudication_relevant".to_string(),
            recommended_social_action: "challenge_fact".to_string(),
            why_this_action_matters: "Challenging this fact opens a dispute before the claim is reused by relationships or governance."
                .to_string(),
        })
    }

    pub fn quote_adjudicate_social_fact(
        &self,
        adjudicator: &ResourceOwner,
        fact_id: u64,
        decision: SocialAdjudicationDecision,
        notes: &str,
    ) -> Result<SocialFactImpactQuote, RejectReason> {
        // Keep the quote validation in the same order as the production action:
        // owner, notes, fact, challenge, and party authorization.  The quote
        // only borrows state, so it cannot consume or settle either stake.
        self.ensure_owner_exists(adjudicator)?;
        let notes = notes.trim();
        if notes.is_empty() {
            return social_rule_denied("social adjudication notes cannot be empty");
        }

        let Some(fact) = self.model.social_facts.get(&fact_id) else {
            return social_rule_denied(format!("social fact not found: {fact_id}"));
        };
        let Some(challenge) = fact.challenge.as_ref() else {
            return social_rule_denied(format!(
                "social fact {fact_id} cannot be adjudicated without challenge"
            ));
        };
        if !social_adjudicator_is_authorized(fact, adjudicator) {
            return social_rule_denied(format!(
                "social adjudicator is not the fact publisher for fact {fact_id}"
            ));
        }

        let actor_stake = fact.stake.as_ref().map(|stake| stake.amount).unwrap_or(0);
        let challenger_stake = challenge
            .stake
            .as_ref()
            .map(|stake| stake.amount)
            .unwrap_or(0);
        let (stake_at_risk, cooperation_opportunity_delta, recommended_social_action, outcome) =
            match decision {
                SocialAdjudicationDecision::Confirm => (
                    challenger_stake,
                    "confirmed",
                    "confirm_fact",
                    format!(
                        "Confirming this fact slashes the challenger stake ({challenger_stake}) to the stake pool and returns the publisher stake ({actor_stake})."
                    ),
                ),
                SocialAdjudicationDecision::Retract => (
                    actor_stake,
                    "retracted",
                    "retract_fact",
                    format!(
                        "Retracting this fact slashes the publisher stake ({actor_stake}) to the stake pool and returns the challenger stake ({challenger_stake})."
                    ),
                ),
            };

        Ok(SocialFactImpactQuote {
            actor_id: social_owner_quote_id(adjudicator),
            action_kind: "adjudicate_fact".to_string(),
            schema_id: fact.schema_id.clone(),
            subject_id: Some(social_owner_quote_id(&fact.subject)),
            object_id: fact.object.as_ref().map(social_owner_quote_id),
            claim_summary: summarize_social_text(&fact.claim),
            confidence_ppm: Some(fact.confidence_ppm),
            stake_at_risk,
            ttl_ticks: fact.ttl_ticks,
            affected_relationships: social_affected_relationships(
                &fact.schema_id,
                &fact.subject,
                fact.object.as_ref(),
            ),
            affected_social_surfaces: social_affected_surfaces(&fact.schema_id),
            cooperation_opportunity_delta: cooperation_opportunity_delta.to_string(),
            blacklist_or_dispute_risk: "adjudication_settlement".to_string(),
            governance_or_claim_relevance: "adjudication_relevant".to_string(),
            recommended_social_action: recommended_social_action.to_string(),
            why_this_action_matters: outcome,
        })
    }

    pub fn quote_revoke_social_fact(
        &self,
        actor: &ResourceOwner,
        fact_id: u64,
        reason: &str,
    ) -> Result<SocialFactImpactQuote, RejectReason> {
        self.ensure_owner_exists(actor)?;
        let reason = reason.trim();
        if reason.is_empty() {
            return social_rule_denied("social revoke reason cannot be empty");
        }

        let Some(fact) = self.model.social_facts.get(&fact_id) else {
            return social_rule_denied(format!("social fact not found: {fact_id}"));
        };
        if fact.actor != *actor {
            return social_rule_denied(format!(
                "social fact {fact_id} can only be revoked by publisher"
            ));
        }
        if matches!(
            fact.lifecycle,
            SocialFactLifecycleState::Retracted
                | SocialFactLifecycleState::Revoked
                | SocialFactLifecycleState::Expired
        ) {
            return social_rule_denied(format!(
                "social fact {fact_id} cannot be revoked in state {:?}",
                fact.lifecycle
            ));
        }
        self.ensure_social_fact_stake_returns_fit(fact)?;

        let publisher_stake_return = fact.stake.as_ref().map(|stake| stake.amount).unwrap_or(0);
        let challenger_stake_return = fact
            .challenge
            .as_ref()
            .and_then(|challenge| challenge.stake.as_ref())
            .map(|stake| stake.amount)
            .unwrap_or(0);
        let mut affected_relationships =
            social_affected_relationships(&fact.schema_id, &fact.subject, fact.object.as_ref());
        let mut active_edge_ids = Vec::new();
        let mut inactive_edge_ids = Vec::new();
        for edge in self.model.social_edges.values() {
            if !edge.backing_fact_ids.contains(&fact_id) {
                continue;
            }
            affected_relationships.push(format!("edge:{}", edge.edge_id));
            if edge.is_active() {
                active_edge_ids.push(edge.edge_id.to_string());
            } else {
                inactive_edge_ids.push(edge.edge_id.to_string());
            }
        }

        let mut affected_social_surfaces = social_affected_surfaces(&fact.schema_id);
        if (!active_edge_ids.is_empty() || !inactive_edge_ids.is_empty())
            && !affected_social_surfaces
                .iter()
                .any(|surface| surface == "relationship")
        {
            affected_social_surfaces.push("relationship".to_string());
        }
        let active_edges = if active_edge_ids.is_empty() {
            "none".to_string()
        } else {
            active_edge_ids.join(", ")
        };
        let inactive_edges = if inactive_edge_ids.is_empty() {
            "none".to_string()
        } else {
            inactive_edge_ids.join(", ")
        };
        let object_summary = fact
            .object
            .as_ref()
            .map(social_owner_quote_id)
            .unwrap_or_else(|| "no object".to_string());

        Ok(SocialFactImpactQuote {
            actor_id: social_owner_quote_id(actor),
            action_kind: "revoke_fact".to_string(),
            schema_id: fact.schema_id.clone(),
            subject_id: Some(social_owner_quote_id(&fact.subject)),
            object_id: fact.object.as_ref().map(social_owner_quote_id),
            claim_summary: summarize_social_text(&fact.claim),
            confidence_ppm: Some(fact.confidence_ppm),
            stake_at_risk: 0,
            ttl_ticks: fact.ttl_ticks,
            affected_relationships,
            affected_social_surfaces,
            cooperation_opportunity_delta: "withdraws_relationship_support".to_string(),
            blacklist_or_dispute_risk: "relationship_withdrawal".to_string(),
            governance_or_claim_relevance: "withdraws_evidence_backed_claim".to_string(),
            recommended_social_action: "revoke_fact".to_string(),
            why_this_action_matters: format!(
                "Revoking the claim about {} and {} for reason '{}' withdraws active backing edges [{}], while already inactive edges [{}] remain unchanged. Publisher stake return: {publisher_stake_return}; challenger stake return: {challenger_stake_return}.",
                social_owner_quote_id(&fact.subject),
                object_summary,
                reason,
                active_edges,
                inactive_edges,
            ),
        })
    }
    #[expect(clippy::too_many_arguments, reason = "stable social seam")]
    pub fn quote_declare_social_edge(
        &self,
        declarer: &ResourceOwner,
        schema_id: &str,
        relation_kind: &str,
        from: &ResourceOwner,
        to: &ResourceOwner,
        weight_bps: i64,
        backing_fact_ids: &[u64],
        ttl_ticks: Option<u64>,
    ) -> Result<SocialFactImpactQuote, RejectReason> {
        self.ensure_owner_exists(declarer)?;
        self.ensure_owner_exists(from)?;
        self.ensure_owner_exists(to)?;

        let schema_id = schema_id.trim();
        let relation_kind = relation_kind.trim();
        if schema_id.is_empty() {
            return social_rule_denied("social edge schema_id cannot be empty");
        }
        if relation_kind.is_empty() {
            return social_rule_denied("social edge relation_kind cannot be empty");
        }
        if !(-10_000..=10_000).contains(&weight_bps) {
            return Err(RejectReason::InvalidAmount { amount: weight_bps });
        }
        if backing_fact_ids.is_empty() {
            return social_rule_denied("social edge backing_fact_ids cannot be empty");
        }
        for fact_id in backing_fact_ids {
            let Some(fact) = self.model.social_facts.get(fact_id) else {
                return social_rule_denied(format!("social backing fact missing: {fact_id}"));
            };
            if !fact.supports_backing() {
                return social_rule_denied(format!(
                    "social backing fact inactive: {fact_id} state={:?}",
                    fact.lifecycle
                ));
            }
        }
        if ttl_ticks.is_some_and(|ticks| ticks == 0) {
            return Err(RejectReason::InvalidAmount { amount: 0 });
        }

        Ok(SocialFactImpactQuote {
            actor_id: social_owner_quote_id(declarer),
            action_kind: "declare_social_edge".to_string(),
            schema_id: schema_id.to_string(),
            subject_id: Some(social_owner_quote_id(from)),
            object_id: Some(social_owner_quote_id(to)),
            claim_summary: summarize_social_text(relation_kind),
            confidence_ppm: None,
            stake_at_risk: 0,
            ttl_ticks,
            affected_relationships: social_affected_relationships(schema_id, from, Some(to)),
            affected_social_surfaces: social_edge_affected_surfaces(schema_id),
            cooperation_opportunity_delta: "positive".to_string(),
            blacklist_or_dispute_risk: "backing_fact_dependent".to_string(),
            governance_or_claim_relevance: "relationship_declaration".to_string(),
            recommended_social_action: "declare_edge".to_string(),
            why_this_action_matters: format!(
                "Declaring this {relation_kind} relationship makes its evidence-backed cooperation signal available to social and governance decisions."
            ),
        })
    }
    #[expect(clippy::too_many_arguments, reason = "stable social seam")]
    pub(super) fn apply_publish_social_fact(
        &mut self,
        actor: ResourceOwner,
        schema_id: String,
        subject: ResourceOwner,
        object: Option<ResourceOwner>,
        claim: String,
        confidence_ppm: i64,
        evidence_event_ids: Vec<WorldEventId>,
        ttl_ticks: Option<u64>,
        stake: Option<SocialStake>,
    ) -> WorldEventKind {
        if let Err(reason) = self.ensure_owner_exists(&actor) {
            return WorldEventKind::ActionRejected { reason };
        }
        if let Err(reason) = self.ensure_owner_exists(&subject) {
            return WorldEventKind::ActionRejected { reason };
        }
        if let Some(owner) = &object
            && let Err(reason) = self.ensure_owner_exists(owner)
        {
            return WorldEventKind::ActionRejected { reason };
        }

        let schema_id = schema_id.trim().to_string();
        if schema_id.is_empty() {
            return social_rule_reject("social schema_id cannot be empty");
        }
        let claim = claim.trim().to_string();
        if claim.is_empty() {
            return social_rule_reject("social claim cannot be empty");
        }
        if !(1..=PPM_BASE).contains(&confidence_ppm) {
            return WorldEventKind::ActionRejected {
                reason: RejectReason::InvalidAmount {
                    amount: confidence_ppm,
                },
            };
        }
        if evidence_event_ids.is_empty() {
            return social_rule_reject("social evidence_event_ids cannot be empty");
        }
        for event_id in &evidence_event_ids {
            if !self.has_journal_event(*event_id) {
                return social_rule_reject(format!("social evidence event missing: {event_id}"));
            }
        }

        if let Some(ticks) = ttl_ticks
            && ticks == 0
        {
            return WorldEventKind::ActionRejected {
                reason: RejectReason::InvalidAmount { amount: 0 },
            };
        }
        if let Err(reason) = validate_social_stake(stake.as_ref()) {
            return WorldEventKind::ActionRejected { reason };
        }
        if let Some(stake_value) = stake.as_ref()
            && let Err(reason) = self.lock_social_stake(&actor, stake_value)
        {
            return WorldEventKind::ActionRejected { reason };
        }

        let fact_id = self.model.next_social_fact_id.max(1);
        self.model.next_social_fact_id = fact_id.saturating_add(1);
        let now = self.time;
        let expires_at_tick = ttl_ticks.map(|ticks| now.saturating_add(ticks));
        let fact = SocialFactState {
            fact_id,
            actor,
            schema_id,
            subject,
            object,
            claim,
            confidence_ppm,
            evidence_event_ids,
            ttl_ticks,
            expires_at_tick,
            stake,
            challenge: None,
            lifecycle: SocialFactLifecycleState::Active,
            created_at_tick: now,
            updated_at_tick: now,
        };
        self.model.social_facts.insert(fact_id, fact.clone());
        WorldEventKind::SocialFactPublished { fact }
    }

    pub(super) fn apply_challenge_social_fact(
        &mut self,
        challenger: ResourceOwner,
        fact_id: u64,
        reason: String,
        stake: Option<SocialStake>,
    ) -> WorldEventKind {
        if let Err(reason) = self.ensure_owner_exists(&challenger) {
            return WorldEventKind::ActionRejected { reason };
        }
        let reason = reason.trim().to_string();
        if reason.is_empty() {
            return social_rule_reject("social challenge reason cannot be empty");
        }
        if let Err(reason) = validate_social_stake(stake.as_ref()) {
            return WorldEventKind::ActionRejected { reason };
        }
        if let Some(stake_value) = stake.as_ref()
            && let Err(reason) = self.lock_social_stake(&challenger, stake_value)
        {
            return WorldEventKind::ActionRejected { reason };
        }

        let now = self.time;
        let Some(fact) = self.model.social_facts.get_mut(&fact_id) else {
            return social_rule_reject(format!("social fact not found: {fact_id}"));
        };
        if !matches!(
            fact.lifecycle,
            SocialFactLifecycleState::Active | SocialFactLifecycleState::Confirmed
        ) {
            return social_rule_reject(format!(
                "social fact {fact_id} cannot be challenged in state {:?}",
                fact.lifecycle
            ));
        }
        if fact.challenge.is_some() {
            return social_rule_reject(format!("social fact {fact_id} already challenged"));
        }

        fact.lifecycle = SocialFactLifecycleState::Challenged;
        fact.challenge = Some(SocialChallengeState {
            challenger: challenger.clone(),
            reason: reason.clone(),
            stake: stake.clone(),
            challenged_at_tick: now,
        });
        fact.updated_at_tick = now;

        WorldEventKind::SocialFactChallenged {
            fact_id,
            challenger,
            reason,
            challenged_at_tick: now,
            stake,
        }
    }

    pub(super) fn apply_adjudicate_social_fact(
        &mut self,
        adjudicator: ResourceOwner,
        fact_id: u64,
        decision: SocialAdjudicationDecision,
        notes: String,
    ) -> WorldEventKind {
        if let Err(reason) = self.ensure_owner_exists(&adjudicator) {
            return WorldEventKind::ActionRejected { reason };
        }
        let notes = notes.trim().to_string();
        if notes.is_empty() {
            return social_rule_reject("social adjudication notes cannot be empty");
        }

        let Some(mut fact) = self.model.social_facts.remove(&fact_id) else {
            return social_rule_reject(format!("social fact not found: {fact_id}"));
        };
        if fact.challenge.is_none() {
            self.model.social_facts.insert(fact_id, fact);
            return social_rule_reject(format!(
                "social fact {fact_id} cannot be adjudicated without challenge"
            ));
        }
        if !social_adjudicator_is_authorized(&fact, &adjudicator) {
            self.model.social_facts.insert(fact_id, fact);
            return social_rule_reject(format!(
                "social adjudicator is not the fact publisher for fact {fact_id}"
            ));
        }

        if let Err(reason) = self.apply_social_fact_adjudication_settlement(&mut fact, decision) {
            self.model.social_facts.insert(fact_id, fact);
            return WorldEventKind::ActionRejected { reason };
        }
        fact.updated_at_tick = self.time;
        self.model.social_facts.insert(fact_id, fact);

        WorldEventKind::SocialFactAdjudicated {
            fact_id,
            adjudicator,
            decision,
            notes,
            adjudicated_at_tick: self.time,
        }
    }

    pub(super) fn apply_revoke_social_fact(
        &mut self,
        actor: ResourceOwner,
        fact_id: u64,
        reason: String,
    ) -> WorldEventKind {
        if let Err(reason) = self.ensure_owner_exists(&actor) {
            return WorldEventKind::ActionRejected { reason };
        }
        let reason = reason.trim().to_string();
        if reason.is_empty() {
            return social_rule_reject("social revoke reason cannot be empty");
        }

        let Some(mut fact) = self.model.social_facts.remove(&fact_id) else {
            return social_rule_reject(format!("social fact not found: {fact_id}"));
        };
        if fact.actor != actor {
            self.model.social_facts.insert(fact_id, fact);
            return social_rule_reject(format!(
                "social fact {fact_id} can only be revoked by publisher"
            ));
        }
        let lifecycle = fact.lifecycle;
        if matches!(
            lifecycle,
            SocialFactLifecycleState::Retracted
                | SocialFactLifecycleState::Revoked
                | SocialFactLifecycleState::Expired
        ) {
            self.model.social_facts.insert(fact_id, fact);
            return social_rule_reject(format!(
                "social fact {fact_id} cannot be revoked in state {:?}",
                lifecycle
            ));
        }

        if let Err(reason) = self.ensure_social_fact_stake_returns_fit(&fact) {
            self.model.social_facts.insert(fact_id, fact);
            return WorldEventKind::ActionRejected { reason };
        }
        if let Err(reason) = self.release_social_fact_stakes(&mut fact) {
            self.model.social_facts.insert(fact_id, fact);
            return WorldEventKind::ActionRejected { reason };
        }
        fact.lifecycle = SocialFactLifecycleState::Revoked;
        fact.updated_at_tick = self.time;
        self.model.social_facts.insert(fact_id, fact);

        WorldEventKind::SocialFactRevoked {
            fact_id,
            actor,
            reason,
            revoked_at_tick: self.time,
        }
    }
    #[expect(clippy::too_many_arguments, reason = "stable social seam")]
    pub(super) fn apply_declare_social_edge(
        &mut self,
        declarer: ResourceOwner,
        schema_id: String,
        relation_kind: String,
        from: ResourceOwner,
        to: ResourceOwner,
        weight_bps: i64,
        backing_fact_ids: Vec<u64>,
        ttl_ticks: Option<u64>,
    ) -> WorldEventKind {
        if let Err(reason) = self.ensure_owner_exists(&declarer) {
            return WorldEventKind::ActionRejected { reason };
        }
        if let Err(reason) = self.ensure_owner_exists(&from) {
            return WorldEventKind::ActionRejected { reason };
        }
        if let Err(reason) = self.ensure_owner_exists(&to) {
            return WorldEventKind::ActionRejected { reason };
        }

        let schema_id = schema_id.trim().to_string();
        let relation_kind = relation_kind.trim().to_string();
        if schema_id.is_empty() {
            return social_rule_reject("social edge schema_id cannot be empty");
        }
        if relation_kind.is_empty() {
            return social_rule_reject("social edge relation_kind cannot be empty");
        }
        if !(-10_000..=10_000).contains(&weight_bps) {
            return WorldEventKind::ActionRejected {
                reason: RejectReason::InvalidAmount { amount: weight_bps },
            };
        }
        if backing_fact_ids.is_empty() {
            return social_rule_reject("social edge backing_fact_ids cannot be empty");
        }
        for fact_id in &backing_fact_ids {
            let Some(fact) = self.model.social_facts.get(fact_id) else {
                return social_rule_reject(format!("social backing fact missing: {fact_id}"));
            };
            if !fact.supports_backing() {
                return social_rule_reject(format!(
                    "social backing fact inactive: {fact_id} state={:?}",
                    fact.lifecycle
                ));
            }
        }
        if let Some(ticks) = ttl_ticks
            && ticks == 0
        {
            return WorldEventKind::ActionRejected {
                reason: RejectReason::InvalidAmount { amount: 0 },
            };
        }

        let edge_id = self.model.next_social_edge_id.max(1);
        self.model.next_social_edge_id = edge_id.saturating_add(1);
        let now = self.time;
        let edge = SocialEdgeState {
            edge_id,
            declarer,
            schema_id,
            relation_kind,
            from,
            to,
            weight_bps,
            backing_fact_ids,
            ttl_ticks,
            expires_at_tick: ttl_ticks.map(|ticks| now.saturating_add(ticks)),
            lifecycle: SocialEdgeLifecycleState::Active,
            created_at_tick: now,
            updated_at_tick: now,
        };
        self.model.social_edges.insert(edge_id, edge.clone());
        WorldEventKind::SocialEdgeDeclared { edge }
    }

    pub(super) fn maintain_social_lifecycle(&mut self) {
        self.expire_social_facts();
        self.expire_social_edges();
    }

    fn expire_social_facts(&mut self) {
        let now = self.time;
        let mut expired_fact_ids = Vec::new();
        for (fact_id, fact) in &self.model.social_facts {
            if matches!(
                fact.lifecycle,
                SocialFactLifecycleState::Retracted
                    | SocialFactLifecycleState::Revoked
                    | SocialFactLifecycleState::Expired
            ) {
                continue;
            }
            if fact
                .expires_at_tick
                .is_some_and(|expires_at_tick| expires_at_tick <= now)
            {
                expired_fact_ids.push(*fact_id);
            }
        }

        for fact_id in expired_fact_ids {
            if self.expire_social_fact_by_id(fact_id, now).is_ok() {
                self.record_event(WorldEventKind::SocialFactExpired {
                    fact_id,
                    expired_at_tick: now,
                });
            }
        }
    }

    fn expire_social_edges(&mut self) {
        let now = self.time;
        let mut expired_edges: Vec<(u64, String)> = Vec::new();

        for (edge_id, edge) in &self.model.social_edges {
            if !edge.is_active() {
                continue;
            }
            if edge
                .expires_at_tick
                .is_some_and(|expires_at_tick| expires_at_tick <= now)
            {
                expired_edges.push((*edge_id, EDGE_EXPIRE_REASON_TTL.to_string()));
                continue;
            }
            if edge
                .backing_fact_ids
                .iter()
                .any(|fact_id| !self.social_fact_supports_backing(*fact_id))
            {
                expired_edges.push((
                    *edge_id,
                    EDGE_EXPIRE_REASON_BACKING_FACT_INACTIVE.to_string(),
                ));
            }
        }

        for (edge_id, reason) in expired_edges {
            if self.expire_social_edge_by_id(edge_id, now).is_ok() {
                self.record_event(WorldEventKind::SocialEdgeExpired {
                    edge_id,
                    reason,
                    expired_at_tick: now,
                });
            }
        }
    }

    pub(super) fn replay_social_fact_published(
        &mut self,
        fact: &SocialFactState,
    ) -> Result<(), String> {
        if self.model.social_facts.contains_key(&fact.fact_id) {
            return Err(format!("social fact already exists: {}", fact.fact_id));
        }
        self.ensure_owner_exists(&fact.actor)
            .map_err(|reason| format!("social fact actor invalid: {reason:?}"))?;
        self.ensure_owner_exists(&fact.subject)
            .map_err(|reason| format!("social fact subject invalid: {reason:?}"))?;
        if let Some(owner) = &fact.object {
            self.ensure_owner_exists(owner)
                .map_err(|reason| format!("social fact object invalid: {reason:?}"))?;
        }
        if let Some(stake) = fact.stake.as_ref() {
            self.lock_social_stake(&fact.actor, stake)
                .map_err(|reason| format!("social fact stake lock failed: {reason:?}"))?;
        }

        self.model.social_facts.insert(fact.fact_id, fact.clone());
        self.model.next_social_fact_id = self
            .model
            .next_social_fact_id
            .max(fact.fact_id.saturating_add(1));
        Ok(())
    }

    pub(super) fn replay_social_fact_challenged(
        &mut self,
        fact_id: u64,
        challenger: &ResourceOwner,
        reason: &str,
        challenged_at_tick: WorldTime,
        stake: Option<SocialStake>,
    ) -> Result<(), String> {
        self.ensure_owner_exists(challenger)
            .map_err(|reason| format!("social challenger invalid: {reason:?}"))?;
        if let Some(stake_value) = stake.as_ref() {
            self.lock_social_stake(challenger, stake_value)
                .map_err(|reason| format!("social challenge stake lock failed: {reason:?}"))?;
        }

        let Some(fact) = self.model.social_facts.get_mut(&fact_id) else {
            return Err(format!("social fact not found: {fact_id}"));
        };
        if fact.challenge.is_some() {
            return Err(format!("social fact already challenged: {fact_id}"));
        }
        fact.lifecycle = SocialFactLifecycleState::Challenged;
        fact.challenge = Some(SocialChallengeState {
            challenger: challenger.clone(),
            reason: reason.to_string(),
            stake,
            challenged_at_tick,
        });
        fact.updated_at_tick = challenged_at_tick;
        Ok(())
    }

    pub(super) fn replay_social_fact_adjudicated(
        &mut self,
        fact_id: u64,
        adjudicator: &ResourceOwner,
        decision: SocialAdjudicationDecision,
        adjudicated_at_tick: WorldTime,
    ) -> Result<(), String> {
        let Some(mut fact) = self.model.social_facts.remove(&fact_id) else {
            return Err(format!("social fact not found: {fact_id}"));
        };
        if fact.challenge.is_none() {
            self.model.social_facts.insert(fact_id, fact);
            return Err(format!(
                "social fact {fact_id} cannot be adjudicated without challenge"
            ));
        }
        if !social_adjudicator_is_authorized(&fact, adjudicator) {
            self.model.social_facts.insert(fact_id, fact);
            return Err(format!(
                "social adjudicator is not the fact publisher: {fact_id}"
            ));
        }
        self.apply_social_fact_adjudication_settlement(&mut fact, decision)
            .map_err(|reason| format!("social adjudication settlement failed: {reason:?}"))?;
        fact.updated_at_tick = adjudicated_at_tick;
        self.model.social_facts.insert(fact_id, fact);
        Ok(())
    }

    pub(super) fn replay_social_fact_revoked(
        &mut self,
        fact_id: u64,
        actor: &ResourceOwner,
        revoked_at_tick: WorldTime,
    ) -> Result<(), String> {
        let Some(mut fact) = self.model.social_facts.remove(&fact_id) else {
            return Err(format!("social fact not found: {fact_id}"));
        };
        if &fact.actor != actor {
            self.model.social_facts.insert(fact_id, fact);
            return Err(format!("social fact revoke actor mismatch: {fact_id}"));
        }
        self.release_social_fact_stakes(&mut fact)
            .map_err(|reason| format!("social revoke release stake failed: {reason:?}"))?;
        fact.lifecycle = SocialFactLifecycleState::Revoked;
        fact.updated_at_tick = revoked_at_tick;
        self.model.social_facts.insert(fact_id, fact);
        Ok(())
    }

    pub(super) fn replay_social_fact_expired(
        &mut self,
        fact_id: u64,
        expired_at_tick: WorldTime,
    ) -> Result<(), String> {
        self.expire_social_fact_by_id(fact_id, expired_at_tick)
    }

    pub(super) fn replay_social_edge_declared(
        &mut self,
        edge: &SocialEdgeState,
    ) -> Result<(), String> {
        if self.model.social_edges.contains_key(&edge.edge_id) {
            return Err(format!("social edge already exists: {}", edge.edge_id));
        }
        self.ensure_owner_exists(&edge.declarer)
            .map_err(|reason| format!("social edge declarer invalid: {reason:?}"))?;
        self.ensure_owner_exists(&edge.from)
            .map_err(|reason| format!("social edge from invalid: {reason:?}"))?;
        self.ensure_owner_exists(&edge.to)
            .map_err(|reason| format!("social edge to invalid: {reason:?}"))?;
        for fact_id in &edge.backing_fact_ids {
            if !self.model.social_facts.contains_key(fact_id) {
                return Err(format!("social edge backing fact missing: {fact_id}"));
            }
        }

        self.model.social_edges.insert(edge.edge_id, edge.clone());
        self.model.next_social_edge_id = self
            .model
            .next_social_edge_id
            .max(edge.edge_id.saturating_add(1));
        Ok(())
    }

    pub(super) fn replay_social_edge_expired(
        &mut self,
        edge_id: u64,
        expired_at_tick: WorldTime,
    ) -> Result<(), String> {
        self.expire_social_edge_by_id(edge_id, expired_at_tick)
    }

    fn social_fact_supports_backing(&self, fact_id: u64) -> bool {
        self.model
            .social_facts
            .get(&fact_id)
            .is_some_and(SocialFactState::supports_backing)
    }

    fn has_journal_event(&self, event_id: WorldEventId) -> bool {
        // Event ids are assigned from next_event_id and replay enforces a gapless sequence.
        event_id < self.next_event_id
    }

    fn ensure_social_stake_available(
        &self,
        owner: &ResourceOwner,
        stake: Option<&SocialStake>,
    ) -> Result<(), RejectReason> {
        validate_social_stake(stake)?;
        let Some(stake) = stake else {
            return Ok(());
        };
        if matches!(owner, ResourceOwner::Location { .. })
            && matches!(stake.kind, super::super::types::ResourceKind::Electricity)
        {
            return social_rule_denied("location electricity pool removed");
        }
        let available = self
            .owner_stock(owner)
            .map(|stock| stock.get(stake.kind))
            .unwrap_or(0);
        if available < stake.amount {
            return Err(RejectReason::InsufficientResource {
                owner: owner.clone(),
                kind: stake.kind,
                requested: stake.amount,
                available,
            });
        }
        Ok(())
    }

    fn lock_social_stake(
        &mut self,
        owner: &ResourceOwner,
        stake: &SocialStake,
    ) -> Result<(), RejectReason> {
        self.remove_from_owner(owner, stake.kind, stake.amount)
    }

    fn release_social_stake(
        &mut self,
        owner: &ResourceOwner,
        stake: SocialStake,
    ) -> Result<(), RejectReason> {
        self.add_to_owner(owner, stake.kind, stake.amount)
    }

    fn slash_social_stake_to_pool(&mut self, stake: SocialStake) -> Result<(), RejectReason> {
        self.model
            .social_stake_pool
            .add(stake.kind, stake.amount)
            .map_err(|err| match err {
                crate::types::StockError::NegativeAmount { amount } => {
                    RejectReason::InvalidAmount { amount }
                }
                crate::types::StockError::Insufficient { requested, .. } => {
                    RejectReason::InvalidAmount { amount: requested }
                }
                crate::types::StockError::Overflow { delta, .. } => {
                    RejectReason::InvalidAmount { amount: delta }
                }
            })
    }

    fn release_social_fact_stakes(
        &mut self,
        fact: &mut SocialFactState,
    ) -> Result<(), RejectReason> {
        if let Some(stake) = fact.stake.take() {
            self.release_social_stake(&fact.actor, stake)?;
        }
        if let Some(challenge) = fact.challenge.as_mut()
            && let Some(stake) = challenge.stake.take()
        {
            self.release_social_stake(&challenge.challenger, stake)?;
        }
        Ok(())
    }

    fn apply_social_fact_adjudication_settlement(
        &mut self,
        fact: &mut SocialFactState,
        decision: SocialAdjudicationDecision,
    ) -> Result<(), RejectReason> {
        let actor_stake = fact.stake.take();
        let challenger_stake = fact
            .challenge
            .as_mut()
            .and_then(|challenge| challenge.stake.take());

        match decision {
            SocialAdjudicationDecision::Confirm => {
                if let Some(stake) = challenger_stake {
                    self.slash_social_stake_to_pool(stake)?;
                }
                if let Some(stake) = actor_stake {
                    self.release_social_stake(&fact.actor, stake)?;
                }
                fact.lifecycle = SocialFactLifecycleState::Confirmed;
            }
            SocialAdjudicationDecision::Retract => {
                if let Some(stake) = actor_stake {
                    self.slash_social_stake_to_pool(stake)?;
                }
                if let Some(challenge) = fact.challenge.as_ref()
                    && let Some(stake) = challenger_stake
                {
                    self.release_social_stake(&challenge.challenger, stake)?;
                }
                fact.lifecycle = SocialFactLifecycleState::Retracted;
            }
        }
        Ok(())
    }

    fn expire_social_fact_by_id(&mut self, fact_id: u64, at_tick: WorldTime) -> Result<(), String> {
        let Some(mut fact) = self.model.social_facts.remove(&fact_id) else {
            return Err(format!("social fact not found: {fact_id}"));
        };
        if matches!(
            fact.lifecycle,
            SocialFactLifecycleState::Retracted
                | SocialFactLifecycleState::Revoked
                | SocialFactLifecycleState::Expired
        ) {
            self.model.social_facts.insert(fact_id, fact);
            return Err(format!("social fact {fact_id} already terminal"));
        }

        self.release_social_fact_stakes(&mut fact)
            .map_err(|reason| format!("social fact expire release stake failed: {reason:?}"))?;
        fact.lifecycle = SocialFactLifecycleState::Expired;
        fact.updated_at_tick = at_tick;
        self.model.social_facts.insert(fact_id, fact);
        Ok(())
    }

    fn expire_social_edge_by_id(&mut self, edge_id: u64, at_tick: WorldTime) -> Result<(), String> {
        let Some(edge) = self.model.social_edges.get_mut(&edge_id) else {
            return Err(format!("social edge not found: {edge_id}"));
        };
        if !edge.is_active() {
            return Err(format!("social edge already terminal: {edge_id}"));
        }
        edge.lifecycle = SocialEdgeLifecycleState::Expired;
        edge.updated_at_tick = at_tick;
        Ok(())
    }
}

fn validate_social_stake(stake: Option<&SocialStake>) -> Result<(), RejectReason> {
    let Some(stake) = stake else {
        return Ok(());
    };
    if stake.amount <= 0 {
        return Err(RejectReason::InvalidAmount {
            amount: stake.amount,
        });
    }
    Ok(())
}

fn social_rule_denied<T>(note: impl Into<String>) -> Result<T, RejectReason> {
    Err(RejectReason::RuleDenied {
        notes: vec![note.into()],
    })
}

fn social_owner_quote_id(owner: &ResourceOwner) -> String {
    match owner {
        ResourceOwner::Agent { agent_id } => agent_id.clone(),
        ResourceOwner::Location { location_id } => location_id.clone(),
    }
}

fn summarize_social_text(text: &str) -> String {
    const MAX_CHARS: usize = 96;
    let trimmed = text.trim();
    if trimmed.chars().count() <= MAX_CHARS {
        return trimmed.to_string();
    }
    let mut summary = trimmed.chars().take(MAX_CHARS).collect::<String>();
    summary.push_str("...");
    summary
}

fn social_affected_relationships(
    schema_id: &str,
    subject: &ResourceOwner,
    object: Option<&ResourceOwner>,
) -> Vec<String> {
    let mut relationships = vec![format!("schema:{schema_id}")];
    relationships.push(format!("subject:{}", social_owner_quote_id(subject)));
    if let Some(object) = object {
        relationships.push(format!("object:{}", social_owner_quote_id(object)));
    }
    relationships
}

fn social_affected_surfaces(schema_id: &str) -> Vec<String> {
    let lower = schema_id.to_ascii_lowercase();
    let mut surfaces = vec!["social_fact_ledger".to_string()];
    if lower.contains("reputation") {
        surfaces.push("reputation".to_string());
    }
    if lower.contains("trust") || lower.contains("relationship") {
        surfaces.push("relationship".to_string());
    }
    if lower.contains("blacklist") {
        surfaces.push("blacklist".to_string());
    }
    surfaces
}

fn social_edge_affected_surfaces(schema_id: &str) -> Vec<String> {
    let mut surfaces = social_affected_surfaces(schema_id);
    for surface in ["reputation", "relationship"] {
        if !surfaces.iter().any(|existing| existing == surface) {
            surfaces.push(surface.to_string());
        }
    }
    surfaces
}

fn social_adjudicator_is_authorized(fact: &SocialFactState, owner: &ResourceOwner) -> bool {
    // v1 has no representable `ResourceOwner::World`; the only runtime
    // authority is the fact publisher. Keep this predicate shared by quote,
    // execution, and replay so all paths enforce the same rule.
    fact.actor == *owner
}

fn social_rule_reject(note: impl Into<String>) -> WorldEventKind {
    WorldEventKind::ActionRejected {
        reason: RejectReason::RuleDenied {
            notes: vec![note.into()],
        },
    }
}
