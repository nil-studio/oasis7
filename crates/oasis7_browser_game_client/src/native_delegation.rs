//! Typed owner control construction; signing and admission remain outside WASM.
use crate::{encode, parse};
use oasis7_agent_api::game::{
    API_REVISION, GameNativeDelegationControlV1, GameNativeDelegationOperationV1, GameTaskFenceV1,
};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    fence_json: String,
    client_operation_id: String,
    operation: GameNativeDelegationOperationV1,
}

pub(super) fn build(input: &str) -> Result<String, &'static str> {
    let input: Input = parse(input)?;
    let control = GameNativeDelegationControlV1 {
        api_rev: API_REVISION,
        client_operation_id: input.client_operation_id,
        fence: parse::<GameTaskFenceV1>(&input.fence_json)?,
        operation: input.operation,
    };
    control.validate().map_err(|_| "contract_mismatch")?;
    encode(&control)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{build, ws};
    use oasis7_agent_api::game::DecimalU64;
    use serde_json::json;

    #[test]
    fn owner_control_preserves_large_decimal_limits_and_full_fence() {
        let snapshot: ws::task_context::GoalExecutionSnapshotV1 = serde_json::from_str(
            include_str!("../../oasis7_game_client/testdata/task-context/native-snapshot.json"),
        )
        .unwrap();
        let fence = GameTaskFenceV1::from_control(&snapshot.control);
        let input = json!({
            "fence_json": encode(&fence).unwrap(),
            "client_operation_id": "ordinary-owner-grant",
            "operation": {
                "operation": "install", "grant_id": "ordinary-build",
                "object_id": "ordinary-starter-smelter-1", "action_kinds": ["BuildFactory"],
                "period_id": "confirmed-first-factory", "valid_from_tick": "0",
                "valid_until_tick": "9007199254740993", "limit_units": "9007199254740995"
            }
        });
        let encoded = build::build("native_delegation", &input.to_string()).unwrap();
        let control: GameNativeDelegationControlV1 = parse(&encoded).unwrap();
        assert_eq!(control.fence, fence);
        assert!(matches!(
            control.operation,
            GameNativeDelegationOperationV1::Install {
                valid_until_tick: DecimalU64(9_007_199_254_740_993),
                limit_units: DecimalU64(9_007_199_254_740_995),
                ..
            }
        ));
        let mut numeric = input.clone();
        numeric["operation"]["limit_units"] = json!(9_007_199_254_740_995_u64);
        assert_eq!(
            build::build("native_delegation", &numeric.to_string()),
            Err("invalid_json")
        );
        let mut broad = input;
        broad["operation"]["action_kinds"] = json!(["MintResource"]);
        assert_eq!(
            build::build("native_delegation", &broad.to_string()),
            Err("contract_mismatch")
        );
    }
}
