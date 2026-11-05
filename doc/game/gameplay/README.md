# `game/gameplay` 热点子域入口

更新时间: 2026-11-05

## 从这里开始
- 想核对跨内容类型的制作范围、资产来源/权利、人员/外包与成本：读[内容制作与来源设计](content-production-and-rights.design.md)及[核对手册](content-production-and-rights.manual.md)。数量按源、内容单元和实际消费者区分；许可材料、产能与预算须分别取证。
- 想快速理解核心玩法骨架：先在 `../prd.md` 确认活跃基线与路由，再读 `gameplay-top-level-design.prd.md` 的 bounded 专业合同
- 想确认首局、前 10/30 分钟吸引力和持续游玩合同：先读 `../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md` 的产品承诺，再读 `gameplay-top-level-design.prd.md` 的 early-retention 专业合同；当前 verdict 由同候选 PR、实际验证记录与 `../../testing/evidence/` 确认。
- 想定位当前 native 工业配方、工厂类别、首局配置与有效初态，或交接内容变更：读[工业内容目录与交付](industrial-native-content-delivery.design.md)及[验证手册](industrial-native-content-delivery.manual.md)。目录范围与世界实际安装、准入、库存和生产资格分别核对。
- 想核对内置 WASM 的具体内容、Profile 默认值与安装组合：读[内置 WASM 内容与交付](builtin-wasm-content-delivery.design.md)及[验证手册](builtin-wasm-content-delivery.manual.md)。源包、构建工件、安装 helper 与正式世界采用分别核验，不把模块名称或测试调用当作完整玩法。
- 想制作世界前提、公共事件、个人结果与 Agent 消息的叙事内容：读[世界叙事内容与交付](world-narrative-content-delivery.design.md)及[验证手册](world-narrative-content-delivery.manual.md)。内容稿、运行事实与小说分别标识，原产品和玩法合同继续拥有题材及事实边界。
- 想了解经营决策、目标委托和回访的体验与实现边界：先读[首局产品设计的范围与优先顺序](../../product/world-rules-core-gameplay/first-session-and-continuation.design.md#gameplay-experience-priorities)，再读[组合系统设计](gameplay-decision-delegation-return.design.md)；真实合作与策略复用保持后续范围，不增加当前首产物或邀请测试门槛。
- 想确认间接控制为什么仍然应该让玩家感觉自己在控制：先读 `gameplay-indirect-control-agency-contract.prd.md`
- 想确认成熟世界里小玩家/新玩家靠什么继续形成独立价值：先读 `../../product/world-rules-core-gameplay/mature-world-progression.prd.md`，再读 `gameplay-mature-world-progression-contract.prd.md` 的专业合同；其中[有限供应用途](gameplay-mature-world-progression-contract.prd.md#regional-supply-use-policy-v1)区分原库存供给、匹配交付、接收方实际使用和首次成长，保留独立继续选择。
- 想确认区域 charter、tenure 与公共融资如何形成玩家可读、可恢复且不封锁独立路线的制度循环：先读 `../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md`，再读 `gameplay-regional-charter-tenure-funding-contract.prd.md`
- 想确认普通治理主体如何聚合、委托、纠错，以及 OC 转让为什么不能旁路事项权利：先读 `../../product/world-rules-core-gameplay/governed-common-decisions-and-constitutional-boundaries.prd.md` 的 `GCB-004/005`，再读 `gameplay-ordinary-governance-subject-and-oc-rights-contract.prd.md` 的 [`AC-GAME-019-04`](gameplay-ordinary-governance-subject-and-oc-rights-contract.prd.md#ac-game-019-04) / [`AC-GAME-019-05`](gameplay-ordinary-governance-subject-and-oc-rights-contract.prd.md#ac-game-019-05)
- 想确认工业能力如何从可审计提案走到确定性模拟、受限试点、治理准入与有期限开放，以及跨区市场如何从新鲜情报走到物理交付、escrow 里程碑和争议恢复：先读 [`GR-009..011`](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-009)、[`SC31-009/AC-SC31-010/011`](../../product/world-rules-core-gameplay/industrial-demand-goals-and-settlement.prd.md#req-sc31-009) 与 [`ES-004`](../../product/world-rules-core-gameplay/market-normal-state-and-emergency-supply.prd.md#req-wr-es-004)，再读 `gameplay-industrial-creation-and-cross-region-market-contract.prd.md`；GCB-005 的事项快照与额度桥消费转由上条 GAME019 AC04/05 承接，产品正文仍由对应 active authority 拥有
- 想确认下游容量不足时如何暂停/降载上游、持有输出并恢复未决流水线：读 `gameplay-industrial-backpressure-recovery-contract.prd.md`
- 想确认需求满足、取消、到期、未知或缩减后如何处置既有工业工作：读 `gameplay-industrial-demand-change-disposition-contract.prd.md`
- 想确认工厂维护真值、计划停机、带风险运行与既有工作的恢复边界：读 `gameplay-industrial-maintenance-planned-downtime-contract.prd.md`
- 想确认配方六态、准入/退役、旧工作处置和 successor 边界：读 `gameplay-industrial-recipe-lifecycle-decision-contract.prd.md`
- 想确认原材料不适配时如何比较换源、合法替代、换配方、减量/延期与恢复：读 `gameplay-industrial-raw-material-qualification-substitution-contract.prd.md`
- 想确认流水线服务窗口、lease 到期、hard/soft 迟到与掉电恢复：读 `gameplay-industrial-pipeline-service-window-contract.prd.md`
- 想确认 1cm 物理世界、动作粒度和表现夸张边界：先读 `../../product/world-rules-core-gameplay/prd.md` 的产品承诺，再读 `gameplay-top-level-design.prd.md` 的玩法合同
- 想确认访问模式、limited preview 或 release readiness：先读 `../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md`；当前 preview 执行状态由 Git、PR 与实际 CI 记录确认。已关闭 Round 1 的发布、渠道 fallback、信号与关闭证据从 Git history 和 GitHub issue `eng-cc/oasis7#48` 追溯
- 想确认 agent claim token cost、claim bond、upkeep、reclaim、restricted grant、starter OC 或 first chat gate：先读 `gameplay-agent-claim-economy-contract.prd.md`，再按需读 `gameplay-agent-claim-restricted-grant-liveops-runbook-2026-03-29.md`
- 想确认额外 Agent 的制造配方、报价、产能与失败成本：读 [Agent claim 经济合同](gameplay-agent-claim-economy-contract.prd.md)的制造政策，再读 [Runtime 制造设计](../../world-runtime/runtime/agent-industrial-manufacturing.design.md)。
- 想确认紧急保供触发、资格优先级、批次窗口、补偿与退出：读 [保供首版政策](gameplay-industrial-creation-and-cross-region-market-contract.prd.md#emergency-supply-policy-v1)，产品承诺仍由 [ES 专题](../../product/world-rules-core-gameplay/market-normal-state-and-emergency-supply.prd.md)拥有。
- 想确认组织有效活动、通知保护期限、接替资格、资产处置与部分交接：读 [组织连续性首版政策](gameplay-organization-continuity-policy.prd.md)，再读 [Runtime 处置设计](../../world-runtime/runtime/organization-continuity-and-disposition.design.md)。
- 想比较有限供能服务的检查点、价款、抵押和失败出口：读[供能服务政策](gameplay-checkpoint-power-service-contract.prd.md)，再读 [Runtime 履约设计](../../world-runtime/runtime/checkpoint-power-service-fulfillment.design.md)及[验收手册](../../testing/manual/checkpoint-power-service-acceptance.manual.md)；实际能力仍须独立实现与验证。
- 想确认已准入的有限 recipe 怎样到期、收缩和替代：读[授权生命周期政策](gameplay-industrial-creation-and-cross-region-market-contract.prd.md#governed-recipe-lifecycle-policy-v1)与[执行设计](../../world-runtime/runtime/governed-extension-lifecycle.design.md#des-wr-gr-lifecycle-001)；未完工作阻止首版替代，失去合法出口保留责任和 blocker。
- 想从创作交付走到受限试点、正式准入和有限收益：读[工业四阶段设计](../../world-runtime/runtime/industrial-creation-simulation-and-admission.design.md)与[有限许可政策](gameplay-industrial-creation-and-cross-region-market-contract.prd.md#industrial-license-policy-v1)，再核[许可执行与结算](../../world-runtime/runtime/industrial-license-and-royalty-settlement.design.md)；模拟、技术发布、正式使用和付款不是同一结果。
- 想在合作前了解对方的可核验履约事实：读[情境声誉政策](gameplay-contextual-reputation-contract.prd.md)和[证据及复核设计](../../world-runtime/runtime/contextual-reputation-evidence-and-review.design.md)；分清主体/控制任期、情境、补救与历史，不把缺记录或旧分数当全球信誉。
- 想在研究开始前约定贡献与收益，或转让已约定份额：读 [R&D charter 政策](gameplay-rd-charter-attribution-and-share-transfer-contract.prd.md)、[归因与转让执行设计](../../world-runtime/runtime/rd-charter-provenance-and-share-transfer.design.md)及[验收手册](../../testing/manual/rd-charter-attribution-and-share-transfer-acceptance.manual.md)；贡献事实、成果使用权和经济份额分别核对。
- 想了解组织或接替岗位的托管返还履历：读[玩法政策](gameplay-organization-custody-role-reputation-contract.prd.md)、[执行设计](../../world-runtime/runtime/organization-custody-role-reputation.design.md)和[验收手册](../../testing/manual/organization-custody-role-reputation-acceptance.manual.md)；分别核组织事项、原接替任期、实际返还和当前读取权限。
- 想精确找某份 gameplay 专题文档，而不是按问题阅读：回到 `../prd.index.md`

## 入口分工
- 本页只作 `gameplay/` 子目录导航，不复制完整长表。
- `../README.md` 是 game 模块级 landing page，负责在模块 PRD、执行台账、文件级索引和少量高频 gameplay 专题之间分流。
- `../prd.md` 是 game 模块目标态与阶段口径真值。
- `gameplay-top-level-design.prd.md` 只拥有核心玩法骨架与 `PRD-GAME-012` early-retention 的详细合同；它不替代 `../prd.md` 的活跃路由/状态职责，也不覆盖其他 topic 或 `doc/product/` 的声明范围。
- `gameplay-mature-world-progression-contract.prd.md` 拥有 `PRD-GAME-015` 的 lane、恢复、专业化、checkpoint 与 anti-grind 详细合同，并在 [generic mature-world recognition/opportunity leaf](./gameplay-mature-world-progression-contract.prd.md#prd-game-015-recognition-opportunity) 中承接 `REQ-WR-MW-004` / `MW-11` 的专业玩法语义；不新增 PRD-ID 或 authority，产品承诺仍由 mature-world 产品分册拥有。
- `gameplay-regional-charter-tenure-funding-contract.prd.md` 拥有 `PRD-GAME-017` 的区域 charter 提案/审查、双资格、退化恢复、tenure 长期规划、服务费/公共 levy 玩家动作与验收；产品 `GR-004` 至 `GR-008` 仍由 `governed-regional-capabilities-and-extensions.prd.md` 拥有，runtime/P2P/QA 保留各自执行与验证 authority。
- `gameplay-ordinary-governance-subject-and-oc-rights-contract.prd.md` 拥有 `PRD-GAME-019` 的普通治理主体聚合、资格/委托、公开审计、重复主体纠错/申诉、事项快照、OC 外部转让与单向非治理额度桥玩家动作和验收；其中 GCB-005 的事项快照与额度桥由 AC-GAME-019-04/05 承接，产品 `GCB-004/005` 仍由 `governed-common-decisions-and-constitutional-boundaries.prd.md` 拥有，runtime/P2P/QA 保留各自执行与验证 authority。
- `gameplay-industrial-creation-and-cross-region-market-contract.prd.md` 拥有 `PRD-GAME-018` 的工业创建、许可/开放、OC/世界资格隔离、跨区发现/物理交付/escrow 里程碑与争议恢复玩家合同；工业/世界资格隔离保持 GR-011 边界。其 [保供政策](gameplay-industrial-creation-and-cross-region-market-contract.prd.md#emergency-supply-policy-v1)集中持有 ES-001 至 ES-004 的首版规则值，产品要求仍由 ES 专题声明，Runtime 拥有执行与恢复合同。 [有限 recipe 授权政策](gameplay-industrial-creation-and-cross-region-market-contract.prd.md#governed-recipe-lifecycle-policy-v1)承接 GR-002/003 的有界专业范围，原工业准入、配方生命周期与许可继续独立合取。
- [组织连续性政策](gameplay-organization-continuity-policy.prd.md)集中持有 OC-001 至 OC-003 的活动、保护、接替和处置规则；组织权利连续性由产品专题声明，外部代币 OC 的治理资格仍由 GAME-019 专题拥有。
- [供能服务政策](gameplay-checkpoint-power-service-contract.prd.md)拥有 CR-001/002 有限个人供能切片的检查点、补救、争议和继受规则；Runtime 拥有资源占用、计量、事务及恢复，既有物料物流和组织处置规则继续由原专题拥有。
- [情境声誉政策](gameplay-contextual-reputation-contract.prd.md)拥有 CR-003 合同履约咨询切片的来源选择、时效与复核规则；Runtime 拥有证据投影和恢复，原合同继续拥有责任及申诉，其他主体/来源沿原范围补齐。
- [R&D charter 政策](gameplay-rd-charter-attribution-and-share-transfer-contract.prd.md)拥有 CR-003 自然主体有界研究的事前同意、固定分配、归因程序与整槽转让规则；Runtime 拥有证据、权利交接和恢复，原工业许可继续拥有实际使用和累计收益。
- [组织与岗位履历政策](gameplay-organization-custody-role-reputation-contract.prd.md)拥有 CR-003 托管返还咨询切片的来源、情境、时效与玩家选择；Runtime 拥有受权派生和恢复，原组织合同继续拥有任命、处分和实质救济。
- GitHub Project task status 与 issue evidence comments 是 retention、preview、经济规则和放行门禁的执行状态入口。
- `../prd.index.md` 是完整文件级索引，适合已知主题后按文件名查找。

## 文档密度
- 当前 inventory 与热点判定统一以仓库根目录执行的 `./scripts/doc-inventory-report.sh` 为准；本页不维护容易漂移的文件数量快照。
- 该子域是持续治理的热点目录；本页帮助读者快速找到首读入口，并将退役的一次性 handoff 内容归入正式 PRD/project/evidence surfaces。

## 首读主题簇

### 1. 核心玩法骨架
- 首读入口: 先 `../prd.md`，再 `gameplay-top-level-design.prd.md`
- 适合问题:
  - 游戏的核心循环、玩家目标和世界互动骨架是什么
  - gameplay 主题之间的关系如何理解
  - 新增玩法专题应该挂到哪个主线下

### 2. 当前体验窗口与留存修复
- 首读入口:
  - `../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md`（产品承诺）
  - `gameplay-top-level-design.prd.md`
  - [经营决策、目标委托与回访续接系统设计](gameplay-decision-delegation-return.design.md)（既有要求的组合接线、来源与验证）
- 适合问题:
  - 进入游戏后的前 10 分钟为什么会掉线
  - micro-loop、反馈可见性和 post-onboarding 阶段如何衔接
  - 哪些问题应该先作为体验修复，而不是系统重写
  - `TASK-GAME-076` 沿用 `scripts/verify-gameplay-attraction-automation.sh` 的 required/live 入口：fixture 卡片只核验体验结构，live 命令日志只证明对应候选的操作链，真人理解、有效决策与自发回访分别由真人记录判断。操作量、动作族、内容量、时长及等待比例保留为诊断目标，不能作为通用最低通关量；blocked 且无恢复仍阻断结构通过。
  - 汇总中的 `meaningful_decision_count` 标明快照选择机会代理计数；真人决策以 participant/decision（decision_id 跨会话稳定，新的实际决策使用新 ID）去重并保留选择及后果。没有真人记录时保持未验证。`production_only` 首产物不要求交付，完整交付样例继续单独检查 local demand 变化；首次实际生产结算、稳定能力和交付分别验收。

报告可使用 `--completion-profile production_only`（默认 `production_and_delivery`）、`--candidate '<候选身份注记>'`、`--fixture-samples-json '<快照数组.json>'` 和 `--human-observations-json '<观察记录数组.json>'`。两个文件必须是有效 JSON 对象数组，路径支持空格；输入在运行验证前读取并保存到报告 input，错误输入不会启动验证 gates；包装脚本先在私有临时目录执行原 writer 的完整消费路径，清理预览后才运行 gates。嵌套类型无法被 writer 消费的样本提前拒绝，`attraction_weak` 等未达标结果仍可正常报告。快照对象沿用 writer 的 fixture schema；真人记录沿用 `participant_id`、`session_id` 及 `observation`/`outcome`/`choice`，具体有效决策还需 `decision_id`、`choice`、`consequence`。不完整对象继续由 writer 排除并计数；非有限数值以及绝对值超过 JavaScript 安全整数边界 `9007199254740991` 的数值（包括嵌套字段、小数和科学计数法）被提前拒绝；边界内有限小数仍可报告。观察记录（包括额外字段）会原样保留在 input/report，提供前应去标识化。

`--candidate` 是调用者提供的字面注记，不验证世界、提交或 authority。提供 fixture、候选身份或真人记录不会把 required tier 升为 live，也不证明正式运行、真人验收或回访；未提供记录仍保持未验证。

### 3. Agency、间接控制与物理尺度
- 首读入口:
  - `gameplay-indirect-control-agency-contract.prd.md`
  - `../../product/world-rules-core-gameplay/prd.md`（产品承诺）与 `gameplay-top-level-design.prd.md`（玩法合同）
  - `../../product/world-rules-core-gameplay/mature-world-progression.prd.md`（产品承诺）与 `gameplay-mature-world-progression-contract.prd.md`（玩法合同）
  - `../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md`（产品承诺）与 `gameplay-regional-charter-tenure-funding-contract.prd.md`（区域制度玩法合同）
  - `gameplay-regional-infrastructure-micro-depot-contract.prd.md`
- 适合问题:
  - 间接控制如何保留玩家主因果感
  - 1cm 物理世界和表现层夸张的边界是什么
  - 小玩家在成熟世界里如何继续产生 leverage
  - 可编程区域设施如何作为中后期区域专业化能力落地，而不变成新手期自由建造或任意脚本

### 4. Preview / beta / release gate
- 首读入口:
  - `../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md`（访问模式、受控 preview 与发行就绪产品承诺）
- 适合问题:
  - limited preview 可以放什么、不能承诺什么
  - closed beta candidate gate 如何判断
  - 当前 release gate 与候选级放行边界在哪里
- 历史 closure / production provenance:
  - release gap、production closure、runtime governance 与 base-runtime/WASM split 旧专题正文已退役；当前首读走 `gameplay-top-level-design.prd.md`、world-runtime/WASM 专业权威与 PR、实际 CI 与评审记录（Issue 按需）。

### 5. Economy、claim 与运营规则
- 首读入口:
  - `gameplay-agent-claim-economy-contract.prd.md`
  - `gameplay-agent-claim-restricted-grant-liveops-runbook-2026-03-29.md`
  - `../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md`
  - `../../product/world-infrastructure/deterministic-world-execution.prd.md`
- 适合问题:
  - agent claim 的 token cost、claim bond、upkeep、reclaim 如何组织
  - restricted grant 如何发放、撤销、过期和 incident 处理
  - starter OC / first chat gate 如何与 restricted starter claim balance 区分
  - economy / longrun hardening 何时需要 QA 或 LiveOps 参与

## 定向检索边界
- 如果你已经知道准确文件名，直接回 `../prd.index.md`。
- runbook、evidence、checklist 和必要的历史执行补充材料继续保留可检索性，但默认不作为首读入口；一次性 role handoff brief 在对应 evidence 或 PR、实际 CI 与评审记录（Issue 按需）已能追溯后应退役删除。旧 MLF008 viewer-to-QA handoff、limited-preview 与 closed-beta role handoff briefs 已退役删除；历史 TASK-GAME-018、029~032、036/037 从 PR 与实际验证记录、专业 evidence surfaces、core review logs、Git history 与 GitHub issue `eng-cc/oasis7#48` 追溯。
- 如果问题需要玩法正确性、平衡、release 放行或对外口径结论，本页只提供文档入口；结论必须回到 `gameplay_designer`、`qa_engineer` 或 `liveops_community` 对应任务证据。
- 历史专题不因出现在本页附近而重新成为当前真值；当前判断以模块 PRD/project、活跃专题、正式 evidence surfaces 与 PR、实际 CI 与评审记录（Issue 按需）为准。

## 维护约定
- 新增 gameplay 专题后，若改变默认首读路径，应同步更新本页。
- 玩法行为、进度、经济规则、preview/beta gate 或对外承诺变化时，必须同步评估是否更新 `../prd.md`、相关高频专题、产品 PRD 与对应 PR 与实际验证记录。
- 本页只维护簇级入口，不维护完整文件清单。
