# core 文档索引

产品层总入口：先从 `doc/product/README.md` 按四大产品模块理解玩家价值与端到端承诺；`core` 只承载跨模块治理、全局口径与工程阅读路由，不形成第五个产品模块。

审计轮次: 7

该标记保留历史审计来源，不代表当前版本已复审，也不要求建立新的 ROUND 台账。日常开发与交付遵循[开发流程规范](../engineering/workflow/source-of-truth.md)。

## 从这里开始
- 想先理解项目级总览、跨模块边界与当前唯一全局口径：`doc/core/prd.md`
- 想看当前任务、最新进展和下一步：进入对应 PR、实际 CI 与评审记录（Issue 按需）。
- 想按主题或文件名继续下钻，而不是从活跃专题列表逐条找：`doc/core/prd.index.md`
- 想确认跨模块优先级规则：读 `doc/core/prd.md`。
- 想先看玩家访问模式与 execution lane 的产品契约：`doc/product/player-entry-distribution/prd.md`
- 想先看“统一持久大世界”默认产品模型与术语契约：`doc/product/world-infrastructure/prd.md`
- 想追溯 ROUND 审计、2026-03 版本候选 readiness / go-no-go 或任务收口依据：按需进入 Git history

## 入口
- PRD: `doc/core/prd.md`
- 设计总览: `doc/core/design.md`
- 可变执行真值: PR、实际 CI 与评审记录（Issue 按需）
- 文件级索引: `doc/core/prd.index.md`

## 入口分工
- `README.md` 只承担 landing page 职责：告诉读者先去哪个权威入口，不重复索引长表和审计台账。
- `prd.md` 是 core 模块权威规格入口，适合先理解项目级模块地图、链路、阶段口径与跨模块规则。
- PR、实际 CI 与评审记录（Issue 按需） 是执行台账，适合确认当前 core 收口动作、最近完成项与下一步。
- `prd.index.md` 是定向检索索引，适合已经知道主题后按文件名继续下钻，不是新读者的首读入口。
- `templates/` 与 `checklists/` 属于配套材料层；已退役审计记录从 Git history 和 PR、实际 CI 与评审记录（Issue 按需） 追溯。

## 活跃阅读面边界
- 当前页只保留 `what / where / next / risk` 所需入口，不再直接平铺活跃专题长名单或 review 台账。
- 高频 active 入口保留在 `prd.md`、`prd.index.md`、PR、实际 CI 与评审记录（Issue 按需） 与少量仍在承担当前跨模块判断职责的正式专题。
- ROUND 审查记录、采证、模板与 checklist 继续保留可检索性，但默认从 `prd.index.md` 或具体专题路径进入。

## 模块职责
- 提供项目级设计总览、模块地图、关键链路与术语口径。
- 维护跨模块治理基线、候选级 readiness / go-no-go 审计留痕与文档总入口同步。
- 维护项目级设计阅读顺序、跨模块优先级规则与历史审计来源导航；当前任务排序和下一步只在 Git、PR 与实际 CI 记录中更新。

## 热点子域导航
- 历史 ROUND 审查、go/no-go、readiness board 与 audit-progress 记录已从活跃文档树退役，按 Git history 和 PR、实际 CI 与评审记录（Issue 按需） 追溯。
- 根目录只保留 core 模块入口、设计总览、主 PRD 与文件级索引，不再保留 dated cross-module 专题三件套；产品契约从 `doc/product/` 对应模块 PRD 进入，历史候选与任务证据从 PR 与实际验证记录 和 Git history 追溯。
- `templates/`：阶段收口和 PRD-ID 追踪模板。
- `checklists/`：跨模块影响检查清单。

## 高密度提示
- 本页不维护容易漂移的文件数量快照；当前模块库存与热点子目录统一以 `./scripts/doc-inventory-report.sh` 为准。默认入口不再尝试把 review / audit 材料直接摊平展示。
- 需要完整活跃专题清单时，进入 `doc/core/prd.index.md`；需要 round 审查、采证或模板时，再按子域进入。

## 共享约定
- 模块根入口、专题落位与 README/legacy redirect 的共享规则统一以 `doc/engineering/doc-governance/doc-structure-standard.design.md` 为准。
- 跨模块边界、候选级入口、优先级规则或主链路变化时，更新 `doc/core/prd.md`；当前排序、任务和下一步更新 Git、PR 与实际 CI 记录。产品承诺变化则更新 `doc/product/` 对应稳定分册。
