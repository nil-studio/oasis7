import { readFileSync } from "node:fs";
import { scheduleRecipeQuotes } from "./schedule_recipe_quote_fixture.js";
import { fireEvent, render, screen, within } from "@solidjs/testing-library";
import { describe, expect, it, vi } from "vitest";
import { ScheduleRecipeQuoteCard, ScheduleRecipeQuotePanel } from "./schedule_recipe_quote_card.jsx";

const normalQuote = scheduleRecipeQuotes.low;

function tr(locale, zh, en) { return locale === "zh" ? zh : en; }

describe("ScheduleRecipeQuoteCard", () => {
  it("keeps the pre-submit quote read-only while separating electricity from battery runway", () => {
    render(() => <ScheduleRecipeQuoteCard quote={normalQuote} locale="en" tr={tr} />);
    const card = screen.getByTestId("schedule-recipe-quote");

    expect(card).toHaveAttribute("data-quote-kind", "preflight");
    expect(within(card).getByText("Schedule Recipe Quote")).toBeInTheDocument();
    expect(within(card).getByText(/read-only quote.*does not schedule production/i)).toBeInTheDocument();
    expect(within(card).getByText("Electricity after scheduling")).toBeInTheDocument();
    expect(within(card).getByText("Battery runway")).toBeInTheDocument();
    expect(within(card).getByText("88")).toBeInTheDocument();
    expect(within(card).getByText("40 → 40 ticks")).toBeInTheDocument();
    expect(within(card).queryByText("Hardware input")).not.toBeInTheDocument();
    expect(within(card).queryByText("Data output")).not.toBeInTheDocument();
    expect(within(card).getByText(/Maintenance: not tracked for this quote/i)).toBeInTheDocument();
    expect(within(card).queryByRole("button", { name: /schedule|submit|commit/i })).not.toBeInTheDocument();
  });

  it.each(["zh", "en"])("renders producer risk and recovery tokens in %s", (locale) => {
    for (const risk of ["low", "elevated"]) {
      const { unmount } = render(() => <ScheduleRecipeQuoteCard quote={scheduleRecipeQuotes[risk]} locale={locale} tr={tr} />);
      const warning = screen.getByTestId("schedule-recipe-quote-risk");
      const recommendation = screen.getByTestId("schedule-recipe-quote-recommendation");
      expect(warning).toHaveTextContent(risk === "low" ? tr(locale, "低电力风险", "Low power risk") : tr(locale, "电力风险升高", "Elevated power risk"));
      expect(recommendation).toHaveTextContent(risk === "low" ? tr(locale, "可安排生产", "Schedule production now") : tr(locale, "先恢复电力，再安排生产", "Restore power before scheduling production"));
      expect(warning.classList.contains("feedback-summary--warn")).toBe(risk === "elevated");
      expect(recommendation.classList.contains("feedback-summary--warn")).toBe(risk === "elevated");
      unmount();
    }
  });

  it("keeps the shared fixture vocabulary aligned with the Rust producer", () => {
    const producer = readFileSync("../oasis7_simulator/src/kernel/actions_schedule_quote.rs", "utf8");
    expect(producer).toContain('if risk_is_elevated { "elevated" } else { "low" }');
    for (const quote of Object.values(scheduleRecipeQuotes)) {
      expect(producer).toContain(`"${quote.recommended_pre_step}"`);
    }
  });

  it.each(["zh", "en"])("keeps unknown tokens conservative in %s", (locale) => {
    render(() => <ScheduleRecipeQuoteCard quote={{ ...normalQuote, continue_production_risk: "future_risk", recommended_pre_step: "future_step" }} locale={locale} tr={tr} />);
    expect(screen.getByTestId("schedule-recipe-quote-risk")).toHaveTextContent(tr(locale, "电力风险待确认", "Power risk pending"));
    expect(screen.getByTestId("schedule-recipe-quote-recommendation")).toHaveTextContent(tr(locale, "重新请求预估后再决定", "Request a fresh quote before deciding"));
  });

  it.each(["critical", "shutdown"])("warns before neutral metrics for %s power risk", (continueProductionRisk) => {
    render(() => <ScheduleRecipeQuoteCard quote={{ ...normalQuote, continue_production_risk: continueProductionRisk, recommended_pre_step: "restore_power", runway_before_ticks: 3, runway_after_ticks: 3, local_shortage_delay_ticks: 5, shortage_reason: "local_hardware_shortage" }} locale="en" tr={tr} />);
    const card = screen.getByTestId("schedule-recipe-quote");
    const warning = within(card).getByTestId("schedule-recipe-quote-risk");
    const grid = card.querySelector(".summary-grid");

    expect(warning).toHaveClass("feedback-summary--warn");
    expect(warning.compareDocumentPosition(grid) & Node.DOCUMENT_POSITION_FOLLOWING).not.toBe(0);
    expect(within(warning).getByText(continueProductionRisk === "critical" ? /Critical power risk/ : /Shutdown risk/)).toBeInTheDocument();
    expect(within(card).getByTestId("schedule-recipe-quote-recommendation")).toHaveTextContent(/Restore power before scheduling production/i);
    expect(within(card).getByText(/Local shortage: 5 ticks/i)).toBeInTheDocument();
  });

  it("keeps normal risk neutral while keeping electricity and runway adjacent", () => {
    render(() => <ScheduleRecipeQuoteCard quote={normalQuote} locale="en" tr={tr} />);
    const card = screen.getByTestId("schedule-recipe-quote");
    expect(within(card).getByTestId("schedule-recipe-quote-risk")).not.toHaveClass("feedback-summary--warn");
    const labels = Array.from(card.querySelectorAll(".summary-grid .metric__label")).map((element) => element.textContent);
    expect(labels.indexOf("Battery runway")).toBe(labels.indexOf("Electricity after scheduling") + 1);
  });

  it("keeps the request reachable and presents pending or sanitized error feedback", async () => {
    const requestScheduleRecipeQuote = vi.fn(async () => ({ ok: true }));
    render(() => <ScheduleRecipeQuotePanel quote={null} requestState={{ status: "pending", error: null }} requestScheduleRecipeQuote={requestScheduleRecipeQuote} locale="en" tr={tr} />);
    const panel = screen.getByTestId("schedule-recipe-quote-panel");
    expect(within(panel).getByRole("status")).toHaveTextContent(/requesting quote/i);
    expect(within(panel).queryByTestId("schedule-recipe-quote")).not.toBeInTheDocument();

    render(() => <ScheduleRecipeQuotePanel quote={null} requestState={{ status: "error", error: "quote_schedule_recipe rejected" }} requestScheduleRecipeQuote={requestScheduleRecipeQuote} locale="en" tr={tr} />);
    expect(screen.getAllByRole("alert").at(-1)).toHaveTextContent(/Could not get the schedule quote/i);
    expect(screen.getAllByRole("alert").at(-1)).not.toHaveTextContent("quote_schedule_recipe rejected");

    render(() => <ScheduleRecipeQuotePanel quote={normalQuote} requestState={{ status: "received", error: null }} requestScheduleRecipeQuote={requestScheduleRecipeQuote} locale="en" tr={tr} />);
    fireEvent.submit(screen.getAllByTestId("schedule-recipe-quote-request-form").at(-1));
    await vi.waitFor(() => expect(requestScheduleRecipeQuote).toHaveBeenCalledWith("factory-0", "assemble_hardware", "1"));
  });
});
