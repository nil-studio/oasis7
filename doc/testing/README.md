# testing 文档索引

审计轮次: 10

## 从这里开始
- 了解 testing 模块覆盖的测试层级、门禁和证据边界：`doc/testing/prd.md`
- 查找好玩性证据、`L4A/L4B/L5` 边界、角色 subagent review 或 simulated player persona 的 canonical topic：`doc/testing/governance/README.md`
- 在一个 worktree 中准备 `L4A + L4B` 验证：先经 `doc/testing/governance/README.md` 确认所需证据层，再读 [L4 工具链合同与验收](../../testing-manual.md#l4-toolchain-validation)。`./scripts/prepare-playability-l4-review.sh` 只生成 scaffold；L4B 还需实际 release/UI、Hosted 执行会话与当前尝试证据，旧 runner 的实现差距见总手册 L4B 章节。
- 执行 Web UI、Playwright、public-testnet attach 或模型视觉评审手册：先读 `doc/testing/manual/README.md`，再按问题进入对应 manual。
- 查看当前测试策略、覆盖缺口与 QA 证据边界：`doc/testing/prd.md`；可变任务状态和历史以 PR、实际 CI 与评审记录（Issue 按需）为准。
- 选择测试套件或查操作步骤：先读 `testing-manual.md`；涉及专项操作再进入 `doc/testing/manual/README.md`。
- 进入 `evidence` 热点子域，按 release gate / hosted access / public-testnet readiness evidence / legacy p2p rehearsal / governance drill / claim-audit 问题分流：`doc/testing/evidence/README.md`
- 确认云上测试/正式环境、hosted-login 服务清单与 testnet/mainnet 口径边界：`doc/engineering/governance/environment-lanes-and-inventory-2026-05-29.md`
- 按子域或文件名继续检索，避免逐行查长表：`doc/testing/prd.index.md`

## 入口
- PRD: `doc/testing/prd.md`
- 设计总览: `doc/testing/design.md`
- 任务状态与历史: PR、实际 CI 与评审记录（Issue 按需）
- 文件级索引: `doc/testing/prd.index.md`

## 入口分工
- `README.md` 只承担 landing page 职责：告诉读者先去哪个权威入口，不重复长表索引内容。
- `prd.md` 是模块权威规格入口，适合先理解 required/full 分层、证据包与跨模块测试边界。
- PR、实际 CI 与评审记录（Issue 按需）承载可变执行状态；PRD、design 与 manual 维护可复用测试合同、命令和声明边界。evidence 保存实际窗口的原始材料、版本身份、原判定及必要解释，不作为后续执行台账。按[长期文档与过程记录分工](../engineering/workflow/source-of-truth.md#information-record-placement)保留来源，不能因日期、SHA 或已关闭任务批删历史证据。
- `evidence/README.md` 是当前最高密度热点子域 `evidence/` 的 canonical 入口，适合先按“release gate / hosted access / public-testnet readiness evidence / legacy p2p rehearsal / governance drill / claim-audit / 定向验证”分流，再进入具体留痕文件。
- `testing-manual.md` 与 `manual/README.md` 是 operator 手册层：前者决定通用测试路径，后者把 Web UI、Playwright、public-testnet attach 与模型视觉评审分流到对应步骤。
- `prd.index.md` 是定向检索索引，适合已知主题后按文件名查找，不是新读者的首读入口。

## 活跃阅读面边界
- 当前页只保留 `what / where / next / risk` 所需入口，不再直接罗列近期专题长名单。
- 高频 active 入口保留在 `prd.md`、PR、实际 CI 与评审记录（Issue 按需）、`testing-manual.md`、`manual/*.manual.md`、`evidence/README.md` 与 `prd.index.md`。
- evidence、templates 与历史 blocker/closure 留痕继续保留可检索性，但默认从 `prd.index.md` 或具体专题路径进入。

## 模块职责
- 维护系统测试手册、required/full 分层门禁、模型视觉评审 SOP 与发布证据包口径。
- 汇总 CI、启动器、长稳、性能、人工手册与治理专题。
- 承接跨模块测试范围定义、证据归档与趋势基线建设。

## 热点子域导航
- `evidence/`：发布证据、趋势基线与审计留痕；当前已补 `evidence/README.md` 作为热点子域入口。
- `ci/`：CI、wasm determinism、tiering 与 gate 保护。
- `longrun/`：长稳、chaos、soak 与在线稳定性。
- [`launcher/`](launcher/README.md)：启动器人工验证清单与 bundle-first playtest 入口。
- `governance/`：质量趋势、release-gate 指标、审计检查与 playability 证据治理；先读 `doc/testing/governance/README.md` 再按问题下钻。
- `templates/`：证据包、报告、模型视觉评审卡与检查清单模板；默认按需进入。
- `performance/`：runtime / viewer 性能观测与方法学。
- `manual/`：系统测试手册分册、Web UI 闭环、Playwright 实跑、public-testnet attach 与模型视觉评审；先读 `doc/testing/manual/README.md`。
- `chaos-plans/`：专项 chaos plan 入口。

## 高密度提示
- 本页不维护容易漂移的文件数量或状态快照；当前模块库存、热点子目录与 `action_required` 等治理状态统一以 `./scripts/doc-inventory-report.sh` 为准。`find` / `rg --files` 只可作为本地探索辅助，不能替代正式报告状态。
- `evidence/` 已有 `doc/testing/evidence/README.md` 作为热点子域入口。
- 子域数量只用于选择下一轮 focused follow-up；不要在本页扩展全量专题长表。
- 需要完整活跃专题清单时，进入 `doc/testing/prd.index.md`；进入 `evidence/` 时，优先读 `doc/testing/evidence/README.md` 再继续下钻；需要 template / blocker 留痕时，再按具体子域进入。

## 共享约定
- 模块根入口、专题落位与 README/legacy redirect 的共享规则统一以 `doc/engineering/doc-governance/doc-structure-standard.design.md` 为准。
- 测试门禁、required/full 分层口径或证据模板变化时，优先更新 `doc/testing/prd.md`、`testing-manual.md` 与对应 evidence 文档；高频入口变化时，再同步回写 `doc/testing/prd.index.md` 与相关热点子域入口（例如 `doc/testing/evidence/README.md`）。
