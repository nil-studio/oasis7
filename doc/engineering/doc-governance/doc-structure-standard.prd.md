# 仓库文档分工与组织规范（2026-03-09）

- 对应设计文档: `doc/engineering/doc-governance/doc-structure-standard.design.md`
- 历史执行记录: Git history、PR 与实际验证记录；本文维护现行需求，不充当执行台账。

审计轮次: 4

- 当前执行入口: Git、PR、实际 CI 与评审记录；Issue/Project 按需使用，不构成建档或修改前置。
- 对应规范正文: `doc/engineering/doc-governance/doc-structure-standard.design.md`

## 目标
- 为仓库后续新增文档建立统一、可判定、可扩展的组织规范，解决“详细设计写在哪里、同类文档如何命名、专题目录如何收口”的问题。
- 冻结“目录按对象（模块/专题/分册）、文件按职责（PRD/Design/Runbook/Manual）”的顶层规则，并将可变任务记录统一到 GitHub Issue/Git、PR 与实际 CI 记录。
- 将详细设计文档从“命名随意、分散在 architecture/interface/manual 中”收敛为可发现、可引用、可复用的正式角色。

## 范围
- 规定 `doc/` 下一级模块、专题目录、分册目录的推荐组织形式。
- 规定 `prd.md`、`*.prd.md`、`*.design.md`、PR 与实际验证记录（Issue 按需）、`*.runbook.md`、`*.manual.md` 的职责边界。
- 规定模块级固定入口、专题级 PRD/Design 文档、阅读顺序、引用关系与命名规则。
- 不包含历史文档迁移计划、批量改名策略与脚本实施细节。

## 接口 / 数据
- 规范适用范围：`doc/**/*.md`（`doc/devlog/**` 仍按既有不可变原则维护）。
- 规范主入口：`doc/README.md`、`doc/<module>/README.md`、`doc/<module>/prd.md`。
- 追踪主键：`PRD-ENGINEERING-015`。
- 标准载体：
  - 需求与边界：`doc/engineering/doc-governance/doc-structure-standard.prd.md`
  - 规范正文：`doc/engineering/doc-governance/doc-structure-standard.design.md`
  - 执行状态：Git、PR、实际 CI 与评审记录；需要跟踪时使用 Issue/Project，不在本 PRD 维护任务状态。

## 里程碑
- M1 (2026-03-09): 输出顶层规范草案，冻结文档对象层级与职责后缀模型。
- M2: 将规范作为后续新增专题建档时的默认依据。
- M3: 在后续治理迭代中按规范逐步补齐模块级 `design.md` 与专题级 `*.design.md`。

## 风险
- 若职责边界描述不够硬，作者仍会把设计细节堆进 `*.prd.md` 或 `*.manual.md`。
- 若目录对象层级定义过细，可能导致小专题建档成本过高。
- 若未保留合理例外，极简说明文档与运行手册会被过度模板化。

## 1. Executive Summary
- Problem Statement: 当前仓库已形成稳定的 PRD / Git、PR 与实际 CI 记录 / Devlog 主流程，但“详细设计”的正式载体与专题目录建档规则尚未统一，导致新文档落点与命名需要人工判断。
- Proposed Solution: 建立一套面向 `doc/` 的顶层组织规范，明确目录表达对象、文件后缀表达职责，并要求模块级入口与专题级 PRD/Design 文档保持一致。
- Success Criteria:
  - SC-1: 后续新增专题可在不口头沟通的前提下，按规范判断应创建的目录与文档类型。
  - SC-2: 详细设计有明确载体 `*.design.md`，且不再需要在 `architecture/interface/manual` 等命名之间临时选择。
  - SC-3: 模块入口文档能为评审者提供稳定阅读顺序：`PRD -> Design -> Topic Docs -> Git、PR 与实际 CI 记录`。
  - SC-4: 同一专题可通过目录内的同名 PRD / Design 文档及 Git、PR 与实际 CI 记录形成可发现的配套关系。
  - SC-5: 规范正文本身可被直接引用为后续文档治理评审的裁定依据。
  - SC-6: 模块 `README.md` 仅保留模块特有的阅读路径、高频专题与例外入口，不再在各模块重复散写共享治理规则。
  - SC-7: root-level legacy redirect 文档仅保留兼容跳转所需的最小信息，不再扩展为重复的 `目标/范围/状态` 壳文档。

## 2. User Experience & Functionality
- User Personas:
  - 文档作者：需要快速判断“这篇该写成什么”。
  - 模块负责人：需要让模块文档入口稳定且可扩展。
  - 评审者：需要可裁定的结构规则，以判断建档是否合规。
- User Scenarios & Frequency:
  - 新专题建档：每次新需求、新子系统或新专题启动时执行。
  - 模块入口维护：每个模块扩容、拆分主题或补文档时执行。
  - 文档评审：每次 review 文档结构、命名和边界时执行。
- User Stories:
  - PRD-ENGINEERING-015-001: As a 文档作者, I want one directory-and-suffix rule, so that I can create new docs without guessing where detailed design belongs.
  - PRD-ENGINEERING-015-002: As a 模块负责人, I want fixed module-level entry files, so that readers always know where to start.
  - PRD-ENGINEERING-015-003: As a 评审者, I want topic docs to use the same basename for paired PRD/Design and link actual PR/CI records, so that related documents are discoverable at a glance.
  - PRD-ENGINEERING-015-004: As a 运维/测试人员, I want runbook/manual to be separated from design, so that operational steps do not overwrite technical rationale.
  - PRD-ENGINEERING-015-005: As a 读者, I want README and legacy redirect pages to stay thin, so that I can reach canonical docs without rereading shared governance boilerplate.
- Critical User Flows:
  1. Flow-DOC-001（模块级建档）:
     `确定需求所属模块 -> 进入 doc/<module>/ -> 阅读 prd.md / design.md / PR 与实际验证记录（Issue 按需） -> 决定是否新增专题目录`
  2. Flow-DOC-002（专题级建档）:
     `先核对已有长期主题 -> 按需维护或新增配套 PRD/Design -> 同步实际受影响入口；Issue 按跟踪需要使用`
  3. Flow-DOC-003（评审阅读）:
     `先读模块 prd.md -> 再读模块 design.md -> 下钻 topic 文档；任务态查 PR 与实际记录，Issue 按需`
  4. Flow-DOC-004（角色裁定）:
     `作者准备写内容 -> 先判断是 Why/What/Done 还是 How/Structure/Contract 还是 How/When/Who -> 分别落到 PRD/Design 与 Git、PR 与实际 CI 记录`
  5. Flow-DOC-005（操作文档建档）:
     `若内容是操作步骤 -> 判断是常规使用还是发布/故障处置 -> 分别落到 *.manual.md / *.runbook.md`
  6. Flow-DOC-006（入口减重）:
     `判断信息是否为模块特有 -> 若是则保留在 README -> 若是共享结构规则则回链到规范正文 -> 若仅为历史兼容则保留最小 redirect 页`
- Functional Specification Matrix:

| 对象/能力 | 字段定义 | 动作/行为 | 状态转换 | 排序/归类规则 | 权限逻辑 |
| --- | --- | --- | --- | --- | --- |
| 模块主入口 | `README.md`、`prd.md`、`design.md`、`prd.index.md` | 建立模块总入口与导航 | `missing -> present -> maintained` | 固定落位于 `doc/<module>/` | 模块维护者可更新，所有贡献者可读 |
| 专题PRD/Design 最小文档集 | `<topic>.prd.md`、按需配对的 `<topic>.design.md`；执行事实另见 PR 与实际验证记录 | 对单一专题分别承载需求、设计、执行 | `draft -> active -> completed` | 同目录、同 basename、后缀区分职责 | 专题作者创建，评审者审核 |
| 设计正文 | 架构、组件、接口、状态机、异常、NFR | 解释系统如何工作 | `implicit -> explicit` | `*.design.md` 为首选正式载体 | 设计 owner 维护，相关模块 reviewer 复核 |
| 操作文档 | 使用步骤或值班剧本 | 区分 `manual` 与 `runbook` | `ad-hoc -> standardized` | 使用/测试走 `manual`，发布/故障走 `runbook` | 操作 owner 维护，执行者可读 |
| 目录分层 | 模块 / 专题 / 分册 | 根据对象大小决定是否拆层 | `flat -> topicized -> partitioned` | 优先模块级，再专题级，最后分册级 | 作者提议，评审者裁定例外 |
| 入口减重 | `README` 路由、共享规则回链、legacy redirect 目标 | 删除重复治理话术，只保留模块特有入口或兼容跳转 | `verbose -> routed` | 共享规则集中到规范正文，兼容页最小化 | 模块 owner 维护，评审者裁定是否仍有重复 |
- Acceptance Criteria:
  - AC-1: 规范明确给出模块级固定入口文件集合。
  - AC-2: 规范明确给出专题级推荐目录结构与同名配套规则。
  - AC-3: 规范明确区分 PRD、Design、Git、PR 与实际 CI 记录、Manual、Runbook 的职责边界。
  - AC-4: 规范明确说明“目录按对象、文件按职责”的原则。
  - AC-5: 规范明确给出阅读顺序与文档间引用关系。
  - AC-6: 规范明确给出允许的例外场景与约束，避免过度模板化。
  - AC-7: 模块 `README.md` 允许保留“从这里开始”、高频专题与模块特有例外，但共享目录规则、根入口收口与通用维护约定应集中到规范正文，不在各模块重复散写。
  - AC-8: root-level legacy redirect 文档只保留当前主入口、相关入口与只读声明，不再复制业务目标、范围、任务状态或设计壳。
- Non-Goals:
  - 不在本规范中规划历史文档迁移批次。
  - 不在本规范中直接修改 `scripts/doc-governance-check.sh` 门禁逻辑。
  - 不强制所有现存杂项说明文档立即改名为 `*.design.md`。

<a id="doc-authority-and-validation"></a>
### 现行权威、入口与检查一致性

本节承接 PRD-ENGINEERING-015 的现行治理要求；技术分配与验证见
[权威与检查设计](doc-structure-standard.design.md#doc-authority-resolution)。

- GOV-AC-1：争议先按内容和声明 scope 确定权威。同一权威内才比较审定版本；较新的 core、产品专题或索引不得跨域覆盖专业合同。跨域矛盾由相关责任人作显式决定，未决定前标明冲突及受影响结论，不新建通用审批材料。
- GOV-AC-2：Git、PR、实际 CI 与评审记录承载交付事实；Issue/Project 按需跟踪。活跃入口不能把已退役 project ledger、`.pm` 映射或额外任务身份作为日常开发前置；历史引用保留其版本与原判定。
- GOV-AC-3：Markdown 长度仅作编辑提示。其他规则成立时，999/1000/1001 行样本不能仅因长度失败；缺失引用、非法结构等真实错误仍失败。规范、help、退出语义及实际调用入口应一致，不改 Rust 代码/测试的体量规则。
- GOV-AC-4：registry.entry 必须落在所登记 domain 内，并在根导航中精确可达；product_overlay 仅归 product 记录。各检查阶段对 owning-module 的允许范围一致；活跃引用扫描覆盖实际消费文档的 tools，不以目录前缀或总数代替归属验证。
- GOV-AC-5：PRD/design 索引只承诺其真实覆盖范围；完整 evidence 检索应指向实际目录或当前 corpus entries/groups。逐链接核对 label 对应的目标与 fragment，不以同行其他链接存在相同 ID 代替。

治理仅处理当前规则冲突、失效入口和仍有实际消费者的检查缺口。去重前先保全独有合同、修复活跃/动态消费者和必要登记；不为文件数量、旧批次数或 main 前进重做历史迁移。冻结 evidence 与 legacy fixture 不改成当前通过记录。每项检查的实现与实际运行另由候选和测试结果证明；文档修订不表示脚本已修复。

## 3. AI System Requirements (If Applicable)
- Tool Requirements: 无新增 AI 专属工具要求；现有 Agent 与人工作者均按同一文档组织规范执行。
- Evaluation Strategy:
  - 人工评审检查：是否可根据正文对新文档唯一落位。
  - 结构检查：规范正文是否包含模块级、专题级、角色分工、命名规则与例外条款。

## 4. Technical Specifications
- Architecture Overview:
  - 一级模块目录是文档树的主边界，负责提供模块总览与固定入口。
  - 专题目录是需求/设计/执行的主要聚合单元，优先采用同名 PRD/Design 文档。
  - `*.design.md` 是详细设计的正式载体，可根据复杂度继续拆成设计分册，但模块/专题总入口保持稳定。
- Integration Points:
  - `doc/README.md`
  - `doc/<module>/README.md`
  - `doc/<module>/prd.md`
  - PR 与实际验证记录（Issue 按需）
  - `doc/engineering/doc-governance/doc-structure-standard.design.md`
- Edge Cases & Error Handling:
  - 极小主题：若只是短期工作且没有独立长期要求或结构设计，可直接使用 PR 说明；涉及稳定要求或组件/接口/状态机时更新适用的既有 PRD/design，不为形式新建文档。
  - 超大专题：允许在专题目录下继续拆 `design/` 或 `parts/` 分册，但保留一个总入口 `*.design.md`。
  - 纯操作文档：若目标仅是“怎么执行”，应直接使用 `*.manual.md` 或 `*.runbook.md`，不要伪装成设计文档。
  - 模块总设计缺失：不阻断本规范成立，但后续新模块应优先补 `doc/<module>/design.md`。
  - 历史自由命名文档：本规范不要求立刻迁移，但新增文档不再复制旧命名分叉。
  - 高体量模块入口：允许保留模块特有的“从这里开始”和高频专题，但不再把 repo-wide 结构规则重复写进每个模块 README。
  - legacy redirect：若文件仅承担兼容跳转，就不再补齐完整 PRD/Design 与 Git、PR 与实际 CI 记录壳；最小可读声明优先于形式完整。
- Non-Functional Requirements:
  - NFR-1: 新增文档的职责应能在 5 分钟内被评审者判定。
  - NFR-2: 模块级固定入口数量保持稳定，不因专题膨胀而改变阅读起点。
  - NFR-3: 专题目录内相关文档的 basename 一致率应为 100%。
  - NFR-4: 文档长度仅作编辑参考；是否拆分按语义与维护成本判断，不设 1000 行硬门禁，不要求长度豁免，也不压成长行规避提示。
  - NFR-5: 结构或职责例外须说明适用范围和理由；这一要求不将长度提示恢复为审批条件。
- Security & Privacy:
  - 本规范仅定义文档组织，不新增数据权限或隐私边界。
  - 涉及密钥、凭据、用户数据的细节仍由对应模块文档处理，不得因组织规范而外泄。

## 5. Risks & Roadmap
- Phased Rollout:
  - MVP (2026-03-09): 发布规范正文，冻结对象层级与职责后缀规则。
  - v1.1: 为一级模块补齐 `design.md` 的推荐模板与阅读顺序示例。
  - v2.0: 在文档治理评审中将该规范作为默认裁定基线。
- Technical Risks:
  - 风险-1: 现有术语“设计文档/架构文档/接口文档”并存，短期内仍会与新规则并行。
  - 风险-2: 若没有后续门禁或评审机制，规范可能停留在“建议”而非“默认做法”。

## 6. Validation & Decision Record
- Test Plan & Traceability:

| PRD-ID | 对应任务 | 测试层级 | 验证方法 | 回归影响范围 |
| --- | --- | --- | --- | --- |
| PRD-ENGINEERING-015 | TASK-ENGINEERING-025 | `test_tier_required` | `doc-governance-check` + 索引/入口引用检查 + 规范正文章节完整性核对 | 后续新增文档的落位与详细设计载体一致性 |

- Decision Log:
  - DEC-015-001: 选择“目录按对象、文件按职责”作为主组织原则；未选择“全仓按文档类型集中建目录”，因为当前仓库已经按模块/专题树长期演进，按对象组织更利于上下文聚合。
  - DEC-015-002: 选择 `*.design.md` 作为详细设计的首选正式载体；未强制继续沿用 `architecture/interface/integration` 自由命名，因为这些命名适合作为设计分册，但不适合作为统一入口类型。
  - DEC-015-003: 选择模块级固定入口 + 专题级 PRD/Design 文档；未要求所有专题强制附带 `manual/runbook`，因为并非每个专题都需要操作文档。
