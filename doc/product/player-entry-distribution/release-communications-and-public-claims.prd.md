# 发行沟通与公开口径

## 文档身份

- 所属产品模块：玩家接入与发行
- 上位产品 PRD：[`prd.md`](prd.md)
- 生命周期：`active`
- Owner role：`producer_system_designer`
- 专业域权威：[`readme 产品与治理 PRD`](../../readme/prd.md)、[`testing PRD`](../../testing/prd.md)、[`Gameplay Agent 认领经济合同`](../../game/gameplay/gameplay-agent-claim-economy-contract.prd.md#public-claim-boundary)、[`Gameplay Agent 认领经济合同 design`](../../game/gameplay/gameplay-agent-claim-economy-contract.design.md#public-claim-boundary)、[`restricted grant LiveOps runbook`](../../game/gameplay/gameplay-agent-claim-restricted-grant-liveops-runbook-2026-03-29.md#public-claim-boundary)、[`README.md`](../../../README.md)
- 公开状态权威：[`README.md`](../../../README.md)
- 专业与执行入口：[`doc/readme/prd.md`](../../readme/prd.md)、[`doc/testing/prd.md`](../../testing/prd.md)
- Last reviewed：2026-09-13

本文是长期产品分册，定义内部候选证据如何形成可审查、可发布、可纠正和可撤回的公开说明。它不保存具体候选 verdict、公告正文、渠道排期、字段模板、任务状态或历史发布记录。

## 设计适用性与生命周期闭合

- 设计判定：`simple-topic-exemption`（`PRD-only-sufficient`）。
- 适用边界：仅豁免重复的产品体验配对 design；专业系统设计仍按各功能状态和准确承接独立判定，不因此变为“不适用”。
- 历史设计判定 Issue：#3680。
- 设计适用性理由：本 PRD 是公开 claim 的证据与审核生命周期合同；其 authority 在发行与 LiveOps 专业文档，独立 design 不增加玩家交互设计。
- 历史设计判定记录：历史分类见 [Issue #3680 C4 设计判定](https://github.com/eng-cc/oasis7/issues/3680#issuecomment-5652452993)，历史闭合要求见 [Issue #3680 accepted repair](https://github.com/eng-cc/oasis7/issues/3680#issuecomment-5652870280)。
## 1. 产品目标

玩家和社区应能明确区分内部候选状态、待审核文案和已经发布的公开事实。任何渠道说明都必须忠实反映同一候选、primary mode 与当前证据范围，并在事实变化时及时纠正，而不能把内部 `go`、QA green 或已批准草稿包装成已经发布或更高阶段。

## 2. 证据与公开口径边界

- 每份发行沟通必须绑定候选或版本、primary mode、适用入口、证据范围与更新时间；不同版本、模式或历史样本不得互相代签。
- 内部 readiness、go/no-go、QA 结论和产品决策只是公开沟通的输入，不会自动生成公开 claim。
- 对外内容按“已证实事实 → 适用范围与已知限制 → 未承诺内容与残余风险 → 下一步或恢复动作”组织；亮点不能覆盖限制。
- 根 `README.md` 是当前公开状态与 claim envelope 的唯一权威。站点、公告、changelog、帖子和回复不得独立升级阶段或扩大承诺。
- 涉及网络层级、安全成熟度、结算、创世、validator 或资产能力的表述，发布前必须同时复核根 README、对应专业权威与同一证据窗口；任一来源不支持时，公开口径采用更窄边界。
- 历史 allowlist/denylist、日期化 verdict、QA/task 完成记录或模板勾选只能用于追溯，不能沉淀为永久公开口径，也不能自动升级当前状态。
- 除非当前权威和证据明确支持，不得暗示 `mainnet`、`mainnet-grade`、production settlement/mint readiness、不可重置、公网 validator 准入或无条件可用性。
- 第三方账号存在、平台认证、原生 profile、帖子或回复都不证明产品集成、身份互通、合作关系、受支持入口或发行可用性；这些结论需要独立的当前产品决策、专业证据与根 README claim。
- 点赞、关注、回复、creator/builder 兴趣和合作线索只是待分流信号，不是 roadmap 承诺、readiness evidence 或批准。渠道回复不得承诺日期、范围、集成或恢复结果。

### 2.1 公共发现与双语事实一致性

- 公共发现页应让首次读者理解产品定位、受支持入口和已验证范围；亮点、架构能力、截图或技术细节不得遮蔽适用限制、未承诺内容和下一步。
- 同一公开事实的 CN/EN 配对 surface 必须表达语义等价的 claim envelope：相同 primary mode、证据窗口、适用限制和下一步。翻译可调整语言与阅读顺序，但不得单侧扩大状态、遗漏限制或把历史事实写成当前承诺。
- 静态 docs hub 与 Viewer 手册的 CN/EN 镜像是公共发现面的派生呈现，不是独立事实源。涉及手册操作、模式、限制或恢复的变更，必须先由对应专业 canonical source 决定；涉及当前公开状态或 claim 时，还必须符合根 `README.md` 的 claim envelope，再在相关镜像中成对同步。镜像与 canonical 不一致时应同时纠正、收窄或标记失效，不得以历史静态文案背书。
- 当当前事实、证据或限制漂移时，所有配对 surface 必须在同一纠正/收窄动作中更新；不能仅修正一侧，或继续以另一侧的旧文案背书。
- 玩家价值可以先于技术实现说明，但信息层级不构成当前状态、发行就绪或公开 claim 的独立证据；当前结论仍以根 `README.md` 和同一证据窗口为准。

<a id="public-claim-four-layer-model"></a>
### 2.0 四层口径分离

每次准备对外说明时，必须把下面四类内容分别记录；任何一类都不能替代另外三类：

| 层级 | 定义 | 发布限制 |
| --- | --- | --- |
| 当前事实（current fact） | 在明确版本、primary mode、入口和时间窗口内，已由根 `README.md`、对应专业 authority 与同一证据束共同支持的现状观察。 | 只能陈述证据实际覆盖的范围；不能从设计目标、局部 green、渠道存在或历史记录外推。 |
| 产品目标（product goal） | 产品希望玩家、运营者或系统最终获得的行为、体验或边界。 | 是需求和验收输入，不是当前已实现、已部署或已发行的事实。 |
| 假设（assumption） | 尚未由当前证据确认、但用于推进设计或运营判断的前提。 | 必须显式标记为假设；未转为当前事实前，不得进入可发布 claim。 |
| 证据边界（evidence limit） | 当前证据能够证明和不能证明的版本、模式、入口、时间、能力与风险范围。 | 缺失、过期、冲突或仅覆盖局部的证据会收窄 claim；无法证明的部分必须写成未承诺或 blocked。 |

只有当当前事实、适用范围和证据边界已经对齐，且产品审核、QA 证据复核和根 README claim envelope 均通过时，才可形成可发布公开 claim（releasable public claim）。可发布 claim 是一条面向外部读者的受范围约束的现状陈述，不得把产品目标或假设改写成当前事实，也不得借此暗示更高阶段、无条件可用性或未批准的后续计划。

## 2.2 阅读体验与发布决策

读者应先辨认已证实事实及其范围，再决定使用当前受支持入口、等待或按真实恢复路径行动。发布者在证据不足或相互冲突时选择收窄、保留草稿或撤回；只有同一窗口支持的事实才进入可发布状态，亮点与目标不抵消限制。

## 3. 沟通生命周期

- `draft`、`reviewed`、`publish_ready`、`published`、`withdrawn/stale` 是不同状态，不能互相代签。草稿和审核通过都不是发布证据。
- 正式发布必须记录 channel、发布时间、URL 或 message ID 与发布 owner；缺少这些信息时只能声称文案已准备，不能声称已发布。
- 内部状态、证据或风险变化时，受影响旧文案立即失效；所有仍活跃的公开 surface 必须纠正、降级或撤回，并保留变更依据。
- 纠正说明应明确旧表述已不再匹配当前状态，并把读者引回根 README 的当前公开状态；不得用历史 claim 文档继续背书。
- 降级说明只收窄已经证实的范围，不承诺恢复时间、后续集成、custody 结果或 roadmap 排期。
- 撤回只改变消息状态与 active surface，不等同于软件回滚、QA 放行、事故恢复或 operator 决策。
- claim rollback 只撤回或修正公开表述，不替代软件回滚、QA blocker、事故恢复或 operator 决策。

## 4. 角色与审核

- `liveops_community` 起草内容并执行渠道同步，确保语气、反馈入口和 surface 一致。
- `producer_system_designer` 审核产品承诺、适用范围、未承诺项与阶段边界。
- `qa_engineer` 复核证据是否支撑对应表述并给出 blocker；QA 不通过模板自行宣布产品发布。
- 公开口径变化必须能追踪上述输入、审核与发布记录。任何角色都不能绕过根 README 或当前证据单独扩大 claim。

## 5. 事故、回退与最小披露

- 事故或回退说明只陈述已确认影响、受影响版本/入口、当前恢复动作和下一次可信更新时间；未获权威确认时使用“正在调查”，不得承诺恢复时间或宣称风险清零。
- rollback communication 必须来自权威 rollback/incident 决策；LiveOps 不独立决定技术回滚、发布时间或恢复承诺。
- 公开内容只披露理解影响和采取行动所需的最小信息，不暴露内部运行目录、命令、敏感配置、私密 issue/comment、账号标识、未脱敏事故报告或不必要的个人复现数据。
- 内部 evidence link 与可公开 source link 必须区分；内部审核路径不得原样外发。

## 5.1 叶级产品要求与验收

<a id="req-entry-claim-001"></a>
### REQ-ENTRY-CLAIM-001：公开 claim 必须绑定同一证据窗口

Trace ID: O7-REQ-e20ed8c7-6f8d-400a-a883-57b2e772acff

- 要求：每条发行沟通必须绑定同一候选或版本、primary mode、适用入口、证据范围、更新时间和根 README claim envelope；内部 readiness、QA green、草稿或历史 verdict 不得单独生成公开事实。
- 验收：AC-ENTRY-CLAIM-001


- 功能点：[REQ-ENTRY-CLAIM-001](#req-entry-claim-001)；验收 [AC-ENTRY-CLAIM-001](#ac-entry-claim-001)。
- 适用范围：所有拟发布口径的同候选、模式、入口、证据窗口及 README 对账；沿用本条原有阶段和前置条件。
- 系统设计状态：部分覆盖
- 系统设计依据：[官网专业设计](../../site/github-pages/public-site-discovery-and-showcase.design.md#des-site-evidence)已承接：runtime_capture 绑定版本/场景/证据窗口与可公开来源；来源不足不背书运行事实；同 Trace 五列承接与六列验证限定网站子范围。 [测试 benchmark 局部边界](../../testing/benchmarks/mainstream-public-chain-testing-benchmark.design.md#des-bench-public-window)仅承接历史/局部 green 不得生成当前公开事实；不覆盖全渠道发布、撤回、LiveOps 审核或四层全域验收，不改变本卡实现状态和既有证据。
- 实现状态：部分实现
- 实现与验证依据：官网 release-candidate 快照将版本和下载资产绑定同一 tag，缺 checksum/platform 时阻断推荐；首页 claim 检查保留公开 preview 边界。 [源码/回归定义](https://github.com/eng-cc/oasis7/blob/ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad/site/assets/release-candidate.js)、[源码/回归定义](https://github.com/eng-cc/oasis7/blob/ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad/scripts/site-release-candidate.test.cjs)、[源码/回归定义](https://github.com/eng-cc/oasis7/blob/ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad/scripts/site-homepage-claim-check.sh)；[核验任务](https://github.com/eng-cc/oasis7/issues/4696)。本轮为固定源码审读，回归定义不等于实际通过。 网站交付及原候选局部检查见[PR #4697](https://github.com/eng-cc/oasis7/pull/4697)，不延伸为全部渠道验收。
- 核对日期与基线：2026-10-15；需求与实现源码 `ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad`；GitHub 固定源码审读；入口与已核对范围见上，未执行运行、浏览器、部署或真人验收；本条原验收条件与层级不变。
- 剩余范围：设计仍按上述准确承接及其未覆盖边界；实现与验证方面，该实现仅网站版本/资产及静态文案子范围；所有公开 claim 的 primary mode、专业事实、QA窗口与LiveOps审核记录仍未核清。

- 设计复核日期与基线：2026-10-15；`dffd797f` 文档候选；设计剩余：全渠道专业事实、QA verdict 和 LiveOps 审核闭包仍外部。不改变本卡固定源码与已取得实现范围。

<a id="ac-entry-claim-001"></a>
### AC-ENTRY-CLAIM-001：局部 green 不会升级公开状态

- 覆盖要求：REQ-ENTRY-CLAIM-001
- 场景与结果：代表性候选文案能区分已证实事实、适用限制、未承诺内容/风险和下一步；入口、专业 authority、QA 与根 README 不一致时采用更窄 claim，不能把局部进程、帖子、认证或历史样本写成当前集成、发行就绪或可用性。
- 证据边界：公开状态由根 README 与同一证据窗口拥有；LiveOps、产品、QA 和专业 authority 提供输入与审核，产品层不保存公告正文或发布记录。

<a id="req-entry-claim-002"></a>
### REQ-ENTRY-CLAIM-002：事实漂移必须触发纠正或撤回

Trace ID: O7-REQ-f71ac2e9-1586-44af-89b7-e98ff364e70c

- 要求：当前事实、证据或限制漂移时，所有 active public surfaces 必须同步纠正、降级或撤回受影响文案；messaging rollback 不得被表达为软件回滚、QA 放行或事故恢复。
- 验收：AC-ENTRY-CLAIM-002


- 功能点：[REQ-ENTRY-CLAIM-002](#req-entry-claim-002)；验收 [AC-ENTRY-CLAIM-002](#ac-entry-claim-002)。
- 适用范围：所有活跃公开 surface 的事实漂移纠正、降级与撤回；沿用本条原有阶段和前置条件。
- 系统设计状态：部分覆盖
- 系统设计依据：[官网专业设计](../../site/github-pages/public-site-discovery-and-showcase.design.md#des-site-publishing)已承接：同 URL 纠正/撤回同步 CN/EN、详情/摘要/RSS，回滚不复活错误正文；同 Trace 五列承接与六列验证限定网站子范围。
- 实现状态：部分实现
- 实现与验证依据：官网发布检查把撤回正文、列表/首页/RSS 派生、稳定ID和双语言状态一起核对；对应回归包含单语言撤回拒绝与双语纠正。 [源码/回归定义](https://github.com/eng-cc/oasis7/blob/ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad/scripts/site-discovery-check.py)、[源码/回归定义](https://github.com/eng-cc/oasis7/blob/ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad/scripts/site-discovery-check.test.py)；[核验任务](https://github.com/eng-cc/oasis7/issues/4696)。本轮为固定源码审读，回归定义不等于实际通过。 网站交付及原候选局部检查见[PR #4697](https://github.com/eng-cc/oasis7/pull/4697)，不延伸为全部渠道验收。
- 核对日期与基线：2026-10-15；需求与实现源码 `ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad`；GitHub 固定源码审读；入口与已核对范围见上，未执行运行、浏览器、部署或真人验收；本条原验收条件与层级不变。
- 剩余范围：设计仍按上述准确承接及其未覆盖边界；实现与验证方面，仅覆盖网站观察/开发日志静态内容；所有 active public surfaces 的漂移检测、实际纠正/撤回同步和发布验证未核清。

- 设计复核日期与基线：2026-10-15；`dffd797f` 文档候选；设计剩余：其他 active public surfaces、自动漂移检测与第三方缓存收回不在本设计。不改变本卡固定源码与已取得实现范围。

<a id="ac-entry-claim-002"></a>
### AC-ENTRY-CLAIM-002：撤回只改变 claim 状态

- 覆盖要求：REQ-ENTRY-CLAIM-002
- 场景与结果：证据失效、blocker 或事故回退后，旧文案标记 stale/withdrawn 或收窄并引回根 README；公开说明只陈述确认影响、受影响入口、当前恢复动作和可信下一步，不承诺恢复时间或泄露内部证据。
- 证据边界：LiveOps 执行渠道同步，QA 复核证据，专业 owner 决定产品边界，技术 rollback/incident 由对应 authority 决定；产品层不越权代签。

<a id="req-entry-claim-003"></a>
### REQ-ENTRY-CLAIM-003：当前事实、产品目标、假设与证据边界必须分离

Trace ID: O7-REQ-f9218cff-4a0e-4a40-8a2f-133d9c824385

- 要求：每个拟发布的公开 claim 必须同时给出当前事实、产品目标、假设和证据边界；只有当前事实在适用范围内得到根 README、对应专业 authority 与同一证据窗口支持时，才可将其标记为可发布公开 claim。
- 验收：AC-ENTRY-CLAIM-003


- 功能点：[REQ-ENTRY-CLAIM-003](#req-entry-claim-003)；验收 [AC-ENTRY-CLAIM-003](#ac-entry-claim-003)。
- 适用范围：每个拟发布 claim 的事实、目标、假设、证据边界与可发布条件；沿用本条原有阶段和前置条件。
- 系统设计状态：部分覆盖
- 系统设计依据：[官网专业设计](../../site/github-pages/public-site-discovery-and-showcase.design.md#des-site-evidence)已承接：concept/illustrative/runtime_capture 与小说类型分离，目标/概念不冒充真实运行来源；同 Trace 五列承接与六列验证限定网站子范围。 [认领经济四层边界](../../game/gameplay/gameplay-agent-claim-economy-contract.design.md#public-claim-boundary)原接收保留。 [测试 benchmark 局部边界](../../testing/benchmarks/mainstream-public-chain-testing-benchmark.design.md#des-bench-public-categories)仅承接历史事实、目标、假设与证据范围分离；不覆盖全渠道发布、撤回、LiveOps 审核或四层全域验收，不改变本卡实现状态和既有证据。
- 实现状态：未核实
- 实现与验证依据：首页真实 case provenance/controlled replay → 静态构建 gate；release 改 tag → 历史试玩 claim 自动收窄。 已定位历史证据不续签新候选的真实运行消费者；gate 校验 source/build/window 格式与限定文案，但不建立每 claim 的事实/目标/假设/可发布审核记录。 [site/assets/app.js:1051–1065](https://github.com/nil-studio/oasis7/blob/0ec10379419b0aaedb9d16157b81c92f4e6e3495/site/assets/app.js#L1051-L1065)；[scripts/site-homepage-claim-check.sh:392–442](https://github.com/nil-studio/oasis7/blob/0ec10379419b0aaedb9d16157b81c92f4e6e3495/scripts/site-homepage-claim-check.sh#L392-L442)；[.github/workflows/pages.yml:23–39](https://github.com/nil-studio/oasis7/blob/0ec10379419b0aaedb9d16157b81c92f4e6e3495/.github/workflows/pages.yml#L23-L39)；[README.md:8–33](https://github.com/nil-studio/oasis7/blob/0ec10379419b0aaedb9d16157b81c92f4e6e3495/README.md#L8-L33)
- 核对日期与基线：2026-10-17；源码 `0ec10379419b0aaedb9d16157b81c92f4e6e3495`，本轮固定源码/操作记录只读复核；未运行安装、浏览器、模型或业务验收。
- 剩余范围：每个拟发布 claim 的四层字段与根 README/authority/同窗 QA 的语义支持及 publish_ready 授权仍未核清；保留网站子范围，不能因 provenance 字段齐全就升整体实现。

- 设计复核日期与基线：2026-10-15；`dffd797f` 文档候选；设计剩余：其他专业领域、非官网渠道及全部公开审核仍外部。不改变本卡固定源码与已取得实现范围。

- 本次实现复核：2026-10-17；需求与源码固定基线 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9`；GitHub固定文本/PR记录审读；覆盖本条原入口/阶段及验收范围，未执行浏览器、安装或业务验收；旧核对基线只保留历史语境。

<a id="ac-entry-claim-003"></a>
### AC-ENTRY-CLAIM-003：目标或假设不会伪装成当前事实

- 覆盖要求：REQ-ENTRY-CLAIM-003
- 场景与结果：候选文案分别标记当前事实、产品目标、假设和证据边界；若目标、假设、局部验证、历史记录或渠道存在没有同一证据窗口与根 README 支持，则候选只能保持 `draft`、`reviewed`、`blocked` 或 `withdrawn/stale`，不能标记 `publish_ready` 或对外声称已发布。
- 证据边界：产品 PRD 定义分类和发布门槛；根 README 拥有当前公开状态；专业 authority 与 QA 提供各自范围内的输入；具体候选 verdict 和发布记录留在对应 task/channel evidence。

## 6. 组合验收

- RC-1：每条发行沟通都绑定同一候选或版本、primary mode、适用入口、当前证据范围与更新时间。
- RC-2：`draft`、`reviewed` 或 `publish_ready` 不会被当前公开入口引用为已发布事实；发布证据包含 channel、时间、URL/message ID 与 owner。
- RC-3：公开说明明确区分 confirmed facts、scope/limitations、non-promises/known risks 与 next/recovery。
- RC-4：缺证、状态漂移或 blocker 会使旧文案失效，并在所有 active surface 触发纠正、降级或撤回。
- RC-4A：网络、安全、结算、创世、validator 与资产 claim 同时绑定当前根 README、专业权威与同一证据窗口；历史 verdict、模板或部分 gate 通过不会自动升级当前公开状态。
- RC-5：根 README、站点、公告、changelog 与渠道说明不存在阶段或 claim 分叉。
- RC-6：沟通可追踪 LiveOps 起草、producer 承诺审核、QA 证据复核和实际发布记录。
- RC-7：事故或 rollback 不沿用旧 go 结论，不把 messaging rollback 冒充软件恢复。
- RC-8：公开内容遵循最小披露，不泄露敏感配置、内部证据或个人信息。
- RC-9：抽样 channel profile、帖子和回复能区分渠道 presence 与产品集成/访问；CTA 只指向当前受支持公开入口或正式反馈/贡献路径。
- RC-10：产品、缺陷与合作信号都有 owner 和下一步，互动指标不会升级 claim、readiness 或 roadmap。
- RC-11：公共发现、静态 docs hub 与 Viewer 手册镜像的配对语言 surface 对同一事实保持 primary mode、证据窗口、限制、下一步及 canonical source 的语义等价；任一事实漂移时，相关 surface 同时纠正或收窄，且亮点与技术说明不掩盖边界。
- RC-12：每个拟发布 claim 都明确区分当前事实、产品目标、假设和证据边界；仅当前事实在同一证据窗口内获得根 README、专业 authority 与 QA 支持时，才可进入 `publish_ready`。

### 6.1 验收追踪

| 成功标准 | 专业 owner | 权威文档 | 验证证据 | 测试层级 |
| --- | --- | --- | --- | --- |
| RC-1 / RC-3 | liveops_community / producer_system_designer / qa_engineer | `doc/readme/prd.md`; `doc/testing/prd.md` | 候选、模式、证据和消息结构抽样审计 | test_tier_required |
| RC-2 / RC-6 | liveops_community / producer_system_designer | `README.md`; `doc/readme/prd.md` | 审核记录及 channel/time/URL/message ID/owner 发布 trace | test_tier_required |
| RC-4 / RC-4A / RC-5 | producer_system_designer / liveops_community / qa_engineer / blockchain_ops_engineer | `README.md`; `doc/readme/prd.md`; `doc/testing/prd.md`; 对应专业权威 | active surface claim 对账、同窗证据核验与 stale/withdrawn 纠正记录 | test_tier_required |
| RC-7 / RC-8 | liveops_community / qa_engineer / runtime_engineer / viewer_engineer / blockchain_ops_engineer | `doc/readme/prd.md`; `doc/testing/prd.md` | incident/rollback 抽样与最小披露审计 | test_tier_required |
| RC-9 / RC-10 | liveops_community / producer_system_designer / qa_engineer | `README.md`; `doc/readme/prd.md`; `doc/testing/prd.md` | 渠道 presence/integration 负例、CTA 与信号 owner/next-action 审计 | test_tier_required |
| RC-11 | liveops_community / producer_system_designer / viewer_engineer / qa_engineer | `README.md`; `doc/readme/prd.md`; `doc/testing/prd.md` | CN/EN 公共发现 surface 的 primary mode、证据窗口、限制、下一步及 drift 纠正对账 | test_tier_required |
| RC-12 | liveops_community / producer_system_designer / qa_engineer | `README.md`; `doc/readme/prd.md`; `doc/testing/prd.md`; 对应专业 authority | 候选 brief 抽样核对四层字段及 publish-ready 门槛 | test_tier_required |

## 7. Non-Goals

- 不固化任何候选 ID、历史 go/no-go、当前阶段、公告正文、FAQ 内容或发布日期。
- 不定义具体模板字段、渠道排期、运营 runbook、事故流程或技术回滚步骤。
- 不替代根 README、正式 changelog、PR 与实际验证记录、QA gate 或 LiveOps 执行记录。
- 不把内部审批、草稿完成或互动量当作 publication evidence。

## 8. 未决问题与假设

- 未决：具体候选的公开 verdict、渠道排期、公告正文和纠正/撤回记录，必须由 LiveOps、QA、产品与根 README 在同一证据窗口内确认；本分册不预先替代这些记录。
- 假设：当公开 surface、专业权威和证据窗口不一致时，采用更窄 claim，标记 draft/review/blocked 或撤回，并把读者引回当前根 README，而不是沿用旧文案。

## 全量语义追踪

| REQ / AC | 专业 owner | 专业权威 | 验证证据 | 测试层级 |
| --- | --- | --- | --- | --- |
| [REQ-ENTRY-CLAIM-001](#req-entry-claim-001) / [AC-ENTRY-CLAIM-001](#ac-entry-claim-001) | `producer_system_designer` | [`readme 产品与治理 PRD`](../../readme/prd.md)、[`testing PRD`](../../testing/prd.md)、[`Gameplay Agent 认领经济合同`](../../game/gameplay/gameplay-agent-claim-economy-contract.prd.md#public-claim-boundary)、[`Gameplay Agent 认领经济合同 design`](../../game/gameplay/gameplay-agent-claim-economy-contract.design.md#public-claim-boundary)、[`restricted grant LiveOps runbook`](../../game/gameplay/gameplay-agent-claim-restricted-grant-liveops-runbook-2026-03-29.md#public-claim-boundary)、[`README.md`](../../../README.md) | 待执行方法：按 AC 抽样同候选文案与 README/QA/专业证据，对比历史或局部 green 负例；待取得审核记录；导航关系不等于通过证据 | `test_tier_required` |
| [REQ-ENTRY-CLAIM-002](#req-entry-claim-002) / [AC-ENTRY-CLAIM-002](#ac-entry-claim-002) | `producer_system_designer` | [`readme 产品与治理 PRD`](../../readme/prd.md)、[`testing PRD`](../../testing/prd.md)、[`Gameplay Agent 认领经济合同`](../../game/gameplay/gameplay-agent-claim-economy-contract.prd.md#public-claim-boundary)、[`Gameplay Agent 认领经济合同 design`](../../game/gameplay/gameplay-agent-claim-economy-contract.design.md#public-claim-boundary)、[`restricted grant LiveOps runbook`](../../game/gameplay/gameplay-agent-claim-restricted-grant-liveops-runbook-2026-03-29.md#public-claim-boundary)、[`README.md`](../../../README.md) | 待执行方法：按 AC 演练事实漂移，核对所有活跃 surface 的纠正/撤回及实际发布定位；待取得同步记录；导航关系不等于通过证据 | `test_tier_required` |
| [REQ-ENTRY-CLAIM-003](#req-entry-claim-003) / [AC-ENTRY-CLAIM-003](#ac-entry-claim-003) | `producer_system_designer` | [`readme 产品与治理 PRD`](../../readme/prd.md)、[`testing PRD`](../../testing/prd.md)、[`Gameplay Agent 认领经济合同`](../../game/gameplay/gameplay-agent-claim-economy-contract.prd.md#public-claim-boundary)、[`Gameplay Agent 认领经济合同 design`](../../game/gameplay/gameplay-agent-claim-economy-contract.design.md#public-claim-boundary)、[`restricted grant LiveOps runbook`](../../game/gameplay/gameplay-agent-claim-restricted-grant-liveops-runbook-2026-03-29.md#public-claim-boundary)、[`README.md`](../../../README.md) | 待执行方法：按 AC 审核四层字段与可发布条件；专业认领边界仅为一项输入，待取得完整公开 claim 审核记录；导航关系不等于通过证据 | `test_tier_required` |
