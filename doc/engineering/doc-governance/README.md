# engineering/doc-governance 专题入口

本目录是 `doc/` 文档树治理的唯一上游分流入口，收拢文档组织、路径落位、入口减重与存量维护成本治理。读者应先从这里判断问题属于哪一类，再进入当前 authority；不要把同一治理规则继续复制到模块 README 或零散专题里。

## 首读路径
- 专业系统设计的十二段骨架、需求承接、验证映射与证据边界：`doc/engineering/doc-governance/system-design-writing-standard.design.md`
- `doc/**` 逐对象覆盖、semantic/evidence 分片与 trusted admission：[`document-corpus-inventory-v3.design.md`](document-corpus-inventory-v3.design.md)；存储和受信准入的技术切换边界见该设计；普通开发与合入规则以 workflow source of truth 为准
- GitHub-backed 项目执行记录、任务范围与组合验收：`doc/engineering/doc-governance/project-management-record-standard.design.md`
- 产品/专业要求、系统设计与项目执行记录的全局 ID 追踪：[`cross-layer-requirements-traceability.design.md`](cross-layer-requirements-traceability.design.md) 与 [`workflow source of truth`](../workflow/source-of-truth.md)
- 文档组织规则、后缀职责、模块 README 边界：`doc/engineering/doc-governance/doc-structure-standard.design.md`
- 产品文档内容要求、四模块边界与三类采纳试点：`doc/engineering/doc-governance/product-documentation-standard.design.md`
- 产品文档存量全量治理、外部语义迁移与 full-corpus 验证：`doc/engineering/doc-governance/product-documentation-full-corpus-governance.design.md`
- 产品文档采纳目标、Done 与范围：`doc/engineering/doc-governance/product-documentation-standard.prd.md`
- 根/专题/design 模板与迁移审查卡：`doc/engineering/doc-governance/product-documentation-standard.templates.md`
- 文档 intake、迁移、registry、例外与证据生命周期的维护步骤：`doc/engineering/doc-governance/documentation-governance.manual.md`
- 默认阅读面、活跃真值/审计留痕/历史归档/兼容跳转的消费边界：`doc/engineering/doc-governance/doc-structure-standard.design.md`
- 文档存量、热点目录、近限文件与维护成本判断：`doc/engineering/governance/README.md`，并用 `scripts/doc-inventory-report.sh` 复算
- 已收口的 `world-simulator/viewer` 与 `readme/governance` 热点路径：直接进入各自 landing page；一次性路径治理三件套已退役，历史从 git 与 PR 与实际验证记录 追溯
- 全量专题三件套索引：`doc/engineering/prd.index.md`

## 按治理问题分流
| 问题 | Canonical 入口 | 说明 |
| --- | --- | --- |
| 文档、Issue、PR、评审和交接文字如何表达 | [语言与写作要求](../workflow/source-of-truth.md#language-and-writing) | 统一表达原则与增量评审口径，正文在开发流程规范维护 |
| 新文档应该放在哪里、承担什么职责 | `doc-structure-standard.design.md` | 顶层组织规范；定义模块、专题、分册、README、PRD/design/manual/runbook 边界，交付事实使用 Git/PR/实际 CI 与评审记录，Issue/Project 按需 |
| 产品文档应该写什么、如何验收和追踪 | `product-documentation-standard.design.md` | 在现有四模块与专业 authority 上补充内容合同、REQ/AC 追踪、每功能点设计/实现状态摘要、证据边界和三类试点 |
| 产品文档如何套用结构、迁移代表性主题 | `product-documentation-standard.templates.md` | 提供根/专题/design 模板、需求/验收卡和 Prompt/首局/SC-31 迁移模式 |
| 用同一全局 ID 查找要求、设计和项目执行记录 | `cross-layer-requirements-traceability.design.md` | 全长 UUID 的一次声明、多层引用与生命周期；已有只读查询/有界索引，结果仍需按来源范围人工核对 |
| 已采纳规范如何覆盖全部存量产品文档 | `product-documentation-full-corpus-governance.design.md` | 定义 active/retired 内容闭合、外部语义迁移、配对 design 判定与全量检查 |
| 专业系统设计如何承接需求、表达合同并映射验证 | `system-design-writing-standard.design.md` | 十二段技术骨架；不替代产品设计、实现、运行或发布证据 |
| 任务范围、实际版本和组合验收如何留证 | `project-management-record-standard.design.md` | 复用 PR 与实际验证记录，Issue/Project 按需；不新增本地台账或状态机 |
| 需要执行迁移、登记一级目录/例外或处理治理检查失败 | `documentation-governance.manual.md` | maintainer how-to；只执行 Design 已定义的规则，不另行裁定规则 |
| 需要按模块或问题核对文档语义、证据边界和修复范围 | [有界人工语义审读](documentation-governance.manual.md#bounded-semantic-review) | 复用既有职责与 authority，区分实际阅读覆盖、文档处置及业务验收；不新增统一模板或 CI 语义门禁 |
| 根入口或模块 README 过长、重复维护共享规则 | `doc-structure-standard.design.md` | 处理默认阅读面噪音，避免 landing page 变成第二份规范正文 |
| 文档总量、热点子目录、devlog backlog 或近限长文件抬高维护成本 | `../governance/README.md` | 处理入口减重之后的存量维护成本，配合 `scripts/doc-inventory-report.sh` 复算 |
| 需要确认全量 `doc/**` 覆盖、对象内容漂移、semantic 复核绑定或 testing evidence 范围 | [`document-corpus-inventory-v3.design.md`](document-corpus-inventory-v3.design.md)；`python3 scripts/document-corpus-inventory-check.py --repo-root .`；`python3 scripts/doc-evidence-inventory-check.py --repo-root .` | 入口为静态 schema，逐对象与复核记录按源分片；全量检查不代表语义批准，迁移仍由对应 owner 裁决 |
| `world-simulator` / Viewer 首读分流或主题簇维护 | `../../world-simulator/viewer/README.md` | 已收口的热点路径；当前 landing page 承接分流与维护触发器 |
| `p2p/node` 首读分流或主题簇维护 | `../../p2p/node/README.md` | 当前 node 子域的 canonical landing page；完整文件检索回到 `../../p2p/prd.index.md` |
| testing evidence 与手册/门禁文档混叠 | `../../testing/evidence/README.md` | 现行 evidence 子域入口；与 testing 模块入口、文件级索引和 operator 手册分层 |
| readme 外部口径、渠道 runbook 与入口职责混叠 | `../../readme/governance/README.md` | 已收口的热点路径；当前 landing page 承接分流与维护触发器 |

PR 新增或实质修改的产品/系统设计文档规范 finding 阻断 CI，未触达历史只警告；范围/执行错误和既有拓扑、库存、安全检查仍阻断。具体范围见[内容规范检查的增量边界](product-documentation-standard.design.md#advisory-content-checks)。

## REQ/AC 追踪边界

Trace refs: O7-REQ-276983b3-3187-405e-8453-e2785f01c5e6；本节承接[全局要求追踪](cross-layer-requirements-traceability.design.md#global-requirement-trace)的适用写作/引用约定。

- 活跃专题 PRD 的行级 REQ/AC 追踪中，凡是用于满足本专题声明完整性的 fragment，都必须解析到同一专题的声明；跨专题或仅能在其他文件解析的 fragment 不能替代这项完整性关系。
- 配对 design 仍可按其配对合同使用带路径和 fragment 的跨文件链接承接 PRD 的 REQ/AC；专业 authority、跨文件消费条款、迁移接收关系和证据入口也继续使用真实的 repository-relative path#fragment。它们表达承接或权威关系，不因此满足活跃专题 PRD 的本地 REQ/AC 声明完整性。
- 具体规则以 [`workflow source of truth`](../workflow/source-of-truth.md) 和对应产品文档内容规范为准；本入口只保留分流与边界说明。

## 维护规则
- 本页只做专题导航和抽象分流，不承载完整治理规则正文。
- 就文档树共享治理规则而言，上游 `doc/README.md` 与 `doc/engineering/README.md` 只链接本页；其他模块或专题导航按各自职责保留。具体规则与专题正文必须从本页继续下钻；可变执行状态和证据归入 Git、PR、实际 CI 与评审记录，Issue/Project 按需使用，避免上游入口各自固定某个专题文件。
- 新增 doc-governance 专题时，同批更新 `doc/engineering/prd.index.md`，并在本页补一行“按治理问题分流”。
- corpus inventory v3 的 storage/admission 细则见配套 design；本页和维护手册只保留导航与操作步骤，通用权限和合入规则以 workflow source of truth 为准，不把历史 activation 包装恢复为普通任务前置。
- 一次性路径治理完成且目标 landing page 已承接首读分流与维护触发器后，退役该专题三件套，并在 `doc/engineering/prd.index.md` 的历史审计留痕记录替代入口；不要保留已经过期的缺口描述或 follow-up 排期。
- 历史证据、旧审读轮次和任务过程仍保留在 PR、实际 CI 与评审记录（Issue 按需）、Git history 与 git history；不要为减重而批量改写历史文件。
