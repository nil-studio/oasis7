# engineering 模块设计总览

审计轮次: 6

- 对应需求文档: `doc/engineering/prd.md`
- 执行记录: Git、PR、实际 CI 与评审记录；Issue 和 Project 按需使用
- 对应文件级索引: `doc/engineering/prd.index.md`

## 1. 设计定位
`engineering` 模块的 `design.md` 是工程治理的总体设计入口：
- 说明工程规则如何从规范文档落到门禁、索引、迁移与审读流程；
- 说明文档治理、角色协作与质量门之间的关系；
- 指明工程规范专题的衔接方式，并保留结构治理轮（如 ROUND-006）的历史来源。

## 2. 阅读顺序
模块级当前导航统一从 `doc/engineering/README.md` 开始；本设计文档只在进入模块三件套后承载结构设计、规则载体与执行链路，不再复制一套根入口顺序。

专题精确检索由 `doc/engineering/prd.index.md` 承担；历史 PRD 审读与 legacy 迁移证据见 Git history。

## 3. 设计结构
### 3.1 规则载体分层
- `prd.md`：定义工程规则 Why / What / Done。
- `design.md`：定义规则如何组织、如何落地、由哪些脚本与文档承接。
- Git、PR、实际 CI 和评审记录：承载交付事实；Issue/Project 按需要跟踪复杂工作的依赖和进度。
- `doc-governance/*`：定义 `doc/` 文档树的组织规范、默认阅读面减重规则与早期文档治理收口。
- `rust-governance/*`：定义 Rust 文件体量、长期零扫描与语义化结构拆分契约，以及 [Workspace、架构边界与协议产物设计](rust-governance/rust-engineering-contracts.design.md)；各专题区分当前实现与目标，不维护冻结扫描基线或一次性 burn-down 台账。
- `command-center/*`：开发指挥中心专题；[产品设计](command-center/command-center.prd.md) 拥有云端与本地的用户行为和验收，[系统设计](command-center/command-center.design.md) 拥有 Sites 网页、云端后端与个人数据、本地宿主及工具、来源身份、安全导航和恢复合同，通用流程仍由 workflow source of truth 主责。
- 历史 PRD 审读/治理台账：保留在 Git history logs；当前新增/变更文档追踪由模块入口与 doc governance 规则承接。

<a id="task-scoped-engineering-context-design"></a>
#### 按任务进入规则与检查

Trace refs: O7-REQ-b189f0a2-0425-4985-9a5d-86179428b586、O7-REQ-f1388c2b-6f64-4ebd-886e-0b741bfe176c；承接[工程知识与验收来源要求](prd.md#task-scoped-engineering-context)和[任务拆分与并行开发](workflow/source-of-truth.md#parallel-task-development)。本条属于工程导航和人工判断方法，产品层不适用，不提供自动加载器、独立检查权威或新的交付表单。

| 载体 | 主责与读取时机 | 维护边界 |
| --- | --- | --- |
| 根 [AGENTS](../../AGENTS.md) 与 [workflow](workflow/source-of-truth.md) | AGENTS 是最短入口；workflow 拥有通用开发、授权与交付规则 | 根入口保留必要约束及链接，不展开全部领域要求 |
| 领域 PRD/design/manual | 当修改其行为或消费其接口时读取；分别拥有要求、技术合同和操作方法 | 找到当前 authority 与适用条款后继续跟随直接消费者；历史状态不能替代当前设计 |
| 模块 README、现有模块指南 | 按任务路径先找职责、Start here、上下游与非显然约束 | 沿已有入口补短导航；不为每个 crate 机械新增 AGENTS |
| [.agents/roles](../../.agents/roles/) 与 .codex/agents | 角色卡提供专业关注点；工具配置只指向同一角色/主责入口 | 当前 Viewer Codex 配置已引用角色卡，修导航时无需复制规则到 TOML。不同客户端只假定其已明确支持的加载行为 |
| [.agents/skills](../../.agents/skills/README.md) | 需要相应操作方法时加载；规则来源仍是主责文档 | 方法不能新增审批、固定角色或覆盖用户授权；本地可选工具资料缺失不成为通用前置 |
| 路径规则与实际检查器 | 根据仓库已登记的路径、依赖、输入和风险选择检查 | 不假定任意规则目录自动加载；需在入口显式链接。实际命令和 required 选择归 [测试手册](../../testing-manual.md)及 CI 主责 |
| Git、PR、CI 与按需 Issue | 核对本次版本、执行及评审事实 | 不把过程日志、成功标签或个人记忆复制为长期产品/工程真值 |

执行者先阅读根入口，再按任务找到模块入口及具体 PRD/design；仅当触达跨层规则、共享接口、风险或外部消费者时补读其主责条款。缺少文档时沿索引和源码核查，能确定的事实继续推进；需要设计裁决或缺失资源才能完成的范围如实说明。上位要求与实现冲突时沿现有流程处理，不以源码现状静默覆盖要求。

按变更收集检查时记录准确 Source（文档条款、清单或测试入口）与选择理由，然后按实际命令、候选/输入、平台、features、target 和证明义务合并重复项。包名相同但 feature、浏览器/进程层或权限边界不同的检查不能合并。普通文档、局部内部实现、CI 控制/安全/兼容等风险采用各自必要审读；独立性由实际作者和审读者决定，不由角色名称或固定五审保证。

已有 PR 的验证段即可承载结果，例如“静态核对 pass，Source 为某设计锚点，Evidence 为固定提交和检查输出”；未执行的浏览器或跨平台范围写 skipped 与原因。命令返回 0 但选中项为空不能直接写覆盖通过；源文件存在仅证明可定位，不证明其测试已运行。实际命令语义由[本地与 CI 验证入口](../../testing-manual.md)拥有，本条不另定义 quick/affected/formal。

从父目标组织可用模块增量、逐步细化近期任务和判断拆分收益，统一按[项目拆解](workflow/source-of-truth.md#project-decomposition)执行；模块增量使用父项现有正文，不增加必建的 Issue 层级。[独立开发会话](workflow/source-of-truth.md#independent-development-sessions)、[原生 sub-issue 跟踪](workflow/source-of-truth.md#code-task-sub-issues)和[验收准备](workflow/source-of-truth.md#code-task-acceptance-readiness)同样由 workflow 定义；[parallel-development](../../.agents/skills/parallel-development/SKILL.md)、[executing-project-tasks](../../.agents/skills/executing-project-tasks/SKILL.md)和 [TPM 角色卡](../../.agents/roles/tpm.md)承接近期派工、早期真实消费检查、组合验收与收尾。直接开发会话与可选 TPM 使用同一[领取与状态维护规则](workflow/source-of-truth.md#coding-agent-task-claim)，现有 Issue/PR 模板承载必要安排和真实结果。

修改这些载体时，走读简单任务直接实施、后续范围未细化而当前批次就绪、同一业务的底层步骤留在原任务、共享接口早期检查、多个开发会话及可选 TPM、原会话修复与中断恢复；核对父子领取、写入隔离、共享预算、局部/整体验收和待集成成果优先收尾。专项试玩与 dot 时间表沿原规则执行，运行适用文档检查。静态验收只证明规则和入口一致；首次可用结果、接口返工和必要重构建次数的改善仍需实际任务数据，不新增固定效率报表或门禁。

#### Viewer 试点与有限走读

首轮使用现有 [viewer_engineer 角色卡](../../.agents/roles/viewer_engineer.md)作为短导航，连接 Viewer landing、前端结构标准、真实代码入口、消费方与测试手册；[Codex 配置](../../.codex/agents/viewer_engineer.toml)继续引用同一角色卡。角色卡只保留便于起步的索引与命令条件，接口、状态、生成产物和视觉验收仍回到领域文档。

| 代表任务 | 应读来源与选择 | 走读要核对的结果 |
| --- | --- | --- |
| 仅修正文档路径 | 根入口、被引用文档、文档治理检查范围 | 运行受影响文档及反向引用检查，更新真实导航；没有业务变化时不要求 Rust/浏览器全量。检查成功仅证明其结构范围 |
| 修改一个 Rust 包的内部函数 | 领域设计、实际调用方、Rust 工程合同、所属 target/feature 与现有回归 | 定向 canonical 测试及真实反向消费者，保留 CI planner 的必要范围；同包不同 tuple 不误去重，不从本地局部通过推断 required 全绿 |
| 修改 Viewer 的跨边界交互 | Viewer 短导航、准确产品/控制面设计、真实前端与协议消费者、S6 | component/协议回归与适用真实浏览器分别选择；状态/权限/世界效果按领域承接，fixture 不代签真实世界 |

以上是导航方案的静态验收，不执行所举业务修改，也不伪造 Rust 或浏览器运行结果。当前 PR 记录固定版本、失效链接的修复、三类来源走读及遗漏；需要量化阅读成本或漏测改善时，使用 [evaluating-development-methods](../../.agents/skills/evaluating-development-methods/SKILL.md)的同条件有限比较，缺少可比样本时只保留静态结论。

风险是入口再次复制正文或调用方改变后导航失效；变更主责条款、代码路径或命令时由对应维护者同步直接引用。回退只恢复受影响导航和配对说明，不回退领域安全/验证合同；单次走读不足以证明全仓导航完整或开发效率提升。

### 3.2 工程治理执行链路
- 规范定义：`doc/engineering/doc-governance/*`、`doc/engineering/rust-governance/*`、`doc/engineering/governance/*`
- 项目追踪：按需使用 Issue/Project；结果查看 PR、实际 CI 与评审记录
- 过程记录：PR、实际 CI 与评审记录（Issue 按需）；历史 `.pm/tasks/*` 记录见[固定版本归档](https://github.com/nil-studio/oasis7/blob/4d8c742a9f5eca75c02a4156678d71ccc7e87d53/.pm/github-project-sync/task-archive.jsonl)，不是当前任务入口
- 静态校验：`scripts/doc-governance-check.sh`
- ROUND 台账：Git history

专业角色卡承接 PRD 的 SC-14 和 NFR-ENG-13～18，按需维护关注点、主责文档与方法导航，根 AGENTS 使用目录入口。交接复用 Issue/PR；旧角色卡生命周期、逐角色入口映射和每日角色日志不再作为维护要求。

SC-24D 由可选的 `scripts/pr-review-threads.py` 承接线程查询与显式 resolve。工具返回实际线程和本次操作结果；合入判断仍按 workflow 核对当前 PR 和 source HEAD，不为每轮线程操作另建固定状态报告。

`scripts/doc-governance-check.sh` 核对真实目录入口、有效路径/锚点、ID、已声明关系与正向完成依据；输入和执行错误仍失败。标题、章节、列数和普遍状态卡只作写作提示。目录主责与例外在原 README 说明，不再维护 registry、完整路径 allowlist 或 corpus 分片。修改检查规则时运行适用完整回归并核对全量适用文档；普通变更只查受影响正文及反向引用，历史报告按需。

### 3.3 文档结构治理链路
- 权威规则：`doc/engineering/doc-governance/doc-structure-standard.prd.md`
- 规范正文：`doc/engineering/doc-governance/doc-structure-standard.design.md`
- 当前执行记录：Git、PR、实际 CI 与评审记录；Issue/Project 按需使用
- 历史逐文档执行面：Git history（compact snapshot entrypoint；pre-compaction 逐行证据见 git snapshot），不作为当前任务台账

### 3.4 主链路、数据流与状态流

专业规范定义规则，模块入口与专题文档承接规则，检查脚本读取受影响文件并输出检查结果。Git、PR、实际 CI 和评审记录承载修改与验证事实，按需 Issue/Project 展示跟踪状态；历史 Git snapshot 只保存当时记录，不代表当前已验证。检查结果不替代专业语义评审，适用门禁与合入条件仍由[开发流程规范](workflow/source-of-truth.md)规定。

## 4. 集成点
- `AGENTS.md`
- `.agents/roles/*.md`
- `.agents/roles/templates/handoff-brief.md`
- `doc/engineering/doc-governance/doc-structure-standard.prd.md`
- `doc/engineering/doc-governance/doc-structure-standard.design.md`
- Git history logs
- `scripts/doc-governance-check.sh`
- `testing-manual.md`

## 5. ROUND-006 入口职责
以下为历史治理分工，不恢复该轮执行义务。`engineering` 当时负责提供 ROUND-006 的规则定义与裁定标准；原治理方法要求发现规范空白时先回写专业规范。现行规则冲突与有界修复按[文档规范](doc-governance/doc-structure-standard.design.md#25-权威裁决与有界修复)和[开发流程规范](workflow/source-of-truth.md)处理。

## 设计目标
- 提供 `engineering` 模块的总体设计入口。

## 设计范围
- 覆盖模块级结构、主链路、分层与专题导航。
- 不替代专题 `*.design.md` 的细化设计。

## 导航边界
- 模块 landing 与当前分流：`doc/engineering/README.md`
- 本文：工程治理的结构设计、规则载体与执行链路。
- 专题文件级精确检索：`doc/engineering/prd.index.md`

## 设计演进计划
以下保留 ROUND-006 的历史规划来源，不表示当前排期或已完成验收；当前任务与结果以实际 PR、CI 和评审记录为准，Issue/Project 按需使用。
- 历史 M1 (2026-03-09): 在 ROUND-006 中补齐模块级 `design.md` 标准入口。
- 历史 M2（原规划，完成情况未在此核实）: 按专题继续补齐高复杂度主题的 `*.design.md`。

## 设计风险
- 若专题级设计未及时补齐，模块级 `design.md` 可能承载过多导航职责。
- 若 legacy redirect 未明确标注为兼容跳转，读者可能误判历史入口为当前执行入口。
- 若治理专题继续回流到 `engineering` 根目录，根入口会重新退化为专题堆放区。

## 私有资料工具

普通用户态资料工具与 signer 共用纯加密实现，详细边界见 [私有资料系统设计](private-context/private-context.design.md)。
