# oasis7 Runtime：可兑现节点资产与电力兑换闭环（二期审计与签名加固，发布说明）

本发布说明按既有 [Task #2655 保留决定](https://github.com/eng-cc/oasis7/issues/2655#issuecomment-5088835783)继续保留 AHA provenance、接口和兼容边界；原 AHA 执行 Task 未唯一核实，不把后续整理 Task 当成原实现者。回归和工作区格式化观察仅属于原发布窗口，不表示今日运行入口、PASS 或待办。

审计轮次: 5
## 发布范围
- 设计/项目文档：
  - 当前权威：`doc/p2p/node/node-redeemable-power-asset.prd.md`
  - 当前追踪：GitHub Issue / GitHub Project
  - 本文件仅保留历史 AHA 发布 provenance，不是现行 operator runbook。
- 代码主线：
  - `crates/oasis7_runtime/src/reward_asset.rs`
  - `crates/oasis7_runtime/src/world/resources.rs`
  - `crates/oasis7_runtime/src/tests/reward_asset.rs`
  - `crates/oasis7_viewer_runtime/src/bin/oasis7_viewer_live.rs`

## 交付摘要
- AHA-2：结算签名语义升级为 `mintsig:v1:<sha256>`，并提供 `World::verify_reward_mint_record_signature` 校验接口。
- AHA-3：新增资产不变量审计报告 `RewardAssetInvariantReport`，覆盖节点/全局守恒与结算签名有效性巡检。
- AHA-4：`oasis7_viewer_live` reward runtime 报表接入审计摘要与明细输出，支持运行时告警。

## 核心接口变化
- Runtime 新增/增强：
  - `reward_mint_signature_v1(...)`
  - `World::verify_reward_mint_record_signature(...)`
  - `World::reward_asset_invariant_report()`
- 新增模型：
  - `RewardAssetInvariantReport`
  - `RewardAssetInvariantViolation`
- 历史 Reward runtime 报表新增字段：
  - `reward_asset_invariant_status`
  - `reward_asset_invariant_report`

## 兼容性说明
- 资产与兑换业务路径保持兼容：未依赖签名字符串具体格式的调用方不受影响。
- 快照结构仅新增审计报告类型（运行时计算），未新增必须持久化字段。
- 报表字段是增强输出，不影响原有 `settlement_report`/`minted_records` 消费链路。

## 回归验证
- 资产闭环与签名/审计：
  - `env -u RUSTC_WRAPPER cargo test -p oasis7 reward_asset_ -- --nocapture`
- reward runtime 参数与报表单测：
  - `env -u RUSTC_WRAPPER cargo test -p oasis7_viewer_runtime --bin oasis7_viewer_live -- --nocapture`
- 编译检查：
  - `env -u RUSTC_WRAPPER cargo check -p oasis7`

## 已知事项
- `mintsig:v1` 仍为可重算摘要语义，不等同于真实私钥签名；后续可平滑升级到硬签名方案（如 `mintsig:v2`）。
- 原发布窗口曾记录无关格式化漂移（不是今日工作区状态）：`crates/oasis7_runtime/src/node_points.rs`，本轮未回退。
- 历史 `oasis7_viewer_live` 命令与 flags 不再构成当前运行入口；现行证据以 chain-runtime CLI、runtime root 与 report artifacts 为准。
