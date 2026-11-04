# testing manual 文档入口

## 从这里开始

- 想先判断测试分层、门禁与通用执行顺序：读仓库级 [`testing-manual.md`](../../../testing-manual.md)。
- 想采样 Viewer Web 页面、保存截图 / state / console 证据：读 [`web-ui-agent-browser-closure-manual.manual.md`](web-ui-agent-browser-closure-manual.manual.md)。
- 想跑真实本地栈、真实 UI 输入与玩家流程矩阵：读 [`web-ui-playwright-closure-manual.manual.md`](web-ui-playwright-closure-manual.manual.md)。
- 想验证像素世界的原操作归属、授权分页/时效、探索跟随、图形恢复、可访问性、五秒首读、容量与 Web/native 同义：读 [`pixel-world-visualization-validation.manual.md`](pixel-world-visualization-validation.manual.md)。这是验证方法，不是当前通过报告；它细化而不替代外部 Runtime 的 viewer-* 组合场景。
- 想组织受控首局的真人研究：读[首局真人试玩手册](first-session-human-playtest.manual.md)，按同一候选完成筛选、两次会话、主持与提示、原始记录及分项分析；实际执行仍须满足原候选开放条件。
- 想核对双边即时交换的非绑定提议、授权接受、原子效果与恢复：读[合同形成验收手册](agent-mediated-contract-formation-acceptance.manual.md)；这是目标验证方法，不是当前业务通过记录。
- 想核对有限供能服务的部分履行、抵押、费用、修复、争议与显式继受：读[供能服务验收手册](checkpoint-power-service-acceptance.manual.md)，分别核资源守恒、责任范围、原请求恢复和玩家理解；方法存在不表示服务已可用。
- 想核对工业模拟、有限试点、治理准入及直接消费者：读[工业创建验收手册](industrial-creation-simulation-and-admission.manual.md)，区分静态输入算例和真实隔离、退出、恢复证据。
- 想核对有限许可、版税托管、通知/争议、开放和整数分配：读[许可结算验收手册](industrial-license-and-royalty-settlement.manual.md)，核真实批次、所有资金来源及旧请求恢复。
- 想核对合作前情境声誉的来源、主体/任期、时效和纠错：读[情境声誉验收手册](contextual-reputation-acceptance.manual.md)，不将 native 分数、自由声明或方法存在当作已交付事实。
- 想核对组织与接替岗位的托管返还履历：读[组织/岗位履历验收手册](organization-custody-role-reputation-acceptance.manual.md)，核真实原件、两条返还权限路径、历史任期、当前受众、派生纠错和三入口同义。
- 想核对研究章程的预工作同意、贡献来源、份额与许可交接：读 [R&D 归因及转让验收手册](rd-charter-attribution-and-share-transfer-acceptance.manual.md)，区分历史贡献、使用权、未付权益和已付历史。
- 想核对外部 Runtime 私有上下文未接续后的告知与重规划：读[私有上下文恢复场景](external-agent-runtime-contract-validation.manual.md#adapter-private-context-recovery)，原模型调用、世界操作、当前任务与私有会话分别取证。
- 想让本地入口接入 formal `public_testnet` world state，或排查 LetAI 配置、代理继承、模型选择、quota 与受控 auto-topup：读 [`local-public-testnet-letai-test-environment-2026-06-23.manual.md`](local-public-testnet-letai-test-environment-2026-06-23.manual.md)。
- 想在受治理 deployment truth 下只物化一台已擦除 validator host、保持服务 disabled/inactive 并输出 public receipt：先读 blockchain ops authority [`../../p2p/blockchain/public-testnet-governed-bootstrap.runbook.md`](../../p2p/blockchain/public-testnet-governed-bootstrap.runbook.md)，再按 [`public-testnet-fresh-validator-host-bootstrap-2026-07-28.manual.md`](public-testnet-fresh-validator-host-bootstrap-2026-07-28.manual.md) 执行其 narrow subprocedure。
- 想用截图加模型视觉评审替代 routine 人工 review：读 [`model-visual-review-sop-2026-05-29.manual.md`](model-visual-review-sop-2026-05-29.manual.md)，并使用 [`../templates/model-visual-review-card-template.md`](../templates/model-visual-review-card-template.md)。

- 想在单台 ECS 上通过 CI / OSS 自动更新 Preview、保留现有世界并验证 / 恢复：读 [`single-ecs-preview-auto-update.manual.md`](single-ecs-preview-auto-update.manual.md)。

## 文档分工

| 需求 | 权威入口 | 不负责的内容 |
| --- | --- | --- |
| 仓库级测试层级、required/full 与通用操作 | `testing-manual.md` | 专项 Web UI 操作步骤 |
| Viewer Web 页面闭环与留证 | `web-ui-agent-browser-closure-manual.manual.md` | launcher 控制面产品动作、public testnet attach 证明 |
| 真实本地栈的 Playwright 玩家流程 | `web-ui-playwright-closure-manual.manual.md` | 通用 agent-browser 页面采样 |
| 像素世界的数据/体验/恢复/质量与逐端验收 | `pixel-world-visualization-validation.manual.md` | 新增通用 CI gate、模拟运行成绩、付费模型或完整发行放行 |
| 受控首局真人研究的招募准备、双会话主持与原始记录 | `first-session-human-playtest.manual.md` | 招募授权、运行成绩、全游戏好玩性与留存证明 |
| 双边 Electricity/Data 合同形成与即时交换 | `agent-mediated-contract-formation-acceptance.manual.md` | 持续服务、争端/继受、现有 native 兼容面或正式入口完成证明 |
| 有限 Electricity 供能服务的检查点与救济 | `checkpoint-power-service-acceptance.manual.md` | 其他服务、通用法院/声誉、实际运行或完整 CR-002 完成证明 |
| 工业创建四阶段的隔离、预算、准入和恢复 | `industrial-creation-simulation-and-admission.manual.md` | Shadow 即模拟、Apply 即准入或未执行的运行证据 |
| 许可范围、有限版税与开放结算 | `industrial-license-and-royalty-settlement.manual.md` | 任意 receipt 计费、外部价值、真实参数平衡或已启用声明 |
| 合同来源的情境声誉与客观复核 | `contextual-reputation-acceptance.manual.md` | 全局评分、资格授予、组织/岗位或完整 CR-003 完成证明 |
| 组织与接替岗位的托管返还履历咨询 | `organization-custody-role-reputation-acceptance.manual.md` | 一般组织经营、开放岗位、资格评分、原处分重裁或完整 CR-003 证明 |
| 预工作 R&D 归因、固定分配与整槽转让 | `rd-charter-attribution-and-share-transfer-acceptance.manual.md` | 现实知识产权裁判、自动贡献评分、已实现或完整 CR-003 证明 |
| 外部 Runtime 私有上下文告知与可信重规划 | [adapter-private-context-recovery](external-agent-runtime-contract-validation.manual.md#adapter-private-context-recovery) | 私有记忆无损迁移、模型理解、世界成果或一个 driver 代签另一个 |
| 本地入口接入 formal public testnet | `local-public-testnet-letai-test-environment-2026-06-23.manual.md` | 纯 local-only LetAI playtest |
| 已擦除 validator host 的 no-start 物化 | `doc/p2p/blockchain/public-testnet-governed-bootstrap.runbook.md`，再进入 `public-testnet-fresh-validator-host-bootstrap-2026-07-28.manual.md` | topology 审批、服务启用、网络 readiness 或 release claim |
| 截图的模型视觉评审 | `model-visual-review-sop-2026-05-29.manual.md` | runtime / security / release 放行结论 |

## 规格与追溯

- 已完成的系统测试手册工程化专题三件套已被 `testing-manual.md`、testing 根 PRD 与证据入口吸收并删除；`PRD-TESTING-MANUAL-001..003`、TMAN/DEC 历史映射保留在 `../prd.md`，实现过程从 Git history 与 PR 与实际验证记录追溯。
- `web-ui-agent-browser-closure-manual.prd.md` 与 PR、实际 CI 与评审记录（Issue 按需）分别保留 Web UI 闭环的需求 / 执行真值；对应 `*.manual.md` 承担实际步骤。
- `web-ui-playwright-closure-manual.design.md` 是该手册系列的 historical/shared design companion；真实操作从 `web-ui-playwright-closure-manual.manual.md` 进入。它仍受索引与审计留痕引用，不能当作无引用腐旧文件删除。

## 维护边界

- 新增或调整 operator 手册时，先更新本页的按问题分流；再在 `doc/testing/README.md`、`doc/testing/prd.index.md` 保持子树入口可达。
- 手册不得把 pure local playtest 与 formal public-testnet attach 证明混为一谈；具体边界以对应 runbook 为准。
- 历史设计 companion、PRD 和 project 只有在已被现行真值替代且 focused 引用审计确认无活跃调用时才能退役删除。

- 组织解散、不活跃保护、授权交接及恢复验收：[组织连续性系统验收计划](organization-continuity-system-acceptance.manual.md)（未执行）。
