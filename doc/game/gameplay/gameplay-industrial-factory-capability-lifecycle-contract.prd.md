# 工厂能力生命周期合同

- 上层产品映射：承接产品 SC-25 对已建工厂 capability 的升级、重配置、继续运行、降载/延期、退役重建与延期选择；本合同只把既有产品承诺转为玩家可比较的工业经营循环。
- 产品语义入口：[`工业流水线生命周期到玩家决策的收口`](../../product/world-rules-core-gameplay/prd.md#工业流水线生命周期到玩家决策的收口)；本合同保留工厂 capability 的选择、节奏与恢复权威。
- Authority 分工：产品层拥有 capability profile、作用域与完成承诺；M4/domain 拥有 factory/recipe fit、物流/容量/终端事实及旧工作处置 profile；world-runtime 拥有状态、cutover、事件、持久化与 replay；gameplay 拥有玩家动作、节奏、收益、机会成本、失败恢复与 progression 表达。
- 边界：动作标签是玩家选择，不是新增 runtime transition。工厂 capability lifecycle 与 recipe lifecycle、site selection、batch/externality、maintenance 数值和 systemic governance 分离；本合同不得把扩容、重配置或退役写成统一迁移能力。

## 1. Candidate、profile 与状态边界

只有当前工厂 capability candidate/profile 明确支持某个动作及其作用域，并说明成本/停机、受影响的 recipe/目标和旧工作 disposition 时，surface 才能展示该动作。每次预览绑定 factory/capability、recipe-fit、owner、scope、authority/config revision 与 next recheck；profile 缺失、过期、冲突或未追踪时显示 `unknown/not_tracked`，涉及不可逆 sink 的候选必须停止在预览、重新报价或原子拒绝。

玩家可读状态可表达 `operational`、`reconfiguration_pending`、`reconfiguring`、`degraded`、`retiring`、`retired`；这些是产品/玩法表达，不冻结 runtime enum。`degraded` 只有 profile 明确允许并标记风险时才可接受新工作；`retiring` 与 `retired` 不接受新排程，保留历史，不因退出本身发放稳定进度或里程碑。

## 2. 六类玩家选择

| profile-supported action | 玩家比较与即时收益 | 失败成本与下一动力 |
| --- | --- | --- |
| `upgrade_existing_capability` | 提升既有工厂可服务的 capability 范围、吞吐或路线；收益是打开 profile 声明的新 recipe/服务选择 | 承担追加资源、维护、停机与共享容量机会成本；完成后读取新 candidate/下一目标，未支持或资料不足时不展示 |
| `reconfigure_recipe_fit` | 把工厂切到 profile 声明的另一 recipe-fit；收益是以既有 footprint 进入不同工业目标 | 承担切换时间、输入/物流/终端重新适配与旧工作处置成本；因果切换后从新 candidate 的 `W=0` 继续 |
| `run_at_current_capability` | 保持当前 capability 继续合法生产/交付；收益是保住当前目标、窗口与已投入价值 | 放弃本次升级机会并承受旧吞吐/容量限制；下一动力是完成当前目标或等待更好时机 |
| `reduce_or_defer` | 降载或延期以保留电力、输入、容量和恢复弹性；收益是避免高风险 cutover 或停机 | 承担吞吐、交付窗口和目标进度延迟；到 profile 声明的复查点再比较新报价 |
| `retire_and_rebuild` | 在 profile 支持时结束旧 capability 并建立 successor；收益是释放旧限制、进入新的合法能力/服务路线 | 承担旧任务 hold/处置、重建成本与失去旧连续性的风险；successor 必须显式 fresh validation 后成为下一目标 |
| `defer` | 暂不改变 capability，保留当前信息与目标选择 | 不产生隐藏 hold、队列、容量或奖励，只保留已披露的等待/机会成本；下一步是 authority 恢复、补资源或重新报价 |

未被 profile 支持的动作不展示，也不得用 `upgrade_existing_line`、Agent 推荐、旧 receipt 或通用 `repair/rebuild/pivot` 静默替代。玩家在确认前必须能比较目标/作用域、追加资源与维护成本、停机/容量机会成本、吞吐/物流/终端影响、受影响旧工作、风险、可撤回性与 next recheck。

## 3. Preview、提交与重新校验

`factory_capability_lifecycle_preview` 只读、确定且不产生 hold、队列、输入 sink、退款、能力效果、W 或奖励。它至少引用当前 factory/capability/recipe-fit candidate，列出实际支持的六类动作、上述成本/影响、旧工作 bucket、primary blocker、保留/损失价值与下一复查点；`defer` 不得包装成隐式 reservation。

确认某动作后，提交必须 fresh revalidate owner、factory capability/recipe fit、recipe/spec、power、logistics、capacity、terminal 与 governance authority。任何事实漂移只能按专业 profile fresh requote、保持有界 pending 或 atomic reject；不得静默降级、换 recipe、换站点、迁移任务、免费退款、自动补偿或以陈旧 preview 形成成功。`unknown/not_tracked` 不得按零成本、安全或兼容处理。

## 4. Causal change、cutover 与 successor

是否改变产出因果由 factory/capability/recipe profile 声明：改变 capability、recipe-fit、输入/输出/副产物、power/logistics/terminal 前置等因果内容时，在一个 canonical cutover snapshot 创建 parent-linked successor candidate，新 candidate 从 `W=0` 开始；只改变展示或非因果 metadata 时保持原 identity。无法证明为非因果变更时 fail closed，不由 gameplay 推断。

Cutover 前后的 factory、candidate、recipe、work、reservation、receipt、稳定窗口与 progression 必须分离。Successor 不能自动继承旧任务、旧 W、队列资格、reservation、receipt 或奖励；只有明确授权、fresh revalidation 与 profile 支持的 parent-linked handoff/conversion 才能进入 successor。没有合法 successor 或 disposition 时返回 `no_safe_fallback`/`unknown`，不得伪造恢复。

## 5. 既有工作逐项处置

| 旧工作 bucket | 可比较的处置 | 边界 |
| --- | --- | --- |
| `accepted-unstarted` | profile 支持的 `hold`、`release`、`replan`、`terminate` 或明确 successor handoff | 未开始工作不自动取得新 capability、优先级或 W |
| `WIP` | profile 支持的 `finish`、`pause`、`hold`、`rework`、`salvage`、`terminate` | 已消费投入、在制状态与旧 lineage 保留；不默认退款或跨 candidate 继续 |
| `in-transit` | profile 支持的 `finish`、`hold`、`return`、`reroute`、`reject` | 保留原 edge、目的地、receipt、损耗与 owner；不得瞬移或静默改道 |
| `buffer` | profile 支持的 `hold`、`handoff`、`conversion`、`reject` | 保留旧 ledger/lineage；新 candidate 只能经显式适用性与容量复验消费 |
| `terminal-pending` | 继续原合法 terminal commitment、等待/取得准入，或 profile 支持的 `hold`、`return`、`reroute`、`handoff`、`reject` | 保留 recipient、destination、admission、capacity obligation 与 provenance；successor/cutover 不得提前 settlement 或静默释放/迁移容量义务，只能由一次显式 disposition 改变 |

每个 bucket 在同一 root/revision/cutover 下至多产生一个 disposition receipt；没有 profile 支持的动作不展示。`retiring` 只允许既有工作按声明规则排空/处置，`retired` 只保留历史/回放与最终处置；二者都禁止新排程。不得同时产生退款与完成、销毁与成功、旧任务迁移与旧身份完成。

## 6. Exactly-once、replay 与跨 surface parity

- 同一 factory/capability/recipe/root/revision 与 authority snapshot 的 preview 不产生世界效果；一次确认至多产生一次 cutover、一次旧工作 disposition、一次 successor handoff/conversion、一次生产/交付关联结果与一次 progression/reward。
- 重复提交、断线重连、Agent retry、snapshot restore、事件乱序与 replay 只能重读原 decision/receipt；不得重复释放 hold、复制产出/奖励、复活 retired capability、刷新旧顺位或重新迁移工作。
- Viewer、pure API 与 Agent 必须同义表达 capability 状态、动作支持性、primary blocker、追加/已占用/已损失价值、旧工作处置、旧/新 identity、W 影响、可撤回性与 next_action/next_recheck；不得把 accepted、pending、cutover 或 production receipt 冒充完成。

## 7. Current/target evidence cutline 与验收

当前证据仅支持既有 Build/Maintain/Recycle/Pause/Schedule 窄路径；没有完整对应 SC-25 的 upgrade/reconfigure capability action、组合 lifecycle surface 或 composite receipt。因此本合同是 `target-contract`，不能宣称当前 capability lifecycle 已实现。

`test_tier_required` 至少覆盖：

- 两种 profile-supported capability 选择；
- 六类动作的只读 preview 与 unsupported 隐藏；
- 成本/停机/容量/物流/终端/旧工作影响；
- fresh revalidation 与 drift requote/atomic reject；
- causal 与 non-causal 变更；
- 单一 parent-linked cutover 与 successor `W=0`；
- `retiring/retired` 禁止新排程；
- 五类旧工作各一次 disposition，其中 terminal-pending 保留 recipient/destination/admission/capacity obligation/provenance 且不提前结算或静默改绑；
- successor 不自动迁移；
- 重复 submit/reconnect/retry/restore/replay 无重复效果；
- Viewer/pure API/Agent 对五类 bucket 保持 parity。

`test_tier_full` 延后至 M4/runtime/QA 提供多阶段升级、并发容量、部分失败、持久化恢复与 successor conversion 的 composite evidence 后执行；测试目标不是当前通过声明。

## 8. Non-goals 与 residual risk

本合同不定义升级/重配置/重建成本、吞吐/维护/停机/容量公式、统一迁移策略、自动迁移/补偿/退款、队列公平、runtime enum/schema/action/API、UI、站点选址、recipe lifecycle、batch/externality、前三个工业专题或当前 release claim。残余风险是 product capability profile、M4 disposition 与 runtime cutover/receipt 仍需共同落地；在 authority 缺失、profile 不明或无法安全处置时保持 `unknown/not_tracked`/`no_safe_fallback`，并由 producer、M4/runtime、QA 与 Viewer/Agent parity 复核后才可转为 current evidence。
