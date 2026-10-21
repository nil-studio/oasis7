import { fireEvent, render, screen, within } from "@solidjs/testing-library";
import { describe, expect, it, vi } from "vitest";
import { RefineQuotePreflightCard, RefineQuotePreflightPanel } from "./refine_quote_preflight_card.jsx";

const quote = {
  owner_agent_id: "agent-0",
  compound_mass_g: 40,
  electricity_cost: 12,
  electricity_after: 88,
  hardware_output: 20,
  target_id: "factory_build_hardware",
  target_gap_before: 20,
  target_gap_after: 0,
  target_linkage: "enables_factory_build_hardware_goal",
  recommended_refine_amount: 40,
  value_classification: "enough_to_advance",
};

function tr(locale, zh, en) {
  return locale === "zh" ? zh : en;
}

describe("RefineQuotePreflightCard", () => {
  it("renders a localized read-only quote without presenting a receipt or submit action", () => {
    const { unmount } = render(() => <RefineQuotePreflightCard quote={quote} locale="en" tr={tr} />);
    const card = screen.getByTestId("refine-quote-preflight");

    expect(card).toHaveAttribute("data-quote-kind", "preflight");
    expect(within(card).getByText("Compound Refining Quote")).toBeInTheDocument();
    expect(within(card).getByText(/read-only quote\. It does not submit refining/i)).toBeInTheDocument();
    expect(within(card).getByText("Electricity cost")).toBeInTheDocument();
    expect(within(card).getByText("Electricity remaining")).toBeInTheDocument();
    expect(within(card).getByText("Hardware output")).toBeInTheDocument();
    expect(within(card).getByText("20 → 0")).toBeInTheDocument();
    const targetGap = within(card).getByText("Target gap").closest(".metric");
    expect(targetGap).toHaveClass("metric--target-gap-outcome");
    expect(card.querySelector(".summary-grid")?.firstElementChild).toBe(targetGap);
    expect(within(card).getByText(/This output satisfies the factory hardware target/i)).toBeInTheDocument();
    expect(within(card).getByText(/Value assessment: Enough to advance/i)).toBeInTheDocument();
    expect(within(card).getByText(/Next decision: This quote can advance the target/i)).toBeInTheDocument();
    const targetBadge = within(card).getByText(/target: Factory hardware build/i);
    expect(targetBadge).toHaveAttribute("data-target-id", "factory_build_hardware");
    expect(within(card).queryByText("factory_build_hardware")).not.toBeInTheDocument();
    expect(within(card).queryByRole("button", { name: /refin|submit|commit/i })).not.toBeInTheDocument();

    unmount();
    render(() => <RefineQuotePreflightCard quote={quote} locale="zh" tr={tr} />);
    const zhCard = screen.getByTestId("refine-quote-preflight");
    expect(within(zhCard).getByText("化合物精炼预估")).toBeInTheDocument();
    expect(within(zhCard).getByText(/这是只读预估，不会提交精炼、扣除电力或生成回执/)).toBeInTheDocument();
    expect(within(zhCard).getByText("电力成本")).toBeInTheDocument();
    expect(within(zhCard).getByText(/目标关联: 本次产出可满足工厂硬件目标/)).toBeInTheDocument();
    expect(within(zhCard).getByText(/价值判断: 足以推进下一步/)).toBeInTheDocument();
    expect(within(zhCard).getByText(/下一步建议: 这笔预估足以推进目标/)).toBeInTheDocument();
    expect(within(zhCard).queryByText("enough_to_advance")).not.toBeInTheDocument();
    expect(within(zhCard).queryByText("enables_factory_build_hardware_goal")).not.toBeInTheDocument();
  });

  it("keeps the authenticated read-only quote trigger reachable before a quote and reports request state", async () => {
    const requestRefineQuote = vi.fn(async () => ({ ok: true }));
    const { unmount } = render(() => <RefineQuotePreflightPanel quote={null} requestRefineQuote={requestRefineQuote} locale="en" tr={tr} />);

    const panel = screen.getByTestId("refine-quote-panel");
    expect(within(panel).getByRole("button", { name: "Request quote" })).toBeInTheDocument();
    expect(within(panel).queryByTestId("refine-quote-preflight")).not.toBeInTheDocument();
    expect(within(panel).getByText(/does not submit refining, spend electricity, or create a receipt/i)).toBeInTheDocument();

    const amount = within(panel).getByRole("spinbutton", { name: "Refine amount (g)" });
    fireEvent.input(amount, { target: { value: "25" } });
    fireEvent.submit(screen.getByTestId("refine-quote-request-form"));
    await vi.waitFor(() => expect(requestRefineQuote).toHaveBeenCalledWith("25"));
    expect(within(panel).getByRole("status")).toHaveTextContent(/Read-only quote requested/i);
    expect(within(panel).queryByRole("button", { name: /submit refining|commit refining/i })).not.toBeInTheDocument();

    unmount();
    render(() => <RefineQuotePreflightPanel quote={quote} requestRefineQuote={requestRefineQuote} locale="en" tr={tr} />);
    expect(within(screen.getByTestId("refine-quote-panel")).getByTestId("refine-quote-preflight")).toBeInTheDocument();
  });

  it("keeps a failed quote request visible without presenting a refine receipt", async () => {
    const requestRefineQuote = vi.fn(async () => ({ ok: false, reason: "refine quote requires a connected viewer websocket" }));
    render(() => <RefineQuotePreflightPanel quote={null} requestRefineQuote={requestRefineQuote} locale="en" tr={tr} />);

    fireEvent.submit(screen.getByTestId("refine-quote-request-form"));
    await vi.waitFor(() => expect(screen.getByRole("alert")).toHaveTextContent(/Could not get the refining quote/i));
    expect(screen.getByRole("alert")).not.toHaveTextContent("refine quote requires a connected viewer websocket");
    expect(screen.queryByTestId("refine-quote-preflight")).not.toBeInTheDocument();
    expect(screen.queryByText("Refining receipt")).not.toBeInTheDocument();
  });

  it("replaces waiting feedback with the server result", () => {
    const requestRefineQuote = vi.fn(async () => ({ ok: true }));
    render(() => (
      <RefineQuotePreflightPanel
        quote={quote}
        requestRefineQuote={requestRefineQuote}
        requestState={{ status: "received", error: null }}
        locale="en"
        tr={tr}
      />
    ));
    expect(screen.getByRole("status")).toHaveTextContent(/Quote received; review the estimate below/i);
    expect(screen.queryByText(/waiting for the quote result/i)).not.toBeInTheDocument();

    render(() => (
      <RefineQuotePreflightPanel
        quote={null}
        requestRefineQuote={requestRefineQuote}
        requestState={{ status: "error", error: "quote_refine_compound rejected" }}
        locale="en"
        tr={tr}
      />
    ));
    expect(screen.getAllByRole("alert").at(-1)).toHaveTextContent(/Could not get the refining quote/i);
    expect(screen.getAllByRole("alert").at(-1)).not.toHaveTextContent("quote_refine_compound rejected");
    expect(screen.queryByText(/waiting for the quote result/i)).not.toBeInTheDocument();

    render(() => (
      <RefineQuotePreflightPanel
        quote={null}
        requestRefineQuote={requestRefineQuote}
        requestState={{ status: "error", error: "quote_refine_compound rejected" }}
        locale="zh"
        tr={tr}
      />
    ));
    expect(screen.getAllByRole("alert").at(-1)).toHaveTextContent(/无法获取精炼预估/);
    expect(screen.getAllByRole("alert").at(-1)).not.toHaveTextContent("quote_refine_compound");
  });

  it.each([
    ["partial_progress", /compare recharging, mining, or waiting before choosing a supported gameplay action/i],
    ["poor_power_tradeoff", /recharge, mine, or wait, then adjust the plan and request a new estimate/i],
  ])("gives localized next-decision guidance for %s", (valueClassification, guidance) => {
    render(() => <RefineQuotePreflightCard quote={{ ...quote, value_classification: valueClassification }} locale="en" tr={tr} />);
    expect(screen.getByTestId("refine-quote-next-decision")).toHaveTextContent(guidance);
  });
  it("explains a binding failure and exposes only the safe business code", () => {
    render(() => <RefineQuotePreflightPanel quote={null} requestRefineQuote={vi.fn()}
      requestState={{ status: "error", errorCode: "player_agent_binding_required", error: "private server diagnostics" }} locale="en" tr={tr} />);
    expect(screen.getByRole("alert")).toHaveTextContent(/claim.*Agent|Agent.*sync/i);
    expect(screen.getByText("player_agent_binding_required")).toBeInTheDocument();
    expect(document.body).not.toHaveTextContent("private server diagnostics");
  });

  it.each(["constructor", "PRIVATE_DIAGNOSTIC_".repeat(10)])("does not echo an unrecognized code %s", code => {
    render(() => <RefineQuotePreflightPanel quote={null} requestRefineQuote={vi.fn()}
      requestState={{ status: "error", errorCode: code, error: "private detail" }} locale="en" tr={tr} />);
    expect(screen.getByText("unknown_error")).toBeInTheDocument();
    expect(screen.getByRole("alert")).not.toHaveTextContent("private detail");
  });

});
