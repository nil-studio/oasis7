use super::*;
use oasis7_wasm_abi::{
    ModuleCallFailure, ModuleCallInput, ModuleCallRequest, ModuleEmit, ModuleOutput, ModuleSandbox,
};

pub(super) fn micro_depot_output_for(proposal: MicroDepotProposal) -> ModuleOutput {
    ModuleOutput {
        new_state: None,
        effects: Vec::new(),
        emits: vec![ModuleEmit {
            kind: MICRO_DEPOT_PROPOSAL_EMIT_KIND.to_string(),
            payload: serde_json::to_value(proposal).expect("serialize proposal"),
        }],
        tick_lifecycle: None,
        output_bytes: 256,
    }
}

#[derive(Clone, Default)]
pub(super) struct DynamicMicroDepotSandbox {
    pub(super) requests: Vec<ModuleCallRequest>,
    pub(super) consumed_resource_classes: Option<Vec<MicroDepotConsumedResourceClass>>,
    pub(super) resource_debits: Option<Vec<MicroDepotProposalResourceDebit>>,
}

impl ModuleSandbox for DynamicMicroDepotSandbox {
    fn call(&mut self, request: &ModuleCallRequest) -> Result<ModuleOutput, ModuleCallFailure> {
        self.requests.push(request.clone());
        let call_input: ModuleCallInput =
            serde_cbor::from_slice(&request.input).expect("decode module call input");
        let payload = call_input.action.expect("micro depot payload");
        let input: MicroDepotEvalInput =
            serde_cbor::from_slice(&payload).expect("decode micro depot eval input");
        let mut proposal = MicroDepotProposal {
            action_id: input.action.action_id,
            facility_id: input.depot.facility_id.clone(),
            decision: MicroDepotDecision::Applicable,
            cost_delta_class: MicroDepotDeltaClass::MinorDecrease,
            risk_delta_class: MicroDepotDeltaClass::None,
            wait_delta_class: MicroDepotDeltaClass::MinorDecrease,
            blocker_change: Some("repair_parts_gap_reduced".to_string()),
            consumed_resource_classes: self.consumed_resource_classes.clone().unwrap_or_else(
                || {
                    if input.schema_version == "micro_depot.eval.v1" {
                        vec![MicroDepotConsumedResourceClass {
                            resource_kind: "data".to_string(),
                            amount_class: MicroDepotPressureClass::Low,
                        }]
                    } else {
                        Vec::new()
                    }
                },
            ),
            resource_debits: self.resource_debits.clone().unwrap_or_else(|| {
                if input.schema_version == "micro_depot.eval.v2" {
                    vec![MicroDepotProposalResourceDebit {
                        resource_kind: ResourceKind::Data,
                        units: 1,
                    }]
                } else {
                    Vec::new()
                }
            }),
            evaluated_inventory_revision: input.depot.inventory_revision,
            evaluated_epoch: input.depot.current_epoch,
            explanation_code: "depot_reserved_local_parts".to_string(),
            proposal_hash: String::new(),
        };
        proposal.proposal_hash = compute_micro_depot_proposal_hash(&input, &proposal)
            .expect("compute runtime-compatible proposal hash");
        Ok(micro_depot_output_for(proposal))
    }
}
