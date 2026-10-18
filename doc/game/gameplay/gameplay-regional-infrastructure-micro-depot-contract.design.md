# Gameplay 区域基础设施 micro_depot 合同设计

- 对应需求文档: `doc/game/gameplay/gameplay-regional-infrastructure-micro-depot-contract.prd.md`
- 可变执行状态: 对应 GitHub Project task 与 issue evidence comments
- 对应任务证据: GitHub issue / task `task_3c55fc4c17ea44aa850adf7a6a4463f4`；退役前 `.pm/tasks/*` execution log 仅通过 `.pm/github-project-sync/task-archive.jsonl` 作 migration/audit 追溯
- 关联玩法真值:
  - `doc/game/gameplay/gameplay-agent-claim-economy-contract.prd.md`
  - `doc/product/world-rules-core-gameplay/prd.md`（产品承诺）与 `doc/game/gameplay/gameplay-top-level-design.prd.md`（玩法合同）
  - [`受治理的区域能力与扩展`](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-001) 是本设计的产品语义入口。
  - `doc/game/gameplay/gameplay-indirect-control-agency-contract.prd.md`
  - `doc/product/world-rules-core-gameplay/mature-world-progression.prd.md`
  - `doc/world-simulator/viewer/viewer-pixel-world-player-leverage-production-readability-2026-05-28.brainstorm.md`

<a id="1-设计定位"></a>
## 1. 目标与设计定位

`micro_depot` 是第一版可编程区域设施，用来验证一件事：

> 玩家通过一个小型、可审计、带 upkeep 的区域设施，改变一次 repair / logistics 行动的条件。

它不是 Minecraft 式逐块建造，不是自由脚本，不是通用 UGC 市场，也不是 UI buff。它是一个 WASM-backed facility：WASM 负责规则化 proposal，runtime 负责权限、扣费、状态变更和 receipt。

核心原则：

```text
WASM proposes, runtime validates / applies / signs.
```

## 2. 玩家循环

目标体验：

1. 玩家看到区域阻塞，例如 `supply_missing` 导致 repair quote 高成本或高风险。
2. 系统建议部署 `micro_depot`。
3. 玩家查看 canonical quote：部署成本、upkeep、覆盖范围、预期收益、仍然存在的 blocker。
4. 玩家查看最小 ROI 条：预计覆盖的后续 blocker 数、break-even uses、upkeep horizon、低使用率警告和推荐理由。
5. 玩家确认安装，消耗 starter funding / claim budget，并占用局部设施压力。
6. Runtime 调用 `micro_depot.wasm` 评估后续 repair / logistics quote。
7. Runtime 校验 proposal，生成 before / after preview。
8. 玩家执行行动，receipt 说明 depot 对本次行动的贡献。

玩家应该能回答：

- 我为什么需要这个 depot？
- 它花了什么？
- 大概要用几次、覆盖多少后续 blocker，才抵过 install + upkeep？
- 它改变了哪一次 repair / logistics 行动？
- 还有什么 blocker 没解决？


### 2.1 需求承接与分配表

| 上游 requirement / product AC | 具体 obligation 与适用条件 | 本设计条款 | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [REQ-WR-GR-001](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-001) Trace refs: O7-REQ-d34e2dec-9ad4-44a8-9ba7-7931956e39b8 | micro_depot 的报价无预留、明确提交、有限库存服务、维护/耗尽与回收恢复切片。 | [有限库存生命周期](#micro-depot-measured-lifecycle) | gameplay / Simulator / runtime / Viewer / QA | 不承接全部区域设施；区域压力引用仍是 opaque，非真实 provenance 认证；不承诺补货、epoch reset 或独立吞吐耗尽玩法。 |
| [AC-WR-GR-001](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#ac-wr-gr-001) Trace refs: O7-REQ-d34e2dec-9ad4-44a8-9ba7-7931956e39b8 | 受支持 micro_depot 同步服务按当前 owner、upkeep、库存、范围及 module proposal 校验，拒绝不得部分扣减。 | [有限库存生命周期](#micro-depot-measured-lifecycle) | Simulator/runtime effect 与玩家读面 | 不证明独立 preview-confirm 并发协议、其他设施或完整区域资格。 |

## 3. 解锁阶段与准入流程

`micro_depot` 不属于首 10 分钟新手循环，也不应作为随时可点的自由建造物出现。它的正确阶段是中后期的 `regional specialist / limited-scope regional influence`：玩家已经理解 agent intent、repair / logistics blocker、claim / upkeep 成本和区域压力后，才解锁第一个可编程区域设施。

所有 micro-depot 验证样本必须明确写作 `post-first-capability 10-minute slice` 或 `regional-specialist 10-minute scenario`。这里的 10 分钟指玩家已进入区域专业化后的局部验证窗口，不是首局 0-10 分钟 onboarding gate。

阶段定位：

- 前期：只暴露 agent intent、repair / logistics blocker、一次性协助或专家行动。目标是建立“玩家下指令，世界有可读反馈”的信任，不让设施建造抢走核心教学。
- 中期：当玩家完成至少一次 repair / logistics 闭环，并拥有稳定 agent claim 后，`micro_depot` 作为第一个持续性区域杠杆解锁。
- 后期：再扩展到 `route_contract`、`repair_station`、`audit_beacon`，以及多设施组合、模块升级和区域治理。

创建准入必须来自游戏流程，而不是裸 action：

1. `first_capability_completed=yes`：玩家已完成 first capability，不再处于首局 trust/capability teaching loop。
2. `stable_claim_available=yes`：玩家拥有可操作 agent claim，且 claim / agent 与目标区域存在合理关系。
3. 目标态要求 `regional_blocker_receipt_id` 或等价的 canonical 区域压力证据已存在，证明玩家见过 repair / logistics blocker，而不是被强行引入设施系统。当前 first slice 尚无 canonical repair/logistics receipt producer 或 regional-pressure evidence ledger，因此只携带 opaque causal reference，不能把非空字符串宣称为已认证 receipt。
4. `regional_specialist_lane_state=active_or_eligible`：玩家已进入或可进入 `regional specialist / limited-scope regional influence`。
5. 玩家已能读懂 claim/upkeep、repair/logistics blocker、before/after quote 和 receipt surface。
6. 当前区域存在重复或高价值 blocker，例如 `supply_missing`、`route_blocked`、repair 等待过长或 logistics quote 风险过高。
7. 目标 location / facility 已生成，且处于 depot 服务半径与区域设施上限规则内。
8. 玩家可支付 install 成本和后续 upkeep；restricted starter funding 不能被扩成无限设施补贴。
9. 系统通过 regional pressure card、repair 失败反馈或 logistics bottleneck 暴露部署建议。
10. Runtime 在 `InstallMicroDepot` 前再次校验 owner、claim scope、资源、upkeep 初始化、重复设施、module hash / schema allowlist。

当前内核 first slice 的硬约束：

- `InstallMicroDepot` 当前必须携带非空 `regional_blocker_receipt_id` 作为前置 repair / logistics blocker 的 opaque causal reference；空值由 runtime 直接拒绝。真实 receipt 的存在性、agent/location/action provenance 校验必须等 canonical producer 或等价 canonical regional-pressure evidence source 落地后再启用 fail-closed gate；当前 slice 不得把非空引用表述为已完成认证。
- 同一 `owner_claim_id + location_id` 只能存在一个 `micro_depot`，避免把它变成自由堆叠建造物。
- 安装会扣固定 `Data` 成本，upkeep 会扣固定 `Data` 成本；数值先作为 kernel 常量落地，后续再迁入平衡表。
- 服务动作必须由 depot owner agent 提交；非 owner 不能触发服务，也不能消耗 owner 资源。
- 服务 quote 必须与 facility pinned 的 `module_id / wasm_hash / entrypoint` 匹配，且 WASM 提议消耗的资源必须属于 facility 的 `supported_resource_kinds`。
- `MicroDepotServiceApplied` receipt 记录 `schema_version / module_id / module_version / wasm_hash / entrypoint / proposal_hash`，保证玩家可见 receipt 和治理审计能追溯到模块证据。

玩家不应该从全局菜单直接“建一个 depot”。推荐入口是：

```text
regional pressure card
  -> repair/logistics blocker explanation
  -> suggested intervention: deploy micro_depot
  -> install quote + upkeep + service radius preview + coarse ROI strip
  -> player confirms
  -> MicroDepotInstalled receipt
  -> subsequent repair/logistics service preview and receipt
```

设计约束：

- `micro_depot` 是区域专业化阶段的第一个可编程基础设施，不是新手建造物。
- 没有区域 blocker、没有稳定 claim、没有可读 receipt 时，不应出现部署入口。
- 玩家只看到“部署建议 / quote / receipt / upkeep 状态”，不直接接触 WASM 内部规则。
- 如果预计使用次数低于 break-even uses，仍可允许玩家安装，但必须给出低使用率警告和推荐/不推荐理由，避免设施变成默认必点税或无脑 buff。
- 如果用于 onboarding，只能作为后续目标预告，不能作为首轮必做任务。

## 4. Facility Taxonomy

第一批区域设施只保留四类，但 MVP 只实现 `micro_depot`。

| Schema | 用途 | 首要玩家价值 | 当前阶段 |
| --- | --- | --- | --- |
| `micro_depot` | 小型区域补给点 | 让附近 repair / logistics quote 更可执行 | MVP |
| `route_contract` | 区域路线承诺 / 运输合约 | 把不稳定路线变成可重复路径 | 后续 |
| `repair_station` | 区域修复服务点 | 降低重复维修成本 | 后续 |
| `audit_beacon` | 风险 / 低效节点标记器 | 揭示隐藏阻塞和异常消耗 | 后续 |

非目标：

- 不做 block placement / digging / terraforming。
- 不开放任意玩家上传 WASM。
- 不让设施绕过 claim / upkeep 成本。
- 不让设施直接提供 global governance 权力。
- 不把 starter funding 扩成无限补贴。

<a id="5-一步到位-wasm-backed-架构"></a>
## 5. 一步到位 WASM-backed 架构边界

第一版不做 runtime-only stub。`micro_depot` 从第一版开始就是 WASM-backed，但 WASM 只参与 proposal。

### 5.1 First-Slice Flow

```text
repair/logistics quote request
  -> runtime builds MicroDepotEvalInput
  -> runtime calls micro_depot.wasm::evaluate_quote(input)
  -> wasm returns MicroDepotProposal
  -> runtime validates proposal
  -> runtime emits canonical preview
  -> player confirms action
  -> runtime validates again or checks proposal_hash
  -> runtime applies accounting/state mutation
  -> runtime emits events and receipt
  -> viewer / pure API render canonical DTO
```

Runtime validation includes:

- owner / claim scope
- distance and service radius
- upkeep state
- supported resource kind
- resource existence
- effect cap
- module hash / schema version
- replay-safe proposal hash

## 6. Minimal ABI (`micro_depot.eval.v2`)

Measured-service 的 normative ABI 是 `schema_version = "micro_depot.eval.v2"`。Runtime 继续接受 `micro_depot.eval.v1` 作为 legacy compatibility input/output，并把 v1 的 class-based consumption 通过既有 adapter 转成 runtime debit；新 module、fixture 和 receipt producer 必须使用 v2 的 exact `resource_debits`，不得继续扩展 v1。v1 compatibility 必须使用变更前历史版本的精确 wire 字段集合、字段名、默认值、canonical serialization 与 input/proposal hash projection；v2 新增字段不得进入 v1 hash/action projection。Schema-bound migration 规则是：legacy v1 persisted state 缺失 measured fields 时按 `inventory_revision = 0`、空库存、吞吐 `0` 解码，不追溯赠送 commissioning stock；只有 v2 新安装设施获得下述版本化 commissioning bundle。

### 6.1 Input

```ts
type MicroDepotEvalInput = {
  schema_version: "micro_depot.eval.v2"
  module_id: string
  module_version: string
  action: {
    action_id: number
    kind: "repair" | "logistics"
    target_id: string
    base_cost_class: "none" | "low" | "medium" | "high" | "critical"
    base_risk_class: "none" | "low" | "medium" | "high" | "critical"
    // serde: absent on encode when None; absent on decode defaults to None.
    blocker_type?: string
  }
  player: {
    account_id: string
    claim_id: string
  }
  depot: {
    facility_id: string
    status: "active" | "degraded" | "upkeep_grace" | "suspended"
    owner_claim_id: string
    capacity_class: "none" | "low" | "medium" | "high" | "critical"
    // serde(default): an absent field decodes to 0; canonical v2 producers emit it.
    inventory_revision: number
    // serde(default): an absent field decodes to 0; canonical v2 producers emit it.
    current_epoch: number
    // serde(default): an absent field decodes to {}; canonical v2 producers emit it.
    available_units_by_kind: Record<string, number>
    // serde(default): an absent field decodes to 0; canonical v2 producers emit it.
    throughput_remaining_units: number
    // serde(default): an absent field decodes to 0; canonical v2 producers emit it.
    throughput_limit_units_per_epoch: number
    // serde(default): an absent field decodes to []; canonical v2 producers emit it.
    supported_resource_kinds: string[]
    service_radius_cm: number
    upkeep_paid: boolean
  }
  context: {
    distance_to_target_cm: number
    resource_gap_class: "none" | "low" | "medium" | "high" | "critical"
    route_pressure_class: "none" | "low" | "medium" | "high" | "critical"
    region_pressure_class: "none" | "low" | "medium" | "high" | "critical"
  }
}
```

### 6.2 Output

```ts
type MicroDepotProposal = {
  action_id: number
  facility_id: string
  decision: "applicable" | "not_applicable" | "blocked"
  cost_delta_class: "major_decrease" | "minor_decrease" | "none" | "minor_increase"
  risk_delta_class: "major_decrease" | "minor_decrease" | "none" | "minor_increase"
  wait_delta_class: "major_decrease" | "minor_decrease" | "none" | "minor_increase"
  // serde: absent on encode when None; absent on decode defaults to None.
  blocker_change?: string
  // Legacy mirror. serde(default) accepts absence as []; canonical serialization emits [].
  // v2 validation requires this array to be empty; only v1 may carry class debits.
  consumed_resource_classes: Array<{
    resource_kind: string
    amount_class: "none" | "low" | "medium" | "high" | "critical"
  }>
  // serde(default) accepts absence as []; canonical serialization emits the field.
  resource_debits: Array<{
    resource_kind: "electricity" | "data"
    units: number
  }>
  // Both evaluated fields use serde(default): absence decodes as 0; producers emit both.
  evaluated_inventory_revision: number
  evaluated_epoch: number
  explanation_code: string
  proposal_hash: string
}
```

上述字段次序与类型是当前 Rust `Serialize` / `Deserialize` wire contract，而不是效果对象草案：output 不存在 `proposal_id` 或嵌套 `effect`。`action_id` 与 `facility_id` 必须逐字匹配 input。所有 pressure、delta、status、decision、action kind 和 resource kind enum 均编码为上列 snake_case 字符串。所有 `units` 都是 runtime 定义的正整数最小计量单位；MVP 不允许浮点、零/负数、隐式单位换算或只报资源类别不报数量。`resource_debits` 按 `resource_kind` 唯一且严格 canonical 顺序参与 `proposal_hash`；`applicable` proposal 必须至少包含一项 debit，其他 decision 必须为空。

兼容字段规则不可混用：v1 proposal 必须只使用 `consumed_resource_classes` 且 `resource_debits = []`；v2 proposal 必须 `consumed_resource_classes = []` 且只使用 exact `resource_debits`。两字段均有 `serde(default)`，所以 decoder 可把缺失字段投影为空数组，但 canonical producer 会显式序列化字段。v1 input/action/hash projection 继续严格排除所有 v2 measured fields，保持历史 golden hash 与 action payload 不变。

### 6.3 Golden contract

Executable golden 位于 `crates/oasis7_simulator/src/tests/micro_depot_schema_compatibility.rs` 的 `micro_depot_eval_v2_wire_shape_and_hash_are_fixed_goldens`。Fixture 固定 `action_id = 73`、`facility_id = "depot-v2-golden"`、`inventory_revision = 7`、`current_epoch = 11`、`available_units_by_kind = { "data": 13 }`、`throughput_remaining_units = 17`、`throughput_limit_units_per_epoch = 23`，并固定 proposal 的 `resource_debits = [{ "resource_kind": "data", "units": 3 }]`、空 `consumed_resource_classes` 与相同 evaluated revision/epoch；其 golden `proposal_hash` 是 `sha256:04b0b917ff7dadfb1d84e984cd17978dabfe17de020fd0620040d47977640204`。测试同时锁定 input/proposal canonical CBOR、module-call action bytes 与该 hash；fixture 或 ABI 发生任何变化都必须显式更新该 golden，不允许由文档另造投影。

WASM 不允许输出真实库存变更、扣费结果、ownership 变更或 canonical world-state patch。它只基于 runtime 提供的同一库存快照提出 debit；runtime 是余额与吞吐扣减的唯一 authority。

## 7. Canonical Runtime Model

### 7.1 State

```text
RegionalInfrastructure {
  facility_id
  kind = micro_depot
  region_id
  location_id
  fragment_anchor_cm
  owner_account_id
  owner_claim_id
  installer_agent_id
  status
  claim_scope
  capacity_class
  service_radius_cm
  supported_resource_kinds
  inventory_revision
  available_units_by_kind
  throughput_limit_units_per_epoch
  throughput_epoch
  throughput_remaining_units
  upkeep_per_epoch
  upkeep_paid_through_epoch
  funding_provenance
  module_id
  module_version
  module_hash
  schema_version
  last_proposal_hash
  last_blocker_type
  last_receipt_id
  created_tick
  updated_tick
}
```

<a id="micro-depot-measured-lifecycle"></a>
### 7.1.1 Measured Supply Contract

- `available_units_by_kind[resource_kind]` 是设施内可服务库存；缺失 key 等价于 `0`。每个值和总量都必须是非负整数，且不得超过 runtime 配置的 per-kind / facility capacity。
- v2 MVP 的 install request 必须且只能声明 canonical `supported_resource_kinds == ["data"]`。空列表、任何非 `data` 值、mixed/duplicate 列表，以及 `Data`、`DATA` 等大小写变体都必须原子拒绝；MVP 不做输入 canonicalization，也不承诺多资源 commissioning。
- v2 当前 install debit 固定为 `10 Data`：其中 `2 Data` 是不可退还的 deployment sink，`8 Data` 从玩家余额转入设施成为 commissioned inventory；同时初始化 throughput limit/remaining `16` units、epoch `0`。`10/8/16` 是当前实现与 replay 的版本化事实，不是最终平衡承诺。
- 当前 v2 MVP 是 **single-commission consumable depot**：这次安装只获得上述 `8 Data` commissioned inventory，库存耗尽即结束本次设施的可服务生命。它不是持续生产、周期补货或可无限续费的设施。
- `MicroDepotInstalled` receipt 通过 `install_cost_resources`、`commissioning_sink_resources`、`commissioned_inventory_by_kind`、`initial_throughput_limit_units_per_epoch` 与 `initial_throughput_remaining_units` 明确证明同一次 accepted install 的 `10 Data` debit、`2 Data` sink、`8 Data` commissioned transfer 和初始 `16/16` throughput；replay 必须校验这些事件值严格对账并使用事件值重建状态，不得从当前常量重新推导历史 commissioning。不得把“支付 2、系统赠送 8”或任何低于 10 Data 获得 bundle 的路径写入实现或玩家口径。
- reclaim refund 固定为零，并销毁设施内所有剩余 inventory 与 throughput；销毁余额不得转回玩家、其他设施或后续安装。reinstall 是全新的 `10 Data` install，重新产生 fresh `8/16` state，不 carry over，也不是 refill。
- upkeep 只维持尚有库存的设施处于可服务状态，不补充 inventory 或 throughput。库存为零时，upkeep 请求必须拒绝且不收费；玩家唯一的当前恢复路径是 destructive reclaim，再支付完整 `10 Data` 安装成本创建全新设施。
- `throughput_limit_units_per_epoch`、`throughput_epoch` 与 `throughput_remaining_units` 是 measured-service canonical state；当前固定 `16` 仅作为内部 defense-in-depth accounting ceiling 与 replay invariant。由于同一 commissioning 只有 `8 Data` 库存，当前玩法不能在库存之前自然触达 throughput exhaustion；不得把它写成独立玩家 blocker、恢复循环、epoch playability 或周期续航承诺。
- 当前 acceptance 不包含补货 action、canonical epoch clock 或吞吐 reset。库存来源授权、补货扣转、epoch clock、rollover/reset 触发与事件必须由 `gameplay_designer + runtime_engineer` 后续联合设计后才能实现；commissioning initialization 之外，upkeep、任意 tick/epoch 变化都不得增加库存或吞吐。
- 每次成功 service debit 都递增 `inventory_revision`。同步 service action 内的 evaluator input 与 proposal 必须具有一致的 `evaluated_inventory_revision + evaluated_epoch`；这是 evaluator input consistency validation，不是跨 action 的 preview reservation 或并发确认合同。
- service apply 必须原子重验 owner/claim、upkeep、range、module/schema/proposal hash、各 kind 库存和总吞吐；全部成立才同时扣减 `available_units_by_kind` 与 `throughput_remaining_units` 并应用 service effect。任一失败则两者和 service world state 都不改变。
- evaluator preview 永不 reserve 或 debit。库存不足为 `DEPOT_INVENTORY_INSUFFICIENT`，吞吐不足为 `DEPOT_THROUGHPUT_EXHAUSTED`；当前同步 action 不承诺独立 preview-confirm concurrency 或 `DEPOT_SUPPLY_SNAPSHOT_STALE` player-facing blocker。

### 7.2 Actions

- `QuoteMicroDepotInstall`
- `InstallMicroDepot`
- `EvaluateMicroDepotQuote`
- `ServiceRepairFromDepot`
- `ServiceLogisticsFromDepot`
- `PayDepotUpkeep`
- `SuspendDepot`
- `ReclaimDepot`
- `InspectDepot`

### 7.3 Events

- `MicroDepotInstallProposed`
- `MicroDepotInstallAccepted`
- `MicroDepotInstalled`
- `MicroDepotQuoteEvaluated`
- `MicroDepotServiceApplied`
- `MicroDepotBlocked`
- `MicroDepotUpkeepSettled`
- `MicroDepotEnteredGrace`
- `MicroDepotSuspended`
- `MicroDepotReclaimed`

所有 player-facing accepted action 必须能追溯：

- `accepted_intent_id`
- `execution_status`
- `blocker_type`
- `world_change_summary`
- `receipt_id`
- `module_id / module_version / module_hash / proposal_hash`
- `evaluated_epoch / inventory_revision_before / inventory_revision_after`
- `resource_debits[] / inventory_units_before_by_kind / inventory_units_after_by_kind`
- `throughput_units_before / throughput_units_after`

## 8. Viewer / Pure API DTO

Viewer 和 pure API 不读取 WASM，不猜测收益，只渲染 runtime DTO。

必需 DTO：

- `infrastructure_nodes[]`
  - id, kind, status, anchor, owner claim, upkeep state, visible effect
- `facility_details`
  - upkeep, service radius, supported actions, module identity, provenance, inventory by kind, current epoch throughput remaining/limit, next useful action
- `action_previews[]`
  - before quote, after quote, depot contribution, remaining blockers
- `player_action_receipts[]`
  - before / after cost, risk, wait, route, repair/logistics outcome, resource debit quantities, inventory and throughput before/after
- `depot_contributions[]`
  - depot id, proposal hash, effect class, explanation code
- `bottlenecks[]`
- missing resource, insufficient inventory/depleted, unpaid upkeep while stocked, out of range, permission denied, degraded
- `throughput exhausted` 仅保留为 malformed/corrupt canonical state 的 fail-closed defense；当前 `8` stock / `16` ceiling 不能自然进入该玩家流程，也没有独立恢复动词
- `module_evidence`
  - module id, version, hash, schema version, proposal hash

## 9. Governance Minimum

MVP 只允许 repo-authored module，不开放玩家上传任意 WASM。

治理最小面：

- allowlist `module_id + version + hash`
- runtime 拒绝 unknown module hash
- schema version 必须匹配 runtime adapter
- module upgrade 需要显式 migration rule，否则设施 pinned 到旧 hash
- receipt 必须记录 module identity
- old receipt replay 必须使用原始 module hash

## 10. 安全与确定性约束

WASM 模块必须满足：

- no wall clock
- no network
- no filesystem
- no host escape
- no floating-point drift
- bounded memory / fuel
- no RNG，除非 runtime 提供 deterministic seed
- pure function：input snapshot + module hash -> proposal

WASM 永远不能：

- mint resources
- bypass upkeep
- change ownership
- expand claim scope
- transfer restricted starter funds
- write canonical world state

Runtime 必须：

- cap all deltas
- validate all permissions
- validate all resource mutations
- emit structured blockers
- record proposal hash in replay / audit
- preserve funding provenance

## 11. 实施切片

1. WASM ABI + host adapter
   - Gate: same input produces byte-identical proposal hash.
2. `micro_depot` canonical state / events
   - Gate: install / service / reclaim replay produces identical state.
3. quote pipeline integration
   - Gate: repair / logistics preview shows before / after with depot contribution.
4. runtime validation / apply path
   - Gate: invalid owner, unpaid upkeep, out-of-range, unsupported resource reject with structured blockers.
5. DTO / viewer surface
   - Gate: pure API and viewer show same quote, blocker, receipt, module evidence.
6. receipt / audit
   - Gate: every applied service records module hash, proposal hash, before / after quote, world change summary.
7. gameplay smoke
   - Gate: reachable lifecycle runs end to end: fresh install (`10 = 2 sink + 8 inventory`) -> measured repair/logistics service(s) -> stock reaches zero -> further service and upkeep both reject without charge or mutation -> destructive reclaim -> optional fresh full-cost reinstall. Receipts reconcile stock/throughput before/after; throughput remains an internal ceiling, not a separately claimed reachable gameplay blocker.


### 11.1 验证映射表

| 上游 requirement / product AC | 本设计条款 | 独立 obligation 与适用条件 | 准确验证方法、source / scenario / layer | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [REQ-WR-GR-001](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-001) Trace refs: O7-REQ-d34e2dec-9ad4-44a8-9ba7-7931956e39b8 | [有限库存生命周期](#micro-depot-measured-lifecycle) | micro_depot 单次 commissioning、有限服务、耗尽、upkeep 拒绝及 destructive reclaim/reinstall 的真实成本。 | 以 [measured supply 回归](../../../crates/oasis7_simulator/src/tests/micro_depot_measured_supply.rs) 的 service debit/receipt 为局部基础，按本设计 §11–12 在固定候选执行 install→库存耗尽→upkeep 无收费拒绝→reclaim→全额新装并对账。 | 实施 PR 的同候选事件、库存与资源前后值、实际玩家入口结果。 | 本映射为验证方法；本次文档审查未运行测试，不证明 refill、长期可玩性或全部设施。 |
| [AC-WR-GR-001](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#ac-wr-gr-001) Trace refs: O7-REQ-d34e2dec-9ad4-44a8-9ba7-7931956e39b8 | [有限库存生命周期](#micro-depot-measured-lifecycle) | 当前同步服务失效时无部分库存/吞吐扣减；quote 不 reserve/debit。 | [同一 measured supply 回归](../../../crates/oasis7_simulator/src/tests/micro_depot_measured_supply.rs)中的 insufficient inventory 负例与本设计 §7.1.1/§10 的 owner、range、upkeep、module/schema/proposal 各自负例；按 §8 分别核 Viewer/API。 | 实施 PR 的拒绝前后状态、receipt 与各入口 captures。 | fixture 注入的 throughput exhaustion 只是内部防御，不是自然可达玩家 blocker；独立 preview-confirm 并发合同不在当前切片。 |

## 12. Post-first-capability 10 分钟验证切片

该样本是 `regional-specialist 10-minute scenario`，不是首局 `0-10 min` onboarding 样本。运行前必须满足：

- `first_capability_completed=yes`
- `stable_claim_available=yes`
- 当前 slice 提供非空 `regional_blocker_receipt_id` opaque causal reference；目标态改由已认证 receipt 或等价 canonical regional pressure evidence 满足
- `regional_specialist_lane_state=active_or_eligible`
- 玩家已能读懂 claim/upkeep、repair/logistics blocker、before/after quote 和 receipt surface

```text
0-2 min after regional-specialist entry: player sees regional pressure card: repair target blocked by supply shortage
2-4 min: player opens repair quote and sees high cost / high risk tied to the current opaque blocker causal reference; this is not authenticated receipt-provenance evidence
4-6 min: system suggests micro_depot; player reviews cost, upkeep, radius, expected effect and coarse ROI strip
6-8 min: micro_depot.wasm proposal changes quote; runtime emits before / after preview
8-10 min: player executes repair/logistics action and receives receipt
```

通过标准：

- 玩家能说明为什么部署 depot。
- 玩家能说明 depot 花了什么。
- 玩家能指出哪一次行动被 depot 改变。
- 玩家能看到 remaining blocker 或下一步建议。
- QA 另以确定性 lifecycle smoke 覆盖 `install -> service until stock=0 -> service blocked -> upkeep rejected/no charge -> reclaim -> fresh full-cost reinstall`；该 smoke 是 consumable lifecycle/accounting evidence，不是 epoch rollover、refill 或长期 playability evidence。
- QA / smoke evidence 明确标注该样本发生在 first capability 之后，不能用于证明首局 0-10 分钟 onboarding 通过。

失败/防漂移条件：

- 若测试、样本或文案在 first capability、稳定 claim、当前 opaque blocker causal reference（目标态为已认证 repair/logistics receipt 或等价 canonical 区域压力证据）、regional specialist lane、claim/upkeep 与 repair/logistics blocker literacy 之前安排 depot 部署，标记 `micro_depot_phase_boundary_missing`。
- 若样本标题只写 `0-10 min` 而不写 `post-first-capability` 或 `regional-specialist`，同样标记 `micro_depot_phase_boundary_missing`，避免 QA / 实现把 depot 当成 early onboarding gate。

## 13. 风险

- 若 quote / receipt 字段不足，viewer 会退回 recent event 猜因果。
- 若收益太弱，depot 像税；太强，depot 变成唯一正确解。
- 若 upkeep 太早压迫新手，会削弱 starter funding 的引导价值。
- 若 module governance 提前开放玩家上传，会把 MVP 变成安全平台问题。
- 若后续设施扩展太快，oasis7 会被误读成建造游戏，而不是间接控制文明模拟。
