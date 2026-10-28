# core PRD 文件级索引

审计轮次: 7

更新时间：2026-06-23

## 入口
- 模块 PRD：`doc/core/prd.md`
- 模块设计总览：`doc/core/design.md`
- 可变执行真值：PR、实际 CI 与评审记录（Issue 按需）

## 首读分流
- 想先回答 core 模块在管什么、哪些口径是全局唯一真值：先读 `doc/core/prd.md`
- 想先回答当前在推进什么、最近完成了什么、下一步是什么：进入对应 PR、实际 CI 与评审记录（Issue 按需）。
- 想确认跨模块优先级规则：读 `doc/core/prd.md`。
- 想直接进入玩家访问模式 / execution lane 的产品契约：先读 `doc/product/player-entry-distribution/prd.md`
- 想直接进入统一持久大世界默认产品模型与术语契约：先读 `doc/product/world-infrastructure/prd.md`
- 想追溯 ROUND 审计、2026-03 版本候选 readiness / go-no-go 或任务收口依据：按需读 Git history
- 想继续按子域或文件名下钻：使用下方热点子域导航，再跳到对应清单区域

## 密度与库存

- 不在索引中冻结容易漂移的文件数量；当前库存统一以 `./scripts/doc-inventory-report.sh` 为准。
- core 根目录不保留 dated 专题三件套；历史评审、模板与 checklist 按各自子目录入口检索。

## 热点子域导航
| 子域 | 文件数 | 适合回答的问题 |
| --- | --- | --- |
| core 根入口 | 动态 | 项目级总览、设计与索引；可变执行台账在 PR、实际 CI 与评审记录（Issue 按需），产品契约从 `doc/product/` 对应模块文档树进入 |
| `templates/` | 2 | 阶段收口与 PRD-ID 追踪模板 |
| `checklists/` | 1 | 跨模块影响检查清单 |

## 活跃补充文档

- 玩家访问模式与统一持久大世界术语的产品契约已收口到 `doc/product/` 对应模块 PRD；其专业域实现与验证权威仍在对应模块。
- 2026-03 版本候选 readiness / go-no-go 已降为审计留痕，按需从 Git history 分流进入。

## 默认阅读面边界
- 本页首屏只负责分流，不再要求读者先顺扫全部活跃专题和 review 文件。
- 模板与 checklist 继续保留可检索性；历史 review 与一次性 handoff 文档已退役删除，从 Git history 和 PR、实际 CI 与评审记录（Issue 按需） 追溯。
- 完整专题清单继续保留在下方，用于精确文件名检索和互链可达性。

## 覆盖规则
- 纳入规则：纳入 core 的活跃专题 PRD 与实际承接其义务的专业设计（包括共享设计）；不以同名或一对一配对作为收录条件；已迁入产品层的主题从 `doc/product/README.md` 进入。
- 活跃补充：仍被当前模块 PRD / 项目态直接引用的 cross-module 入口，可在“活跃补充文档”区定向列出，但不并入 review / template / checklist 清单。
- 排除规则：不纳入 Git history、`doc/core/templates/**`、`doc/core/checklists/**` 与 `doc/devlog/**` 的非三件套材料。
- 按需进入：ROUND 审查、go/no-go 留痕、采证板、模板与 checklist 继续保留可检索性；除非重新成为当前 operator 或 owner 的直接入口，否则不进入默认首屏。

## 完整活跃专题清单

core 根目录当前无独立活跃专题三件套。跨模块治理规则进入 `doc/core/prd.md`，当前任务进入 PR、实际 CI 与评审记录（Issue 按需），产品承诺进入 `doc/product/` 稳定文档树。

## 审计 / 模板 / 清单补充入口
| 文档路径 | 类型 | 用途 |
| --- | --- | --- |
| Git history | `audit_router` | ROUND 审计、任务收口、版本候选 readiness/go-no-go 与支持证据的本地分流；替代根索引平铺多个历史叶子入口 |
| `doc/core/templates/stage-closure-go-no-go-template.md` | `template` | 阶段收口 go/no-go 模板 |
| `doc/core/templates/prd-id-test-evidence-mapping.md` | `template` | PRD-ID 到测试证据映射模板 |
| `doc/core/checklists/cross-module-impact-checklist.md` | `checklist` | 跨模块影响检查清单 |

## 说明
- 本索引用于保证模块专题文档在根入口文档树中可达。
- 文档关系按[专业设计关系规范](../engineering/doc-governance/system-design-writing-standard.design.md#product-system-design-relations)链接真实承接的要求与设计条款，允许多对多；产品体验层配对规则仍只约束其适用范围。可变任务状态和执行历史只在 PR、实际 CI 与评审记录（Issue 按需）。
- 玩家访问模式与统一持久大世界术语已收口到产品层对应模块 PRD；已完成的一次性 docs hub 同步专题和 2026-03 release-candidate 根目录三件套不再作为活跃专题入口。
- 2026-03 core 根目录 release-candidate 三件套与一次性 producer/QA/LiveOps handoff 文件已全量退役删除；当前追溯以 Git history、PR、实际 CI 与评审记录（Issue 按需） 与 pre-PR local role review evidence 为准，其中 TASK-CORE-005 ROUND 收口从 Git history 进入。
- 默认入口面先在 `README.md` / `prd.index.md` 收紧；只有当入口仍无法分流时，才进入后续路径级治理。
