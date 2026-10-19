# game 文档索引

产品层归属：`game` 是“世界规则与玩法系统”的专业域权威；区域设施的产品组合关系由该模块的 [`受治理的区域能力与扩展`](../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md) 专题承载，不形成并列产品入口。产品总导航见 `doc/product/README.md`。

审计轮次: 12

## 从这里开始
- 想先确认当前产品目标、阶段口径与完成定义：先读 `doc/game/prd.md`。
- 想看当前正在推进什么、谁在阻断、下一步做什么：查看对应 GitHub task issue 的 Project 状态与 evidence comments。
- 想直接按文件名定位某个 gameplay 专题：先读 `doc/game/prd.index.md`。
- 想先进入 gameplay 热点子域，而不是顺扫近期长名单：先读 `doc/game/gameplay/README.md`。
- 想快速理解核心玩法骨架：先在 `doc/game/prd.md` 确认活跃基线与路由，再读 `doc/game/gameplay/gameplay-top-level-design.prd.md` 的 bounded 专业合同。
- 想确认工业流水线服务窗口、lease/expiry、hard/soft 迟到与电力连续性：读 `doc/game/gameplay/gameplay-industrial-pipeline-service-window-contract.prd.md`。
- 想确认首局、前 10/30 分钟吸引力与持续游玩合同：先读产品承诺 `doc/product/world-rules-core-gameplay/first-session-and-continuation.prd.md`，再读 `doc/game/gameplay/gameplay-top-level-design.prd.md` 的 early-retention 专业合同；当前 verdict 与复跑边界由对应 PR 与实际验证记录 和 `doc/testing/evidence/` 的同候选样本确认。
- 想确认“间接控制为什么仍然应该感觉像我在控制，而不是旁观 AI”：先读 `doc/game/gameplay/gameplay-indirect-control-agency-contract.prd.md`。
- 想确认“成熟世界里小玩家/新玩家靠什么继续有独立价值，而不是只能投靠大组织”：先读 `doc/product/world-rules-core-gameplay/mature-world-progression.prd.md`，再下钻 `doc/game/gameplay/gameplay-mature-world-progression-contract.prd.md`。
- 想确认当前试玩放行与发行口径：先读 `doc/product/player-entry-distribution/access-modes-and-release-readiness.prd.md`；当前执行状态由 Git、PR 与实际 CI 记录 与同候选 evidence 确认，公开状态只看根 `README.md`。
- 想跟进最近最活跃的经济/运营规则变化：先读 `doc/game/gameplay/gameplay-agent-claim-economy-contract.prd.md`，再按需进入对应 design / project / runbook。
- 想确认制造、紧急保供和组织连续性的首版政策：从 [gameplay 入口](gameplay/README.md)进入各专题的唯一规则版本；配方、配给和处置参数不由 Runtime 或 Viewer 各自定义。

## 入口
- PRD: `doc/game/prd.md`
- 设计总览: `doc/game/design.md`
- 文件级索引: `doc/game/prd.index.md`

## 入口分工
- `README.md` 只承担 landing page 职责：帮助读者先决定去模块 PRD、文件级索引或少量仍承担专业合同的高频专题。
- `prd.md` 是 game 的活跃玩法基线与路由根入口：它拥有 PRD-ID、专题范围、默认首读路径和当前状态指针；专题详细合同只在各自声明范围内优先。
- GitHub Project task status 与 issue evidence comments 是执行状态、阻断与历史 trace 的唯一可变真值；文档只保留稳定规则、验收和可复跑证据。
- `prd.index.md` 是精确检索索引，适合已经知道专题名或需要完整文件清单时使用，不适合作为第一次进入模块时的首读入口。
- `gameplay/README.md` 是 `gameplay/` 热点子域 landing page，负责把玩法骨架、留存、agency、preview/beta gate 与 economy/claim 按问题分流。
- 高频专题文档继续承担声明范围内的专业合同：`gameplay-top-level-design` 管核心玩法骨架与 `PRD-GAME-012` early-retention 合同，`gameplay-indirect-control-agency-contract` 管间接控制下的 agency 合同，`gameplay-agent-claim-economy-contract` 管近期高频经济规则。首局与持续游玩的产品承诺统一由产品分册承载，当前执行由 Git、PR 与实际 CI 记录 与对应 evidence 承载。
- `doc/product/world-rules-core-gameplay/mature-world-progression.prd.md` 管 mature-world 小玩家的产品承诺；`gameplay-mature-world-progression-contract.prd.md` 管对应专业玩法合同。

## 活跃阅读面边界
- 当前页只保留 `what / where / next / risk` 所需入口，不再把 `gameplay/` 下近期专题长名单直接平铺在首屏。
- 默认活跃入口保留在 `doc/game/prd.md`、`doc/game/prd.index.md` 与少量仍承担专业合同职责的正式专题。
- runbook、证据、checklist、handoff 与历史执行补充材料继续保留可检索性，但默认从 `prd.index.md` 或具体专题路径按需进入。

## 模块职责
- 维护玩法目标态、核心循环与发布前可玩性口径。
- 汇总 gameplay 主题下的规则、经济、治理、协作与生产闭环专题。
- 承接体验优化、长期在线硬化与发布阻断相关设计追踪。
- 维护稳定的玩法验收、经济/agency/depot 合同和可玩性边界；当前阶段、准入、执行和对外口径由 Git、PR 与实际 CI 记录、产品 PRD、QA evidence 与根 `README.md` 的 claim envelope 共同约束。

## 热点子域导航
- `gameplay/`：先看 `gameplay/README.md`，再按簇进入玩法骨架、留存修复、preview/beta gate、claim economy、长稳治理、agency 合同、mature-world 小玩家承接与可编程区域设施。
- `gameplay/` 正式专题三件套：用于已知专题后的精确下钻。
- `gameplay/` 补充材料：runbook、evidence、checklist 与跨角色执行留痕。
- 模块根入口：`README.md`、`prd.md`、`design.md`、`prd.index.md`。

## 高密度提示
- 本页不维护容易漂移的文件数量快照；当前模块库存与热点子目录统一以 `./scripts/doc-inventory-report.sh` 为准。默认入口只负责把你送到 `gameplay/README.md`，不再尝试把 gameplay 长表直接摊平到模块首页。
- 需要完整活跃专题清单时，进入 `doc/game/prd.index.md`；需要 runbook、evidence、checklist 或跨角色执行留痕时，再按 `gameplay/` 中的补充文件精确进入。

## 共享约定
- 模块根入口、专题落位与 README/legacy redirect 的共享规则统一以 `doc/engineering/doc-governance/doc-structure-standard.design.md` 为准。
- 玩法行为或体验验收变化时，优先更新 `doc/game/prd.md` 与对应 topic PRD/design/evidence；新增 gameplay 专题或默认首读入口变化时，再同步回写 `doc/game/gameplay/README.md`、`doc/game/prd.index.md` 与本页“从这里开始”。
