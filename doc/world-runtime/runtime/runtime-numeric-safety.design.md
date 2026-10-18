# Runtime 数值安全与原子状态转移设计

- 对应需求文档：`doc/world-runtime/runtime/runtime-numeric-safety.prd.md`
- 当前任务状态与历史变更：GitHub task issue evidence 与 Git history。

<a id="1-设计原则"></a>
## 1. 目标与设计原则

- 后继值先计算、全部校验通过后再提交。
- 算术策略是接口契约的一部分：`Result` 路径采用 checked failure；协议 ID 可采用持久化 rollover；明确允许有损边界的辅助函数才可采用 observable clamp。
- 错误映射不得丢失字段、边界值与所属子系统。
- 恢复和复制路径的数值失败必须 fail closed。

## 2. 子系统权威图

| 子系统 | 当前实现入口 | 设计要求 |
| --- | --- | --- |
| World 资源/规则 | `crates/oasis7_simulator/src/types.rs`、`runtime/world/resources.rs`、`runtime/world/step.rs`、`runtime/state/apply_domain_event_gameplay.rs` | 资源与规则聚合受检；事件 effect 预计算；失败转为拒绝且不部分提交。 |
| Node points | `crates/oasis7_runtime/src/node_points.rs` | award、cumulative、total 与 epoch successor 全部计算成功后一次提交。 |
| PoS | `crates/oasis7_consensus/src/node_pos.rs` | stake/slot 越界返回错误；proposal/vote 状态保持不变。 |
| Node engine/recovery | `crates/oasis7_node/src/pos_state_store.rs` 及 PosNodeEngine 路径 | height/slot/gap/snapshot 后继值受检；不可表示的 snapshot 拒绝启动。 |
| Replication | `crates/oasis7_node/src/replication.rs` | writer position 返回 `Result`；local commit 构造透传错误。 |
| Sequencer/lease | `crates/oasis7_consensus/src/sequencer_mainloop.rs`、`lease.rs` | slot/height/term/expiry 先计算，成功后才更新权威状态。 |
| Membership/clock | `membership_reconciliation.rs`、`membership_recovery/stores.rs`、`dht.rs`、`crates/oasis7_node/src/runtime_util.rs`、`crates/oasis7_net/src/util.rs` | schedule expiry 在写前受检；Duration 窄化按接口选择失败或显式 clamp。net 时钟的边界与测试由下文 `numeric-clock-clamp` 条款维护。 |
| PoS ratio/required stake | `crates/oasis7_proto/src/distributed_pos.rs`、`crates/oasis7_consensus/src/pos.rs`、`crates/oasis7_node/src/pos_validation.rs` | `> 1/2` 判定不通过可能溢出的乘法实现；required stake 以加宽计算和可失败窄化保持三层一致。 |
| Membership retry/replay | `crates/oasis7_consensus/src/membership_recovery/types.rs`、`mod.rs`、`replay.rs` | retry/backoff、调度/cooldown 与计数/容量后继值先受检；阈值/比率比较保持数学语义；失败不部分持久化。 |
| Governance archive/audit | `membership_recovery/replay_archive.rs`、`replay_archive_tiered.rs`、`replay_audit.rs` | drill、retention、offload、rollback 与 alert window 的时间/计数异常在状态更新前显式失败。 |
| Reconciliation/sync/mempool | `membership_reconciliation.rs`、`membership_recovery_support.rs`、`mempool.rs` | 时间差、report counter 与 payload aggregation 使用 checked helper；溢出返回 `WorldError::DistributedValidationFailed`，不得污染 schedule、dedup、report 或 batch。 |
| Federated archive | `membership_recovery/replay_archive_federated.rs` | 聚合与 composite cursor 后继值先受检；失败保留 cursor/aggregate state。 |
| Rolling IDs | `runtime/snapshot.rs`、`runtime/world/mod.rs`、`runtime/world/persistence.rs` | event/action/intent/proposal 的 `era + sequence` 一起持久化；`u64::MAX` 后分配 `(era + 1, 1)`，缺失 era 的旧 snapshot 按 `serde(default)` 读取为 0。 |

### 2.1 需求承接与分配表

| 上游 requirement / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [REQ-DWE-001](../../product/world-infrastructure/deterministic-world-execution.prd.md#req-dwe-001)；Trace refs: O7-REQ-cf672e18-ed16-42a3-9338-ab7b965ac607 | 专业依据：[PRD-WORLD_RUNTIME-041](../prd.md#prd-world-runtime-041)；可失败的权威数值推进先校验再提交；规则成本溢出形成拒绝且不部分应用。 | [原子数值转移](runtime-numeric-safety.design.md#atomic-numeric-transition) | `runtime_engineer`；资源与事件应用实现。 | 单项规则成本回归不证明全仓数值安全、分布式恢复或发布就绪。 |
| [net clock clamp 专业条款](runtime-numeric-safety.prd.md#numeric-clock-clamp) | 保留 net 时钟毫秒转换的有损边界；范围内保值、超界为 `i64::MAX`、epoch 前为 `0`。 | [clock clamp](runtime-numeric-safety.design.md#numeric-clock-clamp) | `runtime_engineer`；`oasis7_net` 时钟消费者。 | 不改变其他受检时钟接口，不将边界值解释成精确时间。 |
| [REQ-DCS-002](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-002)；Trace refs: O7-REQ-3f7d612d-5a6a-4d6d-bb0e-722c44d986f4 | 专业依据：[Action allocator 恢复](runtime-numeric-safety.prd.md#numeric-action-allocator-replay)；完整 finalized checkpoint 与 event suffix 恢复 cursor/era 及相关 queue，后续合法动作与不中断分支一致。 | [分配来源与恢复](#numeric-action-allocator-replay) | runtime 拥有 allocator/queue/replay；[共同恢复设计](../design.md#runtime-journal-suffix-recovery-design) 拥有切点、完整性与兼容范围。 | 目标记录待实现；不新增 HTTP producer 命名空间、不建立全链路复合 ID、不承诺恢复未持久化 pending。 |

<a id="atomic-numeric-transition"></a>

## 3. 状态转移模式

```text
read current state
  -> compute every numeric successor
  -> validate bounds and cross-field invariants
  -> construct complete effect / durable record
  -> commit once
```

任一步失败都直接返回稳定错误；提交前不得修改权威状态。调用方需要继续循环时，必须把错误转换为明确 rejection/evidence，而不是吞掉错误。

## 4. 策略选择

- `checked_add` / `checked_sub` / `try_from`：默认选择，适用于余额、票权、时间差、位置推进与可失败 API。
- rollover：仅在复合代际信息已经持久化且恢复/同步兼容时使用。
- clamp：仅用于函数签名明确承诺边界夹逼的兼容辅助函数；必须返回确定边界并由测试证明，调用方不得据此生成精确时间声明。

<a id="numeric-clock-clamp"></a>

### net clock clamp

产品适用性：N/A；reason=本条只保持 net 时钟辅助函数的有损转换兼容约定，不直接裁决世界效果或承诺精确时刻；scope=duration_millis_to_i64_saturating 与 unix_now_ms_i64 的边界值语义，消费者的 lease/schedule 权威转移仍由各自合同负责。

[`crates/oasis7_net/src/util.rs`](../../../crates/oasis7_net/src/util.rs) 中，`duration_millis_to_i64_saturating` 先通过 `Duration::as_millis()` 得到毫秒值，再以 `i64::try_from` 窄化；超界返回 `i64::MAX`。`unix_now_ms_i64` 对 `SystemTime::now().duration_since(UNIX_EPOCH)` 成功值调用该转换，epoch 前的错误返回 `0`。这些辅助函数只提供时钟读数，不写入世界状态；调用方的 lease、schedule 或持久化转移仍须满足各自受检与原子性合同。

现有内联单测 `tests::duration_millis_to_i64_saturating_keeps_in_range_values` 验证 `1234 ms` 保值，`tests::duration_millis_to_i64_saturating_clamps_overflow_to_i64_max` 验证 `Duration::from_secs(u64::MAX)` 返回 `i64::MAX`。这两项单测不覆盖系统时间早于 epoch 的分支；该分支目前按源码回读确认，不能写成已经通过真实时钟回拨测试。

## 5. 恢复、复制与观测

- snapshot 恢复必须在加载权威状态前验证所有必要后继值。
- replication writer、gap-fill 和 proposal 摄取不得用饱和值伪造进度。
- 错误日志或 rejection evidence 至少包含子系统、字段和导致失败的值。
- 相关测试必须同时断言错误与“状态/持久化未变化”。

<a id="numeric-action-allocator-replay"></a>

### Action allocator 的来源记录与恢复

**现状与目标。** 当前 [`snapshot.rs`](../../../crates/oasis7_runtime/src/snapshot.rs) 保存 `next_action_id`、`action_id_era` 与 `pending_actions`；[`persistence.rs`](../../../crates/oasis7_runtime/src/world/persistence.rs) 在加载 checkpoint 时赋值。分配入口在 [`actions.rs`](../../../crates/oasis7_runtime/src/world/actions.rs) 与 [`world/mod.rs`](../../../crates/oasis7_runtime/src/world/mod.rs)，[`event_processing.rs`](../../../crates/oasis7_runtime/src/world/event_processing.rs) 的既有 suffix replay 尚未闭合 action allocator 的推进。下文是待实现的专业合同，已有 snapshot 字段、event/intent replay 或边界测试不证明本合同已完成。

#### 分配记录的最小语义

宿主必须为真实分配建立版本化、可重放的来源记录，而不是从业务结果反推 cursor。记录可由 canonical journal 直接承载，或由 journal 绑定同代控制记录；采用后者时，其 bytes/hash、顺序和完成边界属于 [完整恢复材料](../design.md#runtime-journal-suffix-recovery-design)，缺失记录不能仍被视为完整 suffix。下表定义必须承载的语义，不声明新增 Rust DTO、wire 字段已经存在。

| 语义 | 约束 |
| --- | --- |
| 宿主 allocation 身份 | 绑定同一 world/history、所属宿主事务及该事务内的分配序号；在相关 checkpoint/replay 保留范围内标识一次真实分配。它由宿主确定，不由 caller 的裸 action ID、payload 相同与否或业务 job 表替代，也不扩展外部重试身份 ABI。 |
| 分配前 cursor、实际分配值、分配后 cursor | cursor 是 `(action_id_era, next_action_id)`。实际分配携带所属 era 与 sequence；后继按原版本的 rollover 规则计算并持久化。replay 校验前驱与后继，不调用 live allocator 再生成一次新分配，不用裸数字 max 或每事件加一。 |
| 来源与 action/queue 关联 | 区分 fresh allocation 与对先前 allocation 的引用；绑定原 action envelope/队列条目及消费处置。显式 ID/预分配路径必须能识别其宿主分配来源，不能把 ID 数字大小当作新分配证明；已在 checkpoint 或前序记录计入的来源不得二次推进。 |
| 所属原子边界与顺序 | 记录标识其真正的分配/入队事务和持久切点。来源记录、cursor、队列插入/移除/既有驱逐处置按各自既有事务边界一起发布；分配顺序由该控制历史确定，不由后来按优先级执行 action 或其业务事件的排列推断。 |

分配记录本身不表示业务 action 成功或取得 finality。已经持久化的分配，即使随后形成业务 `ActionRejected`，仍是一次已发生分配；若分配与入队所在事务在发布前失败，则二者均不出现。若 action 在较早事务已经分配并入队，后续 `step()` 失败必须保留 step 前的 cursor/queue，不能把此前的分配也回退。对没有业务事件的分配、未执行 pending 或既有 backpressure 驱逐，只能依靠完整 checkpoint/来源记录恢复；不能声称它们天然可从 `CausedBy::Action` 重建。本合同不修改既有队列容量、驱逐政策或错误优先级。

#### 重建算法与恢复切点

1. 先按上位设计确认同一历史的 finalized checkpoint C、目标 T 及完整 suffix，加载 C 的 cursor、已计入分配的证据与 pending queue。C=T 的 snapshot roundtrip 与 C<T 的非空 suffix 是独立验收。
2. 在暂存恢复视图按记录的权威顺序处理尚未计入基线的 fresh allocations，验证所属事务、action/queue 关联、`before` 与当前 cursor 相等，以及实际分配/`after` 符合原版本规则，再安装后继。预分配引用只关联原记录；同一已计入 allocation 不再消费 cursor，冲突或无法验证的来源使恢复失败。多条业务事件引用同一 action 也只对应那一次分配；不带 action cause 的事件不凭空推进 action cursor。
3. 同步重建队列处置：已完成的历史 action 不再入队；仍有效的持久 pending 保留原分配关联，恢复后按上位 pending 合同重新校验，不以新 ID 重新提交。队列项若已被原事务消费或按既有策略驱逐，按记录恢复该事实。只有旧 cursor 或只有较新队列均不足以证明完整状态；未持久化的内存提交不被补写成历史。
4. rollover 的证据是记录中的原代际、分配值与后继链。例如 `(e, MAX)` 分配 `MAX` 后进入 `(e+1, 1)`，后续分配 `1` 再进入 `(e+1, 2)`；即使业务处理顺序不同，也不能按裸 ID 排序丢掉该顺序。超出原版本可解释或可表示范围时沿既有数值失败合同处理，不用补洞或静默重置创造历史。
5. 任一 late record、cursor 前驱、来源或队列关联校验失败，丢弃本次完整 suffix 的所有暂存变更；cursor、queue、业务状态与 journal 保持恢复调用前的已发布状态。成功后验证 T 的完整控制状态，再用相同后续 action/tick 做不中断分支对照。历史 replay 不调用 `submit_action`、action executor、WASM 或外部 adapter 重新执行动作。

普通 `World::from_snapshot`、snapshot/load 与 event replay 必须共用该来源 reducer/校验规则。当前 [`oasis7_world_repair_rebuild.rs`](../../../crates/oasis7_ops_tools/src/bin/oasis7_world_repair_rebuild.rs) 的 `--source-world-dir` 从新建 World 的 snapshot 加源 journal 调用 `World::from_snapshot`；它只有在初始基线及源历史足够完整时才可承诺恢复，不能以保存并再加载成功掩盖缺失 allocator/pending 事实。该 binary 的 `--generated-world-dir` 用于生成世界 bootstrap，不作为历史恢复对照。

旧 snapshot 缺 era 的 `serde(default)=0` 读取保持不变，但不能由此推断旧 tail 从未 rollover、没有未记录分配或没有 pending。无法证明来源链和队列完整性的旧数据按 [共同兼容矩阵](../design.md#runtime-journal-suffix-compatibility) 处理；不得默认 cursor、重编号已提交 action、扫描 settled job 补洞或将旧事件重新执行为新业务效果。

<a id="6-演进边界"></a>
## 6. 演进边界与风险

rollover 不建立全链路复合 ID ABI：仅 runtime 内持久化 era 并保持恢复/同步兼容，纯 `u64` ID 仍可能跨 era 复用。所有失败路径继续要求结构化错误、rejection evidence 与状态/持久化未变化的回归。

## 11. 验证设计与可追溯性

以下关系维护本专题受影响的数值与时钟边界；实际候选、命令、结果和未运行项记录在 PR 与真实验证输出中。文档修补按 [测试手册](../../../testing-manual.md) 选择 S0；数值实现变化时再执行对应定向测试，不能把本表中的测试计划写成本次已经通过的结果。

<a id="numeric-safety-validation"></a>
### 11.1 验证映射表

| 上游 requirement / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source 或 ID、scenario/layer、candidate/environment | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [REQ-DWE-001](../../product/world-infrastructure/deterministic-world-execution.prd.md#req-dwe-001)；Trace refs: O7-REQ-cf672e18-ed16-42a3-9338-ab7b965ac607 | [原子数值转移](runtime-numeric-safety.design.md#atomic-numeric-transition) | 专业依据：[PRD-WORLD_RUNTIME-041](../prd.md#prd-world-runtime-041)；规则成本的某个资源项溢出时拒绝动作，其他资源项保持不变。 | 固定候选；按 [rules_enforcement.rs](../../../crates/oasis7_runtime/src/tests/rules_enforcement.rs) 的 `rule_decision_cost_overflow_rejected_without_partial_apply` 执行 L1 回归：`env -u RUSTC_WRAPPER cargo test -p oasis7 --lib --features test_tier_required rule_decision_cost_overflow_rejected_without_partial_apply`。文档修补仅核对合同与现有断言。 | PR 中的源码回读；运行时记录候选、命令与实际结果。 | 不代表其余 numeric phase、durable write、rollover、长稳或跨节点验收。 |
| [net clock clamp 专业条款](runtime-numeric-safety.prd.md#numeric-clock-clamp) | [clock clamp](runtime-numeric-safety.design.md#numeric-clock-clamp) | 范围内毫秒值保留，超界夹到 `i64::MAX`；epoch 前返回 `0` 的当前实现不得被误写成受检错误。 | 固定候选；按 [测试手册 S4](../../../testing-manual.md) 选择 net 单测，窄命令 `env -u RUSTC_WRAPPER cargo test -p oasis7_net --lib duration_millis_to_i64_saturating_` 对应上文两项内联测试；epoch 前分支另行回读 `crates/oasis7_net/src/util.rs::unix_now_ms_i64`。 | PR 中的代码/测试定位；实际运行时记录两个用例及执行环境。 | 两项单测不验证真实系统时钟回拨、epoch 前分支或消费者的分布式行为。 |
| [REQ-DCS-002](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-002)；Trace refs: O7-REQ-3f7d612d-5a6a-4d6d-bb0e-722c44d986f4 | [分配来源与恢复](#numeric-action-allocator-replay) | 专业依据：[Action allocator 恢复](runtime-numeric-safety.prd.md#numeric-action-allocator-replay)；C<T 恢复完整 cursor/queue，后续合法 recipe 正常完成，原已结算 job 不被复用；多事件/拒绝、预分配、pending 和 rollover 各自成立。 | 在 [恢复测试](../../../crates/oasis7_runtime/src/tests/persistence_recovery_tests.rs) 等入口扩展下表 J1 计划场景，固定同一 source/runtime/manifest/input；目标为 `test_tier_required` 的确定性恢复回归，文档修订仅 S0。 | 实施 PR 的场景、候选、实际命令、cursor/queue 对照及 recipe disposition/费用/产出；与 [共同恢复验收](../design.md#runtime-journal-suffix-validation) 汇总。 | 新记录与场景未实现/未运行；现有边界单测不证明 J1 已修复，也不证明真实部署或完整 signed-pending 服务。 |

<a id="numeric-action-allocator-validation"></a>
### 11.2 J1 计划场景与现有测试边界

以下 `J1-*` 是 **planned 验收场景 ID**，不是现有测试函数名。现有源码仅用于选择实现与扩展位置：[`basic.rs`](../../../crates/oasis7_runtime/src/tests/basic.rs) 的 `action_id_rolls_over_into_next_era` 定义 live 分配跨 era 的断言；[`persistence_recovery_compat_tests.rs`](../../../crates/oasis7_runtime/src/tests/persistence_recovery_compat_tests.rs) 的 `snapshot_json_without_era_fields_keeps_backward_compatibility` 只断言缺 era 的 JSON 解码；[`economy.rs`](../../../crates/oasis7_runtime/src/tests/economy.rs) 的 `schedule_recipe_consumes_inputs_and_power_then_produces_outputs` 是正常生产基线。源码存在不代表本次运行通过，它们也不能代替下面的非空 suffix 与续跑组合。

| 场景 ID / 层级 / 状态 | 输入与独立断言 | 已存在的扩展位置或入口 |
| --- | --- | --- |
| J1-CONTINUE / required / planned | 在合法 finalized C 后执行 recipe 并结算至 T；用 C 与完整非空 suffix 恢复，比较 action cursor/era、queue、历史 job/receipt，再向恢复和不中断分支提交同一合法新 recipe。新动作在相同条件下接受并正常完成一次，不复用旧 settled job；旧费用/产出不重复，新费用/产出与对照一致。 | [`persistence_recovery_tests.rs`](../../../crates/oasis7_runtime/src/tests/persistence_recovery_tests.rs)、上述 `economy.rs` 基线。 |
| J1-ORDER / required / planned | 一动作多事件、多个相同 payload 的独立动作、业务拒绝、无 action cause 的 system event，以及分配顺序与业务处理顺序不同。每条真实分配推进一次，各动作保持自身关联；无分配的事件不推进，重复来源或不符前驱不得静默接受。 | `basic.rs`、`persistence_recovery_tests.rs`；[`event_processing.rs`](../../../crates/oasis7_runtime/src/world/event_processing.rs) 为恢复入口。 |
| J1-PENDING / required / planned | C 已含预分配 pending，suffix 消费其中一项且保留另一项；另覆盖 C 后持久分配但尚无业务事件、显式复用此前 ID、既有驱逐和分配事务失败。恢复 cursor 与 queue 同切点，已消费项不重入队、未完成项不换 ID；step 失败不回退此前持久分配，缺控制记录走兼容/fault。 | [`actions.rs`](../../../crates/oasis7_runtime/src/world/actions.rs)、[`cognition_persistence.rs`](../../../crates/oasis7_runtime/src/world/cognition_persistence.rs) 为普通/预分配入口；扩展上述恢复测试。 |
| J1-ROLLOVER / required / planned | 非空 suffix 横跨 `MAX-1 -> MAX -> 1`，C 的 cursor 已接近边界且可含预分配项；分别验证正确 era 后继、旧 era=0 解码、旧 tail 信息不足与畸形后继拒绝。counter 对照不宣称纯 `u64` job ID 跨 era 永久唯一。 | 上述 `basic.rs` 与 `persistence_recovery_compat_tests.rs`；旧数据处置引用共同兼容矩阵。 |
| J1-ENTRYPOINTS / required / planned | 同一充分的基线/历史分别走 from_snapshot、普通 save/load 和 source-world-dir repair，再执行相同后续动作；另在 suffix 尾部注入非法分配/queue 关联，验证无部分发布。repair 缺初始材料时明确失败或受限读取；generated bootstrap 独立验证。 | [`persistence.rs`](../../../crates/oasis7_runtime/src/tests/persistence.rs)、`persistence_recovery_tests.rs` 与 [`oasis7_world_repair_rebuild.rs`](../../../crates/oasis7_ops_tools/src/bin/oasis7_world_repair_rebuild.rs)；现有 generated-bootstrap 测试不覆盖 source-journal repair。 |
