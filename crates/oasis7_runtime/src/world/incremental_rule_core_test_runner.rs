//! Closed entry to the four original actual Linux bounded-worker cases.
/// No data or qualification is returned. These variants only select the
/// original assertion-bearing fixture and its fixed independent old oracle.
#[derive(Clone, Copy, Debug)]
pub enum IwsRuleCoreCaseV1 {
    ModifyWithPricing,
    RejectedMoveGeneralCost,
    InsufficientCost,
    Deny,
}
/// Execute the complete original GEN/FIFO/Root/VM/original/state/counter oracle.
/// Missing real worker executable or any failed assertion fails the test; there
/// is no in-process substitute, supplied Rule decision, or success fallback.
pub fn run_iws_rule_core_case_v1(case: IwsRuleCoreCaseV1) {
    let (payload, publish_override, mode) = match case {
        IwsRuleCoreCaseV1::ModifyWithPricing => (1, true, 1),
        IwsRuleCoreCaseV1::RejectedMoveGeneralCost => (1, true, 2),
        IwsRuleCoreCaseV1::InsufficientCost => (1, true, 3),
        IwsRuleCoreCaseV1::Deny => (7, false, 4),
    };
    super::rule_cases::run_rule_fixture_composed(payload, publish_override, mode);
}
