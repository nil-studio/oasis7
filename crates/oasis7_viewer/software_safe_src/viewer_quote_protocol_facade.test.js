import { describe, expect, it, vi } from "vitest";
import { createViewerQuoteProtocolFacade } from "./viewer_quote_protocol_facade.js";

function integration() {
  return {
    handleMarketQuoteDecision: vi.fn(), handleMarketQuoteDecisionError: vi.fn(() => false),
    handlePowerSaleQuote: vi.fn(), handlePowerSaleQuoteError: vi.fn(() => false),
    handlePowerSurvivalQuote: vi.fn(), handlePowerSurvivalQuoteError: vi.fn(() => false),
    handleScheduleRecipeQuote: vi.fn(), handleScheduleRecipeQuoteError: vi.fn(() => false), invalidateScheduleRecipeQuote: vi.fn(),
    handleProductValidationQuote: vi.fn(), handleProductValidationQuoteError: vi.fn(() => false),
    handleFragmentRefillPreview: vi.fn(), handleFragmentRefillPreviewError: vi.fn(() => false),
    handleGovernanceVoteQuote: vi.fn(), handleGovernanceVoteQuoteError: vi.fn(() => false),
    handleWarDeclarationQuote: vi.fn(), handleWarDeclarationQuoteError: vi.fn(() => false),
    invalidateWarDeclarationQuoteForAuthoritativeSnapshot: vi.fn(),
    invalidateFragmentRefillPreview: vi.fn(),
    invalidatePowerSurvivalQuote: vi.fn(),
  };
}

describe("viewer quote protocol facade", () => {
  it("keeps an in-flight war quote across an unrelated snapshot while clearing snapshot-bound quotes", () => {
    const state = {
      marketQuoteDecision: { value: "old" }, marketQuoteDecisionRequest: { status: "received" },
      warDeclarationQuote: { quoted_at_tick: 12 }, warDeclarationQuoteRequest: { status: "pending", requestKey: "red|blue|3" },
    };
    const powerSurvivalQuote = integration();
    const fragmentRefillPreview = integration();
    const facade = createViewerQuoteProtocolFacade({
      fragmentRefillPreview,
      handleRefineQuoteError: vi.fn(() => false), handleRefineQuotePreflight: vi.fn(),
      marketQuoteDecision: integration(), powerSurvivalQuote, productValidationQuote: integration(), state,
      warDeclarationQuote: integration(),
    });

    facade.invalidateSnapshotBoundQuotes();

    expect(powerSurvivalQuote.invalidatePowerSurvivalQuote).toHaveBeenCalledOnce();
    expect(fragmentRefillPreview.invalidateFragmentRefillPreview).toHaveBeenCalledOnce();
    expect(state.marketQuoteDecision).toBeNull();
    expect(state.marketQuoteDecisionRequest).toEqual({ status: "idle", error: null });
    expect(state.warDeclarationQuoteRequest).toEqual({ status: "pending", requestKey: "red|blue|3" });
    expect(state.warDeclarationQuote).toEqual({ quoted_at_tick: 12 });
  });

  it("routes war quote replies through the extracted protocol boundary", () => {
    const warDeclarationQuote = integration();
    const facade = createViewerQuoteProtocolFacade({
      fragmentRefillPreview: integration(),
      handleRefineQuoteError: vi.fn(() => false), handleRefineQuotePreflight: vi.fn(),
      marketQuoteDecision: integration(), powerSurvivalQuote: integration(), productValidationQuote: integration(),
      state: {}, warDeclarationQuote,
    });
    const quote = { actor_alliance_id: "alliance.red" };

    expect(facade.handleQuoteViewerMessage({ type: "war_declaration_quote_preflight", quote })).toBe(true);
    expect(warDeclarationQuote.handleWarDeclarationQuote).toHaveBeenCalledWith(quote);
  });

  it("routes the schedule recipe preflight through the quote boundary", () => {
    const scheduleRecipeQuote = integration();
    const facade = createViewerQuoteProtocolFacade({
      fragmentRefillPreview: integration(), handleRefineQuoteError: vi.fn(() => false), handleRefineQuotePreflight: vi.fn(),
      marketQuoteDecision: integration(), powerSurvivalQuote: integration(), productValidationQuote: integration(), scheduleRecipeQuote,
      state: {}, warDeclarationQuote: integration(),
    });
    const quote = { factory_id: "factory-0", recipe_id: "assemble_hardware", batches: 2 };
    expect(facade.handleQuoteViewerMessage({ type: "schedule_recipe_quote_preflight", quote })).toBe(true);
    expect(scheduleRecipeQuote.handleScheduleRecipeQuote).toHaveBeenCalledWith(quote);
  });

  it("routes the authenticated governance vote quote with its player decision fields", () => {
    const governanceVoteQuote = integration();
    const facade = createViewerQuoteProtocolFacade({
      fragmentRefillPreview: integration(),
      governanceVoteQuote,
      handleRefineQuoteError: vi.fn(() => false), handleRefineQuotePreflight: vi.fn(),
      marketQuoteDecision: integration(), powerSurvivalQuote: integration(), productValidationQuote: integration(),
      state: {}, warDeclarationQuote: integration(),
    });
    const quote = {
      proposal_id: "proposal.viewer-governance-quote", proposal_topic: "Keep the solar reserve",
      actor_id: "agent-0", action_kind: "cast_governance_vote", closes_at_tick: 17, ticks_remaining: 12,
      current_quorum_weight: 0, required_quorum_weight: 3, current_pass_bps: 0, required_pass_bps: 6000,
      actor_vote_weight: 3, vote_swing_potential: 3, likely_outcome_before_action: "rejected",
      likely_outcome_after_action: "passed", affected_rule_or_priority: "Keep the solar reserve",
      world_change_if_passed: "Prioritize the solar reserve over an emergency drawdown.",
      cost_or_cooldown_if_failed: "No governance action cost or cooldown is defined for this proposal.",
      recommended_governance_action: "cast_vote", why_this_vote_matters: "This vote changes the likely outcome.",
    };

    expect(facade.handleQuoteViewerMessage({ type: "governance_vote_quote_preflight", quote })).toBe(true);
    expect(governanceVoteQuote.handleGovernanceVoteQuote).toHaveBeenCalledWith(quote);
  });

  it("routes a power sale preflight through the seller-side quote boundary", () => {
    const powerSaleQuote = integration();
    const facade = createViewerQuoteProtocolFacade({
      fragmentRefillPreview: integration(),
      handleRefineQuoteError: vi.fn(() => false), handleRefineQuotePreflight: vi.fn(),
      marketQuoteDecision: integration(), powerSaleQuote, powerSurvivalQuote: integration(), productValidationQuote: integration(),
      state: {}, warDeclarationQuote: integration(),
    });
    const quote = { seller_agent_id: "agent-seller", buyer_agent_id: "agent-buyer", production_interrupt_risk: true };

    expect(facade.handleQuoteViewerMessage({ type: "power_sale_quote_preflight", quote })).toBe(true);
    expect(powerSaleQuote.handlePowerSaleQuote).toHaveBeenCalledWith(quote);
  });

  it("invalidates a received quote when an authoritative snapshot changes at the same tick", () => {
    const state = {
      warDeclarationQuote: { quoted_at_tick: 12, state_fingerprint: "before" },
      warDeclarationQuoteRequest: { status: "received", error: null },
    };
    const warDeclarationQuote = integration();
    const fragmentRefillPreview = integration();
    const facade = createViewerQuoteProtocolFacade({
      fragmentRefillPreview,
      handleRefineQuoteError: vi.fn(() => false), handleRefineQuotePreflight: vi.fn(),
      marketQuoteDecision: integration(), powerSurvivalQuote: integration(), productValidationQuote: integration(), state,
      warDeclarationQuote,
    });

    facade.invalidateSnapshotBoundQuotes();

    expect(fragmentRefillPreview.invalidateFragmentRefillPreview).toHaveBeenCalledOnce();
    expect(warDeclarationQuote.invalidateWarDeclarationQuoteForAuthoritativeSnapshot).toHaveBeenCalledOnce();
  });
});

it("invalidates the two missing quotes on dependency changes and leaves unrelated snapshot updates valid",()=>{
 const snapshot={time:1,config:{economy:{}},model:{agents:{a:{resources:{amounts:{data:40,electricity:20}}}}},runtime_snapshot:{state:{industry_progress:{stage:"bootstrap"},product_profiles:{sensor:{role_tag:"build",tradable:true,unlock_stage:"bootstrap"}}}}};
 const state={auth:{boundAgentId:"a"},snapshot,refineQuotePreflight:{value:"refine"},refineQuoteRequest:{status:"received"},productValidationQuote:{value:"product"},productValidationQuoteRequest:{status:"received"}};
 const facade=createViewerQuoteProtocolFacade({fragmentRefillPreview:integration(),handleRefineQuoteError:vi.fn(),handleRefineQuotePreflight:vi.fn(),marketQuoteDecision:integration(),powerSurvivalQuote:integration(),productValidationQuote:integration(),state,warDeclarationQuote:integration()});
 facade.invalidateSnapshotBoundQuotes({...snapshot,time:2});
 expect(state.refineQuotePreflight).toEqual({value:"refine"}); expect(state.productValidationQuote).toEqual({value:"product"});
 const changed=structuredClone(snapshot); changed.model.agents.a.resources.amounts.electricity=0; changed.runtime_snapshot.state.industry_progress.stage="scale_out";
 facade.invalidateSnapshotBoundQuotes(changed);
 expect(state.refineQuotePreflight).toBeNull(); expect(state.productValidationQuote).toBeNull();
});
