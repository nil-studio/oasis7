const values = value => Array.isArray(value) ? value : value && typeof value === "object" ? Object.values(value) : [];
const text = value => typeof value === "string" && value.length ? value : null;
const number = value => Number.isSafeInteger(value) && value >= 0 ? value : null;
const ref = value => text(value?.id);
const qualified = value => ["id", "world_binding", "domain", "schema"].every(key => text(value?.[key])) && Number.isSafeInteger(value?.revision) && value.revision > 0 && /^[0-9a-f]{64}$/.test(value?.digest || "");
const sameRef = (a, b) => qualified(a) && qualified(b) && ["id", "world_binding", "domain", "schema", "revision", "digest"].every(key => a[key] === b[key]);
export function normalizeRecipeSubstitution(value) {
  if (!value || typeof value !== "object") return null;
  return {
    truncated: value.truncated === true,
    roots: values(value.roots).map(root => ({ id: ref(root.root_ref), recipe: text(root.pinned_recipe_id), work: number(root.work) })).filter(root => root.id),
    previews: values(value.previews).map(item => item.preview || item).map(preview => ({
      root: ref(preview.request?.root_ref), slot: text(preview.request?.slot_id),
      status: ["available", "no_legal_substitute", "unknown"].includes(preview.status) ? preview.status : "unknown",
      blocker: text(preview.blocker), nextAction: text(preview.next_action), nextRecheck: number(preview.next_recheck),
      candidates: values(preview.candidates).map(candidate => ({
        kind: ["same_material", "profile_substitute", "new_recipe"].includes(candidate.kind) ? candidate.kind : "unknown",
        source: text(candidate.source_material_kind), target: candidate.kind === "new_recipe" ? text(candidate.authority?.target_recipe) : text(candidate.authority?.output_material),
        sourceRequired: number(candidate.plan?.source_required ?? candidate.native_source_quantity), sourceUnit: text(candidate.plan?.source_unit ?? candidate.authority?.output_unit),
        targetQuantity: number(candidate.plan?.target_executable ?? candidate.native_source_quantity), targetUnit: text(candidate.plan?.target_unit ?? candidate.authority?.output_unit),
        loss: number(candidate.plan?.source_loss ?? (number(candidate.native_source_quantity) !== null ? 0 : null)), residualNumerator: number(candidate.plan?.residual_numerator ?? (number(candidate.native_source_quantity) !== null ? 0 : null)), residualDenominator: number(candidate.plan?.residual_denominator ?? (number(candidate.native_source_quantity) !== null ? 1 : null)),
        power: number(candidate.authority?.power_cost), terminal: number(candidate.authority?.terminal_capacity),
        logistics: ref(candidate.authority?.candidate_impacts?.logistics), buffer: ref(candidate.authority?.candidate_impacts?.buffer),
        valueClass: ref(candidate.authority?.candidate_impacts?.value_class),
        risks: values(candidate.authority?.risks).map(text).filter(Boolean),
        valuation: {unit:text(candidate.authority?.valuation?.unit),held:number(candidate.authority?.valuation?.held),consumed:number(candidate.authority?.valuation?.consumed),expected:number(candidate.authority?.valuation?.expected_output)},
        causalEquivalent: candidate.causal_equivalent === true,
        conversionRequired: candidate.conversion_required === true,
        ratioNumerator: number(candidate.rule?.ratio_numerator ?? (candidate.kind === "same_material" && number(candidate.native_source_quantity) !== null ? 1 : null)), ratioDenominator: number(candidate.rule?.ratio_denominator ?? (candidate.kind === "same_material" && number(candidate.native_source_quantity) !== null ? 1 : null)), rounding: text(candidate.rule?.rounding ?? (candidate.kind === "same_material" && number(candidate.native_source_quantity) !== null ? "exact" : null)),
        recipeInputs: values(candidate.recipe_plan?.consume).map(item => ({kind:text(item.kind),amount:number(item.amount)})).filter(item => item.kind && item.amount !== null),
        recipeOutputs: values(candidate.recipe_plan?.produce).map(item => ({kind:text(item.kind),amount:number(item.amount)})).filter(item => item.kind && item.amount !== null),
        recipePower: number(candidate.recipe_plan?.power_required),
      })),
    })),
    decisions: values(value.decisions).map(decision => {
      const settlement = values(value.settlements).find(item => sameRef(item.decision_ref, decision.decision_ref) && qualified(item.receipt_ref));
      return { id: ref(decision.decision_ref), root: ref(decision.intent?.preview?.request?.root_ref), scope: decision.intent?.scope === "decision_only" ? "decision_only" : decision.intent?.scope === "decision_and_exact_conversion" ? "conversion_authorized" : "unknown", settlement: ref(settlement?.receipt_ref), consumed: number(settlement?.source_consumed), loss: number(settlement?.source_loss) };
    }).filter(decision => decision.id),
  };
}
