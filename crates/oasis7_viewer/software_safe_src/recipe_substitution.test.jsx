import { describe, expect, it } from "vitest";
import { render, screen, cleanup } from "@solidjs/testing-library";
import { normalizeRecipeSubstitution } from "./recipe_substitution.js";
import { RecipeSubstitutionPanel } from "./recipe_substitution_panel.jsx";
const qualifiedRef = id => ({ id, world_binding: "world-1", domain: "industrial", schema: "recipe-substitution/v1", revision: 1, digest: "a".repeat(64) });
const decision = { decision_ref: qualifiedRef("choice-1"), intent: { scope: "decision_only", preview: { request: { root_ref: { id: "root-1" } } } } };
describe("canonical substitution read surface", () => {
  it("keeps absent private state absent", () => {
    expect(normalizeRecipeSubstitution(null)).toBeNull();
    const { container } = render(() => <RecipeSubstitutionPanel model={null} locale="en" />);
    expect(container.textContent).toBe(""); cleanup();
  });
  it("distinguishes unknown from a proved empty legal candidate set", () => {
    const model = normalizeRecipeSubstitution({ previews: [{ status: "unknown", candidates: [] }, { preview: { status: "no_legal_substitute", candidates: [] } }] });
    render(() => <RecipeSubstitutionPanel model={model} locale="en" />);
    expect(screen.getByText(/Substitution conditions unknown/)).toBeTruthy();
    expect(screen.getByText(/No legal substitute/)).toBeTruthy();
    expect(screen.getByText(/unknown compatibility provides no safe option/)).toBeTruthy(); cleanup();
  });
  it("never treats a decision receipt or unrelated settlement as conversion", () => {
    const model = normalizeRecipeSubstitution({ decisions: [decision], settlements: [{ decision_ref: { id: "other" }, receipt_ref: { id: "other-receipt" } }] });
    render(() => <RecipeSubstitutionPanel model={model} locale="en" />);
    expect(screen.getByText("Choice recorded only; conversion is not authorized.")).toBeTruthy();
    expect(screen.getByText("No conversion settlement receipt.")).toBeTruthy();
    expect(screen.queryByText(/Actual conversion settled/)).toBeNull(); cleanup();
  });
  it("does not link same naked ID from a different world", () => {
    const model = normalizeRecipeSubstitution({ decisions: [{ ...decision, decision_ref: { id: "choice-1", world_binding: "world-1" } }], settlements: [{ decision_ref: { id: "choice-1", world_binding: "world-2" }, receipt_ref: { id: "foreign" } }] });
    expect(model.decisions[0].settlement).toBeNull();
  });
  it("does not promote incomplete identities or unknown scopes into authority", () => {
    const model = normalizeRecipeSubstitution({ decisions: [{ ...decision, intent: {} }], settlements: [{ decision_ref: { id: "choice-1" }, receipt_ref: { id: "fake-settlement" } }] });
    expect(model.decisions[0].settlement).toBeNull();
    expect(model.decisions[0].scope).toBe("unknown");
  });
  it("shows exact linked settlement and rejects unsafe numbers", () => {
    const model = normalizeRecipeSubstitution({ decisions: [decision], settlements: [{ decision_ref: qualifiedRef("choice-1"), receipt_ref: qualifiedRef("settlement-1"), source_consumed: "999", source_loss: 2 }] });
    expect(model.decisions[0].consumed).toBeNull(); expect(model.decisions[0].loss).toBe(2);
    render(() => <RecipeSubstitutionPanel model={model} locale="en" />);
    expect(screen.getByText(/Actual conversion settled/).textContent).toContain("settlement-1");
    expect(screen.getByText(/Actual conversion settled/).textContent).toContain("Unpublished / 2"); cleanup();
  });
  it("separates new recipe scheduling power from conversion power", () => {
    const model = normalizeRecipeSubstitution({ previews: [{status:"available",candidates:[{kind:"new_recipe",authority:{target_recipe:"new-recipe",power_cost:0,terminal_capacity:100},recipe_plan:{consume:[],produce:[],power_required:4}}]}] });
    render(() => <RecipeSubstitutionPanel model={model} locale="en" />);
    expect(screen.getByText(/New recipe inputs \/ outputs \/ power/).textContent).toContain("4");
    expect(screen.getByText("Terminal capacity: 100")).toBeTruthy();
    expect(screen.queryByText(/Conversion power/)).toBeNull(); cleanup();
  });

});
