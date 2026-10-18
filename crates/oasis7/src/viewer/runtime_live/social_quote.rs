use super::control_plane::{
    ensure_agent_player_access_runtime, map_auth_verify_error_code, normalize_optional_public_key,
};
use super::session_policy::map_session_policy_error_code;
use super::support::send_response;
use super::*;
use crate::simulator::{WorldJournal, WorldKernel};
use crate::viewer::auth::{
    verify_adjudicate_social_fact_quote_auth_proof, verify_declare_social_edge_quote_auth_proof,
    verify_publish_social_fact_quote_auth_proof, verify_revoke_social_fact_quote_auth_proof,
    verify_social_contact_quote_auth_proof,
};
use crate::viewer::protocol::{
    AdjudicateSocialFactQuotePreflight, AdjudicateSocialFactQuoteRequest,
    DeclareSocialEdgeQuotePreflight, DeclareSocialEdgeQuoteRequest, FirstContactClass,
    GameplayActionError, PublishSocialFactQuotePreflight, PublishSocialFactQuoteRequest,
    RevokeSocialFactQuotePreflight, RevokeSocialFactQuoteRequest, SocialAdjudicationDecision,
    SocialContactQuotePreflight, SocialContactQuoteRequest,
};
use oasis7_agent_api::world_action::ResourceKind;
use oasis7_simulator::{ResourceOwner, SocialStake};
use std::collections::HashMap;

impl ViewerRuntimeLiveServer {
    pub(in crate::viewer::runtime_live) fn handle_social_quote_request(
        &mut self,
        request: ViewerRequest,
        writer: &mut dyn Write,
    ) -> Result<(), ViewerRuntimeLiveServerError> {
        match request {
            ViewerRequest::QuoteDeclareSocialEdge { request } => {
                self.quote_declare_social_edge(request, writer)
            }
            ViewerRequest::QuotePublishSocialFact { request } => {
                self.quote_publish_social_fact(request, writer)
            }
            ViewerRequest::QuoteAdjudicateSocialFact { request } => {
                self.quote_adjudicate_social_fact(request, writer)
            }
            ViewerRequest::QuoteRevokeSocialFact { request } => {
                self.quote_revoke_social_fact(request, writer)
            }
            ViewerRequest::QuoteSocialContact { request } => {
                self.quote_social_contact(request, writer)
            }
            ViewerRequest::QuoteGovernanceVote { request } => {
                self.quote_governance_vote(request, writer)
            }
            ViewerRequest::QuoteDeclareWar { request } => self.quote_declare_war(request, writer),
            _ => unreachable!("non-social quote routed to social quote helper"),
        }
    }

    /// Computes an authenticated, non-mutating social-fact settlement preflight from runtime state.
    pub(in crate::viewer::runtime_live) fn handle_adjudicate_social_fact_quote(
        &mut self,
        request: AdjudicateSocialFactQuoteRequest,
    ) -> Result<AdjudicateSocialFactQuotePreflight, GameplayActionError> {
        const ACTION_ID: &str = "quote_adjudicate_social_fact";
        let auth = request.auth.as_ref().ok_or_else(|| GameplayActionError {
            code: "auth_proof_required".to_string(),
            message: format!("{ACTION_ID} requires auth proof"),
            action_id: Some(ACTION_ID.to_string()),
            target_agent_id: None,
        })?;
        let verified =
            verify_adjudicate_social_fact_quote_auth_proof(&request, auth).map_err(|message| {
                GameplayActionError {
                    code: map_auth_verify_error_code(message.as_str()).to_string(),
                    message,
                    action_id: Some(ACTION_ID.to_string()),
                    target_agent_id: None,
                }
            })?;
        self.session_policy
            .validate_known_session_key(verified.player_id.as_str(), verified.public_key.as_str())
            .map_err(|message| GameplayActionError {
                code: map_session_policy_error_code(message.as_str()).to_string(),
                message,
                action_id: Some(ACTION_ID.to_string()),
                target_agent_id: None,
            })?;
        let adjudicator_agent_id = self
            .llm_sidecar
            .bound_agent_for_player(verified.player_id.as_str())
            .ok_or_else(|| GameplayActionError {
                code: "player_agent_binding_required".to_string(),
                message: format!("{ACTION_ID} requires a bound player Agent session"),
                action_id: Some(ACTION_ID.to_string()),
                target_agent_id: None,
            })?;
        let public_key = normalize_optional_public_key(request.public_key.as_deref());
        ensure_agent_player_access_runtime(
            &self.world,
            &self.llm_sidecar,
            adjudicator_agent_id,
            verified.player_id.as_str(),
            public_key.as_deref(),
        )
        .map_err(|err| GameplayActionError {
            code: err.code,
            message: err.message,
            action_id: Some(ACTION_ID.to_string()),
            target_agent_id: err.agent_id,
        })?;

        let model = super::mapping::runtime_state_to_simulator_model(
            self.world.state(),
            &self.llm_sidecar,
            self.seed_model.as_ref(),
        );
        let decision = match request.decision {
            SocialAdjudicationDecision::Confirm => {
                oasis7_simulator::SocialAdjudicationDecision::Confirm
            }
            SocialAdjudicationDecision::Retract => {
                oasis7_simulator::SocialAdjudicationDecision::Retract
            }
        };
        let quote = WorldKernel::with_model(self.snapshot_config.clone(), model)
            .quote_adjudicate_social_fact(
                &ResourceOwner::Agent {
                    agent_id: adjudicator_agent_id.to_string(),
                },
                request.fact_id,
                decision,
                request.notes.as_str(),
            )
            .map_err(|reason| GameplayActionError {
                code: "adjudicate_social_fact_quote_rejected".to_string(),
                message: format!("{ACTION_ID} rejected: {reason:?}"),
                action_id: Some(ACTION_ID.to_string()),
                target_agent_id: Some(adjudicator_agent_id.to_string()),
            })?;
        Ok(AdjudicateSocialFactQuotePreflight {
            actor_id: quote.actor_id,
            action_kind: quote.action_kind,
            schema_id: quote.schema_id,
            subject_id: quote.subject_id,
            object_id: quote.object_id,
            claim_summary: quote.claim_summary,
            confidence_ppm: quote.confidence_ppm,
            stake_at_risk: quote.stake_at_risk,
            ttl_ticks: quote.ttl_ticks,
            affected_relationships: quote.affected_relationships,
            affected_social_surfaces: quote.affected_social_surfaces,
            cooperation_opportunity_delta: quote.cooperation_opportunity_delta,
            blacklist_or_dispute_risk: quote.blacklist_or_dispute_risk,
            governance_or_claim_relevance: quote.governance_or_claim_relevance,
            recommended_social_action: quote.recommended_social_action,
            why_this_action_matters: quote.why_this_action_matters,
        })
    }

    /// Computes an authenticated, non-mutating first-contact preview from runtime session state.
    pub(in crate::viewer::runtime_live) fn handle_social_contact_quote(
        &mut self,
        request: SocialContactQuoteRequest,
    ) -> Result<SocialContactQuotePreflight, GameplayActionError> {
        const ACTION_ID: &str = "quote_social_contact";
        let auth = request.auth.as_ref().ok_or_else(|| GameplayActionError {
            code: "auth_proof_required".to_string(),
            message: format!("{ACTION_ID} requires auth proof"),
            action_id: Some(ACTION_ID.to_string()),
            target_agent_id: None,
        })?;
        let verified =
            verify_social_contact_quote_auth_proof(&request, auth).map_err(|message| {
                GameplayActionError {
                    code: map_auth_verify_error_code(message.as_str()).to_string(),
                    message,
                    action_id: Some(ACTION_ID.to_string()),
                    target_agent_id: None,
                }
            })?;
        self.session_policy
            .validate_known_session_key(verified.player_id.as_str(), verified.public_key.as_str())
            .map_err(|message| GameplayActionError {
                code: map_session_policy_error_code(message.as_str()).to_string(),
                message,
                action_id: Some(ACTION_ID.to_string()),
                target_agent_id: None,
            })?;
        let agent_id = self
            .llm_sidecar
            .bound_agent_for_player(verified.player_id.as_str())
            .ok_or_else(|| GameplayActionError {
                code: "player_agent_binding_required".to_string(),
                message: format!("{ACTION_ID} requires a bound player Agent session"),
                action_id: Some(ACTION_ID.to_string()),
                target_agent_id: None,
            })?;
        let public_key = normalize_optional_public_key(request.public_key.as_deref());
        ensure_agent_player_access_runtime(
            &self.world,
            &self.llm_sidecar,
            agent_id,
            verified.player_id.as_str(),
            public_key.as_deref(),
        )
        .map_err(|err| GameplayActionError {
            code: err.code,
            message: err.message,
            action_id: Some(ACTION_ID.to_string()),
            target_agent_id: err.agent_id,
        })?;

        let candidate_agent_id = request.candidate_agent_id.trim();
        let candidate_is_known = self.world.state().agents.contains_key(candidate_agent_id);
        let defer_reason = if candidate_agent_id.is_empty() {
            "A candidate Agent is required before contact can be evaluated.".to_string()
        } else if candidate_agent_id == agent_id {
            "Self-contact does not establish a reciprocal opportunity; continue independent local work."
                .to_string()
        } else if !candidate_is_known {
            "The requested candidate Agent is not present in the current runtime state.".to_string()
        } else {
            "The candidate is known, but the runtime has no authoritative reciprocal offer, capacity, or consent evidence."
                .to_string()
        };
        Ok(SocialContactQuotePreflight {
            first_contact_class: FirstContactClass::DeferContact,
            contact_purpose: request.contact_purpose.trim().to_string(),
            expected_mutual_value:
                "No reciprocal value is asserted without authoritative runtime evidence."
                    .to_string(),
            risk_or_commitment: "No resources, time, reputation, or membership are exposed."
                .to_string(),
            solo_lane_preserved: true,
            recommended_contact_action: "Continue independent local work".to_string(),
            defer_reason,
        })
    }

    /// Computes an authenticated, non-mutating social-edge impact preflight from runtime state.
    pub(in crate::viewer::runtime_live) fn handle_declare_social_edge_quote(
        &mut self,
        request: DeclareSocialEdgeQuoteRequest,
    ) -> Result<DeclareSocialEdgeQuotePreflight, GameplayActionError> {
        let auth = request.auth.as_ref().ok_or_else(|| GameplayActionError {
            code: "auth_proof_required".to_string(),
            message: "quote_declare_social_edge requires auth proof".to_string(),
            action_id: Some("quote_declare_social_edge".to_string()),
            target_agent_id: None,
        })?;
        let verified =
            verify_declare_social_edge_quote_auth_proof(&request, auth).map_err(|message| {
                GameplayActionError {
                    code: map_auth_verify_error_code(message.as_str()).to_string(),
                    message,
                    action_id: Some("quote_declare_social_edge".to_string()),
                    target_agent_id: None,
                }
            })?;
        self.session_policy
            .validate_known_session_key(verified.player_id.as_str(), verified.public_key.as_str())
            .map_err(|message| GameplayActionError {
                code: map_session_policy_error_code(message.as_str()).to_string(),
                message,
                action_id: Some("quote_declare_social_edge".to_string()),
                target_agent_id: None,
            })?;
        let declarer_agent_id = self
            .llm_sidecar
            .bound_agent_for_player(verified.player_id.as_str())
            .ok_or_else(|| GameplayActionError {
                code: "player_agent_binding_required".to_string(),
                message: "quote_declare_social_edge requires a bound player Agent session"
                    .to_string(),
                action_id: Some("quote_declare_social_edge".to_string()),
                target_agent_id: None,
            })?;
        let public_key = normalize_optional_public_key(request.public_key.as_deref());
        ensure_agent_player_access_runtime(
            &self.world,
            &self.llm_sidecar,
            declarer_agent_id,
            verified.player_id.as_str(),
            public_key.as_deref(),
        )
        .map_err(|err| GameplayActionError {
            code: err.code,
            message: err.message,
            action_id: Some("quote_declare_social_edge".to_string()),
            target_agent_id: err.agent_id,
        })?;
        let model = super::mapping::runtime_state_to_simulator_model(
            self.world.state(),
            &self.llm_sidecar,
            self.seed_model.as_ref(),
        );
        let quote = WorldKernel::with_model(self.snapshot_config.clone(), model)
            .quote_declare_social_edge(
                &ResourceOwner::Agent {
                    agent_id: declarer_agent_id.to_string(),
                },
                request.schema_id.as_str(),
                request.relation_kind.as_str(),
                &ResourceOwner::Agent {
                    agent_id: request.from_agent_id,
                },
                &ResourceOwner::Agent {
                    agent_id: request.to_agent_id,
                },
                request.weight_bps,
                request.backing_fact_ids.as_slice(),
                request.ttl_ticks,
            )
            .map_err(|reason| GameplayActionError {
                code: "declare_social_edge_quote_rejected".to_string(),
                message: format!("quote_declare_social_edge rejected: {reason:?}"),
                action_id: Some("quote_declare_social_edge".to_string()),
                target_agent_id: Some(declarer_agent_id.to_string()),
            })?;
        Ok(DeclareSocialEdgeQuotePreflight {
            actor_id: quote.actor_id,
            action_kind: quote.action_kind,
            schema_id: quote.schema_id,
            subject_id: quote.subject_id,
            object_id: quote.object_id,
            claim_summary: quote.claim_summary,
            confidence_ppm: quote.confidence_ppm,
            stake_at_risk: quote.stake_at_risk,
            ttl_ticks: quote.ttl_ticks,
            affected_relationships: quote.affected_relationships,
            affected_social_surfaces: quote.affected_social_surfaces,
            cooperation_opportunity_delta: quote.cooperation_opportunity_delta,
            blacklist_or_dispute_risk: quote.blacklist_or_dispute_risk,
            governance_or_claim_relevance: quote.governance_or_claim_relevance,
            recommended_social_action: quote.recommended_social_action,
            why_this_action_matters: quote.why_this_action_matters,
        })
    }

    /// Computes an authenticated, non-mutating social-fact impact preflight from runtime state.
    pub(in crate::viewer::runtime_live) fn handle_publish_social_fact_quote(
        &mut self,
        request: PublishSocialFactQuoteRequest,
    ) -> Result<PublishSocialFactQuotePreflight, GameplayActionError> {
        const ACTION_ID: &str = "quote_publish_social_fact";
        let auth = request.auth.as_ref().ok_or_else(|| GameplayActionError {
            code: "auth_proof_required".to_string(),
            message: format!("{ACTION_ID} requires auth proof"),
            action_id: Some(ACTION_ID.to_string()),
            target_agent_id: None,
        })?;
        let verified =
            verify_publish_social_fact_quote_auth_proof(&request, auth).map_err(|message| {
                GameplayActionError {
                    code: map_auth_verify_error_code(message.as_str()).to_string(),
                    message,
                    action_id: Some(ACTION_ID.to_string()),
                    target_agent_id: None,
                }
            })?;
        self.session_policy
            .validate_known_session_key(verified.player_id.as_str(), verified.public_key.as_str())
            .map_err(|message| GameplayActionError {
                code: map_session_policy_error_code(message.as_str()).to_string(),
                message,
                action_id: Some(ACTION_ID.to_string()),
                target_agent_id: None,
            })?;
        let publisher_agent_id = self
            .llm_sidecar
            .bound_agent_for_player(verified.player_id.as_str())
            .ok_or_else(|| GameplayActionError {
                code: "player_agent_binding_required".to_string(),
                message: format!("{ACTION_ID} requires a bound player Agent session"),
                action_id: Some(ACTION_ID.to_string()),
                target_agent_id: None,
            })?;
        let public_key = normalize_optional_public_key(request.public_key.as_deref());
        ensure_agent_player_access_runtime(
            &self.world,
            &self.llm_sidecar,
            publisher_agent_id,
            verified.player_id.as_str(),
            public_key.as_deref(),
        )
        .map_err(|err| GameplayActionError {
            code: err.code,
            message: err.message,
            action_id: Some(ACTION_ID.to_string()),
            target_agent_id: err.agent_id,
        })?;
        let stake = request
            .stake
            .as_ref()
            .map(|stake| {
                let kind = match stake.kind.trim().to_ascii_lowercase().as_str() {
                    "electricity" => ResourceKind::Electricity,
                    "data" => ResourceKind::Data,
                    _ => {
                        return Err(GameplayActionError {
                            code: "publish_social_fact_quote_rejected".to_string(),
                            message: format!("{ACTION_ID} rejected: invalid stake kind"),
                            action_id: Some(ACTION_ID.to_string()),
                            target_agent_id: Some(publisher_agent_id.to_string()),
                        });
                    }
                };
                Ok(SocialStake {
                    kind,
                    amount: stake.amount,
                })
            })
            .transpose()?;
        let object = request
            .object_agent_id
            .map(|agent_id| ResourceOwner::Agent { agent_id });
        let model = super::mapping::runtime_state_to_simulator_model(
            self.world.state(),
            &self.llm_sidecar,
            self.seed_model.as_ref(),
        );
        let mut simulator_event_ids = HashMap::new();
        let journal = WorldJournal {
            version: oasis7_simulator::JOURNAL_VERSION,
            events: self
                .world
                .journal()
                .events
                .iter()
                .enumerate()
                .map(|(index, event)| {
                    simulator_event_ids.insert(event.id, index as u64);
                    let mut mapped = super::mapping::map_runtime_event(
                        event,
                        &self.snapshot_config,
                        self.seed_model.as_ref(),
                    );
                    mapped.id = index as u64;
                    mapped
                })
                .collect(),
        };
        let evidence_event_ids = request
            .evidence_event_ids
            .iter()
            .map(|runtime_event_id| {
                simulator_event_ids
                    .get(runtime_event_id)
                    .copied()
                    .ok_or_else(|| GameplayActionError {
                        code: "publish_social_fact_quote_rejected".to_string(),
                        message: format!(
                            "{ACTION_ID} rejected: social evidence event missing: {runtime_event_id}"
                        ),
                        action_id: Some(ACTION_ID.to_string()),
                        target_agent_id: Some(publisher_agent_id.to_string()),
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut kernel = WorldKernel::with_model(self.snapshot_config.clone(), model);
        let mut snapshot = kernel.snapshot();
        snapshot.time = self.world.state().time;
        snapshot.journal_len = journal.events.len();
        snapshot.next_event_id = journal.events.len() as u64;
        kernel =
            WorldKernel::from_snapshot(snapshot, journal).map_err(|err| GameplayActionError {
                code: "publish_social_fact_quote_projection_failed".to_string(),
                message: format!("{ACTION_ID} projection failed: {err:?}"),
                action_id: Some(ACTION_ID.to_string()),
                target_agent_id: Some(publisher_agent_id.to_string()),
            })?;
        let quote = kernel
            .quote_publish_social_fact(
                &ResourceOwner::Agent {
                    agent_id: publisher_agent_id.to_string(),
                },
                request.schema_id.as_str(),
                &ResourceOwner::Agent {
                    agent_id: request.subject_agent_id,
                },
                object.as_ref(),
                request.claim.as_str(),
                request.confidence_ppm,
                evidence_event_ids.as_slice(),
                request.ttl_ticks,
                stake.as_ref(),
            )
            .map_err(|reason| GameplayActionError {
                code: "publish_social_fact_quote_rejected".to_string(),
                message: format!("{ACTION_ID} rejected: {reason:?}"),
                action_id: Some(ACTION_ID.to_string()),
                target_agent_id: Some(publisher_agent_id.to_string()),
            })?;
        Ok(PublishSocialFactQuotePreflight {
            actor_id: quote.actor_id,
            action_kind: "publish_fact".to_string(),
            schema_id: quote.schema_id,
            subject_id: quote.subject_id,
            object_id: quote.object_id,
            claim_summary: quote.claim_summary,
            confidence_ppm: quote.confidence_ppm,
            stake_at_risk: quote.stake_at_risk,
            ttl_ticks: quote.ttl_ticks,
            affected_relationships: quote.affected_relationships,
            affected_social_surfaces: quote.affected_social_surfaces,
            cooperation_opportunity_delta: quote.cooperation_opportunity_delta,
            blacklist_or_dispute_risk: quote.blacklist_or_dispute_risk,
            governance_or_claim_relevance: quote.governance_or_claim_relevance,
            recommended_social_action: quote.recommended_social_action,
            why_this_action_matters: quote.why_this_action_matters,
        })
    }

    /// Computes an authenticated, non-mutating social-fact revocation preflight from runtime state.
    pub(in crate::viewer::runtime_live) fn handle_revoke_social_fact_quote(
        &mut self,
        request: RevokeSocialFactQuoteRequest,
    ) -> Result<RevokeSocialFactQuotePreflight, GameplayActionError> {
        const ACTION_ID: &str = "quote_revoke_social_fact";
        let auth = request.auth.as_ref().ok_or_else(|| GameplayActionError {
            code: "auth_proof_required".to_string(),
            message: format!("{ACTION_ID} requires auth proof"),
            action_id: Some(ACTION_ID.to_string()),
            target_agent_id: None,
        })?;
        let verified =
            verify_revoke_social_fact_quote_auth_proof(&request, auth).map_err(|message| {
                GameplayActionError {
                    code: map_auth_verify_error_code(message.as_str()).to_string(),
                    message,
                    action_id: Some(ACTION_ID.to_string()),
                    target_agent_id: None,
                }
            })?;
        self.session_policy
            .validate_known_session_key(verified.player_id.as_str(), verified.public_key.as_str())
            .map_err(|message| GameplayActionError {
                code: map_session_policy_error_code(message.as_str()).to_string(),
                message,
                action_id: Some(ACTION_ID.to_string()),
                target_agent_id: None,
            })?;
        let publisher_agent_id = self
            .llm_sidecar
            .bound_agent_for_player(verified.player_id.as_str())
            .ok_or_else(|| GameplayActionError {
                code: "player_agent_binding_required".to_string(),
                message: format!("{ACTION_ID} requires a bound player Agent session"),
                action_id: Some(ACTION_ID.to_string()),
                target_agent_id: None,
            })?;
        let public_key = normalize_optional_public_key(request.public_key.as_deref());
        ensure_agent_player_access_runtime(
            &self.world,
            &self.llm_sidecar,
            publisher_agent_id,
            verified.player_id.as_str(),
            public_key.as_deref(),
        )
        .map_err(|err| GameplayActionError {
            code: err.code,
            message: err.message,
            action_id: Some(ACTION_ID.to_string()),
            target_agent_id: err.agent_id,
        })?;

        let model = super::mapping::runtime_state_to_simulator_model(
            self.world.state(),
            &self.llm_sidecar,
            self.seed_model.as_ref(),
        );
        let quote = WorldKernel::with_model(self.snapshot_config.clone(), model)
            .quote_revoke_social_fact(
                &ResourceOwner::Agent {
                    agent_id: publisher_agent_id.to_string(),
                },
                request.fact_id,
                request.reason.trim(),
            )
            .map_err(|reason| GameplayActionError {
                code: "revoke_social_fact_quote_rejected".to_string(),
                message: format!("{ACTION_ID} rejected: {reason:?}"),
                action_id: Some(ACTION_ID.to_string()),
                target_agent_id: Some(publisher_agent_id.to_string()),
            })?;
        Ok(RevokeSocialFactQuotePreflight {
            actor_id: quote.actor_id,
            action_kind: quote.action_kind,
            schema_id: quote.schema_id,
            subject_id: quote.subject_id,
            object_id: quote.object_id,
            claim_summary: quote.claim_summary,
            confidence_ppm: quote.confidence_ppm,
            stake_at_risk: quote.stake_at_risk,
            ttl_ticks: quote.ttl_ticks,
            affected_relationships: quote.affected_relationships,
            affected_social_surfaces: quote.affected_social_surfaces,
            cooperation_opportunity_delta: quote.cooperation_opportunity_delta,
            blacklist_or_dispute_risk: quote.blacklist_or_dispute_risk,
            governance_or_claim_relevance: quote.governance_or_claim_relevance,
            recommended_social_action: quote.recommended_social_action,
            why_this_action_matters: quote.why_this_action_matters,
        })
    }

    pub(in crate::viewer::runtime_live) fn quote_declare_social_edge(
        &mut self,
        request: DeclareSocialEdgeQuoteRequest,
        writer: &mut dyn Write,
    ) -> Result<(), ViewerRuntimeLiveServerError> {
        send_response(
            writer,
            &self
                .handle_declare_social_edge_quote(request)
                .map(|quote| ViewerResponse::DeclareSocialEdgeQuotePreflight { quote })
                .unwrap_or_else(|error| ViewerResponse::GameplayActionError { error }),
        )
    }

    pub(in crate::viewer::runtime_live) fn quote_publish_social_fact(
        &mut self,
        request: PublishSocialFactQuoteRequest,
        writer: &mut dyn Write,
    ) -> Result<(), ViewerRuntimeLiveServerError> {
        send_response(
            writer,
            &self
                .handle_publish_social_fact_quote(request)
                .map(|quote| ViewerResponse::PublishSocialFactQuotePreflight { quote })
                .unwrap_or_else(|error| ViewerResponse::GameplayActionError { error }),
        )
    }

    pub(in crate::viewer::runtime_live) fn quote_adjudicate_social_fact(
        &mut self,
        request: AdjudicateSocialFactQuoteRequest,
        writer: &mut dyn Write,
    ) -> Result<(), ViewerRuntimeLiveServerError> {
        send_response(
            writer,
            &self
                .handle_adjudicate_social_fact_quote(request)
                .map(|quote| ViewerResponse::AdjudicateSocialFactQuotePreflight { quote })
                .unwrap_or_else(|error| ViewerResponse::GameplayActionError { error }),
        )
    }

    pub(in crate::viewer::runtime_live) fn quote_revoke_social_fact(
        &mut self,
        request: RevokeSocialFactQuoteRequest,
        writer: &mut dyn Write,
    ) -> Result<(), ViewerRuntimeLiveServerError> {
        send_response(
            writer,
            &self
                .handle_revoke_social_fact_quote(request)
                .map(|quote| ViewerResponse::RevokeSocialFactQuotePreflight { quote })
                .unwrap_or_else(|error| ViewerResponse::GameplayActionError { error }),
        )
    }

    pub(in crate::viewer::runtime_live) fn quote_social_contact(
        &mut self,
        request: SocialContactQuoteRequest,
        writer: &mut dyn Write,
    ) -> Result<(), ViewerRuntimeLiveServerError> {
        send_response(
            writer,
            &self
                .handle_social_contact_quote(request)
                .map(|quote| ViewerResponse::SocialContactQuotePreflight { quote })
                .unwrap_or_else(|error| ViewerResponse::GameplayActionError { error }),
        )
    }
}
