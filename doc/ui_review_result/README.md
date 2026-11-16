# ui_review_result 目录说明

审计轮次: 9

## 入口
- 当前样本池状态：本 README
- 评分模板来源：`doc/world-simulator/viewer/viewer-visual-design-spec-2026-06-05.design.md`

## 定位结论
- 本目录是 `viewer_engineer` 维护的短周期评审样本池入口，不是正式模块，也不承担长期知识库职责。
- 保留在根级例外目录的原因：短期 UI/视觉打分卡需要一个轻量索引与 `output/visual_review/*` 一一对应；当没有活跃样本时，只保留本说明与空索引。
- 正式体验结论、可复用规则与长期口径应回写到 `doc/world-simulator/**`、`doc/playability_test_result/**` 或对应 PRD / GitHub task truth，而不是长期堆积在本目录。

## 目录职责
- 沉淀 UI / 视觉评审结果，结构对齐 `doc/playability_test_result` 的卡片化留痕方式。
- 为 `world-simulator` 的界面体验评审提供可追溯卡片入口。
- 仅保留当前活跃轮次样本，不承担长期归档职责。

## 当前内容
- 当前无活跃评审卡片；旧 `UI-20260306-115029` 样本卡与空列表已退役删除，长期视觉规则以 Viewer 视觉规范为准。
- 空池状态不构成当前 UI 质量通过、失败或未评审结论；它只表示此短周期例外目录当前没有待迭代样本。
- 新增活跃样本时，直接在本目录新增评审卡片并更新本 README 的当前样本池状态；不再长期保留空列表文件。

## 维护约定
- 新增 UI 评审卡后，需同步更新本 README 的当前样本池状态与对应卡片入口。
- 正式评审口径以 `doc/world-simulator/viewer/viewer-visual-design-spec-2026-06-05.design.md` 为准；历史打分卡只作为旧样本来源，不再作为当前权威。
- 历史卡片不在本目录长期归档；如需长期沉淀，应由所属模块专题文档或 GitHub task issue evidence comments 承接。
- 进入条件：当前轮次需要保留可评分的 UI/视觉样本卡。
- 退出条件：当样本对应的体验结论已回写正式模块文档，且无继续迭代需求时，应清空或替换为新的活跃样本，不在此处形成伪模块历史库。
- 例外复核：每次新增样本卡，以及样本池连续两个审计轮次为空时，由 `viewer_engineer` 按本 README 的用途与退出条件判断是否仍需保留，并同步实际入口和根导航。
