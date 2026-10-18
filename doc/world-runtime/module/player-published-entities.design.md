# oasis7 Runtime：玩家发布制成品的 WASM 模块与 Profile 治理闭环设计

- 对应需求文档: `doc/world-runtime/module/player-published-entities.prd.md`
- 当前任务状态与历史变更：GitHub task issue evidence 与 Git history。

<a id="1-设计定位"></a>
## 1. 设计定位与目标
定义玩家发布制成品的 WASM 模块与 Profile 治理闭环，统一玩家产物发布、模块约束与身份治理。

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

## 3. 关键接口 / 入口
- 玩家发布实体模型
- WASM 模块/Profile 绑定
- 治理审核入口
- 发布审计/回滚记录

## 4. 约束与边界
- 玩家发布内容不得绕过治理审核。
- 实体、模块与 profile 绑定必须可追溯。
- 不在本专题扩展完整 UGC 市场机制。

<a id="ppe-publication-authority"></a>
## 5. 发布承诺、当前授权与权威结果
- Profile admission 为 append-only：新发布必须使用尚不存在的 product/recipe/factory ID，Shadow 与 Apply 均复核；同 ID 同内容也不是幂等发布，不提供 patch 绕过。
- Shadow commitment 必须绑定被审核的完整 manifest、module changes 与 profile changes 内容；Approve 与 Apply 消费同一承诺。改动任一 profile 内容不得继承旧审核，失败不得产生部分 module/profile 效果。采用可选、None 时不序列化的 v1 commitment，覆盖完整 ModuleManifest 与按 product_id/recipe_id/factory_id 归一化排序的完整 profile vectors；Shadow 计算，Apply 重算核对。新增字段缺失不改变历史 event bytes；legacy 空 profile 待决请求保留旧路径，legacy 带 profile 且无 commitment 的待决请求 fail closed，须显式重新提交；历史已 committed 事件不追溯改写。
- Approved 只表示当时获得受限准入资格。Shadow/Apply executor 沿用当前已登记 agent 资格，不新增 operator role policy；发布 authority 独立校验，Apply 必须在当前 authority 下复核 request submitter 仍是 exact artifact owner、每个 required-role approver 仍有该 role binding，以及所需角色与世界前置；旧批准、缓存或历史 receipt 不保留新发布权。授权到期、范围变动和原子替代的更完整模型为后续专业合同，不把目标合同表述为现有机制。
- 只有权威提交的 release 与 profile governed 事件确认本次世界效果；提交、构建、Shadow、Approved 不表示已可用。拒绝事件是失败原因，不是成功 receipt；重读历史结果不复制效果或获得新权限。

<a id="ppe-rollback-disposition"></a>
## 6. 回滚的有限处置
- 本闭环选择“仅停止新的发布/激活”：恢复旧模块与被回滚发布单的新增发布/激活冻结必须形成同一原子世界结果，并保留可审计的原因、来源和 disposition。旧批准或 committed receipt 不自动解冻或再次激活。
- 已 committed profile 定义、独立资产、来源与历史事件保留，不能把已发生的发布解释为从未发生。冻结须绑定确切被回滚的 release identity，不由版本差异推断。保留的 profile 可继续被既有 consumer 查询与校验；恢复旧 WASM 不保证每个 profile 均通过其校验，不兼容时沿用 consumer 的明确拒绝，不引入按模块版本切换 profile。既有能力的后续行为仍按当前专业合同校验；本次回滚不新增经济消费限制、没收、同 ID 替换或一般退出机制。
- 回滚失败必须保留回滚前 module/profile/准入事实；重复、snapshot restore 与 replay 只能重读同一结果，不复制恢复、发布或经济效果。
- 旧快照缺少可证明的 release/profile 关联时，不猜测归属或删除 profile；兼容处理须保全历史并给出明确准入处置。rollback receipt 同时落账不可变 admission-freeze marker，作用域为被移除的 FROM release 的 exact `(module_id, from_module_version, from_wasm_hash)`。该 artifact identity 是有意选择的冻结范围：重新建立 request 但复用同一 tuple 仍被拒绝；不同 artifact tuple 不自动受其冻结。既有 `ModuleRollbackApplied.wasm_hash` 继续表示恢复目标 TO artifact，不用作冻结键；新增可选 `from_wasm_hash` 与 `from_release_request_id`，后者仅在唯一 exact Applied request 可归属时填写，缺失或多重匹配不猜测。durable marker 保留 scope tuple、rollback proposal、可证明的来源 linkage 与稳定原因 `rollback_stop_new_admission`，receipt/readback 明确既有事实保留。registration/direct install/direct upgrade/activation/republication 以及 rollback 的 TO-target activation 均读取同一 durable marker 拒绝被冻结 tuple 的新准入；无模块 tuple 的 raw artifact upload 本身不产生世界 publication，不在此冻结范围，existing instances 不因 marker 自动停用。marker map 为空、可选字段为 None 时省略，旧历史事件不重编码为新批准。

## 7. 产品对齐与验收
[`受治理的区域能力与扩展`](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md) §2.1/§2.2 是产品结果 authority。本专题把逐次重验、权威 receipt、历史保全与明确限制结果投影到发布闭环；不改变该产品 authority。

world-first：发布与恢复由权威世界提交；emergence-first：创作者在受限准入内添加能力；persistent：profile、资产与 receipt 历史不因回滚消失；auditable：完整 payload、准入与恢复结果可追溯；extensible：后续 patch/授权/退出机制可扩展，但必须保持这些边界。

`test_tier_required` 至少证明 profile payload 漂移拒绝、当前角色漂移拒绝、回滚准入冻结/历史保全与失败原子性；`test_tier_full` 覆盖持久化恢复/replay、跨节点一致性与既有 SLA。验收目标不构成实现或通过声明，fresh 专业证据由绑定 task issue 承载。

## 11. 验证边界
### 11.1 验证映射表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source 或 ID、scenario/layer、candidate/environment 要求或选择规则 | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [REQ-WR-GR-002](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-002)；Trace refs: O7-REQ-ce8c31b1-5294-4acd-a65f-2012bcbef911 | [发布 authority](player-published-entities.design.md#ppe-publication-authority) | 专业依据：[发布验收](player-published-entities.prd.md#ppe-release-acceptance)；AC-1/4/6/7/8：payload 与权限漂移原子拒绝 | required：同一 candidate tree 的 commitment、owner 与 role 正/负例，比较 module/profile 状态与 receipt；[release commitment source](../../../crates/oasis7_runtime/src/tests/module_action_loop_regressions.rs) | task issue fresh WASM/QA logs | 本表不宣称测试通过；三节点 SLA 另需 full |
| [REQ-WR-GR-003](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-003)；Trace refs: O7-REQ-ac12295b-63a5-4440-9924-deaedafd7965 | [有限回滚](player-published-entities.design.md#ppe-rollback-disposition) | 专业依据：[发布验收](player-published-entities.prd.md#ppe-release-acceptance)；AC-5/9：冻结准入、历史保全、失败原子性与恢复 | required：rollback 后各准入 action 与既有 profiles/instances；full：snapshot restore 与 replay 同一状态；[rollback admission source](../../../crates/oasis7_runtime/src/world/rollback_admission_freeze_test.rs) | task issue fresh runtime/QA logs | 本表不证明跨节点、生产环境或更丰富授权模型 |

## 12. 决策与残余风险

采用 append-only 发布及仅冻结新增准入的有限回滚，理由与边界见 §§5–6；不以旧批准替代当前授权，不删除已发生的 profile、资产和历史。更丰富的授权期限、范围、替代与退出仍未覆盖，由 runtime/WASM owner 在扩展合同或消费者变化时复核；§11 的定位与计划不构成实际通过证据。
