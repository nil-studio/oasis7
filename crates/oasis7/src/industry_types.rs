//! Platform-neutral industrial ledger and settlement wire types.

#[cfg(test)]
use oasis7_wasm_abi::MaterialStack;

pub use oasis7_agent_api::world_action::MaterialLedgerId;

pub use oasis7_agent_api::world_industry::StarterIndustrialSettlementSummaryV1;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn material_ledger_wire_strings_preserve_validation() {
        for value in ["world", "agent:owner", "site:s", "factory:smelter"] {
            let encoded = serde_json::json!(value);
            let ledger: MaterialLedgerId = serde_json::from_value(encoded.clone()).unwrap();
            assert_eq!(serde_json::to_value(ledger).unwrap(), encoded);
        }
        for invalid in ["unknown", "agent:", "site: ", "factory:"] {
            assert!(
                serde_json::from_value::<MaterialLedgerId>(serde_json::json!(invalid)).is_err()
            );
        }
    }

    #[test]
    fn starter_settled_outcome_wire_shape_is_platform_neutral() {
        let encoded = serde_json::json!({
            "settlement_job_id": 5, "settled_at": 27, "output_ledger": "site:s",
            "settlement": {
                "requester_agent_id": "owner", "accepted_batches": 12,
                "consume": [{"kind": "iron_ore", "amount": 48}],
                "power_required": 24,
                "produce": [{"kind": "iron_ingot", "amount": 36}]
            }
        });
        let outcome: crate::simulator::persist::PlayerStarterIndustrialSettledOutcome =
            serde_json::from_value(encoded.clone()).unwrap();
        assert_eq!(outcome.output_ledger, MaterialLedgerId::site("s"));
        assert_eq!(
            outcome.settlement.consume,
            vec![MaterialStack::new("iron_ore", 48)]
        );
        assert_eq!(serde_json::to_value(outcome).unwrap(), encoded);
    }
}
