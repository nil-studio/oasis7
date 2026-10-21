import { quoteDependencyFingerprint, quoteResponseMatches } from "./viewer_quote_dependencies.js";
import { VISUAL_FIXTURE_NAME, visualFixtureQuote } from "./refine_quote_preflight_state_visual_test_data.js";

export function createRefineQuotePreflightStateModule({ clone, getSearchParams, isTestApiEnabled, render, state }) {
  function handleRefineQuotePreflight(quote) {
    if (!quote || typeof quote !== "object" || !quoteResponseMatches(state, state.refineQuoteRequest, quote, "refine")) return;
    // A quote is kept outside gameplay-action feedback so it cannot be presented
    // as an accepted action or receipt.
    state.refineQuotePreflight = clone(quote);
    state.refineQuoteRequest = { ...state.refineQuoteRequest, status: "received", error: null };
  }

  function handleRefineQuoteError(error) {
    if (String(error?.action_id || "").trim() !== "quote_refine_compound") return false;
    if (!quoteResponseMatches(state, state.refineQuoteRequest, error, "refine")) return true;
    state.refineQuoteRequest = {
      ...state.refineQuoteRequest,
      status: "error",
      errorCode: String(error?.code || "unknown_error"),
      error: String(error?.message || error?.code || "refine quote request failed"),
    };
    return true;
  }

  function injectRefineQuotePreflightForTest(quote) {
    if (!isTestApiEnabled()) {
      throw new Error("injectRefineQuotePreflightForTest requires test_api=1");
    }
    state.refineQuotePreflight = clone(quote);
    state.refineQuoteRequest = { status: "received", error: null, dependencyFingerprint: quoteDependencyFingerprint(state, "refine") };
    render();
    return clone(state.refineQuotePreflight);
  }

  function installRefineQuotePreflightVisualFixture() {
    if (!isTestApiEnabled() || getSearchParams().get("fixture") !== VISUAL_FIXTURE_NAME) return;
    state.refineQuotePreflight = clone(visualFixtureQuote);
    state.refineQuoteRequest = { status: "received", error: null, dependencyFingerprint: quoteDependencyFingerprint(state, "refine") };
  }

  return {
    handleRefineQuotePreflight,
    handleRefineQuoteError,
    injectRefineQuotePreflightForTest,
    installRefineQuotePreflightVisualFixture,
  };
}
