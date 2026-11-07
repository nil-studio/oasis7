# 玩家工业内容作者交付与采用核验手册

- 设计来源：[玩家发布制成品设计](player-published-entities.design.md)。
- 专业验收：[发布验收 AC-1–9](player-published-entities.prd.md#ppe-release-acceptance)。
- 上游工业阶段：[GAME-018 AC-01](../../game/gameplay/gameplay-industrial-creation-and-cross-region-market-contract.prd.md#ac-game-018-01) 与 [AC-WR-GR-009](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#ac-wr-gr-009)。
- 记录位置：实际 Task/PR 的交付材料与执行 evidence；本文件保存方法，不保存可变任务状态或声明验收通过。

<a id="ppe-manual-scope"></a>
## 1. 使用范围与开始条件

本手册让作者、发布者和使用者检查同一候选的交接材料。PPE-M-01/02 是只读源核对与离线参数复核；PPE-M-03 的材料检查也可只读完成。PPE-M-04/05/06 涉及真实发布、消费者或回滚的部分必须在原任务已授权的适用环境，由既有专业入口执行并保存实际结果。PPE-M-07 是内容可读性审读。列出方法或定位到测试不等于执行或通过。

开始时记下实际 source tree、设计/材料版本、核验者、日期和范围。每条运行记录另写环境、世界身份、request/module/version/artifact、目标与状态读回位置；它们来自真实结果。尚无世界或构建产物时明确写“未取得”，不填测试 hash、伪签名、随意 target 或生产声明。

本手册使用设计 §8.3 的离线铁锭样例说明检查方法。该样例没有实际源包、WASM 工件或世界状态。不得直接执行发布，也不得把下面的技术验证路径用作缺少提案、确定性模拟、受限试点和治理准入证据时的正式工业采用通道。

## 2. 交给下一位作者和接收者的材料

交接者按[设计交付包](player-published-entities.design.md#ppe-creator-delivery)逐项填实际位置、版本、缺项和下一处理人；记录放在既有 Task/PR 或该内容的真实交付位置。接收者应能找到以下四组材料，不需要创建统一文件格式：

1. **能继续制作的输入**：创作意图、行为源、模板、Cargo manifest/lock、SDK/其他依赖、source inventory、使用权与署名定位。复制单个 `.rs` 不算源输入闭包。
2. **能准确审查的候选**：完整 Profile changes、实际 ModuleManifest、激活意图与安装目标、原参数与差异、依赖、风险、四阶段已有证据及未完成项。
3. **能核实来源和提交的结果**：真实构建产物及既有验证结果、artifact 身份、当前授权、请求/承诺/批准、权威 release 与 Profile 事件。生产证据遵守原构建和发布合同，不能由本文样例生成。
4. **能判断是否使用的说明**：当前已确认状态、未满足的工业准入/安装/激活/资源/所有权条件、消费者入口、失败原因、恢复方式与变更影响。

四组材料互相引用同一候选。缺少工件时仍可以交付前两组的已完成部分；交接结论必须说明“内容说明可审读，真实制作/发布/正式采用待完成”，不得把不完整包标为可发布。

<a id="ppe-manual-offline"></a>
## 3. PPE-M-01/02：离线来源、字段和参数复核

### PPE-M-01：源输入和 Profile 子对象

在待核对的仓库根读取下列来源；固定基线审读可用 `git show <实际 commit>:<路径>`，不要以正在变化的另一个 worktree 代替候选。

| 应读取的源 | 独立核对点 |
| --- | --- |
| [铁锭行为源](../../../crates/oasis7_builtin_wasm_modules/m4_recipe_smelter_iron_ingot/src/lib.rs) | 原 module/recipe ID；输入 4 铁矿、1 碳燃料，输出 3 铁锭、副产物 1 炉渣，电力 8，计划时长 1，marker `smelter` |
| [recipe 共享模板](../../../crates/oasis7_builtin_wasm_modules/_templates/m4_recipe_module_template.rs) | recipe 精确匹配、工厂 ID marker、批次大于 0、按可用输入/电力限制接纳批次、空 effects 与计划 emit；时长不是批次数乘数 |
| [Cargo manifest](../../../crates/oasis7_builtin_wasm_modules/m4_recipe_smelter_iron_ingot/Cargo.toml) 及同目录 `Cargo.lock` | includes 与 SDK path dependency；改名后构建输入闭包仍对应实际文件，包的版本不是发布成功证明 |
| [默认经济 Profile](../../../crates/oasis7_runtime/src/world/bootstrap_economy.rs) | `recipe.smelter.iron_ingot` 的 bottleneck `iron_ore`、stage `bootstrap`、preferred factory `smelter`；确认读到该配方而非相邻条目 |
| [Profile 类型](../../../crates/oasis7_wasm_abi/src/economy.rs)、[ModuleProfileChanges](../../../crates/oasis7_agent_api/src/world_action/action.rs) | 三组数组和具体字段；空 `substitution_rules` 不授权材料替代；RecipeProfile 没有输入产量/电力等行为字段 |

按以下步骤记录实际结果：

1. 从[设计 §8.3](player-published-entities.design.md#ppe-creator-example)读取唯一 JSON 代码块，使用标准 JSON 解析器验证它是对象；核对 `product_profiles`、`recipe_profiles`、`factory_profiles` 三组数组。
2. 核对只有一个 recipe、新 ID 为 `example.recipe.smelter.iron_ingot.v1`，其余字段与上表真实旧元数据一致；product/factory 数组均为空。新 module ID 为 `example.m4.recipe.smelter.iron_ingot.v1`，不等于原内置 ID。
3. 检查示例没有伪造完整 manifest、WASM bytes/hash、签名、world/request/target。空数组只表示不发布该类 Profile，不证明依赖已存在。
4. 独立从源读取参数及字段后与说明比较，不能只比较两份文档中重复的数值。源或 schema 改变时报告差异，更新受影响说明后重新核对。
5. 将“文档样例 ID 与旧内置 ID 不同”和“实际目标世界允许新 ID”分开记录；后者必须查目标世界，离线检查不能证明。实际 ID 若已存在，即使内容相同也按原合同拒绝覆盖。

预期结果是材料和来源一致，读者知道仍缺什么。核对到现有源包不表示新的 example 源包已经制作。许可检查沿[来源与权利](../../game/gameplay/content-production-and-rights.design.md#source-and-rights)记录实际依据；不要把 Cargo 的 `license` 字段解释为玩家工业许可证、版税或市场资格。

### PPE-M-02：可复核的行为预期

这是源规则的纯参数手算，前提仅为 recipe ID 和模板 marker 匹配；不创建工厂目标，不使用假世界快照，不执行 WASM，不证明工业确定性模拟。分别从源读取每批输入/输出/电力，在纸面或普通算术工具中计算下表。

| 参数比较 | 预期接纳/计划内容 | 说明 |
| --- | --- | --- |
| 申请 2 批，铁矿 8、碳燃料 2、电力 16 | 接纳 2；消耗铁矿 8/燃料 2；产出铁锭 6/炉渣 2；电力 16；`duration_ticks = 1` | 接纳数为申请与各输入/电力可支持批数的最小值 |
| 保持上行输入，仅电力改为 8 | 接纳 1；消耗 4/1；产出 3/1；电力 8；`duration_ticks = 1` | 不是“部分成功已产出”，只是假设输入下的计划 |
| 申请 1 批，铁矿 3、燃料 1、电力 8 | 接纳 0，材料不足拒绝 | 不将缺口补成默认库存 |
| 申请 1 批，铁矿 4、燃料 1、电力 7 | 接纳 0，电力不足拒绝 | 不从 Profile 阶段或标签推导电力 |
| `desired_batches = 0`，或 recipe ID 不匹配，或模板 marker 不匹配 | 返回相应拒绝计划 | 分别检查分支；参数算例不能代替真实 factory ownership/资格检查 |

记录数值、静态来源和推导；未运行时写“静态预期”。实际 runtime 还要从工厂账本与 builder 获取资源，将计划交普通 `ScheduleRecipe` 校验和执行；工时、调度成功及最终产出必须在各自事实层核对。

<a id="ppe-manual-release"></a>
## 4. PPE-M-03/04/05：交付包、发布状态与消费者

### PPE-M-03：真实交付包的同一候选检查

适用输入是真实准备交接的候选；只有离线样例时，对未制作项记录缺失，不继续执行。

| 检查 | 具体方法 | 未满足时的结果 |
| --- | --- | --- |
| 源输入闭包 | 对照实际源码、includes/path dependencies、Cargo.lock 与 source inventory；接收者能定位并继续编辑 | 源包未完整交付；补齐真实输入，不能拿旧工件代替新源码 |
| 行为与元数据对应 | 将 recipe/module 身份与实际实现、Profile payload、依赖逐项关联；核对所改数值实际由哪一层拥有 | 记录不一致与受影响消费者；不通过改标签假装改变行为 |
| 构建与身份 | 沿[构建合同](../wasm/wasm-deterministic-build-pipeline.design.md#sr2-build-contract)及[节点接收合同](online-module-release-legality-closure-2026-03-08.design.md#sr2-release-node-evidence-receiver)，核对实际 bytes/proof 与对应验证记录 | 缺 proof、不可达或未验证时保持该结论；文件和 hash 一致性不是全部信任验证 |
| 当前世界与权限 | 从实际状态核对 requester、exact artifact owner、required roles/当前绑定、目标与当前合法发布前置 | 不继承别的环境或旧请求授权；缺权限由既有 authority 处理，不自行添加角色 |
| 四阶段证据 | 按 GAME-018 找到实际提案、同输入模拟、有限试点和有范围准入证据，分别记录缺口 | 技术交付可保留已完成部分；缺项不得批准新工业能力正式采用 |

受控 builder 的实际入口见 [`scripts/build-wasm-module.sh`](../../../scripts/build-wasm-module.sh) 与 [build suite 参数](../../../tools/wasm_build_suite/src/main.rs)；执行者应使用该候选的原构建合同。本文不提供带伪 hash/签名/target 的复制即发布命令，不把 builtin 同步脚本或 regression fixture 的假工件 bytes 当成玩家正式包。

### PPE-M-04：动作结果与请求状态分开核验

在明确授权的适用环境记录每个动作前后的 request 状态、完整 payload、module/Profile 状态、事件/receipt 和当前授权。只读审查可定位下表源和预期，但不得记为运行通过。

| 场景与设置 | 必须独立观察的结果 | 方法来源 |
| --- | --- | --- |
| 合法候选按既有发布入口提交、Shadow、角色批准和应用 | Requested/Shadowed/PartiallyApproved/Approved/Applied 的实际可达状态；各状态依当前读回，Applied 对应原子 release/Profile 结果 | [release 动作处理](../../../crates/oasis7_runtime/src/world/module_actions/release_request_support.rs)、[Apply](../../../crates/oasis7_runtime/src/world/module_actions/release_support.rs)、[publication](../../../crates/oasis7_runtime/src/world/module_release_publication.rs) |
| 目标世界预置同 recipe ID 的同内容 Profile，再尝试发布；另在 Shadow 后使该 ID 出现再 Apply | 同内容仍拒绝；请求状态与本次 ActionRejected 分别读回；本次失败无部分发布 | `module_release_shadow_rejects_existing_recipe_profile_even_when_payload_matches`、`module_release_apply_rechecks_and_rejects_existing_profile_overwrite`，位于 [regression source](../../../crates/oasis7_runtime/src/tests/module_action_loop_regressions.rs) |
| 审核后改动 Profile payload；另设置当前 owner 或批准人 role binding 漂移 | Apply 拒绝原因可读，module/Profile 状态不部分写入；旧批准不提供当前授权 | 同源 `module_release_apply_rejects_tampered_approved_profile_payload_atomically`、`module_release_apply_rejects_when_requester_loses_artifact_ownership`、`module_release_apply_rejects_when_approver_loses_current_role_binding` |
| 对有效请求制造动作拒绝；另通过既有合法拒绝入口形成 `ModuleReleaseRejected` | 前者不从事件名推断 request 为 Rejected；后者读回 Rejected 和 rejected_reason。保留原请求与历史事实 | [release state transition](../../../crates/oasis7_world_state/src/state/module_release_transition.rs) 的 `ModuleReleaseRejected` 分支，与 release 动作的 `ActionRejected` 分支对读 |
| 断开读面或只有旧 readback，再恢复读取 | 不将未知/陈旧结果显示为成功或终态失败；恢复后按原 request 与事件对齐 | [设计状态文案](player-published-entities.design.md#ppe-creator-status-copy)；真实入口的读取/重连观测，不以测试 fixture 代玩家 surface |

对拒绝的比较同时保存操作前后状态和动作原因。`ActionRejected` 证明本次动作被拒绝；请求是否为 `Rejected` 必须读实际状态及对应 release 事件。三节点 `p95 <= 60s` 是原验收目标；单次等待、超时或未执行该 benchmark 不能生成请求终态。

上述 regression 源含为特定逻辑构造的测试工件/证明，适合该层回归，不构成可发布 binary、生产签名、正式 finality 或工业四阶段证据。真实执行需按原测试规范选择同候选的 required/full 层，并记录实际命令与结果；本文不声称已执行这些测试。

### PPE-M-05：技术发布与实际使用分层核验

由 runtime/QA 与使用者在有合法前置的同一候选环境执行。新工业能力未具备四阶段证据时，停止在已授权的技术验证范围，不进行正式采用；测试结果保留原环境标签。

1. 从 release committed 结果和 [Profile reader](../../../crates/oasis7_runtime/src/world/resources.rs)核对实际新增 ID 与 payload。保持原 Profile、资产和历史定位，不能手动写 map 补全观察。
2. 核对实际 module/version/artifact、实例、安装目标和 active 状态；保留“发布定义存在但尚未安装/激活”的反例。使用目标来自实际世界，不能从示例名称构造。
3. 对 recipe 使用 [`ScheduleRecipeWithModule`](../../../crates/oasis7_runtime/src/world/economy.rs) 的真实调用。记录 requester、工厂 owner、实际 input ledger、builder 电力、recipe/module 及批次；资源不足、无合法工厂、错 module/recipe 的案例分别保留拒绝原因。
4. 比较模块输出计划与普通 `ScheduleRecipe` 后续结果；区分计划接纳、调度/工作状态和最终产出到账。分别定位权威事件和账本，不能用 emit 文本替代产物或结算。
5. 若候选还发布产品或设施，分别沿 `ValidateProductWithModule`、`BuildFactoryWithModule` 及其后续消费者验证；recipe 通过不能代签这些类型。其他路径没有执行就保留未执行状态。
6. 回到作者说明，让实际使用者找到“已确认、尚缺、失败原因、保留事实、下一步”。记录其与运行结果是否相符；界面未提供该内容时记录未交付，不用本文静态文案冒充当前 UI。

一个有效的分层反例是“Profile 查询存在，调用因未安装/不兼容/资源不足被拒绝”：应保留前一个事实，并具体解释后一个条件。不得把此反例改成发布记录丢失或自动重发。

<a id="ppe-manual-change"></a>
## 5. PPE-M-06：变更与有限回滚交接

先按[变更传播](player-published-entities.design.md#ppe-creator-change)比较真实候选前后的行为源、依赖、Profile、manifest、权限和使用条件，确定哪些原证据仍对应。只改变说明文字时不强行重新发布；改变审核 payload 或行为时不能沿用旧承诺/结果。

在授权的回滚验证环境，记录 FROM release 的 exact `(module_id, from_module_version, from_wasm_hash)`、恢复 TO 工件、原请求与可证明来源、Profile/实例/资产/历史状态，以及回滚结果。按以下独立义务核对：

- 恢复旧模块与 FROM tuple 的准入冻结是同一原子结果；恢复目标的 `ModuleRollbackApplied.wasm_hash` 不能错当冻结来源 hash。
- 保留 committed Profile、独立资产与历史；回滚失败不部分恢复，重复操作不复制效果。`from_release_request_id` 无唯一可证明匹配时保持缺失，不猜归属。
- 以同一被冻结 tuple 重新请求发布/激活仍被拒绝；旧批准或 receipt 不解冻。原合同没有把所有既有实例停用，也没有新增消费冻结。
- 实际消费者重新判断与恢复 WASM 的兼容性；成功和拒绝均按各自结果表达。不能因为 Profile 留存而许诺旧 WASM 一定接受，也不能从新准入冻结推断旧资产被没收。

已有 required 回归定位：[rollback admission source](../../../crates/oasis7_runtime/src/world/rollback_admission_freeze_test.rs) 和 `module_release_freeze_blocks_new_submit_and_old_approved_apply`（[source](../../../crates/oasis7_runtime/src/tests/module_action_loop_regressions.rs)）。snapshot restore、replay、跨节点与性能分别沿原 full 验收，源码定位或本次文字核对不代签运行证据。

记录格式沿既有 Task/PR：事实前后对照、exact tuple、原因、消费者结果、执行环境及未覆盖项。不新增通用许可撤销、同 ID 替代、资产迁移或冻结管理员。

<a id="ppe-manual-reading"></a>
## 6. PPE-M-07：作者与使用者可读性审读

选取实际参加创作交接或内容使用的人，分别给其完整材料与下列不完整情景；记录参与者角色、所见版本、原话、误读和修改位置。纯文档轮可以做作者独立复核，但不能代签真实玩家试玩或跨入口组合。

| 给读者的情景 | 需要从材料中自行回答的问题 |
| --- | --- |
| 只有铁锭离线样例 | 当前交付了什么？哪几类材料未制作？下一位作者从何处取得可编辑源和依赖？为什么不能直接发布或正式采用？ |
| 请求已 Shadowed，缺模拟/试点证据 | Shadow 证明什么？仍缺哪一阶段？继续补证、缩小范围、延期和回到既有能力各意味着什么？ |
| Approved 后动作被拒绝 | 本次动作结果与 request 当前状态是什么？哪项当前条件漂移？既有资产/历史保留什么？下一步如何核实？ |
| Applied 且 Profile 可查，但调用失败 | 发布、安装/激活、工业准入、资源条件和生产结果哪些已确认？这次失败应找谁、核对哪条事实？ |
| 回滚后旧 Profile 仍在 | 哪个 exact 工件停止新准入？历史和既有实例如何表达？为什么不能承诺所有消费成功或把所有资产标为失效？ |

判定依据是材料能否准确支持上述区分并给出有用下一步；记录实际误读，不发明通过率或可玩性评分。出现“Shadow 就是工业模拟”“Approved 已有生产权”“ActionRejected 一定是请求终态”“Profile 在就能生产”等回答时，定位其误读来源并修订对应内容后再审读。未获得真实参与者、实际入口或运行证据时，保留该缺口。

## 7. 交付结论的写法

结论分开填写实际完成的材料核对、专业执行和读者审读，并附同候选定位。例如，在只执行 PPE-M-01/02 的文档轮，只可写“已核对离线 JSON、示例身份和源参数；未制作真实源包、未构建或发布，四阶段执行与实际消费者未验证”。

发现输入/证据缺口后，给明确可继续工作的事项和原接收方：作者补源和说明，WASM 接收真实构建，runtime 接收状态和消费者，producer/gameplay/QA 补工业阶段与组合证据。后续技术交付再完整，也不能自动把部分 GR-009 内容承接升级为完整设计、正式准入、商业许可或 release readiness。
