# 工业原材料适用性与替代决策合同

- 状态：`target-contract`；当前只有 Product/M4 静态权威与窄 `RecipePlan` / `ScheduleRecipe` / `RecipeScheduled` 证据，没有原料替代的 runtime、Viewer、pure API 与 Agent 组合证据。
- 权威：Product SC-30 拥有同料换源、异料替代、换配方及因果边界；M4/runtime 拥有 quality/custody、batch applicability、join/split/mix/merge、数量守恒与执行 receipt；本文只拥有玩家比较、机会成本、恢复和验收投影。
- 产品语义入口：[`原料替代、替代来源与换配方决策`](../../product/world-rules-core-gameplay/prd.md#原料替代、替代来源与换配方决策（补齐输入阻塞后的合法替代边界）)；本合同保留原料适用性、替代与恢复的玩法权威。
- 边界：异料替代默认禁止。Gameplay 不定义材料 taxonomy、质量等级、替代比例、rounding/residual、产率、产出/value class、terminal 资格或经济公式，也不从同名、`degraded`、客户端缓存或 Agent 推荐推断合法替代。

## 1. 玩家问题与成功收益

来源结算或物流抵达只证明批次存在，不证明其仍适用于当前 recipe。Required input 缺失、数量不足、质量/保管不适用或权威未决时，玩家需要区分可选路径：等待标准料、同料换源、接受 profile 合法异料、换配方、减量/延期或 pivot，而不能只看到“材料不足”。

成功收益只有两类：保留当前 recipe 并解除合法 input blocker，或以已披露的额外投入、损耗和因果变化建立一个可执行的新 candidate。Preview、替代 decision、batch 到达或 join-ready 都不是生产、交付、terminal settlement、稳定 `W` 或奖励；这些仍须各自的权威 receipt。

## 2. 只读决策预览

在 batch 进入 join、conversion 或首个不可逆 input sink 前，玩家必须能读取只读 `raw_material_substitution_preview`。预览至少绑定 root/candidate/revision、pinned recipe、required input、候选 batch/source/material、owner/provenance/quality/custody/applicability authority、数量与权威 ratio/rounding/residual 结果、额外损耗/power/time/logistics/terminal 占用、已持有或已消费价值、预期产出/value class 是否改变、主要风险、`W` 影响、`next_recheck` 与 `recommended_reason`。

Preview 不锁来源或容量，不创建 hold/join/conversion，不扣输入，不改变 recipe/candidate，不产生 receipt、产出、`W`、progression 或 reward。Authority 缺失、过期、冲突或 profile 未声明异料替代时显示 `unknown/blocked` 或 `no_legal_substitute`；不得以零比例、免费 replacement、自动降级或通用“兼容”标签补齐未知事实。

## 3. 真实选择与机会成本

只展示专业 profile 当前真实支持的选择：

| 选择 | 即时收益 | 必须披露的成本与限制 |
|---|---|---|
| `wait_or_replenish_standard_input` | 保留标准 recipe 与预期产出因果 | 等待、库存/buffer/hold、交付延迟与下一复查条件；无有界触发时不能推荐无限等待 |
| `switch_same_material_source` | 在材料身份与 recipe 因果不变时补足 input | 新 source 的 owner、provenance、数量、质量、物流、损耗和竞争重新验证；旧预留或顺位不迁移 |
| `use_profile_legal_substitute` | 用 profile 明示的异料解除 blocker | 权威比例、rounding/residual、额外投入/损耗/power/time，以及产出、quality、byproduct、terminal/value class 是否变化 |
| `switch_to_legal_recipe` | 在当前材料更适用时建立另一条生产路径 | 新 recipe/factory/path/output/terminal 全量重验，形成 parent-linked candidate 且 `W=0` |
| `reduce_or_defer` | 减少当前承诺或保留未来选择 | 吞吐、交付、hold 与目标延迟；减量仍服从 canonical cycle/quantization，不静默裁剪 input |
| `pivot` | 将可保留投入转向另一个合法用途 | 原目标进度、专用投入、重新配置与新用途资格的实际保留/损失 |

不展示不受支持的混批、降级用途、免费换料、自动 replacement 或自动换配方。推荐只能说明哪项真实选择更能保护当前目标、已投入价值或恢复弹性；没有安全选择时返回 `no_legal_substitute` 与下一决策/复查条件。

## 4. Decision、conversion 与因果隔离

玩家确认时必须从 fresh authority 重验 owner/provenance/quality/custody、数量、ratio/rounding/residual、recipe/factory、path/capacity、power、output/value class 与 terminal。漂移只能触发重新报价、首个 sink 前无副作用原子拒绝，或 profile 明示的有界 pending。

Decision 本身只记录一次选择，不产生 sink、credit、conversion、`W` 或奖励。只有专业 authority 实际结算 conversion 时，才产生一个与 decision/root 关联的 exactly-once settlement/provenance receipt；重复 decision、拆分提交或切换候选不能制造材料、舍入余量、refund、顺位或奖励。

只有 profile 明示且材料身份变化后，产出因果、质量、power、时间、byproduct、terminal 与 value class 全部不变，才可保留 pinned recipe 并建立一个 child decision。任一因果项变化或玩家换配方时，必须建立 parent-linked 新 candidate、从 `W=0` 开始；旧 receipt、reservation、queue priority、actual、delivery、progression 与 reward 不迁移。

## 5. Sink 前后与失败恢复

首个不可逆 sink 前，`blocked / expired / quarantine / unknown` 或 recipe 不接受的 `degraded` batch 不得进入 join；`degraded` 只有 profile 明示接受该等级时才可成为候选。到达顺序、同名材料或旧 preview 不能替代 fresh applicability。

首个 sink 后不得追溯重贴 batch 标签、改写 WIP/transit/buffer lineage 或把旧产出归到新 candidate。只允许专业合同支持的单次 hold、rework/conversion、return、salvage 或 termination；已发生的运输、保管、损耗、input sink 与 receipt 保持实际结果，不默认退款。

## 6. 验收、Parity 与非目标

`test_tier_required` 至少覆盖：同料换来源、合法异料替代、换配方与 `no_legal_substitute`；preview 无效果；decision-only 无 sink/conversion/`W`/奖励；实际 conversion 只有一个 linked receipt；ratio/rounding/residual 守恒；`degraded` 接受/拒绝成对样例；unknown/stale/owner/quality/route/power/terminal 漂移 fail closed；same-recipe 全因果不变与 causal candidate/`W=0` 分离；sink 后单次恢复；retry、reconnect、Agent retry、restore、乱序与 replay 不复制材料、conversion、receipt、产出、进度或奖励。`test_tier_full` 再覆盖多输入 join、并发替代批次争用、跨窗口 cutover、持久化恢复与反套利。

Viewer、pure API 与 Agent 必须同义表达候选类型、权威状态、数量/比例/残余、成本、保留/损失、产出/value/terminal 影响、因果分类、blocker、`W`、下一动作与复查点。Agent 只能在授权 scope 内建议或请求确认，不能证明合法替代、选择默认值或自动执行。

本文不新增或冻结材料 taxonomy、quality/custody 规则、比例/rounding/residual/产率/价格公式、batch/join/split/mix/merge 算法、recipe/factory/terminal lifecycle、runtime/API/ABI/schema/action、UI、自动替代/混批/迁移/退款/降级，也不声明当前实现或 release readiness。
