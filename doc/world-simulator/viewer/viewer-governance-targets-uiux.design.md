# Viewer 治理、社会与季节条件目标 UI/UX 设计

- Professional owner: `game_visual_interaction_designer`；Integration owner: `tpm`。
- 状态：专业呈现与交互补充；源证据与目标稿分开。尚未完成的图稿关系保持明确缺口，不是功能已发布声明。
- 固定源：PR #5277 `113868200d1f91bef1008babb689bd257f241428`；产品/Viewer源与审读main `5cc5c2b8a84c22f80cfa9ee16d893eaf6deedb08` 对应subtree相同。
- 配套：[派生目录](viewer-full-function-coverage-catalog.md)、[既有页面总览](viewer-page-module-design-2026-06-18.design.md#detailed-uiux-handoff)。目录不是产品authority。

## 1. 问题、目标与非目标

本设计把已有源的字段、动作、条件及未解决的目标合同分开，使玩家能读清观察对象、当前权限、下一步与原结果。具体引用仅承接表中适用义务；组件存在、图稿数量、accepted和设计覆盖都不等于世界效果或运行验收。

不新增玩法/认证/治理能力，不改变后端权限、报价或结算语义；不代替产品PRD、实际服务capability或真实试玩。

## 2. 上游约束与相关角色

产品语义由各行唯一上游声明拥有；Viewer负责呈现、交互与失败/恢复信息，不接管identity、runtime、world或治理authority。现有品牌和终端层级继续适用。

### 2.1 需求承接与分配表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [REQ-WR-GCB-001](../../product/world-rules-core-gameplay/governed-common-decisions-and-constitutional-boundaries.prd.md#req-wr-gcb-001)；Trace refs: O7-REQ-f282f0ef-655c-4e41-a1b6-fa37482d663b | 条件目标读面：类别 policy / treasury / charter, 目标对象与规范化效果集合, 事项身份 / 修订 / payload digest, 授权者、grant 与来源 receipt；玩家或受权 Agent 声明 intent；分类器确认规范化效果、保护索引和 authority 来源。 | [GCB-001](viewer-governance-targets-uiux.design.md#t-gov-01-gcb-001) | 对应产品/专业authority；原合同和当前主体能力分别核对 | 保留原产品剩余范围，不由图稿代签UI接通、完整写面、运行或世界效果 |
| [REQ-WR-GCB-002](../../product/world-rules-core-gameplay/governed-common-decisions-and-constitutional-boundaries.prd.md#req-wr-gcb-002)；Trace refs: O7-REQ-79b6687e-3b8c-42a3-a266-df4d191fec06 | 条件目标读面：审议与影响说明, 延迟 / 复核的权威时间, 超多数与独立资格快照, 受影响主体独立确认；各独立 producer 分别供审议、延迟、超多数、独立确认、审计、申诉和当前授权；执行者须具本案当前执行权限。 | [GCB-002](viewer-governance-targets-uiux.design.md#t-gov-01-gcb-002) | 对应产品/专业authority；原合同和当前主体能力分别核对 | 保留原产品剩余范围，不由图稿代签UI接通、完整写面、运行或世界效果 |
| [REQ-WR-GCB-003](../../product/world-rules-core-gameplay/governed-common-decisions-and-constitutional-boundaries.prd.md#req-wr-gcb-003)；Trace refs: O7-REQ-4f6597ff-8e34-4d12-bb60-060df0e09595 | 条件目标读面：原 matter / approved revision, 通过时目标、范围与允许效果, 当前授权、保护与前置检查, 唯一 ExecutionSlot / payload；当前有 ExecuteGovernedMatter 权限的主体；执行器固定 ExecutionSlot，非任意 operator 或普通投票者。 | [GCB-003](viewer-governance-targets-uiux.design.md#t-gov-01-gcb-003) | 对应产品/专业authority；原合同和当前主体能力分别核对 | 保留原产品剩余范围，不由图稿代签UI接通、完整写面、运行或世界效果 |
| [REQ-WR-GCB-004](../../product/world-rules-core-gameplay/governed-common-decisions-and-constitutional-boundaries.prd.md#req-wr-gcb-004)；Trace refs: O7-REQ-b584b2ab-bc38-43a1-a8fd-1b89dc3d20cd | 条件目标读面：经验证游戏账户, 已批准身份 / signer / wallet 绑定摘要, 锁定 OC / 有界可撤销委托, 控制人聚合资格结果（最小披露）；当前读权限 audience 可看允许信息；完整写角色不由本设计补造。 | [GCB-004](viewer-governance-targets-uiux.design.md#t-gov-01-gcb-004) | 对应产品/专业authority；原合同和当前主体能力分别核对 | 保留原产品剩余范围，不由图稿代签UI接通、完整写面、运行或世界效果 |
| [REQ-WR-GCB-005](../../product/world-rules-core-gameplay/governed-common-decisions-and-constitutional-boundaries.prd.md#req-wr-gcb-005)；Trace refs: O7-REQ-4b64b7c5-9819-4932-a1ad-3cdb09fa82e5 | 条件目标读面：外部 OC 转让事实, 事项范围内 lock / snapshot, 退出或解除绑定的当前结果, 有效窗口内原计票历史；当前读权限 audience 可看允许信息；完整写角色不由本设计补造。 | [GCB-005](viewer-governance-targets-uiux.design.md#t-gov-01-gcb-005) | 对应产品/专业authority；原合同和当前主体能力分别核对 | 保留原产品剩余范围，不由图稿代签UI接通、完整写面、运行或世界效果 |
| [REQ-WR-GCB-006](../../product/world-rules-core-gameplay/governed-common-decisions-and-constitutional-boundaries.prd.md#req-wr-gcb-006)；Trace refs: O7-REQ-c3d848a8-0668-408a-99ca-ef08a7c64be8 | 条件目标读面：保护动作与受影响对象, 授权主体 / 来源 / 当前范围, 理由与可公开证据, 生效期限与失效边界；当前读权限 audience 可看允许信息；完整写角色不由本设计补造。 | [GCB-006](viewer-governance-targets-uiux.design.md#t-gov-01-gcb-006) | 对应产品/专业authority；原合同和当前主体能力分别核对 | 保留原产品剩余范围，不由图稿代签UI接通、完整写面、运行或世界效果 |
| [REQ-WR-GR-001](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-001)；Trace refs: O7-REQ-d34e2dec-9ad4-44a8-9ba7-7931956e39b8 | 条件目标读面：区域压力与设施对象, 报价成本 / 取舍 / 快照, 提交与权威设施结果, 有限库存 / 服务消耗；设施动作的当前 capability actor；限 micro_depot 合同，不推广全部区域设施。 | [GR-001](viewer-governance-targets-uiux.design.md#t-gov-02-gr-001) | 对应产品/专业authority；原合同和当前主体能力分别核对 | 保留原产品剩余范围，不由图稿代签UI接通、完整写面、运行或世界效果 |
| [REQ-WR-GR-004](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-004)；Trace refs: O7-REQ-7de05591-35c1-4d8a-94b2-f64fce61976e | 条件目标读面：proposal / revision / operation, 空间锚定 scope 与前身 charter, 设施 / 物流 / 服务能力证据, 需求交付 / Agent / 资源 / bond；提案者提交证据；local/neighbor/global 各自当前 reviewer；激活器重验 required_review_set。 | [GR-004](viewer-governance-targets-uiux.design.md#t-gov-02-gr-004) | 对应产品/专业authority；原合同和当前主体能力分别核对 | 保留原产品剩余范围，不由图稿代签UI接通、完整写面、运行或世界效果 |
| [REQ-WR-GR-005](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-005)；Trace refs: O7-REQ-ee810b69-8c6a-4aca-86e8-ac387c8f8fcb | 条件目标读面：charter / authority revision, 事项与地方规则适用 scope, OC / 委托 / 控制人资格来源, 持续本地贡献来源与窗口；LocalEligibility producer 供双来源结果；本地决策主体必须满足该 matter/scope 的现行政策。 | [GR-005](viewer-governance-targets-uiux.design.md#t-gov-02-gr-005) | 对应产品/专业authority；原合同和当前主体能力分别核对 | 保留原产品剩余范围，不由图稿代签UI接通、完整写面、运行或世界效果 |
| [REQ-WR-GR-006](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-006)；Trace refs: O7-REQ-46939a8f-c2a9-4ab6-818c-6bfa4bc83c5c | 条件目标读面：active / observing / suspended / dissolved, 触发证据与权威观察窗口, 能力收缩、恢复条件与异议, bond 与设施、Agent 处置；授权阶段裁决者；各资产 owner 执行独立子处置，不由汇总页面代签。 | [GR-006](viewer-governance-targets-uiux.design.md#t-gov-02-gr-006) | 对应产品/专业authority；原合同和当前主体能力分别核对 | 保留原产品剩余范围，不由图稿代签UI接通、完整写面、运行或世界效果 |
| [REQ-WR-GR-007](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-007)；Trace refs: O7-REQ-14aa9ab5-d30d-4be2-8c24-610bdc1d1186 | 条件目标读面：地点 / 用途 / 建设权范围, 原取得依据与维护续期, 到期、闲置、违约或公共必要性, 通知、理由、申诉状态；当前读权限 audience 可看允许信息；完整写角色不由本设计补造。 | [GR-007](viewer-governance-targets-uiux.design.md#t-gov-02-gr-007) | 对应产品/专业authority；原合同和当前主体能力分别核对 | 保留原产品剩余范围，不由图稿代签UI接通、完整写面、运行或世界效果 |
| [REQ-WR-GR-008](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-008)；Trace refs: O7-REQ-17536c7e-c59a-464a-90e8-c36f0330c8f7 | 条件目标读面：service_fee / public_levy 类型, charter 预授权与专项目的, 受益范围、征收依据与主体, 金额 / 资源 / 上限 / 有效期；合法治理/筹资 authority 提供 LevyAuthority；结算主体在当前专项范围内执行，付款方不能自发升级服务合同。 | [GR-008](viewer-governance-targets-uiux.design.md#t-gov-02-gr-008) | 对应产品/专业authority；原合同和当前主体能力分别核对 | 保留原产品剩余范围，不由图稿代签UI接通、完整写面、运行或世界效果 |
| [REQ-WR-GR-002](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-002)；Trace refs: O7-REQ-ce8c31b1-5294-4acd-a65f-2012bcbef911 | 条件目标读面：release / artifact 精确身份, 完整 profile payload 承诺, 当前 owner 与审批角色, 能力范围与世界前置；当前 owner 与审批角色分别复核完整 payload；只承接已读 player publication 子范围。 | [GR-002](viewer-governance-targets-uiux.design.md#t-gov-03-gr-002) | 对应产品/专业authority；原合同和当前主体能力分别核对 | 保留原产品剩余范围，不由图稿代签UI接通、完整写面、运行或世界效果 |
| [REQ-WR-GR-003](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-003)；Trace refs: O7-REQ-ac12295b-63a5-4440-9924-deaedafd7965 | 条件目标读面：被移除 FROM artifact tuple, 恢复 TO artifact / rollback 来源, 新准入冻结 marker 与理由, 既有 profile / instance 保留；合法 rollback authority；仅停止 exact FROM artifact 新准入，不推导一般撤销/替代。 | [GR-003](viewer-governance-targets-uiux.design.md#t-gov-03-gr-003) | 对应产品/专业authority；原合同和当前主体能力分别核对 | 保留原产品剩余范围，不由图稿代签UI接通、完整写面、运行或世界效果 |
| [REQ-WR-GR-009](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-009)；Trace refs: O7-REQ-3d713ab7-87c2-41df-9c89-827859c23313 | 条件目标读面：能力提案与不可变输入, 确定性模拟来源与结果, 试点 scope / 期限 / 资源上限, 试点退出条件与实际结果；当前读权限 audience 可看允许信息；完整写角色不由本设计补造。 | [GR-009](viewer-governance-targets-uiux.design.md#t-gov-03-gr-009) | 对应产品/专业authority；原合同和当前主体能力分别核对 | 保留原产品剩余范围，不由图稿代签UI接通、完整写面、运行或世界效果 |
| [REQ-WR-GR-010](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-010)；Trace refs: O7-REQ-0ec3d72f-51d1-4d64-9824-ebd9365f2d47 | 条件目标读面：能力 / 用途 / 区域, 期限与到期开放条件, 许可 / 版税 / 收益权依据, 维护责任与审计；当前读权限 audience 可看允许信息；完整写角色不由本设计补造。 | [GR-010](viewer-governance-targets-uiux.design.md#t-gov-03-gr-010) | 对应产品/专业authority；原合同和当前主体能力分别核对 | 保留原产品剩余范围，不由图稿代签UI接通、完整写面、运行或世界效果 |
| [SC32-institution](../../product/world-rules-core-gameplay/prd.md#root-sc32-institution)；Trace refs: O7-REQ-90614e34-bbc7-42df-b355-7ce0b34ccdc3 | 条件目标读面：Alliance — Compatibility keep, War — Defer, Governance — Kernel keep, EconomicContract — Compatibility keep；当前读权限 audience 可看允许信息；完整写角色不由本设计补造。 | [SC32-institution](viewer-governance-targets-uiux.design.md#t-gov-03-sc32-institution) | 对应产品/专业authority；原合同和当前主体能力分别核对 | 保留原产品剩余范围，不由图稿代签UI接通、完整写面、运行或世界效果 |
| [SC32-pilot](../../product/world-rules-core-gameplay/prd.md#root-sc32-pilot)；Trace refs: O7-REQ-f081f96d-f95a-4538-b0b7-3edf23ff772a | 条件目标读面：Alliance 或 EconomicContract 唯一候选, 另一制度保持兼容, 治理批准 manifest / activation, L0 / L1 不可绕过；当前读权限 audience 可看允许信息；完整写角色不由本设计补造。 | [SC32-pilot](viewer-governance-targets-uiux.design.md#t-gov-03-sc32-pilot) | 对应产品/专业authority；原合同和当前主体能力分别核对 | 保留原产品剩余范围，不由图稿代签UI接通、完整写面、运行或世界效果 |
| [SC32-budgetread](../../product/world-rules-core-gameplay/prd.md#root-sc32-budgetread)；Trace refs: O7-REQ-96cb1655-2ca3-445f-af2e-9f113e6f3e8e | 条件目标读面：Agent / Module / System owner, payer 与绑定来源, 能力 scope / 是否计费, quote units 与预算影响；当前读权限 audience 可看允许信息；完整写角色不由本设计补造。 | [SC32-budgetread](viewer-governance-targets-uiux.design.md#t-gov-03-sc32-budgetread) | 对应产品/专业authority；原合同和当前主体能力分别核对 | 保留原产品剩余范围，不由图稿代签UI接通、完整写面、运行或世界效果 |
| [SC32-budgetcommit](../../product/world-rules-core-gameplay/prd.md#root-sc32-budgetcommit)；Trace refs: O7-REQ-70e55a1f-7087-41e8-918d-7d1989c0155a | 条件目标读面：原操作身份与 grant, 执行前预算 / 当前 owner, actual effect 与 debit 关联, authorization receipt / effect completion；当前读权限 audience 可看允许信息；完整写角色不由本设计补造。 | [SC32-budgetcommit](viewer-governance-targets-uiux.design.md#t-gov-03-sc32-budgetcommit) | 对应产品/专业authority；原合同和当前主体能力分别核对 | 保留原产品剩余范围，不由图稿代签UI接通、完整写面、运行或世界效果 |
| [SC33-extscope](../../product/world-rules-core-gameplay/prd.md#root-sc33-extscope)；Trace refs: O7-REQ-e6f4ffa7-0b1b-4d0e-b7a3-8ca38c8d0244 | 条件目标读面：industrial_externality / operational_mitigation, recipe / factory profile 与版本, scope、authority 与有效期, 容量与已有 obligation；profile 声明动作与当前 mitigation authority 同时适用的 owner/scope 主体；不授地方治理权。 | [SC33-extscope](viewer-governance-targets-uiux.design.md#t-gov-03-sc33-extscope) | 对应产品/专业authority；原合同和当前主体能力分别核对 | 保留原产品剩余范围，不由图稿代签UI接通、完整写面、运行或世界效果 |
| [SC33-extdispose](../../product/world-rules-core-gameplay/prd.md#root-sc33-extdispose)；Trace refs: O7-REQ-bd83785c-d414-4bd7-b291-41a4d7272aa3 | 条件目标读面：原 root / revision / child, WIP / transit / buffer / terminal, production result 与 disposition refs, contained / held_pending_disposition；原工业/外部性义务的当前合法处置主体；依 profile 支持动作和阶段 sink 边界逐次校验。 | [SC33-extdispose](viewer-governance-targets-uiux.design.md#t-gov-03-sc33-extdispose) | 对应产品/专业authority；原合同和当前主体能力分别核对 | 保留原产品剩余范围，不由图稿代签UI接通、完整写面、运行或世界效果 |
| [REQ-WR-CR-001](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-001)；Trace refs: O7-REQ-16ef3247-1587-4cd8-84ab-03a9bb18a17f | 条件目标读面：沟通 / 草稿 / 未接受提议, 各方主体与受权范围, 完整条款与明确接受, 权威校验与 receipt；当前读权限 audience 可看允许信息；完整写角色不由本设计补造。 | [CR-001](viewer-governance-targets-uiux.design.md#t-gov-04-cr-001) | 对应产品/专业authority；原合同和当前主体能力分别核对 | 保留原产品剩余范围，不由图稿代签UI接通、完整写面、运行或世界效果 |
| [REQ-WR-CR-002](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-002)；Trace refs: O7-REQ-805fc92a-bbcb-4e77-b2d8-e40105522a0d | 条件目标读面：服务身份 / 原合同, 已完成 checkpoint 与结果, 未履行范围与责任方, 修复或重新履行；当前读权限 audience 可看允许信息；完整写角色不由本设计补造。 | [CR-002](viewer-governance-targets-uiux.design.md#t-gov-04-cr-002) | 对应产品/专业authority；原合同和当前主体能力分别核对 | 保留原产品剩余范围，不由图稿代签UI接通、完整写面、运行或世界效果 |
| [REQ-WR-CR-003](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-003)；Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905 | 条件目标读面：actor / subject / object / action kind, claim summary / confidence / stake risk, 受影响关系与 social surfaces, cooperation / dispute / governance relevance；受当前事实 scope 与共享资格校验的 actor；这里只设计已有 Simulator 只读预览子范围，不启用完整发布/R&D。 | [CR-003](viewer-governance-targets-uiux.design.md#t-gov-04-cr-003) | 对应产品/专业authority；原合同和当前主体能力分别核对 | 保留原产品剩余范围，不由图稿代签UI接通、完整写面、运行或世界效果 |
| [REQ-WR-OC-001](../../product/world-rules-core-gameplay/organization-continuity-dissolution-and-dormancy-protection.prd.md#req-wr-oc-001)；Trace refs: O7-REQ-366c9e94-4bd6-41f8-93dd-8d87f163f21a | 条件目标读面：organization / case / policy / manifest, Inventory → Obligations → CustodyReturn, Claims → BusinessDisposition → Residual, 每项 owner / held-in-trust contributor；原 appointment authority 且当前治理通过；首版仅 Disposition/CustodyReturn 或明确合格 successor scope 的完整 held-in-trust 返还。 | [OC-001](viewer-governance-targets-uiux.design.md#t-gov-05-oc-001) | 对应产品/专业authority；原合同和当前主体能力分别核对 | 保留原产品剩余范围，不由图稿代签UI接通、完整写面、运行或世界效果 |
| [REQ-WR-OC-002](../../product/world-rules-core-gameplay/organization-continuity-dissolution-and-dormancy-protection.prd.md#req-wr-oc-002)；Trace refs: O7-REQ-6e6d890d-c94d-4b43-9947-bfccf2f066e3 | 条件目标读面：Operating / Notified / Protected / Restricted, availability / activity coverage 分列, 权威 C 窗口与暂停原因, 通知记录与保护期；具权利来源的 reclaim/appeal 申请人；独立救济裁决者判定支持范围，不由申请人自行复权。 | [OC-002](viewer-governance-targets-uiux.design.md#t-gov-05-oc-002) | 对应产品/专业authority；原合同和当前主体能力分别核对 | 保留原产品剩余范围，不由图稿代签UI接通、完整写面、运行或世界效果 |
| [REQ-WR-OC-003](../../product/world-rules-core-gameplay/organization-continuity-dissolution-and-dormancy-protection.prd.md#req-wr-oc-003)；Trace refs: O7-REQ-b59a4fa5-9b5f-4119-abf6-7c0f2b4f4268 | 条件目标读面：旧 grant / 新候选资格绑定, object / right / action / quantity scope, 重叠范围与 stay / quarantine, 原请求绑定 grant / scope version；当前任命 authority 与通过当前控制绑定/顺位核验并接受的 successor；重叠旧授权原子终止。 | [OC-003](viewer-governance-targets-uiux.design.md#t-gov-05-oc-003) | 对应产品/专业authority；原合同和当前主体能力分别核对 | 保留原产品剩余范围，不由图稿代签UI接通、完整写面、运行或世界效果 |
| [REQ-WR-CC-001](../../product/world-rules-core-gameplay/chartered-conflict-soft-seasons-and-recovery.prd.md#req-wr-cc-001)；Trace refs: O7-REQ-2e68a48f-4a44-4135-9268-e17c6f32d0b3 | 条件目标读面：charter 与冲突授权, 参战主体 / 区域范围, 暴露资产与时间窗口, 非参与者保护；当前读权限 audience 可看允许信息；完整写角色不由本设计补造。 | [CC-001](viewer-governance-targets-uiux.design.md#t-gov-06-cc-001) | 对应产品/专业authority；原合同和当前主体能力分别核对 | 保留原产品剩余范围，不由图稿代签UI接通、完整写面、运行或世界效果 |
| [REQ-WR-CC-002](../../product/world-rules-core-gameplay/chartered-conflict-soft-seasons-and-recovery.prd.md#req-wr-cc-002)；Trace refs: O7-REQ-19ea1035-2d0a-43fc-a3a8-b0309030273c | 条件目标读面：受损对象与损失范围, 保留的玩家 / Agent 身份, 历史 receipt 与原能力, repair / rebuild / pivot / 撤离；当前读权限 audience 可看允许信息；完整写角色不由本设计补造。 | [CC-002](viewer-governance-targets-uiux.design.md#t-gov-06-cc-002) | 对应产品/专业authority；原合同和当前主体能力分别核对 | 保留原产品剩余范围，不由图稿代签UI接通、完整写面、运行或世界效果 |
| [REQ-WR-CC-003](../../product/world-rules-core-gameplay/chartered-conflict-soft-seasons-and-recovery.prd.md#req-wr-cc-003)；Trace refs: O7-REQ-eedaa5cf-e91e-48df-9c33-158774b40168 | 条件目标读面：project / containment / policy / revision, 参与资格与 source claim, contribution accepted / waiting_evidence, milestone 决定与 target revision；有权 owner 开项目；contributor 提供 source claim；受权 evaluator 判证据；项目 authority 应用 milestone；独立复核处理申诉。 | [CC-003](viewer-governance-targets-uiux.design.md#t-gov-06-cc-003) | 对应产品/专业authority；原合同和当前主体能力分别核对 | 保留原产品剩余范围，不由图稿代签UI接通、完整写面、运行或世界效果 |
| [REQ-WR-CC-004](../../product/world-rules-core-gameplay/chartered-conflict-soft-seasons-and-recovery.prd.md#req-wr-cc-004)；Trace refs: O7-REQ-a2b47612-d2c2-4331-811e-58a61366b64d | 条件目标读面：contested occupation 的授权范围, 持续占有与到期冻结, 已成立权利及其 receipt, 物理提取与在途物品；当前读权限 audience 可看允许信息；完整写角色不由本设计补造。 | [CC-004](viewer-governance-targets-uiux.design.md#t-gov-06-cc-004) | 对应产品/专业authority；原合同和当前主体能力分别核对 | 保留原产品剩余范围，不由图稿代签UI接通、完整写面、运行或世界效果 |
| [REQ-WR-CC-005](../../product/world-rules-core-gameplay/chartered-conflict-soft-seasons-and-recovery.prd.md#req-wr-cc-005)；Trace refs: O7-REQ-ce9fbace-7fc6-4973-8bcf-8b58e11b3339 | 条件目标读面：刷新竞争机会与区域资格, 生效条件、对象及边界, 同一世界时间线, 玩家 / Agent 身份与核心能力；当前读权限 audience 可看允许信息；完整写角色不由本设计补造。 | [CC-005](viewer-governance-targets-uiux.design.md#t-gov-06-cc-005) | 对应产品/专业authority；原合同和当前主体能力分别核对 | 保留原产品剩余范围，不由图稿代签UI接通、完整写面、运行或世界效果 |
| [REQ-WR-FI-001](../../product/world-rules-core-gameplay/frontier-expansion-and-world-information-boundaries.prd.md#req-wr-fi-001)；Trace refs: O7-REQ-3013b8cf-e3db-4506-a30a-db34e25ed636 | 条件目标读面：相邻区域身份与空间来源, 探索前置, 物流连通前置, charter 组合准入；当前读权限 audience 可看允许信息；完整写角色不由本设计补造。 | [FI-001](viewer-governance-targets-uiux.design.md#t-gov-07-fi-001) | 对应产品/专业authority；原合同和当前主体能力分别核对 | 保留原产品剩余范围，不由图稿代签UI接通、完整写面、运行或世界效果 |
| [REQ-WR-FI-002](../../product/world-rules-core-gameplay/frontier-expansion-and-world-information-boundaries.prd.md#req-wr-fi-002)；Trace refs: O7-REQ-066843dd-9e40-4148-8933-d369d16b55a2 | 条件目标读面：地点 / 用途 / 当前持有人, 未完成开发里程碑, 原期限与剩余范围, 受让人当前资格；当前读权限 audience 可看允许信息；完整写角色不由本设计补造。 | [FI-002](viewer-governance-targets-uiux.design.md#t-gov-07-fi-002) | 对应产品/专业authority；原合同和当前主体能力分别核对 | 保留原产品剩余范围，不由图稿代签UI接通、完整写面、运行或世界效果 |
| [REQ-WR-FI-003](../../product/world-rules-core-gameplay/frontier-expansion-and-world-information-boundaries.prd.md#req-wr-fi-003)；Trace refs: O7-REQ-23640893-de22-4940-9b25-ed08c9ab0f7e | 条件目标读面：公共规则 / 已结算事实, 最低安全与通行披露, 有界私用 → 公共 baseline, live inventory / routes 的取得依据；当前读权限 audience 可看允许信息；完整写角色不由本设计补造。 | [FI-003](viewer-governance-targets-uiux.design.md#t-gov-07-fi-003) | 对应产品/专业authority；原合同和当前主体能力分别核对 | 保留原产品剩余范围，不由图稿代签UI接通、完整写面、运行或世界效果 |
| [SC8-dataread](../../product/world-rules-core-gameplay/prd.md#root-sc8-dataread)；Trace refs: O7-REQ-6bd89114-455c-490b-9b21-425fa90df866 | 条件目标读面：owner 与 recipient, purpose 与 scope, 当前 grant / 有效性, 来源与预览身份；当前读权限 audience 可看允许信息；完整写角色不由本设计补造。 | [SC8-dataread](viewer-governance-targets-uiux.design.md#t-gov-07-sc8-dataread) | 对应产品/专业authority；原合同和当前主体能力分别核对 | 保留原产品剩余范围，不由图稿代签UI接通、完整写面、运行或世界效果 |
| [SC8-datacommit](../../product/world-rules-core-gameplay/prd.md#root-sc8-datacommit)；Trace refs: O7-REQ-ebcc34f8-5db5-46f7-85fb-30b44b8b797f | 条件目标读面：原提交 / grant / scope, 提交至结算间有效性, 拒绝或待决原因, 实际使用 / 转移；当前读权限 audience 可看允许信息；完整写角色不由本设计补造。 | [SC8-datacommit](viewer-governance-targets-uiux.design.md#t-gov-07-sc8-datacommit) | 对应产品/专业authority；原合同和当前主体能力分别核对 | 保留原产品剩余范围，不由图稿代签UI接通、完整写面、运行或世界效果 |
| [SC8-datareplay](../../product/world-rules-core-gameplay/prd.md#root-sc8-datareplay)；Trace refs: O7-REQ-c549ec3e-5dc6-4e27-9285-4e326b0fc5d3 | 条件目标读面：历史合法使用 / 转移 receipt, 原 owner / recipient / purpose, 当前 grant 撤销状态, 原请求与重复结果；当前读权限 audience 可看允许信息；完整写角色不由本设计补造。 | [SC8-datareplay](viewer-governance-targets-uiux.design.md#t-gov-07-sc8-datareplay) | 对应产品/专业authority；原合同和当前主体能力分别核对 | 保留原产品剩余范围，不由图稿代签UI接通、完整写面、运行或世界效果 |
| [REQ-WR-GCB-005](../../product/world-rules-core-gameplay/governed-common-decisions-and-constitutional-boundaries.prd.md#req-wr-gcb-005)；Trace refs: O7-REQ-4b64b7c5-9819-4932-a1ad-3cdb09fa82e5 | 条件目标读面：外部 OC 转让事实, 事项范围内 lock / snapshot, 退出或解除绑定的当前结果, 有效窗口内原计票历史；当前读权限 audience 可看允许信息；完整写角色不由本设计补造。 | [GCB-005](viewer-governance-targets-uiux.design.md#t-gov-08-gcb-005) | 对应产品/专业authority；原合同和当前主体能力分别核对 | 保留原产品剩余范围，不由图稿代签UI接通、完整写面、运行或世界效果 |
| [REQ-WR-GR-011](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-011)；Trace refs: O7-REQ-d27a209c-a703-4443-92d9-acadb9820ed2 | 条件目标读面：链上转让规则与链上成本, 世界内资源 / 信用 / 库存 / 合同, 地方 charter 的适用双资格, 工业准入 / 市场 / 紧急保供各自门禁；当前读权限 audience 可看允许信息；完整写角色不由本设计补造。 | [GR-011](viewer-governance-targets-uiux.design.md#t-gov-08-gr-011) | 对应产品/专业authority；原合同和当前主体能力分别核对 | 保留原产品剩余范围，不由图稿代签UI接通、完整写面、运行或世界效果 |

## 3. 当前状态、目标状态与差距

`implemented_in_frozen_source` 只表示固定源码有实际消费者；条件显示不是未实现。`target_only` 只表示已有产品义务的设计目标；未确认专用Viewer入口不证明全仓后端不存在。QA与禁用占位不进入普通玩家当前功能分母。图中的示例数值不是真实世界结果。

## 4. 边界与结构

World / Targets / Command 是三个主Dock；Search在Targets内，Diagnostics/More为次级。Bevy是唯一世界renderer；图稿占位不是JS世界回退。当前主体、观察目标、模型连接、账号、session、admission、control分别表述。

## 5. 关键交互流程

先读对象与可用条件，再编辑草稿/请求预估，只有实际capability允许时才显式提交。pending/unknown保留原操作身份；切换观察对象不改写原回执。关闭弹层返回invoker，焦点/IME不得误触提交。具体feature条款优先于此共同顺序。

## 6. 条件目标的字段、动作与状态


以下保留专业合同完整度、当前Viewer接通与runtime验证三轴，不把所有灰色动作一概称合同缺失。

<a id="t-gov-01-gcb-001"></a>
### 条款 T-GOV-01 / GCB-001

- 唯一上游：[REQ-WR-GCB-001](../../product/world-rules-core-gameplay/governed-common-decisions-and-constitutional-boundaries.prd.md#req-wr-gcb-001)；Trace refs: O7-REQ-f282f0ef-655c-4e41-a1b6-fa37482d663b。
- 状态：源设计 `已覆盖`；源实现 `未实现`。这是原状态卡范围，不能从视觉改变实现结论。
- 角色/条件：玩家或受权 Agent 声明 intent；分类器确认规范化效果、保护索引和 authority 来源。。
- 字段：类别 policy / treasury / charter；目标对象与规范化效果集合；事项身份 / 修订 / payload digest；授权者、grant 与来源 receipt；目标范围 / 能力范围 / 有效区间；route、保护条款、缺项
- 三轴：当前完整Viewer `not_confirmed; preserve listed narrow consumers`；runtime绑定 `not_verified; no current connection or runtime execution claim`；专业动作合同 `complete_target`。
- 具体表示：[具体目标稿](assets/ui-ux/full-function/governance-targets/frames/B01-GCB-001.svg)；`normal_frame`；字段区域按 `field-1…field-6`，当前未运行。
- 动作「记录事项声明」：`target_conditionally_available`；无完整效果声明或来源不得进入普通流程；当前图中条件：disabled because authoritative input/permission/read result unavailable; this is separate from contract completeness；启用条件：exact supported operation/schema + current actor role + scope/authority/versions + fresh prerequisites; read-only controls require current read permission and resolvable original record。
- 状态规范：needs_evidence / rejected_protected / unsupported_route；恢复：查看原分类；补齐原事项证据；不自动拆分复合效果；完整组合仍text_spec_only。
- [状态片段](assets/ui-ux/full-function/governance-targets/states/S13.svg)；区域：inputs x36 y286 w744 h491；checks x820 y286 w744 h491；conditional_ctas y812 h54；仅合同示例。
- 原剩余范围：实现普通事项声明、白名单/保护边界分类与普通/财库/charter/紧急/升级/复合路由；执行前重验并原子拒绝越界且不占用资金/权限，提供缺项和下一步。 实施待安排： [Refs #4855](https://github.com/nil-studio/oasis7/issues/4855)；本状态审查不关闭该任务。
- 精确合同缺口：完整产品范围仍按 remaining_gap 保留；已读子合同可以设计条件可用，不因当前 UI 未接通判合同缺失。
- 专业条款依赖：[governance-classification-and-constitutional-execution.design.md](../../world-runtime/runtime/governance-classification-and-constitutional-execution.design.md#governance-classification)；不将内部API与真实远端authority混为一谈。

<a id="t-gov-01-gcb-002"></a>
### 条款 T-GOV-01 / GCB-002

- 唯一上游：[REQ-WR-GCB-002](../../product/world-rules-core-gameplay/governed-common-decisions-and-constitutional-boundaries.prd.md#req-wr-gcb-002)；Trace refs: O7-REQ-79b6687e-3b8c-42a3-a266-df4d191fec06。
- 状态：源设计 `已覆盖`；源实现 `未实现`。这是原状态卡范围，不能从视觉改变实现结论。
- 角色/条件：各独立 producer 分别供审议、延迟、超多数、独立确认、审计、申诉和当前授权；执行者须具本案当前执行权限。。
- 字段：审议与影响说明；延迟 / 复核的权威时间；超多数与独立资格快照；受影响主体独立确认；审计主体与结论；申诉处理 / 当前授权 / 连续记录
- 三轴：当前完整Viewer `not_confirmed; preserve listed narrow consumers`；runtime绑定 `not_verified; no current connection or runtime execution claim`；专业动作合同 `complete_target`。
- 具体表示：[具体目标稿](assets/ui-ux/full-function/governance-targets/frames/B02-GCB-002.svg)；`normal_frame`；字段区域按 `field-1…field-6`，当前未运行。
- 动作「执行宪制事项」：`target_conditionally_available`；任一条件缺失均不可执行；ready 也不是生效；当前图中条件：disabled because authoritative input/permission/read result unavailable; this is separate from contract completeness；启用条件：exact supported operation/schema + current actor role + scope/authority/versions + fresh prerequisites; read-only controls require current read permission and resolvable original record。
- 状态规范：confirmation_pending / appeal_hold / needs_policy；恢复：保持原案例和修订；查询各条件 producer；不可由普通票局代签；完整组合仍text_spec_only。
- [状态片段](assets/ui-ux/full-function/governance-targets/states/S01.svg)；区域：table x30 y160 w1380 h469；disabled_control x36 y660 w650 h52；recovery x36 y755；仅合同示例。
- [状态片段](assets/ui-ux/full-function/governance-targets/states/S13.svg)；区域：inputs x36 y286 w744 h491；checks x820 y286 w744 h491；conditional_ctas y812 h54；仅合同示例。
- 原剩余范围：实现宪制影响说明、延迟/复核、独立确认、审计、申诉/receipt及当前授权合取闸门；缺任一待决/过期/原子拒绝并显示缺项，不产生部分规则或资产效果。 实施待安排： [Refs #4855](https://github.com/nil-studio/oasis7/issues/4855)；本状态审查不关闭该任务。
- 精确合同缺口：完整产品范围仍按 remaining_gap 保留；已读子合同可以设计条件可用，不因当前 UI 未接通判合同缺失。
- 专业条款依赖：[governance-classification-and-constitutional-execution.design.md](../../world-runtime/runtime/governance-classification-and-constitutional-execution.design.md#constitutional-conjunction)；不将内部API与真实远端authority混为一谈。

<a id="t-gov-01-gcb-003"></a>
### 条款 T-GOV-01 / GCB-003

- 唯一上游：[REQ-WR-GCB-003](../../product/world-rules-core-gameplay/governed-common-decisions-and-constitutional-boundaries.prd.md#req-wr-gcb-003)；Trace refs: O7-REQ-4f6597ff-8e34-4d12-bb60-060df0e09595。
- 状态：源设计 `未核实`；源实现 `部分实现`。这是原状态卡范围，不能从视觉改变实现结论。
- 角色/条件：当前有 ExecuteGovernedMatter 权限的主体；执行器固定 ExecutionSlot，非任意 operator 或普通投票者。。
- 字段：原 matter / approved revision；通过时目标、范围与允许效果；当前授权、保护与前置检查；唯一 ExecutionSlot / payload；已知提交点 / execution receipt；实际效果与后继申诉引用
- 三轴：当前完整Viewer `not_confirmed; preserve listed narrow consumers`；runtime绑定 `not_verified; no current connection or runtime execution claim`；专业动作合同 `complete_target`。
- 具体表示：[具体目标稿](assets/ui-ux/full-function/governance-targets/frames/B03-GCB-003.svg)；`normal_frame`；字段区域按 `field-1…field-6`，当前未运行。
- 动作「执行已通过事项」：`target_conditionally_available`；passed 不预留权限；当前授权或范围失效必须拒绝；当前图中条件：disabled because authoritative input/permission/read result unavailable; this is separate from contract completeness；启用条件：exact supported operation/schema + current actor role + scope/authority/versions + fresh prerequisites; read-only controls require current read permission and resolvable original record。
- 状态规范：execution_eligible ≠ effective / outcome_unknown；恢复：查询原执行身份；已提交只重读 receipt；未知不换键重试；完整组合仍text_spec_only。
- [状态片段](assets/ui-ux/full-function/governance-targets/states/S02.svg)；区域：table x30 y160 w1380 h469；disabled_control x36 y660 w650 h52；recovery x36 y755；仅合同示例。
- [状态片段](assets/ui-ux/full-function/governance-targets/states/S13.svg)；区域：inputs x36 y286 w744 h491；checks x820 y286 w744 h491；conditional_ctas y812 h54；仅合同示例。
- 原剩余范围：政策授权只匹配操作者任一Passed提案，不绑定本次proposal_id或批准payload，也没有按该事项消耗；财库distribution_id去重不等于proposal一次消费。具体批准事项的当前scope、撤销/到期、宪制重分类、跨重试单次世界效果及失效读面仍未闭合。 产品语义已冻结，专业技术承接未核到不等于待定产品政策；未检视路径不判未实现。
- 精确合同缺口：完整产品范围仍按 remaining_gap 保留；已读子合同可以设计条件可用，不因当前 UI 未接通判合同缺失。
- 专业条款依赖：[governance-classification-and-constitutional-execution.design.md](../../world-runtime/runtime/governance-classification-and-constitutional-execution.design.md#governance-commit)；不将内部API与真实远端authority混为一谈。

<a id="t-gov-01-gcb-004"></a>
### 条款 T-GOV-01 / GCB-004

- 唯一上游：[REQ-WR-GCB-004](../../product/world-rules-core-gameplay/governed-common-decisions-and-constitutional-boundaries.prd.md#req-wr-gcb-004)；Trace refs: O7-REQ-b584b2ab-bc38-43a1-a8fd-1b89dc3d20cd。
- 状态：源设计 `未核实`；源实现 `未实现`。这是原状态卡范围，不能从视觉改变实现结论。
- 角色/条件：当前读权限 audience 可看允许信息；完整写角色不由本设计补造。。
- 字段：经验证游戏账户；已批准身份 / signer / wallet 绑定摘要；锁定 OC / 有界可撤销委托；控制人聚合资格结果（最小披露）；challenge / review / correction 链；原计票、最终结果与 appeal
- 三轴：当前完整Viewer `not_confirmed; preserve listed narrow consumers`；runtime绑定 `not_verified; no current connection or runtime execution claim`；专业动作合同 `missing_full_write`。
- 具体表示：[具体目标稿](assets/ui-ux/full-function/governance-targets/frames/B04-GCB-004.svg)；`normal_frame`；字段区域按 `field-1…field-6`，当前未运行。
- 动作「申请主体复核」：`contract_blocked`；完整主体聚合与纠错消费者合同未核清；当前图中条件：disabled because authoritative input/permission/read result unavailable; this is separate from contract completeness；启用条件：无可启用控件；见该条非UI/边界说明。
- 状态规范：资格 unknown / review pending / corrected；恢复：保留原投票和 receipt；受权主体查看纠错原因，不公开私密控制证据；完整组合仍text_spec_only。
- 原剩余范围：实现账户/approved bindings主体聚合、事项锁定OC、可撤销scope委托、实际控制人上限、隐私最小化审计和challenge→review→merge/revocation→计票correction→appeal；保留历史且不双计。
- 精确合同缺口：主体聚合/私密控制断言、challenge/merge/revocation/correction 的完整 producer 与正式写接口、当前角色及跨请求幂等链未核清。

<a id="t-gov-01-gcb-005"></a>
### 条款 T-GOV-01 / GCB-005

- 唯一上游：[REQ-WR-GCB-005](../../product/world-rules-core-gameplay/governed-common-decisions-and-constitutional-boundaries.prd.md#req-wr-gcb-005)；Trace refs: O7-REQ-4b64b7c5-9819-4932-a1ad-3cdb09fa82e5。
- 状态：源设计 `未核实`；源实现 `部分实现`。这是原状态卡范围，不能从视觉改变实现结论。
- 角色/条件：当前读权限 audience 可看允许信息；完整写角色不由本设计补造。。
- 字段：外部 OC 转让事实；事项范围内 lock / snapshot；退出或解除绑定的当前结果；有效窗口内原计票历史；OC → LetAI Run quota 单向服务；token_key 与服务消费（非治理）
- 三轴：当前完整Viewer `not_confirmed; preserve listed narrow consumers`；runtime绑定 `not_verified; no current connection or runtime execution claim`；专业动作合同 `read_boundary_only`。
- 具体表示：[具体目标稿](assets/ui-ux/full-function/governance-targets/frames/B05-GCB-005.svg)；`normal_frame`；字段区域按 `field-1…field-6`，当前未运行。
- 动作「查看来源记录」：`target_conditionally_available`；Viewer 不提供链上转账或世界资格兑换；当前图中条件：disabled because authoritative input/permission/read result unavailable; this is separate from contract completeness；启用条件：exact supported operation/schema + current actor role + scope/authority/versions + fresh prerequisites; read-only controls require current read permission and resolvable original record。
- 状态规范：链上 confirmed ≠ 世界 eligible；恢复：分别核对两侧原结果；一侧失败不覆盖另一侧历史；完整组合仍text_spec_only。
- 原剩余范围：外部OC锁定/退出/解绑→具体治理主体及事项snapshot绑定、跨账户/实际控制人并发转移隔离仍未实现于上述入口；operator-managed quota bridge、token_key及服务消费的全部正式消费者和三种状态并列读面仍未核清，不把局部隔离扩成整条已验证。
- 精确合同缺口：完整产品范围仍按 remaining_gap 保留；已读子合同可以设计条件可用，不因当前 UI 未接通判合同缺失。

<a id="t-gov-01-gcb-006"></a>
### 条款 T-GOV-01 / GCB-006

- 唯一上游：[REQ-WR-GCB-006](../../product/world-rules-core-gameplay/governed-common-decisions-and-constitutional-boundaries.prd.md#req-wr-gcb-006)；Trace refs: O7-REQ-c3d848a8-0668-408a-99ca-ef08a7c64be8。
- 状态：源设计 `未核实`；源实现 `部分实现`。这是原状态卡范围，不能从视觉改变实现结论。
- 角色/条件：当前读权限 audience 可看允许信息；完整写角色不由本设计补造。。
- 字段：保护动作与受影响对象；授权主体 / 来源 / 当前范围；理由与可公开证据；生效期限与失效边界；原请求 / 唯一效果 receipt；恢复、复核与申诉入口
- 三轴：当前完整Viewer `not_confirmed; preserve listed narrow consumers`；runtime绑定 `not_verified; no current connection or runtime execution claim`；专业动作合同 `missing_full_write`。
- 具体表示：[具体目标稿](assets/ui-ux/full-function/governance-targets/frames/B06-GCB-006.svg)；`normal_frame`；字段区域按 `field-1…field-6`，当前未运行。
- 动作「执行保护动作」：`contract_blocked`；无精确专业动作与授权合同，不开放冻结/否决/处罚；当前图中条件：disabled because authoritative input/permission/read result unavailable; this is separate from contract completeness；启用条件：无可启用控件；见该条非UI/边界说明。
- 状态规范：revoked / expired / review pending / unknown；恢复：保留受影响范围与原证据；申诉不取消审计，不改已确认历史；完整组合仍text_spec_only。
- [状态片段](assets/ui-ux/full-function/governance-targets/states/S02.svg)；区域：table x30 y160 w1380 h469；disabled_control x36 y660 w650 h52；recovery x36 y755；仅合同示例。
- 原剩余范围：正式提交者和绑定World事件的authority、initiator/guardian本次签名、证据充分性/target/金额/期限、申诉/裁决角色仍需核清或实现；逐类补齐否决/降权等高影响动作、自动到期恢复、紧急防绕过及正式跨入口验证。不得把内部API缺少角色校验写成远程可绕过。
- 精确合同缺口：具体高影响动作类型、专业授权角色/证据schema、唯一效果键、执行/恢复写接口未核清。

<a id="t-gov-02-gr-001"></a>
### 条款 T-GOV-02 / GR-001

- 唯一上游：[REQ-WR-GR-001](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-001)；Trace refs: O7-REQ-d34e2dec-9ad4-44a8-9ba7-7931956e39b8。
- 状态：源设计 `部分覆盖`；源实现 `部分实现`。这是原状态卡范围，不能从视觉改变实现结论。
- 角色/条件：设施动作的当前 capability actor；限 micro_depot 合同，不推广全部区域设施。。
- 字段：区域压力与设施对象；报价成本 / 取舍 / 快照；提交与权威设施结果；有限库存 / 服务消耗；维护、耗尽与回收状态；下一次可用恢复或退役选择
- 三轴：当前完整Viewer `not_confirmed; preserve listed narrow consumers`；runtime绑定 `not_verified; no current connection or runtime execution claim`；专业动作合同 `partial_subscope`。
- 具体表示：[具体目标稿](assets/ui-ux/full-function/governance-targets/frames/B07-GR-001.svg)；`normal_frame`；字段区域按 `field-1…field-6`，当前未运行。
- 动作「查看设施生命周期」：`target_conditionally_available`；micro_depot 仅局部合同；不将报价当预留或顺位；当前图中条件：disabled because authoritative input/permission/read result unavailable; this is separate from contract completeness；启用条件：exact supported operation/schema + current actor role + scope/authority/versions + fresh prerequisites; read-only controls require current read permission and resolvable original record。
- 状态规范：quoted ≠ committed / exhausted；恢复：读取原设施结果；耗尽后按实际支持回收，不泛造区域 CRUD；完整组合仍text_spec_only。
- 原剩余范围：仅 micro_depot single-commission 消耗型设施有本轮代码核对；区域压力仍是 opaque reference，正式压力认证、其他设施与长期补货/epoch 周期不在已证明范围。
- 精确合同缺口：完整产品范围仍按 remaining_gap 保留；已读子合同可以设计条件可用，不因当前 UI 未接通判合同缺失。
- 专业条款依赖：[gameplay-regional-infrastructure-micro-depot-contract.design.md](../../game/gameplay/gameplay-regional-infrastructure-micro-depot-contract.design.md#micro-depot-measured-lifecycle)；不将内部API与真实远端authority混为一谈。

<a id="t-gov-02-gr-004"></a>
### 条款 T-GOV-02 / GR-004

- 唯一上游：[REQ-WR-GR-004](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-004)；Trace refs: O7-REQ-7de05591-35c1-4d8a-94b2-f64fce61976e。
- 状态：源设计 `已覆盖`；源实现 `未实现`。这是原状态卡范围，不能从视觉改变实现结论。
- 角色/条件：提案者提交证据；local/neighbor/global 各自当前 reviewer；激活器重验 required_review_set。。
- 字段：proposal / revision / operation；空间锚定 scope 与前身 charter；设施 / 物流 / 服务能力证据；需求交付 / Agent / 资源 / bond；本地 / 邻区 / 全局 review set；payload 与唯一创建 receipt
- 三轴：当前完整Viewer `not_confirmed; preserve listed narrow consumers`；runtime绑定 `not_verified; no current connection or runtime execution claim`；专业动作合同 `complete_target`。
- 具体表示：[具体目标稿](assets/ui-ux/full-function/governance-targets/frames/B08-GR-004.svg)；`normal_frame`；字段区域按 `field-1…field-6`，当前未运行。
- 动作「提交成立或调整」：`target_conditionally_available`；缺证据、异议或 bond 不足时不部分成立；当前图中条件：disabled because authoritative input/permission/read result unavailable; this is separate from contract completeness；启用条件：exact supported operation/schema + current actor role + scope/authority/versions + fresh prerequisites; read-only controls require current read permission and resolvable original record。
- 状态规范：needs_evidence / scope_objection / review_pending；恢复：保留原修订，重新取得相关审查；unknown 先查原操作；完整组合仍text_spec_only。
- [状态片段](assets/ui-ux/full-function/governance-targets/states/S03.svg)；区域：table x30 y160 w1380 h469；disabled_control x36 y660 w650 h52；recovery x36 y755；仅合同示例。
- 原剩余范围：实现空间锚定能力/需求/资源/bond证据producer、本地/邻区/全局独立权限审查、当前条件重验和唯一charter receipt；缺项/自报/异议/漂移原子拒绝或待补且不产生部分权力。 实施待安排： [Refs #4857](https://github.com/nil-studio/oasis7/issues/4857)；本状态审查不关闭该任务。
- 精确合同缺口：完整产品范围仍按 remaining_gap 保留；已读子合同可以设计条件可用，不因当前 UI 未接通判合同缺失。
- 专业条款依赖：[regional-charter-authority-and-recovery.design.md](../../world-runtime/runtime/regional-charter-authority-and-recovery.design.md#charter-formation)；不将内部API与真实远端authority混为一谈。

<a id="t-gov-02-gr-005"></a>
### 条款 T-GOV-02 / GR-005

- 唯一上游：[REQ-WR-GR-005](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-005)；Trace refs: O7-REQ-ee810b69-8c6a-4aca-86e8-ac387c8f8fcb。
- 状态：源设计 `已覆盖`；源实现 `未实现`。这是原状态卡范围，不能从视觉改变实现结论。
- 角色/条件：LocalEligibility producer 供双来源结果；本地决策主体必须满足该 matter/scope 的现行政策。。
- 字段：charter / authority revision；事项与地方规则适用 scope；OC / 委托 / 控制人资格来源；持续本地贡献来源与窗口；eligible / ineligible / unknown 原因；基本通行与独立恢复保护
- 三轴：当前完整Viewer `not_confirmed; preserve listed narrow consumers`；runtime绑定 `not_verified; no current connection or runtime execution claim`；专业动作合同 `complete_target`。
- 具体表示：[具体目标稿](assets/ui-ux/full-function/governance-targets/frames/B09-GR-005.svg)；`normal_frame`；字段区域按 `field-1…field-6`，当前未运行。
- 动作「查看资格来源」：`target_conditionally_available`；仅声明要求双资格的地方事项适用；不可外推到市场或工业；当前图中条件：disabled because authoritative input/permission/read result unavailable; this is separate from contract completeness；启用条件：exact supported operation/schema + current actor role + scope/authority/versions + fresh prerequisites; read-only controls require current read permission and resolvable original record。
- 状态规范：qualification_incomplete / unknown；恢复：分别补核两侧来源；不把余额、到访或头衔当贡献；完整组合仍text_spec_only。
- [状态片段](assets/ui-ux/full-function/governance-targets/states/S03.svg)；区域：table x30 y160 w1380 h469；disabled_control x36 y660 w650 h52；recovery x36 y755；仅合同示例。
- 原剩余范围：实现持续本地贡献producer、受限OC/可撤回委托/控制人上限合取gate、成熟区域事项边界及独立宪制路由；在该区域权利上下文验证通行、容量结算、替代路线和独立恢复。 实施待安排： [Refs #4857](https://github.com/nil-studio/oasis7/issues/4857)；本状态审查不关闭该任务。
- 精确合同缺口：完整产品范围仍按 remaining_gap 保留；已读子合同可以设计条件可用，不因当前 UI 未接通判合同缺失。
- 专业条款依赖：[regional-charter-authority-and-recovery.design.md](../../world-runtime/runtime/regional-charter-authority-and-recovery.design.md#charter-dual-qualification)；不将内部API与真实远端authority混为一谈。

<a id="t-gov-02-gr-006"></a>
### 条款 T-GOV-02 / GR-006

- 唯一上游：[REQ-WR-GR-006](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-006)；Trace refs: O7-REQ-46939a8f-c2a9-4ab6-818c-6bfa4bc83c5c。
- 状态：源设计 `已覆盖`；源实现 `未实现`。这是原状态卡范围，不能从视觉改变实现结论。
- 角色/条件：授权阶段裁决者；各资产 owner 执行独立子处置，不由汇总页面代签。。
- 字段：active / observing / suspended / dissolved；触发证据与权威观察窗口；能力收缩、恢复条件与异议；bond 与设施、Agent 处置；tenure / 合同 / 身份保留；每对象 pending / applied / child receipt
- 三轴：当前完整Viewer `not_confirmed; preserve listed narrow consumers`；runtime绑定 `not_verified; no current connection or runtime execution claim`；专业动作合同 `complete_target`。
- 具体表示：[具体目标稿](assets/ui-ux/full-function/governance-targets/frames/B10-GR-006.svg)；`normal_frame`；字段区域按 `field-1…field-6`，当前未运行。
- 动作「查看逐项处置」：`target_conditionally_available`；parent 状态不能替代资产子结果；不按短时停机清除；当前图中条件：disabled because authoritative input/permission/read result unavailable; this is separate from contract completeness；启用条件：exact supported operation/schema + current actor role + scope/authority/versions + fresh prerequisites; read-only controls require current read permission and resolvable original record。
- 状态规范：observing / high_impact_suspended；恢复：恢复原 charter 和各子处置；未确认前不再锁 bond；完整组合仍text_spec_only。
- [状态片段](assets/ui-ux/full-function/governance-targets/states/S03.svg)；区域：table x30 y160 w1380 h469；disabled_control x36 y660 w650 h52；recovery x36 y755；仅合同示例。
- 原剩余范围：实现退化事实/时钟、阶段状态与恢复/暂停/解散producer、bond/设施/Agent/tenure/身份/合同历史的规则化处置及receipt、迁移/回收/申诉读面；短时失败或停机不得静默清除。 实施待安排： [Refs #4857](https://github.com/nil-studio/oasis7/issues/4857)；本状态审查不关闭该任务。
- 精确合同缺口：完整产品范围仍按 remaining_gap 保留；已读子合同可以设计条件可用，不因当前 UI 未接通判合同缺失。
- 专业条款依赖：[regional-charter-authority-and-recovery.design.md](../../world-runtime/runtime/regional-charter-authority-and-recovery.design.md#charter-degradation)；不将内部API与真实远端authority混为一谈。

<a id="t-gov-02-gr-007"></a>
### 条款 T-GOV-02 / GR-007

- 唯一上游：[REQ-WR-GR-007](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-007)；Trace refs: O7-REQ-14aa9ab5-d30d-4be2-8c24-610bdc1d1186。
- 状态：源设计 `未核实`；源实现 `未实现`。这是原状态卡范围，不能从视觉改变实现结论。
- 角色/条件：当前读权限 audience 可看允许信息；完整写角色不由本设计补造。。
- 字段：地点 / 用途 / 建设权范围；原取得依据与维护续期；到期、闲置、违约或公共必要性；通知、理由、申诉状态；迁移 / 回收 / 补偿适用结果；原资产、身份、合同与通行
- 三轴：当前完整Viewer `not_confirmed; preserve listed narrow consumers`；runtime绑定 `not_verified; no current connection or runtime execution claim`；专业动作合同 `missing_full_write`。
- 具体表示：[具体目标稿](assets/ui-ux/full-function/governance-targets/frames/B11-GR-007.svg)；`normal_frame`；字段区域按 `field-1…field-6`，当前未运行。
- 动作「申请续期或规则化转让」：`contract_blocked`；tenure 专业完整写合同未核清；建设权不是主权；当前图中条件：disabled because authoritative input/permission/read result unavailable; this is separate from contract completeness；启用条件：无可启用控件；见该条非UI/边界说明。
- 状态规范：expired / disputed / pending authority change；恢复：原绑定待决不跨 charter 自动迁移；查当前结果及可用恢复；完整组合仍text_spec_only。
- [状态片段](assets/ui-ux/full-function/governance-targets/states/S12.svg)；区域：table x30 y160 w1380 h469；disabled_control x36 y660 w650 h52；recovery x36 y755；仅合同示例。
- 原剩余范围：实现地点权利身份、规划horizon与维护/使用条件、规则化转让/续期、预定义收回触发、通知理由/申诉、迁移回收/适用补偿；保持公共通行和既有资产身份合同，不重复转让/没收。
- 精确合同缺口：tenure 取得/续期/转让/收回的具体写接口、资格producer、幂等身份及各类补偿/迁移执行合同未核清。

<a id="t-gov-02-gr-008"></a>
### 条款 T-GOV-02 / GR-008

- 唯一上游：[REQ-WR-GR-008](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-008)；Trace refs: O7-REQ-17536c7e-c59a-464a-90e8-c36f0330c8f7。
- 状态：源设计 `已覆盖`；源实现 `部分实现`。这是原状态卡范围，不能从视觉改变实现结论。
- 角色/条件：合法治理/筹资 authority 提供 LevyAuthority；结算主体在当前专项范围内执行，付款方不能自发升级服务合同。。
- 字段：service_fee / public_levy 类型；charter 预授权与专项目的；受益范围、征收依据与主体；金额 / 资源 / 上限 / 有效期；专项账目、复核与关闭处置；原 assessment / settlement receipt
- 三轴：当前完整Viewer `not_confirmed; preserve listed narrow consumers`；runtime绑定 `not_verified; no current connection or runtime execution claim`；专业动作合同 `complete_target`。
- 具体表示：[具体目标稿](assets/ui-ux/full-function/governance-targets/frames/B12-GR-008.svg)；`normal_frame`；字段区域按 `field-1…field-6`，当前未运行。
- 动作「提交征收结算」：`target_conditionally_available`；缺授权、账目不可核验或评估过期不扣减、不记欠费；当前图中条件：disabled because authoritative input/permission/read result unavailable; this is separate from contract completeness；启用条件：exact supported operation/schema + current actor role + scope/authority/versions + fresh prerequisites; read-only controls require current read permission and resolvable original record。
- 状态规范：authorized 未结算 / settled / outcome_unknown；恢复：按原义务键查结算；正常服务和基本通行不受无效 levy 影响；完整组合仍text_spec_only。
- [状态片段](assets/ui-ux/full-function/governance-targets/states/S04.svg)；区域：table x30 y160 w1380 h469；disabled_control x36 y660 w650 h52；recovery x36 y755；仅合同示例。
- 原剩余范围：实现不可排他公共品levy的charter预授权、目的/受益scope、预算费率上限/期限、公开账目与复核合取gate及到期处置；无效levy不得扣费、欠费或改资格/通行；补齐服务费/levy比较读面及同候选重复/恢复验证。
- 精确合同缺口：完整产品范围仍按 remaining_gap 保留；已读子合同可以设计条件可用，不因当前 UI 未接通判合同缺失。
- 专业条款依赖：[public-levy-authorization-and-settlement.design.md](../../world-runtime/runtime/public-levy-authorization-and-settlement.design.md#levy-read-model)；不将内部API与真实远端authority混为一谈。

<a id="t-gov-03-gr-002"></a>
### 条款 T-GOV-03 / GR-002

- 唯一上游：[REQ-WR-GR-002](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-002)；Trace refs: O7-REQ-ce8c31b1-5294-4acd-a65f-2012bcbef911。
- 状态：源设计 `部分覆盖`；源实现 `部分实现`。这是原状态卡范围，不能从视觉改变实现结论。
- 角色/条件：当前 owner 与审批角色分别复核完整 payload；只承接已读 player publication 子范围。。
- 字段：release / artifact 精确身份；完整 profile payload 承诺；当前 owner 与审批角色；能力范围与世界前置；发布 / 激活各自结果；权威 publication receipt
- 三轴：当前完整Viewer `not_confirmed; preserve listed narrow consumers`；runtime绑定 `not_verified; no current connection or runtime execution claim`；专业动作合同 `partial_subscope`。
- 具体表示：[具体目标稿](assets/ui-ux/full-function/governance-targets/frames/B13-GR-002.svg)；`normal_frame`；字段区域按 `field-1…field-6`，当前未运行。
- 动作「发布或激活扩展」：`target_conditionally_available`；构建成功和旧批准不替代当前角色、payload 重验；当前图中条件：disabled because authoritative input/permission/read result unavailable; this is separate from contract completeness；启用条件：exact supported operation/schema + current actor role + scope/authority/versions + fresh prerequisites; read-only controls require current read permission and resolvable original record。
- 状态规范：stale payload / stale authority / committed；恢复：查原 release receipt；不把本地构建状态改为世界生效；完整组合仍text_spec_only。
- [状态片段](assets/ui-ux/full-function/governance-targets/states/S10.svg)；区域：table x30 y160 w1380 h469；disabled_control x36 y660 w650 h52；recovery x36 y755；仅合同示例。
- 原剩余范围：仅支持玩家发布 Apply 的当前 owner/role、payload/前置重验及冻结新准入切片。全部 install/activation 入口的治理授权期限、撤销/收缩/替代与产品 capability scope 映射仍未核清；通用 v2 grant 校验、构建成功或 module registry active 不等于该全合同。
- 精确合同缺口：完整产品范围仍按 remaining_gap 保留；已读子合同可以设计条件可用，不因当前 UI 未接通判合同缺失。
- 专业条款依赖：[player-published-entities.design.md](../../world-runtime/module/player-published-entities.design.md#ppe-publication-authority)；不将内部API与真实远端authority混为一谈。

<a id="t-gov-03-gr-003"></a>
### 条款 T-GOV-03 / GR-003

- 唯一上游：[REQ-WR-GR-003](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-003)；Trace refs: O7-REQ-ac12295b-63a5-4440-9924-deaedafd7965。
- 状态：源设计 `部分覆盖`；源实现 `部分实现`。这是原状态卡范围，不能从视觉改变实现结论。
- 角色/条件：合法 rollback authority；仅停止 exact FROM artifact 新准入，不推导一般撤销/替代。。
- 字段：被移除 FROM artifact tuple；恢复 TO artifact / rollback 来源；新准入冻结 marker 与理由；既有 profile / instance 保留；重叠能力与现行授权；历史 receipt / 失败原状态
- 三轴：当前完整Viewer `not_confirmed; preserve listed narrow consumers`；runtime绑定 `not_verified; no current connection or runtime execution claim`；专业动作合同 `partial_subscope`。
- 具体表示：[具体目标稿](assets/ui-ux/full-function/governance-targets/frames/B14-GR-003.svg)；`normal_frame`；字段区域按 `field-1…field-6`，当前未运行。
- 动作「查看限制处置」：`target_conditionally_available`；仅精确回滚冻结子范围有合同；一般退出与替代仍未核全；当前图中条件：disabled because authoritative input/permission/read result unavailable; this is separate from contract completeness；启用条件：exact supported operation/schema + current actor role + scope/authority/versions + fresh prerequisites; read-only controls require current read permission and resolvable original record。
- 状态规范：new admission frozen / history retained；恢复：失败保留回滚前事实；旧 receipt 不解冻，不删除既有资产；完整组合仍text_spec_only。
- [状态片段](assets/ui-ux/full-function/governance-targets/states/S10.svg)；区域：table x30 y160 w1380 h469；disabled_control x36 y660 w650 h52；recovery x36 y755；仅合同示例。
- 原剩余范围：仅支持停止新增准入的有限回滚处置；现存其他 instance 不因该 marker 自动停用。一般撤销/到期/收缩、跨重叠 scope 原子替代唯一执行权、限制/退出的最小安全动作及玩家/创作者当前范围与下一步消费链仍未核清。
- 精确合同缺口：完整产品范围仍按 remaining_gap 保留；已读子合同可以设计条件可用，不因当前 UI 未接通判合同缺失。
- 专业条款依赖：[player-published-entities.design.md](../../world-runtime/module/player-published-entities.design.md#ppe-rollback-disposition)；不将内部API与真实远端authority混为一谈。

<a id="t-gov-03-gr-009"></a>
### 条款 T-GOV-03 / GR-009

- 唯一上游：[REQ-WR-GR-009](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-009)；Trace refs: O7-REQ-3d713ab7-87c2-41df-9c89-827859c23313。
- 状态：源设计 `未核实`；源实现 `部分实现`。这是原状态卡范围，不能从视觉改变实现结论。
- 角色/条件：当前读权限 audience 可看允许信息；完整写角色不由本设计补造。。
- 字段：能力提案与不可变输入；确定性模拟来源与结果；试点 scope / 期限 / 资源上限；试点退出条件与实际结果；治理准入范围与来源；当前可用能力与历史
- 三轴：当前完整Viewer `not_confirmed; preserve listed narrow consumers`；runtime绑定 `not_verified; no current connection or runtime execution claim`；专业动作合同 `missing_full_write`。
- 具体表示：[具体目标稿](assets/ui-ux/full-function/governance-targets/frames/B15-GR-009.svg)；`normal_frame`；字段区域按 `field-1…field-6`，当前未运行。
- 动作「申请下一阶段」：`contract_blocked`；单一阶段不授世界写入权；完整阶段合同未核清；当前图中条件：disabled because authoritative input/permission/read result unavailable; this is separate from contract completeness；启用条件：无可启用控件；见该条非UI/边界说明。
- 状态规范：proposal ≠ simulation ≠ pilot ≠ admission；恢复：保持各阶段原身份；缺批准不自动进入后段；完整组合仍text_spec_only。
- 原剩余范围：这只覆盖工业profile发布准入。Shadow是manifest/profile校验承诺，不是同authority输入的工业确定性模拟；未核到独立有限试点scope/期限/资源/退出stage、四阶段强制顺序、风险证据/成本与玩家选择。required-role绑定也不代签全套法定治理。 产品语义已冻结，专业技术承接未核到不等于待定产品政策；未检视路径不判未实现。
- 精确合同缺口：提案→模拟→试点→准入四段具体 producer/写接口、当前角色集合及跨阶段身份/恢复协议未核清。

<a id="t-gov-03-gr-010"></a>
### 条款 T-GOV-03 / GR-010

- 唯一上游：[REQ-WR-GR-010](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-010)；Trace refs: O7-REQ-0ec3d72f-51d1-4d64-9824-ebd9365f2d47。
- 状态：源设计 `未核实`；源实现 `未实现`。这是原状态卡范围，不能从视觉改变实现结论。
- 角色/条件：当前读权限 audience 可看允许信息；完整写角色不由本设计补造。。
- 字段：能力 / 用途 / 区域；期限与到期开放条件；许可 / 版税 / 收益权依据；维护责任与审计；争议和公共保护边界；范围变更及重新准入
- 三轴：当前完整Viewer `not_confirmed; preserve listed narrow consumers`；runtime绑定 `not_verified; no current connection or runtime execution claim`；专业动作合同 `missing_full_write`。
- 具体表示：[具体目标稿](assets/ui-ux/full-function/governance-targets/frames/B16-GR-010.svg)；`normal_frame`；字段区域按 `field-1…field-6`，当前未运行。
- 动作「申请范围变更」：`contract_blocked`；无精确许可写合同；不得永久封锁公共安全或独立资产；当前图中条件：disabled because authoritative input/permission/read result unavailable; this is separate from contract completeness；启用条件：无可启用控件；见该条非UI/边界说明。
- 状态规范：expired / disputed / open condition pending；恢复：显示原许可与现行范围；不可追溯抹去作者身份；完整组合仍text_spec_only。
- 原剩余范围：实现已获准工业能力许可身份、用途/区域/期限/维护/争议/撤销/开放状态、使用authorization consumer、verified receipt/escrow里程碑版税结算与到期开放producer；旧请求须当前授权且不延长/复制/追溯改写合法历史。
- 精确合同缺口：许可/版税/开放的正式写接口、适用角色、费用/权益执行与幂等恢复合同未核清。

<a id="t-gov-03-sc32-institution"></a>
### 条款 T-GOV-03 / SC32-institution

- 唯一上游：[SC32-institution](../../product/world-rules-core-gameplay/prd.md#root-sc32-institution)；Trace refs: O7-REQ-90614e34-bbc7-42df-b355-7ce0b34ccdc3。
- 状态：源设计 `未核实`；源实现 `未核实`。这是原状态卡范围，不能从视觉改变实现结论。
- 角色/条件：当前读权限 audience 可看允许信息；完整写角色不由本设计补造。。
- 字段：Alliance — Compatibility keep；War — Defer；Governance — Kernel keep；EconomicContract — Compatibility keep；未互斥分类不得新增 native action/state；分类为工程约束，不是玩家管理权限
- 三轴：当前完整Viewer `not_confirmed; preserve listed narrow consumers`；runtime绑定 `not_verified; no current connection or runtime execution claim`；专业动作合同 `non_ui`。
- 具体表示：无独立frame；`text_spec_only`；字段区域按 `field-1…field-6`，当前未运行。
- 动作「无玩家写动作」：`not_a_player_action`；非 UI 权威：不设计 native 制度编辑器；当前图中条件：disabled because authoritative input/permission/read result unavailable; this is separate from contract completeness；启用条件：无可启用控件；见该条非UI/边界说明。
- 状态规范：分类不等于迁移已发生；恢复：保留原制度历史；专业变更审查核对分类；完整组合仍text_spec_only。
- 原剩余范围：四个既有native制度精确互斥分类，未分类不得新增native action/state。未核范围具体保留，不把政策、设计和通用能力提升为制度迁移事实。 需按实际变更批准记录或已选pilot manifest/activation、旧结果连续性证据重新核对；不以缺运行证据单独否定源码。
- 精确合同缺口：完整产品范围仍按 remaining_gap 保留；已读子合同可以设计条件可用，不因当前 UI 未接通判合同缺失。
- 专业条款依赖：[design.md](../../world-runtime/design.md#61-institution-migration-test)；不将内部API与真实远端authority混为一谈。

<a id="t-gov-03-sc32-pilot"></a>
### 条款 T-GOV-03 / SC32-pilot

- 唯一上游：[SC32-pilot](../../product/world-rules-core-gameplay/prd.md#root-sc32-pilot)；Trace refs: O7-REQ-f081f96d-f95a-4538-b0b7-3edf23ff772a。
- 状态：源设计 `未核实`；源实现 `未核实`。这是原状态卡范围，不能从视觉改变实现结论。
- 角色/条件：当前读权限 audience 可看允许信息；完整写角色不由本设计补造。。
- 字段：Alliance 或 EconomicContract 唯一候选；另一制度保持兼容；治理批准 manifest / activation；L0 / L1 不可绕过；旧 snapshot / replay / ABI 连续性；同世界玩家结果与失败恢复
- 三轴：当前完整Viewer `not_confirmed; preserve listed narrow consumers`；runtime绑定 `not_verified; no current connection or runtime execution claim`；专业动作合同 `read_boundary_only`。
- 具体表示：[具体目标稿](assets/ui-ux/full-function/governance-targets/frames/B18-SC32-pilot.svg)；`normal_frame`；字段区域按 `field-1…field-6`，当前未运行。
- 动作「查看已选试点」：`target_conditionally_available`；选择尚未核定时不显示默认候选；无玩家任意切换；当前图中条件：disabled because authoritative input/permission/read result unavailable; this is separate from contract completeness；启用条件：exact supported operation/schema + current actor role + scope/authority/versions + fresh prerequisites; read-only controls require current read permission and resolvable original record。
- 状态规范：not selected / activation unverified；恢复：保留兼容状态；未知不推断已迁移；完整组合仍text_spec_only。
- 原剩余范围：条件满足后选择唯一Alliance/EconomicContract，合法activation、另一兼容，L0/L1/ownerbudget/receipt及旧snapshot/replay/ABI恢复。未核范围具体保留，不把政策、设计和通用能力提升为制度迁移事实。 需按实际变更批准记录或已选pilot manifest/activation、旧结果连续性证据重新核对；不以缺运行证据单独否定源码。
- 精确合同缺口：完整产品范围仍按 remaining_gap 保留；已读子合同可以设计条件可用，不因当前 UI 未接通判合同缺失。
- 专业条款依赖：[design.md](../../world-runtime/design.md#61-institution-migration-test)；不将内部API与真实远端authority混为一谈。

<a id="t-gov-03-sc32-budgetread"></a>
### 条款 T-GOV-03 / SC32-budgetread

- 唯一上游：[SC32-budgetread](../../product/world-rules-core-gameplay/prd.md#root-sc32-budgetread)；Trace refs: O7-REQ-96cb1655-2ca3-445f-af2e-9f113e6f3e8e。
- 状态：源设计 `未核实`；源实现 `部分实现`。这是原状态卡范围，不能从视觉改变实现结论。
- 角色/条件：当前读权限 audience 可看允许信息；完整写角色不由本设计补造。。
- 字段：Agent / Module / System owner；payer 与绑定来源；能力 scope / 是否计费；quote units 与预算影响；affordable / insufficient / unknown / degraded；Data / Electricity 资源费单列
- 三轴：当前完整Viewer `not_confirmed; preserve listed narrow consumers`；runtime绑定 `not_verified; no current connection or runtime execution claim`；专业动作合同 `read_boundary_only`。
- 具体表示：[具体目标稿](assets/ui-ux/full-function/governance-targets/frames/B19-SC32-budgetread.svg)；`normal_frame`；字段区域按 `field-1…field-6`，当前未运行。
- 动作「重新读取预算依据」：`target_conditionally_available`；owner / payer 不可推定；unknown 不可按零费用处理；当前图中条件：disabled because authoritative input/permission/read result unavailable; this is separate from contract completeness；启用条件：exact supported operation/schema + current actor role + scope/authority/versions + fresh prerequisites; read-only controls require current read permission and resolvable original record。
- 状态规范：unknown / degraded；恢复：核对原绑定与 grant，不自动转移 payer 或预算；完整组合仍text_spec_only。
- [状态片段](assets/ui-ux/full-function/governance-targets/states/S10.svg)；区域：table x30 y160 w1380 h469；disabled_control x36 y660 w650 h52；recovery x36 y755；仅合同示例。
- 原剩余范围：补全Agent及Module/System的owner/payer映射、能力scope/是否计费/预算影响、affordable/insufficient/unknown/degraded与复查修复路径，贯通真实Viewer/API或能力预览/提交结果；不得以原始账本getter、snapshot或Data/Electricity资源费替代玩家execution-budget读面。保留SC-32适用条件和test_tier_full；本次未运行。
- 精确合同缺口：完整产品范围仍按 remaining_gap 保留；已读子合同可以设计条件可用，不因当前 UI 未接通判合同缺失。
- 专业条款依赖：[design.md](../../world-runtime/design.md#61-institution-migration-test)；不将内部API与真实远端authority混为一谈。

<a id="t-gov-03-sc32-budgetcommit"></a>
### 条款 T-GOV-03 / SC32-budgetcommit

- 唯一上游：[SC32-budgetcommit](../../product/world-rules-core-gameplay/prd.md#root-sc32-budgetcommit)；Trace refs: O7-REQ-70e55a1f-7087-41e8-918d-7d1989c0155a。
- 状态：源设计 `未核实`；源实现 `部分实现`。这是原状态卡范围，不能从视觉改变实现结论。
- 角色/条件：当前读权限 audience 可看允许信息；完整写角色不由本设计补造。。
- 字段：原操作身份与 grant；执行前预算 / 当前 owner；actual effect 与 debit 关联；authorization receipt / effect completion；preview / stale / deny 无效果分支；已提交 / 未提交 / unknown
- 三轴：当前完整Viewer `not_confirmed; preserve listed narrow consumers`；runtime绑定 `not_verified; no current connection or runtime execution claim`；专业动作合同 `read_boundary_only`。
- 具体表示：[具体目标稿](assets/ui-ux/full-function/governance-targets/frames/B20-SC32-budgetcommit.svg)；`normal_frame`；字段区域按 `field-1…field-6`，当前未运行。
- 动作「核对原操作结果」：`target_conditionally_available`；unknown 不透支、不发第二次动作；执行队列不等于外部完成；当前图中条件：disabled because authoritative input/permission/read result unavailable; this is separate from contract completeness；启用条件：exact supported operation/schema + current actor role + scope/authority/versions + fresh prerequisites; read-only controls require current read permission and resolvable original record。
- 状态规范：outcome_unknown / effect completion pending；恢复：查询原 receipt 和实际效果；不更换 nonce 制造再扣费；完整组合仍text_spec_only。
- [状态片段](assets/ui-ux/full-function/governance-targets/states/S10.svg)；区域：table x30 y160 w1380 h469；disabled_control x36 y660 w650 h52；recovery x36 y755；仅合同示例。
- 原剩余范围：继续贯通全部适用生产入口preview/stale/deny/no-effect无扣账与零预算操作；核准实际effect和receipt边界、外部effect completion及持久unknown恢复，给出同候选test_tier_full单次结算/重连/restore/replay/no-credit证据。已定位直接实现但全义务未闭合，故部分实现而非仅欠验收的待验证。
- 精确合同缺口：完整产品范围仍按 remaining_gap 保留；已读子合同可以设计条件可用，不因当前 UI 未接通判合同缺失。
- 专业条款依赖：[design.md](../../world-runtime/design.md#622-disposition-and-idempotency-contract)；不将内部API与真实远端authority混为一谈。

<a id="t-gov-03-sc33-extscope"></a>
### 条款 T-GOV-03 / SC33-extscope

- 唯一上游：[SC33-extscope](../../product/world-rules-core-gameplay/prd.md#root-sc33-extscope)；Trace refs: O7-REQ-e6f4ffa7-0b1b-4d0e-b7a3-8ca38c8d0244。
- 状态：源设计 `已覆盖`；源实现 `未实现`。这是原状态卡范围，不能从视觉改变实现结论。
- 角色/条件：profile 声明动作与当前 mitigation authority 同时适用的 owner/scope 主体；不授地方治理权。。
- 字段：industrial_externality / operational_mitigation；recipe / factory profile 与版本；scope、authority 与有效期；容量与已有 obligation；支持的 run / reduce / defer 选择；legacy 未声明分支
- 三轴：当前完整Viewer `not_confirmed; preserve listed narrow consumers`；runtime绑定 `not_verified; no current connection or runtime execution claim`；专业动作合同 `complete_target`。
- 具体表示：[具体目标稿](assets/ui-ux/full-function/governance-targets/frames/B21-SC33-extscope.svg)；`normal_frame`；字段区域按 `field-1…field-6`，当前未运行。
- 动作「选择受支持方案」：`target_conditionally_available`；缺 authority / scope 时禁止不可逆动作；legacy 不被强行改造；当前图中条件：disabled because authoritative input/permission/read result unavailable; this is separate from contract completeness；启用条件：exact supported operation/schema + current actor role + scope/authority/versions + fresh prerequisites; read-only controls require current read permission and resolvable original record。
- 状态规范：legacy unchanged / unknown / blocked；恢复：查看当前 profile 和复查点；不把 profile 当区域治理权；完整组合仍text_spec_only。
- [状态片段](assets/ui-ux/full-function/governance-targets/states/S10.svg)；区域：table x30 y160 w1380 h469；disabled_control x36 y660 w650 h52；recovery x36 y755；仅合同示例。
- 原剩余范围：明确industrial_externality/operational_mitigation profile才启用：实际RecipeProfile/FactoryProfile及内置M4 plan没有此facet与启用consumer；legacy无profile不变：现有legacy仍运行是邻域既有事实；没有引入新facet后的分支/兼容合同，不能用未实施即未变化作partial；scope/authority缺失过期unknown/blocked且不授区域治理权：当前排程guard处理普通工厂/配方/材料/电力，不读取mitigation authority，无法对声明该profile的case作要求的判断。结论限当前仓库默认/内置及已审正式路径；未提供的线上自定义artifact不在本轮结论范围。 实施待安排： [Refs #4862](https://github.com/nil-studio/oasis7/issues/4862)；本状态审查不关闭该任务。
- 精确合同缺口：完整产品范围仍按 remaining_gap 保留；已读子合同可以设计条件可用，不因当前 UI 未接通判合同缺失。
- 专业条款依赖：[industrial-externality-and-mitigation.design.md](../../world-runtime/runtime/industrial-externality-and-mitigation.design.md#externality-profile)；不将内部API与真实远端authority混为一谈。

<a id="t-gov-03-sc33-extdispose"></a>
### 条款 T-GOV-03 / SC33-extdispose

- 唯一上游：[SC33-extdispose](../../product/world-rules-core-gameplay/prd.md#root-sc33-extdispose)；Trace refs: O7-REQ-bd83785c-d414-4bd7-b291-41a4d7272aa3。
- 状态：源设计 `已覆盖`；源实现 `未实现`。这是原状态卡范围，不能从视觉改变实现结论。
- 角色/条件：原工业/外部性义务的当前合法处置主体；依 profile 支持动作和阶段 sink 边界逐次校验。。
- 字段：原 root / revision / child；WIP / transit / buffer / terminal；production result 与 disposition refs；contained / held_pending_disposition；保留容量、成本与后继义务；quality / custody / W 各自历史
- 三轴：当前完整Viewer `not_confirmed; preserve listed narrow consumers`；runtime绑定 `not_verified; no current connection or runtime execution claim`；专业动作合同 `complete_target`。
- 具体表示：[具体目标稿](assets/ui-ux/full-function/governance-targets/frames/B22-SC33-extdispose.svg)；`normal_frame`；字段区域按 `field-1…field-6`，当前未运行。
- 动作「核对联合结果」：`target_conditionally_available`；held 不代表 treatment 完成；无安全路径不伪造产出；当前图中条件：disabled because authoritative input/permission/read result unavailable; this is separate from contract completeness；启用条件：exact supported operation/schema + current actor role + scope/authority/versions + fresh prerequisites; read-only controls require current read permission and resolvable original record。
- 状态规范：held_pending_disposition / blocked；恢复：按原义务继续合法处置；不重置 lineage 或重复释放；完整组合仍text_spec_only。
- [状态片段](assets/ui-ux/full-function/governance-targets/states/S10.svg)；区域：table x30 y160 w1380 h469；disabled_control x36 y660 w650 h52；recovery x36 y755；仅合同示例。
- 原剩余范围：preview只读、submit fresh、linked production/externality各一次：普通quote与生产receipt存在，但没有externality intent/disposition身份及linked写点；不把通用只读和job exactly-once当外部性完成；quality/custody/byproduct/maintenance/settlement/W互不改写：不存在外部性处置producer可证明隔离；当前材料/维护/物流分别写不等同新facet隔离已交付；WIP/transit/buffer/terminal保持root/revision和既有义务单次处置：普通pending job/transit有独立结算，没有externality profile变化的root/revision/WIP及terminal处置consumer。结论限当前仓库默认/内置及已审正式路径；未提供的线上自定义artifact不在本轮结论范围。 实施待安排： [Refs #4862](https://github.com/nil-studio/oasis7/issues/4862)；本状态审查不关闭该任务。
- 精确合同缺口：完整产品范围仍按 remaining_gap 保留；已读子合同可以设计条件可用，不因当前 UI 未接通判合同缺失。
- 专业条款依赖：[industrial-externality-and-mitigation.design.md](../../world-runtime/runtime/industrial-externality-and-mitigation.design.md#externality-commit)；不将内部API与真实远端authority混为一谈。

<a id="t-gov-04-cr-001"></a>
### 条款 T-GOV-04 / CR-001

- 唯一上游：[REQ-WR-CR-001](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-001)；Trace refs: O7-REQ-16ef3247-1587-4cd8-84ab-03a9bb18a17f。
- 状态：源设计 `未核实`；源实现 `部分实现`。这是原状态卡范围，不能从视觉改变实现结论。
- 角色/条件：当前读权限 audience 可看允许信息；完整写角色不由本设计补造。。
- 字段：沟通 / 草稿 / 未接受提议；各方主体与受权范围；完整条款与明确接受；权威校验与 receipt；已绑定义务与实际效果；修订及原条款 provenance
- 三轴：当前完整Viewer `not_confirmed; preserve listed narrow consumers`；runtime绑定 `not_verified; no current connection or runtime execution claim`；专业动作合同 `missing_full_write`。
- 具体表示：[具体目标稿](assets/ui-ux/full-function/governance-targets/frames/B23-CR-001.svg)；`normal_frame`；字段区域按 `field-1…field-6`，当前未运行。
- 动作「接受明确条款」：`contract_blocked`；完整绑定合同专业写合同未核清；chat 不是确认；当前图中条件：disabled because authoritative input/permission/read result unavailable; this is separate from contract completeness；启用条件：无可启用控件；见该条非UI/边界说明。
- 状态规范：draft ≠ accepted ≠ committed；恢复：未知查原合同，不重复接受或静默转移责任；完整组合仍text_spec_only。
- [状态片段](assets/ui-ux/full-function/governance-targets/states/S11.svg)；区域：table x30 y160 w1380 h469；disabled_control x36 y660 w650 h52；recovery x36 y755；仅合同示例。
- 原剩余范围：未核清明确条款渲染、双方授权接受、绑定义务创建及同一 receipt 的端到端流程；社交消息或 action ack 不证明合同已成立。
- 精确合同缺口：完整绑定条款schema、各方授权与accept接口、权威commit/receipt/幂等及未知恢复闭包未核清。

<a id="t-gov-04-cr-002"></a>
### 条款 T-GOV-04 / CR-002

- 唯一上游：[REQ-WR-CR-002](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-002)；Trace refs: O7-REQ-805fc92a-bbcb-4e77-b2d8-e40105522a0d。
- 状态：源设计 `未核实`；源实现 `未实现`。这是原状态卡范围，不能从视觉改变实现结论。
- 角色/条件：当前读权限 audience 可看允许信息；完整写角色不由本设计补造。。
- 字段：服务身份 / 原合同；已完成 checkpoint 与结果；未履行范围与责任方；修复或重新履行；终止结清 / 最小争议保全；显式继受与关联 receipt
- 三轴：当前完整Viewer `not_confirmed; preserve listed narrow consumers`；runtime绑定 `not_verified; no current connection or runtime execution claim`；专业动作合同 `missing_full_write`。
- 具体表示：[具体目标稿](assets/ui-ux/full-function/governance-targets/frames/B24-CR-002.svg)；`normal_frame`；字段区域按 `field-1…field-6`，当前未运行。
- 动作「选择受支持恢复」：`contract_blocked`；各选项必须有服务合同支撑；不把续期当重复交付许可；当前图中条件：disabled because authoritative input/permission/read result unavailable; this is separate from contract completeness；启用条件：无可启用控件；见该条非UI/边界说明。
- 状态规范：interrupted / disputed / outcome_unknown；恢复：保留完成部分；原请求核对扣款、履行与继受，不自动重试；完整组合仍text_spec_only。
- [状态片段](assets/ui-ux/full-function/governance-targets/states/S11.svg)；区域：table x30 y160 w1380 h469；disabled_control x36 y660 w650 h52；recovery x36 y755；仅合同示例。
- 原剩余范围：持续服务的collateral/evidence/remedy、检查点已履行/未履行责任、修复/终止/dispute/preserve、续期重报价与重试去重尚未交付；不能以AtomicExchange或通用恢复代替。
- 精确合同缺口：服务checkpoint producer、修复/结清/争议/继受具体命令、角色及资源义务恢复合同未核清。

<a id="t-gov-04-cr-003"></a>
### 条款 T-GOV-04 / CR-003

- 唯一上游：[REQ-WR-CR-003](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-003)；Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905。
- 状态：源设计 `未核实`；源实现 `部分实现`。这是原状态卡范围，不能从视觉改变实现结论。
- 角色/条件：受当前事实 scope 与共享资格校验的 actor；这里只设计已有 Simulator 只读预览子范围，不启用完整发布/R&D。。
- 字段：actor / subject / object / action kind；claim summary / confidence / stake risk；受影响关系与 social surfaces；cooperation / dispute / governance relevance；声誉主体、情境与到期 / 申诉；R&D charter 贡献份额 / Agent provenance
- 三轴：当前完整Viewer `not_confirmed; preserve listed narrow consumers`；runtime绑定 `not_verified; no current connection or runtime execution claim`；专业动作合同 `partial_subscope`。
- 具体表示：[具体目标稿](assets/ui-ux/full-function/governance-targets/frames/B25-CR-003.svg)；`normal_frame`；字段区域按 `field-1…field-6`，当前未运行。
- 动作「查看影响；暂缓发布」：`target_conditionally_available`；Simulator 预览仅局部；完整 R&D 消费及正式写入口未核清；当前图中条件：disabled because authoritative input/permission/read result unavailable; this is separate from contract completeness；启用条件：exact supported operation/schema + current actor role + scope/authority/versions + fresh prerequisites; read-only controls require current read permission and resolvable original record。
- 状态规范：read-only preview / expired / challenged；恢复：预览不裁决、不扣质押；份额转移不转 credential 或一般声誉；完整组合仍text_spec_only。
- [状态片段](assets/ui-ux/full-function/governance-targets/states/S11.svg)；区域：table x30 y160 w1380 h469；disabled_control x36 y660 w650 h52；recovery x36 y755；仅合同示例。
- 原剩余范围：社会事实专业设计覆盖 lifecycle/只读预览的一部分，但与本条的准确反链及 R&D charter、份额/credential 不转移、机会消费组合未闭合。
- 精确合同缺口：完整产品范围仍按 remaining_gap 保留；已读子合同可以设计条件可用，不因当前 UI 未接通判合同缺失。
- 专业条款依赖：[social-fact-ledger-declarative-reputation.design.md](../kernel/social-fact-ledger-declarative-reputation.design.md#social-lifecycle-design)；不将内部API与真实远端authority混为一谈。

<a id="t-gov-05-oc-001"></a>
### 条款 T-GOV-05 / OC-001

- 唯一上游：[REQ-WR-OC-001](../../product/world-rules-core-gameplay/organization-continuity-dissolution-and-dormancy-protection.prd.md#req-wr-oc-001)；Trace refs: O7-REQ-366c9e94-4bd6-41f8-93dd-8d87f163f21a。
- 状态：源设计 `已覆盖`；源实现 `部分实现`。这是原状态卡范围，不能从视觉改变实现结论。
- 角色/条件：原 appointment authority 且当前治理通过；首版仅 Disposition/CustodyReturn 或明确合格 successor scope 的完整 held-in-trust 返还。。
- 字段：organization / case / policy / manifest；Inventory → Obligations → CustodyReturn；Claims → BusinessDisposition → Residual；每项 owner / held-in-trust contributor；原义务、占用、已返还与剩余；effect key 与子 receipt
- 三轴：当前完整Viewer `not_confirmed; preserve listed narrow consumers`；runtime绑定 `not_verified; no current connection or runtime execution claim`；专业动作合同 `complete_target`。
- 具体表示：[具体目标稿](assets/ui-ux/full-function/governance-targets/frames/B26-OC-001.svg)；`normal_frame`；字段区域按 `field-1…field-6`，当前未运行。
- 动作「处置受支持托管返还」：`target_conditionally_available`；首版仅 canonical estate receipt 的有限返还；未知对象拒绝；当前图中条件：disabled because authoritative input/permission/read result unavailable; this is separate from contract completeness；启用条件：exact supported operation/schema + current actor role + scope/authority/versions + fresh prerequisites; read-only controls require current read permission and resolvable original record。
- 状态规范：AwaitingEvidence / PartiallyApplied；恢复：不把未知 estate 当空；按原 asset lineage 查结果，不重复处分；完整组合仍text_spec_only。
- [状态片段](assets/ui-ux/full-function/governance-targets/states/S05.svg)；区域：table x30 y160 w1380 h469；disabled_control x36 y660 w650 h52；recovery x36 y755；仅合同示例。
- [状态片段](assets/ui-ux/full-function/governance-targets/states/S14.svg)；区域：inputs x36 y286 w744 h491；checks x820 y286 w744 h491；conditional_ctas y812 h54；仅合同示例。
- 原剩余范围：设计已由本次 v1 政策和专业接口承接；实现仍须接通组织 case、白名单权属、原业务事务 adapter、多阶段处置、来源守恒与 receipt/恢复读面，并执行复杂资产、外部结算及跨节点验收。已有静态证据只保留独立成员资源/身份/合同的窄负边界，不能覆盖本次新政策。[实现任务 #4868](https://github.com/nil-studio/oasis7/issues/4868) 保持未完成。
- 精确合同缺口：完整产品范围仍按 remaining_gap 保留；已读子合同可以设计条件可用，不因当前 UI 未接通判合同缺失。
- 专业条款依赖：[organization-continuity-and-disposition.design.md](../../world-runtime/runtime/organization-continuity-and-disposition.design.md#des-wr-oc-001)；不将内部API与真实远端authority混为一谈。

<a id="t-gov-05-oc-002"></a>
### 条款 T-GOV-05 / OC-002

- 唯一上游：[REQ-WR-OC-002](../../product/world-rules-core-gameplay/organization-continuity-dissolution-and-dormancy-protection.prd.md#req-wr-oc-002)；Trace refs: O7-REQ-6e6d890d-c94d-4b43-9947-bfccf2f066e3。
- 状态：源设计 `已覆盖`；源实现 `未实现`。这是原状态卡范围，不能从视觉改变实现结论。
- 角色/条件：具权利来源的 reclaim/appeal 申请人；独立救济裁决者判定支持范围，不由申请人自行复权。。
- 字段：Operating / Notified / Protected / Restricted；availability / activity coverage 分列；权威 C 窗口与暂停原因；通知记录与保护期；reclaim / appeal / 独立 stay；裁决支持范围与剩余义务
- 三轴：当前完整Viewer `not_confirmed; preserve listed narrow consumers`；runtime绑定 `not_verified; no current connection or runtime execution claim`；专业动作合同 `complete_target`。
- 具体表示：[具体目标稿](assets/ui-ux/full-function/governance-targets/frames/B27-OC-002.svg)；`normal_frame`；字段区域按 `field-1…field-6`，当前未运行。
- 动作「申请 reclaim 或申诉」：`target_conditionally_available`；须有效 typed charter / activation；申诉待决不自动复权；当前图中条件：disabled because authoritative input/permission/read result unavailable; this is separate from contract completeness；启用条件：exact supported operation/schema + current actor role + scope/authority/versions + fresh prerequisites; read-only controls require current read permission and resolvable original record。
- 状态规范：coverage unknown / UnderReview；恢复：查询原 case 与救济；未知时段不补算期限，维护权不升级为处分权；完整组合仍text_spec_only。
- [状态片段](assets/ui-ux/full-function/governance-targets/states/S05.svg)；区域：table x30 y160 w1380 h469；disabled_control x36 y660 w650 h52；recovery x36 y755；仅合同示例。
- 原剩余范围：首版活动/期限/资格与请求合同已明确；仍须实现组织基础阶段、可核验可用性计时、通知、受限 estate/delegation、reclaim/申诉 consumer，并按边界、暂停、重复与恢复场景验证。不能借 Agent claim 或普通 grant 验收代签。[实现任务 #4868](https://github.com/nil-studio/oasis7/issues/4868) 保持未完成。
- 精确合同缺口：完整产品范围仍按 remaining_gap 保留；已读子合同可以设计条件可用，不因当前 UI 未接通判合同缺失。
- 专业条款依赖：[organization-continuity-and-disposition.design.md](../../world-runtime/runtime/organization-continuity-and-disposition.design.md#des-wr-oc-002)；不将内部API与真实远端authority混为一谈。

<a id="t-gov-05-oc-003"></a>
### 条款 T-GOV-05 / OC-003

- 唯一上游：[REQ-WR-OC-003](../../product/world-rules-core-gameplay/organization-continuity-dissolution-and-dormancy-protection.prd.md#req-wr-oc-003)；Trace refs: O7-REQ-b59a4fa5-9b5f-4119-abf6-7c0f2b4f4268。
- 状态：源设计 `已覆盖`；源实现 `未实现`。这是原状态卡范围，不能从视觉改变实现结论。
- 角色/条件：当前任命 authority 与通过当前控制绑定/顺位核验并接受的 successor；重叠旧授权原子终止。。
- 字段：旧 grant / 新候选资格绑定；object / right / action / quantity scope；重叠范围与 stay / quarantine；原请求绑定 grant / scope version；新授权 receipt 与旧授权终止；未确认请求与剩余义务
- 三轴：当前完整Viewer `not_confirmed; preserve listed narrow consumers`；runtime绑定 `not_verified; no current connection or runtime execution claim`；专业动作合同 `complete_target`。
- 具体表示：[具体目标稿](assets/ui-ux/full-function/governance-targets/frames/B28-OC-003.svg)；`normal_frame`；字段区域按 `field-1…field-6`，当前未运行。
- 动作「确认有界交接」：`target_conditionally_available`；重叠范围只一个可执行授权；旧 pending 不迁移；当前图中条件：disabled because authoritative input/permission/read result unavailable; this is separate from contract completeness；启用条件：exact supported operation/schema + current actor role + scope/authority/versions + fresh prerequisites; read-only controls require current read permission and resolvable original record。
- 状态规范：proposed ≠ active / expired / result_unknown；恢复：原 case + request 核对；不换 case 或 action 名重复处分；完整组合仍text_spec_only。
- [状态片段](assets/ui-ux/full-function/governance-targets/states/S05.svg)；区域：table x30 y160 w1380 h469；disabled_control x36 y660 w650 h52；recovery x36 y755；仅合同示例。
- [状态片段](assets/ui-ux/full-function/governance-targets/states/S14.svg)；区域：inputs x36 y286 w744 h491；checks x820 y286 w744 h491；conditional_ctas y812 h54；仅合同示例。
- 原剩余范围：scope/部分交接、政策/schema 和接口已由本次设计承接；仍须实现唯一授权索引、资格 consumer、新旧原子切换、原业务占用及跨 request/value 去重，验证旧新竞态、冻结/截止 tick、snapshot/replay 和外部 effect 的事务/恢复边界。[实现任务 #4868](https://github.com/nil-studio/oasis7/issues/4868) 保持未完成。
- 精确合同缺口：完整产品范围仍按 remaining_gap 保留；已读子合同可以设计条件可用，不因当前 UI 未接通判合同缺失。
- 专业条款依赖：[organization-continuity-and-disposition.design.md](../../world-runtime/runtime/organization-continuity-and-disposition.design.md#des-wr-oc-003)；不将内部API与真实远端authority混为一谈。

<a id="t-gov-06-cc-001"></a>
### 条款 T-GOV-06 / CC-001

- 唯一上游：[REQ-WR-CC-001](../../product/world-rules-core-gameplay/chartered-conflict-soft-seasons-and-recovery.prd.md#req-wr-cc-001)；Trace refs: O7-REQ-2e68a48f-4a44-4135-9268-e17c6f32d0b3。
- 状态：源设计 `部分覆盖`；源实现 `部分实现`。这是原状态卡范围，不能从视觉改变实现结论。
- 角色/条件：当前读权限 audience 可看允许信息；完整写角色不由本设计补造。。
- 字段：charter 与冲突授权；参战主体 / 区域范围；暴露资产与时间窗口；非参与者保护；成本与结果预览来源；当前可用谈判 / 暂缓路径
- 三轴：当前完整Viewer `not_confirmed; preserve listed narrow consumers`；runtime绑定 `not_verified; no current connection or runtime execution claim`；专业动作合同 `missing_full_write`。
- 具体表示：[具体目标稿](assets/ui-ux/full-function/governance-targets/frames/B29-CC-001.svg)；`normal_frame`；字段区域按 `field-1…field-6`，当前未运行。
- 动作「宣战（不可用）」：`contract_blocked`；当前 war quote 不提交宣战；完整授权管理未核清；当前图中条件：disabled because authoritative input/permission/read result unavailable; this is separate from contract completeness；启用条件：无可启用控件；见该条非UI/边界说明。
- 状态规范：scope denied / quote stale；恢复：返回当前只读 quote；邻近、物流和关系不授目标资格；完整组合仍text_spec_only。
- [状态片段](assets/ui-ux/full-function/governance-targets/states/S07.svg)；区域：table x30 y160 w1380 h469；disabled_control x36 y660 w650 h52；recovery x36 y755；仅合同示例。
- 原剩余范围：core 宣战报价已有只读、成员与资源检查；charter、暴露资产、离线防御及非参与者保护的完整执行链未核清，active M5 reducer 报价明确拒绝。
- 精确合同缺口：现有quote不提供完整charter参战/非参与保护写合同；DeclareWar后置通道当前禁用。
- 专业条款依赖：[gameplay-war-politics-mvp-baseline.design.md](../../game/gameplay/gameplay-war-politics-mvp-baseline.design.md#des-war-politics-001)；不将内部API与真实远端authority混为一谈。

<a id="t-gov-06-cc-002"></a>
### 条款 T-GOV-06 / CC-002

- 唯一上游：[REQ-WR-CC-002](../../product/world-rules-core-gameplay/chartered-conflict-soft-seasons-and-recovery.prd.md#req-wr-cc-002)；Trace refs: O7-REQ-19ea1035-2d0a-43fc-a3a8-b0309030273c。
- 状态：源设计 `未核实`；源实现 `部分实现`。这是原状态卡范围，不能从视觉改变实现结论。
- 角色/条件：当前读权限 audience 可看允许信息；完整写角色不由本设计补造。。
- 字段：受损对象与损失范围；保留的玩家 / Agent 身份；历史 receipt 与原能力；repair / rebuild / pivot / 撤离；每路径投入、代价与限制；当前可用路径及结果
- 三轴：当前完整Viewer `not_confirmed; preserve listed narrow consumers`；runtime绑定 `not_verified; no current connection or runtime execution claim`；专业动作合同 `read_boundary_only`。
- 具体表示：[具体目标稿](assets/ui-ux/full-function/governance-targets/frames/B30-CC-002.svg)；`normal_frame`；字段区域按 `field-1…field-6`，当前未运行。
- 动作「比较恢复路径」：`target_conditionally_available`；专用完整恢复写合同未核清；当前 R-GAME-24 仅窄比较；当前图中条件：disabled because authoritative input/permission/read result unavailable; this is separate from contract completeness；启用条件：exact supported operation/schema + current actor role + scope/authority/versions + fresh prerequisites; read-only controls require current read permission and resolvable original record。
- 状态规范：damaged / unavailable / recovery pending；恢复：保留原身份；不免费瞬间复原，不将损失写成账号删除；完整组合仍text_spec_only。
- [状态片段](assets/ui-ux/full-function/governance-targets/states/S06.svg)；区域：table x30 y160 w1380 h469；disabled_control x36 y660 w650 h52；recovery x36 y755；仅合同示例。
- 原剩余范围：repair/rebuild/pivot/撤离可比较与可执行：未交付完整组合：现有失败卡只选择真实enabled动作，且清空陈旧repair/rebuild/pivot投影；没有冲突资产多路线比较；保留全部AC-4冲突损毁后资产/身份/投入/恢复比较组合验收，现有窄交付不代签。结论限当前仓库默认/内置及已审正式路径；未提供的线上自定义artifact不在本轮结论范围。
- 精确合同缺口：完整产品范围仍按 remaining_gap 保留；已读子合同可以设计条件可用，不因当前 UI 未接通判合同缺失。

<a id="t-gov-06-cc-003"></a>
### 条款 T-GOV-06 / CC-003

- 唯一上游：[REQ-WR-CC-003](../../product/world-rules-core-gameplay/chartered-conflict-soft-seasons-and-recovery.prd.md#req-wr-cc-003)；Trace refs: O7-REQ-eedaa5cf-e91e-48df-9c33-158774b40168。
- 状态：源设计 `已覆盖`；源实现 `未实现`。这是原状态卡范围，不能从视觉改变实现结论。
- 角色/条件：有权 owner 开项目；contributor 提供 source claim；受权 evaluator 判证据；项目 authority 应用 milestone；独立复核处理申诉。。
- 字段：project / containment / policy / revision；参与资格与 source claim；contribution accepted / waiting_evidence；milestone 决定与 target revision；applied receipt 与实际资源义务；退出 / 到期 / 申诉剩余
- 三轴：当前完整Viewer `not_confirmed; preserve listed narrow consumers`；runtime绑定 `not_verified; no current connection or runtime execution claim`；专业动作合同 `complete_target`。
- 具体表示：[具体目标稿](assets/ui-ux/full-function/governance-targets/frames/B31-CC-003.svg)；`normal_frame`；字段区域按 `field-1…field-6`，当前未运行。
- 动作「提交受支持贡献」：`target_conditionally_available`；资格、source、授权与 deadline 当前有效；accepted 不等于修复；当前图中条件：disabled because authoritative input/permission/read result unavailable; this is separate from contract completeness；启用条件：exact supported operation/schema + current actor role + scope/authority/versions + fresh prerequisites; read-only controls require current read permission and resolvable original record。
- 状态规范：recovery_blocked / settlement_pending；恢复：按原 intent 查询；已终止项目不隐式复活，不重复认领 source；完整组合仍text_spec_only。
- [状态片段](assets/ui-ux/full-function/governance-targets/states/S06.svg)；区域：table x30 y160 w1380 h469；disabled_control x36 y660 w650 h52；recovery x36 y755；仅合同示例。
- [状态片段](assets/ui-ux/full-function/governance-targets/states/S15.svg)；区域：inputs x36 y286 w744 h491；checks x820 y286 w744 h491；conditional_ctas y812 h54；仅合同示例。
- 原剩余范围：containment及有界项目作用域、资格、里程碑、退出/申诉：当前native及M5只生产普通Crisis active/resolved/timedout；没有该项项目生命周期与权利消费者；权威贡献receipt确认一次项目效果：ResolveCrisis将success输入转impact；没有贡献身份、资源义务、里程碑receipt或项目效果绑定，普通job去重不适用；重连重试授权/作用域变化不越界且失败有下一步：通用snapshot/replay恢复已有CrisisState，但没有待决贡献及新旧项目/授权处置可供恢复。结论限当前仓库默认/内置及已审正式路径；未提供的线上自定义artifact不在本轮结论范围。 实施待安排： [Refs #4863](https://github.com/nil-studio/oasis7/issues/4863)；本状态审查不关闭该任务。
- 精确合同缺口：完整产品范围仍按 remaining_gap 保留；已读子合同可以设计条件可用，不因当前 UI 未接通判合同缺失。
- 专业条款依赖：[bounded-recovery-projects-and-contribution-settlement.design.md](../../world-runtime/runtime/bounded-recovery-projects-and-contribution-settlement.design.md#recovery-contribution)；不将内部API与真实远端authority混为一谈。

<a id="t-gov-06-cc-004"></a>
### 条款 T-GOV-06 / CC-004

- 唯一上游：[REQ-WR-CC-004](../../product/world-rules-core-gameplay/chartered-conflict-soft-seasons-and-recovery.prd.md#req-wr-cc-004)；Trace refs: O7-REQ-a2b47612-d2c2-4331-811e-58a61366b64d。
- 状态：源设计 `未核实`；源实现 `未实现`。这是原状态卡范围，不能从视觉改变实现结论。
- 角色/条件：当前读权限 audience 可看允许信息；完整写角色不由本设计补造。。
- 字段：contested occupation 的授权范围；持续占有与到期冻结；已成立权利及其 receipt；物理提取与在途物品；受保护目的地入库与安全交割；仍在风险中的权利 / 物品
- 三轴：当前完整Viewer `not_confirmed; preserve listed narrow consumers`；runtime绑定 `not_verified; no current connection or runtime execution claim`；专业动作合同 `missing_full_write`。
- 具体表示：[具体目标稿](assets/ui-ux/full-function/governance-targets/frames/B32-CC-004.svg)；`normal_frame`；字段区域按 `field-1…field-6`，当前未运行。
- 动作「查看分别结算结果」：`contract_blocked`；完整占领及提取写合同未核清；进入或触碰不等于成立；当前图中条件：disabled because authoritative input/permission/read result unavailable; this is separate from contract completeness；启用条件：无可启用控件；见该条非UI/边界说明。
- 状态规范：contested / in transit / safe delivery unverified；恢复：分别查询权利与物流；不以发现或短暂占有替代安全交割；完整组合仍text_spec_only。
- [状态片段](assets/ui-ux/full-function/governance-targets/states/S07.svg)；区域：table x30 y160 w1380 h469；disabled_control x36 y660 w650 h52；recovery x36 y755；仅合同示例。
- 原剩余范围：contested occupation、持续hold与expiry独立判断：native/M5 WarConclude的authority为成员/声誉/疲劳与到期比分，不追踪tenure/occupation持续条件；物理战利品提取与受保护目的地交割分离：普通TransferMaterial/transit有物理运输，但无war entitlement/loot/custody及保护目的地绑定；war资源即时加减不是safe extraction；结果可读已成立权利/已交割/仍有风险物流：WarState只有胜负及participant_outcomes，缺前述独立权利与提取状态，读面不能补成。结论限当前仓库默认/内置及已审正式路径；未提供的线上自定义artifact不在本轮结论范围。
- 精确合同缺口：持续占领/冻结/物理提取/受保护入库分别的producer、写接口、角色、计时判据与receipt关联未核清。

<a id="t-gov-06-cc-005"></a>
### 条款 T-GOV-06 / CC-005

- 唯一上游：[REQ-WR-CC-005](../../product/world-rules-core-gameplay/chartered-conflict-soft-seasons-and-recovery.prd.md#req-wr-cc-005)；Trace refs: O7-REQ-ce9fbace-7fc6-4973-8bcf-8b58e11b3339。
- 状态：源设计 `未核实`；源实现 `未实现`。这是原状态卡范围，不能从视觉改变实现结论。
- 角色/条件：当前读权限 audience 可看允许信息；完整写角色不由本设计补造。。
- 字段：刷新竞争机会与区域资格；生效条件、对象及边界；同一世界时间线；玩家 / Agent 身份与核心能力；可迁移价值 / 历史 receipt / 因果；恢复与申诉路径
- 三轴：当前完整Viewer `not_confirmed; preserve listed narrow consumers`；runtime绑定 `not_verified; no current connection or runtime execution claim`；专业动作合同 `missing_full_write`。
- 具体表示：[具体目标稿](assets/ui-ux/full-function/governance-targets/frames/B33-CC-005.svg)；`normal_frame`；字段区域按 `field-1…field-6`，当前未运行。
- 动作「查看赛季转换」：`contract_blocked`；无完整刷新合同，不提供重置世界按钮；当前图中条件：disabled because authoritative input/permission/read result unavailable; this is separate from contract completeness；启用条件：无可启用控件；见该条非UI/边界说明。
- 状态规范：transition pending / outcome unknown；恢复：保留已确认历史；读取原边界，不清零全局资产；完整组合仍text_spec_only。
- [状态片段](assets/ui-ux/full-function/governance-targets/states/S08.svg)；区域：table x30 y160 w1380 h469；disabled_control x36 y660 w650 h52；recovery x36 y755；仅合同示例。
- 原剩余范围：竞争窗口/公共项目/排行榜/部分区域权利刷新对象和边界：当前typed事件与M5 tick producer只实现战争/危机/治理/meta变化，未建立软赛季刷新对象和boundary；刷新时保持时间线、身份、核心能力、价值、历史因果：已有WorldState持久化保留普通历史，但无真实刷新调用，不能将无刷新等同赛季连续性通过；刷新条件与恢复/申诉可读可审计：无刷新权威对象供消费者关联；war到期/危机周期并非软赛季。结论限当前仓库默认/内置及已审正式路径；未提供的线上自定义artifact不在本轮结论范围。
- 精确合同缺口：软赛季刷新对象/生效producer、授权角色、执行接口、幂等与恢复/申诉合同未核清；不代选周期。

<a id="t-gov-07-fi-001"></a>
### 条款 T-GOV-07 / FI-001

- 唯一上游：[REQ-WR-FI-001](../../product/world-rules-core-gameplay/frontier-expansion-and-world-information-boundaries.prd.md#req-wr-fi-001)；Trace refs: O7-REQ-3013b8cf-e3db-4506-a30a-db34e25ed636。
- 状态：源设计 `未核实`；源实现 `未核实`。这是原状态卡范围，不能从视觉改变实现结论。
- 角色/条件：当前读权限 audience 可看允许信息；完整写角色不由本设计补造。。
- 字段：相邻区域身份与空间来源；探索前置；物流连通前置；charter 组合准入；同世界时间线 / 经济 / 历史；拒绝原因与可用恢复
- 三轴：当前完整Viewer `not_confirmed; preserve listed narrow consumers`；runtime绑定 `not_verified; no current connection or runtime execution claim`；专业动作合同 `missing_full_write`。
- 具体表示：[具体目标稿](assets/ui-ux/full-function/governance-targets/frames/B34-FI-001.svg)；`normal_frame`；字段区域按 `field-1…field-6`，当前未运行。
- 动作「进入 frontier（不可用）」：`contract_blocked`；当前地图/邻接/侦察实现时点未定；不捏造可进入区域；当前图中条件：disabled because authoritative input/permission/read result unavailable; this is separate from contract completeness；启用条件：无可启用控件；见该条非UI/边界说明。
- 状态规范：adjacency unknown / charter unavailable；恢复：读取组合缺项；不跳跃建私有副本或独立结果；完整组合仍text_spec_only。
- [状态片段](assets/ui-ux/full-function/governance-targets/states/S12.svg)；区域：table x30 y160 w1380 h469；disabled_control x36 y660 w650 h52；recovery x36 y755；仅合同示例。
- 原剩余范围：具体未知为：frontier身份/邻接producer、探索/物流/charter组合准入与拒绝、同世界时间线/经济/历史接入，以及玩家/Agent可读原因与恢复入口。产品§5.3与Non-Goals（[frontier-expansion-and-world-information-boundaries.prd.md:210–221](https://github.com/nil-studio/oasis7/blob/97ceafff06d0a76b79eef8e34fc4c49f470ceb05/doc/product/world-rules-core-gameplay/frontier-expansion-and-world-information-boundaries.prd.md#L210-L221)）保留何时进入可验证地图/邻接/物流/侦察实现的未定决策，本轮不代选policy、不宣称代码必不存在或当前可进入frontier。原FI-1及AC-WR-FI-001不变。
- 精确合同缺口：frontier实体、邻接/物流/charter组合producer及正式进入写接口与恢复合同未核清。

<a id="t-gov-07-fi-002"></a>
### 条款 T-GOV-07 / FI-002

- 唯一上游：[REQ-WR-FI-002](../../product/world-rules-core-gameplay/frontier-expansion-and-world-information-boundaries.prd.md#req-wr-fi-002)；Trace refs: O7-REQ-066843dd-9e40-4148-8933-d369d16b55a2。
- 状态：源设计 `未核实`；源实现 `未核实`。这是原状态卡范围，不能从视觉改变实现结论。
- 角色/条件：当前读权限 audience 可看允许信息；完整写角色不由本设计补造。。
- 字段：地点 / 用途 / 当前持有人；未完成开发里程碑；原期限与剩余范围；受让人当前资格；priority 与独立 tenure 状态；到期 / 撤销 / 争议与 receipt
- 三轴：当前完整Viewer `not_confirmed; preserve listed narrow consumers`；runtime绑定 `not_verified; no current connection or runtime execution claim`；专业动作合同 `missing_full_write`。
- 具体表示：[具体目标稿](assets/ui-ux/full-function/governance-targets/frames/B35-FI-002.svg)；`normal_frame`；字段区域按 `field-1…field-6`，当前未运行。
- 动作「申请机会转让」：`contract_blocked`；完整转让合同未核清；不延长原期限或转移身份声誉；当前图中条件：disabled because authoritative input/permission/read result unavailable; this is separate from contract completeness；启用条件：无可启用控件；见该条非UI/边界说明。
- 状态规范：pending / transferred / consumed / expired；恢复：原 receipt 查询；到期不追溯没收独立 tenure，不冻结无关安全通行；完整组合仍text_spec_only。
- [状态片段](assets/ui-ux/full-function/governance-targets/states/S12.svg)；区域：table x30 y160 w1380 h469；disabled_control x36 y660 w650 h52；recovery x36 y755；仅合同示例。
- 原剩余范围：未核清 priority 原期限与剩余范围/里程碑、当前受让资格与待决变动原子处理、并发/重试/关联倒手/历史receipt重放、争议安全披露不冻结，以及独立tenure不被追溯没收。还需原§2.3 charter状态转换下待决tenure请求不能跨授权确认/迁移、同范围单一处置及下一步消费者。保留[frontier-expansion-and-world-information-boundaries.prd.md:210–221](https://github.com/nil-studio/oasis7/blob/97ceafff06d0a76b79eef8e34fc4c49f470ceb05/doc/product/world-rules-core-gameplay/frontier-expansion-and-world-information-boundaries.prd.md#L210-L221)的长期/当前未承诺边界；无专属交付闭包证据，保持具体未知而非未实现，不调整期限或资格policy。
- 精确合同缺口：priority转让、当前资格、消费/期限、并发去重及与tenure处置关联的具体写合同未核清。

<a id="t-gov-07-fi-003"></a>
### 条款 T-GOV-07 / FI-003

- 唯一上游：[REQ-WR-FI-003](../../product/world-rules-core-gameplay/frontier-expansion-and-world-information-boundaries.prd.md#req-wr-fi-003)；Trace refs: O7-REQ-23640893-de22-4940-9b25-ed08c9ab0f7e。
- 状态：源设计 `未核实`；源实现 `未核实`。这是原状态卡范围，不能从视觉改变实现结论。
- 角色/条件：当前读权限 audience 可看允许信息；完整写角色不由本设计补造。。
- 字段：公共规则 / 已结算事实；最低安全与通行披露；有界私用 → 公共 baseline；live inventory / routes 的取得依据；每项来源 / 范围 / freshness；不确定性与授权当前状态
- 三轴：当前完整Viewer `not_confirmed; preserve listed narrow consumers`；runtime绑定 `not_verified; no current connection or runtime execution claim`；专业动作合同 `missing_full_write`。
- 具体表示：[具体目标稿](assets/ui-ux/full-function/governance-targets/frames/B36-FI-003.svg)；`normal_frame`；字段区域按 `field-1…field-6`，当前未运行。
- 动作「刷新或选择替代信息」：`contract_blocked`；无授权不展示实时私有值；信息不足不假装完整真值；当前图中条件：disabled because authoritative input/permission/read result unavailable; this is separate from contract completeness；启用条件：无可启用控件；见该条非UI/边界说明。
- 状态规范：private / stale / conflicting / unknown；恢复：刷新、替代、等待或停止按实际能力；不给全图实时情报；完整组合仍text_spec_only。
- [状态片段](assets/ui-ux/full-function/governance-targets/states/S09.svg)；区域：table x30 y160 w1380 h469；disabled_control x36 y660 w650 h52；recovery x36 y755；仅合同示例。
- 原剩余范围：具体未知：探索数据有界私用→最低安全/通行披露→最终公共baseline，公共事实可审计与实时库存/路线等侦察、关系、购买、合同或授权分类；每项来源、范围、freshness与不确定性，以及授权失效、冲突和过期时刷新/替代/等待/停止的消费链。保留[frontier-expansion-and-world-information-boundaries.prd.md:210–221](https://github.com/nil-studio/oasis7/blob/97ceafff06d0a76b79eef8e34fc4c49f470ceb05/doc/product/world-rules-core-gameplay/frontier-expansion-and-world-information-boundaries.prd.md#L210-L221)的未定侦察实现时点及不承诺当前实时情报/公共baseline边界，不替用户选择保密期或披露policy。
- 精确合同缺口：完整信息分类producer、public baseline、实时授权/撤销/freshness DTO未核清；不编造实时值。

<a id="t-gov-07-sc8-dataread"></a>
### 条款 T-GOV-07 / SC8-dataread

- 唯一上游：[SC8-dataread](../../product/world-rules-core-gameplay/prd.md#root-sc8-dataread)；Trace refs: O7-REQ-6bd89114-455c-490b-9b21-425fa90df866。
- 状态：源设计 `未核实`；源实现 `部分实现`。这是原状态卡范围，不能从视觉改变实现结论。
- 角色/条件：当前读权限 audience 可看允许信息；完整写角色不由本设计补造。。
- 字段：owner 与 recipient；purpose 与 scope；当前 grant / 有效性；来源与预览身份；将使用或转移的内容边界；授权缺项和可用替代
- 三轴：当前完整Viewer `not_confirmed; preserve listed narrow consumers`；runtime绑定 `not_verified; no current connection or runtime execution claim`；专业动作合同 `missing_full_write`。
- 具体表示：[具体目标稿](assets/ui-ux/full-function/governance-targets/frames/B37-SC8-dataread.svg)；`normal_frame`；字段区域按 `field-1…field-6`，当前未运行。
- 动作「请求数据授权（不可用）」：`contract_blocked`；完整跨主体 DTO/producer 未核清；self_collection 不代签；当前图中条件：disabled because authoritative input/permission/read result unavailable; this is separate from contract completeness；启用条件：无可启用控件；见该条非UI/边界说明。
- 状态规范：grant unknown / purpose mismatch；恢复：查看当前来源；不能以历史 receipt 替代当前授权；完整组合仍text_spec_only。
- [状态片段](assets/ui-ux/full-function/governance-targets/states/S09.svg)；区域：table x30 y160 w1380 h469；disabled_control x36 y660 w650 h52；recovery x36 y755；仅合同示例。
- 原剩余范围：跨主体owner/recipient/purpose/scope、当前grant状态可读及全部入口预览语义未闭合；self_collection不能代签跨主体访问授权。
- 精确合同缺口：跨主体owner/recipient/purpose/scope与grant的完整正式预览DTO和producer未核清。

<a id="t-gov-07-sc8-datacommit"></a>
### 条款 T-GOV-07 / SC8-datacommit

- 唯一上游：[SC8-datacommit](../../product/world-rules-core-gameplay/prd.md#root-sc8-datacommit)；Trace refs: O7-REQ-ebcc34f8-5db5-46f7-85fb-30b44b8b797f。
- 状态：源设计 `未核实`；源实现 `部分实现`。这是原状态卡范围，不能从视觉改变实现结论。
- 角色/条件：当前读权限 audience 可看允许信息；完整写角色不由本设计补造。。
- 字段：原提交 / grant / scope；提交至结算间有效性；拒绝或待决原因；实际使用 / 转移；sink / 收益 / 隐性义务检查；唯一合法结算 receipt
- 三轴：当前完整Viewer `not_confirmed; preserve listed narrow consumers`；runtime绑定 `not_verified; no current connection or runtime execution claim`；专业动作合同 `missing_full_write`。
- 具体表示：[具体目标稿](assets/ui-ux/full-function/governance-targets/frames/B38-SC8-datacommit.svg)；`normal_frame`；字段区域按 `field-1…field-6`，当前未运行。
- 动作「核对原 Data 请求」：`contract_blocked`；失效或无法证明有效须拒绝或待决；不得先消费后补授权；当前图中条件：disabled because authoritative input/permission/read result unavailable; this is separate from contract completeness；启用条件：无可启用控件；见该条非UI/边界说明。
- 状态规范：revoked / pending / outcome unknown；恢复：按原请求核对实际 receipt；未知不能写成无效果；完整组合仍text_spec_only。
- [状态片段](assets/ui-ux/full-function/governance-targets/states/S09.svg)；区域：table x30 y160 w1380 h469；disabled_control x36 y660 w650 h52；recovery x36 y755；仅合同示例。
- 原剩余范围：提交至结算间授权失效或不能证明有效时原子拒绝或可读待决，无 sink、收益或隐性义务；合法结算只产生一次 receipt 支持的使用/转移的精确专业分配/验证关系、当前实现和实际验收仍由 §5.1 对应 owner 核定；不得以根 SC 的局部或历史结果代签。
- 精确合同缺口：完整跨入口Data授权失效→拒绝/待决→唯一结算写接口、scope/purpose与receipt闭包未核清。

<a id="t-gov-07-sc8-datareplay"></a>
### 条款 T-GOV-07 / SC8-datareplay

- 唯一上游：[SC8-datareplay](../../product/world-rules-core-gameplay/prd.md#root-sc8-datareplay)；Trace refs: O7-REQ-c549ec3e-5dc6-4e27-9285-4e326b0fc5d3。
- 状态：源设计 `未核实`；源实现 `部分实现`。这是原状态卡范围，不能从视觉改变实现结论。
- 角色/条件：当前读权限 audience 可看允许信息；完整写角色不由本设计补造。。
- 字段：历史合法使用 / 转移 receipt；原 owner / recipient / purpose；当前 grant 撤销状态；原请求与重复结果；保留 provenance；重新授权 / 替代的实际入口
- 三轴：当前完整Viewer `not_confirmed; preserve listed narrow consumers`；runtime绑定 `not_verified; no current connection or runtime execution claim`；专业动作合同 `missing_full_write`。
- 具体表示：[具体目标稿](assets/ui-ux/full-function/governance-targets/frames/B39-SC8-datareplay.svg)；`normal_frame`；字段区域按 `field-1…field-6`，当前未运行。
- 动作「查看原 provenance」：`contract_blocked`；历史 receipt 不复活 grant；未核清重新授权写合同；当前图中条件：disabled because authoritative input/permission/read result unavailable; this is separate from contract completeness；启用条件：无可启用控件；见该条非UI/边界说明。
- 状态规范：historical confirmed / current grant revoked；恢复：保留历史而停止新消费；不把采集 nonce 当所有访问重放证明；完整组合仍text_spec_only。
- [状态片段](assets/ui-ux/full-function/governance-targets/states/S09.svg)；区域：table x30 y160 w1380 h469；disabled_control x36 y660 w650 h52；recovery x36 y755；仅合同示例。
- 原剩余范围：grant撤销后历史receipt不复活权限、合法provenance保留和可读重新授权/替代仍需专门链；不把self_collection nonce当所有Data access重放证明。
- 精确合同缺口：完整Data provenance/重授权/替代入口与历史receipt不复权的跨消费者协议未核清。

<a id="t-gov-08-gcb-005"></a>
### 条款 T-GOV-08 / GCB-005

- 唯一上游：[REQ-WR-GCB-005](../../product/world-rules-core-gameplay/governed-common-decisions-and-constitutional-boundaries.prd.md#req-wr-gcb-005)；Trace refs: O7-REQ-4b64b7c5-9819-4932-a1ad-3cdb09fa82e5。
- 状态：源设计 `未核实`；源实现 `部分实现`。这是原状态卡范围，不能从视觉改变实现结论。
- 角色/条件：当前读权限 audience 可看允许信息；完整写角色不由本设计补造。。
- 字段：外部 OC 转让事实；事项范围内 lock / snapshot；退出或解除绑定的当前结果；有效窗口内原计票历史；OC → LetAI Run quota 单向服务；token_key 与服务消费（非治理）
- 三轴：当前完整Viewer `not_confirmed; preserve listed narrow consumers`；runtime绑定 `not_verified; no current connection or runtime execution claim`；专业动作合同 `read_boundary_only`。
- 具体表示：[具体目标稿](assets/ui-ux/full-function/governance-targets/frames/B05-GCB-005.svg)；`normal_frame`；字段区域按 `field-1…field-6`，当前未运行。
- 动作「查看来源记录」：`target_conditionally_available`；Viewer 不提供链上转账或世界资格兑换；当前图中条件：disabled because authoritative input/permission/read result unavailable; this is separate from contract completeness；启用条件：exact supported operation/schema + current actor role + scope/authority/versions + fresh prerequisites; read-only controls require current read permission and resolvable original record。
- 状态规范：链上 confirmed ≠ 世界 eligible；恢复：分别核对两侧原结果；一侧失败不覆盖另一侧历史；完整组合仍text_spec_only。
- 原剩余范围：外部OC锁定/退出/解绑→具体治理主体及事项snapshot绑定、跨账户/实际控制人并发转移隔离仍未实现于上述入口；operator-managed quota bridge、token_key及服务消费的全部正式消费者和三种状态并列读面仍未核清，不把局部隔离扩成整条已验证。
- 精确合同缺口：完整产品范围仍按 remaining_gap 保留；已读子合同可以设计条件可用，不因当前 UI 未接通判合同缺失。

<a id="t-gov-08-gr-011"></a>
### 条款 T-GOV-08 / GR-011

- 唯一上游：[REQ-WR-GR-011](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-011)；Trace refs: O7-REQ-d27a209c-a703-4443-92d9-acadb9820ed2。
- 状态：源设计 `未核实`；源实现 `部分实现`。这是原状态卡范围，不能从视觉改变实现结论。
- 角色/条件：当前读权限 audience 可看允许信息；完整写角色不由本设计补造。。
- 字段：链上转让规则与链上成本；世界内资源 / 信用 / 库存 / 合同；地方 charter 的适用双资格；工业准入 / 市场 / 紧急保供各自门禁；治理 / tenure / levy 的独立授权；两侧独立 receipt 与失败状态
- 三轴：当前完整Viewer `not_confirmed; preserve listed narrow consumers`；runtime绑定 `not_verified; no current connection or runtime execution claim`；专业动作合同 `read_boundary_only`。
- 具体表示：[具体目标稿](assets/ui-ux/full-function/governance-targets/frames/B40-GR-011.svg)；`normal_frame`；字段区域按 `field-1…field-6`，当前未运行。
- 动作「查看独立来源」：`target_conditionally_available`；无 Viewer 钱包或赎回表单；不把世界资格施加为链上转让限制；当前图中条件：disabled because authoritative input/permission/read result unavailable; this is separate from contract completeness；启用条件：exact supported operation/schema + current actor role + scope/authority/versions + fresh prerequisites; read-only controls require current read permission and resolvable original record。
- 状态规范：chain confirmed ≠ world qualification；恢复：分别查询原结果；无隐式兑换、赎回或资格授予；完整组合仍text_spec_only。
- [状态片段](assets/ui-ux/full-function/governance-targets/states/S04.svg)；区域：table x30 y160 w1380 h469；disabled_control x36 y660 w650 h52；recovery x36 y755；仅合同示例。
- 原剩余范围：仅此独立转账子范围，不等于全部双向隔离。治理输入/区域贡献、资源/信用/库存/合同/工业作者与准入/levy/紧急保供的所有消费者，世界资产自动兑换/赎回负例、完整fee execution及失败一侧不抹另一侧的组合运行仍未核清。 产品语义已冻结，专业技术承接未核到不等于待定产品政策；未检视路径不判未实现。
- 精确合同缺口：完整产品范围仍按 remaining_gap 保留；已读子合同可以设计条件可用，不因当前 UI 未接通判合同缺失。


### 6.1 规划组导航

<a id="t-gov-01"></a>
- T-GOV-01：[该组首条具体义务](#t-gov-01-gcb-001)。
<a id="t-gov-02"></a>
- T-GOV-02：[该组首条具体义务](#t-gov-02-gr-001)。
<a id="t-gov-03"></a>
- T-GOV-03：[该组首条具体义务](#t-gov-03-gr-002)。
<a id="t-gov-04"></a>
- T-GOV-04：[该组首条具体义务](#t-gov-04-cr-001)。
<a id="t-gov-05"></a>
- T-GOV-05：[该组首条具体义务](#t-gov-05-oc-001)。
<a id="t-gov-06"></a>
- T-GOV-06：[该组首条具体义务](#t-gov-06-cc-001)。
<a id="t-gov-07"></a>
- T-GOV-07：[该组首条具体义务](#t-gov-07-fi-001)。
<a id="t-gov-08"></a>
- T-GOV-08：[该组首条具体义务](#t-gov-08-gcb-005)。


### 6.2 条件写面：用户选择与系统只读绑定

这些片段表示完整或局部专业合同的条件交互设计，不表示当前Viewer/API已经接通。只读绑定不得伪装编辑控件；未取得封闭候选schema时不造自由文本。

#### states/S13.svg

[具体状态片段](assets/ui-ux/full-function/governance-targets/states/S13.svg)

| 字段 | 控件 / 可编辑性 | 当前示例值 | 校验与范围 | 精确区域 |
|---|---|---|---|---|
| 事项类别 · 用户选择 | select / True | policy_runtime | 已选合法普通类别；仅声明意图，不授予政策执行权。 | `control-category` |
| 对象 / 范围 · 用户从权威候选选择 | select / True | 请选择目标对象 | 必填：缺少权威对象。禁止用自由文本扩大权限。 | `control-target_scope` |
| 请求效果集合 · 用户多选 | multiselect_unknown / False | 候选类型待当前 schema | contract_unknown：封闭类型清单未取得，不提供自由文本 / other。 | `control-effects` |
| 授权来源引用 · 用户选择引用 | select / True | 请选择可验证 AuthorityRef | 必填：只选择已有来源；不能自行填写 issuer / grant 权限。 | `control-authority_ref` |
| world / matter / revision / payload | readonly / False | 系统绑定 / 不可编辑 | 规范化摘要由声明内容生成；修改效果形成新修订，旧批准不迁移。 | `control-identity` |

- 仍缺：actual candidate catalogs and concrete resource quantity/unit contract not invented；运行未接通/未执行。

#### states/S14.svg

[具体状态片段](assets/ui-ux/full-function/governance-targets/states/S14.svg)

| 字段 | 控件 / 可编辑性 | 当前示例值 | 校验与范围 | 精确区域 |
|---|---|---|---|---|
| case / manifest / expected versions | readonly / False | 系统绑定 / 不可编辑 | 绑定已打开原 case、manifest root 与当前版本；不能通过新 case 重复处分。 | `control-case` |
| 完整托管项 · 用户单选 | select / True | 完整托管项 A（设计示例） | 选中值来自 canonical DepositOrganizationEstate receipt；未知权属不列入。 | `control-asset` |
| 返还对象 / 数量 | readonly / False | 系统绑定：原 contributor / 完整项 | 不可编辑受益人、不可拆分本图首版完整返还项；原记录未知即阻断。 | `control-recipient` |
| appointment / successor grant / scope | readonly / False | 系统绑定 / 不可编辑 | 当前 authority、控制绑定、scope 与期限由权威读取；用户不得手填授权。 | `control-grant` |
| request identity / plan hash / effect key | readonly / False | 系统绑定 / 不可编辑 | PrepareDisposition 生成计划；确认绑定原计划与版本，unknown 查原请求。 | `control-request` |

- 仍缺：actual candidate catalogs and concrete resource quantity/unit contract not invented；运行未接通/未执行。

#### states/S15.svg

[具体状态片段](assets/ui-ux/full-function/governance-targets/states/S15.svg)

| 字段 | 控件 / 可编辑性 | 当前示例值 | 校验与范围 | 精确区域 |
|---|---|---|---|---|
| project · 用户选择 / revision 系统绑定 | select / True | 恢复项目 A（设计示例） | 从当前可读项目选取；revision、scope、policy、deadline 系统绑定，不可编辑。 | `control-project` |
| source receipt / lot · 用户选择 | multiselect / True | 来源 lot A（设计示例） | 只能选可验证来源；claim 索引重验不可重复认领。具体数量/单位控件待领域合同。 | `control-source` |
| actor / 代表授权 / 参与资格 | readonly / False | 系统绑定 / 不可编辑 | Participation 与 Eligibility 分开读取；加入项目不能替代资格。 | `control-eligibility` |
| hold / 成本 / 期限 / 退出责任 | readonly / False | 权威承诺预览 · 待取得 | 不可自由填写 payer、hold、成本或期限。完整预览未取得不能提供贡献。 | `control-commitment` |
| intent / payload / source claim refs | readonly / False | 系统绑定 / 不可编辑 | 保留原贡献身份与内容摘要；unknown 先查原 intent，不换身份再次贡献。 | `control-intent` |

- 仍缺：actual candidate catalogs and concrete resource quantity/unit contract not invented；运行未接通/未执行。

## 7. 状态、事务与持久化

loading、empty、unavailable、permission denied、pending、unknown与明确失败不得混成一个状态。模型/查询成功不等于世界生效。原结果按原主体和请求身份回读，不能因刷新/切换/重连重放在途意图。本设计不增加持久化协议；动态DTO/服务capability未提供的字段或枚举保留未知。

## 8. 安全、权限与隐私

以当前授权为准；权限丢失先停止可执行入口并清理旧主体私有上下文，不把旧图形/反馈留给新账号。凭据只以空字段/掩码示意，图稿不包含真实key、OTP、Cookie或完整私有响应。通用raw JSON不作为获得更多数据的许可。

## 9. 质量、响应式与可访问性

遵循既有brand与visual规范。窄屏/低高度、触控、键盘、焦点回返、读屏语义、IME/reduced-motion须按实际候选验证；静态SVG与文字不证明这些运行行为。错误近字段，busy和禁用原因可读，重要结果不只靠颜色。

## 10. 兼容、迁移与回滚

沿用原14张概览与兼容入口，不移除旧authority锚点。新稿先按具体source/条件接入，不将target-only按钮启用为当前。文档/资产回退只恢复对应呈现规范，不撤销任何外部世界效果；本设计不改变运行合同。

## 11. 验证设计与可追溯性

固定source与静态SVG只支持结构和视觉审读；真实交互、权限执行、跨入口连续性与世界结果仍需按下表验证。测试/手册引用是验证方法，不代表相应场景已经通过。

### 11.1 验证映射表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source 或 ID、scenario/layer、candidate/environment 要求或选择规则 | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [REQ-WR-GCB-001](../../product/world-rules-core-gameplay/governed-common-decisions-and-constitutional-boundaries.prd.md#req-wr-gcb-001)；Trace refs: O7-REQ-f282f0ef-655c-4e41-a1b6-fa37482d663b | [GCB-001](viewer-governance-targets-uiux.design.md#t-gov-01-gcb-001) | 条件目标读面：类别 policy / treasury / charter, 目标对象与规范化效果集合, 事项身份 / 修订 / payload digest, 授权者、grant 与来源 receipt；玩家或受权 Agent 声明 intent；分类器确认规范化效果、保护索引和 authority 来源。 | [Web UI闭环场景](../../../testing-manual.md#testing-web-ui-closed-loop-smoke)；逐本条义务审读输入/状态与恢复边界；运行验证需精确候选、获准主体及真实已公布capability，当前未执行 | 本条SVG区域与静态源/契约核对；运行evidence仍缺 | 不证明未实现写面、所有状态组合、native、权限执行与结算原子性 |
| [REQ-WR-GCB-002](../../product/world-rules-core-gameplay/governed-common-decisions-and-constitutional-boundaries.prd.md#req-wr-gcb-002)；Trace refs: O7-REQ-79b6687e-3b8c-42a3-a266-df4d191fec06 | [GCB-002](viewer-governance-targets-uiux.design.md#t-gov-01-gcb-002) | 条件目标读面：审议与影响说明, 延迟 / 复核的权威时间, 超多数与独立资格快照, 受影响主体独立确认；各独立 producer 分别供审议、延迟、超多数、独立确认、审计、申诉和当前授权；执行者须具本案当前执行权限。 | [Web UI闭环场景](../../../testing-manual.md#testing-web-ui-closed-loop-smoke)；逐本条义务审读输入/状态与恢复边界；运行验证需精确候选、获准主体及真实已公布capability，当前未执行 | 本条SVG区域与静态源/契约核对；运行evidence仍缺 | 不证明未实现写面、所有状态组合、native、权限执行与结算原子性 |
| [REQ-WR-GCB-003](../../product/world-rules-core-gameplay/governed-common-decisions-and-constitutional-boundaries.prd.md#req-wr-gcb-003)；Trace refs: O7-REQ-4f6597ff-8e34-4d12-bb60-060df0e09595 | [GCB-003](viewer-governance-targets-uiux.design.md#t-gov-01-gcb-003) | 条件目标读面：原 matter / approved revision, 通过时目标、范围与允许效果, 当前授权、保护与前置检查, 唯一 ExecutionSlot / payload；当前有 ExecuteGovernedMatter 权限的主体；执行器固定 ExecutionSlot，非任意 operator 或普通投票者。 | [Web UI闭环场景](../../../testing-manual.md#testing-web-ui-closed-loop-smoke)；逐本条义务审读输入/状态与恢复边界；运行验证需精确候选、获准主体及真实已公布capability，当前未执行 | 本条SVG区域与静态源/契约核对；运行evidence仍缺 | 不证明未实现写面、所有状态组合、native、权限执行与结算原子性 |
| [REQ-WR-GCB-004](../../product/world-rules-core-gameplay/governed-common-decisions-and-constitutional-boundaries.prd.md#req-wr-gcb-004)；Trace refs: O7-REQ-b584b2ab-bc38-43a1-a8fd-1b89dc3d20cd | [GCB-004](viewer-governance-targets-uiux.design.md#t-gov-01-gcb-004) | 条件目标读面：经验证游戏账户, 已批准身份 / signer / wallet 绑定摘要, 锁定 OC / 有界可撤销委托, 控制人聚合资格结果（最小披露）；当前读权限 audience 可看允许信息；完整写角色不由本设计补造。 | [Web UI闭环场景](../../../testing-manual.md#testing-web-ui-closed-loop-smoke)；逐本条义务审读输入/状态与恢复边界；运行验证需精确候选、获准主体及真实已公布capability，当前未执行 | 本条SVG区域与静态源/契约核对；运行evidence仍缺 | 不证明未实现写面、所有状态组合、native、权限执行与结算原子性 |
| [REQ-WR-GCB-005](../../product/world-rules-core-gameplay/governed-common-decisions-and-constitutional-boundaries.prd.md#req-wr-gcb-005)；Trace refs: O7-REQ-4b64b7c5-9819-4932-a1ad-3cdb09fa82e5 | [GCB-005](viewer-governance-targets-uiux.design.md#t-gov-01-gcb-005) | 条件目标读面：外部 OC 转让事实, 事项范围内 lock / snapshot, 退出或解除绑定的当前结果, 有效窗口内原计票历史；当前读权限 audience 可看允许信息；完整写角色不由本设计补造。 | [Web UI闭环场景](../../../testing-manual.md#testing-web-ui-closed-loop-smoke)；逐本条义务审读输入/状态与恢复边界；运行验证需精确候选、获准主体及真实已公布capability，当前未执行 | 本条SVG区域与静态源/契约核对；运行evidence仍缺 | 不证明未实现写面、所有状态组合、native、权限执行与结算原子性 |
| [REQ-WR-GCB-006](../../product/world-rules-core-gameplay/governed-common-decisions-and-constitutional-boundaries.prd.md#req-wr-gcb-006)；Trace refs: O7-REQ-c3d848a8-0668-408a-99ca-ef08a7c64be8 | [GCB-006](viewer-governance-targets-uiux.design.md#t-gov-01-gcb-006) | 条件目标读面：保护动作与受影响对象, 授权主体 / 来源 / 当前范围, 理由与可公开证据, 生效期限与失效边界；当前读权限 audience 可看允许信息；完整写角色不由本设计补造。 | [Web UI闭环场景](../../../testing-manual.md#testing-web-ui-closed-loop-smoke)；逐本条义务审读输入/状态与恢复边界；运行验证需精确候选、获准主体及真实已公布capability，当前未执行 | 本条SVG区域与静态源/契约核对；运行evidence仍缺 | 不证明未实现写面、所有状态组合、native、权限执行与结算原子性 |
| [REQ-WR-GR-001](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-001)；Trace refs: O7-REQ-d34e2dec-9ad4-44a8-9ba7-7931956e39b8 | [GR-001](viewer-governance-targets-uiux.design.md#t-gov-02-gr-001) | 条件目标读面：区域压力与设施对象, 报价成本 / 取舍 / 快照, 提交与权威设施结果, 有限库存 / 服务消耗；设施动作的当前 capability actor；限 micro_depot 合同，不推广全部区域设施。 | [Web UI闭环场景](../../../testing-manual.md#testing-web-ui-closed-loop-smoke)；逐本条义务审读输入/状态与恢复边界；运行验证需精确候选、获准主体及真实已公布capability，当前未执行 | 本条SVG区域与静态源/契约核对；运行evidence仍缺 | 不证明未实现写面、所有状态组合、native、权限执行与结算原子性 |
| [REQ-WR-GR-004](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-004)；Trace refs: O7-REQ-7de05591-35c1-4d8a-94b2-f64fce61976e | [GR-004](viewer-governance-targets-uiux.design.md#t-gov-02-gr-004) | 条件目标读面：proposal / revision / operation, 空间锚定 scope 与前身 charter, 设施 / 物流 / 服务能力证据, 需求交付 / Agent / 资源 / bond；提案者提交证据；local/neighbor/global 各自当前 reviewer；激活器重验 required_review_set。 | [Web UI闭环场景](../../../testing-manual.md#testing-web-ui-closed-loop-smoke)；逐本条义务审读输入/状态与恢复边界；运行验证需精确候选、获准主体及真实已公布capability，当前未执行 | 本条SVG区域与静态源/契约核对；运行evidence仍缺 | 不证明未实现写面、所有状态组合、native、权限执行与结算原子性 |
| [REQ-WR-GR-005](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-005)；Trace refs: O7-REQ-ee810b69-8c6a-4aca-86e8-ac387c8f8fcb | [GR-005](viewer-governance-targets-uiux.design.md#t-gov-02-gr-005) | 条件目标读面：charter / authority revision, 事项与地方规则适用 scope, OC / 委托 / 控制人资格来源, 持续本地贡献来源与窗口；LocalEligibility producer 供双来源结果；本地决策主体必须满足该 matter/scope 的现行政策。 | [Web UI闭环场景](../../../testing-manual.md#testing-web-ui-closed-loop-smoke)；逐本条义务审读输入/状态与恢复边界；运行验证需精确候选、获准主体及真实已公布capability，当前未执行 | 本条SVG区域与静态源/契约核对；运行evidence仍缺 | 不证明未实现写面、所有状态组合、native、权限执行与结算原子性 |
| [REQ-WR-GR-006](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-006)；Trace refs: O7-REQ-46939a8f-c2a9-4ab6-818c-6bfa4bc83c5c | [GR-006](viewer-governance-targets-uiux.design.md#t-gov-02-gr-006) | 条件目标读面：active / observing / suspended / dissolved, 触发证据与权威观察窗口, 能力收缩、恢复条件与异议, bond 与设施、Agent 处置；授权阶段裁决者；各资产 owner 执行独立子处置，不由汇总页面代签。 | [Web UI闭环场景](../../../testing-manual.md#testing-web-ui-closed-loop-smoke)；逐本条义务审读输入/状态与恢复边界；运行验证需精确候选、获准主体及真实已公布capability，当前未执行 | 本条SVG区域与静态源/契约核对；运行evidence仍缺 | 不证明未实现写面、所有状态组合、native、权限执行与结算原子性 |
| [REQ-WR-GR-007](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-007)；Trace refs: O7-REQ-14aa9ab5-d30d-4be2-8c24-610bdc1d1186 | [GR-007](viewer-governance-targets-uiux.design.md#t-gov-02-gr-007) | 条件目标读面：地点 / 用途 / 建设权范围, 原取得依据与维护续期, 到期、闲置、违约或公共必要性, 通知、理由、申诉状态；当前读权限 audience 可看允许信息；完整写角色不由本设计补造。 | [Web UI闭环场景](../../../testing-manual.md#testing-web-ui-closed-loop-smoke)；逐本条义务审读输入/状态与恢复边界；运行验证需精确候选、获准主体及真实已公布capability，当前未执行 | 本条SVG区域与静态源/契约核对；运行evidence仍缺 | 不证明未实现写面、所有状态组合、native、权限执行与结算原子性 |
| [REQ-WR-GR-008](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-008)；Trace refs: O7-REQ-17536c7e-c59a-464a-90e8-c36f0330c8f7 | [GR-008](viewer-governance-targets-uiux.design.md#t-gov-02-gr-008) | 条件目标读面：service_fee / public_levy 类型, charter 预授权与专项目的, 受益范围、征收依据与主体, 金额 / 资源 / 上限 / 有效期；合法治理/筹资 authority 提供 LevyAuthority；结算主体在当前专项范围内执行，付款方不能自发升级服务合同。 | [Web UI闭环场景](../../../testing-manual.md#testing-web-ui-closed-loop-smoke)；逐本条义务审读输入/状态与恢复边界；运行验证需精确候选、获准主体及真实已公布capability，当前未执行 | 本条SVG区域与静态源/契约核对；运行evidence仍缺 | 不证明未实现写面、所有状态组合、native、权限执行与结算原子性 |
| [REQ-WR-GR-002](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-002)；Trace refs: O7-REQ-ce8c31b1-5294-4acd-a65f-2012bcbef911 | [GR-002](viewer-governance-targets-uiux.design.md#t-gov-03-gr-002) | 条件目标读面：release / artifact 精确身份, 完整 profile payload 承诺, 当前 owner 与审批角色, 能力范围与世界前置；当前 owner 与审批角色分别复核完整 payload；只承接已读 player publication 子范围。 | [Web UI闭环场景](../../../testing-manual.md#testing-web-ui-closed-loop-smoke)；逐本条义务审读输入/状态与恢复边界；运行验证需精确候选、获准主体及真实已公布capability，当前未执行 | 本条SVG区域与静态源/契约核对；运行evidence仍缺 | 不证明未实现写面、所有状态组合、native、权限执行与结算原子性 |
| [REQ-WR-GR-003](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-003)；Trace refs: O7-REQ-ac12295b-63a5-4440-9924-deaedafd7965 | [GR-003](viewer-governance-targets-uiux.design.md#t-gov-03-gr-003) | 条件目标读面：被移除 FROM artifact tuple, 恢复 TO artifact / rollback 来源, 新准入冻结 marker 与理由, 既有 profile / instance 保留；合法 rollback authority；仅停止 exact FROM artifact 新准入，不推导一般撤销/替代。 | [Web UI闭环场景](../../../testing-manual.md#testing-web-ui-closed-loop-smoke)；逐本条义务审读输入/状态与恢复边界；运行验证需精确候选、获准主体及真实已公布capability，当前未执行 | 本条SVG区域与静态源/契约核对；运行evidence仍缺 | 不证明未实现写面、所有状态组合、native、权限执行与结算原子性 |
| [REQ-WR-GR-009](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-009)；Trace refs: O7-REQ-3d713ab7-87c2-41df-9c89-827859c23313 | [GR-009](viewer-governance-targets-uiux.design.md#t-gov-03-gr-009) | 条件目标读面：能力提案与不可变输入, 确定性模拟来源与结果, 试点 scope / 期限 / 资源上限, 试点退出条件与实际结果；当前读权限 audience 可看允许信息；完整写角色不由本设计补造。 | [Web UI闭环场景](../../../testing-manual.md#testing-web-ui-closed-loop-smoke)；逐本条义务审读输入/状态与恢复边界；运行验证需精确候选、获准主体及真实已公布capability，当前未执行 | 本条SVG区域与静态源/契约核对；运行evidence仍缺 | 不证明未实现写面、所有状态组合、native、权限执行与结算原子性 |
| [REQ-WR-GR-010](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-010)；Trace refs: O7-REQ-0ec3d72f-51d1-4d64-9824-ebd9365f2d47 | [GR-010](viewer-governance-targets-uiux.design.md#t-gov-03-gr-010) | 条件目标读面：能力 / 用途 / 区域, 期限与到期开放条件, 许可 / 版税 / 收益权依据, 维护责任与审计；当前读权限 audience 可看允许信息；完整写角色不由本设计补造。 | [Web UI闭环场景](../../../testing-manual.md#testing-web-ui-closed-loop-smoke)；逐本条义务审读输入/状态与恢复边界；运行验证需精确候选、获准主体及真实已公布capability，当前未执行 | 本条SVG区域与静态源/契约核对；运行evidence仍缺 | 不证明未实现写面、所有状态组合、native、权限执行与结算原子性 |
| [SC32-institution](../../product/world-rules-core-gameplay/prd.md#root-sc32-institution)；Trace refs: O7-REQ-90614e34-bbc7-42df-b355-7ce0b34ccdc3 | [SC32-institution](viewer-governance-targets-uiux.design.md#t-gov-03-sc32-institution) | 条件目标读面：Alliance — Compatibility keep, War — Defer, Governance — Kernel keep, EconomicContract — Compatibility keep；当前读权限 audience 可看允许信息；完整写角色不由本设计补造。 | [Web UI闭环场景](../../../testing-manual.md#testing-web-ui-closed-loop-smoke)；逐本条义务审读输入/状态与恢复边界；运行验证需精确候选、获准主体及真实已公布capability，当前未执行 | 本条SVG区域与静态源/契约核对；运行evidence仍缺 | 不证明未实现写面、所有状态组合、native、权限执行与结算原子性 |
| [SC32-pilot](../../product/world-rules-core-gameplay/prd.md#root-sc32-pilot)；Trace refs: O7-REQ-f081f96d-f95a-4538-b0b7-3edf23ff772a | [SC32-pilot](viewer-governance-targets-uiux.design.md#t-gov-03-sc32-pilot) | 条件目标读面：Alliance 或 EconomicContract 唯一候选, 另一制度保持兼容, 治理批准 manifest / activation, L0 / L1 不可绕过；当前读权限 audience 可看允许信息；完整写角色不由本设计补造。 | [Web UI闭环场景](../../../testing-manual.md#testing-web-ui-closed-loop-smoke)；逐本条义务审读输入/状态与恢复边界；运行验证需精确候选、获准主体及真实已公布capability，当前未执行 | 本条SVG区域与静态源/契约核对；运行evidence仍缺 | 不证明未实现写面、所有状态组合、native、权限执行与结算原子性 |
| [SC32-budgetread](../../product/world-rules-core-gameplay/prd.md#root-sc32-budgetread)；Trace refs: O7-REQ-96cb1655-2ca3-445f-af2e-9f113e6f3e8e | [SC32-budgetread](viewer-governance-targets-uiux.design.md#t-gov-03-sc32-budgetread) | 条件目标读面：Agent / Module / System owner, payer 与绑定来源, 能力 scope / 是否计费, quote units 与预算影响；当前读权限 audience 可看允许信息；完整写角色不由本设计补造。 | [Web UI闭环场景](../../../testing-manual.md#testing-web-ui-closed-loop-smoke)；逐本条义务审读输入/状态与恢复边界；运行验证需精确候选、获准主体及真实已公布capability，当前未执行 | 本条SVG区域与静态源/契约核对；运行evidence仍缺 | 不证明未实现写面、所有状态组合、native、权限执行与结算原子性 |
| [SC32-budgetcommit](../../product/world-rules-core-gameplay/prd.md#root-sc32-budgetcommit)；Trace refs: O7-REQ-70e55a1f-7087-41e8-918d-7d1989c0155a | [SC32-budgetcommit](viewer-governance-targets-uiux.design.md#t-gov-03-sc32-budgetcommit) | 条件目标读面：原操作身份与 grant, 执行前预算 / 当前 owner, actual effect 与 debit 关联, authorization receipt / effect completion；当前读权限 audience 可看允许信息；完整写角色不由本设计补造。 | [Web UI闭环场景](../../../testing-manual.md#testing-web-ui-closed-loop-smoke)；逐本条义务审读输入/状态与恢复边界；运行验证需精确候选、获准主体及真实已公布capability，当前未执行 | 本条SVG区域与静态源/契约核对；运行evidence仍缺 | 不证明未实现写面、所有状态组合、native、权限执行与结算原子性 |
| [SC33-extscope](../../product/world-rules-core-gameplay/prd.md#root-sc33-extscope)；Trace refs: O7-REQ-e6f4ffa7-0b1b-4d0e-b7a3-8ca38c8d0244 | [SC33-extscope](viewer-governance-targets-uiux.design.md#t-gov-03-sc33-extscope) | 条件目标读面：industrial_externality / operational_mitigation, recipe / factory profile 与版本, scope、authority 与有效期, 容量与已有 obligation；profile 声明动作与当前 mitigation authority 同时适用的 owner/scope 主体；不授地方治理权。 | [Web UI闭环场景](../../../testing-manual.md#testing-web-ui-closed-loop-smoke)；逐本条义务审读输入/状态与恢复边界；运行验证需精确候选、获准主体及真实已公布capability，当前未执行 | 本条SVG区域与静态源/契约核对；运行evidence仍缺 | 不证明未实现写面、所有状态组合、native、权限执行与结算原子性 |
| [SC33-extdispose](../../product/world-rules-core-gameplay/prd.md#root-sc33-extdispose)；Trace refs: O7-REQ-bd83785c-d414-4bd7-b291-41a4d7272aa3 | [SC33-extdispose](viewer-governance-targets-uiux.design.md#t-gov-03-sc33-extdispose) | 条件目标读面：原 root / revision / child, WIP / transit / buffer / terminal, production result 与 disposition refs, contained / held_pending_disposition；原工业/外部性义务的当前合法处置主体；依 profile 支持动作和阶段 sink 边界逐次校验。 | [Web UI闭环场景](../../../testing-manual.md#testing-web-ui-closed-loop-smoke)；逐本条义务审读输入/状态与恢复边界；运行验证需精确候选、获准主体及真实已公布capability，当前未执行 | 本条SVG区域与静态源/契约核对；运行evidence仍缺 | 不证明未实现写面、所有状态组合、native、权限执行与结算原子性 |
| [REQ-WR-CR-001](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-001)；Trace refs: O7-REQ-16ef3247-1587-4cd8-84ab-03a9bb18a17f | [CR-001](viewer-governance-targets-uiux.design.md#t-gov-04-cr-001) | 条件目标读面：沟通 / 草稿 / 未接受提议, 各方主体与受权范围, 完整条款与明确接受, 权威校验与 receipt；当前读权限 audience 可看允许信息；完整写角色不由本设计补造。 | [Web UI闭环场景](../../../testing-manual.md#testing-web-ui-closed-loop-smoke)；逐本条义务审读输入/状态与恢复边界；运行验证需精确候选、获准主体及真实已公布capability，当前未执行 | 本条SVG区域与静态源/契约核对；运行evidence仍缺 | 不证明未实现写面、所有状态组合、native、权限执行与结算原子性 |
| [REQ-WR-CR-002](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-002)；Trace refs: O7-REQ-805fc92a-bbcb-4e77-b2d8-e40105522a0d | [CR-002](viewer-governance-targets-uiux.design.md#t-gov-04-cr-002) | 条件目标读面：服务身份 / 原合同, 已完成 checkpoint 与结果, 未履行范围与责任方, 修复或重新履行；当前读权限 audience 可看允许信息；完整写角色不由本设计补造。 | [Web UI闭环场景](../../../testing-manual.md#testing-web-ui-closed-loop-smoke)；逐本条义务审读输入/状态与恢复边界；运行验证需精确候选、获准主体及真实已公布capability，当前未执行 | 本条SVG区域与静态源/契约核对；运行evidence仍缺 | 不证明未实现写面、所有状态组合、native、权限执行与结算原子性 |
| [REQ-WR-CR-003](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-003)；Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905 | [CR-003](viewer-governance-targets-uiux.design.md#t-gov-04-cr-003) | 条件目标读面：actor / subject / object / action kind, claim summary / confidence / stake risk, 受影响关系与 social surfaces, cooperation / dispute / governance relevance；受当前事实 scope 与共享资格校验的 actor；这里只设计已有 Simulator 只读预览子范围，不启用完整发布/R&D。 | [Web UI闭环场景](../../../testing-manual.md#testing-web-ui-closed-loop-smoke)；逐本条义务审读输入/状态与恢复边界；运行验证需精确候选、获准主体及真实已公布capability，当前未执行 | 本条SVG区域与静态源/契约核对；运行evidence仍缺 | 不证明未实现写面、所有状态组合、native、权限执行与结算原子性 |
| [REQ-WR-OC-001](../../product/world-rules-core-gameplay/organization-continuity-dissolution-and-dormancy-protection.prd.md#req-wr-oc-001)；Trace refs: O7-REQ-366c9e94-4bd6-41f8-93dd-8d87f163f21a | [OC-001](viewer-governance-targets-uiux.design.md#t-gov-05-oc-001) | 条件目标读面：organization / case / policy / manifest, Inventory → Obligations → CustodyReturn, Claims → BusinessDisposition → Residual, 每项 owner / held-in-trust contributor；原 appointment authority 且当前治理通过；首版仅 Disposition/CustodyReturn 或明确合格 successor scope 的完整 held-in-trust 返还。 | [Web UI闭环场景](../../../testing-manual.md#testing-web-ui-closed-loop-smoke)；逐本条义务审读输入/状态与恢复边界；运行验证需精确候选、获准主体及真实已公布capability，当前未执行 | 本条SVG区域与静态源/契约核对；运行evidence仍缺 | 不证明未实现写面、所有状态组合、native、权限执行与结算原子性 |
| [REQ-WR-OC-002](../../product/world-rules-core-gameplay/organization-continuity-dissolution-and-dormancy-protection.prd.md#req-wr-oc-002)；Trace refs: O7-REQ-6e6d890d-c94d-4b43-9947-bfccf2f066e3 | [OC-002](viewer-governance-targets-uiux.design.md#t-gov-05-oc-002) | 条件目标读面：Operating / Notified / Protected / Restricted, availability / activity coverage 分列, 权威 C 窗口与暂停原因, 通知记录与保护期；具权利来源的 reclaim/appeal 申请人；独立救济裁决者判定支持范围，不由申请人自行复权。 | [Web UI闭环场景](../../../testing-manual.md#testing-web-ui-closed-loop-smoke)；逐本条义务审读输入/状态与恢复边界；运行验证需精确候选、获准主体及真实已公布capability，当前未执行 | 本条SVG区域与静态源/契约核对；运行evidence仍缺 | 不证明未实现写面、所有状态组合、native、权限执行与结算原子性 |
| [REQ-WR-OC-003](../../product/world-rules-core-gameplay/organization-continuity-dissolution-and-dormancy-protection.prd.md#req-wr-oc-003)；Trace refs: O7-REQ-b59a4fa5-9b5f-4119-abf6-7c0f2b4f4268 | [OC-003](viewer-governance-targets-uiux.design.md#t-gov-05-oc-003) | 条件目标读面：旧 grant / 新候选资格绑定, object / right / action / quantity scope, 重叠范围与 stay / quarantine, 原请求绑定 grant / scope version；当前任命 authority 与通过当前控制绑定/顺位核验并接受的 successor；重叠旧授权原子终止。 | [Web UI闭环场景](../../../testing-manual.md#testing-web-ui-closed-loop-smoke)；逐本条义务审读输入/状态与恢复边界；运行验证需精确候选、获准主体及真实已公布capability，当前未执行 | 本条SVG区域与静态源/契约核对；运行evidence仍缺 | 不证明未实现写面、所有状态组合、native、权限执行与结算原子性 |
| [REQ-WR-CC-001](../../product/world-rules-core-gameplay/chartered-conflict-soft-seasons-and-recovery.prd.md#req-wr-cc-001)；Trace refs: O7-REQ-2e68a48f-4a44-4135-9268-e17c6f32d0b3 | [CC-001](viewer-governance-targets-uiux.design.md#t-gov-06-cc-001) | 条件目标读面：charter 与冲突授权, 参战主体 / 区域范围, 暴露资产与时间窗口, 非参与者保护；当前读权限 audience 可看允许信息；完整写角色不由本设计补造。 | [Web UI闭环场景](../../../testing-manual.md#testing-web-ui-closed-loop-smoke)；逐本条义务审读输入/状态与恢复边界；运行验证需精确候选、获准主体及真实已公布capability，当前未执行 | 本条SVG区域与静态源/契约核对；运行evidence仍缺 | 不证明未实现写面、所有状态组合、native、权限执行与结算原子性 |
| [REQ-WR-CC-002](../../product/world-rules-core-gameplay/chartered-conflict-soft-seasons-and-recovery.prd.md#req-wr-cc-002)；Trace refs: O7-REQ-19ea1035-2d0a-43fc-a3a8-b0309030273c | [CC-002](viewer-governance-targets-uiux.design.md#t-gov-06-cc-002) | 条件目标读面：受损对象与损失范围, 保留的玩家 / Agent 身份, 历史 receipt 与原能力, repair / rebuild / pivot / 撤离；当前读权限 audience 可看允许信息；完整写角色不由本设计补造。 | [Web UI闭环场景](../../../testing-manual.md#testing-web-ui-closed-loop-smoke)；逐本条义务审读输入/状态与恢复边界；运行验证需精确候选、获准主体及真实已公布capability，当前未执行 | 本条SVG区域与静态源/契约核对；运行evidence仍缺 | 不证明未实现写面、所有状态组合、native、权限执行与结算原子性 |
| [REQ-WR-CC-003](../../product/world-rules-core-gameplay/chartered-conflict-soft-seasons-and-recovery.prd.md#req-wr-cc-003)；Trace refs: O7-REQ-eedaa5cf-e91e-48df-9c33-158774b40168 | [CC-003](viewer-governance-targets-uiux.design.md#t-gov-06-cc-003) | 条件目标读面：project / containment / policy / revision, 参与资格与 source claim, contribution accepted / waiting_evidence, milestone 决定与 target revision；有权 owner 开项目；contributor 提供 source claim；受权 evaluator 判证据；项目 authority 应用 milestone；独立复核处理申诉。 | [Web UI闭环场景](../../../testing-manual.md#testing-web-ui-closed-loop-smoke)；逐本条义务审读输入/状态与恢复边界；运行验证需精确候选、获准主体及真实已公布capability，当前未执行 | 本条SVG区域与静态源/契约核对；运行evidence仍缺 | 不证明未实现写面、所有状态组合、native、权限执行与结算原子性 |
| [REQ-WR-CC-004](../../product/world-rules-core-gameplay/chartered-conflict-soft-seasons-and-recovery.prd.md#req-wr-cc-004)；Trace refs: O7-REQ-a2b47612-d2c2-4331-811e-58a61366b64d | [CC-004](viewer-governance-targets-uiux.design.md#t-gov-06-cc-004) | 条件目标读面：contested occupation 的授权范围, 持续占有与到期冻结, 已成立权利及其 receipt, 物理提取与在途物品；当前读权限 audience 可看允许信息；完整写角色不由本设计补造。 | [Web UI闭环场景](../../../testing-manual.md#testing-web-ui-closed-loop-smoke)；逐本条义务审读输入/状态与恢复边界；运行验证需精确候选、获准主体及真实已公布capability，当前未执行 | 本条SVG区域与静态源/契约核对；运行evidence仍缺 | 不证明未实现写面、所有状态组合、native、权限执行与结算原子性 |
| [REQ-WR-CC-005](../../product/world-rules-core-gameplay/chartered-conflict-soft-seasons-and-recovery.prd.md#req-wr-cc-005)；Trace refs: O7-REQ-ce9fbace-7fc6-4973-8bcf-8b58e11b3339 | [CC-005](viewer-governance-targets-uiux.design.md#t-gov-06-cc-005) | 条件目标读面：刷新竞争机会与区域资格, 生效条件、对象及边界, 同一世界时间线, 玩家 / Agent 身份与核心能力；当前读权限 audience 可看允许信息；完整写角色不由本设计补造。 | [Web UI闭环场景](../../../testing-manual.md#testing-web-ui-closed-loop-smoke)；逐本条义务审读输入/状态与恢复边界；运行验证需精确候选、获准主体及真实已公布capability，当前未执行 | 本条SVG区域与静态源/契约核对；运行evidence仍缺 | 不证明未实现写面、所有状态组合、native、权限执行与结算原子性 |
| [REQ-WR-FI-001](../../product/world-rules-core-gameplay/frontier-expansion-and-world-information-boundaries.prd.md#req-wr-fi-001)；Trace refs: O7-REQ-3013b8cf-e3db-4506-a30a-db34e25ed636 | [FI-001](viewer-governance-targets-uiux.design.md#t-gov-07-fi-001) | 条件目标读面：相邻区域身份与空间来源, 探索前置, 物流连通前置, charter 组合准入；当前读权限 audience 可看允许信息；完整写角色不由本设计补造。 | [Web UI闭环场景](../../../testing-manual.md#testing-web-ui-closed-loop-smoke)；逐本条义务审读输入/状态与恢复边界；运行验证需精确候选、获准主体及真实已公布capability，当前未执行 | 本条SVG区域与静态源/契约核对；运行evidence仍缺 | 不证明未实现写面、所有状态组合、native、权限执行与结算原子性 |
| [REQ-WR-FI-002](../../product/world-rules-core-gameplay/frontier-expansion-and-world-information-boundaries.prd.md#req-wr-fi-002)；Trace refs: O7-REQ-066843dd-9e40-4148-8933-d369d16b55a2 | [FI-002](viewer-governance-targets-uiux.design.md#t-gov-07-fi-002) | 条件目标读面：地点 / 用途 / 当前持有人, 未完成开发里程碑, 原期限与剩余范围, 受让人当前资格；当前读权限 audience 可看允许信息；完整写角色不由本设计补造。 | [Web UI闭环场景](../../../testing-manual.md#testing-web-ui-closed-loop-smoke)；逐本条义务审读输入/状态与恢复边界；运行验证需精确候选、获准主体及真实已公布capability，当前未执行 | 本条SVG区域与静态源/契约核对；运行evidence仍缺 | 不证明未实现写面、所有状态组合、native、权限执行与结算原子性 |
| [REQ-WR-FI-003](../../product/world-rules-core-gameplay/frontier-expansion-and-world-information-boundaries.prd.md#req-wr-fi-003)；Trace refs: O7-REQ-23640893-de22-4940-9b25-ed08c9ab0f7e | [FI-003](viewer-governance-targets-uiux.design.md#t-gov-07-fi-003) | 条件目标读面：公共规则 / 已结算事实, 最低安全与通行披露, 有界私用 → 公共 baseline, live inventory / routes 的取得依据；当前读权限 audience 可看允许信息；完整写角色不由本设计补造。 | [Web UI闭环场景](../../../testing-manual.md#testing-web-ui-closed-loop-smoke)；逐本条义务审读输入/状态与恢复边界；运行验证需精确候选、获准主体及真实已公布capability，当前未执行 | 本条SVG区域与静态源/契约核对；运行evidence仍缺 | 不证明未实现写面、所有状态组合、native、权限执行与结算原子性 |
| [SC8-dataread](../../product/world-rules-core-gameplay/prd.md#root-sc8-dataread)；Trace refs: O7-REQ-6bd89114-455c-490b-9b21-425fa90df866 | [SC8-dataread](viewer-governance-targets-uiux.design.md#t-gov-07-sc8-dataread) | 条件目标读面：owner 与 recipient, purpose 与 scope, 当前 grant / 有效性, 来源与预览身份；当前读权限 audience 可看允许信息；完整写角色不由本设计补造。 | [Web UI闭环场景](../../../testing-manual.md#testing-web-ui-closed-loop-smoke)；逐本条义务审读输入/状态与恢复边界；运行验证需精确候选、获准主体及真实已公布capability，当前未执行 | 本条SVG区域与静态源/契约核对；运行evidence仍缺 | 不证明未实现写面、所有状态组合、native、权限执行与结算原子性 |
| [SC8-datacommit](../../product/world-rules-core-gameplay/prd.md#root-sc8-datacommit)；Trace refs: O7-REQ-ebcc34f8-5db5-46f7-85fb-30b44b8b797f | [SC8-datacommit](viewer-governance-targets-uiux.design.md#t-gov-07-sc8-datacommit) | 条件目标读面：原提交 / grant / scope, 提交至结算间有效性, 拒绝或待决原因, 实际使用 / 转移；当前读权限 audience 可看允许信息；完整写角色不由本设计补造。 | [Web UI闭环场景](../../../testing-manual.md#testing-web-ui-closed-loop-smoke)；逐本条义务审读输入/状态与恢复边界；运行验证需精确候选、获准主体及真实已公布capability，当前未执行 | 本条SVG区域与静态源/契约核对；运行evidence仍缺 | 不证明未实现写面、所有状态组合、native、权限执行与结算原子性 |
| [SC8-datareplay](../../product/world-rules-core-gameplay/prd.md#root-sc8-datareplay)；Trace refs: O7-REQ-c549ec3e-5dc6-4e27-9285-4e326b0fc5d3 | [SC8-datareplay](viewer-governance-targets-uiux.design.md#t-gov-07-sc8-datareplay) | 条件目标读面：历史合法使用 / 转移 receipt, 原 owner / recipient / purpose, 当前 grant 撤销状态, 原请求与重复结果；当前读权限 audience 可看允许信息；完整写角色不由本设计补造。 | [Web UI闭环场景](../../../testing-manual.md#testing-web-ui-closed-loop-smoke)；逐本条义务审读输入/状态与恢复边界；运行验证需精确候选、获准主体及真实已公布capability，当前未执行 | 本条SVG区域与静态源/契约核对；运行evidence仍缺 | 不证明未实现写面、所有状态组合、native、权限执行与结算原子性 |
| [REQ-WR-GCB-005](../../product/world-rules-core-gameplay/governed-common-decisions-and-constitutional-boundaries.prd.md#req-wr-gcb-005)；Trace refs: O7-REQ-4b64b7c5-9819-4932-a1ad-3cdb09fa82e5 | [GCB-005](viewer-governance-targets-uiux.design.md#t-gov-08-gcb-005) | 条件目标读面：外部 OC 转让事实, 事项范围内 lock / snapshot, 退出或解除绑定的当前结果, 有效窗口内原计票历史；当前读权限 audience 可看允许信息；完整写角色不由本设计补造。 | [Web UI闭环场景](../../../testing-manual.md#testing-web-ui-closed-loop-smoke)；逐本条义务审读输入/状态与恢复边界；运行验证需精确候选、获准主体及真实已公布capability，当前未执行 | 本条SVG区域与静态源/契约核对；运行evidence仍缺 | 不证明未实现写面、所有状态组合、native、权限执行与结算原子性 |
| [REQ-WR-GR-011](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-011)；Trace refs: O7-REQ-d27a209c-a703-4443-92d9-acadb9820ed2 | [GR-011](viewer-governance-targets-uiux.design.md#t-gov-08-gr-011) | 条件目标读面：链上转让规则与链上成本, 世界内资源 / 信用 / 库存 / 合同, 地方 charter 的适用双资格, 工业准入 / 市场 / 紧急保供各自门禁；当前读权限 audience 可看允许信息；完整写角色不由本设计补造。 | [Web UI闭环场景](../../../testing-manual.md#testing-web-ui-closed-loop-smoke)；逐本条义务审读输入/状态与恢复边界；运行验证需精确候选、获准主体及真实已公布capability，当前未执行 | 本条SVG区域与静态源/契约核对；运行evidence仍缺 | 不证明未实现写面、所有状态组合、native、权限执行与结算原子性 |

## 12. 决策、长期风险与未决问题

- 图稿仅代表实际映射区域/状态；没有对应frame的项保持text_spec_only，未公布提交合同保持contract_blocked。图稿完备性不得用页面数判定。
- 本主题的具体frame及字段/动作/状态关系见随稿覆盖目录；未画状态明确保留text_spec_only，不能从概览推导。
- 动态DTO、未知action集合、部署能力、原生桌面及真实世界结果链仍待各专业owner在获准环境验证。
- 需求设计关系只覆盖表中适用范围，不自动将产品状态升为已覆盖；未核实的专业协议或目标CRUD保持明确缺口。

