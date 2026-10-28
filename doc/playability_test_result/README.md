# playability_test_result 文档索引

审计轮次: 10

## 入口
- PRD: `doc/playability_test_result/prd.md`
- 设计总览: `doc/playability_test_result/design.md`
- 可变任务状态与历史: GitHub task issue evidence comments
- 文件级索引: `doc/playability_test_result/prd.index.md`

## 从这里开始
- 确认可玩性证据的字段、评分口径与发布引用边界：读 `doc/playability_test_result/prd.md`。
- 查看当前活跃任务或最近收口：读 GitHub task issue evidence comments。
- 复用单次结果卡或人工执行说明：先读 `doc/playability_test_result/playability_test_card.md`；专题回归执行从 `doc/playability_test_result/topics/` 进入。
- 查找专题回归卡组而非单次样本：进入 `doc/playability_test_result/topics/`，当前高频入口是 `industrial-onboarding-required-tier-cards-2026-03-15.md`。
- 追溯正式发布证据或跨模块引用样例：进入 `doc/playability_test_result/evidence/`。

## 模块职责
- 维护可玩性反馈卡、评分口径、高优问题闭环与发布证据包格式。
- 承接 game / testing / core 之间的体验证据互链。
- 统一最近活跃轮次的卡片与正式模板入口。

## 关键文档
- `doc/playability_test_result/game-test.prd.md`
- `doc/playability_test_result/playability_test_card.md`
- `doc/playability_test_result/topics/industrial-onboarding-required-tier-cards-2026-03-15.md`
- `doc/playability_test_result/templates/`
- `doc/playability_test_result/evidence/`
- `doc/playability_test_result/topics/`

## 根目录收口
- 模块根目录主入口保留：`README.md`、`prd.md`、`design.md`、`prd.index.md`；`playability_test_card.md` 是当前模板入口。
- 根目录中仍被 release/evidence 文档直接引用的 `card_*.md` 仅作为 evidence-linked legacy samples 保留，不作为当前入口；未被当前文档引用的单次样本卡不再保留在仓库。
- 新增单次结果卡不得进入模块根目录；需要保留的专题回归卡组与专题执行资产放入子目录（如 `topics/` 或 `evidence/`）。
- 可变模块状态、任务交接与下一任务仅记录在 GitHub task issue evidence comments。

## 历史证据边界
- 所有带日期的单次卡片与发布证据包只证明其记录窗口内的观察，不构成当前可玩性、发布或继续游玩门禁结论。
- 当前保留的根目录 `card_*.md` 均因仍被正式 evidence 文档引用而作为 historical evidence-linked samples 保留；其旧版 `pass`、`通过`、`继续可玩`、`需观察` 或阻断标签不得聚合为当前结论。
- 这些旧卡早于当前 player-leverage 必填字段，不能替代新鲜的 L4A/L4B 执行。当前主张边界以根级 `testing-manual.md` 和 `doc/product/world-rules-core-gameplay/playability-evidence-and-claim-boundaries.prd.md` 为准。
- 经语义复核的逐文件处置记录存于 v3 corpus semantic 分片；使用 `python3 scripts/document-corpus-inventory.py locate --kind semantic --path <path>` 定位记录。该分类是耐久治理记录，不记录任务状态或当前测试结果。

## 维护约定
- 可玩性模板、评分口径、专题卡组或发布引用格式变化时，需同步更新 `prd.md`、相关 evidence 文档与 GitHub task evidence。
- 新增专题后，需同步回写 `doc/playability_test_result/prd.index.md` 与本目录索引。
- README 优先帮助读者使用和追溯证据，不替代 `evidence/`、`topics/` 或 `prd.index.md` 的详细清单。
