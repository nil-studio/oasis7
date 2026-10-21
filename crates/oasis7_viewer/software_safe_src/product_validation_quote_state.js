import { quoteDependencyFingerprint, quoteResponseMatches } from "./viewer_quote_dependencies.js";
import { VISUAL_FIXTURE_NAME, visualFixtureQuote } from "./product_validation_quote_state_visual_test_data.js";

export function createProductValidationQuoteStateModule({ clone, getSearchParams, isTestApiEnabled, render, state }) {
  function handleProductValidationQuote(quote) {
    if (!quote || typeof quote !== "object" || !quoteResponseMatches(state, state.productValidationQuoteRequest, quote, "product")) return;
    state.productValidationQuote = clone(quote);
    state.productValidationQuoteRequest = { ...state.productValidationQuoteRequest, status: "received", error: null };
  }

  function handleProductValidationQuoteError(error) {
    if (String(error?.action_id || "").trim() !== "quote_validate_product") return false;
    if (!quoteResponseMatches(state, state.productValidationQuoteRequest, error, "product")) return true;
    state.productValidationQuoteRequest = {
      ...state.productValidationQuoteRequest,
      status: "error",
      error: String(error?.message || error?.code || "product validation quote request failed"),
    };
    return true;
  }

  function injectProductValidationQuoteForTest(quote) {
    if (!isTestApiEnabled()) {
      throw new Error("injectProductValidationQuoteForTest requires test_api=1");
    }
    state.productValidationQuote = clone(quote);
    state.productValidationQuoteRequest = { status: "received", error: null, dependencyFingerprint: quoteDependencyFingerprint(state, "product") };
    render();
    return clone(state.productValidationQuote);
  }

  function installProductValidationQuoteVisualFixture() {
    if (!isTestApiEnabled() || getSearchParams().get("fixture") !== VISUAL_FIXTURE_NAME) return;
    state.productValidationQuote = clone(visualFixtureQuote);
    state.productValidationQuoteRequest = { status: "received", error: null, dependencyFingerprint: quoteDependencyFingerprint(state, "product") };
  }

  return {
    handleProductValidationQuote,
    handleProductValidationQuoteError,
    injectProductValidationQuoteForTest,
    installProductValidationQuoteVisualFixture,
  };
}
