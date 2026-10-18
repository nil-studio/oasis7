// Wire values from oasis7_simulator/src/kernel/actions_schedule_quote.rs, shared by UI and visual tests.
const low = Object.freeze({
  owner_agent_id: "agent-0", factory_id: "factory-0", recipe_id: "assemble_hardware", batches: 2,
  base_duration_ticks: 6, electricity_cost: 12, electricity_after: 88, hardware_cost: 4,
  data_output: 8, finished_product_id: "hardware", finished_product_units: 2,
  local_shortage_delay_ticks: 0, shortage_reason: "none", recommended_pre_step: "schedule_now",
  runway_before_ticks: 40, runway_after_ticks: 40, downtime_threshold_ppm: 250000,
  continue_production_risk: "low", maintenance_pressure_delta: "unchanged",
  recommended_maintenance_action: "continue_production",
});
export const scheduleRecipeQuotes = Object.freeze({
  low,
  elevated: Object.freeze({ ...low, continue_production_risk: "elevated",
    recommended_pre_step: "restore_power_before_scheduling", recommended_maintenance_action: "restore_power",
    runway_before_ticks: 3, runway_after_ticks: 3, local_shortage_delay_ticks: 5,
    shortage_reason: "local_hardware_shortage" }),
});
