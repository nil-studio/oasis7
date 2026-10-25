# engineering PRD 文件级索引

## 入口
- 模块 PRD：`doc/engineering/prd.md`
- 模块设计总览：`doc/engineering/design.md`
- 通用开发流程：[开发流程规范](workflow/source-of-truth.md)；当前任务与结果在 PR、实际 CI 与评审记录（Issue 按需）
- 文档治理专题入口：`doc/engineering/doc-governance/README.md`（规范正文与 maintainer manual）
- Rust 工程治理专题入口：`doc/engineering/rust-governance/README.md`

## 索引边界
- 本页只负责工程专题的文件级精确检索与相关文档可达性；文档树问题从 `doc/engineering/doc-governance/README.md` 开始，Rust 文件结构、Workspace、架构边界与协议产物问题从 `doc/engineering/rust-governance/README.md` 开始。

| 专题 PRD | 专题设计文档 | 任务与证据入口 |
| --- | --- | --- |
| [Rust 工程契约需求](rust-governance/rust-engineering-contracts.prd.md)（PRD-ENGINEERING-036） | [Rust 工程契约系统设计](rust-governance/rust-engineering-contracts.design.md)；[实施验证手册](rust-governance/rust-engineering-contracts.manual.md) | 目标设计与当前实现分别判断；实施、验证与剩余范围见 PR、实际 CI 与评审记录 |
| [私有资料产品需求](private-context/private-context.prd.md)（PRD-ENGINEERING-035） | [私有资料系统设计](private-context/private-context.design.md) | PR、实际 CI 与评审记录（Issue 按需） |
| [指挥中心产品设计](command-center/command-center.prd.md)（PRD-ENGINEERING-034） | [系统设计](command-center/command-center.design.md)；[文档树设计](command-center/document-tree.design.md)；[本机工具设计](command-center/local-tools.design.md)；[持续更新流程](command-center/command-center.manual.md) | 云端四视图与个人空间、本地设备能力分别承接；[项目状态](command-center/README.md)与[架构索引](command-center/architecture/README.md)提供派生导航，交付事实仍在 PR、实际 CI 与评审记录（Issue 按需） |
| `doc/engineering/doc-governance/doc-structure-standard.prd.md` | `doc/engineering/doc-governance/doc-structure-standard.design.md`；`documentation-governance.manual.md`（how-to） | PR、实际 CI 与评审记录（Issue 按需） |
| `doc/engineering/doc-governance/product-documentation-standard.prd.md` | `doc/engineering/doc-governance/product-documentation-standard.design.md`；`product-documentation-standard.templates.md` | PR、实际 CI 与评审记录（Issue 按需） |
| 产品文档全量治理（standalone execution design） | `doc/engineering/doc-governance/product-documentation-full-corpus-governance.design.md` | PR、实际 CI 与评审记录（Issue 按需） |
| 系统设计写作规范（standalone content standard） | `doc/engineering/doc-governance/system-design-writing-standard.design.md` | PR、实际 CI 与评审记录（Issue 按需） |
| 项目管理记录规范（standalone content standard） | `doc/engineering/doc-governance/project-management-record-standard.design.md` | PR、实际 CI 与评审记录（Issue 按需） |
| 跨层要求追踪规范（standalone content standard） | [cross-layer-requirements-traceability.design.md](doc-governance/cross-layer-requirements-traceability.design.md) | PR、实际 CI 与评审记录（Issue 按需） |
| Document Corpus Inventory v3（standalone storage/admission design） | `doc/engineering/doc-governance/document-corpus-inventory-v3.design.md` | PR、实际 CI 与评审记录（Issue 按需） |
| `doc/engineering/rust-governance/rust-1200-line-root-cause-governance-2026-03-29.prd.md` | 不另建 design：单一文件扫描与结构切片合同由该专业 PRD 完整承载 | 当前 PR、扫描与定向测试结果；旧批次见 Git history |
| Historical document-governance triplets | Current organization and consumption rules: `doc/engineering/doc-governance/doc-structure-standard.design.md`; inventory and maintenance-cost routing: `doc/engineering/governance/README.md` | Historical decision/rollout evidence: Git history and GitHub task issue evidence comments |
| Historical self-evolution / memory / borrowing / skill-surface triplets | Current task/evidence rules: `doc/engineering/workflow/source-of-truth.md`; current pending work: PR and optional Issue; professional scope: `doc/engineering/prd.md`; default-vs-library skill reachability: `.agents/skills/README.md` | Historical decision and rollout evidence: Git history and PR、实际 CI 与评审记录（Issue 按需） |

## 说明
- 本索引用于保证模块专题文档在根入口文档树中可达。
- 文档配对规则：`*.prd.md`、`*.design.md` 与相关 PR 和实际验证记录。
- 上述 standalone content standard 没有另建 PRD 或本地 project ledger；它们的规范正文由 design 文件承接，任务与执行证据使用 Git、PR 与实际 CI 记录。
- `doc/engineering/doc-governance/README.md` 是 doc-governance 簇级分流入口；本索引保留相关规范与执行记录的可达性，不再要求读者从长表里判断治理问题归属。
- `doc/engineering/rust-governance/README.md` 是 Rust 工程治理簇级分流入口；本索引保留文件结构及工程契约专题的精确检索，不复制已完成专题的 design / project。
- `engineering` 根目录默认只保留 `README.md / prd.md / design.md / prd.index.md` 四个模块文件入口；PR、实际 CI 与评审记录承载交付事实，Issue 按需；治理专题已分别下沉到 `doc-governance/`、`rust-governance/` 与 `governance/`。`doc-structure-standard` 负责组织、职责和消费层边界，`governance/README.md` 负责 inventory 和维护成本 follow-up 路由；workflow source of truth 与 `.agents/skills/README.md` 分别承接 self-evolution task/evidence boundary 与 skill reachability。已完成且规则已被这些 current authorities 承接的一次性专题不再作为 live 三件套暴露。`doc/devlog` 的当前入口是 `doc/devlog/README.md` compact archive summary，不再通过 active 专题三件套暴露。

## 历史审计留痕
- 2026-04 `p2p-node-path-governance` 一次性落位三件套已退役删除；当前 `p2p/node` 的首读分流、主题簇与维护触发器由 `doc/p2p/node/README.md` 承接，历史实施证据保留在 git history 与 GitHub task evidence 中。
- 2026-04 `testing-evidence-path-governance` 一次性落位三件套已退役删除；当前 `testing/evidence` 的分流与维护边界由 `doc/testing/evidence/README.md` 承接，历史实施证据保留在 git history 与 GitHub task evidence 中。
- 2026-04 `world-simulator-viewer-path-governance` 一次性落位三件套已退役删除；当前 `world-simulator/viewer` 的首读分流、主题簇与维护触发器由 `doc/world-simulator/viewer/README.md` 承接，历史实施证据保留在 git history 与 GitHub task evidence 中。
- 2026-04 `readme-governance-path-governance` 一次性落位三件套已退役删除；当前 `readme/governance` 的首读分流、主题簇与维护触发器由 `doc/readme/governance/README.md` 承接，历史实施证据保留在 git history 与 GitHub task evidence 中。
- 2026-03 legacy 文档迁移 closure / handoff 记录已退役删除；历史迁移证据保留在 Git history logs 与 git history，当前迁移规则入口为 `doc/engineering/doc-governance/doc-structure-standard.design.md`、`doc/engineering/workflow/source-of-truth.md` 与 PR、实际 CI 与评审记录（Issue 按需）。
- 2026-02 documentation-governance-engineering closure 三件套已退役删除；历史审读证据保留在 Git history logs 与 git history，当前工程文档治理入口为 `doc/engineering/doc-governance/doc-structure-standard.design.md`、`doc/engineering/governance/README.md`、`doc/engineering/workflow/source-of-truth.md` 与 PR、实际 CI 与评审记录（Issue 按需）。
- 2026-03 engineering governance producer->QA 一次性 handoff 记录已退役删除；当前 governance trend / quarterly review 证据入口为 PR、实际 CI 与评审记录（Issue 按需）、按风险安排的实际评审记录 与 workflow source-of-truth。
- 2026-03 engineering governance trend tracking 与 quarterly governance cycle 一次性建模三件套已退役删除；当前趋势、季度复核、环境与仓库健康巡检统一从 `doc/engineering/governance/README.md` 分流，专题正文与 PR、实际 CI 与评审记录（Issue 按需） 保留各自事实和执行证据；文档树规则仍以 `doc/engineering/doc-governance/README.md`、task/PR 规则仍以 `doc/engineering/workflow/source-of-truth.md` 为准。
- 2026-03 全量 PRD 审读机制三件套已退役删除；历史审读证据保留在 Git history logs，当前新增/变更文档追踪以模块入口、`prd.index.md`、PR、实际 CI 与评审记录（Issue 按需）、`prd.md` 与现行 doc governance 规则为准。
- 2026-03 core release-candidate / next-round producer/QA/LiveOps 一次性 handoff 面与 release-candidate readiness / version / go-no-go 根目录三件套已退役删除；历史发布候选证据保留在 Git history 与 git history，当前角色证据 sink 为 PR、实际 CI 与评审记录（Issue 按需） 与 按风险安排的实际评审记录；历史 `.pm` execution log 只作为迁移前追溯层。
- 2026-03 self-evolution file-based PM project 面与历史 pointer PRD 已退役删除；`PRD-ENGINEERING-021` 历史锚点由 `file-based-self-evolution-management-2026-03-30.design.md` 的 object-model 背景承接，当前开发、验证、评审与交付 规则以 `doc/engineering/workflow/source-of-truth.md` 为准。
- 2026-03/05 dated self-evolution memory, external-workflow borrowing and skill-surface triplets，以及 2026-04 dated document-reading-surface and corpus-maintenance triplets 已退役删除；当前规则以 workflow source of truth、doc structure standard、engineering governance README、当前 PR 与按需 Issue 中的剩余义务 与 `.agents/skills/README.md` 为准。历史决策和 rollout 证据保留在 Git history 与 GitHub task issue evidence comments。
- 2026-04 `devlog-history-compaction` 一次性专题三件套已退役删除；`doc/devlog/README.md` 保留 compact archive summary，当前交付事实使用 Git、PR、实际 CI 和评审记录，Issue/Project 按需使用。
- 2026-02 oversized Rust file splitting round3 三件套已退役删除；当前 Rust 1200 行治理统一从 `doc/engineering/rust-governance/README.md` 分流，历史 round3 审读证据保留在 Git history logs 与 git history。
