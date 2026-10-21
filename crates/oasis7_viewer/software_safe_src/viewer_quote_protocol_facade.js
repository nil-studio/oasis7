import { invalidateDependentQuote } from "./viewer_quote_dependencies.js";
export function createViewerQuoteProtocolFacade({
  fragmentRefillPreview,
  governanceVoteQuote,
  handleRefineQuoteError,
  handleRefineQuotePreflight,
  marketQuoteDecision,
  powerSaleQuote,
  powerSurvivalQuote,
  productValidationQuote,
  scheduleRecipeQuote,
  state,
  transferMaterialQuote,
  warDeclarationQuote,
}) {
  function handleQuoteGameplayActionError(error) {
    return handleRefineQuoteError(error)
      || productValidationQuote.handleProductValidationQuoteError(error)
      || powerSaleQuote?.handlePowerSaleQuoteError(error)
      || powerSurvivalQuote.handlePowerSurvivalQuoteError(error)
      || scheduleRecipeQuote?.handleScheduleRecipeQuoteError(error)
      || transferMaterialQuote?.handleTransferMaterialQuoteError(error)
      || fragmentRefillPreview.handleFragmentRefillPreviewError(error)
      || governanceVoteQuote?.handleGovernanceVoteQuoteError(error)
      || warDeclarationQuote.handleWarDeclarationQuoteError(error)
      || marketQuoteDecision.handleMarketQuoteDecisionError(error);
  }

  function handleQuoteViewerMessage(message) {
    switch (message?.type) {
      case "market_quote_decision_preflight":
        marketQuoteDecision.handleMarketQuoteDecision(message.quote);
        return true;
      case "refine_quote_preflight":
        handleRefineQuotePreflight(message.quote);
        return true;
      case "schedule_recipe_quote_preflight":
        if (!scheduleRecipeQuote) return false;
        scheduleRecipeQuote.handleScheduleRecipeQuote(message.quote);
        return true;
      case "transfer_material_quote_preflight":
        transferMaterialQuote?.handleTransferMaterialQuote(message.quote);
        return true;
      case "product_validation_quote_preflight":
        productValidationQuote.handleProductValidationQuote(message.quote);
        return true;
      case "power_survival_quote_preflight":
        powerSurvivalQuote.handlePowerSurvivalQuote(message.quote);
        return true;
      case "power_sale_quote_preflight":
        if (!powerSaleQuote) return false;
        powerSaleQuote.handlePowerSaleQuote(message.quote);
        return true;
      case "war_declaration_quote_preflight":
        warDeclarationQuote.handleWarDeclarationQuote(message.quote);
        return true;
      case "governance_vote_quote_preflight":
        if (!governanceVoteQuote) return false;
        governanceVoteQuote.handleGovernanceVoteQuote(message.quote);
        return true;
      case "fragment_refill_preview_preflight":
        fragmentRefillPreview.handleFragmentRefillPreview(message.quote);
        return true;
      default:
        return false;
    }
  }

  function invalidateSnapshotBoundQuotes(snapshot = state.snapshot) {
    invalidateDependentQuote(state, "refine", snapshot);
    invalidateDependentQuote(state, "product", snapshot);
    powerSaleQuote?.invalidatePowerSaleQuote();
    powerSurvivalQuote.invalidatePowerSurvivalQuote();
    scheduleRecipeQuote?.invalidateScheduleRecipeQuote();
    transferMaterialQuote?.invalidateTransferMaterialQuote();
    fragmentRefillPreview.invalidateFragmentRefillPreview();
    governanceVoteQuote?.invalidateGovernanceVoteQuote();
    warDeclarationQuote.invalidateWarDeclarationQuoteForAuthoritativeSnapshot();
    state.marketQuoteDecision = null;
    state.marketQuoteDecisionRequest = { status: "idle", error: null };
    // In-flight signed requests retain their correlation; completed quotes do
    // not survive a fresh authoritative snapshot, even at the same tick.
  }

  return { handleQuoteGameplayActionError, handleQuoteViewerMessage, invalidateSnapshotBoundQuotes };
}
