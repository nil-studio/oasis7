# oasis7 Runtime：链 PoS 时间与控制面合同设计

审计轮次: 1

- 上游规格：`doc/world-runtime/runtime/chain-pos-control-plane.prd.md`
- 当前任务状态与实现过程：GitHub task issue evidence 与 Git history。

## 1. 设计目标

把 PoS 时间语义收敛为一个可由 runtime、launcher、status 与恢复链路共同引用的专业合同：共享时间锚决定 logical tick/slot/phase，runtime 只在确定的相位和完整 guard 下推进，恢复只恢复已持久化后果。

## 2. 结构与数据流

```text
validated defaults / CLI
          |
          v
slot-clock genesis + timing config
          |
          v
logical_tick -> slot + tick_phase -> guard evaluation -> proposal or idle
          |                                  |
          v                                  v
missed counters / snapshot ------------> status (read-only)
          |
          v
restart / checkpoint / replay
```

- `chain_pos_defaults` 提供受校验的仓库默认参数；CLI 可以显式覆盖，但不能接受零值 duration/ticks 或越界 phase。
- timing calculation 只依赖共享输入与整数公式，避免把本地 scheduler jitter 转化为共识语义。
- strict-lag alignment 只生成当前 tick 的临时恢复边沿；它不是持久状态，也不是 replay 中的“补块”指令。
- snapshot/reconcile 保存结果性 counters 和已选配置；状态读取从 immutable snapshot 构造，不可借由读取写入或延长任何观测 episode。

## 3. 边界与错误处理

- future/stale proposal 或 attestation：在进入后续共识处理前拒绝；不得以本地 wall-clock 差异放宽窗口。
- launcher 输入错误：在启动/构造参数阶段给出字段级错误；不以隐式默认值掩盖显式非法值。
- restart 数据不一致：保留持久状态为诊断依据并 fail closed；不得重置 counters 来伪造健康进度。
- 无新 committed action 的 status polling：仅返回当前状态，不得被解释为 logical world progression。

## 4. 演进约束

### 风险与联审约束

任何涉及 slot 算法、准入窗口、validator 规则、参数 ABI 或跨节点配置源的修改都需要 TPM 触发相应 runtime/系统/链角色联审。此设计不授权改动协议经济或节点运维流程。

## PoS 适用设计视图
owner runtime_engineer；审读基线 eng-cc/oasis7@f9d5a552d9af04c1b1398262808198a58e560230。本设计 current timing/control/status 合同不等于 BFT/activation/service-ready。上游专业 PRD §2.1–2.4、SC-1/2/3 是准确适用范围；validator/ABI/ops/economy 与玩家规则保持外部 authority。

<a id="pos-timing-state"></a>
### 时间输入、状态与拒绝顺序
validated defaults/CLI → shared genesis/config → integer logical_tick/slot/tick_phase → monotone observed cursors/missed counters → guards → proposal or idle。duration/ticks>0、phase<ticks；非法显式值先字段级拒绝，不以默认替代。steady phase 必须相等且 next_slot<=current；严格 next_slot<current 对齐仅产生当前 tick edge，blocked 后丢弃、不 sticky、不下 tick 重试资格；pending/replication/participation/local enablement/expected proposer/signature/threshold/execution/fork/finality 均不被绕过。入站 future/stale/target epoch 在共识后续处理前拒绝，不能以 wall-clock jitter 放宽。
当前 defaults/CLI/status/startup reconcile 来源分别 chain_pos_defaults.rs、bin/oasis7_chain_runtime/cli.rs、status_payload.rs、startup_reconcile.rs。launchers 只校验透传，chain_node_tick_ms 是 poll/fallback interval。timing 不赋 manifest 激活选择权，[root version](../design.md#runtime-version-design) 按 canonical block 判断；同样 timing green 不开放 [service](../design.md#runtime-recovery-design)。

<a id="pos-persistence-readonly"></a>
### 状态、事务、持久化与恢复
config/observed_slot/observed_tick/missed accounting 属 snapshot/recovery 输入；off-phase edge 属 tick-local 暂态，不能持久/重放成补块许可。timing 计算不修改历史输入，status 由 immutable snapshot 生成，无推进、续租、观测 evidence。proposal publication 仍走既有 guard/consensus/execution transaction，不另设 timing inner commit。
restart 读取一致的 persisted snapshot 单调继续；不一致保留诊断 fail closed，不能 reset counters 伪造健康。replay 只消费记录的事件/snapshot/canonical log，不按当前 clock 补历史 proposal/block/event。generation/checkpoint/retention 由 [storage R0–R3](runtime-storage-footprint-governance.design.md#42-replay-contract) 承接，本专题不新增 crash 原子性。capacity/资源仅既有 timing/queue guards，不定义吞吐阈值；observability 只读同名 config/counters。compatible overrides/default parsing 保持当前回归，参数/算法/跨节点配置变化需 runtime/P2P/launcher/QA 联审；残余风险是共享输入漂移、edge sticky 化、status 被当 progression。

<a id="pos-case-shared-formula"></a>
#### 共享genesis与整数logical_tick/slot/phase公式跨节点同输入同结果，missed accounting不补历史
本分项按 [时间状态](#pos-timing-state) 与 [持久只读](#pos-persistence-readonly) 的输入、guards、失败/恢复边界执行；需要同config/now/genesis/snapshot比较，不授予BFT/manifest/service/W权限。

<a id="pos-case-config-rejection"></a>
#### duration/ticks正值、phase<tps，explicit非法CLI字段级拒绝；default config权威与透传

产品层 N/A；reason: 本条是默认参数解析、显式非法参数拒绝与launcher无损透传的内部配置一致性验收，不独立设立产品时序或协议权利；scope: 仅duration/ticks/phase合法性、默认配置权威与启动参数透传；不排除共享输入确定性、全部guard、恢复/服务/经济与发布的既有产品义务。仅免新增产品叶分配，不免本条专业验收。
本分项按 [时间状态](#pos-timing-state) 与 [持久只读](#pos-persistence-readonly) 的输入、guards、失败/恢复边界执行；需要同config/now/genesis/snapshot比较，不授予BFT/manifest/service/W权限。

<a id="pos-case-phase-guards"></a>
#### steady phase gate、next_slot条件及全部pending/replication/participation/proposer/signature/execution/finality guard

产品层 N/A；reason: 本条细化runtime内部phase与next_slot检查和既有guard组合，不独立定义产品提交权或新增协议准入；scope: 仅steady phase、next_slot及原pending/replication/participation/proposer/signature/execution/finality guard执行；root DWE/DCS与服务闸门仍全部适用，不放宽任何guard或验收条件。仅免独立产品叶分配，不免本条专业验收。
本分项按 [时间状态](#pos-timing-state) 与 [持久只读](#pos-persistence-readonly) 的输入、guards、失败/恢复边界执行；需要同config/now/genesis/snapshot比较，不授予BFT/manifest/service/W权限。

<a id="pos-case-edge-nonsticky"></a>
#### strict next_slot<current本tick恢复edge，guard blocked即丢弃，next tick无继承

产品层 N/A；reason: 本条规定runtime内部tick-local strict-lag一次性边沿的生命周期，不独立创建产品补块或重试权；scope: 仅strict next_slot<current本tickedge、blocked丢弃及next tick无继承；全部既有guard、root DWE/DCS、恢复与服务约束仍适用。仅免独立产品叶分配，不免本条专业验收。
本分项按 [时间状态](#pos-timing-state) 与 [持久只读](#pos-persistence-readonly) 的输入、guards、失败/恢复边界执行；需要同config/now/genesis/snapshot比较，不授予BFT/manifest/service/W权限。

<a id="pos-case-admission-window"></a>
#### future/stale与target epoch不匹配拒绝，不借jitter放宽

产品层 N/A；reason: 本条执行既有future/stale/target epoch负向输入校验，不独立定义validator资格或产品服务等级；scope: 仅原future/stale窗口与target epoch不匹配拒绝，不能借jitter放宽；root DWE/DCS、共识资格、恢复/服务和完整guard仍适用。仅免独立产品叶分配，不免本条专业验收。
本分项按 [时间状态](#pos-timing-state) 与 [持久只读](#pos-persistence-readonly) 的输入、guards、失败/恢复边界执行；需要同config/now/genesis/snapshot比较，不授予BFT/manifest/service/W权限。

<a id="pos-case-readonly-status"></a>
#### status immutable timing/counters，只读不推进slot/续租/制造evidence；poll≠slot

产品层 N/A；reason: 本条约束runtime immutable诊断读取的无副作用实现，不独立给出产品服务可用性或世界进度承诺；scope: 仅原timing/counters读取不推进slot、不续租、不制造evidence及poll不等于slot；root DWE/DCS、恢复服务闸门与业务权限边界仍适用。仅免独立产品叶分配，不免本条专业验收。
本分项按 [时间状态](#pos-timing-state) 与 [持久只读](#pos-persistence-readonly) 的输入、guards、失败/恢复边界执行；需要同config/now/genesis/snapshot比较，不授予BFT/manifest/service/W权限。

<a id="pos-case-restart-monotonic"></a>
#### persistedconfig/counters一致，restart不倒退重复计；inconsistentfail closed保留诊断
本分项按 [时间状态](#pos-timing-state) 与 [持久只读](#pos-persistence-readonly) 的输入、guards、失败/恢复边界执行；需要同config/now/genesis/snapshot比较，不授予BFT/manifest/service/W权限。

<a id="pos-case-replay-recorded"></a>
#### replay只按snapshot/events/log，不当前wall clock补proposal/block/event
本分项按 [时间状态](#pos-timing-state) 与 [持久只读](#pos-persistence-readonly) 的输入、guards、失败/恢复边界执行；需要同config/now/genesis/snapshot比较，不授予BFT/manifest/service/W权限。

### 2.1 需求承接与分配表

限定产品来源沿配对 PRD“专业接受与证据边界”及既有根 receiver 承接，不把以下局部关系扩成八行的统一产品验收：

- Trace refs: O7-REQ-cf672e18-ed16-42a3-9338-ab7b965ac607；[product_requirement: REQ-DWE-001](../../product/world-infrastructure/deterministic-world-execution.prd.md#req-dwe-001) 经 [root deterministic receiver](../design.md#runtime-deterministic-design) 承接共享时序输入子义务，仅定位 [pos-accept-shared-formula](chain-pos-control-plane.prd.md#pos-accept-shared-formula) → [pos-case-shared-formula](#pos-case-shared-formula)。不代签全部验证者、实例隔离或原子拒绝组合。
- Trace refs: O7-REQ-3f7d612d-5a6a-4d6d-bb0e-722c44d986f4；[product_requirement: REQ-DCS-002](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-002) 经 [root recovery receiver](../design.md#runtime-recovery-design) 承接 counters/config 连续性子义务，仅定位 restart-monotonic/replay-recorded 两组专业接受与设计分项。非空事件后缀正向续跑、真实故障域、重新 serving 与原 `test_tier_full` 组合仍由 runtime/P2P/ops/QA 共同判定。

上述引用不改变产品的部分覆盖/部分实现/未核实状态，不新增执行证据；其余行保留准确专业关系，未建立准确产品关系的范围仍待相应 owner 核对。

输入身份 eng-cc/oasis7@f9d5a552d9af04c1b1398262808198a58e560230；新增 local anchors 是本文技术接受关系，非机器 schema。每行范围独立，外部未决保留。

| 上游 requirement / acceptance | 具体 obligation 与条件 | 本设计条款 | 外部 owner / dependency | 排除或未覆盖 |
| --- | --- | --- | --- | --- |
| [professional_acceptance: pos-accept-shared-formula](chain-pos-control-plane.prd.md#pos-accept-shared-formula) | 共享genesis与整数logical_tick/slot/phase公式跨节点同输入同结果，missed accounting不补历史 | [设计](#pos-case-shared-formula) | runtime timing；launcher同名参数；P2P guard；QA | 不关闭BFT/工业window/服务闸门 |
| [professional_acceptance: pos-accept-config-rejection](chain-pos-control-plane.prd.md#pos-accept-config-rejection) | duration/ticks正值、phase<tps，explicit非法CLI字段级拒绝；default config权威与透传 | [设计](#pos-case-config-rejection) | runtime timing；launcher同名参数；P2P guard；QA | 不关闭BFT/工业window/服务闸门 |
| [professional_acceptance: pos-accept-phase-guards](chain-pos-control-plane.prd.md#pos-accept-phase-guards) | steady phase gate、next_slot条件及全部pending/replication/participation/proposer/signature/execution/finality guard | [设计](#pos-case-phase-guards) | runtime timing；launcher同名参数；P2P guard；QA | 不关闭BFT/工业window/服务闸门 |
| [professional_acceptance: pos-accept-edge-nonsticky](chain-pos-control-plane.prd.md#pos-accept-edge-nonsticky) | strict next_slot<current本tick恢复edge，guard blocked即丢弃，next tick无继承 | [设计](#pos-case-edge-nonsticky) | runtime timing；launcher同名参数；P2P guard；QA | 不关闭BFT/工业window/服务闸门 |
| [professional_acceptance: pos-accept-admission-window](chain-pos-control-plane.prd.md#pos-accept-admission-window) | future/stale与target epoch不匹配拒绝，不借jitter放宽 | [设计](#pos-case-admission-window) | runtime timing；launcher同名参数；P2P guard；QA | 不关闭BFT/工业window/服务闸门 |
| [professional_acceptance: pos-accept-readonly-status](chain-pos-control-plane.prd.md#pos-accept-readonly-status) | status immutable timing/counters，只读不推进slot/续租/制造evidence；poll≠slot | [设计](#pos-case-readonly-status) | runtime timing；launcher同名参数；P2P guard；QA | 不关闭BFT/工业window/服务闸门 |
| [professional_acceptance: pos-accept-restart-monotonic](chain-pos-control-plane.prd.md#pos-accept-restart-monotonic) | persistedconfig/counters一致，restart不倒退重复计；inconsistentfail closed保留诊断 | [设计](#pos-case-restart-monotonic) | runtime timing；launcher同名参数；P2P guard；QA | 不关闭BFT/工业window/服务闸门 |
| [professional_acceptance: pos-accept-replay-recorded](chain-pos-control-plane.prd.md#pos-accept-replay-recorded) | replay只按snapshot/events/log，不当前wall clock补proposal/block/event | [设计](#pos-case-replay-recorded) | runtime timing；launcher同名参数；P2P guard；QA | 不关闭BFT/工业window/服务闸门 |

### 11.1 验证映射表

局部 source 定义范围：[chain_pos_defaults.rs](../../../crates/oasis7/src/chain_pos_defaults.rs) 的 `parse_chain_pos_timing_defaults` 检查 duration/ticks 非零及 phase<ticks；inline tests `repo_chain_pos_defaults_parse_to_current_profile` 与 `repo_chain_pos_defaults_reject_invalid_phase` 分别核对当前配置和 phase==ticks 拒绝。默认数值仍由 `config/chain-pos-defaults.env` 提供，不在本设计冻结；这些定义未运行，不证明跨节点共享公式或全部 guard。下表 config-rejection 行另以真实 CLI phase 拒绝测试作为局部验证 source，未覆盖的零 duration/ticks CLI、透传及组合目标继续保留。

所有下列行为验证在本次文档编辑中未运行。定义/计划与当前实现、实际执行、发布分别成立；执行须另固定 source/integration/tested tree、config/world/entry/environment/window、exit/result/artifacts，并回 PR 与实际验证记录。target场景尚无完整runner时明确保持待证明，现有test/manual只是有界接收入口，不能伪称已实现或通过。

| 上游 requirement / acceptance | 本设计条款 | 独立 obligation / 条件 | 验证 source / ID、层级及候选环境 | 证据目标 | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [professional_acceptance: pos-accept-shared-formula](chain-pos-control-plane.prd.md#pos-accept-shared-formula) | [设计](#pos-case-shared-formula) | 共享genesis与整数logical_tick/slot/phase公式跨节点同输入同结果，missed accounting不补历史 | [现行 manual](../../../testing-manual.md)；精确局部 source ../../../crates/oasis7/src/chain_pos_defaults.rs（定义/入口，非执行证据）；required定义入口 chain_pos_defaults / oasis7_chain_runtime_tests；target 共享genesis与整数logical_tick/slot/phase公式跨节点同输入同结果，missed accounting不补历史，固定same now/genesis/config/snapshot与guard组合，复核计数/零历史合成；restart跨边界由full同候选联验 | 未运行；未来同候选 log/root/receipt/metrics或consumer artifact入task evidence，QA判定 | 不关闭BFT/工业window/服务闸门 |
| [professional_acceptance: pos-accept-config-rejection](chain-pos-control-plane.prd.md#pos-accept-config-rejection) | [设计](#pos-case-config-rejection) | duration/ticks正值、phase<tps，explicit非法CLI字段级拒绝；default config权威与透传 | [CLI phase 局部测试定义](../../../crates/oasis7_chain_runtime/src/host/oasis7_chain_runtime_tests.rs)；`parse_options_rejects_proposal_tick_phase_out_of_range` 输入 `--pos-ticks-per-slot=4`、phase=4，断言拒绝且错误含 `--pos-proposal-tick-phase`；上文 defaults parser/inline tests 另说明配置校验。定义未运行，不证明零 duration/ticks CLI、跨节点共享公式或全部 guard；required定义入口 chain_pos_defaults / oasis7_chain_runtime_tests；target duration/ticks正值、phase<tps，explicit非法CLI字段级拒绝；default config权威与透传，固定same now/genesis/config/snapshot与guard组合，复核计数/零历史合成；restart跨边界由full同候选联验 | 未运行；未来同候选 log/root/receipt/metrics或consumer artifact入task evidence，QA判定 | 不关闭BFT/工业window/服务闸门 |
| [professional_acceptance: pos-accept-phase-guards](chain-pos-control-plane.prd.md#pos-accept-phase-guards) | [设计](#pos-case-phase-guards) | steady phase gate、next_slot条件及全部pending/replication/participation/proposer/signature/execution/finality guard | [现行 manual](../../../testing-manual.md)；精确局部 source ../../../crates/oasis7_chain_runtime/src/host/oasis7_chain_runtime_tests.rs（定义/入口，非执行证据）；required定义入口 chain_pos_defaults / oasis7_chain_runtime_tests；target steady phase gate、next_slot条件及全部pending/replication/participation/proposer/signature/execution/finality guard，固定same now/genesis/config/snapshot与guard组合，复核计数/零历史合成；restart跨边界由full同候选联验 | 未运行；未来同候选 log/root/receipt/metrics或consumer artifact入task evidence，QA判定 | 不关闭BFT/工业window/服务闸门 |
| [professional_acceptance: pos-accept-edge-nonsticky](chain-pos-control-plane.prd.md#pos-accept-edge-nonsticky) | [设计](#pos-case-edge-nonsticky) | strict next_slot<current本tick恢复edge，guard blocked即丢弃，next tick无继承 | [现行 manual](../../../testing-manual.md)；精确局部 source ../../../crates/oasis7_chain_runtime/src/host/oasis7_chain_runtime_tests.rs（定义/入口，非执行证据）；required定义入口 chain_pos_defaults / oasis7_chain_runtime_tests；target strict next_slot<current本tick恢复edge，guard blocked即丢弃，next tick无继承，固定same now/genesis/config/snapshot与guard组合，复核计数/零历史合成；restart跨边界由full同候选联验 | 未运行；未来同候选 log/root/receipt/metrics或consumer artifact入task evidence，QA判定 | 不关闭BFT/工业window/服务闸门 |
| [professional_acceptance: pos-accept-admission-window](chain-pos-control-plane.prd.md#pos-accept-admission-window) | [设计](#pos-case-admission-window) | future/stale与target epoch不匹配拒绝，不借jitter放宽 | [现行 manual](../../../testing-manual.md)；精确局部 source ../../../crates/oasis7_chain_runtime/src/host/oasis7_chain_runtime_tests.rs（定义/入口，非执行证据）；required定义入口 chain_pos_defaults / oasis7_chain_runtime_tests；target future/stale与target epoch不匹配拒绝，不借jitter放宽，固定same now/genesis/config/snapshot与guard组合，复核计数/零历史合成；restart跨边界由full同候选联验 | 未运行；未来同候选 log/root/receipt/metrics或consumer artifact入task evidence，QA判定 | 不关闭BFT/工业window/服务闸门 |
| [professional_acceptance: pos-accept-readonly-status](chain-pos-control-plane.prd.md#pos-accept-readonly-status) | [设计](#pos-case-readonly-status) | status immutable timing/counters，只读不推进slot/续租/制造evidence；poll≠slot | [现行 manual](../../../testing-manual.md)；精确局部 source ../../../crates/oasis7_chain_runtime/src/host/oasis7_chain_runtime_tests.rs（定义/入口，非执行证据）；required定义入口 chain_pos_defaults / oasis7_chain_runtime_tests；target status immutable timing/counters，只读不推进slot/续租/制造evidence；poll≠slot，固定same now/genesis/config/snapshot与guard组合，复核计数/零历史合成；restart跨边界由full同候选联验 | 未运行；未来同候选 log/root/receipt/metrics或consumer artifact入task evidence，QA判定 | 不关闭BFT/工业window/服务闸门 |
| [professional_acceptance: pos-accept-restart-monotonic](chain-pos-control-plane.prd.md#pos-accept-restart-monotonic) | [设计](#pos-case-restart-monotonic) | persistedconfig/counters一致，restart不倒退重复计；inconsistentfail closed保留诊断 | [现行 manual](../../../testing-manual.md)；精确局部 source ../../../crates/oasis7_chain_runtime/src/host/oasis7_chain_runtime_tests.rs（定义/入口，非执行证据）；required定义入口 chain_pos_defaults / oasis7_chain_runtime_tests；target persistedconfig/counters一致，restart不倒退重复计；inconsistentfail closed保留诊断，固定same now/genesis/config/snapshot与guard组合，复核计数/零历史合成；restart跨边界由full同候选联验 | 未运行；未来同候选 log/root/receipt/metrics或consumer artifact入task evidence，QA判定 | 不关闭BFT/工业window/服务闸门 |
| [professional_acceptance: pos-accept-replay-recorded](chain-pos-control-plane.prd.md#pos-accept-replay-recorded) | [设计](#pos-case-replay-recorded) | replay只按snapshot/events/log，不当前wall clock补proposal/block/event | [现行 manual](../../../testing-manual.md)；精确局部 source ../../../crates/oasis7_chain_runtime/src/host/oasis7_chain_runtime_tests.rs（定义/入口，非执行证据）；required定义入口 chain_pos_defaults / oasis7_chain_runtime_tests；target replay只按snapshot/events/log，不当前wall clock补proposal/block/event，固定same now/genesis/config/snapshot与guard组合，复核计数/零历史合成；restart跨边界由full同候选联验 | 未运行；未来同候选 log/root/receipt/metrics或consumer artifact入task evidence，QA判定 | 不关闭BFT/工业window/服务闸门 |
