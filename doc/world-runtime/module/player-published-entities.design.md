# oasis7 Runtime：玩家发布制成品的 WASM 模块与 Profile 治理闭环设计

- 对应需求文档: `doc/world-runtime/module/player-published-entities.prd.md`
- 作者交付与核验手册：[player-published-entities.manual.md](player-published-entities.manual.md)。
- 当前任务状态与历史变更：GitHub task issue evidence 与 Git history。

<a id="1-设计定位"></a>
## 1. 设计定位与目标
定义玩家发布制成品的 WASM 模块与 Profile 治理闭环，统一玩家产物发布、模块约束与身份治理。

本专题同时拥有这条闭环的作者内容交接：把想改变的工业行为、可继续编辑的源、Profile、构建与授权证据、采用条件和失败下一步交给发布者及消费者。§§8–10 补齐该交接，不新增创作服务、发布 API 或工业四阶段状态机。源码事实审读基线为 `a23bc41943893da72dbbd78df65214a8a0ae964c`；离线样例只证明材料可核查，不是已经制作、发布或可玩的新能力。

## 2. 设计结构
- 玩家产物层：定义玩家发布实体、制成品与其模块/profile 表达。
- 治理审核层：对玩家发布内容执行合法性、权限与版本审核。
- 运行接入层：审核通过后进入运行时模块与实体主路径。
- 审计回滚层：记录发布来源、治理结论与必要回滚信息。

### 2.1 需求承接与分配表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [REQ-WR-GR-002](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-002)；Trace refs: O7-REQ-ce8c31b1-5294-4acd-a65f-2012bcbef911 | 专业依据：[发布验收](player-published-entities.prd.md#ppe-release-acceptance)；AC-1/4/6/7/8：append-only、完整承诺与当前授权 | [发布 authority](player-published-entities.design.md#ppe-publication-authority) | WASM/runtime/QA | 不新增 operator role policy、profile patch 或完整期限/范围框架 |
| [REQ-WR-GR-003](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-003)；Trace refs: O7-REQ-ac12295b-63a5-4440-9924-deaedafd7965 | 专业依据：[发布验收](player-published-entities.prd.md#ppe-release-acceptance)；AC-5/9：回滚冻结新准入且保留历史、原子性与 replay | [有限回滚](player-published-entities.design.md#ppe-rollback-disposition) | WASM/runtime/QA | 不删除历史、不新增消费冻结或版本选择 profile |
| [REQ-WR-GR-009](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-009)；Trace refs: O7-REQ-3d713ab7-87c2-41df-9c89-827859c23313 | [AC-WR-GR-009](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#ac-wr-gr-009) 的内容交付子范围：来源、假设、投入、范围、风险及阶段缺口可读，并给补证或停止路径 | [作者输入与阶段边界](player-published-entities.design.md#ppe-creator-inputs) | GAME-018 / producer / gameplay / WASM / runtime | 部分承接；不代签确定性工业模拟、受限试点、四阶段强制顺序或有范围准入的执行与组合验收 |
| [REQ-WR-GR-009](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-009)；Trace refs: O7-REQ-3d713ab7-87c2-41df-9c89-827859c23313 | AC-WR-GR-009 的可核查内容子范围：通过真实旧源和新 ID 离线样例区分行为参数、Profile 与未制作材料 | [离线铁锭样例](player-published-entities.design.md#ppe-creator-example) | GAME-018 / producer / gameplay / WASM / runtime | 部分承接；不代签确定性工业模拟、受限试点、四阶段强制顺序或有范围准入的执行与组合验收 |
| [REQ-WR-GR-002](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-002)；Trace refs: O7-REQ-ce8c31b1-5294-4acd-a65f-2012bcbef911 | 专业依据：[发布验收](player-published-entities.prd.md#ppe-release-acceptance) AC-1/2/4/6/7/8：完整交付包关联同一候选的源、当前授权、实际 payload、工件与待补证材料 | [交付包](player-published-entities.design.md#ppe-creator-delivery) | WASM / runtime / economics / QA；既有 release 与 consumer 合同 | 不增加发布权限，不把 Profile 查询、技术 Apply 或模块计划当成正式工业采用及生产完成 |
| [REQ-WR-GR-002](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-002)；Trace refs: O7-REQ-ce8c31b1-5294-4acd-a65f-2012bcbef911 | 专业依据：[发布验收](player-published-entities.prd.md#ppe-release-acceptance) AC-1/2/4/6/7/8：动作结果、发布单状态与未知/陈旧读面分别准确表达，不从批准或失败推断世界效果 | [状态文案](player-published-entities.design.md#ppe-creator-status-copy) | WASM / runtime / economics / QA；既有 release 与 consumer 合同 | 不增加发布权限，不把 Profile 查询、技术 Apply 或模块计划当成正式工业采用及生产完成 |
| [REQ-WR-GR-002](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-002)；Trace refs: O7-REQ-ce8c31b1-5294-4acd-a65f-2012bcbef911 | 专业依据：[发布验收](player-published-entities.prd.md#ppe-release-acceptance) AC-1/2/4/6/7/8：release/Profile 定义、安装/激活、合法工业采用与各类型实际消费者分别核验 | [采用核验](player-published-entities.design.md#ppe-creator-adoption) | WASM / runtime / economics / QA；既有 release 与 consumer 合同 | 不增加发布权限，不把 Profile 查询、技术 Apply 或模块计划当成正式工业采用及生产完成 |
| [REQ-WR-GR-003](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-003)；Trace refs: O7-REQ-ac12295b-63a5-4440-9924-deaedafd7965 | 专业依据：[发布验收](player-published-entities.prd.md#ppe-release-acceptance) AC-5/9：作者变更、有限回滚与消费者交接准确说明保留和失效的事实 | [变更传播](player-published-entities.design.md#ppe-creator-change) | WASM / runtime / content owner；当前 exact artifact 与历史 receipt | 不扩展许可到期、原子替代、既有实例停用或通用经济消费冻结 |

## 3. 关键接口 / 入口
- 玩家发布实体模型
- WASM 模块/Profile 绑定
- 治理审核入口
- 发布审计/回滚记录

本次内容设计消费下列专业技术边界；接口、持久化、权限、兼容与恢复仍由对应合同拥有，不以交接表另立协议。

| 内容交接依赖 | 专业合同与直接消费者 | 作者需要交付的差异 |
| --- | --- | --- |
| 可复建源、工件与身份 | [确定性构建](../wasm/wasm-deterministic-build-pipeline.design.md#sr2-build-contract)、[节点证据接收](online-module-release-legality-closure-2026-03-08.design.md#sr2-release-node-evidence-receiver) | 源输入闭包、真实构建产物与验证结果；不从文件存在推断外部签名或 finality 已验证 |
| 发布、激活与恢复 | 本文 §§5–6；[activation 合同](module-lifecycle.md#sr2-activation-contract)、[迁移与 publication](module-lifecycle.md#sr2-migration-publication) | 精确 manifest、实例目标、激活意图、当前批准与失败处置；目标设计和本候选实证分开 |
| Profile 与经济调用 | [Profile 类型](../../../crates/oasis7_wasm_abi/src/economy.rs)、[发布请求](../../../crates/oasis7_agent_api/src/world_action/action.rs)、[经济动作消费者](../../../crates/oasis7_runtime/src/world/economy.rs) | 新 ID 元数据、行为源和实际工厂/物料依赖分别交接；不让一类材料代替另一类 |
| 读面与可观测结果 | [release 状态](../../../crates/oasis7_world_state/src/state.rs)、[release 状态提交](../../../crates/oasis7_world_state/src/state/module_release_transition.rs)、[Profile reader](../../../crates/oasis7_runtime/src/world/resources.rs) | 请求状态、动作结果、实际 profile 内容与运行状态分开呈现；本文文案不宣称专用 UI 已上线 |

本次不改变存储/ABI、节点部署或性能预算；质量目标沿用配对 PRD，实际三节点 SLA、恢复和跨节点一致性仍须既有专业验收。

## 4. 约束与边界
- 玩家发布内容不得绕过治理审核。
- 实体、模块与 profile 绑定必须可追溯。
- 不在本专题扩展完整 UGC 市场机制。
- [GAME-018](../../game/gameplay/gameplay-industrial-creation-and-cross-region-market-contract.prd.md#ac-game-018-01) 拥有提案、确定性模拟、受限试点、治理准入的玩家循环。本专题只承接内容输入、交付和可读边界；当前 `Shadow` 是 manifest/Profile 校验承诺，不能称为工业确定性模拟。
- `Applied` 的技术发布事实不能放行缺少上述四阶段证据的新工业能力正式采用。未具备专业执行入口或证据时，保留可复核交付包，补证、缩小目标、延期或使用已具备合法条件的既有能力；不能用手工批准、换 ID 或离线样例替代未完成的执行链。

<a id="ppe-publication-authority"></a>
## 5. 发布承诺、当前授权与权威结果
- Profile admission 为 append-only：新发布必须使用尚不存在的 product/recipe/factory ID，Shadow 与 Apply 均复核；同 ID 同内容也不是幂等发布，不提供 patch 绕过。
- Shadow commitment 必须绑定被审核的完整 manifest、module changes 与 profile changes 内容；Approve 与 Apply 消费同一承诺。改动任一 profile 内容不得继承旧审核，失败不得产生部分 module/profile 效果。

  采用可选、None 时不序列化的 v1 commitment，覆盖完整 ModuleManifest 与按 product_id/recipe_id/factory_id 归一化排序的完整 profile vectors；Shadow 计算，Apply 重算核对。新增字段缺失不改变历史 event bytes；legacy 空 profile 待决请求保留旧路径，legacy 带 profile 且无 commitment 的待决请求 fail closed，须显式重新提交；历史已 committed 事件不追溯改写。
- Approved 只表示当时获得受限准入资格。Shadow/Apply executor 沿用当前已登记 agent 资格，不新增 operator role policy；发布 authority 独立校验，Apply 必须在当前 authority 下复核 request submitter 仍是 exact artifact owner、每个 required-role approver 仍有该 role binding，以及所需角色与世界前置；旧批准、缓存或历史 receipt 不保留新发布权。授权到期、范围变动和原子替代的更完整模型为后续专业合同，不把目标合同表述为现有机制。
- 只有权威提交的 release 与 profile governed 事件确认本次世界效果；提交、构建、Shadow、Approved 不表示已可用。拒绝事件是失败原因，不是成功 receipt；重读历史结果不复制效果或获得新权限。

<a id="ppe-rollback-disposition"></a>
## 6. 回滚的有限处置
- 本闭环选择“仅停止新的发布/激活”：恢复旧模块与被回滚发布单的新增发布/激活冻结必须形成同一原子世界结果，并保留可审计的原因、来源和 disposition。旧批准或 committed receipt 不自动解冻或再次激活。
- 已 committed profile 定义、独立资产、来源与历史事件保留，不能把已发生的发布解释为从未发生。冻结须绑定确切被回滚的 release identity，不由版本差异推断。保留的 profile 可继续被既有 consumer 查询与校验；恢复旧 WASM 不保证每个 profile 均通过其校验，不兼容时沿用 consumer 的明确拒绝，不引入按模块版本切换 profile。既有能力的后续行为仍按当前专业合同校验；本次回滚不新增经济消费限制、没收、同 ID 替换或一般退出机制。
- 回滚失败必须保留回滚前 module/profile/准入事实；重复、snapshot restore 与 replay 只能重读同一结果，不复制恢复、发布或经济效果。
- 旧快照缺少可证明的 release/profile 关联时，不猜测归属或删除 profile；兼容处理须保全历史并给出明确准入处置。

  rollback receipt 同时落账不可变 admission-freeze marker，作用域为被移除的 FROM release 的 exact `(module_id, from_module_version, from_wasm_hash)`。该 artifact identity 是有意选择的冻结范围：重新建立 request 但复用同一 tuple 仍被拒绝；不同 artifact tuple 不自动受其冻结。

  既有 `ModuleRollbackApplied.wasm_hash` 继续表示恢复目标 TO artifact，不用作冻结键；新增可选 `from_wasm_hash` 与 `from_release_request_id`，后者仅在唯一 exact Applied request 可归属时填写，缺失或多重匹配不猜测。durable marker 保留 scope tuple、rollback proposal、可证明的来源 linkage 与稳定原因 `rollback_stop_new_admission`，receipt/readback 明确既有事实保留。

  registration/direct install/direct upgrade/activation/republication 以及 rollback 的 TO-target activation 均读取同一 durable marker 拒绝被冻结 tuple 的新准入；无模块 tuple 的 raw artifact upload 本身不产生世界 publication，不在此冻结范围，existing instances 不因 marker 自动停用。

  marker map 为空、可选字段为 None 时省略，旧历史事件不重编码为新批准。

一般授权变动与本节 exact admission-freeze 保持不同职责。[有限 recipe 授权生命周期](../runtime/governed-extension-lifecycle.design.md#extension-current-guard)规定其受支持 profile 的当前授权、稳定范围、期限/处分及发布/安装/激活消费者合同；[原子替代](../runtime/governed-extension-lifecycle.design.md#extension-atomic-replacement)要求受影响旧工作全部结清后才交接范围。这些是待实现的组合适配，不改变本节 marker 对现存实例没有自动停用效果的事实，也不以新 grant 撤销原 freeze、owner/role 或 append-only 检查。

## 7. 产品对齐与验收
[`受治理的区域能力与扩展`](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md) §2.1/§2.2 是产品结果 authority。本专题把逐次重验、权威 receipt、历史保全与明确限制结果投影到发布闭环；不改变该产品 authority。

world-first：发布与恢复由权威世界提交；emergence-first：创作者在受限准入内添加能力；persistent：profile、资产与 receipt 历史不因回滚消失；auditable：完整 payload、准入与恢复结果可追溯；extensible：后续 patch/授权/退出机制可扩展，但必须保持这些边界。

`test_tier_required` 至少证明 profile payload 漂移拒绝、当前角色漂移拒绝、回滚准入冻结/历史保全与失败原子性；`test_tier_full` 覆盖持久化恢复/replay、跨节点一致性与既有 SLA。验收目标不构成实现或通过声明，fresh 专业证据由绑定 task issue 承载。

## 8. 作者输入、完整交付包与离线样例

<a id="ppe-creator-inputs"></a>
### 8.1 从创作意图到可审读材料

作者先说明“哪项现有工业需求无法满足、准备改变哪一个可观察结果”，再选择新配方、产品或设施内容。依赖已有能力即可解决的问题可以直接回到既有路线，不要求为参与创作而发布重复内容。交接中的自然语言说明至少回答下表；它随当前 Task/PR 或已有交付位置保存，不形成新的注册表或运行时字段。

| 输入 | 必须可读的实际内容 | 与既有执行合同的关系 |
| --- | --- | --- |
| 问题与比较 | 受影响需求、现有路线、预期改善、准备保留的行为、不采用的理由 | 收益是假设；没有实际观测不写“更高效”或“已平衡” |
| 能力与依赖 | 新 module/content ID、行为源、Profile ID、依赖物料/工厂/阶段与已有消费者 | 全部依赖在实际候选世界复核；内置定义不证明所有世界已安装或拥有资源 |
| 资源与风险 | 预期输入/输出/副产物/电力/时长，作者制作投入，以及待验证的资源压力、失败与兼容风险 | 制作工时与世界资源分开；模拟预测、试点已投入/损失、正式生产实际结果分开 |
| 适用范围与退出 | 预期用途、区域/对象、期限、资源上限、停止条件、保留价值与恢复选择 | 缺少合法的实际 scope 或执行入口时写“未取得”，不能用作者意向代替授权 |
| 来源与待补证 | 源版本、依赖及使用权定位，提案/模拟/试点/准入各阶段已有和缺少的证据、下一复查条件 | 引用 [来源与权利](../../game/gameplay/content-production-and-rights.design.md#source-and-rights)；代码许可与世界内许可/版税分别处理 |

四阶段的交付需求来自 GAME-018 §3，不新增阶段规则：

| 产品阶段 | 交接者必须能找到什么 | 当前技术发布能说明什么；不足时的下一步 |
| --- | --- | --- |
| 可审计提案 | 来源、问题、假设、预期影响、风险、缺口及撤回条件 | release payload 可承载候选模块/Profile；完整提案与成本/选择仍需可读材料。缺项先补材料，不据此取得世界能力 |
| 确定性模拟 | 同一权威输入与声明范围、模拟结果及失败/敏感假设的比较 | Shadow 只承诺校验内容；构建可复现、手算和单次模块输出也不代签此阶段。缺专业入口/证据时保留假设并延期，不能标为模拟通过 |
| 原型/受限试点 | 实际允许的 scope、期限、资源上限、退出条件，投入/损失与结果 | release 请求本身不提供完整有限试点生命周期；无该证据不把任意测试世界调用改称试点。可缩小提案并等待合法试点条件 |
| 治理准入 | 绑定前序证据的一次有范围结果、用途/期限、复核和撤销条件 | required-role、owner、commitment 与技术 Apply 只覆盖发布子链；不足以证明整套工业准入。缺失时保留技术事实并显示“正式采用条件未齐备” |

“待补证”不是新增世界状态。下一复查条件写成可执行事项，例如“取得同一输入下的模拟证据后再审读”，不虚构日期、倒计时或已存在的 `next_recheck` API。若未取得可执行阶段，仍可交付可编辑源、参数说明和风险假设，由原 owner 接续；没有合法替代时沿 GAME-018 使用 `no_safe_creation_recovery` 的停止含义。

<a id="ppe-creator-delivery"></a>
### 8.2 发布者与使用者收到的完整交付包

交付包是现有材料的关联说明，不增加归档格式、manifest 字段或签名方案。作者说明每份材料的实际位置和版本；发布者复核同一候选；消费者读出实际采用前置。缺项保留为明确缺口，不用示例值填实。

| 材料 | 应交付的实际内容与接收者 | 完整性判据 |
| --- | --- | --- |
| 创作说明 | §8.1 的问题、比较、风险、四阶段证据位置和缺口；gameplay/producer 接收 | 能区分内容假设、技术发布事实和正式工业采用条件 |
| 可编辑源与输入闭包 | 模块源码、包含的模板、Cargo manifest/lock、SDK 和其他依赖定位、构建输入规则与源版本；WASM owner 接收 | 新作者能继续编辑；`include!` 引入文件及 path dependency 均在构建 source inventory 内，不能只交单个 `.rs` |
| 行为与 Profile | 实际行为常量/逻辑、完整 `ModuleProfileChanges` 三组数组、新 ID 与既有依赖清单；runtime/economics 接收 | 材料/电力/产量来自行为；Profile 的阶段、偏好、维护或可交易性不是同一类数据，不互相代写 |
| 实际发布输入 | 实际 `ModuleManifest`、`activate`、`install_target`、requester 与 `required_roles`，及当前 artifact owner/角色核验位置；发布者接收 | 使用现有 Action 类型与实际世界事实；目标实例、授权与角色不由样例生成。空角色集合的既有默认行为不是新权限策略 |
| 真实构建与来源证明 | artifact bytes、构建 receipt/manifest、源与工件身份、builder/canonicalizer、签名与适用 release proof 的可检索原件；WASM/节点接收 | 由[受控构建](../wasm/wasm-deterministic-build-pipeline.prd.md#sr2-obligation-sc-5)和[证据接收](online-module-release-legality-closure-2026-03-08.design.md#sr2-release-node-evidence-receiver)验证。同名文件、路径、CID、hash 文本或本地自签不自行构成验证通过 |
| 当前请求与提交结果 | 真实 request ID、被审 manifest/Profile、承诺、当前批准、动作拒绝或 release 提交事件及 readback；runtime/QA 接收 | 显示所属世界、读回时刻/位置与实际状态；不得提前编造 ID、hash、签名、finality 或成功 receipt |
| 使用说明与故障恢复 | 新内容用途、模块/实例/激活依赖、合法工厂/库存/电力条件、预期输出、失败原因及下一步；使用者接收 | 能沿 §9.2 找到消费者，不以发布成功代替一次真实使用；保留 §10 的变更影响 |
| 来源、许可与继续制作 | 源和第三方素材的许可证/署名/修改依据，原生可编辑内容和维护接收方；content owner 接收 | 沿既有[制作交接](../../game/gameplay/content-production-and-rights.design.md#production-handoff)。Cargo 的代码许可声明不授予工业范围、版税、独占或外部商业收益 |

生产源码由既有外部受控 builder 形成工件；宿主直编只在既有 dev/test 授权范围内。发布包不附带游戏内 IDE、发现市场、商店、图标/模型分发或自助授权的承诺。现有模块上限、Profile 数量和三节点时延指标仍在配对 PRD，不新增一套作者等级或服务 SLA。

<a id="ppe-creator-example"></a>
### 8.3 离线铁锭配方交接样例

**用途与状态。** 这个例子用于学习“同一份行为与 Profile 如何交给发布者”，是独立编号的文档演练，不是新的平衡方案、实际 source package、已构建工件或已批准能力。演练保留现有铁锭参数，只改待制作候选的身份；不存在提高收益或替换既有配方的主张。未执行源码复制、构建、Shadow、发布、模拟或试点。

**可编辑起点。** 行为取自 [`m4_recipe_smelter_iron_ingot/src/lib.rs`](../../../crates/oasis7_builtin_wasm_modules/m4_recipe_smelter_iron_ingot/src/lib.rs)，并依赖 [共享 recipe 模板](../../../crates/oasis7_builtin_wasm_modules/_templates/m4_recipe_module_template.rs)。制作时须同时检查该包的 [Cargo.toml](../../../crates/oasis7_builtin_wasm_modules/m4_recipe_smelter_iron_ingot/Cargo.toml)、`Cargo.lock`、SDK path dependency 与 `package.metadata.oasis7_source_inputs` 的 includes；复制或改名后必须使源路径与输入清单相符。这里不修改内置注册表，也不把内置同步脚本作为玩家准入入口。

| 内容 | 现有源值 | 离线候选说明 |
| --- | --- | --- |
| 模块身份 | `m4.recipe.smelter.iron_ingot` | 待制作 `MODULE_ID = "example.m4.recipe.smelter.iron_ingot.v1"`；仅示例新 ID，实际世界仍须检查身份与依赖 |
| 配方身份 | `recipe.smelter.iron_ingot` | 待制作 `RECIPE_ID = "example.recipe.smelter.iron_ingot.v1"`，与下面 Profile 同值；不覆盖原 ID |
| 行为参数 | 每批 `iron_ore: 4`、`carbon_fuel: 1`；产出 `iron_ingot: 3`；副产物 `slag: 1`；电力 `8`；计划 `duration_ticks: 1` | 完整保留；数量改变须改行为源并重新构建，不能只改 Profile |
| 模板匹配 | `REQUIRED_FACTORY_MARKER = "smelter"`；emit kind `economy.recipe_execution_plan` | 模板检查工厂 ID 小写后包含 marker；这不是工厂 ownership/资格授权，也不由 Profile 偏好标签代替 |
| 原有元数据 | bottleneck `iron_ore`；stage `bootstrap`；preferred factory `smelter` | 取自 [bootstrap 默认 recipe profiles](../../../crates/oasis7_runtime/src/world/bootstrap_economy.rs) 的同一配方；不新增材料替代授权 |
| 依赖 | 既有输入/产出物料、工厂与阶段条件 | 实际世界须查询；样例没有创建物料、工厂实例、库存或电力，不能假定每个世界都有这些依赖 |

下面是完整的 **Profile 变更子对象**，可独立做 JSON 和字段比对；它不是 `ModuleReleaseSubmit` 的完整参数，也不是可提交发布包。空的 product/factory 数组表示本演练不发布这两类 Profile；已有定义不能为了“包完整”再次放入。

```json
{
  "product_profiles": [],
  "recipe_profiles": [
    {
      "recipe_id": "example.recipe.smelter.iron_ingot.v1",
      "substitution_rules": [],
      "bottleneck_tags": ["iron_ore"],
      "stage_gate": "bootstrap",
      "preferred_factory_tags": ["smelter"]
    }
  ],
  "factory_profiles": []
}
```

此对象没有源包、manifest、artifact、真实 world/request/target、批准或签名；这些缺项必须由实际制作和既有 authority 填齐。`substitution_rules: []` 不允许替代原料；`stage_gate` 和标签不提供库存、工厂所有权或模块实例。若目标世界已存在这个示例 ID，应按 §5 停止覆盖，而不是因为它带有 `example` 就放行。

**可手工复核的行为预期。** 在 recipe ID 和模板工厂匹配等前置成立的纯参数比较中，申请 2 批、可用铁矿 8/碳燃料 2/电力 16，模板预计接纳 2 批，消耗 8/2、产出铁锭 6/炉渣 2、请求电力 16，计划 `duration_ticks` 仍为 1；仅将可用电力改为 8 时接纳 1 批、消耗 4/1、产出 3/1、电力 8。资源不足一批时返回拒绝原因，`desired_batches = 0` 也拒绝。以上是对现有模板的静态推导，未运行模块；计划时长不是整条工业工作实际完成时刻，计划也不直接扣料或产出资产。

**演练交付结果。** 已交付内容只有来源定位、候选身份、参数、完整 Profile 子对象和复核方法。后续可由作者在授权的制作环境准备真实源包，由原专业流程补齐构建、四阶段证据和实际消费者验证；当前可选择继续编辑说明、缩小目标、延期或回到既有合法生产路线。不能把本演练的技术发布方法当成缺少四阶段证据时的正式采用通道。

## 9. 创作者可读状态与实际采用

<a id="ppe-creator-status-copy"></a>
### 9.1 状态、失败原因和下一步文案

以下是现有事实应表达的正文，不宣称已有专用界面或新增状态枚举。引用真实 request ID 时同时给世界、实际 module/version/artifact、目标及读回位置/时刻；缺事实显示“尚未取得”。动作响应与 request 状态分别读取，不用本地按钮、等待时长或缓存推导成功。

| 可核对事实 | 给作者的正文 | 下一步及边界 |
| --- | --- | --- |
| 只有本地材料，未读到请求 | “材料已准备；尚未确认发布请求。缺少的材料：按实际交付包列出。” | 补齐实际源、依赖和证据；可能已提交但结果未知时先查询，不能自动复制提交 |
| `Requested` | “发布请求已记录，尚未完成内容校验与承诺。” | 读取已提交 payload，依既有入口进行 Shadow；不占有生产资格 |
| `Shadowed` | “本次 manifest 与 Profile 已形成校验承诺；工业模拟和试点证据须另行核对。” | 查看 required roles 与所需发布证据；只展示实际缺项，不把 Shadow 改称模拟通过 |
| `PartiallyApproved` | “已收到部分角色批准，仍缺少请求要求的批准。” | 按请求实际集合列出已满足/缺少项；不代填默认批准，不宣称将必然通过 |
| `Approved` | “请求所需角色批准已齐；应用时仍复核当前权限、内容和世界前置。” | 使用既有合法应用入口；当前 owner/role/证据漂移仍可能拒绝，批准不预留权限 |
| `Applied` 及对应 committed 事件 | “本次技术发布已提交；Profile 与模块结果见此次读回。实际使用条件及工业准入证据仍需核对。” | 对照 §9.2；激活、目标、资源与四阶段条件分别确认，不能显示“已可生产/已获永久许可” |
| `Rejected` 且有 `ModuleReleaseRejected` | “该发布请求已被拒绝。原因：按实际拒绝记录显示。” | 保留原 request 与结果；修正材料或条件后按既有流程重提，不把终态请求恢复成新批准 |
| 本次 `ActionRejected` | “本次动作未被接受。原因：按实际事件显示。请求当前状态：按重新读回显示。” | 校正原因并复核当前状态；这类事件本身不把请求改为 `Rejected`，也不抹去以前的发布事实 |
| 读面断开、陈旧或结果不明 | “尚不能确认最新结果；下面显示的是已知读回及其时刻。” | 恢复读取并按原 request/事件核对；不按超时猜测 Applied/Rejected，不以重复提交恢复 |
| §6 的 exact artifact 准入冻结 | “该工件停止新的发布/激活；既有 Profile、资产和历史仍保留。” | 显示确切冻结范围与原因；既有消费仍由其当前合同判断，旧批准不能解冻 |

`ActionRejected` 是动作层结果；[状态提交实现](../../../crates/oasis7_world_state/src/state/module_release_transition.rs)处理 `ModuleReleaseRejected` 才将该请求置为 `Rejected` 并保存 `rejected_reason`。Profile 冲突、owner/role 漂移或 Apply 失败的展示必须读取两者，不能把“本次无新发布效果”写成“历史请求和资产已删除”。

针对作者最常见的三类失败，正文补足可执行下一步：ID 已存在时说明“保留原定义，核实是否需要新的内容身份”；审核后 payload 改变时说明“本次提交材料与已审核承诺不同，整理新候选重新审查”；角色或所有者变化时说明“当前授权条件未满足，先取得当前合法条件再请求处理”。这些建议不绕过冻结、不承诺用户可自行授权，也不保证重提必然成功。

<a id="ppe-creator-adoption"></a>
### 9.2 从发布结果到实际使用的独立核验

交接必须让使用者沿相同候选找到下列事实。观察层级可以逐项未完成；不得把一个绿灯折叠为“已经能玩”。对新工业能力，GAME-018 的阶段证据是正式采用前置，下面技术核验不能替代它。

| 核对层 | 可观察结果与直接消费者 | 不足时给使用者的下一步 |
| --- | --- | --- |
| 发布定义 | 同一 release 的 committed module/Profile 事件；实际 product/recipe/factory map 与 payload 一致；[reader](../../../crates/oasis7_runtime/src/world/resources.rs) | 缺提交或读回不一致时保留请求与证据，交原发布 owner 诊断，不手动补 map |
| 安装与激活 | 同一 module/version/artifact 的实际 registry/实例、目标与 active 状态，符合 [lifecycle](module-lifecycle.md#sr2-activation-contract) | Profile 存在但目标未安装/未激活时显示该缺项；不声称 `activate` 意图等于已激活，也不另开权限旁路 |
| 合法使用前置 | 工业四阶段证据与实际作用范围；当前使用者资格、工厂 owner、物料账本、电力和阶段等消费者前置 | 缺前序证据则停在交接；缺资源/合法工厂则补齐实际条件或使用既有路线，不自动造资源或扩大 scope |
| 配方调用 | [`ScheduleRecipeWithModule`](../../../crates/oasis7_runtime/src/world/economy.rs)取实际工厂 input ledger 与 builder 电力，调用 exact module；核对拒绝原因或接纳批次 | 模块不兼容、ID 不符、资源不足或实际权限拒绝时保留原因；改材料/范围需按当前合同重验，不能只调整标签宣称已修复 |
| 世界生产结果 | 模块计划进入普通 `ScheduleRecipe` 的后续校验；区分调度接受、进行中的工作与实际完成/产出等权威事件及账本变化 | 模块 emit 或调度成功未证明产出到账；继续观察实际工作/结果，失败按其专业合同恢复，不重复发货或领奖 |
| 其他内容类型 | 产品变更沿 `ValidateProductWithModule` 的实际结果，设施变更沿 `BuildFactoryWithModule` 与普通建设消费者核验 | 铁锭 recipe 样例不证明产品校验、工厂建设、库存合法性或市场/许可链路已通过；相关作者另交同候选证据 |

共享模板只返回计划 emit，`effects` 为空；即便拿到预计产量，也没有据此产生物品。反过来，保留的 Profile 也不保证恢复后的旧 WASM 接受它。发布说明应让作者同时看到“已确认的事实、尚缺的条件、现有事实的保留方式”，避免把部分可见性升级为全局采用承诺。

<a id="ppe-creator-change"></a>
## 10. 变更传播与后续交接

| 发生的变更 | 作者/发布者应更新的材料 | 失效或仍须核对的内容 |
| --- | --- | --- |
| 行为源、模板、依赖或构建输入变化 | 可编辑源闭包、参数差异、受控构建输出与身份、依赖及风险说明 | 原构建证明不能证明新 bytes；消费结果需按新候选复核，共享模板变化须列实际受影响模块 |
| 待决 Profile 内容或 manifest/激活输入变化 | 新候选完整 payload 及与旧材料的差异，依既有流程重新提交/Shadow/审核 | 旧承诺与批准不转移到新内容；同数组换序按现有归一化处理，不能自造 canonical hash |
| 已 committed Profile 需要修正 | 保留旧定义与历史，说明新内容身份及依赖迁移需求，交原专业 owner 评估 | 当前不支持同 ID patch，也不承诺新 ID 自动替代旧内容、转移资产或解除冻结 |
| 当前 owner/role/实际世界前置变化 | 更新真实授权与读回定位，按当前合同重新复核 | 历史批准和 receipt 仍可审计但不保留新发布权；补证不等于授权已恢复 |
| 回滚或 exact artifact 被冻结 | 保留原 release/工件/已提交 Profile 与资产，交付恢复目标、冻结范围、原因和消费者兼容信息 | 按 §6 只冻结新的准入；不删除历史，不将所有旧实例或产品一概标为不可用 |
| 来源或素材使用权材料变化 | 按[来源与权利](../../game/gameplay/content-production-and-rights.design.md#source-and-rights)重新交接实际证明与适用范围 | 不据此自动改写世界工业许可、已结算收益或历史；有范围许可与版税仍归 GAME-018/原专业 authority |

每次交接沿[同一候选的变更关系](../../game/gameplay/content-production-and-rights.design.md#change-and-candidate)记录受影响材料与消费者。重新制作或验证的范围取决于实际变更，不要求每次文本修正重跑整个工业链；但任何影响已审核 payload、行为或使用条件的变更都不得沿用不再对应的证据。

## 11. 验证边界

具体方法见[配套手册](player-published-entities.manual.md#ppe-manual-scope)。静态审读与离线算例可验证来源、参数和材料组织；真实构建、发布、游戏调用、正式工业四阶段和三节点验收各有独立前置与证据，不由本轮文档检查代签。

### 11.1 验证映射表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source 或 ID、scenario/layer、candidate/environment 要求或选择规则 | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [REQ-WR-GR-002](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-002)；Trace refs: O7-REQ-ce8c31b1-5294-4acd-a65f-2012bcbef911 | [发布 authority](player-published-entities.design.md#ppe-publication-authority) | 专业依据：[发布验收](player-published-entities.prd.md#ppe-release-acceptance)；AC-1/4/6/7/8：payload 与权限漂移原子拒绝 | required：同一 candidate tree 的 commitment、owner 与 role 正/负例，比较 module/profile 状态与 receipt；[release commitment source](../../../crates/oasis7_runtime/src/tests/module_action_loop_regressions.rs) | task issue fresh WASM/QA logs | 本表不宣称测试通过；三节点 SLA 另需 full |
| [REQ-WR-GR-003](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-003)；Trace refs: O7-REQ-ac12295b-63a5-4440-9924-deaedafd7965 | [有限回滚](player-published-entities.design.md#ppe-rollback-disposition) | 专业依据：[发布验收](player-published-entities.prd.md#ppe-release-acceptance)；AC-5/9：冻结准入、历史保全、失败原子性与恢复 | required：rollback 后各准入 action 与既有 profiles/instances；full：snapshot restore 与 replay 同一状态；[rollback admission source](../../../crates/oasis7_runtime/src/world/rollback_admission_freeze_test.rs) | task issue fresh runtime/QA logs | 本表不证明跨节点、生产环境或更丰富授权模型 |
| [REQ-WR-GR-009](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-009)；Trace refs: O7-REQ-3d713ab7-87c2-41df-9c89-827859c23313 | [作者输入与阶段边界](player-published-entities.design.md#ppe-creator-inputs) | AC-WR-GR-009 的来源/假设/成本与阶段缺口交接，不将 Shadow 提升为模拟/准入 | [PPE-M-07](player-published-entities.manual.md#ppe-manual-reading)：实际读者复述阶段、缺证与安全下一步，记录所见版本、原话和误读；静态独立审读不代签真人体验 | task issue / PR 的静态核对及人工审读记录，分别写执行与未执行 | 不证明真实源包、构建、模拟/试点、四阶段强制顺序、玩家体验或正式工业准入已通过 |
| [REQ-WR-GR-009](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-009)；Trace refs: O7-REQ-3d713ab7-87c2-41df-9c89-827859c23313 | [离线铁锭样例](player-published-entities.design.md#ppe-creator-example) | AC-WR-GR-009 的具体内容来源、参数与 Profile 子对象可核查，不把离线算例提升为模拟/准入 | [PPE-M-01/02](player-published-entities.manual.md#ppe-manual-offline)：同一固定 source tree 核对 JSON、ID、参数与源输入，记录标量推导、材料缺口和未执行项 | task issue / PR 的静态核对及人工审读记录，分别写执行与未执行 | 不证明真实源包、构建、模拟/试点、四阶段强制顺序、玩家体验或正式工业准入已通过 |
| [REQ-WR-GR-002](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-002)；Trace refs: O7-REQ-ce8c31b1-5294-4acd-a65f-2012bcbef911 | [交付包](player-published-entities.design.md#ppe-creator-delivery) | 发布验收 AC-1/2/4/6/7/8：完整交付包关联同一候选的源、当前授权、实际 payload、工件与待补证材料 | [交付与消费者核验](player-published-entities.manual.md#ppe-manual-release)：PPE-M-03：核源包闭包、实际构建/身份/权限与四阶段材料，缺前置不执行写入；标明实际 dev-test 或 production 环境，不以 mock 代生产证据 | task issue 的交付清单、fresh 专业 logs 与实际 readback；本轮只记录定位和静态审读 | 不证明缺少四阶段证据的正式采用、三节点 SLA、未执行类型的消费者或专用 UI 已交付 |
| [REQ-WR-GR-002](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-002)；Trace refs: O7-REQ-ce8c31b1-5294-4acd-a65f-2012bcbef911 | [状态文案](player-published-entities.design.md#ppe-creator-status-copy) | 发布验收 AC-1/2/4/6/7/8：动作结果、发布单状态与未知/陈旧读面分别准确表达，不从批准或失败推断世界效果 | [交付与消费者核验](player-published-entities.manual.md#ppe-manual-release)：PPE-M-04：对照原 request/动作/事件核 ActionRejected、ModuleReleaseRejected、当前授权和状态文案；标明实际 dev-test 或 production 环境，不以 mock 代生产证据 | task issue 的交付清单、fresh 专业 logs 与实际 readback；本轮只记录定位和静态审读 | 不证明缺少四阶段证据的正式采用、三节点 SLA、未执行类型的消费者或专用 UI 已交付 |
| [REQ-WR-GR-002](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-002)；Trace refs: O7-REQ-ce8c31b1-5294-4acd-a65f-2012bcbef911 | [采用核验](player-published-entities.design.md#ppe-creator-adoption) | 发布验收 AC-1/2/4/6/7/8：release/Profile 定义、安装/激活、合法工业采用与各类型实际消费者分别核验 | [交付与消费者核验](player-published-entities.manual.md#ppe-manual-release)：PPE-M-05：同一候选核 Profile readback、安装/激活、合法前置、计划/调度/产出及各类型消费者；标明实际 dev-test 或 production 环境，不以 mock 代生产证据 | task issue 的交付清单、fresh 专业 logs 与实际 readback；本轮只记录定位和静态审读 | 不证明缺少四阶段证据的正式采用、三节点 SLA、未执行类型的消费者或专用 UI 已交付 |
| [REQ-WR-GR-003](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-003)；Trace refs: O7-REQ-ac12295b-63a5-4440-9924-deaedafd7965 | [变更传播](player-published-entities.design.md#ppe-creator-change) | 发布验收 AC-5/9 的变更与回滚交接保留准确来源、冻结及既有事实 | [PPE-M-06](player-published-entities.manual.md#ppe-manual-change)：同候选前后状态、exact tuple、profile/实例/资产/receipt 对照；required 引用既有 rollback regression，恢复/replay 和跨节点组合仍需 full | task issue 的差异表、专业测试或实际观测记录，注明环境及未执行项 | 不证明许可生命周期、原子替代、既有消费者冻结、所有内容兼容或 full 验收通过 |

## 12. 决策与残余风险

采用 append-only 发布及仅冻结新增准入的有限回滚，理由与边界见 §§5–6；不以旧批准替代当前授权，不删除已发生的 profile、资产和历史。[有限 recipe 授权设计](../runtime/governed-extension-lifecycle.design.md#des-wr-gr-lifecycle-001)另承接其有界期限、范围、替代和退出；其他扩展类型及真实共同消费者仍须由 runtime/WASM owner 核实，不能由本专题有限回滚代签；§11 的定位与计划不构成实际通过证据。

作者输入和交付包只承接 GR-009 的内容交接子范围。完整确定性工业模拟、有限试点的状态/限额/退出、阶段强制顺序、成本/损失读面及跨入口组合，由[工业四阶段执行设计](../runtime/industrial-creation-simulation-and-admission.design.md)消费 GAME-018 并独立承接；GR-010 的有限许可、版税与开放由[许可结算设计](../runtime/industrial-license-and-royalty-settlement.design.md)承接。它们没有把本专题 Shadow/Apply 改成工业模拟/准入，也没有扩张 exact FROM 回滚对既有实例的权限。真实实现与组合证据仍须由原 producer/gameplay/runtime/WASM/QA 完成；不能以手工批准或新 ID 绕过。发现/分享市场、专用编辑器和图形资产分发仍不在本交接范围。
