# readme 文档索引

审计轮次: 13

## 从这里开始
- 想先回答 README 模块在管什么、哪些内容属于正式对外口径：`doc/readme/prd.md`
- 想看当前执行任务、最新完成项与后续活跃动作：实际 PR 或已有 Task 及其验证、评审记录。
- 想按子域或文件名继续下钻，而不是从长名单里逐条找：`doc/readme/prd.index.md`
- 想先进入 `governance` 热点子域，并按治理控制 / release communication / Moltbook / limited preview 与 reward / 小红书 / 公开定位分流：`doc/readme/governance/README.md`
- 想先理解 README 与实现/流程缺口的主从关系，再按具体能力下钻：`doc/readme/gap/README.md`
- 想追溯 README production 收口、历史基线或受台账约束的 production 素材：`doc/readme/production/README.md`

## 入口
- PRD: `doc/readme/prd.md`
- 设计总览: `doc/readme/design.md`
- 执行记录: Git、PR、实际 CI 与评审记录；Issue/Project 按需。
- 文件级索引: `doc/readme/prd.index.md`

## 入口分工
- `README.md` 只承担 landing page 职责：告诉读者先去哪个权威入口，不重复长表索引内容。
- `prd.md` 是模块权威规格入口，适合先理解 README 对外口径、缺口治理与运营内容边界。
- PR、实际 CI 与评审记录（Issue 按需） 是执行台账，适合确认当前活跃专题、收口状态与最新完成项。
- `prd.index.md` 是定向检索索引，适合已经知道主题后按子域或文件名继续下钻，不是新读者的首读入口。
- `governance/README.md` 是当前最高密度热点子域 `governance/` 的 canonical 入口，适合先按治理控制、release communication、Moltbook、limited preview/reward、小红书与公开定位分流，再进入具体专题。
- `gap/README.md` 是 README gap 收口资料的本地入口；先区分总收口主文档、具体增量与历史追溯，避免把任一完成态 leaf 当成当前模块口径。
- `production/README.md` 是 production 证据与素材的本地入口，区分当前可检索专题、历史压缩三件套和仍受模块台账约束的 dated 素材；它不替代模块 PRD 或项目台账。
- liveops runbook / material / execution 文档按需进入，不再在模块 README 首屏平铺成长名单。

## 活跃阅读面边界
- 当前页只保留 `what / where / next / risk` 所需入口，不再直接罗列 canonical、material 与近期专题的长名单。
- 高频 active 入口保留在 `prd.md`、PR、实际 CI 与评审记录（Issue 按需）、`prd.index.md` 与当前仍在执行的 runbook 文档。
- 审计留痕、历史背景、素材包与执行记录继续保留可检索性，但默认从 `prd.index.md` 或具体专题路径进入。

## 模块职责
- 统一仓库对外说明口径与文档入口。
- 跟踪 README 与设计/实现的一致性缺口。
- 承接 release communication、公告底稿、运营 runbook 与根 README 状态同步等对外口径闭环。

## 热点子域导航
- `governance/`：release communication、渠道运营 runbook、奖励与 invite 包；当前已补 `governance/README.md` 作为热点子域入口，已删除旧根 README 对齐和顶层链接检查一次性专题三件套。
- `gap/`：README 与实现/流程间差距闭环；先从 `gap/README.md` 区分主收口、增量和历史追溯。
- `production/`：生产收口、阶段边界、readiness 主题及受台账约束的 production 素材；先从 `production/README.md` 分流。

## 高密度提示
- 本页不维护容易漂移的文件数量快照；当前模块库存与热点子目录统一以 `./scripts/doc-inventory-report.sh` 为准。这一层入口不再尝试把 canonical、material 与 execution 专题全部摊平展示。
- 需要完整活跃专题清单时，进入 `doc/readme/prd.index.md`；进入 `governance/`、`gap/` 或 `production/` 时，优先读各自的 `README.md` 再继续下钻；需要素材包、执行记录或历史专题时，再按具体子域进入。

## 根目录收口
- 模块根目录主入口保留：`README.md`、`prd.md`、`design.md`、PR、实际 CI 与评审记录（Issue 按需）、`prd.index.md`。
- 其余专题文档按主题下沉到 `gap/`、`production/`、`governance/`。

## 共享约定
- 模块根入口、专题落位与 README/legacy redirect 的共享规则统一以 `doc/engineering/doc-governance/doc-structure-standard.design.md` 为准。
- 对外口径、运营路径或 README 状态同步规则变化时，优先更新受影响的 `doc/readme/prd.md` / `doc/readme/design.md` 条款；高频入口变化时，再同步回写 `doc/readme/prd.index.md` 与相关子域入口（例如 `doc/readme/governance/README.md`、`doc/readme/production/README.md`）。
