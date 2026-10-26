# 共识代码收敛到 oasis7_consensus 设计

## 1. 问题、目标与非目标

将共识纯逻辑集中于唯一core，避免node/pos重复规则漂移，同时保护ordered payload及committed replay。

Owner role：runtime_engineer；canonical repository：eng-cc/oasis7；固定审读基线 f9d5a552d9af04c1b1398262808198a58e560230，2026-09-26。本设计消费下表有限关系；旧章节保留作兼容详细条款，新增DES细化原义务而不改算法/schema/默认值/产品承诺。任务实际source/integration/tested tree/config/environment/window/结果由GitHub Issue/Project evidence维护，本文不伪造候选/通过结果。

非目标：不以文档active/合并、local fixture、历史MIG或CLI可编译宣称运行、BFT、部署、SLA或release。剩余215-object required set及root SC1..10/full组合不变。

## 2. 上游约束与相关角色

runtime/P2P拥有机制；ops拥有同窗口部署事实；QA拥有组合证据判定；producer拥有产品含义；消费者和WASM拥有各自schema/manifest/行为。professional_acceptance与product_requirement是关系类型，本文不新增机器trace schema或activation。

### 2.1 需求承接与分配表

有限产品关系只适用于下表 ordered-envelope 与 commit-replay 的原有子义务。Trace refs: O7-REQ-cf672e18-ed16-42a3-9338-ab7b965ac607, O7-REQ-4fcbad4f-f576-4bcf-b4fc-b027967b0405。唯一声明分别为 [REQ-DWE-001](../../product/world-infrastructure/deterministic-world-execution.prd.md#req-dwe-001) 与 [REQ-DWE-002](../../product/world-infrastructure/deterministic-world-execution.prd.md#req-dwe-002)；上层接收边界分别见 [runtime-deterministic-design](../../world-runtime/design.md#runtime-deterministic-design) 与 [runtime-pending-design](../../world-runtime/design.md#runtime-pending-design)。本关系不改产品 partial/未核实状态，不代签 full AC；core/dependency/stage2 的独立工程验收不借产品 ID 合并。

| 上游 requirement / product AC / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [professional_acceptance: ccg-core](consensus-code-consolidation-to-oasis7-consensus.prd.md#ccg-core) | 唯一types/functions/status adapter；node保留接线与Consensus error、顺序不回退 | [des-ccg-core-adapter](#des-ccg-core-adapter) | runtime/node/net/distfs与QA；相关外部authority | local test source≠执行通过；全拓扑/恢复/finality/release未证明；产品层不适用；理由：共享类型、函数与状态 adapter 的代码收敛本身不是独立玩家结果；范围：仅 ccg-core 的 AC-1..6 工程分配，保留 node alias/接线/Consensus error 与原顺序；owner：runtime/P2P、node 与 QA；证据：配对 PRD ccg-core 及原 AC-1..6、§11.1 局部溢出测试定义，old/new 完整序列仍 planned；重评条件：新状态、算法、ABI 或对外结果含义改变。 |
| [professional_acceptance: ccg-dependency](consensus-code-consolidation-to-oasis7-consensus.prd.md#ccg-dependency) | node→consensus单向、DHT/net abstractions内聚且无反向net dependency | [des-ccg-dependency](#des-ccg-dependency) | runtime/node/net/distfs与QA；相关外部authority | local test source≠执行通过；全拓扑/恢复/finality/release未证明；产品层不适用；理由：crate 单向依赖与抽象归属是工程结构约束，不是独立玩家结果；范围：仅 ccg-dependency 的 node→consensus、DHT/net 内聚和无反向依赖/循环；owner：runtime/P2P、net 与 QA；证据：配对 PRD ccg-dependency 及原依赖分层条款，依赖图审读/编译仍 planned；重评条件：依赖方向、抽象边界或 PoS/成员治理语义改变。 |
| [professional_acceptance: ccg-stage2](consensus-code-consolidation-to-oasis7-consensus.prd.md#ccg-stage2) | 残留action-root/signature/messages纯逻辑集中；pos复用node_pos，外部API/闭环不回退 | [des-ccg-single-core](#des-ccg-single-core) | runtime/node/net/distfs与QA；相关外部authority | local test source≠执行通过；全拓扑/恢复/finality/release未证明；产品层不适用；理由：纯逻辑归并与同核心复用是维护机制，不能独立代表确定性执行或 BFT；范围：仅 ccg-stage2 的 action-root/签名/消息迁移、pos 复用及外部 API/闭环不回退；owner：runtime/P2P、node/pos 与 QA；证据：配对 PRD ccg-stage2 及原第二阶段条款，完整调用点和 old/new 序列仍 planned；重评条件：算法、外部 API、闭环语义或产品承诺改变。 |
| [product_requirement: REQ-DWE-001](../../product/world-infrastructure/deterministic-world-execution.prd.md#req-dwe-001) | envelope hash/root/order/version提交前一致；不丢known runtime payload；专业来源：[professional_acceptance: ccg-ordered-replay](consensus-code-consolidation-to-oasis7-consensus.prd.md#ccg-ordered-replay) | [des-ccg-ordered-envelope](#des-ccg-ordered-envelope) | runtime/node/net/distfs与QA；相关外部authority | local test source≠执行通过；全拓扑/恢复/finality/release未证明；DWE-001 仅承接同 ordered input 的 envelope/action-root/版本完整性前置条件，不覆盖全部验证者、实例隔离、runtime 原子应用或 full AC-DWE-001 |
| [product_requirement: REQ-DWE-001](../../product/world-infrastructure/deterministic-world-execution.prd.md#req-dwe-001) | 只committed按序回放；accepted/pending/empty batch非世界推进，失效显式拒绝/requeue；专业来源：[professional_acceptance: ccg-ordered-replay](consensus-code-consolidation-to-oasis7-consensus.prd.md#ccg-ordered-replay) | [des-ccg-commit-replay](#des-ccg-commit-replay) | runtime/node/net/distfs与QA；相关外部authority | local test source≠执行通过；全拓扑/恢复/finality/release未证明；DWE-001 仅 committed 同序消费；DWE-002 仅原 accepted/pending/empty 无效果边界，不承接通用 pending 持久化、当前条件重审、撤回/替代或恢复服务，不覆盖 full AC-DWE-001/002 |
| [product_requirement: REQ-DWE-002](../../product/world-infrastructure/deterministic-world-execution.prd.md#req-dwe-002) | accepted/pending/empty batch非世界推进（仅 DWE-002 原无效果子义务；committed 同序与显式拒绝/requeue 仍由上一独立关系保留）；专业来源：[professional_acceptance: ccg-ordered-replay](consensus-code-consolidation-to-oasis7-consensus.prd.md#ccg-ordered-replay) | [des-ccg-commit-replay](#des-ccg-commit-replay) | runtime/node/net/distfs与QA；相关外部authority | local test source≠执行通过；全拓扑/恢复/finality/release未证明；DWE-001 仅 committed 同序消费；DWE-002 仅原 accepted/pending/empty 无效果边界，不承接通用 pending 持久化、当前条件重审、撤回/替代或恢复服务，不覆盖 full AC-DWE-001/002 |

## 3. 当前状态、目标状态与差距

| 对象 | 当前专业合同（冻结source） | 目标 / 差距 | 实现 / 执行证据 |
| --- | --- | --- | --- |
| 本专题现行规则 | 配对PRD与保留旧章节的有界接口/失败合同 | 泛型复杂度、pending→commit时机、接口震荡和分散接线风险；core owner runtime/P2P在新状态/算法/ABI变化时联审，禁止把阶段完成当finality发布。 | 本次source阅读，未运行behavior tests；具体测试定义存在不等于覆盖或通过 |
| 跨节点组合能力 | local documented contract，历史完成是provenance | 依赖root/runtime/ops/consumer与full同candidate验证 | 运行/部署/发行未验证；不能由文档推导 |

## 4. 边界与结构

本专题的 core 收敛针对 PoS/Node 推进；基于 `oasis7_proto` wire 类型的共享、纯、版本化 BFT 证明验证合同由 `oasis7_formal_protocol::bft` 承载，consensus 保留 machine、journal、持久化并重导出同一份协议 API，具体边界见 [P2P 结构](../design.md#4-边界与结构)。

consensus node_pos拥有泛型attestation/pending/decision与状态adapter；node拥有runtime state、网络gossip/endpoint、复制/execution hook/snapshot接线及NodeError翻译；pos复用同core。依赖node→consensus，consensus内聚DHT/net abstraction，不能反依赖net。

### des-ccg-core-adapter

唯一types/functions/status adapter；node保留接线与Consensus error、顺序不回退。

node准备精确有序actions→core构造proposal→插入/推进attestations→core产出decision→node按原推进顺序接线。错误统一Consensus reason。proposal→commit→replication携带同envelope/root；回放先校验version/hash/order/root/decode，再由runtime apply committed context；accepted/pending/空batch无效果。

### des-ccg-dependency

node→consensus单向、DHT/net abstractions内聚且无反向net dependency。

node准备精确有序actions→core构造proposal→插入/推进attestations→core产出decision→node按原推进顺序接线。错误统一Consensus reason。proposal→commit→replication携带同envelope/root；回放先校验version/hash/order/root/decode，再由runtime apply committed context；accepted/pending/空batch无效果。

### des-ccg-single-core

残留action-root/signature/messages纯逻辑集中；pos复用node_pos，外部API/闭环不回退。

node准备精确有序actions→core构造proposal→插入/推进attestations→core产出decision→node按原推进顺序接线。错误统一Consensus reason。proposal→commit→replication携带同envelope/root；回放先校验version/hash/order/root/decode，再由runtime apply committed context；accepted/pending/空batch无效果。

### des-ccg-ordered-envelope

envelope hash/root/order/version提交前一致；不丢known runtime payload。

node准备精确有序actions→core构造proposal→插入/推进attestations→core产出decision→node按原推进顺序接线。错误统一Consensus reason。proposal→commit→replication携带同envelope/root；回放先校验version/hash/order/root/decode，再由runtime apply committed context；accepted/pending/空batch无效果。

### des-ccg-commit-replay

只committed按序回放；accepted/pending/empty batch非世界推进，失效显式拒绝/requeue。

node准备精确有序actions→core构造proposal→插入/推进attestations→core产出decision→node按原推进顺序接线。错误统一Consensus reason。proposal→commit→replication携带同envelope/root；回放先校验version/hash/order/root/decode，再由runtime apply committed context；accepted/pending/空batch无效果。

## 5. 关键运行流程

node准备精确有序actions→core构造proposal→插入/推进attestations→core产出decision→node按原推进顺序接线。错误统一Consensus reason。proposal→commit→replication携带同envelope/root；回放先校验version/hash/order/root/decode，再由runtime apply committed context；accepted/pending/空batch无效果。

拒绝保留可定位原因；timeout/retry受原策略有界限制。并发冲突遵循原guard/precondition/共识authority，不能因接线或重连获得新权限。取消/替代只有外部专业合同明确支持时成立，本设计不新建取消或补偿schema。

## 6. 接口与数据合同

NodePosAttestation、NodePosPendingProposal<TAction,TStatus>、NodePosDecision<TAction,TStatus>由consensus产生/node alias消费；NodePosStatusAdapter只映射node枚举。propose_next_head/advance_pending_attestations/insert_attestation/decision_from_proposal不改规则。签名绑定完整有序root/payload；未知非runtime按版本跳过或拒绝，known runtime不丢。

| producer → consumer | 输入身份/版本 | 顺序与幂等 | success / error | 兼容与资源边界 |
| --- | --- | --- | --- | --- |
| consensus core → node/runtime | 原PRD列明identity/hash/version，不复制schema | 上述flow及DES验证顺序；local幂等不推导global exactly-once | 完成相应validation/apply/persist/publish后才声明该结果；失败不伪进展 | 原policy/defaults及旧data兼容；不加未知deadline/阈值 |

## 7. 状态、事务与持久化

pending→decision的原更新顺序不可变化；拒绝decision动作显式失败或requeue，不静默丢失。commit与runtime state apply的原子/持久证明归runtime/node。抽取没有新增QC/锁/round持久状态，旧snapshot/replay兼容必须回归，不能拿crate编译当恢复。

accepted、applied、persisted、published必须分别具备各自证据；workflow done/文档active不属于运行状态。外部receipt/journal/state root生命周期和recovery由runtime authority；本地观察不能提供更强保证。

## 8. 部署、安全与运行约束

只有既有授权validator参与proposal/vote；迁移不扩大权限或改签名算法。大小/queue admission由root/node原合同限制；保留无完整Ethereum/fork-choice/BLS升级、无一次性网络代码迁移。

设计只定义受控输入和失败条件；操作留在现行runbook。当前governed live网络raw-copy禁令和signed-V2边界只适用于该runbook环境，不否定历史/offline local事实。任何真实fleet/health/restore claim需固定inventory/manifest/package/config与同窗口证据，不从triad标签或本地成功推导。

## 9. 质量与容量

| 环境/刺激 | 预期响应 | 判定指标/阈值来源 | 验证入口 | 当前证明范围 |
| --- | --- | --- | --- | --- |
| 同冻结候选的local unit/fixture，逐DES负例与旧数据兼容 | 精确拒绝/no false mutation；正确路径满足原顺序 | 配对PRD常量/default/policy；未定义容量/latency不新增数字 | §11.1逐场景的source/手册 | source定义/计划，SN1未执行 |
| 多节点/恢复/服务角色组合，固定world/version/config/window | 同history/root或明确隔离/只读，不产生无证世界效果 | root与testing-manual S9A；root历史lag≤50、DistFS ratio≤0.1、mismatch0及insufficient_data阻断口径保留 | [testing-manual](../../../testing-manual.md) S9A/S9/S10；按原场景选择tier | 无真实规模、窗口或容量/SLA执行证明 |

## 10. 兼容、迁移与回滚

先共享core/types/adapters，再迁纯action-root/signature/message逻辑、pos复用，逐调用点回归。对外node API/错误行为及旧ordered payload必须兼容；无法维持同输入结果时阻断迁移。回退code位置不能撤回已经committed历史，回放依原版本。

本文保留原path/旧章节anchors与历史ID，不替换task truth；添加的接受/DES只细化旧合同。新输入、消费者或语义变更需重新绑定/专业审读。package/config回退不能复原已删chain state或撤回committed效果；破坏性操作依原runbook与runtime恢复合同。

## 11. 验证设计与可追溯性

下表是可复用计划：existing test source、planned场景与executed evidence分开。每条只消费准确upstream/DES；执行时须同可比较candidate/config/入口/环境，local fixture不升级为真实网络。未定位完整测试写planned，owner为runtime/QA；缺证阻断该组合结论，不能从required局部降低root full。

### 11.1 验证映射表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source 或 ID、scenario/layer、candidate/environment 要求或选择规则 | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [professional_acceptance: ccg-core](consensus-code-consolidation-to-oasis7-consensus.prd.md#ccg-core) | [des-ccg-core-adapter](#des-ccg-core-adapter) | 唯一types/functions/status adapter；node保留接线与Consensus error、顺序不回退 | [testing-manual S4/S9A](../../../testing-manual.md)；exact local test source `crates/oasis7_consensus/src/node_pos.rs`；existing insert_attestation_rejects_overflow_without_mutating_proposal：approved stake 溢出返回错误，attestations 仍空、approved_stake=1、rejected_stake=0、status Pending；仅定义局部不变条件，未运行，不证明 adapter old/new 完整序列；同inputs/status old-new outcome比较planned，局部core与node定向required。 | 当次GitHub task Issue evidence、实际命令/exit code/log/artifact；实际source/integration/tested tree/config/environment/window在执行时绑定 | local test source≠执行通过；全拓扑/恢复/finality/release未证明；产品层不适用；理由：共享类型、函数与状态 adapter 的代码收敛本身不是独立玩家结果；范围：仅 ccg-core 的 AC-1..6 工程分配，保留 node alias/接线/Consensus error 与原顺序；owner：runtime/P2P、node 与 QA；证据：配对 PRD ccg-core 及原 AC-1..6、§11.1 局部溢出测试定义，old/new 完整序列仍 planned；重评条件：新状态、算法、ABI 或对外结果含义改变。 |
| [professional_acceptance: ccg-dependency](consensus-code-consolidation-to-oasis7-consensus.prd.md#ccg-dependency) | [des-ccg-dependency](#des-ccg-dependency) | node→consensus单向、DHT/net abstractions内聚且无反向net dependency | [testing-manual.md](../../../testing-manual.md)；S4 required；依赖图/调用点静态审读+consensus/node既有lib suite编译计划；只证明边界/编译，不是部署。 | 当次GitHub task Issue evidence、实际命令/exit code/log/artifact；实际source/integration/tested tree/config/environment/window在执行时绑定 | local test source≠执行通过；全拓扑/恢复/finality/release未证明；产品层不适用；理由：crate 单向依赖与抽象归属是工程结构约束，不是独立玩家结果；范围：仅 ccg-dependency 的 node→consensus、DHT/net 内聚和无反向依赖/循环；owner：runtime/P2P、net 与 QA；证据：配对 PRD ccg-dependency 及原依赖分层条款，依赖图审读/编译仍 planned；重评条件：依赖方向、抽象边界或 PoS/成员治理语义改变。 |
| [professional_acceptance: ccg-stage2](consensus-code-consolidation-to-oasis7-consensus.prd.md#ccg-stage2) | [des-ccg-single-core](#des-ccg-single-core) | 残留action-root/signature/messages纯逻辑集中；pos复用node_pos，外部API/闭环不回退 | [testing-manual.md](../../../testing-manual.md)；S4 required定向old/new相同proposal/attestation/decision序列与node/viewer committed replay；单核心调用点审读planned，外部完整组合仍缺。 | 当次GitHub task Issue evidence、实际命令/exit code/log/artifact；实际source/integration/tested tree/config/environment/window在执行时绑定 | local test source≠执行通过；全拓扑/恢复/finality/release未证明；产品层不适用；理由：纯逻辑归并与同核心复用是维护机制，不能独立代表确定性执行或 BFT；范围：仅 ccg-stage2 的 action-root/签名/消息迁移、pos 复用及外部 API/闭环不回退；owner：runtime/P2P、node/pos 与 QA；证据：配对 PRD ccg-stage2 及原第二阶段条款，完整调用点和 old/new 序列仍 planned；重评条件：算法、外部 API、闭环语义或产品承诺改变。 |
| [product_requirement: REQ-DWE-001](../../product/world-infrastructure/deterministic-world-execution.prd.md#req-dwe-001) | [des-ccg-ordered-envelope](#des-ccg-ordered-envelope) | envelope hash/root/order/version提交前一致；不丢known runtime payload；专业来源：[professional_acceptance: ccg-ordered-replay](consensus-code-consolidation-to-oasis7-consensus.prd.md#ccg-ordered-replay) | [testing-manual S4/S9A](../../../testing-manual.md)；exact local test source `crates/oasis7_node/src/tests_action_payload.rs`；existing submit_consensus_action_payload_rejects_zero_action_id / submit_consensus_action_payload_rejects_payload_over_limit / submit_consensus_action_payload_rejects_queue_saturation；三个测试分别定义 zero ID → Consensus error、5 bytes 对 limit=4 返回 payload too large、queue limit=1 首次 accepted 后第二次 queue saturated；均未运行，不是完整 envelope 验收；tamper/root/order/version/decode/未知vs已知矩阵planned，无effect oracle。 | 当次GitHub task Issue evidence、实际命令/exit code/log/artifact；实际source/integration/tested tree/config/environment/window在执行时绑定 | local test source≠执行通过；全拓扑/恢复/finality/release未证明；DWE-001 仅承接同 ordered input 的 envelope/action-root/版本完整性前置条件，不覆盖全部验证者、实例隔离、runtime 原子应用或 full AC-DWE-001 |
| [product_requirement: REQ-DWE-001](../../product/world-infrastructure/deterministic-world-execution.prd.md#req-dwe-001) | [des-ccg-commit-replay](#des-ccg-commit-replay) | 只committed按序回放；accepted/pending/empty batch非世界推进，失效显式拒绝/requeue；专业来源：[professional_acceptance: ccg-ordered-replay](consensus-code-consolidation-to-oasis7-consensus.prd.md#ccg-ordered-replay) | [testing-manual S4/S9A](../../../testing-manual.md)；exact local test source `crates/oasis7_node/src/tests_action_payload.rs`；existing pos_engine_apply_rejected_decision_surfaces_requeue_overflow_instead_of_dropping：定义保留 requeue rejected consensus actions failed 与 engine buffer saturated 原因、queue len=1、next_height=1；未运行，不证明 world apply 或 committed replay 原子性；node+runtime+consumer同candidate replay/error场景planned；不以本地成功关闭DWE1。 | 当次GitHub task Issue evidence、实际命令/exit code/log/artifact；实际source/integration/tested tree/config/environment/window在执行时绑定 | local test source≠执行通过；全拓扑/恢复/finality/release未证明；DWE-001 仅 committed 同序消费；DWE-002 仅原 accepted/pending/empty 无效果边界，不承接通用 pending 持久化、当前条件重审、撤回/替代或恢复服务，不覆盖 full AC-DWE-001/002 |
| [product_requirement: REQ-DWE-002](../../product/world-infrastructure/deterministic-world-execution.prd.md#req-dwe-002) | [des-ccg-commit-replay](#des-ccg-commit-replay) | accepted/pending/empty batch非世界推进（仅 DWE-002 原无效果子义务；committed 同序与显式拒绝/requeue 仍由上一独立关系保留）；专业来源：[professional_acceptance: ccg-ordered-replay](consensus-code-consolidation-to-oasis7-consensus.prd.md#ccg-ordered-replay) | [testing-manual S4/S9A](../../../testing-manual.md)；exact local test source `crates/oasis7_node/src/tests_action_payload.rs`；existing pos_engine_apply_rejected_decision_surfaces_requeue_overflow_instead_of_dropping：定义保留 requeue rejected consensus actions failed 与 engine buffer saturated 原因、queue len=1、next_height=1；未运行，不证明 world apply 或 committed replay 原子性；node+runtime+consumer同candidate replay/error场景planned；该 requeue 定义只证明其局部失败断言，不证明 accepted/pending/empty 的世界无效果；同候选下三类输入的 committed receipt/世界状态无推进对照仍 planned，不关闭 DWE-002。 | 当次GitHub task Issue evidence、实际命令/exit code/log/artifact；实际source/integration/tested tree/config/environment/window在执行时绑定 | local test source≠执行通过；全拓扑/恢复/finality/release未证明；DWE-001 仅 committed 同序消费；DWE-002 仅原 accepted/pending/empty 无效果边界，不承接通用 pending 持久化、当前条件重审、撤回/替代或恢复服务，不覆盖 full AC-DWE-001/002 |

## 12. 决策、长期风险与未决问题

选择保留现有专业合同并精确分配，拒绝以导航/历史标签免除真实设计，也拒绝将local证明升级为full。泛型复杂度、pending→commit时机、接口震荡和分散接线风险；core owner runtime/P2P在新状态/算法/ABI变化时联审，禁止把阶段完成当finality发布。

runtime/P2P负责缺失技术/实现；ops负责环境事实；QA负责计划覆盖和verdict；producer审读产品含义。新算法/schema、权限/默认值变化、额外消费者或claim扩展触发重新评审；未解除项保留在GitHub正式evidence及剩余required set，不能靠未来Issue占位关闭。下列原章节的里程碑/审计轮次是保留provenance，不维护可变进度。



- 对应需求文档: `doc/p2p/consensus/consensus-code-consolidation-to-oasis7-consensus.prd.md`
- 对应GitHub Issue/Project task truth: GitHub Issue / GitHub Project

## 1. 设计定位
定义共识相关实现向 `oasis7_consensus` 统一收敛的方案，减少重复代码、跨 crate 漂移和一致性维护成本。

## 2. 设计结构
- 模块收敛层：把分散的共识逻辑归并到统一 crate。
- 接口边界层：明确外部 crate 只依赖稳定共识接口，而不是复制实现。
- 回归保护层：在收敛过程中用 targeted 测试兜底语义一致性。
- 迁移闭环层：配套文档、依赖和调用点同步回写。

## 3. 关键接口 / 入口
- `oasis7_consensus`
- 外部调用接口
- 共识逻辑迁移边界
- 回归测试矩阵

## 4. 约束与边界
- 目标是代码收敛，不是重写共识算法。
- 外部 crate 依赖要逐步迁移，避免一次性大爆炸。
- 共识语义必须保持稳定，不能因目录迁移引入行为漂移。
- 文档需要把“权威实现位置”讲清楚。

## 5. 设计演进计划
- 先冻结收敛目标和边界。
- 再迁移分散实现到统一 crate。
- 最后通过回归和文档互链完成收口。
