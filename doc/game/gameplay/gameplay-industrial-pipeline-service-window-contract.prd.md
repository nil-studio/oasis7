# 工业流水线服务窗口与电力连续性合同

- 状态：`target-contract`；当前没有 service-window/power continuity 的 runtime、Viewer、pure API 与 Agent 组合证据，不得由本文宣称 current implementation 或 release readiness。
- 权威：Product 拥有 service-window 与 power mode 的玩家承诺；M4/Power/runtime 拥有 owner-held electricity、stage/edge/buffer/terminal obligation、lease/admission、hold/debit/release/revalidation、receipt、持久化与 replay 真值；本文只拥有玩家选择、机会成本、失败恢复和 gameplay 验收投影。
- 产品语义入口：[`多阶段工业流水线与中间品背压`](../../product/world-rules-core-gameplay/prd.md#多阶段工业流水线与中间品背压)；本合同保留 service-window、lease/expiry 与 power continuity 的专业玩法权威。
- 边界：本合同不吸收维护/计划停机、battery/storage、capacity allocator、queue/fairness 或 terminal settlement authority；维护决策继续见 [`工业维护与计划停机合同`](gameplay-industrial-maintenance-planned-downtime-contract.prd.md)。

## 1. Conditional service window

仅在专业 profile 支持时，玩家才可获得只读 `pipeline_service_window_preview`，把 stage duration/ready boundary、join/output-bundle gate、edge transit、buffer admission 与 terminal settlement 组合成同一 conditional window；否则明确为 best-effort。

预览至少说明 none/soft/hard window policy、start trigger、target event、各 segment 的条件性时界与 root risk，以及 factory slot、edge throughput、destination buffer 与 terminal capacity 的 lease scope/amount/expiry；terminal capacity 未持有、仅条件成立或 non-reserved 时必须明确显示，不得表述为端到端 hard guarantee。玩家比较 `commit_window`、`run_best_effort`、`relax_or_requote_window`、改道或延期；preview 不锁容量、不保证精确 ETA 或未来条件。

## 2. 提交、lease 与迟到恢复

提交按同一快照重验完整 path 与 lease chain：hard window 任一 mandatory segment 无法满足时在首个/下一个 sink 前延期或原子拒绝；soft window 只能以可读 late 结果继续。

Lease 到期在开始前只释放未消费 hold 一次，开始后的 WIP/transit/output 继续服从既有 interruption/terminal 合同并保留 provenance；玩家按 profile 比较 accept-late、hold/quarantine、alternate terminal、reroute/requote、reduce、defer/abandon，不获得自动退款或静默续期。Stage on-time 但 transit/terminal late 时，必须保留 production receipt，不产生 on-time delivery receipt，并指出 root segment。

## 3. Service-window 验收

`test_tier_required` 至少覆盖：preview 确定且只读，commit lease/best-effort/relax 的容量与交付风险取舍可读；stage、join/bundle、transit、buffer 或 terminal 在首个 sink 前漂移时，hard 延期/重报价或无副作用原子拒绝，soft 明示 at-risk/late；首个 sink 后漂移时保留既有 sink、WIP/transit/output、identity 与 provenance，并在下一个 sink 前停止或按 profile 进入一次受支持的 hold/quarantine、accept-late、alternate terminal、reroute/requote、reduce、defer/abandon 处置，不回滚已发生效果；lease expiry/release/renewal 各只生效一次且无 orphan hold。

重连、重复 commit/renew、retry、arrival reorder 与 replay 不免费延期、刷新优先级、复制 hold/sink/reward 或刷稳定 `W`；生产准时而运输/终端迟到的样例只产生一次 late/expired recovery。Viewer、pure API 与 Agent 对 window/lease identity、segment state、on-time/at-risk/late/expired、机会成本、root blocker 和下一步保持一致。

## 4. Power obligation 投影

Power-dependent path 的 `pipeline_service_window_preview` 必须把每个 stage/cycle/branch/terminal electricity obligation 显示为 `start_only`、`held_through_completion_or_arrival`、`boundary_revalidated` 或 `non_reserved`，并说明 owner power available/held/unmet、scope/amount、下一复查边界与是否进入 hard/soft guarantee。

玩家只比较 profile 支持的 `reserve_power_for_window`、`run_best_effort_power`、`restore_power_first`、`reduce/defer` 或已有 reroute；reserve/hold 会挤占其他 recipe、交易或恢复用途，best-effort 保留灵活性但承担 WIP、late delivery、power blocker 与 `W` reset 风险。`electricity_after`、battery runway 与 maintenance pressure 都不能冒充 path reservation。

## 5. Power shortfall 与验收

Power shortfall 在 hard path 的下一 irreversible sink 前 deferred/atomic reject；soft/non-reserved path 明示 power-at-risk/late/blocked，并保留 prior receipt/WIP，使用已有 wait/repair/replan/hold/reduce/compensation。

结果展示 root/segment、actual power sink/hold/release/unmet 与 primary power blocker，downstream backlog 只是 secondary；不自动 top-up、overdraft、双扣、隐式续租或退款。

Power 的 `test_tier_required` 至少覆盖：start-only 后续余额漂移不改写旧 job；held/revalidated mode 的单次 hold/debit/release；stage 1 后 stage 2 前失电的 hard/soft 差异；production receipt 后失电无虚假 delivery finality；两个 roots 争用 power 不 over-hold/插队；unknown 不显示 0；checkpoint/cutover/retry/replay 不复制 power/W/reward。Viewer、pure API 与 Agent 对 mode、机会成本、blocker 与下一步一致。

## 6. 非目标

本文不冻结 deadline/TTL/SLA 数值、精确 ETA、power amount/formula/price、lease/capacity/queue/fairness/allocator、runtime 字段/event/schema、UI 布局、自动 renewal/curtail/top-up/refund，也不新增 battery/storage/maintenance、动作能力或当前实现声明。
