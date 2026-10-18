#[cfg(not(target_arch = "wasm32"))]
mod auth;
#[cfg(target_arch = "wasm32")]
mod auth {
    pub use oasis7_client_api::viewer_auth::verify_session_register_auth_proof_signature as verify_session_register_auth_proof;
    pub use oasis7_client_api::viewer_auth::*;
}
#[cfg(not(target_arch = "wasm32"))]
mod demo;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) use oasis7::runtime_host::gameplay_actions;
#[cfg(target_arch = "wasm32")]
pub(crate) use oasis7_client_api::gameplay_actions;
#[cfg(not(target_arch = "wasm32"))]
mod live;
mod protocol;
#[cfg(not(target_arch = "wasm32"))]
mod runtime_live;
#[cfg(all(test, not(target_arch = "wasm32")))]
pub(crate) fn canonical_runtime_provider_env_lock() -> &'static std::sync::Mutex<()> {
    runtime_live::canonical_runtime_provider_env_lock()
}
#[cfg(not(target_arch = "wasm32"))]
mod world_view;
#[cfg(not(target_arch = "wasm32"))]
mod world_view_authority;

#[cfg(not(target_arch = "wasm32"))]
pub(crate) use auth::{
    claim_registration_grant_nonce_for_recovery, consume_registration_grant_nonce,
};

pub use auth::{
    HOSTED_AUTHORIZATION_ISSUER_PUBLIC_KEY_ENV, HOSTED_AUTHORIZATION_MAX_AGE_MS,
    HOSTED_IDENTITY_V2_ENABLED_ENV, HOSTED_REGISTRATION_ISSUER_PRIVATE_KEY_ENV,
    HOSTED_REGISTRATION_ISSUER_PUBLIC_KEY_ENV, HostedAuthContextV2, PromptControlAuthIntent,
    VIEWER_HOSTED_STRONG_AUTH_GRANT_SIGNATURE_V1_PREFIX, VIEWER_PLAYER_AUTH_SIGNATURE_V1_PREFIX,
    VerifiedPlayerAuth, derive_hosted_registration_issuer_public_key, hosted_identity_v2_enabled,
    hosted_identity_v2_enabled_checked, issue_hosted_authorization_grant,
    issue_hosted_registration_grant, sign_adjudicate_social_fact_quote_auth_proof,
    sign_agent_chat_auth_proof, sign_collect_data_auth_proof,
    sign_declare_social_edge_quote_auth_proof, sign_director_capability_grant,
    sign_fragment_refill_preview_auth_proof, sign_gameplay_action_auth_proof,
    sign_governance_vote_quote_auth_proof, sign_hosted_prompt_control_strong_auth_grant,
    sign_market_quote_decision_auth_proof, sign_power_sale_quote_auth_proof,
    sign_power_survival_quote_auth_proof, sign_product_validation_quote_auth_proof,
    sign_prompt_control_apply_auth_proof, sign_prompt_control_rollback_auth_proof,
    sign_publish_social_fact_quote_auth_proof, sign_refine_quote_auth_proof,
    sign_revoke_social_fact_quote_auth_proof, sign_schedule_recipe_quote_auth_proof,
    sign_session_register_auth_proof, sign_social_contact_quote_auth_proof,
    sign_transfer_material_quote_auth_proof, sign_war_declaration_quote_auth_proof,
    verify_adjudicate_social_fact_quote_auth_proof, verify_agent_chat_auth_proof,
    verify_collect_data_auth_proof, verify_declare_social_edge_quote_auth_proof,
    verify_director_capability_grant, verify_fragment_refill_preview_auth_proof,
    verify_gameplay_action_auth_proof, verify_governance_vote_quote_auth_proof,
    verify_hosted_prompt_control_apply_strong_auth_grant,
    verify_hosted_prompt_control_rollback_strong_auth_grant,
    verify_market_quote_decision_auth_proof, verify_power_sale_quote_auth_proof,
    verify_power_survival_quote_auth_proof, verify_product_validation_quote_auth_proof,
    verify_prompt_control_apply_auth_proof, verify_prompt_control_rollback_auth_proof,
    verify_publish_social_fact_quote_auth_proof, verify_refine_quote_auth_proof,
    verify_revoke_social_fact_quote_auth_proof, verify_schedule_recipe_quote_auth_proof,
    verify_session_register_auth_proof, verify_social_contact_quote_auth_proof,
    verify_transfer_material_quote_auth_proof, verify_war_declaration_quote_auth_proof,
};
#[cfg(not(target_arch = "wasm32"))]
pub use demo::{ViewerDemoError, ViewerDemoSummary, generate_viewer_demo};
#[cfg(not(target_arch = "wasm32"))]
pub use gameplay_actions::build_runtime_action_from_gameplay_request;
pub use gameplay_actions::{
    ACTION_BUILD_ASSEMBLER_MK1, ACTION_BUILD_SMELTER_MK1, ACTION_CLAIM_AGENT,
    ACTION_CLAIM_FIRST_AGENT, ACTION_CLAIM_STARTER_OC, ACTION_RELEASE_AGENT_CLAIM,
    ACTION_SCHEDULE_ASSEMBLER_CONTROL_CHIP, ACTION_SCHEDULE_ASSEMBLER_FACTORY_CORE,
    ACTION_SCHEDULE_ASSEMBLER_GEAR, ACTION_SCHEDULE_ASSEMBLER_LOGISTICS_DRONE,
    ACTION_SCHEDULE_ASSEMBLER_MODULE_RACK, ACTION_SCHEDULE_ASSEMBLER_MOTOR_MK1,
    ACTION_SCHEDULE_ASSEMBLER_SENSOR_PACK, ACTION_SCHEDULE_SMELTER_ALLOY_PLATE,
    ACTION_SCHEDULE_SMELTER_COPPER_WIRE, ACTION_SCHEDULE_SMELTER_IRON_INGOT,
    ACTION_SCHEDULE_SMELTER_POLYMER_RESIN, FACTORY_ASSEMBLER_MK1, FACTORY_SMELTER_MK1,
    FIRST_AGENT_CLAIM_TARGET_AGENT_ID, gameplay_action_requires_actor_agent,
};
#[cfg(not(target_arch = "wasm32"))]
pub use live::{
    ViewerLiveDecisionMode, ViewerLiveServer, ViewerLiveServerConfig, ViewerLiveServerError,
};
pub use oasis7_client_api::rollback_audit_evidence::{
    RollbackStrictAuditEvidenceInput, build_unsigned_strict_audit_evidence,
    strict_audit_artifact_digest, strict_audit_manifest_digest,
};
pub use protocol::{
    AdjudicateSocialFactQuotePreflight, AdjudicateSocialFactQuoteRequest, AgentChatAck,
    AgentChatError, AgentChatRequest, AuthoritativeBatchFinality, AuthoritativeChallengeAck,
    AuthoritativeChallengeCommand, AuthoritativeChallengeError,
    AuthoritativeChallengeResolveRequest, AuthoritativeChallengeStatus,
    AuthoritativeChallengeSubmitRequest, AuthoritativeFinalityState,
    AuthoritativeReconnectSyncRequest, AuthoritativeRecoveryAck, AuthoritativeRecoveryCommand,
    AuthoritativeRecoveryError, AuthoritativeRecoveryStatus, AuthoritativeRollbackRequest,
    AuthoritativeSessionRegisterRequest, AuthoritativeSessionRevokeRequest,
    AuthoritativeSessionRotateRequest, CollectDataCommand, CollectDataPreflight,
    CollectDataRequest, ControlCompletionAck, ControlCompletionStatus, DIRECTOR_CAPABILITY_ACTION,
    DIRECTOR_CAPABILITY_AUDIENCE, DIRECTOR_CAPABILITY_DOMAIN, DIRECTOR_CAPABILITY_GRANT_VERSION,
    DIRECTOR_CAPABILITY_MAX_TTL_MS, DIRECTOR_CAPABILITY_SCOPE,
    DIRECTOR_CAPABILITY_SIGNATURE_V1_PREFIX, DeclareSocialEdgeQuotePreflight,
    DeclareSocialEdgeQuoteRequest, DirectorCapabilityGrant, FirstContactClass,
    FragmentRefillElementRemaining, FragmentRefillPreviewChunk, FragmentRefillPreviewPreflight,
    FragmentRefillPreviewProtocolRequest, FragmentRefillPreviewRequest,
    FragmentRefillPreviewResponse, GameplayActionAck, GameplayActionError, GameplayActionRequest,
    GovernanceVoteQuotePreflight, GovernanceVoteQuoteRequest, HostedSessionReleaseRequest,
    HostedStrongAuthGrant, LiveControl, MarketQuoteDecisionPreflight, MarketQuoteDecisionRequest,
    MarketQuoteMaterialContribution, MarketQuoteMaterialRequest, PlaybackControl, PlayerAuthProof,
    PlayerAuthScheme, PowerSaleQuotePreflight, PowerSaleQuoteRequest, PowerSurvivalQuotePreflight,
    PowerSurvivalQuoteRequest, PowerSurvivalRecoveryAction, ProductValidationQuotePreflight,
    ProductValidationQuoteRequest, PromptControlAck, PromptControlApplyRequest,
    PromptControlCommand, PromptControlError, PromptControlOperation, PromptControlRollbackRequest,
    PublishSocialFactQuotePreflight, PublishSocialFactQuoteRequest, PublishSocialFactQuoteStake,
    RefineQuotePreflight, RefineQuoteRequest, RevokeSocialFactQuotePreflight,
    RevokeSocialFactQuoteRequest, ScheduleRecipeQuotePreflight, ScheduleRecipeQuoteRequest,
    SocialAdjudicationDecision, SocialContactQuotePreflight, SocialContactQuoteRequest,
    TransferMaterialPriority, TransferMaterialQuotePreflight, TransferMaterialQuoteRequest,
    VIEWER_PROTOCOL_VERSION, ViewerControl, ViewerControlProfile, ViewerEventKind, ViewerRequest,
    ViewerResponse, ViewerStream, WORLD_FEED_SCHEMA_VERSION, WarDeclarationQuotePreflight,
    WarDeclarationQuoteRequest, WorldFeedEnvelope, WorldFeedEvent, WorldFeedGapReason,
    WorldFeedStatus, WorldFeedUnavailableReason,
};
#[cfg(not(target_arch = "wasm32"))]
pub use runtime_live::{
    ChainLinkPolicy, ProviderBackedBootstrapAuthorityV1, VIEWER_FORMAL_RELEASE_DEFAULT_WORLD_ID,
    ViewerRuntimeLiveServer, ViewerRuntimeLiveServerConfig, ViewerRuntimeLiveServerError,
    runtime_agent_chat_echo_enabled_from_env, viewer_bootstrap_formal_release_runtime_world,
    viewer_bootstrap_generated_sidecar_runtime_world,
};

// Playback projection remains a runtime authority; transport hosts consume it.
pub use protocol::viewer_event_kind_matches;
#[cfg(not(target_arch = "wasm32"))]
pub use world_view::build as build_playback_world_view;

#[cfg(not(target_arch = "wasm32"))]
pub use auth::{
    ExclusiveDirectoryProcessLock, HOSTED_REGISTRATION_REPLAY_LEDGER_PATH_ENV,
    preflight_hosted_registration_replay_ledger,
};
