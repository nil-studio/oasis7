# playability_test_result 模块设计总览

审计轮次: 6

- 对应需求文档: `doc/playability_test_result/prd.md`
- 可变任务状态与历史: GitHub task issue evidence comments
- 对应文件级索引: `doc/playability_test_result/prd.index.md`

## 1. 设计定位
本模块的 `design.md` 说明可玩性测试结果的组织、记录、回流和归档方式。

## 2. 阅读顺序
1. `doc/playability_test_result/prd.md`
2. `doc/playability_test_result/design.md`
3. `doc/playability_test_result/prd.index.md`
5. 下钻卡片、手册与专题结果文档

## 3. 设计结构
- 结果层：测试卡、记录卡与专题结果的落位方式。
- 回流层：结果如何回流到 game / testing / world-simulator。
- 证据层：记录如何作为发布与整改证据使用。

## 4. 集成点
- `doc/game/prd.md`
- `doc/testing/prd.md`
- `testing-manual.md`

## 5. 专题导航
- 结果卡与手册保留在本模块下
- 后续若出现结构化测试方案，应补专题 `*.design.md`

## 设计目标
- 提供 `playability_test_result` 模块的总体设计入口。

## 设计范围
- 覆盖模块级结构、主链路、分层与专题导航。
- 不替代专题 `*.design.md` 的细化设计。

## 关键接口 / 入口
- 需求入口：`doc/playability_test_result/prd.md`
- 可变执行状态：GitHub task issue evidence comments
- 索引入口：`doc/playability_test_result/prd.index.md`

## 设计演进计划
- M1 (2026-03-09): 在 ROUND-006 中补齐模块级 `design.md` 标准入口。
- M2: 按专题继续补齐高复杂度主题的 `*.design.md`。

## 设计风险
- 若专题级设计未及时补齐，模块级 `design.md` 可能承载过多导航职责。
- 若 legacy redirect 未明确标注为兼容跳转，读者可能误判历史入口为当前执行入口。


<a id="feedback-score-calculation-design"></a>
## 评分计算与回流

依据[问卷评分需求](prd.md#feedback-score-product-contract)，计算按“保留原答案与来源 → 校验题目回答及评审归属 → 依唯一映射换算 → 检查维度完整性 → 未舍入聚合/判档 → 应用附加规则 → 展示/归档”执行。

公式与常量只在[卡片评分合同](playability_test_card.md#feedback-score-contract)维护，输出同时保留原始分数和显示分数，避免显示舍入影响标签。这里定义实现/人工复核的共同算法，不声称已有脚本自动完成。

缺答、非法单选多勾或未评审会阻断相应计算，不补值、不重分配权重；运行未入局与证据缺失继续按 RunResult 表达。

汇总器先固定当前候选及来源分组，再选择该组的完整已评卡片；排除理由和数量必须可见，旧卡、新卡及 AI/真人来源不暗中混合。保存新评审而保留旧答案/旧结论，不能用重算覆盖历史事实。数值建议、owner 结论、玩家杠杆和发布证据保持可分别审阅。

<a id="feedback-score-evidence-boundary"></a>
### 评分证据隔离

Trace refs: O7-REQ-1a903b9b-03df-4ae2-86f2-2a767affc9c1；承接[评分证据层级](prd.md#feedback-score-evidence-level)的局部负向边界。计算结果保留当前尝试、原始答案、来源和 review_status 的关联；未评审、未执行、缺答和非法回答保持未知，已有独立阻断不因未知总分消失。数值建议、owner 裁决、玩家杠杆、L4/L5 及发布判断分别审阅；高分不能把 AI 来源包装为真实人类意愿，不能把数值“继续可玩”转为产品 go 或发布放行。实际 RunResult、试玩和所有下游 claim 消费者仍由各自 authority 负责，本节不证明它们已实现隔离。

<a id="feedback-score-comparison-boundary"></a>
### 评分比较边界

Trace refs: O7-REQ-bacdddac-0615-4447-9242-0145c1cc4e07；承接[评分汇总与产品比较单位](prd.md#feedback-score-comparison-scope)。沿卡片唯一合同保留组键、原始卡片值、有效/未知/排除数量与逐卡阻断；完整卡片的描述性均值不能填补缺卡或抵消其他组的阻断。向产品评审提供汇总时，明确同评分组不等于同产品比较单位：体验目标、formal 入口、候选/变更、窗口及条件化环境仍需原 owner 独立核对；范围不明不能从均值提升结论。此处不增设字段、阈值、自动聚合器或新的比较/发布规则。

### 2.1 需求承接与分配表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [评分复算合同](prd.md#feedback-score-product-contract) | 同答卷唯一复算，缺失不补值；五维与题目映射只引用唯一卡片合同 | [计算与回流](#feedback-score-calculation-design) | QA/玩法 owner；卡片评分合同 | 产品适用性：N/A；reason=此行只分配问卷数学复算，不定义玩家价值或发布验收；scope=题目映射、完整性、精度及数值建议，产品证据交集由以下两行独立承接 |
| Trace refs: O7-REQ-1a903b9b-03df-4ae2-86f2-2a767affc9c1；[评分证据层级](prd.md#feedback-score-evidence-level) | 已评来源与当前尝试保留，数值不能代签更高层证据或发布 | [评分证据隔离](#feedback-score-evidence-boundary) | QA/producer 与 L4/L5、发行 authority | 只覆盖评分输出解释，不证明实际试玩或全部 claim 消费者 |
| Trace refs: O7-REQ-bacdddac-0615-4447-9242-0145c1cc4e07；[比较单位边界](prd.md#feedback-score-comparison-scope) | 同评分组均值不等于同产品比较单位，逐卡独立阻断不被抵消 | [评分比较边界](#feedback-score-comparison-boundary) | QA/producer；产品比较单位 authority | 不实现聚合器，不替 owner 核定全部产品建议入口 |

### 11.1 验证映射表

以下是静态复算与文档审阅计划；实际输入、候选版本、结果和未覆盖项记在当前 PR，不冒充真实试玩。

| 上游 requirement / product AC / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source 或 ID、scenario/layer、candidate/environment | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [评分复算合同](prd.md#feedback-score-product-contract) | [计算与回流](#feedback-score-calculation-design) | 改规则或引用后离线复核唯一计算与附加规则 | 本候选的[卡片合同](playability_test_card.md#feedback-score-contract)：逐步复算原反例、四档、Q14、Q7、缺答、3.2/4.2邻近值、附加规则最严格优先；用精确分数，不启动游戏 | 同修订合同、静态输入与未舍入结果/建议标签 | 不证明已部署评分器、历史卡通过或发布准备度 |
| Trace refs: O7-REQ-1a903b9b-03df-4ae2-86f2-2a767affc9c1；[评分证据层级](prd.md#feedback-score-evidence-level) | [评分证据隔离](#feedback-score-evidence-boundary) | 高分、未评审、低层来源与独立阻断分别保留 | 本候选[卡片合同](playability_test_card.md#feedback-score-contract)及本地条款静态负例审阅：AI高分无L5、未评审满分、unknown但已有block；核对来源/尝试、未知与独立裁决，不生成强claim | 当前 PR 的反例与逐项解释 | 不证明 runtime RunResult 或全部 claim 消费者执行拒绝 |
| Trace refs: O7-REQ-bacdddac-0615-4447-9242-0145c1cc4e07；[比较单位边界](prd.md#feedback-score-comparison-scope) | [评分比较边界](#feedback-score-comparison-boundary) | 评分描述均值不补缺证、不跨单位抵消阻断 | 按本候选[卡片合同](playability_test_card.md#feedback-score-contract)静态对照同评分组但不同证据窗口/条件化环境，以及不同候选/来源卡片；核对均值仅描述、排除数及逐卡阻断保留、产品比较条件另审 | 当前 PR 的分组输入与边界审阅记录 | 不证明全部 go/watch/hold/block 消费者的实际执行 |
